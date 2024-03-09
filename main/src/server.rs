use crate::multicast::MulticastReceiver;
use crate::multicast::TokioMulticastReceiver;
use crate::response_sender::GrpcResponseSender;
use crate::response_sender::ResponseSender;
use futures_util::TryFutureExt;
use futures_util::TryStreamExt;
use prost::Message;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use tansa_protocol::Request;
use tansa_protocol::Response;

pub async fn serve(
    multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
    service_name: &str,
    service_port: u16,
) -> std::io::Result<()> {
    serve_internal(
        multicast_network_interface_indexes,
        service_name,
        service_port,
        TokioMulticastReceiver::new(64, crate::get_multicast_address().port())?,
        GrpcResponseSender,
    )
    .await
}

async fn serve_internal(
    multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
    service_name: &str,
    service_port: u16,
    multicast_receiver: impl MulticastReceiver,
    response_sender: impl ResponseSender,
) -> std::io::Result<()> {
    let multicast_ip = *crate::get_multicast_address().ip();
    multicast_network_interface_indexes
        .into_iter()
        .try_for_each(|i| multicast_receiver.join_multicast(multicast_ip, i))?;
    let receive = |_| async { Some(((multicast_receiver.receive().await), ())) };
    let handle = |(packet, remote_address): (_, SocketAddrV6)| {
        handle_packet(
            packet,
            *remote_address.ip(),
            service_name,
            service_port,
            &response_sender,
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
    service_name: &str,
    service_port: u16,
    response_sender: &impl ResponseSender,
) -> anyhow::Result<()> {
    let request = Request::decode(packet.as_slice())?;
    log::debug!("Received {:?} via multicast from {}", request, remote_ip);
    if service_name != request.service_name {
        log::debug!(
            "Dropping a request for an unknown service: {}",
            request.service_name
        );
        return Ok(());
    }
    let response_collector_address =
        format!("http://[{}]:{}", remote_ip, request.response_collector_port);
    let response = Response {
        service_port: service_port.into(),
    };
    log::debug!(
        "Sending {:?} to `ResponseCollector` at {}",
        response,
        response_collector_address
    );
    response_sender
        .send(response, response_collector_address)
        .await
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::multicast::MockMulticastReceiver;
    use crate::response_sender::MockResponseSender;
    use mockall::predicate::eq;
    use std::io::ErrorKind;

    #[tokio::test]
    async fn serve() {
        let multicast_ip = *crate::get_multicast_address().ip();
        let request = Request {
            service_name: "SERVICE".into(),
            response_collector_port: 3,
        };
        let request_address = "[::123]:2".parse().unwrap();

        let expected_response = Response { service_port: 10 };

        let mut multicast_receiver = MockMulticastReceiver::default();
        let mut requests = [
            Ok((request.encode_to_vec(), request_address)),
            Err(ErrorKind::Other.into()),
        ]
        .into_iter();
        multicast_receiver
            .expect_join_multicast()
            .with(eq(multicast_ip), eq(1))
            .return_once_st(|_, _| Ok(()));
        multicast_receiver
            .expect_join_multicast()
            .with(eq(multicast_ip), eq(2))
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

        // when
        let result = serve_internal(
            [1, 2],
            &request.service_name,
            expected_response.service_port.try_into().unwrap(),
            multicast_receiver,
            response_sender,
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
        let result = serve_internal([1], "", 1, multicast_receiver, response_sender).await;

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
        let result = serve_internal([1], "", 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn ignore_other_service_names() {
        let request = Request {
            service_name: "UNKNOWN".into(),
            response_collector_port: 3,
        };
        let request_address = "[::123]:2".parse().unwrap();

        let mut multicast_receiver = MockMulticastReceiver::default();
        let mut requests = [
            Ok((request.encode_to_vec(), request_address)),
            Err(ErrorKind::Other.into()),
        ]
        .into_iter();
        multicast_receiver
            .expect_join_multicast()
            .returning(|_, _| Ok(()));
        multicast_receiver
            .expect_receive()
            .returning(move || requests.next().unwrap());

        let response_sender = MockResponseSender::default();

        // when
        let result = serve_internal([1], "SERVICE", 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    fn assert_server_exits_with_dummy_error(result: std::io::Result<()>) {
        assert_eq!(
            ErrorKind::Other,
            result.unwrap_err().kind(),
            "Server must have exited with the dummy error supplied at the end of all requests"
        );
    }
}
