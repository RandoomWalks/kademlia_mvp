use bincode;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};
// use std::net::{IpAddr, SocketAddr};
// use tokio::time::{sleep, Duration};
use rand::seq::SliceRandom;

const K: usize = 20; // Kademlia constant for k-bucket size
const ALPHA: usize = 3; // Number of parallel lookups

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NodeId([u8; 32]);

impl NodeId {
    fn new() -> Self {
        NodeId(rand::random())
    }

    fn distance(&self, other: &NodeId) -> [u8; 32] {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.0[i] ^ other.0[i];
        }
        result
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum KademliaMessage {
    Ping(NodeId),
    Pong(NodeId),
    FindNode(NodeId, NodeId), // (sender_id, target_id)
    FindNodeResponse(NodeId, Vec<(NodeId, SocketAddr)>),
    Store(NodeId, Vec<u8>, Vec<u8>), // (sender_id, key, value)
    FindValue(NodeId, Vec<u8>),      // (sender_id, key)
    FindValueResponse(NodeId, Option<Vec<u8>>, Vec<(NodeId, SocketAddr)>), // (sender_id, value, closest_nodes)
}

impl KademliaMessage {
    fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn deserialize(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        bincode::deserialize(data).map_err(|e| e.into())
    }
}

struct KBucket {
    nodes: VecDeque<(NodeId, SocketAddr)>,
}

impl KBucket {
    fn new() -> Self {
        KBucket {
            nodes: VecDeque::with_capacity(K),
        }
    }

    fn add_node(&mut self, node: (NodeId, SocketAddr)) -> bool {
        if let Some(index) = self.nodes.iter().position(|x| x.0 == node.0) {
            // Move existing node to the front
            let existing = self.nodes.remove(index).unwrap();
            self.nodes.push_front(existing);
            false
        } else if self.nodes.len() < K {
            // Add new node to the front
            self.nodes.push_front(node);
            true
        } else {
            // Bucket is full, new node is ignored
            false
        }
    }
}

struct RoutingTable {
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

    fn add_node(&mut self, node: (NodeId, SocketAddr)) -> bool {
        let index = self.bucket_index(&node.0);
        self.buckets[index].add_node(node)
    }

    fn bucket_index(&self, other: &NodeId) -> usize {
        let distance = self.node_id.distance(other);
        distance
            .iter()
            .position(|&b| b != 0)
            .map_or(255, |i| i * 8 + distance[i].leading_zeros() as usize)
    }

