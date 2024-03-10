use crate::multicast::TokioMulticastReceiver;
use crate::packet::MulticastPacketReceiver;
use crate::response_sender::GrpcResponseSender;
use crate::response_sender::ResponseSender;
use futures_util::TryFutureExt;
use futures_util::TryStreamExt;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use tansa_protocol::MulticastPacket;
use tansa_protocol::Response;

pub async fn serve(discovery_port: u16, service_port: u16) -> Result<(), crate::Error> {
    serve_internal(
        discovery_port,
        service_port,
        TokioMulticastReceiver,
        GrpcResponseSender,
    )
    .await
}

async fn serve_internal(
    discovery_port: u16,
    service_port: u16,
    multicast_receiver: impl MulticastPacketReceiver,
    response_sender: impl ResponseSender,
) -> Result<(), crate::Error> {
    if discovery_port == 0 {
        return Err(crate::Error::InvalidDiscoveryPort);
    }

    let multicast_address = SocketAddrV6::new(crate::get_discovery_ip(), discovery_port, 0, 0);
    let handle = |(request, remote_address): (_, SocketAddrV6)| {
        handle_packet(
            request,
            *remote_address.ip(),
            service_port,
            &response_sender,
        )
        .inspect_err(|e| log::error!("Failed to handle a packet: {}", e))
        .or_else(|_| async { Ok(()) })
    };
    multicast_receiver
        .receive(multicast_address)
        .try_for_each_concurrent(0, handle)
        .await
        .map_err(Into::into)
}

async fn handle_packet(
    packet: MulticastPacket,
    remote_ip: Ipv6Addr,
    service_port: u16,
    response_sender: &impl ResponseSender,
) -> anyhow::Result<()> {
    log::debug!("Received {:?} via multicast from {}", packet, remote_ip);
    let request = packet
        .unwrap_request()
        .ok_or_else(|| anyhow::anyhow!("Not a `Request`"))?;
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
    use crate::packet::MockMulticastPacketReceiver;
    use crate::response_sender::MockResponseSender;
    use futures_util::stream::BoxStream;
    use futures_util::StreamExt;
    use mockall::predicate::eq;
    use std::io::ErrorKind::Other;
    use tansa_protocol::Request;

    const DISCOVERY_PORT: u16 = 50000;

    #[tokio::test]
    async fn serve() {
        crate::test::init();

        let request: MulticastPacket = Request {
            response_collector_port: 3,
        }
        .into();
        let request_address = "[::123]:2".parse().unwrap();

        let expected_response = Response { service_port: 10 };

        let mut multicast_receiver = MockMulticastPacketReceiver::default();
        let requests = [Ok((request.clone(), request_address)), Err(Other.into())];
        multicast_receiver
            .expect_receive()
            .return_once(|_| futures_util::stream::iter(requests).boxed());

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
            DISCOVERY_PORT,
            expected_response.service_port.try_into().unwrap(),
            multicast_receiver,
            response_sender,
        )
        .await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn invalid_discovery_port() {
        crate::test::init();

        // When
        let e = super::serve(0, 1).await.unwrap_err();

        // Then
        if let crate::Error::InvalidDiscoveryPort = e {
        } else {
            panic!("0 must be an invalid discovery port");
        }
    }

    #[tokio::test]
    async fn failing_to_handle_packet_does_not_stop_serving() {
        crate::test::init();

        let request_source_address = "[::123]:2".parse().unwrap();

        let mut multicast_receiver = MockMulticastPacketReceiver::default();
        let requests = one_shot_request_ending_with_dummy_error(Ok((
            Request::default().into(),
            request_source_address,
        )));
        multicast_receiver
            .expect_receive()
            .return_once(|_| requests);

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .return_once(|_, _| anyhow::bail!("Failed to send response"));

        // when
        let result = serve_internal(DISCOVERY_PORT, 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn handle_many_requests() {
        crate::test::init();

        let request_size = 128;
        let request_source_address = "[::123]:2".parse().unwrap();
        let requests =
            std::iter::repeat_with(move || Ok((Request::default().into(), request_source_address)))
                .take(request_size)
                .chain(std::iter::once(Err(Other.into())));

        let mut multicast_receiver = MockMulticastPacketReceiver::default();
        multicast_receiver
            .expect_receive()
            .return_once(|_| futures_util::stream::iter(requests).boxed());

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .times(request_size)
            .returning(|_, _| Ok(()));

        // when
        let result = serve_internal(DISCOVERY_PORT, 1, multicast_receiver, response_sender).await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    fn one_shot_request_ending_with_dummy_error(
        request: Result<(MulticastPacket, SocketAddrV6), std::io::Error>,
    ) -> BoxStream<'static, Result<(MulticastPacket, SocketAddrV6), std::io::Error>> {
        let requests = [request, Err(Other.into())];
        futures_util::stream::iter(requests).boxed()
    }

    fn assert_server_exits_with_dummy_error(result: Result<(), crate::Error>) {
        if let crate::Error::NetworkIo(e) = result.unwrap_err() {
            assert_eq!(
                Other,
                e.kind(),
                "Server must have exited with the dummy error supplied at the end of all requests"
            );
        }
    }
}
