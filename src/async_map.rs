use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::task;
use tracing::{debug, error, info, warn};

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

impl<'de> Deserialize<'de> for SerdeInstant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = i64::deserialize(deserializer)?;
        let duration = Duration::from_secs(secs.unsigned_abs() as u64);
        let instant = if secs.is_negative() {
            Instant::now() - duration
        } else {
            Instant::now() + duration
        };
        Ok(SerdeInstant(instant))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct ValueWithTTL<V: Clone + Send + Sync> {
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

#[derive(serde::Serialize, serde::Deserialize)]
pub struct KeyValMap<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + std::fmt::Debug,
    V: Clone + Send + Sync + std::fmt::Debug,
{
    value: HashMap<K, ValueWithTTL<V>>,
    #[serde(skip)]
    _phantom: PhantomData<(K, V)>,
}

#[derive(Clone)]
pub struct KeyValueStoreTTL<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + std::fmt::Debug,
    V: Clone + Send + Sync + std::fmt::Debug,
{
    pub store: Arc<RwLock<HashMap<K, ValueWithTTL<V>>>>,
    pub stop_flag: Arc<Notify>,
    pub stop_snapshot_flag: Arc<Notify>,
    pub cleanup_task: Arc<RwLock<Option<task::JoinHandle<()>>>>,
    pub snapshot_task: Arc<RwLock<Option<task::JoinHandle<Result<(), KVStoreError>>>>>,
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
        + std::fmt::Debug
        + 'static
        + Serialize
        + for<'de> Deserialize<'de>,
    V: Clone + Send + Sync + std::fmt::Debug + 'static + Serialize + for<'de> Deserialize<'de>,
{
    pub fn new() -> Self {
        KeyValueStoreTTL {
            store: Arc::new(RwLock::new(HashMap::new())),
            stop_flag: Arc::new(Notify::new()),
            stop_snapshot_flag: Arc::new(Notify::new()),
            cleanup_task: Arc::new(RwLock::new(None)),
            snapshot_task: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn create_snapshot(&self) -> Result<Vec<u8>> {
        debug!("Creating snapshot...");

        let store_read = self
            .store
            .read()
            .await
            .map_err(|e| anyhow!("Serialization error: {:#?}", e))?;
        let kv_map = KeyValMap {
            value: store_read.clone(),
            _phantom: PhantomData,
        };
        let snapshot = serde_json::to_vec(&kv_map).context("Failed to serialize snapshot")?;

        debug!("Snapshot created: {:?}", snapshot);
        Ok(snapshot)
    }

    pub async fn restore_from_snapshot(&self, data: &[u8]) -> Result<()> {
        debug!("Restoring from snapshot...");
        let kv: KeyValMap<K, V> =
            serde_json::from_slice(data).context("Failed to deserialize snapshot")?;

        debug!("Snapshot data: {:#?}", kv);

        let mut store_write = self
            .store
            .write()
            .await
            .map_err(|e| anyhow!("Lock acquisition failed: {:#?}", e))?;

        *store_write = kv.value;

        Ok(())
    }

    pub async fn save_snapshot_to_file(&self, p: &Path) -> Result<()> {
        debug!("Saving snapshot to file...");
        let snapshot = self.create_snapshot().await?;

        let mut f_handle = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(p)
            .await
            .map_err(|e| KVStoreError::SnapshotError(e.to_string()))?;

        f_handle
            .write_all(&snapshot)
            .await
            .map_err(|e| KVStoreError::SnapshotError(e.to_string()))?;

        debug!("Snapshot saved to file: {:?}", p);
        Ok(())
    }

    pub async fn load_snapshot_from_file(&self, path: &Path) -> Result<()> {
        debug!("Loading snapshot from file...");
        let data = tokio::fs::read(path)
            .await
            .map_err(|e| KVStoreError::RestoreError(e.to_string()))?;
        self.restore_from_snapshot(&data).await?;
        debug!("Snapshot loaded from file: {:?}", path);
        Ok(())
    }

    pub async fn start_snapshot_task(&self, interval: Duration, path: PathBuf) -> Result<()> {
        debug!("Starting snapshot task with interval: {:?}", interval);
        let store_clone = self.store.clone();
        let flag_clone = self.stop_snapshot_flag.clone();

        let join_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;
                if flag_clone.notified().await {
                    debug!("Snapshot task stopping...");
                    break;
                }

                debug!("Snapshot task waking up to take snapshot...");
                let snapshot = match store_clone.read().await {
                    Ok(snap) => serde_json::to_vec(&*snap)?,
                    Err(e) => {
                        error!("Failed to acquire read lock for snapshot: {}", e);
                        continue;
                    }
                };

                if let Err(e) = tokio::fs::write(&path, snapshot).await {
                    error!("Failed to write snapshot to file: {}", e);
                } else {
                    debug!("Snapshot written to file: {:?}", path);
                }
            }
            Ok(())
        });

        let mut snapshot_task = self
            .snapshot_task
            .write()
            .await
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;

        *snapshot_task = Some(join_handle);

        Ok(())
    }

    pub async fn start_cleanup_task(&self) -> Result<()> {
        debug!("Starting cleanup task...");
        let store_clone = self.store.clone();
        let flag_clone = self.stop_flag.clone();

        let join_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if flag_clone.notified().await {
                    debug!("Cleanup task stopping...");
                    break;
                }

                debug!("Cleanup task waking up to clean expired entries...");
                if let Err(e) = Self::cleanup_expired_entries(&store_clone).await {
                    error!("Error during cleanup: {:?}", e);
                }
            }
        });

        let mut cleanup_task = self
            .cleanup_task
            .write()
            .await
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;
        *cleanup_task = Some(join_handle);

        Ok(())
    }

    pub async fn cleanup_expired_entries(
        store: &Arc<RwLock<HashMap<K, ValueWithTTL<V>>>>,
    ) -> Result<()> {
        debug!("Cleaning up expired entries...");
        let mut s = store
            .write()
            .await
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;

        let before_count = s.len();
        s.retain(|_, v| !v.is_expired());
        let after_count = s.len();

        info!("Cleaned up {} expired entries", before_count - after_count);

        Ok(())
    }

    pub async fn insert(&self, key: K, val: V) -> Result<()> {
        debug!("Inserting key: {:?}", key);
        let ttl_val = ValueWithTTL::new(val);

        self.store
            .write()
            .await
            .map_err(|e| KVStoreError::LockError(e.to_string()))?
            .insert(key.clone(), ttl_val);

        debug!("Inserted key: {:?}", key);
        Ok(())
    }

    pub async fn retrieve(&self, key: K) -> Result<V> {
        debug!("Retrieving key: {:?}", key);
        let store = self
            .store
            .read()
            .await
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

    pub async fn get_keys(&self) -> Result<Vec<K>> {
        debug!("Getting all keys...");
        let store = self
            .store
            .read()
            .await
            .map_err(|e| KVStoreError::LockError(e.to_string()))?;
        let keys: Vec<K> = store.keys().cloned().collect();

        debug!("Retrieved {} keys", keys.len());
        Ok(keys)
    }
}

