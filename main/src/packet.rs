use crate::multicast::MulticastReceiver;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mockall::automock;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use tansa_protocol::DecodeError;
use tansa_protocol::MulticastPacket;
use tansa_protocol::ProtobufDecoder;

#[automock]
pub trait MulticastPacketReceiver {
    fn receive(
        &self,
        multicast_address: SocketAddrV6,
    ) -> BoxStream<'static, std::io::Result<(MulticastPacket, SocketAddrV6)>>;
}

impl<T> MulticastPacketReceiver for T
where
    T: MulticastReceiver + Send,
{
    fn receive(
        &self,
        multicast_address: SocketAddrV6,
    ) -> BoxStream<'static, std::io::Result<(MulticastPacket, SocketAddrV6)>> {
        self.receive(multicast_address, ProtobufDecoder::default())
            .filter_map(|r| async { strip_protobuf_error(r) })
            .filter_map(|r| async { extract_ipv6(r) })
            .boxed()
    }
}

fn strip_protobuf_error(
    result: Result<(MulticastPacket, SocketAddr), DecodeError>,
) -> Option<std::io::Result<(MulticastPacket, SocketAddr)>> {
    match result {
        Ok(inner) => Some(Ok(inner)),
        Err(DecodeError::Io(e)) => Some(Err(e)),
        Err(DecodeError::Protobuf(e)) => {
            log::debug!(
                "Invalid Protocol Buffers packet for `MulticastPacket`: {}",
                e
            );
            None
        }
    }
}

fn extract_ipv6(
    result: std::io::Result<(MulticastPacket, SocketAddr)>,
) -> Option<std::io::Result<(MulticastPacket, SocketAddrV6)>> {
    match result {
        Ok((_, SocketAddr::V4(_))) => {
            log::debug!("Dropping a packet from IPv4");
            None
        }
        Ok((packet, SocketAddr::V6(addr))) => Some(Ok((packet, addr))),
        Err(e) => Some(Err(e)),
    }
}
