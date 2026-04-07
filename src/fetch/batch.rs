use booru_rs::{Client, client::ClientBuilder};

use crate::{
    config::BWConfig,
    rating::BWRatingToBooruRating,
    types::{BWImageboard, BWSortBy, TryGetDimensions, TryGetUrl, WallpaperCandidate},
};

use super::{
    DEFAULT_BATCH_SIZE, DEFAULT_WALLPAPER_MIN_HEIGHT, DEFAULT_WALLPAPER_MIN_WIDTH,
    criteria::wallpaper_dimensions_match,
};

pub async fn fetch_wallpapers<C>(config: &BWConfig) -> anyhow::Result<Vec<WallpaperCandidate>>
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

    apply_resolution_prefilter(config, &mut tags);
    if let Some(min_score) = config.min_score {
        tags.push(format!("score:>={}", min_score));
    }

    log::debug!("tags: {:?}", tags);

    for tag in tags {
        builder = builder.tag(tag)?;
    }
    for tag in config.blacklist_tags.clone().unwrap_or_default() {
        builder = builder.blacklist_tag(tag);
    }

    builder = apply_sorting(config.sort_by.as_ref(), builder);
    if let Some(rating) = &config.rating {
        builder = builder.rating(C::rating_from_bw(rating));
    }

    let batch_size = config.batch_size.unwrap_or(DEFAULT_BATCH_SIZE);
    let posts = builder.limit(batch_size).build().get().await?;
    let candidates = posts
        .into_iter()
        .filter_map(|post| match (post.try_get_url(), post.try_get_dimensions()) {
            (Ok(url), Ok((width, height))) if wallpaper_dimensions_match(width, height, config) => {
                Some(WallpaperCandidate {
                    url: url.to_string(),
                    width: Some(width),
                    height: Some(height),
                })
            }
            (Ok(_), Ok((width, height))) => {
                log::debug!(
                    "Skipping post by metadata dimensions: {}x{} does not match wallpaper criteria",
                    width,
                    height
                );
                None
            }
            (Ok(url), Err(error)) => {
                log::debug!("Using post without metadata dimensions: {}", error);
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

fn apply_resolution_prefilter(config: &BWConfig, tags: &mut Vec<String>) {
    let imageboard = config
        .imageboard
        .as_ref()
        .unwrap_or(&BWImageboard::Safebooru);
    if *imageboard == BWImageboard::Danbooru || config.disable_resolution_filter.unwrap_or(false) {
        return;
    }

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

fn apply_sorting<C: Client>(
    sort_by: Option<&BWSortBy>,
    builder: ClientBuilder<C>,
) -> ClientBuilder<C> {
    match sort_by {
        Some(sort_by) => builder.sort(sort_by.into()),
        None => builder.random(),
    }
}
