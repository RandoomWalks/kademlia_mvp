use rand::Rng;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use log::{info, warn, debug, error};

pub const K: usize = 20; // Maximum number of nodes in a k-bucket
pub const ALPHA: usize = 3; // Number of parallel lookups
pub const BOOTSTRAP_NODES: [&str; 1] = ["127.0.0.1:33333"]; // Hardcoded bootstrap node


#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub struct NodeId([u8; 32]);

impl NodeId {
    pub fn new() -> Self {
        let random_bytes: [u8; 32] = rand::random();
        NodeId(random_bytes)
    }

    pub fn distance(&self, other: &NodeId) -> NodeId {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.0[i] ^ other.0[i];
        }
        NodeId(result)
    }
    pub fn from_slice(bytes: &[u8; 32]) -> Self {
        NodeId(*bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Ord for NodeId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for NodeId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
