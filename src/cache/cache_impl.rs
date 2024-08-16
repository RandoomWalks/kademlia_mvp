use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::entry::CacheEntry;
use super::policy::CachePolicy;

use std::fmt;

// Define custom errors for cache operations
#[derive(Debug)]
pub enum CacheError {
    KeyNotFound,
    InvalidValue,
    Expired,
    CacheFull,
    PolicyError(String),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::KeyNotFound => write!(f, "Key not found in cache"),
            CacheError::InvalidValue => write!(f, "Value is invalid in cache"),
            CacheError::Expired => write!(f, "Key has expired in cache"),
            CacheError::CacheFull => write!(f, "Cache is full, unable to insert new entry"),
            CacheError::PolicyError(msg) => write!(f, "Cache policy error: {}", msg),
        }
    }
}

impl std::error::Error for CacheError {}

// Represents a thread-safe caching mechanism using read-write locks for concurrency control.
pub struct Cache<K, V>
where
    K: Eq + Hash + Clone + 'static + Send + Sync,
{
    // The actual store of cached entries, protected by a read-write lock.
    // This allows multiple readers or one writer at a time.
    cache_store: RwLock<HashMap<K, CacheEntry<V>>>,

    // The eviction policy for cache entries, stored as a boxed trait object.
    // Access to the policy is protected by a read-write lock.
    policy: Arc<RwLock<Box<dyn CachePolicy<K>>>>,

    // Metrics tracking cache hits, misses, and evictions, protected by a read-write lock.
    metrics: Arc<RwLock<CacheMetrics>>,

    capacity: usize, // New field to store the maximum capacity of the cache
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone + 'static + Send + Sync,
    V: Clone + 'static + Send + Sync,
{
    // Constructs a new cache instance with a given eviction policy.
    // Example:
    // let lru_policy = Box::new(LRUCachePolicy::new());
    // let cache = Cache::new(lru_policy);
    // In this example, `LRUCachePolicy` would define how entries are evicted based on least recently used criteria.
    pub fn new(policy: Box<dyn CachePolicy<K>>, capacity: usize) -> Self {
        Self {
            cache_store: RwLock::new(HashMap::new()),
            policy: Arc::new(RwLock::new(policy)),
            metrics: Arc::new(RwLock::new(CacheMetrics::new())),
            capacity, // Initialize the capacity,
        }
    }

    // Constructs a cache instance with a specific eviction policy type.
    // Example:
    // let policy = EvictionPolicy::LRU; // Assume this defines an LRU policy.
    // let cache = Cache::with_policy(policy); // Uses the policy to build a cache.
    pub fn with_policy(policy: super::policy::EvictionPolicy, capacity: usize) -> Self {
        Self::new(policy.build(), capacity)
    }

    // Adds or updates a value in the cache with a specified TTL (time-to-live).
    // Example:
    // let ttl = Duration::from_secs(300); // TTL of 5 minutes.
    // cache.put("user123", "data", ttl).await;
    // This will insert or update the cache entry with key "user123" and value "data".
    pub async fn put(&self, key: K, value: V, ttl: Duration) -> Result<(), CacheError> {
        let mut store = self.cache_store.write().await;

        if store.len() >= self.capacity {
            let evicted_key = self.evict().await.ok_or(CacheError::CacheFull)?;
            store.remove(&evicted_key);
        }
    
        let entry = CacheEntry::new(value, ttl);
        store.insert(key.clone(), entry);
        self.policy.write().await.on_insert(&key);
        Ok(())
    }

    // Retrieves a value from the cache if it exists and is not expired.
    // Example:
    // let value = cache.get(&"user123").await;
    // If the cache contains "user123" and it is not expired, `value` will be `Some("data")`.
    // If the key is not found or expired, `value` will be `None`.
    pub async fn get(&self, key: &K) -> Result<V, CacheError> {
        let mut store = self.cache_store.write().await;
        if let Some(entry) = store.get_mut(key) {
            if entry.is_expired() {
                store.remove(key);
                self.policy.write().await.remove(key); // Remove from the policy tracking
                self.metrics.write().await.record_miss();
                return Err(CacheError::Expired);
            } else {
                entry.update_access_time();
                self.policy.write().await.on_access(key);
                self.metrics.write().await.record_hit();
                return Ok(entry.value.clone());
            }
        } else {
            self.metrics.write().await.record_miss();
            Err(CacheError::KeyNotFound)
        }
    }

    // Evicts an entry from the cache based on the eviction policy.
    // Example:
    // let evicted_key = cache.evict().await;
    // If the eviction policy determines that a key should be evicted, `evicted_key` will contain the key.
    // Otherwise, it will be `None`.
    pub async fn evict(&self) -> Option<K> {
        let mut store = self.cache_store.write().await;
        if let Some(key) = self.policy.write().await.evict() {
            store.remove(&key);
            self.metrics.write().await.record_eviction();
            Some(key)
        } else {
            None
        }
    }

    // Updated method to check if the cache is full
    async fn is_full(&self) -> bool {
        self.cache_store.read().await.len() >= self.capacity
    }

    // Retrieves a string report of current cache metrics (hits, misses, evictions).
    // Example:
    // let metrics_report = cache.report_metrics().await;
    // This might return a string like "Hits: 10, Misses: 3, Evictions: 5".
    pub async fn report_metrics(&self) -> String {
        self.metrics.read().await.report()
    }
}

// Represents metrics tracking for cache operations.
pub struct CacheMetrics {
    hits: usize,
    misses: usize,
    evictions: usize,
}

impl CacheMetrics {
    // Initializes a new cache metrics instance with all counts set to zero.
    // Example:
    // let metrics = CacheMetrics::new(); // Creates a new `CacheMetrics` with hits, misses, and evictions all set to 0.
    pub fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    // Records a cache hit, incrementing the hits count.
    // Example:
    // metrics.record_hit(); // Increments the hits count by 1.
    pub fn record_hit(&mut self) {
        self.hits += 1;
    }

    // Records a cache miss, incrementing the misses count.
    // Example:
    // metrics.record_miss(); // Increments the misses count by 1.
    pub fn record_miss(&mut self) {
        self.misses += 1;
    }

    // Records a cache eviction, incrementing the evictions count.
    // Example:
    // metrics.record_eviction(); // Increments the evictions count by 1.
    pub fn record_eviction(&mut self) {
        self.evictions += 1;
    }

    // Generates a report of cache metrics.
    // Example:
    // let report = metrics.report(); // Returns a string like "Hits: 10, Misses: 5, Evictions: 2".
    pub fn report(&self) -> String {
        format!(
            "Hits: {}, Misses: {}, Evictions: {}",
            self.hits, self.misses, self.evictions
        )
    }
}
