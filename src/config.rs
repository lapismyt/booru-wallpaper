use crate::{
    cli::CliArgs,
    types::{BWImageboard, BWRating, BWSortBy, BWWallpaperSetter},
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BWConfig {
    pub tags: Option<Vec<String>>,
    pub blacklist_tags: Option<Vec<String>>,
    pub rating: Option<BWRating>,
    pub imageboard: Option<BWImageboard>,
    pub min_score: Option<u32>,
    pub cycle_interval_seconds: Option<u64>,
    pub api_key: Option<String>,
    pub user_id: Option<String>,
    pub sort_by: Option<BWSortBy>,
    pub disable_resolution_filter: Option<bool>,
    pub wallpaper_setter: Option<BWWallpaperSetter>,
    pub max_retries: Option<u32>,
    pub retry_interval_seconds: Option<u64>,
    pub batch_size: Option<u32>,
    pub wallpaper_min_width: Option<u32>,
    pub wallpaper_min_height: Option<u32>,
    pub wallpaper_aspect_ratio_min: Option<f32>,
    pub wallpaper_aspect_ratio_max: Option<f32>,
    pub animated_max_duration_seconds: Option<u32>,
    pub animated_fps: Option<u32>,
    pub animated_width: Option<u32>,
}

impl Default for BWConfig {
    fn default() -> Self {
        Self {
            tags: Default::default(),
            rating: Default::default(),
            imageboard: Default::default(),
            min_score: Default::default(),
            blacklist_tags: Default::default(),
            cycle_interval_seconds: Default::default(),
            api_key: Default::default(),
            user_id: Default::default(),
            sort_by: Default::default(),
            disable_resolution_filter: Default::default(),
            wallpaper_setter: Default::default(),
            max_retries: Default::default(),
            retry_interval_seconds: Default::default(),
            batch_size: Default::default(),
            wallpaper_min_width: Default::default(),
            wallpaper_min_height: Default::default(),
            wallpaper_aspect_ratio_min: Default::default(),
            wallpaper_aspect_ratio_max: Default::default(),
            animated_max_duration_seconds: Default::default(),
            animated_fps: Default::default(),
            animated_width: Default::default(),
        }
    }
}

impl BWConfig {
    pub fn with_cli_args(&self, args: &CliArgs) -> Self {
        Self {
            tags: match &args.tags {
                Some(tags) => Some(
                    tags.clone()
                        .split(" ")
                        .map(String::from)
                        .collect::<Vec<_>>(),
                ),
                None => self.tags.clone(),
            },
            blacklist_tags: match &args.blacklist_tags {
                Some(blacklist_tags) => Some(
                    blacklist_tags
                        .clone()
                        .split(" ")
                        .map(String::from)
                        .collect::<Vec<_>>(),
                ),
                None => self.blacklist_tags.clone(),
            },
            rating: args.rating.clone().or(self.rating.clone()),
            imageboard: args.imageboard.clone().or(self.imageboard.clone()),
            min_score: args.min_score.or(self.min_score),
            cycle_interval_seconds: args.cycle_interval_seconds.or(self.cycle_interval_seconds),
            api_key: args.api_key.clone().or(self.api_key.clone()),
            user_id: args.user_id.clone().or(self.user_id.clone()),
            sort_by: args.sort_by.clone().or(self.sort_by.clone()),
            disable_resolution_filter: if args.disable_resolution_filter {
                Some(true)
            } else {
                self.disable_resolution_filter
            },
            wallpaper_setter: args
                .wallpaper_setter
                .clone()
                .or(self.wallpaper_setter.clone()),
            max_retries: args.max_retries.or(self.max_retries),
            retry_interval_seconds: args.retry_interval_seconds.or(self.retry_interval_seconds),
            batch_size: args.batch_size.or(self.batch_size),
            wallpaper_min_width: args.wallpaper_min_width.or(self.wallpaper_min_width),
            wallpaper_min_height: args.wallpaper_min_height.or(self.wallpaper_min_height),
            wallpaper_aspect_ratio_min: args
                .wallpaper_aspect_ratio_min
                .or(self.wallpaper_aspect_ratio_min),
            wallpaper_aspect_ratio_max: args
                .wallpaper_aspect_ratio_max
                .or(self.wallpaper_aspect_ratio_max),
            animated_max_duration_seconds: args
                .animated_max_duration_seconds
                .or(self.animated_max_duration_seconds),
            animated_fps: args.animated_fps.or(self.animated_fps),
            animated_width: args.animated_width.or(self.animated_width),
        }
    }
}
