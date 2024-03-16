use futures_util::Stream;
use futures_util::TryFutureExt;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use tokio::net::UdpSocket;
use tokio_util::codec::Decoder;
use tokio_util::udp::UdpFramed;

pub trait UdpReceiver {
    fn receive<T, C, E>(
        &self,
        multicast_ip: Ipv6Addr,
        port: u16,
        decoder: C,
    ) -> impl Stream<Item = Result<(T, SocketAddr), E>> + Send + 'static
    where
        C: Decoder<Item = T, Error = E> + Send + 'static,
        E: From<std::io::Error> + 'static;
}

pub struct TokioUdpReceiver;

impl TokioUdpReceiver {
    async fn new_socket<C>(
        multicast_ip: Ipv6Addr,
        port: u16,
        decoder: C,
    ) -> std::io::Result<UdpFramed<C>> {
        let bind_address = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);
        log::info!("Binding `UdpReceiver` socket at {}", bind_address);
        let socket = UdpSocket::bind(bind_address).await?;
        socket.join_multicast_v6(&multicast_ip, 0)?;

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

impl UdpReceiver for TokioUdpReceiver {
    fn receive<T, C, E>(
        &self,
        multicast_ip: Ipv6Addr,
        port: u16,
        decoder: C,
    ) -> impl Stream<Item = Result<(T, SocketAddr), E>> + Send + 'static
    where
        C: Decoder<Item = T, Error = E> + Send + 'static,
        E: From<std::io::Error> + 'static,
    {
        Self::new_socket(multicast_ip, port, decoder)
            .err_into()
            .try_flatten_stream()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::network::udp_sender::TokioUdpSender;
    use crate::network::udp_sender::UdpSender;
    use futures_util::StreamExt;
    use tokio_util::codec::BytesCodec;

    #[tokio::test]
    async fn multicast() {
        crate::test::init();

        let multicast_ip = crate::get_discovery_ip();
        let port = 50000;
        let address = SocketAddrV6::new(multicast_ip, port, 0, 0);
        let expected_data = vec![1, 2, 3];
        let codec = BytesCodec::default();

        let (actual_data, _) = crate::stream::join::<anyhow::Error, _, _, _, _, _, _>(
            TokioUdpSender.send_multicast(address, expected_data.clone().into()),
            TokioUdpReceiver.receive(multicast_ip, port, codec),
        )
        .boxed()
        .next()
        .await
        .unwrap()
        .unwrap();
        assert_eq!(expected_data, actual_data, "Must receive the packet back");
    }
}
