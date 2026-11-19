use anyhow::Context;
use clap::Parser;
use env_logger::Env;
use newsagent::agent::Agent;
use newsagent::config::AppConfig;
use newsagent::output;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Draft a source-linked Tech Updates outline for an engineering newsletter",
    after_help = "Configuration is read from NEWSAGENT_* environment variables or a .env file."
)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Write the outline to a new file instead of stdout; never overwrites"
    )]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(path) = &cli.output {
        output::ensure_absent(path)?;
    }

    // Parse the specified (or default) .env file
    let dotenv_path = env::var("NEWSAGENT_DOTENV_PATH").unwrap_or_else(|_| ".env".to_string());
    let dotenv_result = dotenvy::from_path(&dotenv_path);
    match dotenv_result {
        Ok(()) => log::info!("Loaded env from {}", dotenv_path),
        Err(err) => log::debug!("No .env loaded from {}: {}", dotenv_path, err),
    }

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let config = AppConfig::from_env().context("Reading configuration")?;
    let agent = Agent::new(config.clone())?;
    let response = agent.prompt().await?;

    match &cli.output {
        Some(path) => output::write_new(path, &response)?,
        None => println!("{}", response),
    }
    Ok(())
}
