use clap::{Parser, Subcommand};
use tracing::info;
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
        /// Port to bind to (overrides config file)
        #[arg(short, long)]
        port: Option<u16>,

        /// Bind address (overrides config file)
        #[arg(short, long)]
        bind: Option<String>,

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
            let mut config = exoclaw::config::load()?;

            // CLI args override config file
            if let Some(p) = port {
                config.gateway.port = p;
            }
            if let Some(b) = bind {
                config.gateway.bind = b;
            }

            info!(
                provider = %config.agent.provider,
                model = %config.agent.model,
                plugins = config.plugins.len(),
                bindings = config.bindings.len(),
                "config loaded"
            );

            exoclaw::gateway::run(config, token).await
        }
        Commands::Plugin { action } => match action {
            PluginAction::List => {
                println!("No plugins loaded.");
                Ok(())
            }
            PluginAction::Load { path } => exoclaw::sandbox::load_plugin(&path).await,
        },
        Commands::Status => {
            println!("exoclaw v{}", env!("CARGO_PKG_VERSION"));
            println!("status: idle");
            Ok(())
        }
    }
}
