use super::IpNeighbor;
use super::IpNeighborScanError;
use super::IpNeighborScanner;
use csv::ReaderBuilder;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::Ipv6Addr;

pub struct IpRoute2IpNeighborScanner;

impl IpRoute2IpNeighborScanner {
    async fn scan() -> Result<Vec<IpNeighbor>, IpNeighborScanError> {
        let ip_links = crate::process::run("ip", &["-json", "link"]).await?;
        let ip_neighbors = crate::process::run("ip", &["-family", "inet6", "neighbor"]).await?;
        Self::parse_output(&ip_links, &ip_neighbors).map_err(Into::into)
    }

    fn parse_output(
        ip_link_output: &[u8],
        ip_neighbor_output: &[u8],
    ) -> Result<Vec<IpNeighbor>, IpNeighborScanError> {
        let neighbors = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b' ')
            .from_reader(ip_neighbor_output)
            .deserialize()
            .collect::<Result<Vec<Neighbor>, _>>()?;
        neighbors
            .iter()
            .for_each(|n| log::debug!("Scanned IP neighbor: {:?}", n));

        let links: Vec<Link> = serde_json::from_reader(ip_link_output)?;
        links
            .iter()
            .for_each(|l| log::debug!("Scanned IP link: {:?}", l));
        let links: HashMap<_, _> = links.into_iter().map(|l| (l.ifname.clone(), l)).collect();

        let neighbors: Vec<_> = neighbors
            .into_iter()
            .filter(|n| n.state != "FAILED")
            .filter(|n| super::is_valid_ip(n.ip))
            .filter_map(|n| {
                links.get(&n.ifname).map(|l| IpNeighbor {
                    address: n.ip,
                    network_interface_index: l.ifindex,
                })
            })
            .collect();
        neighbors
            .iter()
            .for_each(|n| log::info!("Valid IP neighbor: {:?}", n));

        Ok(neighbors)
    }
}

impl IpNeighborScanner for IpRoute2IpNeighborScanner {
    fn supports_current_operating_system(&self) -> BoxFuture<'static, bool> {
        crate::process::probe("ip", &["-Version"]).boxed()
    }

    fn scan(&self) -> BoxFuture<'static, Result<Vec<IpNeighbor>, IpNeighborScanError>> {
        Self::scan().boxed()
    }
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case, dead_code)]
struct Neighbor {
    ip: Ipv6Addr,
    dev: String,
    ifname: String,
    lladdr: String,
    mac: String,
    router: String,
    state: String,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct Link {
    ifindex: u32,
    ifname: String,
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn scan() {
        crate::test::init();

        let scanner = IpRoute2IpNeighborScanner;

        if !scanner.supports_current_operating_system().await {
            println!("`iproute2` does not exist, skipping.");
            return;
        }

        scanner.scan().await.unwrap();
    }

    #[test]
    fn parse_output() {
        let ip_neighbor_output = r#"
fe80::1:abcd dev wlan0 lladdr 00:00:00:00:00:01 router REACHABLE
fe80::2:abcd dev eth0 lladdr 00:00:00:00:00:02 router STALE
fe80::3:abcd dev wlan2 lladdr 00:00:00:00:00:03 router STALE
ff02::4:abcd dev wlan0 lladdr 00:00:00:00:00:04 router REACHABLE
fe80::5:abcd dev wlan0 lladdr 00:00:00:00:00:05 router FAILED
        "#
        .trim();
        let ip_link_output = r#"
        [
          {
            "ifindex": 1,
            "ifname": "wlan0",
            "mtu": 1500
          },
          {
            "ifindex": 2,
            "ifname": "eth0",
            "mtu": 1500
          }
        ]
        "#;
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
        let actual_neighbors = IpRoute2IpNeighborScanner::parse_output(
            ip_link_output.as_bytes(),
            ip_neighbor_output.as_bytes(),
        )
        .unwrap();

        // Then
        assert_eq!(actual_neighbors, expected_neighbors);
    }
}
