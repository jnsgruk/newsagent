use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use env_logger::Env;
use newsagent::agent::Agent;
use newsagent::config::AppConfig;
use newsagent::links::{extract_links, LinkChecker, LinkStatus};
use newsagent::mentions::{check_mention, extract_mentions, MentionChecker, MentionStatus};
use newsagent::output;
use newsagent::tools::discourse::DiscourseConfig;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    version,
    about = "Draft a source-linked Tech Updates outline for an engineering newsletter",
    after_help = "Configuration is read from NEWSAGENT_* environment variables or a .env file.",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Write the outline to a new file instead of stdout; never overwrites"
    )]
    output: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        about = "Check that @mentions in a Markdown file are users on a configured Discourse",
        after_help = "Exits 0 when every mention is valid, 1 when any needs attention, 2 on error."
    )]
    CheckMentions {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    #[command(
        about = "Check that http(s) links in a Markdown file are reachable",
        after_help = "Retries temporary failures after 1s, 5s and 10s. \
            Exits 0 when every link is reachable, 1 when any is not, 2 on error."
    )]
    CheckLinks {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
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

    match cli.command {
        Some(command) => {
            let result = match command {
                Command::CheckMentions { file } => check_mentions(&file).await,
                Command::CheckLinks { file } => check_links(&file).await,
            };
            Ok(result.unwrap_or_else(|err| {
                eprintln!("Error: {err:#}");
                ExitCode::from(2)
            }))
        }
        None => {
            generate(cli.output.as_deref()).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

async fn generate(output_path: Option<&Path>) -> anyhow::Result<()> {
    let config = AppConfig::from_env().context("Reading configuration")?;
    let agent = Agent::new(config.clone())?;
    let response = agent.prompt().await?;

    match output_path {
        Some(path) => output::write_new(path, &response)?,
        None => println!("{}", response),
    }
    Ok(())
}

async fn check_mentions(file: &Path) -> anyhow::Result<ExitCode> {
    let config = envy::prefixed("NEWSAGENT_")
        .from_env::<DiscourseConfig>()
        .context("Reading Discourse configuration")?;
    if config.instances.is_empty() {
        bail!("No Discourse instances are configured in NEWSAGENT_DISCOURSE_INSTANCES");
    }
    let checkers = config
        .instances
        .iter()
        .map(|i| {
            MentionChecker::new(
                &i.base_url,
                &format!("https://{}", i.base_url),
                i.api_key.clone(),
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let text =
        std::fs::read_to_string(file).with_context(|| format!("Reading {}", file.display()))?;

    let (mut valid, mut suggested, mut unknown) = (0, 0, 0);
    for handle in extract_mentions(&text) {
        match check_mention(&checkers, &handle).await? {
            MentionStatus::Valid(host) => {
                valid += 1;
                println!("valid @{handle} ({host})");
            }
            MentionStatus::Suggested(c) => {
                suggested += 1;
                println!("suggest @{handle} -> @{} ({})", c.username, c.host);
            }
            MentionStatus::Unknown(candidates) => {
                unknown += 1;
                if candidates.is_empty() {
                    println!("unknown @{handle}");
                } else {
                    let candidates: Vec<String> = candidates
                        .iter()
                        .map(|c| format!("@{} ({})", c.username, c.host))
                        .collect();
                    println!("unknown @{handle} (candidates: {})", candidates.join(", "));
                }
            }
        }
    }
    println!("SUMMARY valid={valid} suggest={suggested} unknown={unknown}");

    Ok(if suggested + unknown == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

async fn check_links(file: &Path) -> anyhow::Result<ExitCode> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("Reading {}", file.display()))?;
    let checker = LinkChecker::with_default_retries()?;

    let (mut ok, mut broken, mut temporary) = (0, 0, 0);
    for (link, status) in checker.check_all(extract_links(&text)).await {
        match status {
            LinkStatus::Ok => ok += 1,
            LinkStatus::Broken(reason) => {
                broken += 1;
                println!("broken {reason} {} (line {})", link.url, link.line);
            }
            LinkStatus::Temporary(reason) => {
                temporary += 1;
                println!("temporary {reason} {} (line {})", link.url, link.line);
            }
        }
    }
    println!("SUMMARY ok={ok} broken={broken} temporary={temporary}");

    Ok(if broken + temporary == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
