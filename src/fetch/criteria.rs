use std::{path::Path, process::Command};

use crate::config::BWConfig;

use super::{
    DEFAULT_WALLPAPER_ASPECT_RATIO_MAX, DEFAULT_WALLPAPER_ASPECT_RATIO_MIN,
    DEFAULT_WALLPAPER_MIN_HEIGHT, DEFAULT_WALLPAPER_MIN_WIDTH,
};

pub fn validate_wallpaper_candidate(
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
            "Wallpaper dimensions {}x{} (aspect ratio {:.3}) do not match the configured criteria",
            width,
            height,
            aspect_ratio
        ));
    }

    Ok(())
}

pub fn wallpaper_dimensions_match(width: u32, height: u32, config: &BWConfig) -> bool {
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

pub fn should_rotate_portrait(metadata_dimensions: Option<(u32, u32)>, config: &BWConfig) -> bool {
    config.rotate_portrait.unwrap_or(false)
        && metadata_dimensions
            .map(|(width, height)| width < height)
            .unwrap_or(false)
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
