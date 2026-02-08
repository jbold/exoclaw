use clap::{Parser, Subcommand};
use std::io::{self, Write};
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
    /// Run first-time onboarding and save config with an API key
    Onboard {
        /// Provider to configure (anthropic|openai). If omitted, prompts interactively.
        #[arg(long)]
        provider: Option<String>,
    },

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
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("warn,exoclaw=info"))
        .unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Onboard { provider } => run_onboard(provider),
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

fn run_onboard(provider_arg: Option<String>) -> anyhow::Result<()> {
    let mut config = exoclaw::config::load()?;

    println!("exoclaw onboarding");
    println!(
        "This writes config to {}.",
        exoclaw::config::resolve_path().display()
    );

    let provider = resolve_provider(provider_arg, &config.agent.provider)?;
    let provider_default_model = default_model_for_provider(&provider);
    let provider_label = provider_display_name(&provider);
    let key_label = format!("{provider_label} API key");

    let key = rpassword::prompt_password(format!("{key_label}: "))?;
    let key = key.trim().to_string();
    if key.is_empty() {
        anyhow::bail!("{key_label} cannot be empty");
    }
    let key_path = exoclaw::secrets::store_api_key(&provider, &key)?;

    config.agent.provider = provider.clone();
    config.agent.model = provider_default_model.to_string();
    // Keep config key-free; runtime resolves key from env or credential file.
    config.agent.api_key = None;

    let path = exoclaw::config::save(&config)?;
    println!("Saved config to {}", path.display());
    println!("Saved API key to {}", key_path.display());
    println!(
        "Configured provider={} model={}",
        config.agent.provider, config.agent.model
    );
    println!("Next: cargo run -- gateway");

    Ok(())
}

fn resolve_provider(input: Option<String>, current: &str) -> anyhow::Result<String> {
    let provider = match input {
        Some(value) => value,
        None => {
            let default = if current == "openai" || current == "anthropic" {
                current
            } else {
                "anthropic"
            };
            prompt_with_default("Provider (anthropic/openai)", default)?
        }
    };

    let normalized = provider.trim().to_ascii_lowercase();
    if normalized != "anthropic" && normalized != "openai" {
        anyhow::bail!("invalid provider '{provider}': expected anthropic or openai");
    }
    Ok(normalized)
}

fn prompt_with_default(prompt: &str, default: &str) -> anyhow::Result<String> {
    print!("{prompt} [{default}]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(input.to_string())
    }
}

fn provider_display_name(provider: &str) -> &'static str {
    match provider {
        "openai" => "OpenAI",
        _ => "Anthropic",
    }
}

fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "openai" => "gpt-4o",
        _ => "claude-sonnet-4-5-20250929",
    }
}
