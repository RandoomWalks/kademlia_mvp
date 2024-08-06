// src/lib.rs
use log::{debug, error, info, warn};
use rand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const K: usize = 20; // Maximum number of nodes in a k-bucket
const BUCKET_REFRESH_INTERVAL: Duration = Duration::from_secs(3600); // 1 hour

/// NodeId represents a unique identifier for a node in the network.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct NodeId([u8; 32]);

// Implement `Display` for `MinMax`.
impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(")?;
        for (index, &byte) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:02X}", byte)?;
        }
        write!(f, ")")
    }
}

impl NodeId {
    /// Creates a new NodeId with random bytes.
    pub fn new() -> Self {
        let random_bytes: [u8; 32] = rand::random();
        let node_id = NodeId(random_bytes);
        println!("Created new NodeId: {:?}", node_id);
        node_id
    }


    /// Generates a NodeId from a given key using SHA-256 hashing.
    pub fn from_key<K: AsRef<[u8]>>(key: K) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let result = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&result);
        let node_id = NodeId(id);
        println!("Created NodeId from key: {:?}", node_id);
        node_id
    }

    pub fn from_bytes(data: [u8; 32]) -> Self {
        let node_id = NodeId(data);
        println!("Created NodeId from bytes: {:?}", node_id);
        node_id
    }

    pub fn distance(&self, other: &NodeId) -> NodeId {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.0[i] ^ other.0[i];
        }
        let distance = NodeId(result);
        println!("Calculated distance between {:?} and {:?}: {:?}", self, other, distance);
        distance
    }
    
}

impl std::fmt::Debug for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeId({})", hex::encode(self.0))
    }
}

/// Represents an entry in a k-bucket, storing node information.
#[derive(Debug)]
pub struct KBucketEntry {
    pub node_id: NodeId,
    pub addr: SocketAddr,
    pub last_seen: Instant,
}

/// Represents a k-bucket used in the Kademlia routing table.
#[derive(Debug)]
pub struct KBucket {
    pub entries: VecDeque<KBucketEntry>,
    pub last_updated: Instant,
}

impl KBucket {
    /// Creates a new, empty k-bucket.
    pub fn new() -> Self {
        KBucket {
            entries: VecDeque::with_capacity(K),
            last_updated: Instant::now(),
        }
    }

    /// Updates the k-bucket with a given node information.
    pub fn update(&mut self, node_id: NodeId, addr: SocketAddr) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.node_id == node_id)
        {
            let mut entry = self.entries.remove(index).unwrap();
            entry.last_seen = Instant::now();
            entry.addr = addr;
            self.entries.push_back(entry);
            println!("Updated existing entry in k-bucket for {:?}", node_id);
        } else if self.entries.len() < K {
            self.entries.push_back(KBucketEntry {
                node_id,
                addr,
                last_seen: Instant::now(),
            });
            println!("Added new entry to k-bucket for {:?}", node_id);
        } else {
            println!("K-bucket is full, ignoring new entry for {:?}", node_id);
        }
        self.last_updated = Instant::now();
    }

    pub fn needs_refresh(&self) -> bool {
        let needs_refresh = self.last_updated.elapsed() > BUCKET_REFRESH_INTERVAL;
        debug!(
            "Checking if k-bucket needs refresh (last updated: {:?}, needs refresh: {})",
            self.last_updated,
            needs_refresh
        );
        needs_refresh
    }
}

/// Represents the routing table used in the Kademlia protocol.
pub struct RoutingTable {
    pub buckets: Vec<KBucket>,
    pub node_id: NodeId,
}

impl RoutingTable {
    /// Creates a new routing table for a given node ID.
    pub fn new(node_id: NodeId) -> Self {
        RoutingTable {
            buckets: (0..256).map(|_| KBucket::new()).collect(),
            node_id,
        }
    }

    /// Updates the routing table with a given node information.
    pub fn update(&mut self, node: NodeId, addr: SocketAddr) {
        let distance = self.node_id.distance(&node);
        let bucket_index = distance.0.iter().position(|&x| x != 0).unwrap_or(255);
        self.buckets[bucket_index].update(node, addr);
        println!("Updated routing table with node {:?} at distance {}", node, bucket_index);
    }

    pub fn get_closest_nodes(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut all_nodes: Vec<_> = self
            .buckets
            .iter()
            .flat_map(|bucket| {
                bucket
                    .entries
                    .iter()
                    .map(|entry| (entry.node_id, entry.addr))
            })
            .collect();

        all_nodes.sort_by_key(|(node_id, _)| node_id.distance(target));
        let closest_nodes = all_nodes.into_iter().take(count).collect();
        println!("Retrieved closest nodes to {:?}: {:?}", target, closest_nodes);
        closest_nodes
    }
}

/// Represents different types of messages used in the Kademlia protocol.
#[derive(Serialize, Deserialize, Debug)]
enum Message {
    Ping { sender: NodeId },
    Pong { sender: NodeId },
    FindNode { sender: NodeId, target: NodeId },
    FindNodeResponse { nodes: Vec<(NodeId, SocketAddr)> },
}

