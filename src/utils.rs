use lazy_static::lazy_static; // `lazy_static!` is a macro for defining global, lazily initialized static variables.
use log::{debug, error, info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::net::SocketAddr; // `SocketAddr` represents a socket address with both an IP address and a port.
use std::sync::{Mutex, Once}; // `Once` ensures that a block of code runs only once 

pub const K: usize = 20; // Maximum number of nodes in a k-bucket
pub const ALPHA: usize = 3; // Number of parallel lookups
// pub const BOOTSTRAP_NODES: [&str; 1] = ["127.0.0.1:33333"]; // Hardcoded bootstrap node

/// Struct representing a unique identifier for a node in the network.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub [u8; 32]); // 32-byte identifier

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x?}", self.0)
    }
}

lazy_static! {
    // BOOTSTRAP_NODES is a lazily-initialized global static variable that holds a `Mutex`-protected `Vec` of `SocketAddr`.
    // The `Mutex` ensures that only one thread can modify the vector at a time, preventing race conditions.
    pub static ref BOOTSTRAP_NODES: Mutex<Vec<SocketAddr>> = Mutex::new(Vec::new());
}

// This function loads a list of bootstrap nodes into the global `BOOTSTRAP_NODES` vector.
// It takes a `Vec<String>` where each string is expected to be a valid IP address and port.
// The function iterates over the provided list, attempts to parse each string into a `SocketAddr`,
// and if successful, adds it to the global vector. If a string is invalid, it prints an error message.
pub fn load_bootstrap_nodes(nodes: Vec<String>) {
    let mut bootstrap_nodes = BOOTSTRAP_NODES.lock().unwrap();
    for node in nodes {
        if let Ok(addr) = node.parse::<SocketAddr>() {
            bootstrap_nodes.push(addr); // Add the valid `SocketAddr` to the vector.
        } else {
            eprintln!("Invalid address: {}", node); // Print an error if the string cannot be parsed into a `SocketAddr`.
        }
    }
}

// This function returns a clone of the current list of bootstrap nodes.
pub fn get_bootstrap_nodes() -> Vec<SocketAddr> {
    BOOTSTRAP_NODES.lock().unwrap().clone() // Lock the `Mutex` to access the vector, then return a cloned copy of it.
}

impl NodeId {
    pub fn new() -> Self {
        let random_bytes: [u8; 32] = rand::random();
        NodeId(random_bytes)
    }

    /// Calculates the XOR distance between this `NodeId` and another `NodeId`.
    ///
    /// # Arguments
    ///
    /// * `other` - A reference to another `NodeId` to calculate the distance from.
    ///
    /// # Returns
    ///
    /// A `NodeId` representing the XOR distance between the two nodes.
    ///
    /// # Example
    ///
    /// ```
    /// let distance = node_id1.distance(&node_id2);
    /// ```
    pub fn distance(&self, other: &NodeId) -> NodeId {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.0[i] ^ other.0[i]; // XOR each byte to calculate distance
        }
        NodeId(result)
    }

    /// Creates a `NodeId` from a 32-byte slice.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A reference to a 32-byte array.
    ///
    /// # Returns
    ///
    /// A `NodeId` instance created from the provided byte slice.
    ///
    /// # Example
    ///
    /// ```
    /// let bytes = [0u8; 32];
    /// let node_id = NodeId::from_slice(&bytes);
    /// ```
    pub fn from_slice(bytes: &[u8; 32]) -> Self {
        NodeId(*bytes)
    }

    /// Returns a reference to the internal byte array.
    ///
    /// # Returns
    ///
    /// A reference to the internal 32-byte array.
    ///
    /// # Example
    ///
    /// ```
    /// let bytes = node_id.as_bytes();
    /// ```
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Ord for NodeId {
    /// Compares two `NodeId` instances for ordering.
    ///
    /// # Arguments
    ///
    /// * `other` - A reference to another `NodeId` to compare with.
    ///
    /// # Returns
    ///
    /// An `Ordering` indicating the result of the comparison.
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0) // Lexicographical comparison of the byte arrays
    }
}

impl PartialOrd for NodeId {
    /// Partially compares two `NodeId` instances for ordering.
    ///
    /// # Arguments
    ///
    /// * `other` - A reference to another `NodeId` to compare with.
    ///
    /// # Returns
    ///
    /// An `Option<Ordering>` with the comparison result.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
