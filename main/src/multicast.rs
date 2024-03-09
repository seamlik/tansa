use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use mockall::automock;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tokio::net::UdpSocket;

#[automock]
pub trait MulticastReceiver {
    async fn receive(&self) -> std::io::Result<(Vec<u8>, SocketAddrV6)>;
}

pub struct TokioMulticastReceiver {
    socket: UdpSocket,
    buffer_size: usize,
}

impl TokioMulticastReceiver {
    pub async fn new(
        buffer_size: usize,
        local_port: u16,
        multicast_address: Ipv6Addr,
    ) -> std::io::Result<Self> {
        let address = format!("[::]:{}", local_port);
        log::info!("Binding `MulticastReceiver` socket at {}", address);
        let socket = UdpSocket::bind(address).await?;
        socket.join_multicast_v6(&multicast_address, 0)?;

        // Multicast loop should be enabled only in test.
        // Disabling it reduces the chance of flooding and filters out echoes.
        socket.set_multicast_loop_v6(false)?;

        Ok(Self {
            socket,
            buffer_size,
        })
    }
}

impl MulticastReceiver for TokioMulticastReceiver {
    async fn receive(&self) -> std::io::Result<(Vec<u8>, SocketAddrV6)> {
        let mut buffer = Vec::default();
        buffer.resize(self.buffer_size, 0);
        let (receive_size, remote_address) = self.socket.recv_from(&mut buffer).await?;
        buffer.resize(receive_size, 0);
        Ok((buffer, unwrap_ipv6(remote_address)))
    }
}

fn unwrap_ipv6(address: SocketAddr) -> SocketAddrV6 {
    if let SocketAddr::V6(addr) = address {
        addr
    } else {
        panic!("Must be IPv6")
    }
}

#[automock]
pub trait MulticastSender {
    fn send(
        &self,
        multicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>>;
}

pub struct TokioMulticastSender;

impl TokioMulticastSender {
    async fn send(multicast_address: SocketAddrV6, data: Arc<[u8]>) -> std::io::Result<()> {
        let socket = UdpSocket::bind("[::]:0").await?;
        log::debug!(
            "Created `MulticastSender` socket at {:?}",
            socket.local_addr()?
        );
        socket.join_multicast_v6(multicast_address.ip(), 0)?;

        log::debug!("Sending packet to multicast address {}", multicast_address);
        socket.send_to(&data, multicast_address).await?;
        Ok(())
    }
}

impl MulticastSender for TokioMulticastSender {
    fn send(
        &self,
        multicast_address: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>> {
        Self::send(multicast_address, data).boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn multicast() {
        crate::test::init();

        let address = crate::get_multicast_address();
        let expected_data = vec![1, 2, 3];

        let receiver = TokioMulticastReceiver::new(16, address.port(), *address.ip())
            .await
            .unwrap();
        receiver.socket.set_multicast_loop_v6(true).unwrap();

        let (_, (actual_data, _)) = futures_util::future::try_join(
            TokioMulticastSender.send(address, expected_data.clone().into()),
            receiver.receive(),
        )
        .await
        .unwrap();
        assert_eq!(expected_data, actual_data, "Must receive the packet back");
    }
}
