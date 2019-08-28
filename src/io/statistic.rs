use std::cmp::max;

use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
pub struct Statistic {
    pub hit_count: AtomicUsize,
    pub hit_bytes: AtomicUsize,
    pub miss_count: AtomicUsize,
    pub miss_bytes: AtomicUsize,
    pub remote_count: AtomicUsize,
}

impl Statistic {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add_hit(&self, bytes: usize) {
        self.hit_count.fetch_add(1, Ordering::Release);
        self.hit_bytes.fetch_add(bytes, Ordering::Release);
    }

    pub fn add_miss(&self, bytes: usize) {
        self.miss_count.fetch_add(1, Ordering::Release);
        self.miss_bytes.fetch_add(bytes, Ordering::Release);
    }

    pub fn inc_remote(&self) {
        self.remote_count.fetch_add(1, Ordering::Release);
    }

    pub fn to_string(&self) -> String {
        let hit_count = self.hit_count.load(Ordering::Relaxed);
        let hit_bytes = self.hit_bytes.load(Ordering::Relaxed);
        let miss_count = self.miss_count.load(Ordering::Relaxed);
        let miss_bytes = self.miss_bytes.load(Ordering::Relaxed);
        let remote_count = self.remote_count.load(Ordering::Relaxed);
        let total_count = hit_count + miss_count;
        format!(
            "Cache statistic: hit {} of {} ({} %), remote {}, read {}, write {}, total {}",
            hit_count,
            total_count,
            hit_count * 100 / max(total_count, 1),
            remote_count,
            hit_bytes,
            miss_bytes,
            hit_bytes + miss_bytes,
        )
    }
}
