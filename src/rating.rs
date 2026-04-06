use booru_rs::{Client, DanbooruClient, GelbooruClient, Rule34Client, SafebooruClient};

use crate::types::BWRating;

pub trait BWRatingToBooruRating: Client {
    fn rating_from_bw(rating: &BWRating) -> Self::Rating;
}

impl BWRatingToBooruRating for DanbooruClient {
    fn rating_from_bw(rating: &BWRating) -> Self::Rating {
        match rating {
            BWRating::Safe => Self::Rating::General,
            BWRating::Questionable => Self::Rating::Questionable,
            BWRating::Explicit => Self::Rating::Explicit,
        }
    }
}

impl BWRatingToBooruRating for GelbooruClient {
    fn rating_from_bw(rating: &BWRating) -> Self::Rating {
        match rating {
            BWRating::Safe => Self::Rating::Safe,
            BWRating::Questionable => Self::Rating::Questionable,
            BWRating::Explicit => Self::Rating::Explicit,
        }
    }
}

impl BWRatingToBooruRating for SafebooruClient {
    fn rating_from_bw(rating: &BWRating) -> Self::Rating {
        match rating {
            BWRating::Safe => Self::Rating::Safe,
            BWRating::Questionable => Self::Rating::Questionable,
            BWRating::Explicit => Self::Rating::Explicit,
        }
    }
}

impl BWRatingToBooruRating for Rule34Client {
    fn rating_from_bw(rating: &BWRating) -> Self::Rating {
        match rating {
            BWRating::Safe => Self::Rating::Safe,
            BWRating::Questionable => Self::Rating::Questionable,
            BWRating::Explicit => Self::Rating::Explicit,
        }
    }
}
