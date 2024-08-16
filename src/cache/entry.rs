use std::time::{Duration, Instant};

pub struct CacheEntry<V> {
    pub value: V,
    pub ttl: Duration,
    pub last_accessed: Instant,
}

impl<V> CacheEntry<V> {
    pub fn new(value: V, ttl: Duration) -> Self {
        Self {
            value,
            ttl,
            last_accessed: Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.last_accessed.elapsed() > self.ttl
    }

    pub fn update_access_time(&mut self) {
        self.last_accessed = Instant::now();
    }
}


