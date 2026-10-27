use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<bmm_domain::paths::PathsError> for AppError {
    fn from(error: bmm_domain::paths::PathsError) -> Self {
        Self::new("paths", error.to_string())
    }
}
