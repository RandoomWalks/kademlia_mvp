use crate::kbucket::KBucket;
use crate::utils::NodeId;
// use tokio::net::SocketAddr;
use std::net::SocketAddr;

use log::{info, warn, debug, error};
use std::collections::BinaryHeap;

pub struct RoutingTable {
    buckets: Vec<KBucket>,
    node_id: NodeId,
}

impl RoutingTable {
    pub fn new(node_id: NodeId) -> Self {
        RoutingTable {
            buckets: (0..256).map(|_| KBucket::new()).collect(),
            node_id,
        }
    }

    pub fn update(&mut self, node: NodeId, addr: SocketAddr) {
        let distance = self.node_id.distance(&node);
        let bucket_index = distance.as_bytes().iter().position(|&x| x != 0).unwrap_or(255);
        self.buckets[bucket_index].update(node, addr);
        debug!("Updated routing table for node {:#?}", node);
    }

    pub fn find_closest(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut heap = BinaryHeap::new();

        for bucket in &self.buckets {
            for entry in &bucket.entries {
                let distance = entry.node_id.distance(target);
                heap.push((std::cmp::Reverse(distance), entry.node_id, entry.addr));
            }
        }

        let closest_nodes = heap
            .into_iter()
            .take(count)
            .map(|(_, node_id, addr)| (node_id, addr))
            .collect();

        info!("Found closest nodes to {:#?}: {:?}", target, closest_nodes);
        closest_nodes
    }
}