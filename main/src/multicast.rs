use mockall::automock;
use socket2::Domain;
use socket2::Socket;
use socket2::Type;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV6;
use std::net::UdpSocket as StdUdpSocket;
use tokio::net::ToSocketAddrs;
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
        let socket = new_multicast_socket()?;
        let local_ip = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);
        socket.bind(&local_ip.into())?;
        log::info!("Multicast receiver socket listening at {}", local_ip);
        let socket = new_async_socket(socket)?;
        Ok(Self {
            socket,
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

#[derive(Default)]
pub struct MulticastSender;

impl MulticastSender {
    pub async fn send(
        &self,
        network_interface_index: u32,
        destination: impl ToSocketAddrs,
        data: &[u8],
    ) -> std::io::Result<()> {
        let socket = new_multicast_socket()?;
        socket.set_multicast_if_v6(network_interface_index)?;

        let local_ip = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0);
        socket.bind(&local_ip.into())?;

        new_async_socket(socket)?.send_to(data, destination).await?;
        Ok(())
    }
}

fn new_multicast_socket() -> std::io::Result<Socket> {
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, None)?;
    socket.set_multicast_loop_v6(false)?;
    socket.set_reuse_address(true)?;
    socket.set_only_v6(true)?;
    Ok(socket)
}

fn new_async_socket(socket: Socket) -> std::io::Result<UdpSocket> {
    let socket: StdUdpSocket = socket.into();
    socket.try_into()
}
