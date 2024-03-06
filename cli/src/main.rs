mod server;

use clap::Parser;
use clap::Subcommand;

/// Dummy IPv6 multicast address used in testing service discovery.
///
/// The "group ID" part (the last 16 bytes) was randomly generated.
///
/// The multicast scope is set to 5 meaning organization-local networks.
/// This scope covers LAN devices routed through VPNs.
const MULTICAST_ADDRESS: &str = "[FF05::F329:58AF]:50000";

const SERVICE_NAME: &str = "test";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    match Cli::parse().command {
        Command::Serve { service_port } => crate::server::serve(service_port).await?,
    };
    Ok(())
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve { service_port: u16 },
}
