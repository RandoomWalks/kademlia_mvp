use crate::node::NodeId;
use log::{debug};
use std::collections::VecDeque;
use std::net::SocketAddr;

const K: usize = 20; // Kademlia constant for k-bucket size

#[derive(Debug, Clone)]
pub struct KBucket {
    pub nodes: VecDeque<(NodeId, SocketAddr)>,
}

#[derive(Debug, Clone)]
pub struct RoutingTable {
    pub buckets: Vec<KBucket>,
    pub node_id: NodeId,
}

// Implementation of KBucket, RoutingTable, and related functions
// ... (rest of the routing logic)


// #[derive(Debug, Clone)]
// struct KBucket {
//     nodes: VecDeque<(NodeId, SocketAddr)>,
// }

impl KBucket {
    pub fn new() -> Self {
        KBucket {
            nodes: VecDeque::with_capacity(K),
        }
    }

    pub fn add_node(&mut self, node: (NodeId, SocketAddr)) -> bool {
        if let Some(index) = self.nodes.iter().position(|x| x.0 == node.0) {
            debug!("Node {:?} already exists, moving it to the front.", node);
            let existing = self.nodes.remove(index).unwrap();
            self.nodes.push_front(existing);
            false
        } else if self.nodes.len() < K {
            debug!("Adding new node {:?} to the front.", node);
            self.nodes.push_front(node);
            true
        } else {
            debug!("Bucket full. Ignoring new node {:?}", node);
            false
        }
    }
}

// #[derive(Debug, Clone)]
// struct RoutingTable {
//     buckets: Vec<KBucket>,
//     node_id: NodeId,
// }

impl RoutingTable {
    pub fn new(node_id: NodeId) -> Self {
        debug!("Creating new routing table for NodeId: {:?}", node_id);
        RoutingTable {
            buckets: (0..256).map(|_| KBucket::new()).collect(),
            node_id,
        }
    }

    pub fn add_node(&mut self, node: (NodeId, SocketAddr)) -> bool {
        let index = self.bucket_index(&node.0);
        debug!("Adding node {:?} to bucket index: {}", node, index);
        self.buckets[index].add_node(node)
    }

    pub fn bucket_index(&self, other: &NodeId) -> usize {
        let distance = self.node_id.distance(other);
        let index = distance
            .iter()
            .position(|&b| b != 0)
            .map_or(255, |i| i * 8 + distance[i].leading_zeros() as usize);
        debug!("Calculated bucket index: {} for NodeId: {:?}", index, other);
        index
    }

    pub fn find_closest_nodes(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut closest: Vec<_> = self
            .buckets
            .iter()
            .flat_map(|bucket| bucket.nodes.iter().cloned())
            .collect();
        debug!("Collected all nodes for lookup: {:?}", closest);
        closest.sort_by_key(|(id, _)| id.distance(target));
        closest.truncate(count);
        debug!("Sorted and truncated closest nodes: {:?}", closest);
        closest
    }
}
