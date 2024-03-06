mod server;

use clap::Parser;
use clap::Subcommand;

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
