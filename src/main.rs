mod agent;
mod bus;
mod gateway;
mod router;
mod sandbox;
mod store;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "exoclaw")]
#[command(about = "A secure, WASM-sandboxed AI agent runtime")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Gateway {
        /// Port to bind to
        #[arg(short, long, default_value = "7200")]
        port: u16,

        /// Bind address
        #[arg(short, long, default_value = "127.0.0.1")]
        bind: String,

        /// Auth token (required for non-loopback)
        #[arg(long, env = "EXOCLAW_TOKEN")]
        token: Option<String>,
    },

    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Show runtime status
    Status,
}

#[derive(Subcommand)]
enum PluginAction {
    /// List loaded plugins
    List,
    /// Load a WASM plugin
    Load {
        /// Path to .wasm file
        path: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Gateway { port, bind, token } => {
            gateway::run(gateway::Config { port, bind, token }).await
        }
        Commands::Plugin { action } => match action {
            PluginAction::List => {
                println!("No plugins loaded.");
                Ok(())
            }
            PluginAction::Load { path } => sandbox::load_plugin(&path).await,
        },
        Commands::Status => {
            println!("exoclaw v{}", env!("CARGO_PKG_VERSION"));
            println!("status: idle");
            Ok(())
        }
    }
}
