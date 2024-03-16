use super::IpNeighbor;
use super::IpNeighborScanError;
use super::IpNeighborScanner;
use csv::Reader;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use serde::Deserialize;
use std::net::Ipv6Addr;

pub struct PowerShellIpNeighborScanner;

impl PowerShellIpNeighborScanner {
    async fn scan() -> Result<Vec<IpNeighbor>, IpNeighborScanError> {
        let stdout = crate::process::eval(
            "pwsh",
            &["-Command", "-"],
            include_bytes!("./Print-IpNeighbors.ps1"),
        )
        .await?;
        Self::parse_output(&stdout).map_err(Into::into)
    }

    fn parse_output(output: &[u8]) -> Result<Vec<IpNeighbor>, csv::Error> {
        let neighbors = Reader::from_reader(output)
            .deserialize()
            .collect::<Result<Vec<NetNeighbor>, _>>()?;
        neighbors
            .iter()
            .for_each(|n| log::debug!("Scanned IP neighbor: {:?}", n));

        let neighbors: Vec<_> = neighbors
            .into_iter()
            .filter(|n| n.State != "Unreachable")
            .filter(|n| n.IPAddress.segments().starts_with(&[0xFE80, 0, 0, 0]))
            .map(Into::into)
            .collect();
        neighbors
            .iter()
            .for_each(|n| log::info!("Valid IP neighbor: {:?}", n));

        Ok(neighbors)
    }
}

impl IpNeighborScanner for PowerShellIpNeighborScanner {
    fn supports_current_operating_system(&self) -> BoxFuture<'static, bool> {
        crate::process::probe("pwsh", &["-Command", "Get-Command Get-NetNeighbor"]).boxed()
    }

    fn scan(&self) -> BoxFuture<'static, Result<Vec<IpNeighbor>, IpNeighborScanError>> {
        Self::scan().boxed()
    }
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct NetNeighbor {
    InterfaceIndex: u32,
    IPAddress: Ipv6Addr,
    State: String,
}

impl From<NetNeighbor> for IpNeighbor {
    fn from(value: NetNeighbor) -> Self {
        Self {
            address: value.IPAddress,
            network_interface_index: value.InterfaceIndex,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn scan() {
        crate::test::init();

        let scanner = PowerShellIpNeighborScanner;

        if !scanner.supports_current_operating_system().await {
            println!("PowerShell does not exist, skipping.");
            return;
        }

        scanner.scan().await.unwrap();
    }

    #[test]
    fn parse_output() {
        let output = r#"
"InterfaceIndex","IPAddress","State"
"1","fe80::1:abcd","Reachable"
"2","fe80::2:abcd","Permanent"
"3","fe80::3:abcd","Unreachable"
"4","ff02::4:abcd","Reachable"
        "#
        .trim();
        let expected_neighbors = vec![
            IpNeighbor {
                network_interface_index: 1,
                address: "fe80::1:abcd".parse().unwrap(),
            },
            IpNeighbor {
                network_interface_index: 2,
                address: "fe80::2:abcd".parse().unwrap(),
            },
        ];

        // When
        let actual_neighbors =
            PowerShellIpNeighborScanner::parse_output(output.as_bytes()).unwrap();

        // Then
        assert_eq!(actual_neighbors, expected_neighbors);
    }
}
