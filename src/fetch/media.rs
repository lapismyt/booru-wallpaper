use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{config::BWConfig, types::BWWallpaperSetter};

use super::{DEFAULT_ANIMATED_FPS, DEFAULT_ANIMATED_MAX_DURATION_SECONDS, DEFAULT_ANIMATED_WIDTH};

pub fn is_supported_content_type(wallpaper_setter: &BWWallpaperSetter, content_type: &str) -> bool {
    if content_type.starts_with("image/") {
        return true;
    }

    matches!(
        wallpaper_setter,
        BWWallpaperSetter::Wallpaper | BWWallpaperSetter::Awww
    ) && matches!(content_type, "video/mp4" | "video/webm")
}

pub fn prepare_wallpaper_path(
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

pub fn set_wallpaper(wallpaper_setter: &BWWallpaperSetter, path: &str) -> anyhow::Result<()> {
    log::debug!(
        "Setting wallpaper via {:?} from: {}",
        wallpaper_setter,
        path
    );

    match wallpaper_setter {
        BWWallpaperSetter::Wallpaper => wallpaper::set_from_path(path)
            .map_err(|e| anyhow::anyhow!("Failed to set wallpaper (path: {}): {}", path, e)),
        BWWallpaperSetter::Awww => run_command(
            Command::new("awww").args(["img", path]),
            format!("awww img {}", path),
        ),
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
    let output_path = with_extension(path, "gif");
    let video_filter = animated_video_filter(config, rotate_portrait);
    let max_duration = config
        .animated_max_duration_seconds
        .unwrap_or(DEFAULT_ANIMATED_MAX_DURATION_SECONDS)
        .to_string();

    log::debug!(
        "Converting video for awww: {} -> {}",
        path.display(),
        output_path.display()
    );
    run_command(
        Command::new("ffmpeg")
            .args(["-y", "-t"])
            .arg(&max_duration)
            .args(["-i"])
            .arg(path)
            .args(["-an", "-vf"])
            .arg(&video_filter)
            .args(["-loop", "0"])
            .arg(&output_path),
        format!("ffmpeg convert {}", path.display()),
    )?;

    path_to_string(&output_path)
}

fn extract_static_frame(
    path: &Path,
    config: &BWConfig,
    rotate_portrait: bool,
) -> anyhow::Result<String> {
    let output_path = with_extension(path, "png");
    let frame_filter = static_frame_filter(config, rotate_portrait);

    log::debug!(
        "Extracting static frame: {} -> {}",
        path.display(),
        output_path.display()
    );
    run_command(
        Command::new("ffmpeg")
            .args(["-y", "-i"])
            .arg(path)
            .args(["-frames:v", "1", "-vf"])
            .arg(&frame_filter)
            .arg(&output_path),
        format!("ffmpeg frame {}", path.display()),
    )?;

    path_to_string(&output_path)
}

fn rotate_image_clockwise(path: &Path) -> anyhow::Result<String> {
    let output_path = with_suffix(path, "rotated.png");
    let image = image::open(path)
        .map_err(|e| anyhow::anyhow!("Failed to open image {}: {}", path.display(), e))?;
    image.rotate90().save(&output_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to save rotated image {}: {}",
            output_path.display(),
            e
        )
    })?;
    path_to_string(&output_path)
}

fn rotate_gif_clockwise(path: &Path) -> anyhow::Result<String> {
    let output_path = with_suffix(path, "rotated.gif");
    run_command(
        Command::new("ffmpeg")
            .args(["-y", "-i"])
            .arg(path)
            .args(["-vf", "transpose=1"])
            .arg(&output_path),
        format!("ffmpeg rotate {}", path.display()),
    )?;
    path_to_string(&output_path)
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

fn run_command(command: &mut Command, context: String) -> anyhow::Result<()> {
    let output = command
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute {}: {}", context, e))?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(anyhow::anyhow!(
        "{} failed with status {}. stdout: {} stderr: {}",
        context,
        output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string()),
        stdout,
        stderr
    ))
}

fn with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut output_path = path.to_path_buf();
    output_path.set_extension(extension);
    output_path
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("wallpaper");
    path.with_file_name(format!("{}.{}", stem, suffix))
}

fn path_to_string(path: &Path) -> anyhow::Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Failed to convert path to string: {}", path.display()))
}
