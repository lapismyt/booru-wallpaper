use std::io::Write;

use crate::{
    config::BWConfig,
    types::{BWWallpaperSetter, WallpaperCandidate},
};

use super::{
    criteria::{should_rotate_portrait, validate_wallpaper_candidate},
    media::{is_supported_content_type, prepare_wallpaper_path, set_wallpaper},
    temp_files::CachedTempDir,
};

pub async fn process_wallpaper_candidate(
    config: &BWConfig,
    client: &reqwest::Client,
    referer: &str,
    wallpaper_setter: &BWWallpaperSetter,
    candidate: &WallpaperCandidate,
    dry_run: bool,
) -> anyhow::Result<()> {
    log::debug!("Downloading wallpaper candidate from: {}", candidate.url);

    let response = client
        .get(&candidate.url)
        .header("Referer", referer)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

    let content_type = response
        .headers()
        .get("content-type")
        .map(|value| value.to_str().unwrap_or("").to_string());
    if let Some(content_type_str) = content_type.as_deref() {
        log::debug!("Content-Type: {:?}", content_type_str);
        if !is_supported_content_type(wallpaper_setter, content_type_str) {
            return Err(anyhow::anyhow!(
                "Unsupported content type for {:?}: {}",
                wallpaper_setter,
                content_type_str
            ));
        }
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read wallpaper bytes: {}", e))?;

    let temp_dir = CachedTempDir::new("booru-wallpaper")?;
    let file_name = candidate
        .url
        .split('/')
        .next_back()
        .filter(|s| !s.is_empty())
        .map(|s| s.split('?').next().unwrap_or(s))
        .filter(|s| !s.is_empty())
        .unwrap_or("wallpaper.png");
    let file_path = temp_dir.path().join(file_name);

    log::debug!("Saving wallpaper to: {:?}", file_path);
    let mut file = std::fs::File::create(&file_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;

    let metadata_dimensions = candidate.width.zip(candidate.height);
    let rotate_portrait = should_rotate_portrait(metadata_dimensions, config);
    let content_type_for_check = content_type.clone();
    let config_for_check = config.clone();
    let file_path_for_check = file_path.clone();
    tokio::task::spawn_blocking(move || {
        validate_wallpaper_candidate(
            &file_path_for_check,
            content_type_for_check.as_deref(),
            metadata_dimensions,
            &config_for_check,
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))??;

    if dry_run {
        println!("{}", candidate.url);
        return Ok(());
    }

    let content_type_for_prepare = content_type.clone();
    let config_for_prepare = config.clone();
    let wallpaper_setter_for_prepare = wallpaper_setter.clone();
    tokio::task::spawn_blocking(move || {
        let prepared_path = prepare_wallpaper_path(
            &wallpaper_setter_for_prepare,
            &file_path,
            content_type_for_prepare.as_deref(),
            rotate_portrait,
            &config_for_prepare,
        )?;
        set_wallpaper(&wallpaper_setter_for_prepare, &prepared_path)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))??;

    Ok(())
}
