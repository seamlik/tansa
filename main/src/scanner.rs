use crate::network::ip_neighbor::IpNeighborScanError;
use crate::network::ip_neighbor::IpNeighborScanner;
use crate::network::udp_receiver::TokioUdpReceiver;
use crate::network::udp_sender::TokioUdpSender;
use crate::network::udp_sender::UdpSender;
use crate::packet::DiscoveryPacketReceiver;
use crate::response_collector::GrpcResponseCollector;
use crate::response_collector::ResponseCollector;
use anyhow::Context;
use futures_util::stream::BoxStream;
use futures_util::FutureExt;
use futures_util::Stream;
use futures_util::StreamExt;
use futures_util::TryFutureExt;
use futures_util::TryStreamExt;
use prost::Message;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tansa_protocol::DiscoveryPacket;
use tansa_protocol::Request;
use thiserror::Error;

/// Service information discovered during [scan].
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Service {
    pub address: SocketAddrV6,
}

/// Scans for all services being published to `discovery_port`.
pub fn scan(discovery_port: u16) -> impl Stream<Item = Result<Service, ScanError>> {
    let response_collector = GrpcResponseCollector::new().err_into();
    let ip_neighbor_scanner = crate::network::ip_neighbor::ip_neighbor_scanner().map(Ok);
    futures_util::future::try_join(response_collector, ip_neighbor_scanner)
        .map_ok(move |(c, s)| scan_internal(discovery_port, c, TokioUdpSender, TokioUdpReceiver, s))
        .try_flatten_stream()
}

fn scan_internal(
    discovery_port: u16,
    response_collector: impl ResponseCollector,
    udp_sender: impl UdpSender + Send + 'static,
    udp_receiver: impl DiscoveryPacketReceiver + Send + 'static,
    ip_neighbor_scanner: Box<dyn IpNeighborScanner + Send>,
) -> BoxStream<'static, Result<Service, ScanError>> {
    if discovery_port == 0 {
        return futures_util::stream::once(async { Err(ScanError::InvalidDiscoveryPort) }).boxed();
    }

    let discovery_ip = crate::get_discovery_ip();
    let request = Request {
        response_collector_port: response_collector.get_port().into(),
    };
    let services = futures_util::stream::select(
        response_collector.collect().map_err(Into::into),
        receive_announcements(udp_receiver, discovery_ip, discovery_port),
    );
    crate::stream::join(
        send_requests(udp_sender, ip_neighbor_scanner, request, discovery_port),
        services,
    )
    .boxed()
}

async fn send_requests(
    udp_sender: impl UdpSender,
    ip_neighbor_scanner: Box<dyn IpNeighborScanner + Send>,
    request: Request,
    discovery_port: u16,
) -> Result<(), ScanError> {
    let packet: DiscoveryPacket = request.into();
    let packet_bytes: Arc<[u8]> = packet.encode_to_vec().into();
    let multicast_ip = crate::get_discovery_ip();

    let multicast_address = SocketAddrV6::new(multicast_ip, discovery_port, 0, 0);
    log::debug!(
        "Sending {:?} to multicast address {}",
        packet,
        multicast_address
    );
    udp_sender
        .send_multicast(multicast_address, packet_bytes.clone())
        .await?;

    for ip_neighbor in ip_neighbor_scanner.scan().await?.into_iter() {
        let link_local_address = ip_neighbor.get_socket_address(discovery_port);
        log::debug!(
            "Sending {:?} to link-local address {}",
            packet,
            link_local_address
        );
        udp_sender
            .send_unicast(link_local_address, packet_bytes.clone())
            .await?;
    }
    Ok(())
}

fn receive_announcements(
    udp_receiver: impl DiscoveryPacketReceiver,
    discovery_ip: Ipv6Addr,
    discovery_port: u16,
) -> impl Stream<Item = Result<Service, ScanError>> {
    udp_receiver
        .receive(discovery_ip, discovery_port)
        .err_into()
        .map_ok(|(packet, address)| {
            extract_service(packet, address)
                .inspect_err(|e| log::debug!("Failed to handle an `Announcement`: {}", e))
                .ok()
        })
        .filter_map(|r| async { r.transpose() })
}

fn extract_service(packet: DiscoveryPacket, mut address: SocketAddrV6) -> anyhow::Result<Service> {
    let response = packet
        .unwrap_response()
        .ok_or_else(|| anyhow::anyhow!("Not an `Announcement`"))?;
    let port = response
        .service_port
        .try_into()
        .context("Port out of range")?;
    address.set_port(port);
    Ok(Service { address })
}

