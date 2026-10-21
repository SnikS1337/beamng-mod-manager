use chrono::{DateTime, FixedOffset};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriceStatus {
    #[default]
    Unknown,
    Free,
    Paid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferKind {
    Direct,
    ExternalHost,
    PaidLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub kind: OfferKind,
    pub host: String,
    pub url: Url,
    pub host_file_id: Option<String>,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub source: String,
    pub url: Url,
    pub source_id: String,
    pub title: String,
    pub mod_version: Option<String>,
    pub game_version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub size_bytes: Option<i64>,
    pub preview_url: Option<Url>,
    pub preview_hash: Option<u64>,
    pub price: PriceStatus,
    pub offers: Vec<Offer>,
    pub published_at: Option<DateTime<FixedOffset>>,
    pub updated_at: Option<DateTime<FixedOffset>>,
    pub external_id: Option<String>,
    pub first_seen: Option<DateTime<FixedOffset>>,
    pub last_seen: Option<DateTime<FixedOffset>>,
}

impl Listing {
    pub fn new(
        source: impl Into<String>,
        url: Url,
        source_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            url,
            source_id: source_id.into(),
            title: title.into(),
            mod_version: None,
            game_version: None,
            author: None,
            category: None,
            size_bytes: None,
            preview_url: None,
            preview_hash: None,
            price: PriceStatus::Unknown,
            offers: Vec::new(),
            published_at: None,
            updated_at: None,
            external_id: None,
            first_seen: None,
            last_seen: None,
        }
    }
}