impl<K, V> Drop for KeyValueStoreTTL<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + std::fmt::Debug + 'static,
    V: Clone + Send + Sync + std::fmt::Debug + 'static,
{
    fn drop(&mut self) {
        debug!("Dropping KeyValueStoreTTL and stopping tasks...");
        self.stop_flag.notify_waiters();
        if let Ok(mut cleanup_task) = self.cleanup_task.blocking_write() {
            if let Some(handle) = cleanup_task.take() {
                // tokio::task::block_in_place(|| handle.await.unwrap());
                tokio::task::block_in_place(|| async {
                    handle.await.unwrap();
                })
                .await;
            }
        }

        self.stop_snapshot_flag.notify_waiters();
        if let Ok(mut snapshot_task) = self.snapshot_task.blocking_write() {
            if let Some(handle) = snapshot_task.take() {
                // tokio::task::block_in_place(|| handle.await.unwrap());
                tokio::task::block_in_place(|| async {
                    handle.await.unwrap();
                })
                .await;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();

    store
        .start_cleanup_task()
        .await
        .map_err(|e| anyhow!("Failed to start cleanup task: {}", e))?;

    let path = Path::new("KeyValueSnapshot.json");

    store
        .start_snapshot_task(Duration::from_secs(5), path.to_path_buf())
        .await
        .map_err(|e| anyhow!("Failed to start snapshot task: {}", e))?;

    store
        .insert("key1".to_string(), 42)
        .await
        .map_err(|e| anyhow!("Failed to insert key1: {}", e))?;
    store
        .insert("key2".to_string(), 24)
        .await
        .map_err(|e| anyhow!("Failed to insert key2: {}", e))?;

    let val1 = store
        .retrieve("key1".to_string())
        .await
        .map_err(|e| anyhow!("Failed to retrieve key1: {}", e))?;
    info!("Retrieved value for key1: {}", val1);

    let keys = store
        .get_keys()
        .await
        .map_err(|e| anyhow!("Failed to get keys: {}", e))?;
    info!("All keys: {:?}", keys);

    // Save a snapshot
    store
        .save_snapshot_to_file(Path::new("manual_snapshot.json"))
        .await
        .map_err(|e| anyhow!("Failed to save snapshot: {}", e))?;

    // Simulate some changes
    store
        .insert("key3".to_string(), 99)
        .await
        .map_err(|e| anyhow!("Failed to insert key3: {}", e))?;

    // Load the snapshot (this will overwrite the current state)
    store
        .load_snapshot_from_file(Path::new("manual_snapshot.json"))
        .await
        .map_err(|e| anyhow!("Failed to load snapshot: {}", e))?;

    let keys_after_restore = store
        .get_keys()
        .await
        .map_err(|e| anyhow!("Failed to get keys after restore: {}", e))?;
    info!("Keys after restore: {:?}", keys_after_restore);

    // Keep the main thread alive indefinitely
    loop {
        debug!("Main thread sleeping...");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }

    // Ok(())
}
