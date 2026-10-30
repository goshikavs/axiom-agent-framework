use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use uuid::Uuid;

pub struct OtlpLimiter {
    counts: DashMap<(Uuid, u64), u32>,
    per_minute: u32,
    hits: AtomicU64,
}

impl OtlpLimiter {
    pub fn new(per_minute: u32) -> Self {
        Self { counts: DashMap::new(), per_minute, hits: AtomicU64::new(0) }
    }

    pub fn allow(&self, project_id: Uuid) -> bool {
        if self.per_minute == 0 {
            return true;
        }
        let minute = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 60).unwrap_or(0);
        let mut entry = self.counts.entry((project_id, minute)).or_insert(0);
        if *entry >= self.per_minute {
            return false;
        }
        *entry += 1;
        self.hits.fetch_add(1, Ordering::Relaxed);
        true
    }
}
