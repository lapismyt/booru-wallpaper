mod batch;
mod criteria;
mod media;
mod processing;
mod temp_files;

use booru_rs::{DanbooruClient, GelbooruClient, Rule34Client, SafebooruClient};

use crate::{
    config::BWConfig,
    types::{BWImageboard, DEFAULT_IMAGEBOARD, DEFAULT_WALLPAPER_SETTER},
};

pub(super) const DEFAULT_MAX_RETRIES: u32 = 3;
pub(super) const DEFAULT_RETRY_INTERVAL_SECONDS: u64 = 2;
pub(super) const DEFAULT_BATCH_SIZE: u32 = 100;
pub(super) const DEFAULT_WALLPAPER_MIN_WIDTH: u32 = 1600;
pub(super) const DEFAULT_WALLPAPER_MIN_HEIGHT: u32 = 900;
pub(super) const DEFAULT_WALLPAPER_ASPECT_RATIO_MIN: f32 = 1.6;
pub(super) const DEFAULT_WALLPAPER_ASPECT_RATIO_MAX: f32 = 2.1;
pub(super) const DEFAULT_ANIMATED_MAX_DURATION_SECONDS: u32 = 12;
pub(super) const DEFAULT_ANIMATED_FPS: u32 = 10;
pub(super) const DEFAULT_ANIMATED_WIDTH: u32 = 1280;

pub async fn fetch_and_set_wallpaper(config: &BWConfig, dry_run: bool) -> anyhow::Result<()> {
    let max_attempts = config.max_retries.unwrap_or(DEFAULT_MAX_RETRIES) + 1;
    let retry_interval = std::time::Duration::from_secs(
        config
            .retry_interval_seconds
            .unwrap_or(DEFAULT_RETRY_INTERVAL_SECONDS),
    );

    for attempt in 1..=max_attempts {
        match fetch_and_set_wallpaper_once(config, dry_run).await {
            Ok(()) => return Ok(()),
            Err(error) if attempt == max_attempts => return Err(error),
            Err(error) => {
                log::warn!(
                    "Wallpaper attempt {}/{} failed: {}. Retrying...",
                    attempt,
                    max_attempts,
                    error
                );
                tokio::time::sleep(retry_interval).await;
            }
        }
    }

    Err(anyhow::anyhow!("Unknown wallpaper error"))
}

async fn fetch_and_set_wallpaper_once(config: &BWConfig, dry_run: bool) -> anyhow::Result<()> {
    let candidates = match config.imageboard.as_ref().unwrap_or(&DEFAULT_IMAGEBOARD) {
        BWImageboard::Gelbooru => batch::fetch_wallpapers::<GelbooruClient>(config).await?,
        BWImageboard::Rule34 => batch::fetch_wallpapers::<Rule34Client>(config).await?,
        BWImageboard::Safebooru => batch::fetch_wallpapers::<SafebooruClient>(config).await?,
        BWImageboard::Danbooru => batch::fetch_wallpapers::<DanbooruClient>(config).await?,
    };

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;
    let referer = referer_for_imageboard(config.imageboard.as_ref().unwrap_or(&DEFAULT_IMAGEBOARD));
    let wallpaper_setter = config
        .wallpaper_setter
        .clone()
        .unwrap_or(DEFAULT_WALLPAPER_SETTER);

    let mut last_candidate_error = None;
    for candidate in candidates {
        match processing::process_wallpaper_candidate(
            config,
            &client,
            referer,
            &wallpaper_setter,
            &candidate,
            dry_run,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                log::debug!("Skipping candidate {}: {}", candidate.url, error);
                last_candidate_error = Some(error);
            }
        }
    }

    Err(last_candidate_error.unwrap_or_else(|| {
        anyhow::anyhow!(
            "No suitable wallpaper found in the fetched batch of {} posts",
            config.batch_size.unwrap_or(DEFAULT_BATCH_SIZE)
        )
    }))
}

fn referer_for_imageboard(imageboard: &BWImageboard) -> &'static str {
    match imageboard {
        BWImageboard::Gelbooru => "https://gelbooru.com/",
        BWImageboard::Rule34 => "https://rule34.xxx/",
        BWImageboard::Safebooru => "https://safebooru.org/",
        BWImageboard::Danbooru => "https://danbooru.donmai.us/",
    }
}
