use chrono::{DateTime, FixedOffset, Utc};
use std::sync::Arc;

pub type Clock = Arc<dyn Fn() -> DateTime<FixedOffset> + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| Utc::now().fixed_offset())
}
