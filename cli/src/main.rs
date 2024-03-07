mod server;

use clap::Parser;
use clap::Subcommand;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    match Cli::parse().command {
        Command::Serve {
            service_name,
            service_port,
        } => crate::server::serve(&service_name, service_port).await?,
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
    Serve {
        #[arg(long)]
        service_name: String,

        #[arg(long)]
        service_port: u16,
    },
}
