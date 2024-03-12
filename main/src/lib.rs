//! LAN service discovery over IPv6 multicast.
//!
//! # General steps of usage
//!
//! Like traditional applications operating in the local IP.
//! When providing your service through network sockets, you must agree on a fixed port.
//! However, when using this crate, that fixed port is called "discovery port."
//! Your application can bind to a wildcard socket (like `0.0.0.0:0`),
//! after which the operating system will give you a concrete random port known as "service port".
//! You will then use [serve] to publish the service port to the discovery port.
//! Other devices within the same LAN can use [scan] to discover it.
//!
//! # Caveat
//!
//! IPv4, both in discovery and application, is unsupported.
//! This simplifies the implementation of this crate.
//! Given that we are only talking about LAN which is more likely to support IPv6,
//! it should not be a major limitation.
//!
//! # Frequently used terms
//!
//! * Discovery IP: The multicast IPv6 acting as rendezvous for service information.
//!   It is hardcoded in this crate.
//! * Discovery port: The port on the discovery IP acting as rendezvous for service information.
//!   Each application must use its own discovery port.
//! * Service port: The port to connect to your application.
//!   It is usually randomly allocated by the operating system.
//! * Service information: Necessary information you need to connect to a service you discoverd.
//!   Usually contains the socket address accessible within the LAN you connect.

mod network;
mod os;
mod packet;
mod process;
mod response_collector;
mod response_sender;
mod scanner;
mod server;
mod stream;

pub use crate::scanner::scan;
pub use crate::scanner::ScanError;
pub use crate::scanner::Service;
pub use crate::server::serve;
pub use crate::server::ServeError;

use std::net::Ipv6Addr;

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
