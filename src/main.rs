// src/main.rs

use log::{debug, error, info, warn};
use tokio::time::Duration;

use bincode::{deserialize, serialize, ErrorKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Instant, SystemTime};
use std::u64::MAX;

const K: usize = 20; // Maximum number of nodes in a k-bucket
const ALPHA: usize = 3; // Number of parallel lookups
const BOOTSTRAP_NODES: [&str; 1] = ["127.0.0.1:33333"]; // Hardcoded bootstrap node

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

#[derive(Clone, Serialize, Deserialize)]
struct KBucketEntry {
    node_id: NodeId,
    addr: SocketAddr,
    last_seen: SystemTime,
}

struct KBucket {
    entries: Vec<KBucketEntry>,
}

impl KBucket {
    fn new() -> Self {
        KBucket {
            entries: Vec::with_capacity(K),
        }
    }

    fn update(&mut self, node_id: NodeId, addr: SocketAddr) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.node_id == node_id)
        {
            let mut entry = self.entries.remove(index);
            entry.last_seen = SystemTime::now();
            self.entries.push(entry);
            info!("Updated node {:#?} in k-bucket", node_id);
        } else if self.entries.len() < K {
            self.entries.push(KBucketEntry {
                node_id,
                addr,
                last_seen: SystemTime::now(),
            });
            info!("Added new node {:#?} to k-bucket", node_id);
        } else {
            // Implement node eviction policy
            // Use a separate scope to find the oldest entry and its index
            let (oldest_index, should_evict) = {
                let oldest = self.entries
                    .iter()
                    .enumerate()
                    .min_by_key(|&(_, entry)| entry.last_seen)
                    .expect("There should be at least one entry in the bucket");

                let should_evict = match oldest.1.last_seen.elapsed() {
                    Ok(elapsed) => elapsed > Duration::from_secs(3600), // 1 hour
                    Err(_) => false,
                };

                (oldest.0, should_evict)
            };

            // Now that the immutable borrow is out of scope, we can mutate self.entries
            if should_evict {
                let evicted_node_id = self.entries[oldest_index].node_id;
                self.entries.remove(oldest_index);
                self.entries.push(KBucketEntry { node_id, addr, last_seen: SystemTime::now() });
                info!("Evicted oldest node {:?} and added new node {:?}", evicted_node_id, node_id);
            } else {
                let oldest_entry = &self.entries[oldest_index];
                warn!("Oldest node {:?} was not evicted as it was recently seen", oldest_entry.node_id);
            }

        }
    }
}

pub struct RoutingTable {
    buckets: Vec<KBucket>,
    node_id: NodeId,
}

impl RoutingTable {
    fn new(node_id: NodeId) -> Self {
        RoutingTable {
            buckets: (0..256).map(|_| KBucket::new()).collect(),
            node_id,
        }
    }

    fn update(&mut self, node: NodeId, addr: SocketAddr) {
        let distance = self.node_id.distance(&node);
        let bucket_index = distance.0.iter().position(|&x| x != 0).unwrap_or(255);
        self.buckets[bucket_index].update(node, addr);
        debug!("Updated routing table for node {:#?}", node);
    }

    fn find_closest(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)> {
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

pub struct KademliaNode {
    id: NodeId,
    addr: SocketAddr,
    routing_table: RoutingTable,
    storage: HashMap<Vec<u8>, Vec<u8>>,
    socket: UdpSocket,
}

impl KademliaNode {
    pub fn new(addr: SocketAddr) -> std::io::Result<Self> {
        let id = NodeId::new();
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;

        Ok(KademliaNode {
            id: id.clone(),
            addr,
            routing_table: RoutingTable::new(id),
            storage: HashMap::new(),
            socket,
        })
    }

    pub fn bootstrap(&mut self) -> std::io::Result<()> {
        for &bootstrap_addr in BOOTSTRAP_NODES.iter() {
            match bootstrap_addr.parse() {
                Ok(addr) => {
                    if let Err(e) = self.ping(addr) {
                        error!(
                            "Failed to ping bootstrap node {:#?}: {:?}",
                            bootstrap_addr, e
                        );
                    }
                }
                Err(e) => error!("Invalid bootstrap address {:#?}: {:?}", bootstrap_addr, e),
            }
        }
        Ok(())
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        let mut buf = [0u8; 1024];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((size, src)) => match deserialize(&buf[..size]) {
                    Ok(message) => {
                        if let Err(e) = self.handle_message(message, src) {
                            error!("Failed to handle message from {:#?}: {:?}", src, e);
                        }
                    }
                    Err(e) => error!("Failed to deserialize message from {:#?}: {:?}", src, e),
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available, continue with other tasks
                    debug!("No data available, continuing");
                }
                Err(e) => {
                    error!("Socket error: {:?}", e);
                    return Err(e);
                }
            }
            // Perform periodic tasks here (e.g., refresh buckets, republish data)
        }
    }