/// Custom error type for Kademlia-related errors.
#[derive(Error, Debug)]
pub enum KademliaError {
    #[error("Network error: {0}")]
    NetworkError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] bincode::Error),
    #[error("Unexpected message received")]
    UnexpectedMessage,
    #[error("Node not found")]
    NodeNotFound,
}

/// Represents a Kademlia node with networking and routing capabilities.
pub struct KademliaNode {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub routing_table: RoutingTable,
}

impl KademliaNode {
    /// Creates a new Kademlia node with a random ID.
    pub async fn new(addr: SocketAddr) -> Self {
        let id = NodeId::new();
        println!("Creating new KademliaNode with ID: {:?} and address: {}", id, addr);
        KademliaNode {
            id: id.clone(),
            addr,
            routing_table: RoutingTable::new(id),
        }
    }


    /// Starts the node, listening for incoming connections and handling messages.
    pub async fn start(&self) -> Result<(), KademliaError> {
        let listener = TcpListener::bind(self.addr).await?;
        println!("Node {} listening on: {}", self.id, self.addr);

        loop {
            let (socket, _) = listener.accept().await?;
            let node_id = self.id;
            tokio::spawn(async move {
                if let Err(e) = KademliaNode::handle_connection(socket, node_id).await {
                    println!("Error handling connection: {:?}", e);
                }
            });
        }
    }

    /// Handles incoming connections and processes Kademlia messages.
    async fn handle_connection(
        mut socket: TcpStream,
        node_id: NodeId,
    ) -> Result<(), KademliaError> {
        let mut buf = [0; 1024];
        let n = socket.read(&mut buf).await?;
        let message: Message = bincode::deserialize(&buf[..n])?;

        match message {
            Message::Ping { sender } => {
                println!("Received PING from: {:?}", sender);
                let response = Message::Pong { sender: node_id };
                let serialized = bincode::serialize(&response)?;
                socket.write_all(&serialized).await?;
            }
            Message::FindNode { sender, target } => {
                println!("Received FIND_NODE for {:?} from: {:?}", target, sender);
                // In a real implementation, we would search the routing table here
                let response = Message::FindNodeResponse { nodes: vec![] };
                let serialized = bincode::serialize(&response)?;
                socket.write_all(&serialized).await?;
            }
            _ => {
                println!("Unexpected message: {:?}", message);
                return Err(KademliaError::UnexpectedMessage);
            }
        }

        Ok(())
    }

    /// Sends a ping message to a target node.
    pub async fn ping(&self, target: SocketAddr) -> Result<(), KademliaError> {
        println!("Pinging node at {}", target);
        let mut stream = TcpStream::connect(target).await?;
        let message = Message::Ping { sender: self.id };
        let serialized = bincode::serialize(&message)?;
        stream.write_all(&serialized).await?;

        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await?;
        let response: Message = bincode::deserialize(&buf[..n])?;

        match response {
            Message::Pong { sender } => {
                println!("Received PONG from: {:?}", sender);
                Ok(())
            }
            _ => {
                println!("Unexpected response to PING");
                Err(KademliaError::UnexpectedMessage)
            }
        }
    }

    /// Bootstraps the node using a list of known bootstrap nodes.
    pub async fn bootstrap(
        &mut self,
        bootstrap_nodes: Vec<SocketAddr>,
    ) -> Result<(), KademliaError> {
        let mut discovered_nodes: HashSet<SocketAddr> = HashSet::new();

        for &addr in &bootstrap_nodes {
            if let Ok(()) = self.ping(addr).await {
                discovered_nodes.insert(addr);
            }
        }

        // Create a vector of copies of the discovered nodes
        let sock_addr_vec: Vec<SocketAddr> = discovered_nodes.iter().cloned().collect();

        // Perform FIND_NODE for our own ID to populate routing table
        let target = self.id;
        for addr in sock_addr_vec {
            if let Ok(nodes) = self.find_node(addr, &target).await {
                for (node_id, node_addr) in nodes {
                    self.routing_table.update(node_id, node_addr);
                    discovered_nodes.insert(node_addr);
                }
            }
        }

        Ok(())
    }

    /// Sends a FIND_NODE message to a target address and returns the found nodes.
    async fn find_node(
        &self,
        target_addr: SocketAddr,
        node_id: &NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, KademliaError> {
        let mut stream = TcpStream::connect(target_addr).await?;
        let message = Message::FindNode {
            sender: self.id,
            target: *node_id,
        };
        let serialized = bincode::serialize(&message)?;
        stream.write_all(&serialized).await?;

        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await?;
        let response: Message = bincode::deserialize(&buf[..n])?;

        match response {
            Message::FindNodeResponse { nodes } => Ok(nodes),
            _ => Err(KademliaError::UnexpectedMessage),
        }
    }

    /// Updates the routing table with a given node information.
    pub async fn update_routing_table(&mut self, node_id: NodeId, addr: SocketAddr) {
        self.routing_table.update(node_id, addr);
    }

    /// Retrieves the closest nodes to a target node ID.
    pub fn get_closest_nodes(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)> {
        self.routing_table.get_closest_nodes(target, count)
    }
}