/// Error during [scan].
#[derive(Error, Debug)]
pub enum ScanError {
    #[error("Invalid discovery port")]
    InvalidDiscoveryPort,

    #[error("Multicast network error")]
    SendRequest(#[from] std::io::Error),

    #[error("gRPC error")]
    CollectResponse(#[from] tonic::transport::Error),

    #[error("IP neighbor scan error")]
    ScanIpNeighbor(#[from] IpNeighborScanError),
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::network::ip_neighbor::IpNeighbor;
    use crate::network::ip_neighbor::MockIpNeighborScanner;
    use crate::network::udp_sender::MockUdpSender;
    use crate::packet::MockDiscoveryPacketReceiver;
    use crate::response_collector::MockResponseCollector;
    use futures_util::FutureExt;
    use futures_util::StreamExt;
    use futures_util::TryStreamExt;
    use mockall::predicate::eq;
    use tansa_protocol::DiscoveryPacket;
    use tansa_protocol::Response;

    #[tokio::test]
    async fn scan() {
        crate::test::init();

        let expected_services = vec![
            Service {
                address: "[::1]:1".parse().unwrap(),
            },
            Service {
                address: "[::2]:2".parse().unwrap(),
            },
        ];
        let expected_services_clone = expected_services.clone();

        let announcement: DiscoveryPacket = Response { service_port: 2 }.into();
        let announcement_address = "[::2]:10".parse().unwrap();
        let discovery_packets =
            futures_util::stream::once(async move { Ok((announcement, announcement_address)) })
                .boxed();

        let discovery_ip = crate::get_discovery_ip();
        let discovery_port = 10;
        let multicast_address = SocketAddrV6::new(discovery_ip, discovery_port, 0, 0);
        let request = Request {
            response_collector_port: 100,
        };
        let packet: DiscoveryPacket = request.clone().into();
        let packet_bytes: Arc<[u8]> = packet.encode_to_vec().into();

        let ip_neighbors = vec![
            IpNeighbor {
                network_interface_index: 1,
                address: "fe80::1:abcd".parse().unwrap(),
            },
            IpNeighbor {
                network_interface_index: 2,
                address: "fe80::2:abcd".parse().unwrap(),
            },
        ];

        let mut response_collector = MockResponseCollector::new();
        response_collector
            .expect_get_port()
            .return_const(request.response_collector_port as u16);
        response_collector.expect_collect().return_once(|| {
            futures_util::stream::once(async move { expected_services_clone[0] })
                .map(Ok)
                .boxed()
        });

        let mut udp_sender = MockUdpSender::default();
        udp_sender
            .expect_send_multicast()
            .with(eq(multicast_address), eq(packet_bytes.clone()))
            .return_once(|_, _| async { Ok(()) }.boxed());
        udp_sender
            .expect_send_unicast()
            .with(
                eq("[fe80::1:abcd%1]:10".parse::<SocketAddrV6>().unwrap()),
                eq(packet_bytes.clone()),
            )
            .return_once(|_, _| async { Ok(()) }.boxed());
        udp_sender
            .expect_send_unicast()
            .with(
                eq("[fe80::2:abcd%2]:10".parse::<SocketAddrV6>().unwrap()),
                eq(packet_bytes.clone()),
            )
            .return_once(|_, _| async { Ok(()) }.boxed());

        let mut udp_receiver = MockDiscoveryPacketReceiver::default();
        udp_receiver
            .expect_receive()
            .with(eq(discovery_ip), eq(discovery_port))
            .return_once_st(move |_, _| discovery_packets);

        let mut ip_neighbor_scanner = MockIpNeighborScanner::default();
        ip_neighbor_scanner
            .expect_scan()
            .return_once(|| async { Ok(ip_neighbors) }.boxed());

        // When
        let actual_services: Vec<_> = scan_internal(
            multicast_address.port(),
            response_collector,
            udp_sender,
            udp_receiver,
            Box::new(ip_neighbor_scanner),
        )
        .take(3)
        .try_collect()
        .await
        .unwrap();

        // Then
        assert_eq!(actual_services, expected_services);
    }

    #[tokio::test]
    async fn invalid_discovery_port() {
        crate::test::init();

        // When
        let e = super::scan(0).try_collect::<Vec<_>>().await.unwrap_err();

        // Then
        if let ScanError::InvalidDiscoveryPort = e {
        } else {
            panic!("0 must be an invalid discovery port");
        }
    }
}
