use anyhow::Ok;
use booru_rs::{
    Client, DanbooruClient, GelbooruClient, Rule34Client, SafebooruClient, client::ClientBuilder,
};
use std::io::Write;
use tempdir::TempDir;

use crate::{
    config::BWConfig,
    rating::BWRatingToBooruRating,
    types::{BWImageboard, BWSortBy, DEFAULT_IMAGEBOARD, HEIGHT, TryGetUrl, WIDTH},
};

pub async fn fetch_and_set_wallpaper(config: &BWConfig, dry_run: bool) -> anyhow::Result<()> {
    let img_url = match config.imageboard.as_ref().unwrap_or(&DEFAULT_IMAGEBOARD) {
        BWImageboard::Gelbooru => fetch_wallpaper::<GelbooruClient>(config).await?,
        BWImageboard::Rule34 => fetch_wallpaper::<Rule34Client>(config).await?,
        BWImageboard::Safebooru => fetch_wallpaper::<SafebooruClient>(config).await?,
        BWImageboard::Danbooru => fetch_wallpaper::<DanbooruClient>(config).await?,
    };

    if dry_run {
        println!("{}", img_url);
        Ok(())
    } else {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()?;

        let referer = match config.imageboard.as_ref().unwrap_or(&DEFAULT_IMAGEBOARD) {
            BWImageboard::Gelbooru => "https://gelbooru.com/",
            BWImageboard::Rule34 => "https://rule34.xxx/",
            BWImageboard::Safebooru => "https://safebooru.org/",
            BWImageboard::Danbooru => "https://danbooru.donmai.us/",
        };

        log::debug!("Downloading wallpaper from: {}", img_url);

        let response = client
            .get(&img_url)
            .header("Referer", referer)
            .send()
            .await?
            .error_for_status()?;

        if let Some(content_type) = response.headers().get("content-type") {
            log::debug!("Content-Type: {:?}", content_type);
            let content_type_str = content_type.to_str().unwrap_or("");
            if !content_type_str.starts_with("image/") {
                return Err(anyhow::anyhow!(
                    "Expected image content type, got: {}",
                    content_type_str
                ));
            }
        }

        let bytes = response.bytes().await?;

        let tmp_dir = TempDir::new("booru-wallpaper")?;
        let file_name = img_url
            .split('/')
            .last()
            .filter(|s| !s.is_empty())
            .map(|s| s.split('?').next().unwrap_or(s))
            .filter(|s| !s.is_empty())
            .unwrap_or("wallpaper.png");
        let file_path = tmp_dir.into_path().join(file_name);

        log::debug!("Saving wallpaper to: {:?}", file_path);

        {
            let mut file = std::fs::File::create(&file_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }

        let path_str = file_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert path to string"))?
            .to_string();

        tokio::task::spawn_blocking(move || {
            log::debug!("Setting wallpaper from: {}", path_str);
            wallpaper::set_from_path(&path_str)
                .map_err(|e| anyhow::anyhow!("Failed to set wallpaper (path: {}): {}", path_str, e))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))??;

        Ok(())
    }
}

async fn fetch_wallpaper<C>(config: &BWConfig) -> anyhow::Result<String>
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

    let mut blacklisted = config.blacklist_tags.clone().unwrap_or_default();

    if let Some(imageboard) = &config.imageboard {
        if imageboard != &BWImageboard::Danbooru {
            let disable_resolution_filter = config.disable_resolution_filter.unwrap_or(false);

            if !disable_resolution_filter {
                tags.extend_from_slice(&[
                    format!("width:={}", WIDTH),
                    format!("height:={}", HEIGHT),
                ]);
            }

            blacklisted.push("animated".to_string())
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

    let res = builder
        .limit(1)
        .build()
        .get()
        .await?
        .into_iter()
        .next()
        .ok_or(anyhow::anyhow!("Unable to find a wallpaper"))?;

    Ok(res.try_get_url()?.to_string())
}

fn apply_sorting<C: Client>(sort_by: &BWSortBy, builder: ClientBuilder<C>) -> ClientBuilder<C> {
    builder.sort(sort_by.into())
}
