use crate::kbucket::KBucket;
use crate::utils::NodeId;
use log::{info, debug};
use std::net::SocketAddr;
use std::collections::BinaryHeap;
use std::fmt;

/// Struct representing the routing table of a Kademlia DHT node.
pub struct RoutingTable {
    pub buckets: Vec<KBucket>, // List of k-buckets in the routing table
    pub node_id: NodeId,       // ID of the current node
}

impl fmt::Debug for RoutingTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoutingTable")
            .field("node_id", &format!("{:x?}", self.node_id.0))
            .field("buckets_count", &self.buckets.len())
            .finish()
    }
}


impl RoutingTable {
    /// Constructs a new `RoutingTable` with initialized k-buckets.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The ID of the current node.
    ///
    /// # Returns
    ///
    /// A new `RoutingTable` instance.
    pub fn new(node_id: NodeId) -> Self {
        RoutingTable {
            buckets: (0..256).map(|_| KBucket::new()).collect(), // 256 k-buckets, one for each possible distance byte
            node_id,
        }
    }

    /// Updates the routing table with information about a node.
    ///
    /// # Arguments
    ///
    /// * `node` - The ID of the node to update.
    /// * `addr` - The network address of the node.
    ///
    /// # Output
    ///
    /// This function does not return any value but updates the internal state of the routing table.
    pub fn update(&mut self, node_id: NodeId, addr: SocketAddr) {
        let bucket_index = self.node_id.leading_zeros(&node_id) as usize; // Calculate the bucket index based on the XOR distance

        if let Some(bucket) = self.buckets.get_mut(bucket_index) {
            bucket.update(node_id.clone(), addr); // Use the `update` method of `KBucket`
            debug!(
                "Added/Updated node {:?} in bucket {} with distance {:?}",
                node_id, bucket_index, self.node_id.distance(&node_id)
            );
        }
    }
    /// Finds the closest nodes to a given target ID.
    ///
    /// # Arguments
    ///
    /// * `target` - The ID of the target node.
    /// * `count` - The number of closest nodes to find.
    ///
    /// # Returns
    ///
    /// A vector of tuples containing the closest nodes' IDs and network addresses.
    pub fn find_closest(&self, target: &NodeId, k: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut nodes = Vec::new();

        // Iterate over buckets to find the closest nodes
        for bucket in &self.buckets {
            for entry in &bucket.entries {
                nodes.push((entry.node_id.clone(), entry.addr));
            }
        }

        // Sort nodes by XOR distance to the target
        nodes.sort_by_key(|(node_id, _)| target.distance(node_id));
        
        // Return the top K closest nodes
        nodes.into_iter().take(k).collect()
    }
}
