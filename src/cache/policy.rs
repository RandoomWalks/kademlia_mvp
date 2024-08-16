use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::time::Duration;

// Adding + 'static to the type bounds ensures that K lives for the entire duration of the program, which is necessary for trait objects.
// May prevent the use of types that contain non-'static references


pub trait CachePolicy<K: 'static>: Send + Sync {
    fn on_insert(&mut self, key: &K);
    fn on_access(&mut self, key: &K);
    fn evict(&mut self) -> Option<K>;

    // New method to handle key removal from the cache
    fn remove(&mut self, key: &K);
}

pub struct LRUCachePolicy<K> {
    usage_order: VecDeque<K>,
    positions: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone> LRUCachePolicy<K> {
    pub fn new() -> Self {
        Self {
            usage_order: VecDeque::new(),
            positions: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone + 'static + Send + Sync> CachePolicy<K> for LRUCachePolicy<K> {
    fn on_insert(&mut self, key: &K) {
        if !self.positions.contains_key(key) {
            self.usage_order.push_back(key.clone());
            self.positions
                .insert(key.clone(), self.usage_order.len() - 1);
        }
    }

    fn on_access(&mut self, key: &K) {
        if let Some(&pos) = self.positions.get(key) {
            self.usage_order.remove(pos); // Remove the key from the old position
            self.usage_order.push_back(key.clone()); // Add it to the back (most recently used)
            self.positions
                .insert(key.clone(), self.usage_order.len() - 1);
        }
    }

    fn evict(&mut self) -> Option<K> {
        if let Some(least_used) = self.usage_order.pop_front() {
            self.positions.remove(&least_used);
            return Some(least_used);
        }
        None
    }

    fn remove(&mut self, key: &K) {
        if let Some(&pos) = self.positions.get(key) {
            self.usage_order.remove(pos); // Remove from the usage order
            self.positions.remove(key); // Remove from the positions map
        }
    }
}

pub enum EvictionPolicy {
    LRU,
    // Placeholder for additional strategies like LFU or Time-based.
}

impl EvictionPolicy {
    pub fn build<K: Eq + Hash + Clone + 'static  + Send + Sync>(&self) -> Box<dyn CachePolicy<K>> {
        match self {
            EvictionPolicy::LRU => Box::new(LRUCachePolicy::new()),
        }
    }
}

// impl<K: Eq + Hash + Clone> EvictionPolicy<K> {
//     pub fn build(self) -> Box<dyn CachePolicy<K>> {
//         match self {
//             EvictionPolicy::LRU => Box::new(LRUCachePolicy::new()),
//         }
//     }
// }


