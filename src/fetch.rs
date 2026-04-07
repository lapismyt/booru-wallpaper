use booru_rs::{
    Client, DanbooruClient, GelbooruClient, Rule34Client, SafebooruClient, client::ClientBuilder,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use tempdir::TempDir;

use crate::{
    config::BWConfig,
    rating::BWRatingToBooruRating,
    types::{
        BWImageboard, BWSortBy, BWWallpaperSetter, DEFAULT_IMAGEBOARD, DEFAULT_WALLPAPER_SETTER,
        TryGetUrl,
    },
};

const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_RETRY_INTERVAL_SECONDS: u64 = 2;
const DEFAULT_BATCH_SIZE: u32 = 100;
const DEFAULT_WALLPAPER_MIN_WIDTH: u32 = 1600;
const DEFAULT_WALLPAPER_MIN_HEIGHT: u32 = 900;
const DEFAULT_WALLPAPER_ASPECT_RATIO_MIN: f32 = 1.6;
const DEFAULT_WALLPAPER_ASPECT_RATIO_MAX: f32 = 2.1;
const DEFAULT_ANIMATED_MAX_DURATION_SECONDS: u32 = 12;
const DEFAULT_ANIMATED_FPS: u32 = 10;
const DEFAULT_ANIMATED_WIDTH: u32 = 1280;

pub async fn fetch_and_set_wallpaper(config: &BWConfig, dry_run: bool) -> anyhow::Result<()> {
    let max_attempts = config.max_retries.unwrap_or(DEFAULT_MAX_RETRIES) + 1;
    let retry_interval = std::time::Duration::from_secs(
        config
            .retry_interval_seconds
            .unwrap_or(DEFAULT_RETRY_INTERVAL_SECONDS),
    );
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match fetch_and_set_wallpaper_once(config, dry_run).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                if attempt == max_attempts {
                    return Err(error);
                }

                log::warn!(
                    "Wallpaper attempt {}/{} failed: {}. Retrying...",
                    attempt,
                    max_attempts,
                    error
                );
                tokio::time::sleep(retry_interval).await;
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Unknown wallpaper error")))
}

async fn fetch_and_set_wallpaper_once(config: &BWConfig, dry_run: bool) -> anyhow::Result<()> {
    let img_urls = match config.imageboard.as_ref().unwrap_or(&DEFAULT_IMAGEBOARD) {
        BWImageboard::Gelbooru => fetch_wallpapers::<GelbooruClient>(config).await?,
        BWImageboard::Rule34 => fetch_wallpapers::<Rule34Client>(config).await?,
        BWImageboard::Safebooru => fetch_wallpapers::<SafebooruClient>(config).await?,
        BWImageboard::Danbooru => fetch_wallpapers::<DanbooruClient>(config).await?,
    };

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    let referer = match config.imageboard.as_ref().unwrap_or(&DEFAULT_IMAGEBOARD) {
        BWImageboard::Gelbooru => "https://gelbooru.com/",
        BWImageboard::Rule34 => "https://rule34.xxx/",
        BWImageboard::Safebooru => "https://safebooru.org/",
        BWImageboard::Danbooru => "https://danbooru.donmai.us/",
    };
    let wallpaper_setter = config
        .wallpaper_setter
        .clone()
        .unwrap_or(DEFAULT_WALLPAPER_SETTER);
    let mut last_candidate_error = None;

    for img_url in img_urls {
        match process_wallpaper_candidate(
            config,
            &client,
            referer,
            &wallpaper_setter,
            &img_url,
            dry_run,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                log::debug!("Skipping candidate {}: {}", img_url, error);
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

async fn process_wallpaper_candidate(
    config: &BWConfig,
    client: &reqwest::Client,
    referer: &str,
    wallpaper_setter: &BWWallpaperSetter,
    img_url: &str,
    dry_run: bool,
) -> anyhow::Result<()> {
    log::debug!("Downloading wallpaper candidate from: {}", img_url);

    let response = client
        .get(img_url)
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
        if !is_supported_content_type(&wallpaper_setter, content_type_str) {
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

    let tmp_dir = TempDir::new("booru-wallpaper")?;
    let file_name = img_url
        .split('/')
        .last()
        .filter(|s| !s.is_empty())
        .map(|s| s.split('?').next().unwrap_or(s))
        .filter(|s| !s.is_empty())
        .unwrap_or("wallpaper.png");
    let file_path = tmp_dir.path().join(file_name);

    log::debug!("Saving wallpaper to: {:?}", file_path);

    {
        let mut file = std::fs::File::create(&file_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }

    let file_path_for_check = file_path.clone();
    let content_type_for_check = content_type.clone();
    let config_for_check = config.clone();
    let validation = tokio::task::spawn_blocking(move || {
        validate_wallpaper_candidate(
            &file_path_for_check,
            content_type_for_check.as_deref(),
            &config_for_check,
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?;

    validation?;

    if dry_run {
        println!("{}", img_url);
        return Ok(());
    }

    let config_for_prepare = config.clone();
    let wallpaper_setter_for_prepare = wallpaper_setter.clone();
    let res = tokio::task::spawn_blocking(move || {
        let prepared_path = prepare_wallpaper_path(
            &wallpaper_setter_for_prepare,
            &file_path,
            content_type.as_deref(),
            &config_for_prepare,
        )?;
        set_wallpaper(&wallpaper_setter_for_prepare, &prepared_path)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e));

    res??;

    Ok(())
}

async fn fetch_wallpapers<C>(config: &BWConfig) -> anyhow::Result<Vec<String>>
where
    C: Client + BWRatingToBooruRating,
    C::Post: TryGetUrl,
{
    let mut builder = C::builder();

    let Some(mut tags) = config.tags.clone() else {
        return Err(anyhow::anyhow!("No tags specified"));
    };

    if let (Some(user_id), Some(api_key)) = (&config.user_id, &config.api_key) {
        builder = builder.set_credentials(api_key.to_string(), user_id.to_string());
    } else {
        log::warn!("User ID and API key is not specified");
    };

    let blacklisted = config.blacklist_tags.clone().unwrap_or_default();

    if let Some(imageboard) = &config.imageboard {
        if imageboard != &BWImageboard::Danbooru {
            let disable_resolution_filter = config.disable_resolution_filter.unwrap_or(false);

            if !disable_resolution_filter {
                tags.extend_from_slice(&[
                    format!(
                        "width:>={}",
                        config
                            .wallpaper_min_width
                            .unwrap_or(DEFAULT_WALLPAPER_MIN_WIDTH)
                    ),
                    format!(
                        "height:>={}",
                        config
                            .wallpaper_min_height
                            .unwrap_or(DEFAULT_WALLPAPER_MIN_HEIGHT)
                    ),
                ]);
            }
        }
    }

    if let Some(min_score) = config.min_score {
        tags.push(format!("score:>={}", min_score));
    }

    log::debug!("tags: {:?}", tags);

    for tag in tags {
        builder = builder.tag(tag)?;
    }

    for tag in blacklisted {
        builder = builder.blacklist_tag(tag);
    }

    if let Some(sort_by) = &config.sort_by {
        builder = apply_sorting(sort_by, builder);
    } else {
        builder = builder.random();
    }

    if let Some(rating) = &config.rating {
        builder = builder.rating(C::rating_from_bw(rating));
    }

    let batch_size = config.batch_size.unwrap_or(DEFAULT_BATCH_SIZE);

    let res = builder.limit(batch_size).build().get().await?;

    let urls = res
        .into_iter()
        .filter_map(|post| match post.try_get_url() {
            Ok(url) => Some(url.to_string()),
            Err(error) => {
                log::debug!("Skipping post without usable URL: {}", error);
                None
            }
        })
        .collect::<Vec<_>>();

    if urls.is_empty() {
        return Err(anyhow::anyhow!(
            "Unable to find a wallpaper in the fetched batch of {} posts",
            batch_size
        ));
    }

    Ok(urls)
}

fn apply_sorting<C: Client>(sort_by: &BWSortBy, builder: ClientBuilder<C>) -> ClientBuilder<C> {
    builder.sort(sort_by.into())
}

fn is_supported_content_type(wallpaper_setter: &BWWallpaperSetter, content_type: &str) -> bool {
    if content_type.starts_with("image/") {
        return true;
    }

    match wallpaper_setter {
        BWWallpaperSetter::Wallpaper | BWWallpaperSetter::Awww => {
            matches!(content_type, "video/mp4" | "video/webm")
        }
    }
}

fn prepare_wallpaper_path(
    wallpaper_setter: &BWWallpaperSetter,
    path: &Path,
    content_type: Option<&str>,
    config: &BWConfig,
) -> anyhow::Result<String> {
    match wallpaper_setter {
        BWWallpaperSetter::Wallpaper => prepare_path_for_wallpaper(path, content_type, config),
        BWWallpaperSetter::Awww => prepare_path_for_awww(path, content_type, config),
    }
}

fn prepare_path_for_wallpaper(
    path: &Path,
    content_type: Option<&str>,
    config: &BWConfig,
) -> anyhow::Result<String> {
    match content_type {
        Some("video/mp4") | Some("video/webm") => extract_static_frame(path, config),
        _ => path_to_string(path),
    }
}

fn prepare_path_for_awww(
    path: &Path,
    content_type: Option<&str>,
    config: &BWConfig,
) -> anyhow::Result<String> {
    match content_type {
        Some("video/mp4") | Some("video/webm") => convert_video_for_awww(path, config),
        _ => path_to_string(path),
    }
}

fn convert_video_for_awww(path: &Path, config: &BWConfig) -> anyhow::Result<String> {
    let output_path = gif_output_path(path);
    let video_filter = format!(
        "fps={},scale={}:-2:flags=lanczos",
        config.animated_fps.unwrap_or(DEFAULT_ANIMATED_FPS),
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH)
    );
    let max_duration = config
        .animated_max_duration_seconds
        .unwrap_or(DEFAULT_ANIMATED_MAX_DURATION_SECONDS)
        .to_string();

    log::debug!(
        "Converting animated wallpaper for awww: {} -> {} (max {}s, {} fps, width {})",
        path.display(),
        output_path.display(),
        max_duration,
        config.animated_fps.unwrap_or(DEFAULT_ANIMATED_FPS),
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH)
    );

    let output = Command::new("ffmpeg")
        .args(["-y", "-t"])
        .arg(&max_duration)
        .args(["-i"])
        .arg(path)
        .args(["-an", "-vf"])
        .arg(&video_filter)
        .args(["-loop", "0"])
        .arg(&output_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute ffmpeg for {}: {}", path.display(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        return Err(anyhow::anyhow!(
            "ffmpeg failed with status {} while converting {}. stdout: {} stderr: {}",
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string()),
            path.display(),
            stdout,
            stderr
        ));
    }

    path_to_string(&output_path)
}

fn extract_static_frame(path: &Path, config: &BWConfig) -> anyhow::Result<String> {
    let output_path = png_output_path(path);
    let frame_filter = format!(
        "scale={}:-2:flags=lanczos",
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH)
    );

    log::debug!(
        "Extracting static frame for wallpaper backend: {} -> {} (width {})",
        path.display(),
        output_path.display(),
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH)
    );

    let output = Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-vf"])
        .arg(&frame_filter)
        .arg(&output_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute ffmpeg for {}: {}", path.display(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        return Err(anyhow::anyhow!(
            "ffmpeg failed with status {} while extracting frame from {}. stdout: {} stderr: {}",
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string()),
            path.display(),
            stdout,
            stderr
        ));
    }

    path_to_string(&output_path)
}

