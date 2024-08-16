use log::{debug, error};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tokio::time::sleep;

use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
struct CacheEntry {
    value: Vec<u8>,
    last_accessed: Instant,
    ttl: Duration,
}

#[derive(Debug)]
struct AsyncCache {
    store: Arc<RwLock<HashMap<Vec<u8>, CacheEntry>>>, // Key
}

#[derive(Debug)]
enum CacheError {
    KeyNotFound,
    InvalidValue,
    Expired,
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            CacheError::InvalidValue => write!(f, "Value invalid in cache"),
            CacheError::KeyNotFound => write!(f, "Key not found in cache"),
            CacheError::Expired => write!(f, "Key has expired"),
        }
    }
}

impl std::error::Error for CacheError {}

impl AsyncCache {
    fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    async fn get(&self, _key: Vec<u8>) -> Result<CacheEntry, CacheError> {
        let mut store = self.store.write().await; // to update ttl
        let res: Option<&CacheEntry> = store.get(&_key);
        if let Some(entry) = store.get_mut(&_key) {
            // check ttl
            if entry.last_accessed.elapsed() > entry.ttl {
                // evict
                store.remove(&_key);
                return Err(CacheError::Expired);
            } else {
                // update ts
                entry.last_accessed = Instant::now();
                return Ok(entry.clone());
            }
        } else {
            Err(CacheError::KeyNotFound)
        }
    }

    async fn put(&self, _key: Vec<u8>, _val: Vec<u8>, _ttl: Duration) {
        // create before acquiring lock
        let mut new_entry = CacheEntry {
            value: _val,
            last_accessed: Instant::now(),
            ttl: _ttl,
        };

        let mut store: tokio::sync::RwLockWriteGuard<HashMap<Vec<u8>, CacheEntry>> =
            self.store.write().await;
        new_entry.last_accessed = Instant::now();

        // insert (possibly overwriting prev)
        store.insert(_key, new_entry);
    }

    async fn size(&self) -> usize {
        let store = self.store.read().await;
        store.len()
    }
    
}


pub async fn cache_run() {
    let cache = AsyncCache::new();
    let new_key = vec![0,1,2,3];
    let new_val = vec![4,5,6,7];
    
    cache.put(new_key.clone(), new_val, Duration::from_secs(2)).await;
    println!("Cache size after put: {}", cache.size().await);

    if let Ok(entry) = cache.get(new_key.clone()).await {
        println!("value retrieved: {:#?}",entry );
    }
    sleep(Duration::from_secs(1)).await;
    
        if let Ok(entry) = cache.get(new_key.clone()).await {
            println!("value retrieved after 1 sec: {:#?}",entry );
        }

    // Wait for 6 seconds (longer than TTL)
    sleep(Duration::from_secs(2)).await;
    

    match cache.get(new_key.clone()).await {
        Ok(val) => println!("value retrieved after 3 sec: {:#?}", val),
        Err(e) => println!("Error: {}", e),
    }
    
    println!("Cache size after TTL expiration: {}", cache.size().await);

}

