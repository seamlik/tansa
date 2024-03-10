mod multicast;
mod packet;
mod response_collector;
mod response_sender;
mod scanner;
mod server;
mod stream;

pub use scanner::scan;
pub use scanner::ScanError;
pub use scanner::Service;
pub use server::serve;

use std::net::Ipv6Addr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid discovery port")]
    InvalidDiscoveryPort,

    #[error("Network I/O error")]
    NetworkIo(#[from] std::io::Error),
}

/// IPv6 multicast address used in service discovery.
///
/// The "group ID" part (the last 16 bytes) was randomly generated.
///
/// The multicast scope is set to 5 meaning organization-local networks.
/// This scope covers LAN devices routed through VPNs.
fn get_discovery_ip() -> Ipv6Addr {
    "FF05::F329:58AF".parse().expect("Invalid IP")
}

#[cfg(test)]
mod test {
    use log::LevelFilter::Info;

    pub fn init() {
        let _ = env_logger::builder()
            .is_test(true)
            .filter_level(Info)
            .try_init();
    }
}
