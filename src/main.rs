// use kademlia_mvp::{
//     message::Message,
//     node::KademliaNode,
//     utils::{Config, NodeId, BOOTSTRAP_NODES},
// };
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

// use log::{debug, error, info, warn};
// use std::collections::HashMap;
use std::net::IpAddr;
// use std::sync::Arc;
// use tokio::net::UdpSocket;
use tokio::sync::Mutex;
// use tokio::time::{sleep, Duration};
use futures::future::join_all;
use std::collections::VecDeque;
use tokio::time::timeout;
// use std::net::SocketAddr;
// use std::sync::Arc;
// use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NodeId([u8; 32]);

impl NodeId {
    fn new() -> Self {
        NodeId(rand::random())
    }
}

#[derive(Clone)]
struct Node {
    id: NodeId,
    addr: SocketAddr,
    routing_table: Arc<Mutex<HashMap<NodeId, SocketAddr>>>,
    socket: Arc<UdpSocket>,
    buckets: Arc<Mutex<Vec<Vec<SocketAddr>>>>, // Add this line
}

impl Node {
    async fn new(addr: &str) -> std::io::Result<Self> {
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let actual_addr = socket.local_addr()?;
        println!("Bound to address: {}", actual_addr);

        // Initialize 128 empty buckets (one for each possible XOR distance range)
        let buckets = Arc::new(Mutex::new(vec![Vec::new(); 128]));

        Ok(Node {
            id: NodeId::new(),
            addr: actual_addr,
            routing_table: Arc::new(Mutex::new(HashMap::new())),
            socket,
            buckets, // Add this line
        })
    }


    // Add a peer to the appropriate bucket based on XOR distance
    async fn add_to_bucket(&self, peer_id: NodeId, peer_addr: SocketAddr) {
        let bucket_index = self.get_bucket_index(peer_id);
        let mut buckets = self.buckets.lock().await;

        // Add the peer to the appropriate bucket
        buckets[bucket_index].push(peer_addr);
    }
    
    // Method to print the contents of the routing table
    pub async fn print_routing_table(&self) {
        let routing_table = self.routing_table.lock().await;
        println!(
            "\nRouting Table for Node {:?} (Address: {}):",
            self.id, self.addr
        );
        for (node_id, addr) in routing_table.iter() {
            println!("  NodeId: {:?}, Address: {}", node_id, addr);
        }
    }
    // Enhanced bootstrap function to support multiple bootstrap nodes
    async fn bootstrap(
        &self,
        known_nodes: Vec<SocketAddr>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Node {:?} starting bootstrap process", self.id);

        // Iterate over the list of bootstrap nodes
        for bootstrap_addr in known_nodes {
            let mut backoff = 1; // Linear backoff for retries
            for _ in 0..3 {
                // Try 3 times per node
                match self.ping(bootstrap_addr).await {
                    Ok(()) => {
                        println!("Successfully pinged bootstrap node at {}", bootstrap_addr);
                        self.routing_table
                            .lock()
                            .await
                            .insert(NodeId::new(), bootstrap_addr);
                        return Ok(());
                    }
                    Err(e) => {
                        println!(
                            "Failed to ping bootstrap node at {}: {:?}",
                            bootstrap_addr, e
                        );
                        println!("Retrying in {} seconds...", backoff);
                        sleep(Duration::from_secs(backoff)).await;
                        backoff *= 2; // Exponential backoff
                    }
                }
            }
        }

        // If no bootstrap nodes responded, this node starts a new network.
        println!("No bootstrap nodes responded. This node is the first in the network.");
        Ok(())
    }

