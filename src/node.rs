use crate::message::KademliaMessage;
use crate::routing::{KBucket, RoutingTable};

use bincode;
use log::{debug, error, info, warn};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};

const K: usize = 20; // Kademlia constant for k-bucket size
const ALPHA: usize = 3; // Number of parallel lookups

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display only the first 4 bytes for brevity
        write!(f, "{:02X?}...", &self.0[0..4])
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId( [u8; 32] );

impl NodeId {
    pub fn new() -> Self {
        NodeId(rand::random())
    }
    pub fn new_val( val:[u8; 32]) -> Self {
        NodeId(val)
    }


    pub fn distance(&self, other: &NodeId) -> [u8; 32] {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.0[i] ^ other.0[i];
        }
        debug!(
            "Calculated distance: {:?} for NodeId: {:?} and {:?}",
            result, self, other
        );
        result
    }

    pub fn short(&self) -> String {
        let id_bytes = &self.0[..4]; // Only take the first 4 bytes for display
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            id_bytes[0], id_bytes[1], id_bytes[2], id_bytes[3]
        )
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub routing_table: Arc<Mutex<RoutingTable>>,
    pub socket: Arc<UdpSocket>,
    pub storage: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
}

impl Node {
    pub async fn new(addr: &str) -> std::io::Result<Self> {
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let actual_addr = socket.local_addr()?;
        let id = NodeId::new();
        info!("Created node with ID {:?} at address: {}", id, actual_addr);

        Ok(Node {
            id,
            addr: actual_addr,
            routing_table: Arc::new(Mutex::new(RoutingTable::new(id))),
            socket,
            storage: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn bootstrap(
        &mut self,
        known_nodes: Vec<SocketAddr>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut rng = rand::thread_rng();
        let mut shuffled_nodes = known_nodes.clone();
        shuffled_nodes.shuffle(&mut rng);

        debug!(
            "Attempting to bootstrap with known nodes: {:?}",
            shuffled_nodes
        );
        for &bootstrap_addr in shuffled_nodes.iter() {
            match self.attempt_bootstrap(bootstrap_addr).await {
                Ok(_) => {
                    info!("Successfully bootstrapped with node at {}", bootstrap_addr);
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        "Failed to bootstrap with node at {}: {:?}",
                        bootstrap_addr, e
                    );
                }
            }
        }

        warn!(
            "Failed to bootstrap with any known nodes. This node may be the first in the network."
        );
        Ok(())
    }

    pub async fn attempt_bootstrap(
        &mut self,
        bootstrap_addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let max_retries = 3;
        let base_delay = Duration::from_secs(1);

        debug!(
            "Attempting bootstrap with {} (max retries: {})",
            bootstrap_addr, max_retries
        );
        for attempt in 0..max_retries {
            match self.ping_and_add_node(bootstrap_addr).await {
                Ok(node_id) => {
                    debug!(
                        "Ping succeeded, performing node lookup for self ID: {:?}",
                        self.id
                    );
                    let discovered_nodes = self.node_lookup(self.id).await?;
                    for (node_id, addr) in discovered_nodes {
                        self.add_node_to_routing_table(node_id, addr).await;
                    }
                    return Ok(());
                }
                Err(e) => {
                    warn!("Bootstrap attempt {} failed: {:?}", attempt + 1, e);
                    if attempt < max_retries - 1 {
                        let delay = base_delay * 2u32.pow(attempt as u32);
                        debug!("Retrying after delay: {:?}", delay);
                        sleep(delay).await;
                    }
                }
            }
        }

        Err("Max bootstrap attempts reached".into())
    }

    pub async fn ping_and_add_node(
        &mut self,
        addr: SocketAddr,
    ) -> Result<NodeId, Box<dyn std::error::Error>> {
        let node_id = self.ping(addr).await?;
        self.add_node_to_routing_table(node_id, addr).await;
        Ok(node_id)
    }

    pub async fn add_node_to_routing_table(&mut self, node_id: NodeId, addr: SocketAddr) {
        debug!("Adding node {:?} at {} to routing table", node_id, addr);
        let mut routing_table = self.routing_table.lock().await;
        if routing_table.add_node((node_id, addr)) {
            info!("Added node {:?} at {} to routing table", node_id, addr);
        }
    }

    pub async fn ping(&self, addr: SocketAddr) -> Result<NodeId, Box<dyn std::error::Error>> {
        let actual_addr = self.resolve_address(addr)?;
        debug!("Pinging node at {} from {}", actual_addr, self.addr);

        let msg = KademliaMessage::Ping(self.id).serialize();
        self.socket.send_to(&msg, actual_addr).await?;
        info!("Sent PING to {}", actual_addr);

        let mut buf = [0u8; 1024];
        let (size, src) =
            tokio::time::timeout(Duration::from_secs(5), self.socket.recv_from(&mut buf)).await??;

        if self.is_same_node(actual_addr, src) {
            match KademliaMessage::deserialize(&buf[..size])? {
                KademliaMessage::Pong(node_id) => {
                    info!("Received PONG from {} with ID {:?}", src, node_id);
                    Ok(node_id)
                }
                _ => {
                    error!("Unexpected response during PING: {:?}", buf);
                    Err("Unexpected response".into())
                }
            }
        } else {
            error!("Received response from unexpected address: {}", src);
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

    pub async fn handle_message(
        &mut self,
        msg: &[u8],
        src: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = KademliaMessage::deserialize(msg)?;
        debug!("Handling message: {:?} from {}", message, src);
        match message {
            KademliaMessage::Ping(sender_id) => {
                info!("Received PING from {} with ID {:?}", src, sender_id);
                self.add_node_to_routing_table(sender_id, src).await;
                let pong = KademliaMessage::Pong(self.id).serialize();
                self.socket.send_to(&pong, src).await?;
                info!("Sent PONG to {}", src);
            }
            KademliaMessage::FindNode(sender_id, target_id) => {
                info!("Received FIND_NODE from {} for target {:?}", src, target_id);
                self.add_node_to_routing_table(sender_id, src).await;
                let closest_nodes = {
                    let routing_table = self.routing_table.lock().await;
                    routing_table.find_closest_nodes(&target_id, K)
                };
                debug!(
                    "Closest nodes found for target {:?}: {:?}",
                    target_id, closest_nodes
                );
                let response =
                    KademliaMessage::FindNodeResponse(self.id, closest_nodes).serialize();
                self.socket.send_to(&response, src).await?;
            }
            KademliaMessage::Store(sender_id, key, value) => {
                info!("Received STORE request from {}", src);
                self.add_node_to_routing_table(sender_id, src).await;
                let mut storage = self.storage.lock().await;
                debug!("Storing key: {:?} with value: {:?}", key, value);
                storage.insert(key, value);
            }
            KademliaMessage::FindValue(sender_id, key) => {
                info!("Received FIND_VALUE request from {}", src);
                self.add_node_to_routing_table(sender_id, src).await;
                let storage = self.storage.lock().await;
                let response = if let Some(value) = storage.get(&key) {
                    debug!("Found value for key: {:?}", key);
                    KademliaMessage::FindValueResponse(self.id, Some(value.clone()), vec![])
                } else {
                    debug!("Value not found, finding closest nodes for key: {:?}", key);
                    let routing_table = self.routing_table.lock().await;
                    let closest_nodes = routing_table.find_closest_nodes(&NodeId::new(), K); // Use a dummy NodeId for now
                    KademliaMessage::FindValueResponse(self.id, None, closest_nodes)
                };
                self.socket.send_to(&response.serialize(), src).await?;
            }
            _ => {
                warn!("Received unexpected message type: {:?}", message);
            }
        }
        Ok(())
    }

    pub async fn node_lookup(
        &self,
        target_id: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, Box<dyn std::error::Error>> {
        debug!("Starting node lookup for target_id: {:?}", target_id);
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
                    debug!("Updated closest nodes during lookup: {:?}", closest_nodes);
                }
                Err(e) => {
                    warn!("Error during node lookup: {:?}", e);
                }
            }
        }

        info!("Completed node lookup for target_id: {:?}", target_id);
        Ok(closest_nodes)
    }

    pub async fn find_node(
        &self,
        id: NodeId,
        addr: SocketAddr,
        target_id: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, Box<dyn std::error::Error>> {
        debug!(
            "Sending FIND_NODE message to {} for target_id: {:?}",
            addr, target_id
        );
        let message = KademliaMessage::FindNode(self.id, target_id).serialize();
        self.socket.send_to(&message, addr).await?;

        let mut buf = [0u8; 1024];
        let (size, src) = self.socket.recv_from(&mut buf).await?;
        debug!("Received response from {}: {:?} bytes", src, size);

        if src == addr {
            if let KademliaMessage::FindNodeResponse(_, nodes) =
                KademliaMessage::deserialize(&buf[..size])?
            {
                debug!("FIND_NODE response with nodes: {:?}", nodes);
                Ok(nodes)
            } else {
                error!("Unexpected response during FIND_NODE");
                Err("Unexpected response".into())
            }
        } else {
            error!("Response from unexpected source: {}", src);
            Err("Response from unexpected source".into())
        }
    }

    pub fn is_same_node(&self, addr1: SocketAddr, addr2: SocketAddr) -> bool {
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

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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

    pub async fn print_routing_table(&self) {
        info!(
            "\nRouting Table for Node {} (Address: {}):",
            self.id, self.addr
        );
        for (bucket_index, bucket) in self.routing_table.lock().await.buckets.iter().enumerate() {
            if !bucket.nodes.is_empty() {
                info!("  Bucket {}:", bucket_index);
                for (node_id, addr) in &bucket.nodes {
                    info!("    NodeId: {}, Address: {}", node_id, addr);
                }
            }
        }
        info!("-----------------------------");
    }

    pub async fn store(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let target_id = NodeId::new(); // Hash the key to get a NodeId in a real implementation
        debug!("Storing key: {:?} with value: {:?}", key, value);
        let closest_nodes = self.node_lookup(target_id).await?;

        for (node_id, addr) in closest_nodes {
            let message = KademliaMessage::Store(self.id, key.clone(), value.clone()).serialize();
            self.socket.send_to(&message, addr).await?;
            info!("Sent STORE message to node {:?} at {}", node_id, addr);
        }

        Ok(())
    }

    pub async fn find_value(
        &self,
        key: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        let target_id = NodeId::new(); // Hash the key to get a NodeId in a real implementation
        debug!("Finding value for key: {:?}", key);
        let closest_nodes = self.node_lookup(target_id).await?;

        for (node_id, addr) in closest_nodes {
            let message = KademliaMessage::FindValue(self.id, key.clone()).serialize();
            self.socket.send_to(&message, addr).await?;
            info!("Sent FIND_VALUE message to node {:?} at {}", node_id, addr);

            let mut buf = [0u8; 1024];
            let (size, src) = self.socket.recv_from(&mut buf).await?;
            debug!("Received {} bytes from {}", size, src);

            if src == addr {
                if let KademliaMessage::FindValueResponse(_, value, _) =
                    KademliaMessage::deserialize(&buf[..size])?
                {
                    if let Some(v) = value {
                        info!("Found value for key: {:?} from node {:?}", key, node_id);
                        return Ok(Some(v));
                    }
                }
            }
        }

        debug!("Value not found for key: {:?}", key);
        Ok(None)
    }
}
