use crate::network::link_local::IpNeighbor;
use crate::network::link_local::IpNeighborScanError;
use crate::network::link_local::IpNeighborScanner;
use crate::network::udp_receiver::TokioUdpReceiver;
use crate::network::udp_sender::TokioUdpSender;
use crate::network::udp_sender::UdpSender;
use crate::packet::DiscoveryPacketReceiver;
use crate::response_sender::GrpcResponseSender;
use crate::response_sender::ResponseSender;
use anyhow::Context;
use futures_util::TryFutureExt;
use futures_util::TryStreamExt;
use prost::Message;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tansa_protocol::DiscoveryPacket;
use tansa_protocol::Response;
use thiserror::Error;

/// Error during [serve].
#[derive(Error, Debug)]
pub enum ServeError {
    #[error("Invalid discovery port")]
    InvalidDiscoveryPort,

    #[error("Network I/O error")]
    NetworkIo(#[from] std::io::Error),

    #[error("Failed to scan for IP neighbors")]
    IpNeighbor(#[from] IpNeighborScanError),
}

/// Publishes the service provided at `service_port` to `discovery port`.
pub async fn serve(discovery_port: u16, service_port: u16) -> Result<(), ServeError> {
    serve_internal(
        discovery_port,
        service_port,
        TokioUdpReceiver,
        GrpcResponseSender,
        TokioUdpSender,
        crate::network::link_local::ip_neighbor_scanner().await,
    )
    .await
}

async fn serve_internal(
    discovery_port: u16,
    service_port: u16,
    udp_receiver: impl DiscoveryPacketReceiver,
    response_sender: impl ResponseSender,
    udp_sender: impl UdpSender,
    ip_neighbor_scanner: Box<dyn IpNeighborScanner>,
) -> Result<(), crate::ServeError> {
    if discovery_port == 0 {
        return Err(crate::ServeError::InvalidDiscoveryPort);
    }

    let discovery_ip = crate::get_discovery_ip();
    let multicast_address = SocketAddrV6::new(discovery_ip, discovery_port, 0, 0);
    announce(
        multicast_address,
        service_port,
        udp_sender,
        ip_neighbor_scanner,
    )
    .await?;
    let handle = |(request, remote_address): (_, SocketAddrV6)| {
        handle_packet(request, remote_address, service_port, &response_sender)
            .inspect_err(|e| log::error!("Failed to handle a packet: {}", e))
            .or_else(|_| async { Ok(()) })
    };
    udp_receiver
        .receive(discovery_ip, discovery_port)
        .try_for_each_concurrent(0, handle)
        .await
        .map_err(Into::into)
}

async fn announce(
    discovery_address: SocketAddrV6,
    service_port: u16,
    udp_sender: impl UdpSender,
    ip_neighbor_scanner: Box<dyn IpNeighborScanner>,
) -> Result<(), ServeError> {
    let neighbors = ip_neighbor_scanner.scan().await?;
    let announcement: DiscoveryPacket = Response {
        service_port: service_port.into(),
    }
    .into();
    let announcement_bytes: Arc<[u8]> = announcement.encode_to_vec().into();
    udp_sender
        .send_multicast(discovery_address, announcement_bytes.clone())
        .await?;
    announce_to_ip_neighbors(
        discovery_address.port(),
        neighbors,
        announcement_bytes,
        udp_sender,
    )
    .await?;
    Ok(())
}

async fn announce_to_ip_neighbors(
    discovery_port: u16,
    neighbors: Vec<IpNeighbor>,
    data: Arc<[u8]>,
    udp_sender: impl UdpSender,
) -> std::io::Result<()> {
    let tasks = neighbors
        .into_iter()
        .map(|n| n.get_socket_address(discovery_port))
        .map(|a| udp_sender.send_unicast(a, data.clone()));
    futures_util::future::try_join_all(tasks)
        .map_ok(|_| ())
        .await
}

async fn handle_packet(
    packet: DiscoveryPacket,
    remote_address: SocketAddrV6,
    service_port: u16,
    response_sender: &impl ResponseSender,
) -> anyhow::Result<()> {
    log::debug!(
        "Received {:?} via multicast from {}",
        packet,
        remote_address
    );
    let request = packet
        .unwrap_request()
        .ok_or_else(|| anyhow::anyhow!("Not a `Request`"))?;
    let response_collector_port = request
        .response_collector_port
        .try_into()
        .context("Port out of range")?;
    let response_collector_address =
        get_response_collector_address(remote_address, response_collector_port);
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

fn get_response_collector_address(
    packet_source_address: SocketAddrV6,
    response_collector_port: u16,
) -> String {
    let mut address = packet_source_address;
    address.set_port(response_collector_port);
    format!("http://{}", address)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::network::link_local::MockIpNeighborScanner;
    use crate::network::udp_sender::MockUdpSender;
    use crate::packet::MockDiscoveryPacketReceiver;
    use crate::response_sender::MockResponseSender;
    use futures_util::stream::BoxStream;
    use futures_util::FutureExt;
    use futures_util::StreamExt;
    use mockall::predicate::eq;
    use std::io::ErrorKind::Other;
    use std::sync::Arc;
    use tansa_protocol::Request;

    const DISCOVERY_PORT: u16 = 50000;

    #[tokio::test]
    async fn serve() {
        crate::test::init();

        let request: DiscoveryPacket = Request {
            response_collector_port: 3,
        }
        .into();
        let request_address = "[::123%456]:2".parse().unwrap();
        let multicast_address = SocketAddrV6::new(crate::get_discovery_ip(), DISCOVERY_PORT, 0, 0);
        let ip_neighbors = vec![
            IpNeighbor {
                address: "::A".parse().unwrap(),
                network_interface_index: 1000,
            },
            IpNeighbor {
                address: "::B".parse().unwrap(),
                network_interface_index: 1001,
            },
        ];

        let response = Response { service_port: 10 };
        let response_bytes: Arc<[u8]> = DiscoveryPacket::from(response.clone())
            .encode_to_vec()
            .into();

        let mut udp_receiver = MockDiscoveryPacketReceiver::default();
        let requests = [Ok((request.clone(), request_address)), Err(Other.into())];
        udp_receiver
            .expect_receive()
            .return_once(|_, _| futures_util::stream::iter(requests).boxed());

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .with(
                eq(response.clone()),
                eq::<String>("http://[::123%456]:3".into()),
            )
            .return_once(|_, _| Ok(()));

        let mut udp_sender = MockUdpSender::default();
        udp_sender
            .expect_send_multicast()
            .with(eq(multicast_address), eq(response_bytes.clone()))
            .return_once(|_, _| async { Ok(()) }.boxed());
        udp_sender
            .expect_send_unicast()
            .with(
                eq("[::A%1000]:50000".parse::<SocketAddrV6>().unwrap()),
                eq(response_bytes.clone()),
            )
            .return_once(|_, _| async { Ok(()) }.boxed());
        udp_sender
            .expect_send_unicast()
            .with(
                eq("[::B%1001]:50000".parse::<SocketAddrV6>().unwrap()),
                eq(response_bytes.clone()),
            )
            .return_once(|_, _| async { Ok(()) }.boxed());

        let mut ip_neighbor_scanner = MockIpNeighborScanner::default();
        ip_neighbor_scanner
            .expect_scan()
            .return_once(|| async { Ok(ip_neighbors) }.boxed());

        // when
        let result = serve_internal(
            DISCOVERY_PORT,
            response.service_port.try_into().unwrap(),
            udp_receiver,
            response_sender,
            udp_sender,
            Box::new(ip_neighbor_scanner),
        )
        .await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    #[tokio::test]
    async fn port_out_of_range() {
        crate::test::init();

        let request: DiscoveryPacket = Request {
            response_collector_port: 10000000,
        }
        .into();
        let request_address = "[::123%456]:2".parse().unwrap();

        let mut udp_receiver = MockDiscoveryPacketReceiver::default();
        let requests = [Ok((request.clone(), request_address)), Err(Other.into())];
        udp_receiver
            .expect_receive()
            .return_once(|_, _| futures_util::stream::iter(requests).boxed());

        let response_sender = MockResponseSender::default();

        let mut udp_sender = MockUdpSender::default();
        udp_sender
            .expect_send_multicast()
            .return_once(|_, _| async { Ok(()) }.boxed());

        // when
        let result = serve_internal(
            DISCOVERY_PORT,
            10,
            udp_receiver,
            response_sender,
            udp_sender,
            mock_ip_neighbor_scanner(),
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
        if let ServeError::InvalidDiscoveryPort = e {
        } else {
            panic!("0 must be an invalid discovery port");
        }
    }

    #[tokio::test]
    async fn failing_to_handle_packet_does_not_stop_serving() {
        crate::test::init();

        let request_source_address = "[::123]:2".parse().unwrap();

        let mut udp_receiver = MockDiscoveryPacketReceiver::default();
        let requests = one_shot_request_ending_with_dummy_error(Ok((
            Request::default().into(),
            request_source_address,
        )));
        udp_receiver.expect_receive().return_once(|_, _| requests);

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .return_once(|_, _| anyhow::bail!("Failed to send response"));

        // when
        let result = serve_internal(
            DISCOVERY_PORT,
            1,
            udp_receiver,
            response_sender,
            mock_udp_sender(),
            mock_ip_neighbor_scanner(),
        )
        .await;

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

        let mut udp_receiver = MockDiscoveryPacketReceiver::default();
        udp_receiver
            .expect_receive()
            .return_once(|_, _| futures_util::stream::iter(requests).boxed());

        let mut response_sender = MockResponseSender::default();
        response_sender
            .expect_send()
            .times(request_size)
            .returning(|_, _| Ok(()));

        // when
        let result = serve_internal(
            DISCOVERY_PORT,
            1,
            udp_receiver,
            response_sender,
            mock_udp_sender(),
            mock_ip_neighbor_scanner(),
        )
        .await;

        // Then
        assert_server_exits_with_dummy_error(result);
    }

    fn one_shot_request_ending_with_dummy_error(
        request: Result<(DiscoveryPacket, SocketAddrV6), std::io::Error>,
    ) -> BoxStream<'static, Result<(DiscoveryPacket, SocketAddrV6), std::io::Error>> {
        let requests = [request, Err(Other.into())];
        futures_util::stream::iter(requests).boxed()
    }

    fn assert_server_exits_with_dummy_error(result: Result<(), crate::ServeError>) {
        if let ServeError::NetworkIo(e) = result.unwrap_err() {
            assert_eq!(
                Other,
                e.kind(),
                "Server must have exited with the dummy error supplied at the end of all requests"
            );
        }
    }

    fn mock_udp_sender() -> impl UdpSender {
        let mut udp_sender = MockUdpSender::default();
        udp_sender
            .expect_send_multicast()
            .return_once(|_, _| async { Ok(()) }.boxed());
        udp_sender
    }

    fn mock_ip_neighbor_scanner() -> Box<dyn IpNeighborScanner> {
        let mut scanner = MockIpNeighborScanner::default();
        scanner
            .expect_scan()
            .return_once(|| async { Ok(vec![]) }.boxed());
        Box::new(scanner)
    }
}
