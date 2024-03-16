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

fn is_valid_ip(ip: Ipv6Addr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast()
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
        let scope = if self.is_link_local() {
            self.network_interface_index
        } else {
            0
        };
        SocketAddrV6::new(self.address, port, 0, scope)
    }

    fn is_link_local(&self) -> bool {
        self.address.octets().starts_with(&[0xFE, 0x80])
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn find_scanner() {
        ip_neighbor_scanner().await.scan().await.unwrap();
    }

    #[test]
    fn get_socket_address_unicast() {
        let neighbor = IpNeighbor {
            address: "2001::1".parse().unwrap(),
            network_interface_index: 123,
        };
        let port = 10;
        let expected_address: SocketAddrV6 = "[2001::1]:10".parse().unwrap();

        // When
        let actual_address = neighbor.get_socket_address(port);

        // Then
        assert_eq!(actual_address, expected_address);
    }

    #[test]
    fn get_socket_address_link_local() {
        let neighbor = IpNeighbor {
            address: "FE80::1".parse().unwrap(),
            network_interface_index: 123,
        };
        let port = 10;
        let expected_address: SocketAddrV6 = "[FE80::1%123]:10".parse().unwrap();

        // When
        let actual_address = neighbor.get_socket_address(port);

        // Then
        assert_eq!(actual_address, expected_address);
    }
}
