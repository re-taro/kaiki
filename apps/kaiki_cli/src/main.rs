mod error;
mod init;
mod prepare;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::error::CliError;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(name = "kaiki", about = "Visual regression testing tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to config file
    #[arg(short, long, default_value = "regconfig.json")]
    config: PathBuf,

    /// Dry run mode
    #[arg(short = 't', long)]
    test: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Quiet mode
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run full pipeline (sync → compare → publish → notify)
    Run,
    /// Compare images only
    Compare,
    /// Download expected images
    SyncExpected,
    /// Upload results (use -n to also send notifications)
    Publish {
        /// Also send notifications
        #[arg(short = 'n', long)]
        notify: bool,
    },
    /// Initialize configuration file
    Init,
    /// Prepare plugin configurations
    Prepare,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let log_level = if cli.quiet {
        tracing::Level::ERROR
    } else if cli.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt().with_max_level(log_level).init();

    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let dry_run = cli.test;

    let result = rt.block_on(async {
        match cli.command {
            Commands::Run => run_pipeline(&cli.config, dry_run).await,
            Commands::Compare => run_compare(&cli.config, dry_run).await,
            Commands::SyncExpected => run_sync_expected(&cli.config, dry_run).await,
            Commands::Publish { notify } => run_publish(&cli.config, notify, dry_run).await,
            Commands::Init => init::run_init_wizard(),
            Commands::Prepare => prepare::run_prepare(&cli.config),
        }
    });

    match result {
        Ok(has_failures) => {
            if has_failures {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "command failed");
            ExitCode::from(1)
        }
    }
}

async fn run_pipeline(config_path: &std::path::Path, dry_run: bool) -> Result<bool, CliError> {
    let config = kaiki_config::load_config(config_path)?;
    let working_dir = PathBuf::from(&config.core.working_dir);
    std::fs::create_dir_all(&working_dir)?;

    let processor = build_processor(config, dry_run).await?;
    let result = processor.run().await?;
    Ok(result.has_failures)
}

async fn run_compare(config_path: &std::path::Path, dry_run: bool) -> Result<bool, CliError> {
    let config = kaiki_config::load_config(config_path)?;
    let processor = build_processor(config, dry_run).await?;
    let comparison = processor.compare()?;
    Ok(comparison.has_failures())
}

async fn run_sync_expected(config_path: &std::path::Path, dry_run: bool) -> Result<bool, CliError> {
    let config = kaiki_config::load_config(config_path)?;
    let processor = build_processor(config, dry_run).await?;
    if let Some(key) = processor.get_expected_key()? {
        processor.sync_expected(&key).await?;
    }
    Ok(false)
}

async fn run_publish(
    config_path: &std::path::Path,
    notify: bool,
    dry_run: bool,
) -> Result<bool, CliError> {
    let config = kaiki_config::load_config(config_path)?;
    let processor = build_processor(config, dry_run).await?;
    let key = processor.get_actual_key()?;
    let _report_url = processor.publish(&key).await?;
    if notify {
        tracing::info!("notification would be sent here");
    }
    Ok(false)
}

async fn build_processor(
    config: kaiki_config::RegSuitConfiguration,
    dry_run: bool,
) -> Result<kaiki_core::processor::RegProcessor, CliError> {
    let working_dir = PathBuf::from(&config.core.working_dir);

    let keygen: Box<dyn kaiki_git::KeyGenerator> =
        if config.plugins.contains_key("reg-keygen-git-hash-plugin") {
            Box::new(kaiki_git::commit_explorer::GitHashKeygen::new(&PathBuf::from("."))?
                as kaiki_git::commit_explorer::GitHashKeygen)
        } else if let Some(val) = config.plugins.get("reg-simple-keygen-plugin") {
            let keygen_config: kaiki_config::SimpleKeygenConfig =
                serde_json::from_value(val.clone())?;
            Box::new(kaiki_git::SimpleKeygen { expected_key: keygen_config.expected_key })
        } else {
            Box::new(kaiki_git::commit_explorer::GitHashKeygen::new(&PathBuf::from("."))?)
        };

    let mut storage: Option<Box<dyn kaiki_core::processor::StorageDyn>> = None;
    let mut notifiers: Vec<Box<dyn kaiki_core::processor::NotifierDyn>> = Vec::new();

    if !dry_run {
        if let Some(val) = config.plugins.get("reg-publish-s3-plugin") {
            let s3_config: kaiki_config::S3PluginConfig = serde_json::from_value(val.clone())?;
            let s3 = kaiki_storage::s3::S3Storage::new(s3_config).await?;
            storage = Some(Box::new(s3));
        } else if let Some(val) = config.plugins.get("reg-publish-gcs-plugin") {
            let gcs_config: kaiki_config::GcsPluginConfig = serde_json::from_value(val.clone())?;
            let gcs = kaiki_storage::gcs::GcsStorage::new(gcs_config).await?;
            storage = Some(Box::new(gcs));
        }

        if let Some(val) = config.plugins.get("reg-notify-github-plugin") {
            let gh_config: kaiki_config::GitHubNotifyConfig = serde_json::from_value(val.clone())?;
            let gh = kaiki_notify::github::GitHubNotifier::new(gh_config)?;
            notifiers.push(Box::new(gh));
        }

        if let Some(val) = config.plugins.get("reg-notify-slack-plugin") {
            let slack_config: kaiki_config::SlackNotifyConfig =
                serde_json::from_value(val.clone())?;
            let slack = kaiki_notify::slack::SlackNotifier::new(slack_config)?;
            notifiers.push(Box::new(slack));
        }
    }

    Ok(kaiki_core::processor::RegProcessor::new(
        config.core,
        working_dir,
        keygen,
        storage,
        notifiers,
    ))
}
