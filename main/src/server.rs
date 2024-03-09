use crate::multicast::MulticastReceiver;
use crate::multicast::TokioMulticastReceiver;
use crate::response_sender::GrpcResponseSender;
use crate::response_sender::ResponseSender;
use futures_util::StreamExt;
use futures_util::TryFutureExt;
use futures_util::TryStreamExt;
use std::net::IpAddr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use tansa_protocol::DecodeError;
use tansa_protocol::Request;
use tansa_protocol::RequestDecoder;
use tansa_protocol::Response;

pub async fn serve(service_name: &str, service_port: u16) -> std::io::Result<()> {
    let multicast_ip = *crate::get_multicast_address().ip();
    let multicast_receiver = TokioMulticastReceiver::new(
        crate::get_multicast_address().port(),
        multicast_ip,
        RequestDecoder,
    )
    .await?;
    serve_internal(
        service_name,
        service_port,
        multicast_receiver,
        GrpcResponseSender,
    )
    .await
}

async fn serve_internal(
    service_name: &str,
    service_port: u16,
    multicast_receiver: impl MulticastReceiver<Request, DecodeError>,
    response_sender: impl ResponseSender,
) -> std::io::Result<()> {
    let handle = |(request, remote_address): (_, SocketAddr)| {
        handle_packet(
            request,
            remote_address.ip(),
            service_name,
            service_port,
            &response_sender,
        )
        .inspect_err(|e| log::error!("Failed to handle a packet: {}", e))
        .or_else(|_| async { Ok(()) })
    };
    multicast_receiver
        .receive()
        .filter_map(|r| async { strip_protobuf_error(r) })
        .try_for_each_concurrent(0, handle)
        .await
}

fn strip_protobuf_error(
    result: Result<(Request, SocketAddr), DecodeError>,
) -> Option<std::io::Result<(Request, SocketAddr)>> {
    match result {
        Ok(inner) => Some(Ok(inner)),
        Err(DecodeError::Io(e)) => Some(Err(e)),
        Err(DecodeError::Protobuf(e)) => {
            log::warn!("Invalid Protocol Buffers packet for `Request`: {}", e);
            None
        }
    }
}

async fn handle_packet(
    request: Request,
    remote_ip: IpAddr,
    service_name: &str,
    service_port: u16,
    response_sender: &impl ResponseSender,
) -> anyhow::Result<()> {
    log::debug!("Received {:?} via multicast from {}", request, remote_ip);
    let remote_ip = unwrap_ipv6(remote_ip)?;
    if service_name != request.service_name {
        anyhow::bail!("Unknown service: {}", service_name);
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

fn unwrap_ipv6(ip: IpAddr) -> anyhow::Result<Ipv6Addr> {
    if let IpAddr::V6(ip) = ip {
        Ok(ip)
    } else {
        Err(anyhow::anyhow!("Must be IPv6"))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::multicast::MockMulticastReceiver;
    use crate::response_sender::MockResponseSender;
    use futures_util::stream::BoxStream;
    use mockall::predicate::eq;
    use prost::Message;
    use std::io::ErrorKind::Other;

    #[tokio::test]
    async fn serve() {
        crate::test::init();

        let request = Request {
            service_name: "SERVICE".into(),
            response_collector_port: 3,
        };
        let request_address = "[::123]:2".parse().unwrap();

        let expected_response = Response { service_port: 10 };

        let mut multicast_receiver = MockMulticastReceiver::default();
        let requests = [
            Ok((request.clone(), request_address)),
            Err(DecodeError::Io(Other.into())),
        ];
        multicast_receiver
            .expect_receive()
            .return_once(|| futures_util::stream::iter(requests).boxed());

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
        crate::test::init();

        let request_source_address = "[::123]:2".parse().unwrap();

        let mut multicast_receiver = MockMulticastReceiver::default();
        let requests = one_shot_request_ending_with_dummy_error(Ok((
            Default::default(),
            request_source_address,
        )));
        multicast_receiver.expect_receive().return_once(|| requests);

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .return_once(|_, _| anyhow::bail!("Failed to send response"));

        // when
        let result = serve_internal("", 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn handle_many_requests() {
        crate::test::init();

        let request_size = 128;
        let request_source_address = "[::123]:2".parse().unwrap();
        let requests =
            std::iter::repeat_with(move || Ok((Default::default(), request_source_address)))
                .take(request_size)
                .chain(std::iter::once(Err(DecodeError::Io(Other.into()))));

        let mut multicast_receiver = MockMulticastReceiver::default();
        multicast_receiver
            .expect_receive()
            .return_once(|| futures_util::stream::iter(requests).boxed());

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .times(request_size)
            .returning(|_, _| Ok(()));

        // when
        let result = serve_internal("", 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn ignore_invalid_protobuf() {
        crate::test::init();

        let mut multicast_receiver = MockMulticastReceiver::default();
        let requests = [
            Err(DecodeError::Protobuf(new_prost_decode_error())),
            Err(DecodeError::Io(Other.into())),
        ]
        .into_iter();
        multicast_receiver
            .expect_receive()
            .return_once(|| futures_util::stream::iter(requests).boxed());

        let response_sender = MockResponseSender::default();

        // when
        let result = serve_internal("SERVICE", 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn ignore_ipv4() {
        crate::test::init();

        let request = Request {
            service_name: "SERVICE".into(),
            response_collector_port: 3,
        };
        let request_address = "1.1.1.1:2".parse().unwrap();

        let mut multicast_receiver = MockMulticastReceiver::default();
        let requests =
            one_shot_request_ending_with_dummy_error(Ok((request.clone(), request_address)));
        multicast_receiver.expect_receive().return_once(|| requests);

        let response_sender = MockResponseSender::default();

        // when
        let result = serve_internal("", 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn ignore_other_service_names() {
        crate::test::init();

        let request = Request {
            service_name: "UNKNOWN".into(),
            response_collector_port: 3,
        };
        let request_address = "[::123]:2".parse().unwrap();

        let mut multicast_receiver = MockMulticastReceiver::default();
        let requests = one_shot_request_ending_with_dummy_error(Ok((request, request_address)));
        multicast_receiver.expect_receive().return_once(|| requests);

        let response_sender = MockResponseSender::default();

        // when
        let result = serve_internal("SERVICE", 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    fn one_shot_request_ending_with_dummy_error(
        request: Result<(Request, SocketAddr), DecodeError>,
    ) -> BoxStream<'static, Result<(Request, SocketAddr), DecodeError>> {
        let requests = [request, Err(DecodeError::Io(Other.into()))];
        futures_util::stream::iter(requests).boxed()
    }

    fn assert_server_exits_with_dummy_error(result: std::io::Result<()>) {
        assert_eq!(
            Other,
            result.unwrap_err().kind(),
            "Server must have exited with the dummy error supplied at the end of all requests"
        );
    }

    fn new_prost_decode_error() -> prost::DecodeError {
        let data: Vec<u8> = vec![1, 2, 3];
        Request::decode(data.as_slice()).unwrap_err()
    }
}
