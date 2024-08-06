use kademlia_mvp::*; // Import functions and structs for testing

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
use kademlia_mvp::NodeId;

// #[cfg(test)]
// pub mod tests {
//     use super::*;
//     use std::sync::Arc;
//     use std::thread;

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

//     // Edge Case Tests

//     #[test]
//     fn test_insert_multiple_keys_same_value() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), 42).unwrap();
//         store.insert("key2".to_string(), 42).unwrap();
        
//         let val1 = store.retrieve("key1".to_string()).unwrap();
//         let val2 = store.retrieve("key2".to_string()).unwrap();
        
//         assert_eq!(val1, 42);
//         assert_eq!(val2, 42);
//     }

//     #[test]
//     fn test_retrieve_after_immediate_expiration() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), 42).unwrap();
        
//         thread::sleep(Duration::from_secs(10)); // Wait for entry to expire
        
//         let result = store.retrieve("key1".to_string());
//         assert!(matches!(result.unwrap_err().downcast_ref::<KVStoreError>(), Some(KVStoreError::ExpiredEntry(_))));
//     }

//     #[test]
//     fn test_insert_with_no_ttl() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         let mut val = ValueWithTTL::new(42);
//         val.expire_time = None; // No TTL
//         store.store.write().unwrap().insert("key1".to_string(), val);
        
//         let retrieved_val = store.retrieve("key1".to_string()).unwrap();
//         assert_eq!(retrieved_val, 42);
//     }

//     #[test]
//     fn test_manual_ttl_setting() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         let mut val = ValueWithTTL::new(42);
        
        
//         // val.expire_time = Some(SerdeInstant(Instant::now() + Duration::new(15, 0))); // 15 seconds TTL

//         val.expire_time = Some(SerdeInstant::new(Instant::now() + Duration::new(15, 0)));

        
//         store.store.write().unwrap().insert("key1".to_string(), val);
        
//         thread::sleep(Duration::from_secs(11)); // Wait less than TTL
        
//         let retrieved_val = store.retrieve("key1".to_string()).unwrap();
//         assert_eq!(retrieved_val, 42);
//     }

//     #[test]
//     fn test_insertion_of_different_types() {
//         let store: KeyValueStoreTTL<String, String> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), "value1".to_string()).unwrap();
//         let val = store.retrieve("key1".to_string()).unwrap();
//         assert_eq!(val, "value1");
//     }

//     #[test]
//     fn test_concurrent_expiration_handling() {
//         let store = Arc::new(KeyValueStoreTTL::<String, i32>::new());
//         let threads_count = 10;
//         let mut handles = vec![];
        
//         for i in 0..threads_count {
//             let store_clone = store.clone();
//             let handle = thread::spawn(move || {
//                 store_clone.insert(format!("key{}", i), i).unwrap();
//                 thread::sleep(Duration::from_secs(11)); // Wait for expiration
//                 let result = store_clone.retrieve(format!("key{}", i));
//                 assert!(matches!(result.unwrap_err().downcast_ref::<KVStoreError>(), Some(KVStoreError::ExpiredEntry(_))));
//             });
//             handles.push(handle);
//         }
        
//         for handle in handles {
//             handle.join().unwrap();
//         }
//     }

//     #[test]
//     fn test_snapshot_restore_mixed_ttl_entries() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         let mut val1 = ValueWithTTL::new(42);
//         val1.expire_time = None; // No TTL
//         store.store.write().unwrap().insert("key1".to_string(), val1);
        
//         let val2 = ValueWithTTL::new(24); // With TTL
//         store.store.write().unwrap().insert("key2".to_string(), val2);
        
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
        
//         let val1_retrieved = store.retrieve("key1".to_string()).unwrap();
//         assert_eq!(val1_retrieved, 42);
        
//         let val2_retrieved = store.retrieve("key2".to_string()).unwrap();
//         assert_eq!(val2_retrieved, 24);
//     }

//     #[test]
//     fn test_retrieve_expired_key_after_snapshot_restore() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
        
//         store.insert("key1".to_string(), 42).unwrap();
        
//         let snapshot = store.create_snapshot().unwrap();
        
//         thread::sleep(Duration::from_secs(11)); // Wait for entry to expire
        
//         // Restore from snapshot
//         store.restore_from_snapshot(&snapshot).unwrap();
        
//         let result = store.retrieve("key1".to_string());
//         assert!(matches!(result.unwrap_err().downcast_ref::<KVStoreError>(), Some(KVStoreError::ExpiredEntry(_))));
//     }

//     #[test]
//     fn test_cleanup_with_no_entries() {
//         let store: KeyValueStoreTTL<String, i32> = KeyValueStoreTTL::new();
//         store.start_cleanup_thread().unwrap();
        
//         thread::sleep(Duration::from_secs(6)); // Wait for cleanup to occur
        
//         let keys = store.get_keys().unwrap();
//         assert!(keys.is_empty());
//     }

//     #[test]
//     fn test_insert_and_retrieve_large_data() {
//         let store: KeyValueStoreTTL<String, Vec<u8>> = KeyValueStoreTTL::new();
        
//         let large_data = vec![0u8; 10_000]; // 10 KB of data
//         store.insert("key1".to_string(), large_data.clone()).unwrap();
        
//         let retrieved_data = store.retrieve("key1".to_string()).unwrap();
//         assert_eq!(retrieved_data, large_data);
//     }
// }


#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_node_id_distance() {
        let id1 = NodeId::from_bytes([0; 32]);
        let id2 = NodeId::from_bytes([1; 32]);
        let distance = id1.distance(&id2);
        assert_eq!(*distance._distance_internal(), [1; 32]);
    }

    #[test]
    fn test_node_id_initialization() {
        let id1 = NodeId::from_bytes([0; 32]);
        let id2 = NodeId::from_bytes([1; 32]);

        // Add your test assertions here
    }

    #[test]
    fn test_k_bucket_update() {
        let mut bucket = KBucket::new();
        let node_id = NodeId::new();
        let addr = "127.0.0.1:8000".parse().unwrap();

        bucket.update(node_id, addr);
        assert_eq!(bucket.entries.len(), 1);
        assert_eq!(bucket.entries[0].node_id, node_id);
        assert_eq!(bucket.entries[0].addr, addr);
    }

    #[tokio::test]
    async fn test_kademlia_node_ping() {
        let addr1 = "127.0.0.1:8001".parse().unwrap();
        let addr2 = "127.0.0.1:8002".parse().unwrap();
        let node1 = KademliaNode::new(addr1).await;
        let node2 = KademliaNode::new(addr2).await;

        tokio::spawn(async move {
            node2.start().await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = node1.ping(addr2).await;
        assert!(result.is_ok());
    }
}