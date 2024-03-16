use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use mockall::automock;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tokio::net::UdpSocket;

#[automock]
pub trait UdpSender {
    fn send_multicast(
        &self,
        multicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>>;

    fn send_unicast(
        &self,
        unicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>>;
}

pub struct TokioUdpSender;

impl TokioUdpSender {
    async fn send_multicast(
        multicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> std::io::Result<()> {
        let socket = Self::new_socket().await?;
        socket.join_multicast_v6(multicast_address.ip(), 0)?;
        socket.send_to(&data, multicast_address).await?;
        Ok(())
    }

    async fn send_unicast(unicast_address: SocketAddrV6, data: Arc<[u8]>) -> std::io::Result<()> {
        let socket = Self::new_socket().await?;
        socket.send_to(&data, unicast_address).await?;
        Ok(())
    }

    async fn new_socket() -> std::io::Result<UdpSocket> {
        let socket = UdpSocket::bind("[::]:0").await?;
        log::debug!("Created `UdpSender` socket at {:?}", socket.local_addr()?);
        Ok(socket)
    }
}

impl UdpSender for TokioUdpSender {
    fn send_multicast(
        &self,
        multicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>> {
        Self::send_multicast(multicast_address, data).boxed()
    }

    fn send_unicast(
        &self,
        unicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>> {
        Self::send_unicast(unicast_address, data).boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::net::Ipv6Addr;

    #[tokio::test]
    async fn send_multicast() {
        crate::test::init();

        let address = SocketAddrV6::new(crate::get_discovery_ip(), 1, 0, 0);
        TokioUdpSender
            .send_multicast(address, Arc::new([1]))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn send_unicast() {
        crate::test::init();

        let address = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 1, 0, 0);
        TokioUdpSender
            .send_unicast(address, Arc::new([1]))
            .await
            .unwrap();
    }
}
