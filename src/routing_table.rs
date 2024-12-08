use crate::kbucket::KBucket;
use crate::utils::NodeId;
use log::{info, debug};
use std::net::SocketAddr;
use std::collections::BinaryHeap;
use std::fmt;
use std::backtrace::Backtrace;
use parking_lot::RwLock;


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

    pub fn update(&mut self, node: NodeId, addr: SocketAddr) {
        let distance = self.node_id.distance(&node); // Calculate distance between current node and the target node
        let bucket_index = distance.as_bytes().iter().position(|&x| x != 0).unwrap_or(255); // Determine the appropriate k-bucket index based on distance
        self.buckets[bucket_index].update(node, addr); // Update the corresponding k-bucket with the node's information
        // debug!("Updated routing table for node {:?}", node);
        debug!(
            "RoutingTable::update() Updated routing table with node {:?} at address {}",
            node, addr
        );
        let bt = Backtrace::capture();
        debug!(
            "{:?}",
            bt
        );

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
    pub fn find_closest(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut heap = BinaryHeap::new(); // Binary heap to store nodes ordered by distance

        // Iterate through each k-bucket and its entries to find closest nodes
        for bucket in &self.buckets {
            for entry in &bucket.entries {
                let distance = entry.node_id.distance(target); // Calculate distance from the target node
                heap.push((std::cmp::Reverse(distance), entry.node_id, entry.addr)); // Push node info into the heap with reverse distance for max-heap behavior
            }
        }

        // Extract the closest nodes from the heap, limited by the count parameter
        let closest_nodes = heap
            .into_iter()
            .take(count)
            .map(|(_, node_id, addr)| (node_id, addr))
            .collect();

        info!("Found closest nodes to {:?}: {:?}", target, closest_nodes);
        closest_nodes
    }
}
