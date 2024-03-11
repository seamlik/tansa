use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use futures_util::Stream;
use futures_util::TryFutureExt;
use mockall::automock;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio_util::codec::Decoder;
use tokio_util::udp::UdpFramed;

pub trait MulticastReceiver {
    fn receive<T, C, E>(
        &self,
        multicast_address: SocketAddrV6,
        decoder: C,
    ) -> impl Stream<Item = Result<(T, SocketAddr), E>> + Send + 'static
    where
        C: Decoder<Item = T, Error = E> + Send + 'static,
        E: From<std::io::Error> + 'static;
}

pub struct TokioMulticastReceiver;

impl TokioMulticastReceiver {
    async fn new_socket<C>(
        multicast_address: SocketAddrV6,
        decoder: C,
    ) -> std::io::Result<UdpFramed<C>> {
        let bind_address = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, multicast_address.port(), 0, 0);
        log::info!("Binding `MulticastReceiver` socket at {}", bind_address);
        let socket = UdpSocket::bind(bind_address).await?;
        socket.join_multicast_v6(multicast_address.ip(), 0)?;

        // Multicast loop should be enabled only in test.
        // Disabling it reduces the chance of flooding and filters out echoes.
        socket.set_multicast_loop_v6(false)?;
        #[cfg(test)]
        {
            socket.set_multicast_loop_v6(true)?;
        }

        Ok(UdpFramed::new(socket, decoder))
    }
}

impl MulticastReceiver for TokioMulticastReceiver {
    fn receive<T, C, E>(
        &self,
        multicast_address: SocketAddrV6,
        decoder: C,
    ) -> impl Stream<Item = Result<(T, SocketAddr), E>> + Send + 'static
    where
        C: Decoder<Item = T, Error = E> + Send + 'static,
        E: From<std::io::Error> + 'static,
    {
        Self::new_socket(multicast_address, decoder)
            .err_into()
            .try_flatten_stream()
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

        let address = SocketAddrV6::new(crate::get_discovery_ip(), 50000, 0, 0);
        let expected_data = vec![1, 2, 3];
        let codec = BytesCodec::default();

        let (actual_data, _) = crate::stream::join::<anyhow::Error, _, _, _, _, _, _>(
            TokioMulticastSender.send(address, expected_data.clone().into()),
            TokioMulticastReceiver.receive(address, codec),
        )
        .boxed()
        .next()
        .await
        .unwrap()
        .unwrap();
        assert_eq!(expected_data, actual_data, "Must receive the packet back");
    }
}
