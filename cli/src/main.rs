use clap::Parser;
use clap::Subcommand;
use futures_util::TryStreamExt;
use tansa::Scanner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    match Cli::parse().command {
        Command::Serve {
            discovery_port,
            service_port,
        } => serve(discovery_port, service_port).await,
        Command::Scan { discovery_port } => scan(discovery_port).await,
    }
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Publishes a service to all connected LANs.
    Serve {
        /// Port to listen for multicast requests from scanners.
        #[arg(long)]
        discovery_port: u16,

        /// Port of the service you want to publish.
        #[arg(long)]
        service_port: u16,
    },

    /// Scans for services in all connected LANs.
    Scan {
        /// Port to send multicast requests to.
        #[arg(long)]
        discovery_port: u16,
    },
}

async fn serve(discovery_port: u16, service_port: u16) -> anyhow::Result<()> {
    tansa::serve(discovery_port, service_port).await?;
    Ok(())
}

async fn scan(discovery_port: u16) -> anyhow::Result<()> {
    Scanner::new(discovery_port)
        .await?
        .scan()
        .try_for_each(|service| {
            println!("Discovered {:?}", service);
            async { Ok(()) }
        })
        .await?;
    Ok(())
}
