use std::net::SocketAddr;

use clap::Parser;
use tracing::info;

#[derive(clap::Parser)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(clap::Parser)]
enum SubCommand {
    Master {
        addr: SocketAddr,
        target: SocketAddr,
    },
    Slave {
        addr: SocketAddr,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(fmt::layer())
        .init();
    let opts = Opts::parse();
    match opts.subcmd {
        SubCommand::Master { addr, target } => {
            let server = timesync::server::Server::new(
                timesync::server::Config {
                    target,
                    ..Default::default()
                },
                addr,
            )
            .await?;
            server.serve().await?;
        }
        SubCommand::Slave { addr } => {
            let (server, mut rx) = timesync::client::Server::new(Default::default(), addr).await?;
            tokio::spawn(async move {
                loop {
                    if let Some(msg) = rx.recv().await {
                        info!("msg: {:?}", msg);
                    }
                }
            });
            server.serve().await?;
        }
    }
    Ok(())
}
