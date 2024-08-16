use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::time::Duration;

// Trait defining the interface for cache policies.
pub trait CachePolicy<K: 'static>: Send + Sync {
    // Called when a new key-value pair is inserted into the cache.
    // Example:
    // policy.on_insert(&"key1");
    fn on_insert(&mut self, key: &K);

    // Called when an existing key-value pair is accessed.
    // Example:
    // policy.on_access(&"key1");
    fn on_access(&mut self, key: &K);

    // Determines which key should be evicted based on the policy.
    // Example:
    // let evicted_key = policy.evict(); // Returns Some("key1") if a key is evicted, otherwise None.
    fn evict(&mut self) -> Option<K>;

    // Removes a key from the policy tracking.
    // Example:
    // policy.remove(&"key1");
    fn remove(&mut self, key: &K);
}

// LRU (Least Recently Used) cache policy implementation.
pub struct LRUCachePolicy<K> {
    // Keeps track of the order of key usage (most recent at the back).
    usage_order: VecDeque<K>,
    // Maps keys to their positions in the `usage_order` queue.
    positions: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone> LRUCachePolicy<K> {
    // Creates a new LRUCachePolicy with empty usage tracking.
    // Example:
    // let lru_policy = LRUCachePolicy::new(); // Initializes a new LRU cache policy.
    pub fn new() -> Self {
        Self {
            usage_order: VecDeque::new(),
            positions: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone + 'static + Send + Sync> CachePolicy<K> for LRUCachePolicy<K> {
    // Handles the insertion of a key into the cache.
    // Adds the key to the end of the `usage_order` and updates its position.
    // Example:
    // lru_policy.on_insert(&"key1"); // Adds "key1" to the cache policy.
    fn on_insert(&mut self, key: &K) {
        if !self.positions.contains_key(key) {
            self.usage_order.push_back(key.clone());
            self.positions
                .insert(key.clone(), self.usage_order.len() - 1);
        }
    }

    // Handles the access of a key.
    // Moves the key to the end of the `usage_order` to mark it as most recently used.
    // Example:
    // lru_policy.on_access(&"key1"); // Updates the position of "key1" to the most recent.
    fn on_access(&mut self, key: &K) {
        if let Some(&pos) = self.positions.get(key) {
            self.usage_order.remove(pos); // Remove the key from the old position
            self.usage_order.push_back(key.clone()); // Add it to the back (most recently used)
            self.positions
                .insert(key.clone(), self.usage_order.len() - 1);
        }
    }

    // Evicts the least recently used key.
    // Removes the key from both `usage_order` and `positions`.
    // Example:
    // let evicted_key = lru_policy.evict(); // Might return Some("key1") if it was the least recently used.
    fn evict(&mut self) -> Option<K> {
        if let Some(least_used) = self.usage_order.pop_front() {
            self.positions.remove(&least_used);
            return Some(least_used);
        }
        None
    }

    // Removes a key from the policy tracking.
    // Example:
    // lru_policy.remove(&"key1"); // Removes "key1" from the cache policy.
    fn remove(&mut self, key: &K) {
        if let Some(&pos) = self.positions.get(key) {
            self.usage_order.remove(pos); // Remove from the usage order
            self.positions.remove(key); // Remove from the positions map
        }
    }
}

pub struct CacheConfig {
    pub eviction_policy: EvictionPolicy,
    pub ttl: Duration,
    pub max_size: usize,
    pub maintenance_interval: Duration,
}

pub enum EvictionPolicy {
    LRU,
    LFU,
    FIFO,
}

impl Default for CacheConfig {
    fn default() -> Self {
        CacheConfig {
            eviction_policy: EvictionPolicy::LRU,
            ttl: Duration::from_secs(3600),
            max_size: 10000,
            maintenance_interval: Duration::from_secs(3600),
        }
    }
}

impl EvictionPolicy {
    // Constructs a cache policy based on the specified eviction policy type.
    // Example:
    // let policy = EvictionPolicy::LRU.build::<String>(); // Builds an LRU cache policy for String keys.
    pub fn build<K: Eq + Hash + Clone + 'static  + Send + Sync>(&self) -> Box<dyn CachePolicy<K>> {
        match self {
            EvictionPolicy::LRU => Box::new(LRUCachePolicy::new()),
            _ => Box::new(LRUCachePolicy::new()),
        }
    }
}

// Example Usage:

// Creating a new LRU cache policy
// let lru_policy = LRUCachePolicy::new(); // Initializes a new LRU cache policy.

// Handling key insertion
// lru_policy.on_insert(&"key1"); // Adds "key1" to the LRU policy.

// Handling key access
// lru_policy.on_access(&"key1"); // Updates the position of "key1" to most recently used.

// Evicting a key
// let evicted_key = lru_policy.evict(); // Evicts the least recently used key and returns it.

// Removing a key
// lru_policy.remove(&"key1"); // Removes "key1" from the LRU policy.

// Building a cache policy from an eviction policy type
// let eviction_policy = EvictionPolicy::LRU;
// let policy = eviction_policy.build::<String>(); // Constructs an LRU policy for String keys.
