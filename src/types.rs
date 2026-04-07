use std::path::PathBuf;

use booru_rs::{
    danbooru::DanbooruPost, gelbooru::GelbooruPost, rule34::Rule34Post, safebooru::SafebooruPost,
};
use directories::ProjectDirs;

pub const DEFAULT_IMAGEBOARD: BWImageboard = BWImageboard::Safebooru;
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;

pub fn get_default_config_path() -> PathBuf {
    let proj_dirs = ProjectDirs::from("uno", "lapis", "booru-wallpaper")
        .expect("Unable to get project dirs on your platform");

    let config_dir = proj_dirs.config_dir();
    let _ = std::fs::create_dir_all(config_dir);

    config_dir.join("config.toml").to_path_buf()
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum BWImageboard {
    Danbooru,
    Gelbooru,
    Rule34,
    Safebooru,
}

impl Default for BWImageboard {
    fn default() -> Self {
        Self::Safebooru
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum BWRating {
    Safe,
    Questionable,
    Explicit,
}

impl Default for BWRating {
    fn default() -> Self {
        Self::Safe
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum BWSortBy {
    Random,
    Id,
    Score,
    Rating,
    User,
    Height,
    Width,
    Source,
    Updated,
}

impl Default for BWSortBy {
    fn default() -> Self {
        Self::Random
    }
}

impl Into<booru_rs::Sort> for &BWSortBy {
    fn into(self) -> booru_rs::Sort {
        match self {
            BWSortBy::Random => booru_rs::Sort::Random,
            BWSortBy::Id => booru_rs::Sort::Id,
            BWSortBy::Score => booru_rs::Sort::Score,
            BWSortBy::Rating => booru_rs::Sort::Rating,
            BWSortBy::User => booru_rs::Sort::User,
            BWSortBy::Height => booru_rs::Sort::Height,
            BWSortBy::Width => booru_rs::Sort::Width,
            BWSortBy::Source => booru_rs::Sort::Source,
            BWSortBy::Updated => booru_rs::Sort::Updated,
        }
    }
}

pub trait TryGetUrl {
    fn try_get_url(&self) -> anyhow::Result<&str>;
}

impl TryGetUrl for DanbooruPost {
    fn try_get_url(&self) -> anyhow::Result<&str> {
        if let Some(url) = &self.large_file_url {
            return Ok(url);
        }

        if let Some(url) = &self.file_url {
            return Ok(url);
        }

        if let Some(url) = &self.preview_file_url {
            return Ok(url);
        }

        Err(anyhow::anyhow!("No file URL found"))
    }
}

impl TryGetUrl for GelbooruPost {
    fn try_get_url(&self) -> anyhow::Result<&str> {
        Ok(&self.file_url)
    }
}

impl TryGetUrl for Rule34Post {
    fn try_get_url(&self) -> anyhow::Result<&str> {
        Ok(&self.file_url)
    }
}

impl TryGetUrl for SafebooruPost {
    fn try_get_url(&self) -> anyhow::Result<&str> {
        Ok(&self.file_url)
    }
}
