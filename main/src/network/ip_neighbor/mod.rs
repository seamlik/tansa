mod windows;

use self::windows::PowerShellIpNeighborScanner;
use crate::os::OperatingSystem;
use crate::process::ProcessError;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use mockall::automock;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use thiserror::Error;

pub async fn ip_neighbor_scanner() -> Box<dyn IpNeighborScanner> {
    match crate::os::detect_operating_system().await {
        Ok(OperatingSystem::Windows) => Box::new(PowerShellIpNeighborScanner),
        Ok(_) => {
            log::info!("Unsupported operating system, disabling IP neighbor discovery.");
            Box::new(DummyIpNeighborScanner)
        }
        Err(e) => {
            log::warn!("Failed to detect operating system: {}", e);
            log::info!("Unknown operating system, disabling IP neighbor discovery.");
            Box::new(DummyIpNeighborScanner)
        }
    }
}

#[derive(Error, Debug)]
pub enum IpNeighborScanError {
    #[error("Failed in running an external command")]
    ChildProcess(#[from] ProcessError),

    #[error("Failed to parse the CSV output of a child process")]
    ChildProcessCsvOutput(#[from] csv::Error),
}

#[automock]
pub trait IpNeighborScanner {
    fn scan(&self) -> BoxFuture<'static, Result<Vec<IpNeighbor>, IpNeighborScanError>>;
}

struct DummyIpNeighborScanner;

impl IpNeighborScanner for DummyIpNeighborScanner {
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