    async fn ping(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        println!("Pinging node at {} from {}", addr, self.addr);
        let msg = b"PING";
        self.socket.send_to(msg, addr).await?;
        println!("Sent PING to {}", addr);

        let mut buf = [0u8; 1024];
        match timeout(Duration::from_secs(5), self.socket.recv_from(&mut buf)).await {
            Ok(Ok((_, src))) => {
                println!("Received response from {}", src);
                if self.is_same_node(addr, src) {
                    println!("Received pong from {}", src);
                    Ok(())
                } else {
                    Err(format!("Received response from unexpected address: {}", src).into())
                }
            }
            Ok(Err(e)) => Err(format!("Error receiving response: {:?}", e).into()),
            Err(_) => Err("Ping timed out".into()),
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

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Node {:?} running on {}", self.id, self.addr);
        let mut buf = [0u8; 1024];
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((size, src)) => {
                    println!("Received {} bytes from {}", size, src);
                    if &buf[..4] == b"PING" {
                        println!("Received PING from {}", src);
                        match self.socket.send_to(b"PONG", src).await {
                            Ok(_) => println!("Sent PONG to {}", src),
                            Err(e) => println!("Error sending PONG to {}: {:?}", src, e),
                        }
                    }
                }
                Err(e) => println!("Error receiving data: {:?}", e),
            }
        }
    }
    // Calculate the XOR distance between this node's ID and another node's ID
    fn calculate_xor_distance(&self, other_id: NodeId) -> u128 {
        // We use the first 16 bytes (128 bits) of the NodeId for simplicity
        let self_id = u128::from_le_bytes(self.id.0[..16].try_into().unwrap());
        let other_id = u128::from_le_bytes(other_id.0[..16].try_into().unwrap());
        self_id ^ other_id
    }
    // Determine the k-bucket index based on the XOR distance
    fn get_bucket_index(&self, other_id: NodeId) -> usize {
        let distance = self.calculate_xor_distance(other_id);
        127 - distance.leading_zeros() as usize // This gives us the correct bucket index
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Node 1 (the initial node)
    let node1 = Node::new("0.0.0.0:0").await?;
    println!("Created node1 at address: {}", node1.addr);
    node1.bootstrap(vec![]).await?;

    let node1_clone = node1.clone();
    let node1_handle = tokio::spawn(async move {
        if let Err(e) = node1_clone.run().await {
            eprintln!("Node1 error: {:?}", e);
        }
    });

    sleep(Duration::from_secs(1)).await;

    // Node 2
    let node2 = Node::new("0.0.0.0:0").await?;
    println!("Created node2 at address: {}", node2.addr);
    node2.bootstrap(vec![node1.addr]).await?;

    let node2_clone = node2.clone();
    let node2_handle = tokio::spawn(async move {
        if let Err(e) = node2_clone.run().await {
            eprintln!("Node2 error: {:?}", e);
        }
    });
d
    sleep(Duration::from_secs(1)).await;

    // Node 3
    let node3 = Node::new("0.0.0.0:0").await?;
    println!("Created node3 at address: {}", node3.addr);
    node3.bootstrap(vec![node1.addr, node2.addr]).await?;

    let node3_clone = node3.clone();
    let node3_handle = tokio::spawn(async move {
        if let Err(e) = node3_clone.run().await {
            eprintln!("Node3 error: {:?}", e);
        }
    });

    sleep(Duration::from_secs(1)).await;

    // Node 4
    let node4 = Node::new("0.0.0.0:0").await?;
    println!("Created node4 at address: {}", node4.addr);
    node4
        .bootstrap(vec![node1.addr, node2.addr, node3.addr])
        .await?;
    let node4_clone = node4.clone();

    let node4_handle = tokio::spawn(async move {
        if let Err(e) = node4_clone.run().await {
            eprintln!("Node4 error: {:?}", e);
        }
    });

    // Periodically print routing tables for all nodes
    tokio::spawn(async move {
        loop {
            node1.print_routing_table().await;
            node2.print_routing_table().await;
            node3.print_routing_table().await;
            node4.print_routing_table().await;

            // Wait for a while before printing again
            sleep(Duration::from_secs(10)).await;
        }
    });

    // Wait for all nodes to finish (they won’t in this example)
    node1_handle.await?;
    node2_handle.await?;
    node3_handle.await?;
    node4_handle.await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_calculate_xor_distance() {
        // Create two nodes with known IDs for testing
        let node1_id = NodeId([0xAA; 32]); // Example Node ID with all bytes set to 0xAA
        let node2_id = NodeId([0x55; 32]); // Example Node ID with all bytes set to 0x55

        let node1 = Node {
            id: node1_id,
            addr: "127.0.0.1:0".parse().unwrap(),
            routing_table: Arc::new(Mutex::new(HashMap::new())),
            socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
        };

        let xor_distance = node1.calculate_xor_distance(node2_id);

        // Expected distance: 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF (for first 16 bytes)
        assert_eq!(xor_distance, u128::MAX);
    }

    #[tokio::test]
    async fn test_get_bucket_index() {
        // Create two nodes with known IDs for testing
        let node1_id = NodeId([0x00; 32]); // All zeros
        let node2_id = NodeId([0x80; 32]); // Leading bit set to 1 (in the first byte)

        let node1 = Node {
            id: node1_id,
            addr: "127.0.0.1:0".parse().unwrap(),
            routing_table: Arc::new(Mutex::new(HashMap::new())),
            socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
        };

        let bucket_index = node1.get_bucket_index(node2_id);

        // Expected bucket index: 127 (because leading zeros are 0, highest distance)
        assert_eq!(bucket_index, 127);
    }

    #[tokio::test]
    async fn test_add_to_bucket() {
        // Create nodes with known IDs for testing
        let node1_id = NodeId([0x00; 32]); // All zeros
        let node2_id = NodeId([0x80; 32]); // Leading bit set to 1
        let node3_id = NodeId([0x40; 32]); // Second-highest bit set to 1

        let node1 = Node::new("127.0.0.1:0").await.unwrap();

        // Simulate adding nodes to the bucket
        node1
            .add_to_bucket(node2_id, "127.0.0.1:10000".parse().unwrap())
            .await;
        node1
            .add_to_bucket(node3_id, "127.0.0.1:10001".parse().unwrap())
            .await;

        let buckets = node1.buckets.lock().await;

        // Check bucket 127 for node2 (highest distance)
        assert_eq!(buckets[127], vec!["127.0.0.1:10000".parse().unwrap()]);

        // Check bucket 126 for node3 (next highest distance)
        assert_eq!(buckets[126], vec!["127.0.0.1:10001".parse().unwrap()]);
    }
}

// #[tokio::test]
// async fn test_add_to_bucket() {
//     // Create nodes with known IDs for testing
//     let node1_id = NodeId([0x00; 32]); // All zeros
//     let node2_id = NodeId([0x80; 32]); // Leading bit set to 1
//     let node3_id = NodeId([0x40; 32]); // Second-highest bit set to 1

//     let node1 = Node::new("127.0.0.1:0").await.unwrap();

//     // Simulate adding nodes to the bucket
//     node1
//         .add_to_bucket(node2_id, "127.0.0.1:10000".parse().unwrap())
//         .await;
//     node1
//         .add_to_bucket(node3_id, "127.0.0.1:10001".parse().unwrap())
//         .await;

//     let buckets = node1.buckets.lock().await;

//     // Check bucket 127 for node2 (highest distance)
//     assert_eq!(buckets[127], vec!["127.0.0.1:10000".parse().unwrap()]);

//     // Check bucket 126 for node3 (next highest distance)
//     assert_eq!(buckets[126], vec!["127.0.0.1:10001".parse().unwrap()]);
// }
