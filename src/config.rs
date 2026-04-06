use crate::{
    cli::CliArgs,
    types::{BWImageboard, BWRating, BWSortBy},
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
            disable_resolution_filter: Some(args.disable_resolution_filter),
        }
    }
}