    fn handle_message(&mut self, message: Message, src: SocketAddr) -> std::io::Result<()> {
        match message {
            Message::Ping { sender } => {
                self.routing_table.update(sender, src);
                self.send_message(&Message::Pong { sender: self.id }, src)?;
                info!("Received Ping from {:#?}, responded with Pong", src);
            }
            Message::Pong { sender } => {
                self.routing_table.update(sender, src);
                info!("Received Pong from {:#?}", src);
            }
            Message::Store { key, value } => {
                self.store(&key, &value);
                self.send_message(&Message::Stored, src)?;
                info!("Stored value for key {:?} from {:#?}", key, src);
            }
            Message::FindNode { target } => {
                let nodes = self.find_node(&target);
                self.send_message(&Message::NodesFound(nodes), src)?;
                info!(
                    "Received FindNode from {:#?}, responded with NodesFound",
                    src
                );
            }
            Message::FindValue { key } => match self.find_value(&key) {
                FindValueResult::Value(value) => {
                    self.send_message(&Message::ValueFound(value), src)?;
                    info!("Found value for key {:?} from {:#?}", key, src);
                }
                FindValueResult::Nodes(nodes) => {
                    self.send_message(&Message::NodesFound(nodes), src)?;
                    info!("NodesFound for key {:?} from {:#?}", key, src);
                }
            },
            _ => warn!("Received unknown message type from {:#?}", src), // Handle other message types
        }
        Ok(())
    }

    fn send_message(&self, message: &Message, dst: SocketAddr) -> std::io::Result<()> {
        let serialized = serialize(message).map_err(|e| {
            error!("Failed to serialize message: {:?}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Serialization error")
        })?;
        self.socket.send_to(&serialized, dst)?;
        debug!("Sent message to {:#?}", dst);
        Ok(())
    }

    pub fn store(&mut self, key: &[u8], value: &[u8]) {
        let hash = Self::hash_key(key);
        self.storage.insert(hash.to_vec(), value.to_vec());
        info!("Stored value for key: {:?}", hash);
    }

    fn hash_key(key: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.finalize().to_vec()
    }

    pub fn find_node(&self, target: &NodeId) -> Vec<(NodeId, SocketAddr)> {
        self.routing_table.find_closest(target, K)
    }

    pub fn find_value(&self, key: &[u8]) -> FindValueResult {
        let hash = Self::hash_key(key);
        if let Some(value) = self.storage.get(&hash) {
            FindValueResult::Value(value.clone())
        } else {
            let target = NodeId(hash.try_into().unwrap());
            FindValueResult::Nodes(self.find_node(&target))
        }
    }

    pub fn ping(&mut self, addr: SocketAddr) -> std::io::Result<()> {
        self.send_message(&Message::Ping { sender: self.id }, addr)
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> std::io::Result<()> {
        let hash = Self::hash_key(key);
        let target = NodeId(hash.try_into().unwrap());
        let nodes = self.find_node(&target);

        for (_, addr) in nodes.iter().take(ALPHA) {
            if let Err(e) = self.send_message(
                &Message::Store {
                    key: key.to_vec(),
                    value: value.to_vec(),
                },
                *addr,
            ) {
                error!("Failed to send Store message to {:#?}: {:?}", addr, e);
            }
        }

        self.store(key, value);
        Ok(())
    }

    pub fn get(&mut self, key: &[u8]) -> std::io::Result<Option<Vec<u8>>> {
        if let Some(value) = self.storage.get(&Self::hash_key(key)) {
            return Ok(Some(value.clone()));
        }

        let hash = Self::hash_key(key);
        let target = NodeId(hash.try_into().unwrap());
        let nodes = self.find_node(&target);

        for (_, addr) in nodes.iter().take(ALPHA) {
            if let Err(e) = self.send_message(&Message::FindValue { key: key.to_vec() }, *addr) {
                error!("Failed to send FindValue message to {:#?}: {:?}", addr, e);
            }
        }

        // For simplicity, we're just returning None here
        // In a real implementation, we would wait for responses and return the value if found
        Ok(None)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Ping { sender: NodeId },
    Pong { sender: NodeId },
    Store { key: Vec<u8>, value: Vec<u8> },
    FindNode { target: NodeId },
    FindValue { key: Vec<u8> },
    NodesFound(Vec<(NodeId, SocketAddr)>),
    ValueFound(Vec<u8>),
    Stored,
}

#[derive(Debug)]
pub enum FindValueResult {
    Value(Vec<u8>),
    Nodes(Vec<(NodeId, SocketAddr)>),
}

fn main() -> std::io::Result<()> {
    env_logger::init(); // Initialize the logger
    let addr: SocketAddr = "127.0.0.1:33334".parse().unwrap();
    let mut node = KademliaNode::new(addr)?;
    node.bootstrap()?;
    node.run()
}
