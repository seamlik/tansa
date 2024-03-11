use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use mockall::automock;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IpNeighborScanError {}

#[automock]
pub trait IpNeighborScanner {
    fn scan(&self) -> BoxFuture<'static, Result<Vec<IpNeighbor>, IpNeighborScanError>>;
}

pub struct DummyIpNeighborScanner;

impl IpNeighborScanner for DummyIpNeighborScanner {
    fn scan(&self) -> BoxFuture<'static, Result<Vec<IpNeighbor>, IpNeighborScanError>> {
        async { Ok(vec![]) }.boxed()
    }
}

pub struct IpNeighbor {
    pub address: Ipv6Addr,
    pub network_interface_index: u32,
}

impl IpNeighbor {
    pub fn get_socket_address(&self, port: u16) -> SocketAddrV6 {
        SocketAddrV6::new(self.address, port, 0, self.network_interface_index)
    }
}
