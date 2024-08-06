use anyhow::{anyhow, Context, Result};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use serde::de::{DeserializeOwned, DeserializeSeed};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hash;
use std::io::prelude::*;
use std::io::{self, BufWriter, Write};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use tracing_subscriber;
use std::fmt::Debug;


#[derive(Debug, thiserror::Error)]
pub enum KVStoreError {
    #[error("Lock acquisition failed: {0}")]
    LockError(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Entry expired: {0}")]
    ExpiredEntry(String),
    #[error("Insertion error: {0}")]
    InsertionError(String),
    #[error("SnapshotError error: {0}")]
    SnapshotError(String),
    #[error("RestoreError error: {0}")]
    RestoreError(String),
    #[error("SerdeError error: {0}")]
    SerdeError(String),
}

#[derive(Debug, Clone, Copy)]
pub struct SerdeInstant(Instant);

impl SerdeInstant {
    pub fn new(instant: Instant) -> Self {
        SerdeInstant(instant)
    }
}

impl Serialize for SerdeInstant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = self.0.duration_since(Instant::now());
        serializer.serialize_i64(duration.as_secs() as i64)
    }
}

//  lifetime parameter 'de, which is necessary for the deserialization process.
impl<'de> Deserialize<'de> for SerdeInstant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize an i64 value from the deserializer.
        // This value represents the number of seconds from the current time.
        let secs = i64::deserialize(deserializer)?;

        // Convert the deserialized i64 value into a Duration.
        // We use unsigned_abs() to handle both positive and negative values.
        // unsigned_abs() ensures we always get a non-negative duration.
        let duration = Duration::from_secs(secs.unsigned_abs() as u64);

        // Determine the Instant based on whether the deserialized value is positive or negative.
        // If secs is negative, we subtract the duration from the current time.
        // If secs is positive, we add the duration to the current time.
        let instant = if secs.is_negative() {
            Instant::now() - duration
        } else {
            Instant::now() + duration
        };

        // Wrap the calculated Instant in a SerdeInstant struct and return it.
        Ok(SerdeInstant(instant))
    }
}


#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct ValueWithTTL<V: Clone + Send + Sync + Debug> {
    pub val: V,
    pub expire_time: Option<SerdeInstant>,
}

