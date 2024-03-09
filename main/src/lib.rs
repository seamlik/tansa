mod multicast;
mod response_collector;
mod response_sender;
mod scanner;
mod server;
mod stream;

use std::net::SocketAddrV6;

pub use scanner::Scanner;
pub use scanner::Service;
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