fn gif_output_path(path: &Path) -> PathBuf {
    let mut output_path = path.to_path_buf();
    output_path.set_extension("gif");
    output_path
}

fn png_output_path(path: &Path) -> PathBuf {
    let mut output_path = path.to_path_buf();
    output_path.set_extension("png");
    output_path
}

fn path_to_string(path: &Path) -> anyhow::Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Failed to convert path to string: {}", path.display()))
}

fn validate_wallpaper_candidate(
    path: &Path,
    content_type: Option<&str>,
    config: &BWConfig,
) -> anyhow::Result<()> {
    if config.disable_resolution_filter.unwrap_or(false) {
        return Ok(());
    }

    let (width, height) = detect_dimensions(path, content_type)?;
    let min_width = config
        .wallpaper_min_width
        .unwrap_or(DEFAULT_WALLPAPER_MIN_WIDTH);
    let min_height = config
        .wallpaper_min_height
        .unwrap_or(DEFAULT_WALLPAPER_MIN_HEIGHT);
    let aspect_ratio_min = config
        .wallpaper_aspect_ratio_min
        .unwrap_or(DEFAULT_WALLPAPER_ASPECT_RATIO_MIN);
    let aspect_ratio_max = config
        .wallpaper_aspect_ratio_max
        .unwrap_or(DEFAULT_WALLPAPER_ASPECT_RATIO_MAX);
    let aspect_ratio = width as f32 / height as f32;

    log::debug!(
        "Candidate dimensions: {}x{}, aspect ratio {:.3}",
        width,
        height,
        aspect_ratio
    );

    if width < min_width {
        return Err(anyhow::anyhow!(
            "Wallpaper width {} is below configured minimum {}",
            width,
            min_width
        ));
    }

    if height < min_height {
        return Err(anyhow::anyhow!(
            "Wallpaper height {} is below configured minimum {}",
            height,
            min_height
        ));
    }

    if aspect_ratio < aspect_ratio_min || aspect_ratio > aspect_ratio_max {
        return Err(anyhow::anyhow!(
            "Wallpaper aspect ratio {:.3} is outside configured range [{:.3}, {:.3}]",
            aspect_ratio,
            aspect_ratio_min,
            aspect_ratio_max
        ));
    }

    Ok(())
}

