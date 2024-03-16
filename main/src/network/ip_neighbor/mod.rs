mod linux;
mod windows;

use self::linux::IpRoute2IpNeighborScanner;
use self::windows::PowerShellIpNeighborScanner;
use crate::process::ProcessError;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use futures_util::StreamExt;
use mockall::automock;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use thiserror::Error;

pub async fn ip_neighbor_scanner() -> Box<dyn IpNeighborScanner + Send> {
    let scanners: Vec<Box<dyn IpNeighborScanner + Send>> = vec![
        Box::new(PowerShellIpNeighborScanner),
        Box::new(IpRoute2IpNeighborScanner),
    ];
    if let Some(supported_scanner) = futures_util::stream::iter(scanners)
        .filter(|s| s.supports_current_operating_system())
        .next()
        .await
    {
        supported_scanner
    } else {
        log::info!("Unsupported operating system, disabling IP neighbor discovery.");
        Box::new(DummyIpNeighborScanner)
    }
}

#[derive(Error, Debug)]
pub enum IpNeighborScanError {
    #[error("Failed in running an external command")]
    ChildProcess(#[from] ProcessError),

    #[error("Failed to parse the CSV output of a child process")]
    ParseCsv(#[from] csv::Error),

    #[error("Failed to parse the JSON output of a child process")]
    ParseJson(#[from] serde_json::Error),
}

#[automock]
pub trait IpNeighborScanner {
    fn supports_current_operating_system(&self) -> BoxFuture<'static, bool>;
    fn scan(&self) -> BoxFuture<'static, Result<Vec<IpNeighbor>, IpNeighborScanError>>;
}

struct DummyIpNeighborScanner;

impl IpNeighborScanner for DummyIpNeighborScanner {
    fn supports_current_operating_system(&self) -> BoxFuture<'static, bool> {
        async { true }.boxed()
    }

    fn scan(&self) -> BoxFuture<'static, Result<Vec<IpNeighbor>, IpNeighborScanError>> {
        async { Ok(vec![]) }.boxed()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct IpNeighbor {
    pub address: Ipv6Addr,
    pub network_interface_index: u32,
}

impl IpNeighbor {
    pub fn get_socket_address(&self, port: u16) -> SocketAddrV6 {
        SocketAddrV6::new(self.address, port, 0, self.network_interface_index)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn find_scanner() {
        ip_neighbor_scanner().await.scan().await.unwrap();
    }
}
