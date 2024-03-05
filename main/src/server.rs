use crate::multicast::MulticastReceiver;
use crate::multicast::TokioMulticastReceiver;
use futures_util::TryFutureExt;
use futures_util::TryStreamExt;
use mockall::automock;
use prost::Message;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use tansa_protocol::response_collector_service_client::ResponseCollectorServiceClient;
use tansa_protocol::Request;
use tansa_protocol::Response;
use tonic::IntoRequest;
use uuid::Uuid;

pub async fn serve(
    multicast_address: SocketAddrV6,
    multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
    service_port: u16,
) -> std::io::Result<()> {
    serve_internal(
        multicast_address,
        multicast_network_interface_indexes,
        service_port,
        TokioMulticastReceiver::new(64, multicast_address.port())?,
        GrpcResponseSender,
        UuidGenerator,
    )
    .await
}

async fn serve_internal(
    multicast_address: SocketAddrV6,
    multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
    service_port: u16,
    multicast_receiver: impl MulticastReceiver,
    response_sender: impl ResponseSender,
    response_id_generator: impl ResponseIdGenerator,
) -> std::io::Result<()> {
    multicast_network_interface_indexes
        .into_iter()
        .try_for_each(|i| multicast_receiver.join_multicast(*multicast_address.ip(), i))?;
    let receive = |_| async { Some(((multicast_receiver.receive().await), ())) };
    let handle = |(packet, remote_address): (_, SocketAddrV6)| {
        handle_packet(
            packet,
            *remote_address.ip(),
            service_port,
            &response_sender,
            &response_id_generator,
        )
        .inspect_err(|e| log::error!("Failed to handle a packet: {}", e))
        .or_else(|_| async { Ok(()) })
    };
    futures_util::stream::unfold((), receive)
        .try_for_each_concurrent(0, handle)
        .await
}

async fn handle_packet(
    packet: Vec<u8>,
    remote_ip: Ipv6Addr,
    local_service_port: u16,
    response_sender: &impl ResponseSender,
    response_id_generator: &impl ResponseIdGenerator,
) -> anyhow::Result<()> {
    let request = Request::decode(packet.as_slice())?;
    let response_collector_address =
        format!("http://[{}]:{}", remote_ip, request.response_collector_port);
    log::info!(
        "Connecting to response collector at {}",
        &response_collector_address
    );
    let response = Response {
        request_id: request.request_id,
        response_id: response_id_generator.generate(),
        port: local_service_port.into(),
    };
    response_sender
        .send(response, response_collector_address)
        .await
}

#[automock]
trait ResponseIdGenerator {
    fn generate(&self) -> Vec<u8>;
}

struct UuidGenerator;

impl ResponseIdGenerator for UuidGenerator {
    fn generate(&self) -> Vec<u8> {
        Uuid::new_v4().into_bytes().into()
    }
}

#[automock]
trait ResponseSender {
    async fn send(
        &self,
        response: Response,
        response_collector_address: String,
    ) -> anyhow::Result<()>;
}

struct GrpcResponseSender;

impl ResponseSender for GrpcResponseSender {
    async fn send(
        &self,
        response: Response,
        response_collector_address: String,
    ) -> anyhow::Result<()> {
        ResponseCollectorServiceClient::connect(response_collector_address)
            .await?
            .submit_response(response.into_request())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::multicast::MockMulticastReceiver;
    use mockall::predicate::eq;
    use std::io::ErrorKind;

    #[tokio::test]
    async fn serve() {
        let multicast_address: SocketAddrV6 = "[::A]:1".parse().unwrap();
        let request = Request {
            request_id: "REQUEST".into(),
            response_collector_port: 3,
        };
        let request_address = "[::123]:2".parse().unwrap();

        let expected_response = Response {
            request_id: request.request_id.clone(),
            response_id: "RESPONSE".into(),
            port: 10,
        };

        let mut multicast_receiver = MockMulticastReceiver::default();
        let mut requests = [
            Ok((request.encode_to_vec(), request_address)),
            Err(ErrorKind::Other.into()),
        ]
        .into_iter();
        multicast_receiver
            .expect_join_multicast()
            .with(eq(*multicast_address.ip()), eq(1))
            .return_once_st(|_, _| Ok(()));
        multicast_receiver
            .expect_join_multicast()
            .with(eq(*multicast_address.ip()), eq(2))
            .return_once_st(|_, _| Ok(()));
        multicast_receiver
            .expect_receive()
            .returning(move || requests.next().unwrap());

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .with(
                eq(expected_response.clone()),
                eq::<String>("http://[::123]:3".into()),
            )
            .return_once(|_, _| Ok(()));

        let mut response_id_generator = MockResponseIdGenerator::default();
        response_id_generator
            .expect_generate()
            .return_const::<Vec<_>>(expected_response.response_id.clone());

        // when
        let result = serve_internal(
            multicast_address,
            [1, 2],
            expected_response.port.try_into().unwrap(),
            multicast_receiver,
            response_sender,
            response_id_generator,
        )
        .await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn failing_to_handle_packet_does_not_stop_serving() {
        let request_source_address = "[::123]:2".parse().unwrap();
        let mut requests = [
            Ok((vec![], request_source_address)),
            Err(ErrorKind::Other.into()),
        ]
        .into_iter();

        let mut multicast_receiver = MockMulticastReceiver::default();
        multicast_receiver
            .expect_join_multicast()
            .return_once_st(|_, _| Ok(()));
        multicast_receiver
            .expect_receive()
            .returning(move || requests.next().unwrap());

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .return_once(|_, _| anyhow::bail!("Failed to send response"));

        // when
        let result = serve_internal(
            "[::A]:1".parse().unwrap(),
            [1],
            1,
            multicast_receiver,
            response_sender,
            UuidGenerator,
        )
        .await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn handle_many_requests() {
        let request_size = 128;
        let request_source_address = "[::123]:2".parse().unwrap();
        let mut requests = std::iter::repeat_with(move || Ok((vec![], request_source_address)))
            .take(request_size)
            .chain(std::iter::once(Err(ErrorKind::Other.into())));

        let mut multicast_receiver = MockMulticastReceiver::default();
        multicast_receiver
            .expect_join_multicast()
            .return_once_st(|_, _| Ok(()));
        multicast_receiver
            .expect_receive()
            .returning(move || requests.next().unwrap());

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .times(request_size)
            .returning(|_, _| Ok(()));

        // when
        let result = serve_internal(
            "[::A]:1".parse().unwrap(),
            [1],
            1,
            multicast_receiver,
            response_sender,
            UuidGenerator,
        )
        .await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[test]
    fn response_id_has_reasonable_size() {
        let response_id = UuidGenerator.generate();
        assert!(response_id.len() <= 16);
    }

    fn assert_server_exits_with_dummy_error(result: std::io::Result<()>) {
        assert_eq!(
            ErrorKind::Other,
            result.unwrap_err().kind(),
            "Server must have exited with the dummy error supplied at the end of all requests"
        );
    }
}
