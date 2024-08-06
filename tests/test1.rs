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