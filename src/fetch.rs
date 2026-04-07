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
        TryGetDimensions, TryGetUrl, WallpaperCandidate,
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
    let candidates = match config.imageboard.as_ref().unwrap_or(&DEFAULT_IMAGEBOARD) {
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

    for candidate in candidates {
        match process_wallpaper_candidate(
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

async fn process_wallpaper_candidate(
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
    let file_name = candidate
        .url
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
    let metadata_dimensions = candidate.width.zip(candidate.height);
    let rotate_portrait = should_rotate_portrait(metadata_dimensions, config);
    let config_for_check = config.clone();
    let validation = tokio::task::spawn_blocking(move || {
        validate_wallpaper_candidate(
            &file_path_for_check,
            content_type_for_check.as_deref(),
            metadata_dimensions,
            &config_for_check,
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?;

    validation?;

    if dry_run {
        println!("{}", candidate.url);
        return Ok(());
    }

    let config_for_prepare = config.clone();
    let wallpaper_setter_for_prepare = wallpaper_setter.clone();
    let res = tokio::task::spawn_blocking(move || {
        let prepared_path = prepare_wallpaper_path(
            &wallpaper_setter_for_prepare,
            &file_path,
            content_type.as_deref(),
            rotate_portrait,
            &config_for_prepare,
        )?;
        set_wallpaper(&wallpaper_setter_for_prepare, &prepared_path)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e));

    res??;

    Ok(())
}

async fn fetch_wallpapers<C>(config: &BWConfig) -> anyhow::Result<Vec<WallpaperCandidate>>
where
    C: Client + BWRatingToBooruRating,
    C::Post: TryGetUrl + TryGetDimensions,
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

    let candidates = res
        .into_iter()
        .filter_map(|post| match (post.try_get_url(), post.try_get_dimensions()) {
            (Ok(url), Ok((width, height))) => {
                if wallpaper_dimensions_match(width, height, config) {
                    Some(WallpaperCandidate {
                        url: url.to_string(),
                        width: Some(width),
                        height: Some(height),
                    })
                } else {
                    log::debug!(
                        "Skipping post by metadata dimensions: {}x{} does not match wallpaper criteria",
                        width,
                        height
                    );
                    None
                }
            }
            (Ok(url), Err(error)) => {
                log::debug!(
                    "Using post without metadata dimensions, fallback validation will happen after download: {}",
                    error
                );
                Some(WallpaperCandidate {
                    url: url.to_string(),
                    width: None,
                    height: None,
                })
            }
            (Err(error), _) => {
                log::debug!("Skipping post without usable URL: {}", error);
                None
            }
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Err(anyhow::anyhow!(
            "Unable to find a wallpaper in the fetched batch of {} posts",
            batch_size
        ));
    }

    Ok(candidates)
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
    rotate_portrait: bool,
    config: &BWConfig,
) -> anyhow::Result<String> {
    match wallpaper_setter {
        BWWallpaperSetter::Wallpaper => {
            prepare_path_for_wallpaper(path, content_type, rotate_portrait, config)
        }
        BWWallpaperSetter::Awww => {
            prepare_path_for_awww(path, content_type, rotate_portrait, config)
        }
    }
}

fn prepare_path_for_wallpaper(
    path: &Path,
    content_type: Option<&str>,
    rotate_portrait: bool,
    config: &BWConfig,
) -> anyhow::Result<String> {
    match content_type {
        Some("video/mp4") | Some("video/webm") => {
            extract_static_frame(path, config, rotate_portrait)
        }
        Some("image/gif") if rotate_portrait => rotate_image_clockwise(path),
        _ if rotate_portrait => rotate_image_clockwise(path),
        _ => path_to_string(path),
    }
}

fn prepare_path_for_awww(
    path: &Path,
    content_type: Option<&str>,
    rotate_portrait: bool,
    config: &BWConfig,
) -> anyhow::Result<String> {
    match content_type {
        Some("video/mp4") | Some("video/webm") => {
            convert_video_for_awww(path, config, rotate_portrait)
        }
        Some("image/gif") if rotate_portrait => rotate_gif_clockwise(path),
        _ if rotate_portrait => rotate_image_clockwise(path),
        _ => path_to_string(path),
    }
}

fn convert_video_for_awww(
    path: &Path,
    config: &BWConfig,
    rotate_portrait: bool,
) -> anyhow::Result<String> {
    let output_path = gif_output_path(path);
    let video_filter = animated_video_filter(config, rotate_portrait);
    let max_duration = config
        .animated_max_duration_seconds
        .unwrap_or(DEFAULT_ANIMATED_MAX_DURATION_SECONDS)
        .to_string();

    log::debug!(
        "Converting animated wallpaper for awww: {} -> {} (max {}s, {} fps, width {}, rotate {})",
        path.display(),
        output_path.display(),
        max_duration,
        config.animated_fps.unwrap_or(DEFAULT_ANIMATED_FPS),
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH),
        rotate_portrait
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

fn extract_static_frame(
    path: &Path,
    config: &BWConfig,
    rotate_portrait: bool,
) -> anyhow::Result<String> {
    let output_path = png_output_path(path);
    let frame_filter = static_frame_filter(config, rotate_portrait);

    log::debug!(
        "Extracting static frame for wallpaper backend: {} -> {} (width {}, rotate {})",
        path.display(),
        output_path.display(),
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH),
        rotate_portrait
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

fn rotated_png_output_path(path: &Path) -> PathBuf {
    let mut output_path = path.to_path_buf();
    output_path.set_file_name(format!(
        "{}.rotated.png",
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("wallpaper")
    ));
    output_path
}

fn rotated_gif_output_path(path: &Path) -> PathBuf {
    let mut output_path = path.to_path_buf();
    output_path.set_file_name(format!(
        "{}.rotated.gif",
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("wallpaper")
    ));
    output_path
}

fn path_to_string(path: &Path) -> anyhow::Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Failed to convert path to string: {}", path.display()))
}

fn rotate_image_clockwise(path: &Path) -> anyhow::Result<String> {
    let output_path = rotated_png_output_path(path);
    let image = image::open(path)
        .map_err(|e| anyhow::anyhow!("Failed to open image {}: {}", path.display(), e))?;
    let rotated = image.rotate90();
    rotated.save(&output_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to save rotated image {}: {}",
            output_path.display(),
            e
        )
    })?;
    path_to_string(&output_path)
}

fn rotate_gif_clockwise(path: &Path) -> anyhow::Result<String> {
    let output_path = rotated_gif_output_path(path);
    let output = Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(path)
        .args(["-vf", "transpose=1"])
        .arg(&output_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute ffmpeg for {}: {}", path.display(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        return Err(anyhow::anyhow!(
            "ffmpeg failed with status {} while rotating {}. stdout: {} stderr: {}",
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

fn validate_wallpaper_candidate(
    path: &Path,
    content_type: Option<&str>,
    metadata_dimensions: Option<(u32, u32)>,
    config: &BWConfig,
) -> anyhow::Result<()> {
    if config.disable_resolution_filter.unwrap_or(false) {
        return Ok(());
    }

    let (width, height) = match metadata_dimensions {
        Some(dimensions) => dimensions,
        None => detect_dimensions(path, content_type)?,
    };
    let source = if metadata_dimensions.is_some() {
        "post metadata"
    } else {
        "downloaded file"
    };
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
    let rotate_portrait = config.rotate_portrait.unwrap_or(false) && width < height;

    log::debug!(
        "Candidate dimensions from {}: {}x{}, aspect ratio {:.3}, rotate portrait {}",
        source,
        width,
        height,
        aspect_ratio,
        rotate_portrait
    );

    if !wallpaper_dimensions_match(width, height, config) {
        return Err(anyhow::anyhow!(
            "Wallpaper dimensions {}x{} (aspect ratio {:.3}) do not match the configured criteria: min {}x{}, aspect ratio [{:.3}, {:.3}], rotate_portrait={}",
            width,
            height,
            aspect_ratio,
            min_width,
            min_height,
            aspect_ratio_min,
            aspect_ratio_max,
            rotate_portrait
        ));
    }

    Ok(())
}

fn wallpaper_dimensions_match(width: u32, height: u32, config: &BWConfig) -> bool {
    if config.disable_resolution_filter.unwrap_or(false) {
        return true;
    }

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
    dimensions_match(
        width,
        height,
        min_width,
        min_height,
        aspect_ratio_min,
        aspect_ratio_max,
    ) || (config.rotate_portrait.unwrap_or(false)
        && width < height
        && rotated_portrait_dimensions_match(height, width, min_width, min_height))
}

fn dimensions_match(
    width: u32,
    height: u32,
    min_width: u32,
    min_height: u32,
    aspect_ratio_min: f32,
    aspect_ratio_max: f32,
) -> bool {
    let aspect_ratio = width as f32 / height as f32;

    width >= min_width
        && height >= min_height
        && aspect_ratio >= aspect_ratio_min
        && aspect_ratio <= aspect_ratio_max
}

fn rotated_portrait_dimensions_match(
    rotated_width: u32,
    rotated_height: u32,
    min_width: u32,
    min_height: u32,
) -> bool {
    rotated_width >= min_width && rotated_height >= min_height
}

fn should_rotate_portrait(metadata_dimensions: Option<(u32, u32)>, config: &BWConfig) -> bool {
    config.rotate_portrait.unwrap_or(false)
        && metadata_dimensions
            .map(|(width, height)| width < height)
            .unwrap_or(false)
}

fn animated_video_filter(config: &BWConfig, rotate_portrait: bool) -> String {
    let mut filters = Vec::new();

    if rotate_portrait {
        filters.push("transpose=1".to_string());
    }

    filters.push(format!(
        "fps={},scale={}:-2:flags=lanczos",
        config.animated_fps.unwrap_or(DEFAULT_ANIMATED_FPS),
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH)
    ));

    filters.join(",")
}

fn static_frame_filter(config: &BWConfig, rotate_portrait: bool) -> String {
    let mut filters = Vec::new();

    if rotate_portrait {
        filters.push("transpose=1".to_string());
    }

    filters.push(format!(
        "scale={}:-2:flags=lanczos",
        config.animated_width.unwrap_or(DEFAULT_ANIMATED_WIDTH)
    ));

    filters.join(",")
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
