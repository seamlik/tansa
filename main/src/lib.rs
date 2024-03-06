mod multicast;
mod response_sender;
mod server;

use std::net::SocketAddrV6;

pub use server::serve;

/// IPv6 multicast address used in service discovery.
///
/// The "group ID" part (the last 16 bytes) was randomly generated.
///
/// The multicast scope is set to 5 meaning organization-local networks.
/// This scope covers LAN devices routed through VPNs.
fn get_multicast_address() -> SocketAddrV6 {
    "[FF05::F329:58AF]:50000"
        .parse()
        .expect("Invalid multicast address")
}
