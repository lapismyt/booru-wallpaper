use std::path::PathBuf;

use clap::Parser;

use crate::{
    config::BWConfig,
    fetch::fetch_and_set_wallpaper,
    types::{BWImageboard, BWRating, BWSortBy, BWWallpaperSetter, get_default_config_path},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliArgs {
    /// Imageboard to use. "safebooru" by default.
    #[arg(short, long)]
    pub imageboard: Option<BWImageboard>,

    /// Minimum score filter. No minimum by default.
    #[arg(short, long)]
    pub min_score: Option<u32>,

    /// Tags to search for. No tags by default.
    #[arg(short, long)]
    pub tags: Option<String>,

    /// Ignore images with these tags. Empty by default.
    #[arg(short = 'B', long)]
    pub blacklist_tags: Option<String>,

    /// Safety rating. Not set by default.
    #[arg(short, long)]
    pub rating: Option<BWRating>,

    /// Cycle interval in seconds. Runs once by default.
    #[arg(short, long)]
    pub cycle_interval_seconds: Option<u64>,

    /// API key for the imageboard. Not set by default.
    #[arg(short, long)]
    pub api_key: Option<String>,

    /// User ID for the imageboard. Not set by default.
    #[arg(short, long)]
    pub user_id: Option<String>,

    /// Posts sort_by option. Random by default.
    #[arg(short, long)]
    pub sort_by: Option<BWSortBy>,

    /// Path to the base config file.
    /// Can be disabled with "none" to use only CLI args.
    /// By default, uses ~/.config on UNIX and AppData on Windows.
    #[arg(default_value = "default")]
    pub config: Option<String>,

    /// Disable resolution filtering tags and local size checks. False by default.
    #[arg(short = 'D', long)]
    pub disable_resolution_filter: bool,

    /// Wallpaper setter backend. "wallpaper" by default.
    #[arg(short, long)]
    pub wallpaper_setter: Option<BWWallpaperSetter>,

    /// Maximum retries after the first failed attempt. 3 by default.
    #[arg(short = 'R', long)]
    pub max_retries: Option<u32>,

    /// Delay between retries in seconds. 2 by default.
    #[arg(short = 'I', long)]
    pub retry_interval_seconds: Option<u64>,

    /// Number of posts fetched per attempt before retrying. 100 by default.
    #[arg(short = 'b', long)]
    pub batch_size: Option<u32>,

    /// Minimum wallpaper width. 1600 by default.
    #[arg(short = 'W', long)]
    pub wallpaper_min_width: Option<u32>,

    /// Minimum wallpaper height. 900 by default.
    #[arg(short = 'E', long)]
    pub wallpaper_min_height: Option<u32>,

    /// Minimum wallpaper aspect ratio. 1.6 by default.
    #[arg(short = 'n', long)]
    pub wallpaper_aspect_ratio_min: Option<f32>,

    /// Maximum wallpaper aspect ratio. 2.1 by default.
    #[arg(short = 'x', long)]
    pub wallpaper_aspect_ratio_max: Option<f32>,

    /// Maximum duration in seconds used when preparing animated wallpapers. 12 by default.
    #[arg(short = 'T', long)]
    pub animated_max_duration_seconds: Option<u32>,

    /// FPS used when preparing animated wallpapers. 10 by default.
    #[arg(short = 'F', long)]
    pub animated_fps: Option<u32>,

    /// Output width used when preparing animated wallpapers. 1280 by default.
    #[arg(short = 'P', long)]
    pub animated_width: Option<u32>,

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

                if !actual_path.exists() {
                    tokio::fs::write(&actual_path, include_str!("resources/config.toml.example"))
                        .await
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to create template config in {}: {}",
                                &actual_path.display(),
                                e
                            )
                        })?;
                    log::info!("Config template is saved to {}", &actual_path.display())
                };

                let _config = tokio::fs::read_to_string(&actual_path).await.map_err(|e| {
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