fn detect_dimensions(path: &Path, content_type: Option<&str>) -> anyhow::Result<(u32, u32)> {
    match content_type {
        Some("video/mp4") | Some("video/webm") => detect_video_dimensions(path),
        _ => image::image_dimensions(path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read image dimensions for {}: {}",
                path.display(),
                e
            )
        }),
    }
}

fn detect_video_dimensions(path: &Path) -> anyhow::Result<(u32, u32)> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute ffprobe for {}: {}", path.display(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(anyhow::anyhow!(
            "ffprobe failed for {}: {}",
            path.display(),
            stderr
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let dims = stdout.trim();
    let (width, height) = dims.split_once('x').ok_or_else(|| {
        anyhow::anyhow!(
            "Unexpected ffprobe dimensions format for {}: {}",
            path.display(),
            dims
        )
    })?;

    Ok((width.parse()?, height.parse()?))
}

fn set_wallpaper(wallpaper_setter: &BWWallpaperSetter, path: &str) -> anyhow::Result<()> {
    log::debug!(
        "Setting wallpaper via {:?} from: {}",
        wallpaper_setter,
        path
    );

    match wallpaper_setter {
        BWWallpaperSetter::Wallpaper => wallpaper::set_from_path(path)
            .map_err(|e| anyhow::anyhow!("Failed to set wallpaper (path: {}): {}", path, e)),
        BWWallpaperSetter::Awww => {
            let output = Command::new("awww")
                .args(["img", path])
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to execute 'awww img {}': {}", path, e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

                return Err(anyhow::anyhow!(
                    "awww failed with status {}. stdout: {} stderr: {}",
                    output
                        .status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "terminated by signal".to_string()),
                    stdout,
                    stderr
                ));
            }

            log::debug!("awww stdout: {}", String::from_utf8(output.stdout)?);
            log::debug!("awww stderr: {}", String::from_utf8(output.stderr)?);

            Ok(())
        }
    }
}
