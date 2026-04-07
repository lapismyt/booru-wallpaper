use std::path::PathBuf;

use clap::Parser;

use crate::{
    config::BWConfig,
    fetch::fetch_and_set_wallpaper,
    types::{BWImageboard, BWRating, BWSortBy, get_default_config_path},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliArgs {
    /// Imageboard to use.
    #[arg(short, long)]
    pub imageboard: Option<BWImageboard>,

    /// Minimum score filter.
    #[arg(short, long)]
    pub min_score: Option<u32>,

    /// Tags to search for.
    #[arg(short, long)]
    pub tags: Option<String>,

    /// Ignore images with these tags.
    #[arg(short = 'B', long)]
    pub blacklist_tags: Option<String>,

    /// Safety rating.
    #[arg(short, long)]
    pub rating: Option<BWRating>,

    /// Cycle interval in seconds. Runs once if not set.
    #[arg(short, long)]
    pub cycle_interval_seconds: Option<u64>,

    /// API key for the imageboard.
    #[arg(short, long)]
    pub api_key: Option<String>,

    /// User ID for the imageboard.
    #[arg(short, long)]
    pub user_id: Option<String>,

    /// Posts sort_by option.
    #[arg(short, long)]
    pub sort_by: Option<BWSortBy>,

    /// Path to the base config file.
    /// Can be disabled with "none" to use only CLI args.
    #[arg(default_value = "default")]
    pub config: Option<String>,

    /// Disable resolution filtering tags.
    #[arg(short = 'D', long)]
    pub disable_resolution_filter: bool,

    /// Dry run - only print the image URL, don't set it on the wallpaper.
    #[arg(short, long)]
    pub dry_run: bool,
}

pub async fn run() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    let config: BWConfig = match &args.config {
        None => BWConfig::default(),
        Some(path) => {
            if path.is_empty() {
                BWConfig::default()
            } else if path.to_lowercase() == "none" {
                BWConfig::default()
            } else {
                let actual_path = if path.to_lowercase() == "default" {
                    get_default_config_path()
                } else {
                    PathBuf::from(path)
                };

                log::debug!("config path: {:?}", actual_path);

                if actual_path
                    .extension()
                    .ok_or(anyhow::anyhow!("Config file must be a .toml file"))?
                    != "toml"
                {
                    return Err(anyhow::anyhow!("Config file must be a .toml file"));
                }
                let _config = std::fs::read_to_string(&actual_path).map_err(|e| {
                    anyhow::anyhow!("Unable to read {}: {}", &actual_path.display(), e)
                })?;
                toml::from_str(&_config).map_err(|e| {
                    anyhow::anyhow!(
                        "Unable to parse config file {}: {}",
                        &actual_path.display(),
                        e
                    )
                })?
            }
        }
    };

    let config = config.with_cli_args(&args);

    let Some(tags) = &config.tags else {
        return Err(anyhow::anyhow!("No tags specified"));
    };

    if tags.is_empty() {
        return Err(anyhow::anyhow!("No tags specified"));
    }

    if config.cycle_interval_seconds.is_none() {
        run_once(&config, args.dry_run).await;
        Ok(())
    } else {
        loop {
            run_once(&config, args.dry_run).await;
            tokio::time::sleep(std::time::Duration::from_secs(
                config.cycle_interval_seconds.unwrap(),
            ))
            .await;
        }
    }
}

async fn run_once(config: &BWConfig, dry_run: bool) {
    let start_time = std::time::Instant::now();

    if let Err(e) = fetch_and_set_wallpaper(config, dry_run).await {
        log::error!("Failed to fetch or set wallpaper image: {}", e);
    }

    let elapsed = start_time.elapsed();
    log::debug!("Wallpaper image fetched and set in {:?}", elapsed);
}
