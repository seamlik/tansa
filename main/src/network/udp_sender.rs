use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use mockall::automock;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tokio::net::UdpSocket;

#[automock]
pub trait UdpSender {
    fn send(
        &self,
        multicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>>;
}

pub struct TokioUdpSender;

impl TokioUdpSender {
    async fn send(multicast_address: SocketAddrV6, data: Arc<[u8]>) -> std::io::Result<()> {
        let socket = UdpSocket::bind("[::]:0").await?;
        log::debug!("Created `UdpSender` socket at {:?}", socket.local_addr()?);
        socket.join_multicast_v6(multicast_address.ip(), 0)?;
        socket.send_to(&data, multicast_address).await?;
        Ok(())
    }
}

impl UdpSender for TokioUdpSender {
    fn send(
        &self,
        multicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>> {
        Self::send(multicast_address, data).boxed()
    }
}