    fn find_closest_nodes(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut closest: Vec<_> = self
            .buckets
            .iter()
            .flat_map(|bucket| bucket.nodes.iter().cloned())
            .collect();
        closest.sort_by_key(|(id, _)| id.distance(target));
        closest.truncate(count);
        closest
    }
}

struct Node {
    id: NodeId,
    addr: SocketAddr,
    routing_table: Arc<Mutex<RoutingTable>>,
    socket: Arc<UdpSocket>,
    storage: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
}

impl Node {
    async fn new(addr: &str) -> std::io::Result<Self> {
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let actual_addr = socket.local_addr()?;
        let id = NodeId::new();
        println!("Created node with ID {:?} at address: {}", id, actual_addr);

        Ok(Node {
            id,
            addr: actual_addr,
            routing_table: Arc::new(Mutex::new(RoutingTable::new(id))),
            socket,
            storage: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    async fn bootstrap(
        &mut self,
        known_nodes: Vec<SocketAddr>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut rng = rand::thread_rng();
        let mut shuffled_nodes = known_nodes.clone();
        shuffled_nodes.shuffle(&mut rng);

        for &bootstrap_addr in shuffled_nodes.iter() {
            match self.attempt_bootstrap(bootstrap_addr).await {
                Ok(_) => {
                    println!("Successfully bootstrapped with node at {}", bootstrap_addr);
                    return Ok(());
                }
                Err(e) => {
                    println!(
                        "Failed to bootstrap with node at {}: {:?}",
                        bootstrap_addr, e
                    );
                }
            }
        }

        println!(
            "Failed to bootstrap with any known nodes. This node may be the first in the network."
        );
        Ok(())
    }

    async fn attempt_bootstrap(
        &mut self,
        bootstrap_addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let max_retries = 3;
        let base_delay = Duration::from_secs(1);

        for attempt in 0..max_retries {
            match self.ping_and_add_node(bootstrap_addr).await {
                Ok(node_id) => {
                    // Perform a node lookup for our own ID to populate our routing table
                    let discovered_nodes = self.node_lookup(self.id).await?;
                    for (node_id, addr) in discovered_nodes {
                        self.add_node_to_routing_table(node_id, addr).await;
                    }
                    return Ok(());
                }
                Err(e) => {
                    println!("Bootstrap attempt {} failed: {:?}", attempt + 1, e);
                    if attempt < max_retries - 1 {
                        let delay = base_delay * 2u32.pow(attempt as u32);
                        sleep(delay).await;
                    }
                }
            }
        }

        Err("Max bootstrap attempts reached".into())
    }
    async fn ping_and_add_node(
        &mut self,
        addr: SocketAddr,
    ) -> Result<NodeId, Box<dyn std::error::Error>> {
        let node_id = self.ping(addr).await?;
        self.add_node_to_routing_table(node_id, addr).await;
        Ok(node_id)
    }

    async fn add_node_to_routing_table(&mut self, node_id: NodeId, addr: SocketAddr) {
        let mut routing_table = self.routing_table.lock().await;
        if routing_table.add_node((node_id, addr)) {
            println!("Added node {:?} at {} to routing table", node_id, addr);
        }
    }

    async fn ping(&self, addr: SocketAddr) -> Result<NodeId, Box<dyn std::error::Error>> {
        let actual_addr = self.resolve_address(addr)?;
        println!("Pinging node at {} from {}", actual_addr, self.addr);

        let msg = KademliaMessage::Ping(self.id).serialize();
        self.socket.send_to(&msg, actual_addr).await?;
        println!("Sent PING to {}", actual_addr);

        let mut buf = [0u8; 1024];
        let (size, src) =
            tokio::time::timeout(Duration::from_secs(5), self.socket.recv_from(&mut buf)).await??;

        if self.is_same_node(actual_addr, src) {
            match KademliaMessage::deserialize(&buf[..size])? {
                KademliaMessage::Pong(node_id) => {
                    println!("Received PONG from {} with ID {:?}", src, node_id);
                    Ok(node_id)
                }
                _ => Err("Unexpected response".into()),
            }
        } else {
            Err(format!("Received response from unexpected address: {}", src).into())
        }
    }

    fn resolve_address(&self, addr: SocketAddr) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        match addr.ip() {
            IpAddr::V4(ip) if ip.is_unspecified() => Ok(SocketAddr::new(
                IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                addr.port(),
            )),
            IpAddr::V6(ip) if ip.is_unspecified() => Ok(SocketAddr::new(
                IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
                addr.port(),
            )),
            _ => Ok(addr),
        }
    }

    async fn handle_message(
        &mut self,
        msg: &[u8],
        src: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = KademliaMessage::deserialize(msg)?;
        match message {
            KademliaMessage::Ping(sender_id) => {
                println!("Received PING from {} with ID {:?}", src, sender_id);
                self.add_node_to_routing_table(sender_id, src).await;
                let pong = KademliaMessage::Pong(self.id).serialize();
                self.socket.send_to(&pong, src).await?;
                println!("Sent PONG to {}", src);
            }
            KademliaMessage::FindNode(sender_id, target_id) => {
                println!("Received FIND_NODE from {} for target {:?}", src, target_id);
                self.add_node_to_routing_table(sender_id, src).await;
                let closest_nodes = {
                    let routing_table = self.routing_table.lock().await;
                    routing_table.find_closest_nodes(&target_id, K)
                };
                let response =
                    KademliaMessage::FindNodeResponse(self.id, closest_nodes).serialize();
                self.socket.send_to(&response, src).await?;
            }
            KademliaMessage::Store(sender_id, key, value) => {
                println!("Received STORE request from {}", src);
                self.add_node_to_routing_table(sender_id, src).await;
                let mut storage = self.storage.lock().await;
                storage.insert(key, value);
            }
            KademliaMessage::FindValue(sender_id, key) => {
                println!("Received FIND_VALUE request from {}", src);
                self.add_node_to_routing_table(sender_id, src).await;
                let storage = self.storage.lock().await;
                let response = if let Some(value) = storage.get(&key) {
                    KademliaMessage::FindValueResponse(self.id, Some(value.clone()), vec![])
                } else {
                    let routing_table = self.routing_table.lock().await;
                    let closest_nodes = routing_table.find_closest_nodes(&NodeId::new(), K); // Use a dummy NodeId for now
                    KademliaMessage::FindValueResponse(self.id, None, closest_nodes)
                };
                self.socket.send_to(&response.serialize(), src).await?;
            }
            _ => {
                // Handle other message types if needed
            }
        }
        Ok(())
    }

    async fn node_lookup(
        &self,
        target_id: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, Box<dyn std::error::Error>> {
        let mut closest_nodes = {
            let routing_table = self.routing_table.lock().await;
            routing_table.find_closest_nodes(&target_id, ALPHA)
        };
        let mut asked = HashSet::new();
        let mut to_ask = VecDeque::from(closest_nodes.clone());

        while let Some(node) = to_ask.pop_front() {
            if asked.contains(&node.0) {
                continue;
            }
            asked.insert(node.0);

            match self.find_node(node.0, node.1, target_id).await {
                Ok(closer_nodes) => {
                    for closer_node in closer_nodes {
                        if !asked.contains(&closer_node.0) {
                            closest_nodes.push(closer_node);
                            to_ask.push_back(closer_node);
                        }
                    }
                    closest_nodes.sort_by_key(|n| n.0.distance(&target_id));
                    closest_nodes.truncate(K);
                }
                Err(e) => {
                    println!("Error during node lookup: {:?}", e);
                }
            }
        }

        Ok(closest_nodes)
    }

    async fn find_node(
        &self,
        id: NodeId,
        addr: SocketAddr,
        target_id: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, Box<dyn std::error::Error>> {
        let message = KademliaMessage::FindNode(self.id, target_id).serialize();
        self.socket.send_to(&message, addr).await?;

        let mut buf = [0u8; 1024];
        let (size, src) = self.socket.recv_from(&mut buf).await?;

        if src == addr {
            if let KademliaMessage::FindNodeResponse(_, nodes) =
                KademliaMessage::deserialize(&buf[..size])?
            {
                Ok(nodes)
            } else {
                Err("Unexpected response".into())
            }
        } else {
            Err("Response from unexpected source".into())
        }
    }

    fn is_same_node(&self, addr1: SocketAddr, addr2: SocketAddr) -> bool {
        if addr1 == addr2 {
            return true;
        }
        match (addr1.ip(), addr2.ip()) {
            (IpAddr::V4(ip1), IpAddr::V4(ip2)) => {
                (ip1.is_unspecified() || ip2.is_unspecified() || ip1 == ip2)
                    && addr1.port() == addr2.port()
            }
            _ => false,
        }
    }

    async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Node {:?} running on {}", self.id, self.addr);
        let mut buf = [0u8; 1024];
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((size, src)) => {
                    debug!("Received {} bytes from {}", size, src);
                    if let Err(e) = self.handle_message(&buf[..size], src).await {
                        error!("Error handling message: {:?}", e);
                    }
                }
                Err(e) => error!("Error receiving data: {:?}", e),
            }
        }
    }

    async fn print_routing_table(&self) {
        let routing_table = self.routing_table.lock().await;
        println!(
            "\nRouting Table for Node {:?} (Address: {}):",
            self.id, self.addr
        );
        for (i, bucket) in routing_table.buckets.iter().enumerate() {
            if !bucket.nodes.is_empty() {
                println!("  Bucket {}:", i);
                for (node_id, addr) in &bucket.nodes {
                    println!("    NodeId: {:?}, Address: {}", node_id, addr);
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut nodes: Vec<Arc<Mutex<Node>>> = vec![];
    let num_nodes = 4;

    // Create first node
    let first_node = Node::new("127.0.0.1:0").await?;
    let first_node_addr = first_node.addr;
    nodes.push(Arc::new(Mutex::new(first_node)));

    // Create additional nodes
    for i in 1..num_nodes {
        let mut node = Node::new("127.0.0.1:0").await?;
        println!("Created node{} at address: {}", i + 1, node.addr);

        let bootstrap_addrs = vec![first_node_addr];
        node.bootstrap(bootstrap_addrs).await?;

        nodes.push(Arc::new(Mutex::new(node)));
    }
    
    // Create nodes
    // for i in 0..num_nodes {
    //     let mut node: Node = Node::new("0.0.0.0:0").await?;
    //     println!("Created node{} at address: {}", i + 1, node.addr);

    //     if i == 0 {
    //         node.bootstrap(vec![]).await?;
    //     } else {
    //         // let bootstrap_addrs = nodes.iter().map(|n: &Arc<Mutex<Node>>| async {n.lock().await.addr}).collect();
    //         let mut bootstrap_addrs = Vec::new();
    //         for node in &nodes {
    //             let addr = node.lock().await.addr;
    //             bootstrap_addrs.push(addr);
    //         }
    //         node.bootstrap(bootstrap_addrs).await?;
    //     }

    //     nodes.push(Arc::new(Mutex::new(node)));
    // }

    // Run nodes in the background
    let node_handles: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let node_clone: Arc<Mutex<Node>> = Arc::clone(node);
            tokio::spawn(async move {
                if let Err(e) = node_clone.lock().await.run().await {
                    eprintln!("Node {} error: {:?}", i + 1, e);
                }
            })
        })
        .collect();

    // Wait for nodes to initialize
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Print routing tables
    for (i, node) in nodes.iter().enumerate() {
        println!("Routing table for Node {}:", i + 1);
        node.lock().await.print_routing_table().await;
    }

    // Demonstrate node lookup
    let target_id = NodeId::new();
    println!("Performing node lookup for target ID: {:?}", target_id);
    if let Some(node) = nodes.first() {
        let closest_nodes = node.lock().await.node_lookup(target_id).await?;
        println!("Closest nodes to target:");
        for (id, addr) in closest_nodes {
            println!("  NodeId: {:?}, Address: {}", id, addr);
        }
    }

    // Demonstrate value storage and retrieval
    let key = "test_key".as_bytes().to_vec();
    let value = "test_value".as_bytes().to_vec();
    println!("Storing value for key: {:?}", key);
    if let Some(node) = nodes.first() {
        node.lock().await.store(key.clone(), value.clone()).await?;
    }

    // Wait for value to propagate
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("Retrieving value for key: {:?}", key);
    if let Some(node) = nodes.last() {
        match node.lock().await.find_value(key.clone()).await? {
            Some(retrieved_value) => println!("Retrieved value: {:?}", retrieved_value),
            None => println!("Value not found"),
        }
    }

    // Keep the network running
    for handle in node_handles {
        handle.await?;
    }

    Ok(())
}

// Add these methods to the Node impl
impl Node {
    async fn store(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        let target_id = NodeId::new(); // Hash the key to get a NodeId in a real implementation
        let closest_nodes = self.node_lookup(target_id).await?;

        for (node_id, addr) in closest_nodes {
            let message = KademliaMessage::Store(self.id, key.clone(), value.clone()).serialize();
            self.socket.send_to(&message, addr).await?;
        }

        Ok(())
    }

    async fn find_value(
        &self,
        key: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        let target_id = NodeId::new(); // Hash the key to get a NodeId in a real implementation
        let closest_nodes = self.node_lookup(target_id).await?;

        for (node_id, addr) in closest_nodes {
            let message = KademliaMessage::FindValue(self.id, key.clone()).serialize();
            self.socket.send_to(&message, addr).await?;

            let mut buf = [0u8; 1024];
            let (size, src) = self.socket.recv_from(&mut buf).await?;

            if src == addr {
                if let KademliaMessage::FindValueResponse(_, value, _) =
                    KademliaMessage::deserialize(&buf[..size])?
                {
                    if let Some(v) = value {
                        return Ok(Some(v));
                    }
                }
            }
        }

        Ok(None)
    }
}
