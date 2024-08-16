use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::entry::CacheEntry;
use super::policy::CachePolicy;

// 2. Send Trait Bound
// The Send trait indicates that a type is safe to send (transfer ownership) between threads.
// Implications:

// Ensures thread safety for types that are moved between threads

// When to use:

// When working with multi-threaded code
// When using types in conjunction with Arc, Mutex, or RwLock

pub struct Cache<K,V>
where
    K: Eq + Hash + Clone + 'static + Send + Sync,
{
    store: RwLock<HashMap<K, CacheEntry<V>>>,
    policy: Arc<RwLock<Box<dyn CachePolicy<K>>>>,
    metrics: Arc<RwLock<CacheMetrics>>,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone + 'static + Send + Sync,
    V: Clone + 'static + Send + Sync,
{
    pub fn new(policy: Box<dyn CachePolicy<K>>) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            policy: Arc::new(RwLock::new(policy)),
            metrics: Arc::new(RwLock::new(CacheMetrics::new())),
        }
    }

    pub fn with_policy(policy: super::policy::EvictionPolicy) -> Self {
        Self::new(policy.build())
    }

    pub async fn put(&self, key: K, value: V, ttl: Duration) {
        let entry = CacheEntry::new(value, ttl);
        self.store.write().await.insert(key.clone(), entry);
        self.policy.write().await.on_insert(&key);
    }

    pub async fn get(&self, key: &K) -> Option<V> {
        let mut store = self.store.write().await;
        if let Some(entry) = store.get_mut(key) {
            if entry.is_expired() {
                store.remove(key);
                self.policy.write().await.remove(key); // Remove from the policy tracking
                self.metrics.write().await.record_miss();
                None
            } else {
                entry.update_access_time();
                self.policy.write().await.on_access(key);
                self.metrics.write().await.record_hit();
                Some(entry.value.clone())
            }
        } else {
            self.metrics.write().await.record_miss();
            None
        }
    }

    pub async fn evict(&self) -> Option<K> {
        let mut store = self.store.write().await;
        if let Some(key) = self.policy.write().await.evict() {
            store.remove(&key);
            self.metrics.write().await.record_eviction();
            return Some(key);
        }
        None
    }

    pub async fn report_metrics(&self) -> String {
        self.metrics.read().await.report()
    }
}

pub struct CacheMetrics {
    hits: usize,
    misses: usize,
    evictions: usize,
}

impl CacheMetrics {
    pub fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub fn record_hit(&mut self) {
        self.hits += 1;
    }

    pub fn record_miss(&mut self) {
        self.misses += 1;
    }

    pub fn record_eviction(&mut self) {
        self.evictions += 1;
    }

    pub fn report(&self) -> String {
        format!(
            "Hits: {}, Misses: {}, Evictions: {}",
            self.hits, self.misses, self.evictions
        )
    }
}


