use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use mockall::automock;
use socket2::Domain;
use socket2::Socket;
use socket2::Type;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use std::net::UdpSocket as StdUdpSocket;
use std::sync::Arc;
use tokio::net::UdpSocket;

#[automock]
pub trait MulticastReceiver {
    fn join_multicast(
        &self,
        multicast_ip: Ipv6Addr,
        network_interface_index: u32,
    ) -> std::io::Result<()>;
    async fn receive(&self) -> std::io::Result<(Vec<u8>, SocketAddrV6)>;
}

pub struct TokioMulticastReceiver {
    socket: UdpSocket,
    buffer_size: usize,
}

impl TokioMulticastReceiver {
    pub fn new(buffer_size: usize, port: u16) -> std::io::Result<Self> {
        Self::new_with_multicast_loop(buffer_size, port)
    }

    /// Creates a new instance with multicast loop enabled.
    ///
    /// Multicast loop should be enabled only in test.
    /// Disabling it makes sure that the socket never receives any packets sent from the current network interface.
    /// This reduces the chance of flooding and filters out echoes for [Scanner](crate::Scanner)s.
    fn new_with_multicast_loop(buffer_size: usize, port: u16) -> std::io::Result<Self> {
        let socket = new_multicast_socket()?;
        socket.set_multicast_loop_v6(true)?;

        let local_ip = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);
        log::info!("Binding multicast receiver socket at {}", local_ip);
        socket.bind(&local_ip.into())?;

        Ok(Self {
            socket: new_async_socket(socket)?,
            buffer_size,
        })
    }

    fn unwrap_ipv6(address: SocketAddr) -> SocketAddrV6 {
        if let SocketAddr::V6(addr) = address {
            addr
        } else {
            panic!("Must be IPv6")
        }
    }
}

impl MulticastReceiver for TokioMulticastReceiver {
    fn join_multicast(
        &self,
        multicast_ip: Ipv6Addr,
        network_interface_index: u32,
    ) -> std::io::Result<()> {
        log::info!(
            "Joining multicast group {} on network interface {}",
            multicast_ip,
            network_interface_index
        );
        self.socket
            .join_multicast_v6(&multicast_ip, network_interface_index)?;
        Ok(())
    }
    async fn receive(&self) -> std::io::Result<(Vec<u8>, SocketAddrV6)> {
        let mut buffer = Vec::default();
        buffer.resize(self.buffer_size, 0);
        let (receive_size, remote_address) = self.socket.recv_from(&mut buffer).await?;
        buffer.resize(receive_size, 0);
        Ok((buffer, Self::unwrap_ipv6(remote_address)))
    }
}

#[automock]
pub trait MulticastSender {
    fn send(
        &self,
        network_interface_index: u32,
        destination: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>>;
}

pub struct TokioMulticastSender;

impl TokioMulticastSender {
    async fn send(
        network_interface_index: u32,
        destination: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> std::io::Result<()> {
        let socket = new_multicast_socket()?;
        socket.set_multicast_if_v6(network_interface_index)?;
        socket.set_multicast_loop_v6(false)?;

        let local_ip = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0);
        socket.bind(&local_ip.into())?;

        new_async_socket(socket)?
            .send_to(&data, destination)
            .await?;
        Ok(())
    }
}

impl MulticastSender for TokioMulticastSender {
    fn send(
        &self,
        network_interface_index: u32,
        destination: SocketAddrV6,
        data: Arc<[u8]>,
    ) -> BoxFuture<'static, std::io::Result<()>> {
        Self::send(network_interface_index, destination, data).boxed()
    }
}

fn new_multicast_socket() -> std::io::Result<Socket> {
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    socket.set_only_v6(true)?;
    Ok(socket)
}

fn new_async_socket(socket: Socket) -> std::io::Result<UdpSocket> {
    let socket: StdUdpSocket = socket.into();
    socket.try_into()
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn multicast() {
        let address = crate::get_multicast_address();
        let expected_data = vec![1, 2, 3];

        let receiver = TokioMulticastReceiver::new_with_multicast_loop(16, address.port()).unwrap();
        receiver.join_multicast(*address.ip(), 1).unwrap();

        match futures_util::future::try_join(
            TokioMulticastSender.send(1, address, expected_data.clone().into()),
            receiver.receive(),
        )
        .await
        {
            Ok((_, (actual_data, _))) => {
                assert_eq!(expected_data, actual_data, "Must receive the packet back")
            }
            Err(e) if e.raw_os_error() == Some(101) => {
                println!("Network unreachable, ignoring test result.")
            }
            Err(e) => panic!("Test failed: {}", e),
        }
    }
}
