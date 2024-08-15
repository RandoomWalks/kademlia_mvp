use std::net::SocketAddr;
use crate::utils::NodeId;
use log::{info, warn};
use serde::{Serialize, Deserialize};
use tokio::time::{Instant, Duration};
use std::fmt;

const K: usize = 20; // Maximum number of nodes in a k-bucket

#[derive(Clone, Serialize, Deserialize)]
pub struct KBucketEntry {
    pub node_id: NodeId,
    pub addr: SocketAddr,
    #[serde(serialize_with = "serialize_instant", deserialize_with = "deserialize_instant")]
    pub last_seen: Instant,
}

impl fmt::Debug for KBucketEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KBucketEntry")
            .field("node_id", &format!("{:x?}", self.node_id.0))
            .field("addr", &self.addr)
            .field("last_seen", &self.last_seen)
            .finish()
    }
}


/// Represents a Kademlia k-bucket, which stores up to `K` nodes that are close to each other
/// in terms of XOR distance from the owning node's ID.
#[derive(Debug)]
pub struct KBucket {
    pub entries: Vec<KBucketEntry>, // Stores up to `K` entries of nodes
}

/// Serializes an `Instant` into a `u64` representing the number of milliseconds elapsed since the instant.
pub fn serialize_instant<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let duration = instant.elapsed().as_millis() as u64;
    serializer.serialize_u64(duration)
}

/// Deserializes a `u64` into an `Instant` by subtracting the milliseconds from the current time.
pub fn deserialize_instant<'de, D>(deserializer: D) -> Result<Instant, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let millis = u64::deserialize(deserializer)?;
    Ok(Instant::now() - tokio::time::Duration::from_millis(millis))
}

impl KBucket {
    pub fn new() -> Self {
        KBucket {
            entries: Vec::with_capacity(K),
        }
    }

    /// Updates an entry in the k-bucket or adds a new entry if space is available.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The ID of the node to update or add.
    /// * `addr` - The network address of the node.
    ///
    /// # Output
    ///
    /// This function does not return any value but updates the internal state of the k-bucket.
    pub fn update(&mut self, node_id: NodeId, addr: SocketAddr) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.node_id == node_id)
        {
            // Update existing entry if the node ID already exists in the k-bucket
            let mut entry = self.entries.remove(index);
            entry.last_seen = Instant::now();
            self.entries.push(entry);
            info!("Updated node {:#?} in k-bucket", node_id);
        } else if self.entries.len() < K {
            // Add new entry if there is space in the k-bucket
            self.entries.push(KBucketEntry {
                node_id,
                addr,
                last_seen: Instant::now(),
            });
            info!("Added new node {:?} to k-bucket", node_id);
        } else {
            // Evict the oldest entry if the k-bucket is full
            let (oldest_index, should_evict) = {
                let oldest = self.entries
                    .iter()
                    .enumerate()
                    .min_by_key(|&(_, entry)| entry.last_seen)
                    .expect("There should be at least one entry in the bucket");

                let should_evict = oldest.1.last_seen.elapsed() > Duration::from_secs(3600); // Evict if older than 1 hour

                (oldest.0, should_evict)
            };

            if should_evict {
                // Evict the oldest node and add the new node
                let evicted_node_id = self.entries[oldest_index].node_id;
                self.entries.remove(oldest_index);
                self.entries.push(KBucketEntry { node_id, addr, last_seen: Instant::now() });
                info!("Evicted oldest node {:?} and added new node {:?}", evicted_node_id, node_id);
            } else {
                let oldest_entry = &self.entries[oldest_index];
                warn!("Oldest node {:?} was not evicted as it was recently seen", oldest_entry.node_id);
            }
        }
    }
}
