use crate::network::udp_receiver::UdpReceiver;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mockall::automock;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use tansa_protocol::DecodeError;
use tansa_protocol::DiscoveryPacket;
use tansa_protocol::ProtobufDecoder;

#[automock]
pub trait DiscoveryPacketReceiver {
    fn receive(
        &self,
        multicast_ip: Ipv6Addr,
        port: u16,
    ) -> BoxStream<'static, std::io::Result<(DiscoveryPacket, SocketAddrV6)>>;
}

impl<T> DiscoveryPacketReceiver for T
where
    T: UdpReceiver + Send,
{
    fn receive(
        &self,
        multicast_ip: Ipv6Addr,
        port: u16,
    ) -> BoxStream<'static, std::io::Result<(DiscoveryPacket, SocketAddrV6)>> {
        self.receive(multicast_ip, port, ProtobufDecoder::default())
            .filter_map(|r| async { strip_protobuf_error(r) })
            .filter_map(|r| async { extract_ipv6(r) })
            .boxed()
    }
}

fn strip_protobuf_error(
    result: Result<(DiscoveryPacket, SocketAddr), DecodeError>,
) -> Option<std::io::Result<(DiscoveryPacket, SocketAddr)>> {
    match result {
        Ok(inner) => Some(Ok(inner)),
        Err(DecodeError::Io(e)) => Some(Err(e)),
        Err(DecodeError::Protobuf(e)) => {
            log::debug!(
                "Invalid Protocol Buffers packet for `DiscoveryPacket`: {}",
                e
            );
            None
        }
    }
}

fn extract_ipv6(
    result: std::io::Result<(DiscoveryPacket, SocketAddr)>,
) -> Option<std::io::Result<(DiscoveryPacket, SocketAddrV6)>> {
    match result {
        Ok((_, SocketAddr::V4(_))) => {
            log::debug!("Dropping a packet from IPv4");
            None
        }
        Ok((packet, SocketAddr::V6(addr))) => Some(Ok((packet, addr))),
        Err(e) => Some(Err(e)),
    }
}