impl<V: Clone + Send + Sync> ValueWithTTL<V> {
    pub fn new(val: V) -> Self {
        ValueWithTTL {
            val,
            expire_time: Some(SerdeInstant(Instant::now() + Duration::new(10, 0))),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expire_time
            .map_or(false, |exp_time| exp_time.0 <= Instant::now())
    }
}

#[derive(Debug,serde::Serialize, serde::Deserialize)]
pub struct KeyValMap<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + Debug,
    V: Clone + Send + Sync + Debug,
{
    value: HashMap<K, ValueWithTTL<V>>,
    #[serde(skip)]
    _phantom: PhantomData<(K, V)>,
}

#[derive(Debug,Clone)]
pub struct KeyValueStoreTTL<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + Debug,
    V: Clone + Send + Sync + Debug,
{
    pub store: Arc<RwLock<HashMap<K, ValueWithTTL<V>>>>,
    pub stop_flag: Arc<AtomicBool>,
    pub stop_snapshot_flag: Arc<AtomicBool>,
    pub cleanup_thread: Arc<RwLock<Option<thread::JoinHandle<()>>>>,
    pub snapshot_thread: Arc<RwLock<Option<thread::JoinHandle<Result<(), KVStoreError>>>>>,
}

impl From<std::io::Error> for KVStoreError {
    fn from(error: std::io::Error) -> Self {
        KVStoreError::SnapshotError(error.to_string())
    }
}

impl From<serde_json::Error> for KVStoreError {
    fn from(err: serde_json::Error) -> KVStoreError {
        KVStoreError::SerdeError(err.to_string())
    }
}

impl<K, V> KeyValueStoreTTL<K, V>
where
    K: Eq
        + Hash
        + Clone
        + Send
        + Sync
        + Debug
        + 'static
        + Serialize
        + for<'de> Deserialize<'de>,
    V: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de>,
{
    pub fn new() -> Self {
        KeyValueStoreTTL {
            store: Arc::new(RwLock::new(HashMap::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            stop_snapshot_flag: Arc::new(AtomicBool::new(false)),
            cleanup_thread: Arc::new(RwLock::new(None)),
            snapshot_thread: Arc::new(RwLock::new(None)),
            // phantom: PhantomData,
        }
    }

    pub fn create_snapshot(&self) -> Result<Vec<u8>> {
        debug!("Creating snapshot...");
        
        let store_read: std::sync::RwLockReadGuard<HashMap<K, ValueWithTTL<V>>> = self
            .store
            .read()
            .map_err(|e| anyhow!("Serialization error: {:#?}", e))?;
        let kv_map = KeyValMap {
            value: store_read.clone(),
            _phantom: PhantomData,
        };
        // let snapshot = serde_json::to_vec(&kv_map).map_err(|e| anyhow!("serde_json::to_vec() FAIL: {:#?}", e));
        let snapshot = serde_json::to_vec(&kv_map).context("Failed to serialize snapshot")?;
        
        debug!("Snapshot created: {:?}",snapshot);
        Ok(snapshot)
    }

    pub fn restore_from_snapshot(&self, data: &[u8]) -> Result<()> {
        debug!("Restoring from snapshot...");
        // let kv: KeyValMap<K, V> = serde_json::from_slice(data).map_err(|e| anyhow!("Deserialization error: {:#?}", e))?;
        let kv: KeyValMap<K, V> = serde_json::from_slice(data).context("Failed to deserialize snapshot")?;
        
        debug!("Snapshot data: {:#?}", kv);

        let mut store_write = self
            .store
            .write()
            .map_err(|e| anyhow!("Lock acquisition failed: {:#?}", e))?;

        *store_write = kv.value;

        Ok(())
    }

    pub fn save_snapshot_to_file(&self, p: &Path) -> Result<()> {
        debug!("Saving snapshot to file...");
        let snapshot = self.create_snapshot()?;

        let mut f_handle = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(p)
            .map_err(|e| KVStoreError::SnapshotError(e.to_string()))?;

        f_handle
            .write_all(&snapshot)
            .map_err(|e| KVStoreError::SnapshotError(e.to_string()))?;
        
        debug!("Snapshot saved to file: {:?}", p);
        Ok(())
    }

    pub fn load_snapshot_from_file(&self, path: &Path) -> Result<()> {
        debug!("Loading snapshot from file...");
        let data = std::fs::read(path).map_err(|e| KVStoreError::RestoreError(e.to_string()))?;
        self.restore_from_snapshot(&data)?;
        debug!("Snapshot loaded from file: {:?}", path);
        Ok(())
    }

    pub fn start_snapshot_thread(&self, interval: Duration, path: PathBuf) -> Result<()> {
        debug!("Starting snapshot thread with interval: {:?}", interval);
        let store_clone = self.store.clone();
        let flag_clone = self.stop_snapshot_flag.clone();

        let join_handle = thread::spawn(move || -> Result<(), KVStoreError> {
            loop {
                if flag_clone.load(Ordering::SeqCst) {
                    debug!("Snapshot thread stopping...");
                    break;
                }

                thread::sleep(interval);

                debug!("Snapshot thread waking up to take snapshot...");
                let snapshot = match store_clone.read() {
                    Ok(snap) => {
                        serde_json::to_vec(&*snap)?
                    }
                    Err(e) => {
                        error!("Failed to acquire read lock for snapshot: {}", e);
                        continue;
                    }
                };

                if let Err(e) = std::fs::write(&path, snapshot) {
                    error!("Failed to write snapshot to file: {}", e);
                } else {
                    debug!("Snapshot written to file: {:?}", path);
                }
            }
            Ok(())
        });

        let mut snapshot_thread = self
            .snapshot_thread
            .write()
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;

        *snapshot_thread = Some(join_handle);

        Ok(())
    }

    pub fn start_cleanup_thread(&self) -> Result<()> {
        debug!("Starting cleanup thread...");
        let store_clone = self.store.clone();
        let flag_clone = self.stop_flag.clone();

        let join_handle = thread::spawn(move || loop {
            if flag_clone.load(Ordering::SeqCst) {
                debug!("Cleanup thread stopping...");
                break;
            }
            thread::sleep(Duration::from_secs(5));

            debug!("Cleanup thread waking up to clean expired entries...");
            if let Err(e) = Self::cleanup_expired_entries(&store_clone) {
                error!("Error during cleanup: {:?}", e);
            }
        });

        let mut cleanup_thread = self
            .cleanup_thread
            .write()
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;
        *cleanup_thread = Some(join_handle);

        Ok(())
    }

    pub fn cleanup_expired_entries(store: &Arc<RwLock<HashMap<K, ValueWithTTL<V>>>>) -> Result<()> {
        debug!("Cleaning up expired entries...");
        let mut s = store
            .write()
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;

        let before_count = s.len();
        s.retain(|_, v| !v.is_expired());
        let after_count = s.len();

        info!("Cleaned up {} expired entries", before_count - after_count);

        Ok(())
    }

    pub fn insert(&self, key: K, val: V) -> Result<()> {
        debug!("Inserting key: {:?}", key);
        let ttl_val = ValueWithTTL::new(val);

        self.store
            .write()
            .map_err(|e| KVStoreError::LockError(e.to_string()))?
            .insert(key.clone(), ttl_val);

        debug!("Inserted key: {:?}", key);
        Ok(())
    }

    pub fn retrieve(&self, key: K) -> Result<V> {
        debug!("Retrieving key: {:?}", key);
        let store = self
            .store
            .read()
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;

        let ttl_val = store
            .get(&key)
            .ok_or_else(|| KVStoreError::KeyNotFound(format!("{:?}", key)))?;

        if ttl_val.is_expired() {
            warn!("Attempted to retrieve expired key: {:?}", key);
            return Err(KVStoreError::ExpiredEntry(format!("{:?}", key)).into());
        }

        debug!("Retrieved key: {:?}", key);
        Ok(ttl_val.val.clone())
    }

    pub fn get_keys(&self) -> Result<Vec<K>> {
        debug!("Getting all keys...");
        let store = self
            .store
            .read()
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;
        let keys: Vec<K> = store.keys().cloned().collect();

        debug!("Retrieved {} keys", keys.len());
        Ok(keys)
    }
}

impl<K, V> Drop for KeyValueStoreTTL<K, V>
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    fn drop(&mut self) {
        debug!("Dropping KeyValueStoreTTL and stopping threads...");
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Ok(mut cleanup_thread) = self.cleanup_thread.write() {
            if let Some(handle) = cleanup_thread.take() {
                if let Err(e) = handle.join() {
                    error!("Error joining cleanup thread: {:?}", e);
                } else {
                    debug!("Cleanup thread joined successfully.");
                }
            }
        }

        self.stop_snapshot_flag.store(true, Ordering::SeqCst);
        if let Ok(mut snapshot_thread) = self.snapshot_thread.write() {
            if let Some(handle) = snapshot_thread.take() {
                if let Err(e) = handle.join() {
                    error!("Error joining snapshot thread: {:?}", e);
                } else {
                    debug!("Snapshot thread joined successfully.");
                }
            }
        }
    }
}

pub fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();

    store
        .start_cleanup_thread()
        .map_err(|e| anyhow!("Failed to start cleanup thread: {}", e))?;

    let path = Path::new("KeyValueSnapshot.json");

    store
        .start_snapshot_thread(Duration::from_secs(5), path.to_path_buf())
        .map_err(|e| anyhow!("Failed to start snapshot thread: {}", e))?;

    store
        .insert("key1".to_string(), 42)
        .map_err(|e| anyhow!("Failed to insert key1: {}", e))?;
    store
        .insert("key2".to_string(), 24)
        .map_err(|e| anyhow!("Failed to insert key2: {}", e))?;

    let val1 = store
        .retrieve("key1".to_string())
        .map_err(|e| anyhow!("Failed to retrieve key1: {}", e))?;
    info!("Retrieved value for key1: {}", val1);

    let keys = store
        .get_keys()
        .map_err(|e| anyhow!("Failed to get keys: {}", e))?;
    info!("All keys: {:?}", keys);

    // Save a snapshot
    store
        .save_snapshot_to_file(Path::new("manual_snapshot.json"))
        .map_err(|e| anyhow!("Failed to save snapshot: {}", e))?;

    // Simulate some changes
    store
        .insert("key3".to_string(), 99)
        .map_err(|e| anyhow!("Failed to insert key3: {}", e))?;

    // Load the snapshot (this will overwrite the current state)
    store
        .load_snapshot_from_file(Path::new("manual_snapshot.json"))
        .map_err(|e| anyhow!("Failed to load snapshot: {}", e))?;

    let keys_after_restore = store
        .get_keys()
        .map_err(|e| anyhow!("Failed to get keys after restore: {}", e))?;
    info!("Keys after restore: {:?}", keys_after_restore);

    // Keep the main thread alive indefinitely
    loop {
        debug!("Main thread sleeping...");
        thread::sleep(Duration::from_secs(60));
    }

    // if let Some(handle) = store.cleanup_thread.read().unwrap().as_ref() {
    //     handle.join().expect("Failed to join cleanup thread");
    // }

    // if let Some(handle) = store.snapshot_thread.read().unwrap().as_ref() {
    //     handle.join().expect("Failed to join snapshot thread");
    // }

    // Ok(())
}


// #[cfg(test)]
// pub mod tests {
//     use super::*;

//     #[test]
//     fn test_insert_and_retrieve() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), 42).unwrap();
//         let val = store.retrieve("key1".to_string()).unwrap();
//         assert_eq!(val, 42);
//     }

//     #[test]
//     fn test_expired_entry() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), 42).unwrap();
//         thread::sleep(Duration::from_secs(11)); // Wait for the entry to expire
        
//         let result = store.retrieve("key1".to_string());
//         // assert!(matches!(result, Err(KVStoreError::ExpiredEntry(_))));


//         assert!(matches!(result.unwrap_err().downcast_ref::<KVStoreError>(), Some(KVStoreError::ExpiredEntry(_))));

//     }

//     #[test]
//     fn test_cleanup_thread() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
//         store.start_cleanup_thread().unwrap();
        
//         store.insert("key1".to_string(), 42).unwrap();
//         store.insert("key2".to_string(), 24).unwrap();
        
//         thread::sleep(Duration::from_secs(11)); // Wait for entries to expire and cleanup to occur
        
//         let keys = store.get_keys().unwrap();
//         assert!(keys.is_empty());
//     }

//     #[test]
//     fn test_snapshot_and_restore() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), 42).unwrap();
//         store.insert("key2".to_string(), 24).unwrap();
        
//         let snapshot = store.create_snapshot().unwrap();
        
//         // Modify the store
//         store.insert("key3".to_string(), 99).unwrap();
        
//         // Restore from snapshot
//         store.restore_from_snapshot(&snapshot).unwrap();
        
//         let keys = store.get_keys().unwrap();
//         assert_eq!(keys.len(), 2);
//         assert!(keys.contains(&"key1".to_string()));
//         assert!(keys.contains(&"key2".to_string()));
//         assert!(!keys.contains(&"key3".to_string()));
//     }

//     #[test]
//     fn test_snapshot_to_file() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
//         let path = Path::new("test_snapshot.json");
        
//         store.insert("key1".to_string(), 42).unwrap();
//         store.save_snapshot_to_file(path).unwrap();
        
//         assert!(path.exists());
        
//         // Clean up
//         std::fs::remove_file(path).unwrap();
//     }

//     #[test]
//     fn test_load_snapshot_from_file() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
//         let path = Path::new("test_snapshot.json");
        
//         store.insert("key1".to_string(), 42).unwrap();
//         store.save_snapshot_to_file(path).unwrap();
        
//         // Modify the store
//         store.insert("key2".to_string(), 24).unwrap();
        
//         // Load snapshot
//         store.load_snapshot_from_file(path).unwrap();
        
//         let keys = store.get_keys().unwrap();
//         assert_eq!(keys.len(), 1);
//         assert!(keys.contains(&"key1".to_string()));
//         assert!(!keys.contains(&"key2".to_string()));
        
//         // Clean up
//         std::fs::remove_file(path).unwrap();
//     }

//     #[test]
//     fn test_concurrent_access() {
//         let store = Arc::new(KeyValueStoreTTL::<String, i32>::new());
//         let threads_count = 10;
//         let mut handles = vec![];
        
//         for i in 0..threads_count {
//             let store_clone = store.clone();
//             let handle = thread::spawn(move || {
//                 store_clone.insert(format!("key{}", i), i).unwrap();
//                 let val = store_clone.retrieve(format!("key{}", i)).unwrap();
//                 assert_eq!(val, i);
//             });
//             handles.push(handle);
//         }
        
//         for handle in handles {
//             handle.join().unwrap();
//         }
        
//         let keys = store.get_keys().unwrap();
//         // assert_eq!(keys.len(), threads_count);
//         assert_eq!(keys.len(), threads_count as usize);

//     }

//     #[test]
//     fn test_snapshot_thread() {
//         let store = Arc::new(KeyValueStoreTTL::<String, i32>::new());
//         let path = Path::new("test_snapshot_thread.json");
        
//         store.start_snapshot_thread(Duration::from_secs(1), path.to_path_buf()).unwrap();
        
//         store.insert("key1".to_string(), 42).unwrap();
        
//         thread::sleep(Duration::from_secs(2)); // Wait for snapshot to occur
        
//         assert!(path.exists());
        
//         // Clean up
//         std::fs::remove_file(path).unwrap();
//     }

//     #[test]
//     fn test_key_not_found() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         let result = store.retrieve("non_existent_key".to_string());
//         // assert!(matches!(result, Err(KVStoreError::KeyNotFound(_))));
//         assert!(matches!(result.unwrap_err().downcast_ref::<KVStoreError>(), Some(KVStoreError::KeyNotFound(_))));

//     }

//     #[test]
//     fn test_insert_overwrite() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), 42).unwrap();
//         store.insert("key1".to_string(), 24).unwrap();
        
//         let val = store.retrieve("key1".to_string()).unwrap();
//         assert_eq!(val, 24);
//     }
// }
