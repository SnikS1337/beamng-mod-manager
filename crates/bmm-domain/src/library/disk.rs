use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskMod {
    pub file_name: String,
    pub enabled: bool,
    pub size_bytes: i64,
    pub sub_folder: Option<String>,
    pub modified_utc: Option<DateTime<Utc>>,
}

impl DiskMod {
    pub fn from_game_repo(&self) -> bool {
        self.sub_folder.is_some()
    }
}
