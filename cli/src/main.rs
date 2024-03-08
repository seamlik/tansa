mod network;

use clap::Parser;
use clap::Subcommand;
use futures_util::TryStreamExt;
use tansa::Scanner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    match Cli::parse().command {
        Command::Serve {
            service_name,
            service_port,
        } => serve(&service_name, service_port).await,
        Command::Scan { service_name } => scan(service_name).await,
    }
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(long)]
        service_name: String,

        #[arg(long)]
        service_port: u16,
    },
    Scan {
        #[arg(long)]
        service_name: String,
    },
}

async fn serve(service_name: &str, service_port: u16) -> anyhow::Result<()> {
    let multicast_interface_indexes = crate::network::get_multicast_interface_indexes().await?;
    tansa::serve(multicast_interface_indexes, service_name, service_port).await?;
    Ok(())
}

async fn scan(service_name: String) -> anyhow::Result<()> {
    let multicast_interface_indexes = crate::network::get_multicast_interface_indexes().await?;
    Scanner::new(service_name, multicast_interface_indexes)
        .await?
        .scan()
        .try_for_each(|service| {
            println!("Scanned: {:?}", service);
            async { Ok(()) }
        })
        .await?;
    Ok(())
}
