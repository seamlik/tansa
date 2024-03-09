use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use futures_util::Stream;
use mockall::automock;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio_util::codec::Decoder;
use tokio_util::udp::UdpFramed;

#[automock]
pub trait MulticastReceiver<T, E> {
    fn receive(self) -> impl Stream<Item = Result<(T, SocketAddr), E>>;
}

pub struct TokioMulticastReceiver<C>
where
    C: Decoder,
{
    socket: UdpSocket,
    decoder: C,
}

impl<C> TokioMulticastReceiver<C>
where
    C: Decoder,
{
    pub async fn new(
        local_port: u16,
        multicast_address: Ipv6Addr,
        decoder: C,
    ) -> std::io::Result<Self> {
        let address = format!("[::]:{}", local_port);
        log::info!("Binding `MulticastReceiver` socket at {}", address);
        let socket = UdpSocket::bind(address).await?;
        socket.join_multicast_v6(&multicast_address, 0)?;

        // Multicast loop should be enabled only in test.
        // Disabling it reduces the chance of flooding and filters out echoes.
        socket.set_multicast_loop_v6(false)?;

        Ok(Self { socket, decoder })
    }
}

impl<T, C, E> MulticastReceiver<T, E> for TokioMulticastReceiver<C>
where
    C: Decoder<Item = T, Error = E>,
{
    fn receive(self) -> impl Stream<Item = Result<(T, SocketAddr), E>> {
        UdpFramed::new(self.socket, self.decoder)
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
    use futures_util::StreamExt;
    use tokio_util::codec::BytesCodec;

    #[tokio::test]
    async fn multicast() {
        crate::test::init();

        let address = crate::get_multicast_address();
        let expected_data = vec![1, 2, 3];
        let codec = BytesCodec::default();

        let receiver = TokioMulticastReceiver::new(address.port(), *address.ip(), codec)
            .await
            .unwrap();
        receiver.socket.set_multicast_loop_v6(true).unwrap();

        let (actual_data, _) = crate::stream::join::<anyhow::Error, _, _, _, _, _, _>(
            TokioMulticastSender.send(address, expected_data.clone().into()),
            receiver.receive(),
        )
        .boxed()
        .next()
        .await
        .unwrap()
        .unwrap();
        assert_eq!(expected_data, actual_data, "Must receive the packet back");
    }
}
