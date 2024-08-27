// use kademlia_mvp::{
//     message::Message,
//     node::KademliaNode,
//     utils::{Config, NodeId, BOOTSTRAP_NODES},
// };
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::collections::HashSet;
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
use bincode;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tokio::time::timeout;
// use std::net::SocketAddr;
// use std::sync::Arc;
// use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Debug, Clone)]
enum KademliaMessage {
    Ping(NodeId),
    Pong(NodeId),
    FindNode(NodeId, NodeId), // (sender_id, target_id)
    FindNodeResponse(NodeId, Vec<(NodeId, SocketAddr)>),
    FindValue(NodeId, Vec<u8>),
    FindValueResponse(NodeId, Option<Vec<u8>>, Vec<(NodeId, SocketAddr)>),
    Store(NodeId, Vec<u8>, Vec<u8>),
}

impl KademliaMessage {
    fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn deserialize(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        bincode::deserialize(data).map_err(|e| e.into())
    }
    // fn serialize(&self) -> Vec<u8> {
    //     match self {
    //         KademliaMessage::Ping => vec![0],
    //         KademliaMessage::Pong => vec![1],
    //         KademliaMessage::FindNode(_,_) => vec![2],
    //         KademliaMessage::FindNodeResponse(_) => vec![3],
    //         KademliaMessage::FindValue(_) => vec![4],
    //         KademliaMessage::FindValueResponse(_, _) => vec![5],
    //         KademliaMessage::Store(_, _) => vec![6],
    //     }
    // }

    // fn deserialize(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
    //     match data.get(0) {
    //         Some(0) => Ok(KademliaMessage::Ping),
    //         Some(1) => Ok(KademliaMessage::Pong),
    //         Some(2) => Ok(KademliaMessage::FindNode(NodeId::new(),NodeId::new())), // Use a dummy NodeId for now
    //         Some(3) => Ok(KademliaMessage::FindNodeResponse(vec![])), // Use an empty vec for now
    //         Some(4) => Ok(KademliaMessage::FindValue(vec![])),       // Use an empty vec for now
    //         Some(5) => Ok(KademliaMessage::FindValueResponse(None, vec![])), // Use None and an empty vec for now
    //         Some(6) => Ok(KademliaMessage::Store(vec![], vec![])), // Use empty vecs for now
    //         _ => Err("Invalid message".into()),
    //     }
    // }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    buckets: Arc<Mutex<Vec<Vec<(NodeId, SocketAddr)>>>>, // Change this line
}

impl Node {
    async fn new(addr: &str) -> std::io::Result<Self> {
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let actual_addr = socket.local_addr()?;
        println!("Bound to address: {}", actual_addr);

        // Initialize 256 empty buckets (one for each possible XOR distance range)
        let buckets = Arc::new(Mutex::new(vec![Vec::new(); 256]));

        Ok(Node {
            id: NodeId::new(),
            addr: actual_addr,
            routing_table: Arc::new(Mutex::new(HashMap::new())),
            socket,
            buckets, // Add this line
        })
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
    async fn find_closest_nodes(
        &self,
        target_id: NodeId,
        count: usize,
    ) -> Vec<(NodeId, SocketAddr)> {
        let routing_table = self.routing_table.lock().await;
        let mut nodes: Vec<_> = routing_table
            .iter()
            .map(|(&id, &addr)| (id, addr, self.xor_distance(id, target_id)))
            .collect();

        nodes.sort_by_key(|(_, _, distance)| distance.clone());
        nodes.truncate(count);

        nodes.into_iter().map(|(id, addr, _)| (id, addr)).collect()
    }
    async fn find_node(
        &self,
        id: NodeId,
        addr: SocketAddr,
        target_id: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, Box<dyn std::error::Error>> {
        let message = KademliaMessage::FindNode(id, target_id);
        let serialized = message.serialize();
        self.socket.send_to(&serialized, addr).await?;

        let mut buf = [0u8; 1024];
        let (_, src) = self.socket.recv_from(&mut buf).await?;

        if src == addr {
            if let KademliaMessage::FindNodeResponse(target_id,nodes) = KademliaMessage::deserialize(&buf)? {
                Ok(nodes)
            } else {
                Err("Unexpected response".into())
            }
        } else {
            Err("Response from unexpected source".into())
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
                // Add the sender to the routing table
                let sender_id = NodeId::new(); // We need to get the actual sender ID
                self.add_node_to_routing_table(sender_id, src).await;
                let pong = KademliaMessage::Pong(self.id).serialize();

                self.socket.send_to(&pong, src).await?;
                println!("Sent PONG to {}", src);
            }
            KademliaMessage::Pong(sender_id) => {
                println!("Received PONG from {} with ID {:?}", src, sender_id);
                self.add_node_to_routing_table(sender_id, src).await;
            }
            KademliaMessage::FindNode(sender_id, target_id) => {
                let closest = self.find_closest_nodes(target_id, 20).await;
                let response = KademliaMessage::FindNodeResponse(sender_id, closest).serialize();
                self.socket.send_to(&response, src).await?;
            }
            // Implement other message handlers...
            _ => {
                // Handle other message types
            }
        }
        Ok(())
    }

    // async fn handle_message_old(
    //     &mut self,
    //     msg: &[u8],
    //     src: SocketAddr,
    // ) -> Result<(), Box<dyn std::error::Error>> {
    //     let message = KademliaMessage::deserialize(msg)?;
    //     match message {
    //         KademliaMessage::Ping(sender_id) => {
    //             println!("Received PING from {} with ID {:?}", src, sender_id);
    //             self.add_node_to_routing_table(sender_id, src).await;
    //             let pong = KademliaMessage::Pong(self.id).serialize();
    //             self.socket.send_to(&pong, src).await?;
    //             println!("Sent PONG to {}", src);
    //         }
    //         KademliaMessage::Pong(sender_id) => {
    //             println!("Received PONG from {} with ID {:?}", src, sender_id);
    //             self.add_node_to_routing_table(sender_id, src).await;
    //         }
    //         // ... other message handlers remain the same for now
    //         _ => {
    //             println!("Received unhandled message type from {}", src);
    //         }
    //     }
    //     Ok(())
    // }

    async fn add_node_to_routing_table(&mut self, node_id: NodeId, addr: SocketAddr) {
        let mut routing_table = self.routing_table.lock().await;

        if !routing_table.contains_key(&node_id) {
            routing_table.insert(node_id, addr);

            // Determine the appropriate k-bucket
            let bucket_index = self.calculate_bucket_index(node_id);
            let mut buckets = self.buckets.lock().await;

            // Add to k-bucket if not full (assuming k=20)
            if buckets[bucket_index].len() < 20 {
                buckets[bucket_index].push((node_id, addr));
            } else {
                // Implement k-bucket splitting or replacement strategy here
                // For now, we'll just replace the last element
                buckets[bucket_index].pop();
                buckets[bucket_index].push((node_id, addr));
            }
        }
    }

    fn calculate_bucket_index(&self, other_id: NodeId) -> usize {
        let distance = self
            .id
            .0
            .iter()
            .zip(other_id.0.iter())
            .map(|(a, b)| a ^ b)
            .fold(0u8, |acc, x| acc.max(x));
        255 - distance.leading_zeros() as usize
    }

    // Enhanced bootstrap function to support multiple bootstrap nodes
    async fn bootstrap(
        &mut self,
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
                        let bootstrap_node_id = NodeId::new(); // In reality, you'd get this from the ping response
                        self.add_node_to_routing_table(bootstrap_node_id, bootstrap_addr)
                            .await;

                        // Perform a node lookup for our own ID to populate our routing table
                        let discovered_nodes = self.node_lookup(self.id).await;
                        for (node_id, addr) in discovered_nodes {
                            self.add_node_to_routing_table(node_id, addr).await;
                        }

                        // self.routing_table
                        //     .lock()
                        //     .await
                        //     .insert(NodeId::new(), bootstrap_addr);

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

    async fn ping(&mut self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        println!("Pinging node at {} from {}", addr, self.addr);
        let msg = KademliaMessage::Ping(self.id).serialize();
        self.socket.send_to(&msg, addr).await?;
        println!("Sent PING to {}", addr);

        let mut buf = [0u8; 1024];
        match timeout(Duration::from_secs(5), self.socket.recv_from(&mut buf)).await {
            Ok(Ok((size, src))) => {
                println!("Received {} bytes from {}", size, src);
                if self.is_same_node(addr, src) {
                    match KademliaMessage::deserialize(&buf[..size])? {
                        KademliaMessage::Pong(node_id) => {
                            println!("Received PONG from {} with ID {:?}", src, node_id);
                            self.add_node_to_routing_table(node_id, src).await;
                            Ok(())
                        }
                        _ => Err("Unexpected response".into()),
                    }
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
    // Add a peer to the appropriate bucket based on XOR distance
    async fn add_to_bucket(&mut self, peer_id: NodeId, peer_addr: SocketAddr) {
        let bucket_index = self.get_bucket_index(peer_id);
        let mut buckets = self.buckets.lock().await;

        // Check if the node is already in the bucket
        if !buckets[bucket_index].iter().any(|(id, _)| *id == peer_id) {
            // If the bucket is not full, simply add the new node
            if buckets[bucket_index].len() < 20 {
                // Assuming k=20
                buckets[bucket_index].push((peer_id, peer_addr));
            } else {
                // If the bucket is full, implement a replacement strategy
                // For now, we'll just replace the oldest node
                buckets[bucket_index].remove(0);
                buckets[bucket_index].push((peer_id, peer_addr));
            }
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

    // Print the contents of all buckets
    async fn print_buckets(&self) {
        let buckets = self.buckets.lock().await;
        for (index, bucket) in buckets.iter().enumerate() {
            if !bucket.is_empty() {
                println!("Bucket {}: {:?}", index, bucket);
            }
        }
    }
    async fn node_lookup(&mut self, target_id: NodeId) -> Vec<(NodeId, SocketAddr)> {
        let mut closest_nodes = self.find_closest_nodes(target_id, 3).await;
        if closest_nodes.is_empty() {
            return Vec::new();
        }

        let mut asked = HashSet::new();
        let mut to_ask = closest_nodes.clone();

        while !to_ask.is_empty() {
            let node = to_ask.pop().unwrap();
            asked.insert(node.0);

            if let Ok(closer_nodes) = self.find_node(node.0, node.1, target_id).await {
                for closer_node in closer_nodes {
                    if !asked.contains(&closer_node.0) {
                        closest_nodes.push(closer_node);
                        to_ask.push(closer_node);
                    }
                }
                closest_nodes.sort_by_key(|n| self.xor_distance(n.0, target_id));
                closest_nodes.truncate(20); // k = 20
                to_ask.sort_by_key(|n| self.xor_distance(n.0, target_id));
            }
        }

        closest_nodes
    }
    // async fn find_node(
    //     &self,
    //     id: NodeId,
    //     addr: SocketAddr,
    //     target_id: NodeId,
    // ) -> Result<Vec<(NodeId, SocketAddr)>, Box<dyn std::error::Error>> {
    //     // Implement FIND_NODE RPC here
    //     // This should send a FIND_NODE message to the specified node and return the result
    // }

    fn xor_distance(&self, id1: NodeId, id2: NodeId) -> Vec<u8> {
        id1.0.iter().zip(id2.0.iter()).map(|(a, b)| a ^ b).collect()
    }
}

async fn print_detailed_routing_table(node: &Node) {
    let routing_table = node.routing_table.lock().await;
    println!("Routing Table for Node {:?} (Address: {}):", node.id, node.addr);
    for (node_id, addr) in routing_table.iter() {
        println!("  NodeId: {:?}, Address: {}", node_id, addr);
    }
    if routing_table.is_empty() {
        println!("  (empty)");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Node 1 (the initial node)
    let mut node1: Node = Node::new("0.0.0.0:0").await?;
    println!("Created node1 at address: {}", node1.addr);
    node1.bootstrap(vec![]).await?;

    let mut node1_clone = node1.clone();
    let node1_handle = tokio::spawn(async move {
        if let Err(e) = node1_clone.run().await {
            eprintln!("Node1 error: {:?}", e);
        }
    });

    sleep(Duration::from_secs(1)).await;

    // Node 2
    let mut node2 = Node::new("0.0.0.0:0").await?;
    println!("Created node2 at address: {}", node2.addr);
    node2.bootstrap(vec![node1.addr]).await?;

    let mut node2_clone = node2.clone();
    let node2_handle = tokio::spawn(async move {
        if let Err(e) = node2_clone.run().await {
            eprintln!("Node2 error: {:?}", e);
        }
    });

    sleep(Duration::from_secs(1)).await;

    // Node 3
    let mut node3 = Node::new("0.0.0.0:0").await?;
    println!("Created node3 at address: {}", node3.addr);
    node3.bootstrap(vec![node1.addr, node2.addr]).await?;

    let mut node3_clone = node3.clone();
    let node3_handle = tokio::spawn(async move {
        if let Err(e) = node3_clone.run().await {
            eprintln!("Node3 error: {:?}", e);
        }
    });

    sleep(Duration::from_secs(1)).await;

    // Node 4
    let mut node4 = Node::new("0.0.0.0:0").await?;
    println!("Created node4 at address: {}", node4.addr);
    node4
        .bootstrap(vec![node1.addr, node2.addr, node3.addr])
        .await?;
    let mut node4_clone = node4.clone();

    let node4_handle = tokio::spawn(async move {
        if let Err(e) = node4_clone.run().await {
            eprintln!("Node4 error: {:?}", e);
        }
    });

    // Periodically print routing tables for all nodes
    tokio::spawn(async move {
        loop {
            println!("\n--- Routing Table Status ---");
            print_detailed_routing_table(&node1).await;
            print_detailed_routing_table(&node2).await;
            print_detailed_routing_table(&node3).await;
            print_detailed_routing_table(&node4).await;
            println!("-----------------------------\n");

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
    use std::net::SocketAddr;
    use tokio::runtime::Runtime;
    use tokio::test;

    // Helper function to create a test node
    async fn create_test_node() -> Node {
        Node::new("127.0.0.1:0").await.unwrap()
    }

    #[test]
    async fn test_node_id_generation() {
        let node_id1 = NodeId::new();
        let node_id2 = NodeId::new();
        assert_ne!(node_id1, node_id2);
    }

    // #[test]
    // async fn test_xor_distance() {
    //     let rt = Runtime::new().unwrap();
    //     rt.block_on(async {
    //         let node = create_test_node().await;
    //         let id1 = NodeId([0; 32]);
    //         let id2 = NodeId([0xFF; 32]);
    //         let distance = node.calculate_xor_distance(id1, id2);
    //         assert_eq!(distance, u128::MAX);
    //     });
    // }

    #[test]
    async fn test_bucket_index() {
        let node_id = NodeId([0; 32]);
        let other_id = NodeId([0x80; 32]);

        // Create a mock node for testing
        let node = Node {
            id: node_id,
            addr: "127.0.0.1:0".parse().unwrap(),
            routing_table: Arc::new(Mutex::new(HashMap::new())),
            socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
            buckets: Arc::new(Mutex::new(vec![Vec::new(); 256])),
        };

        assert_eq!(node.get_bucket_index(other_id), 255);
    }

    #[tokio::test]
    async fn test_ping_pong() {
        let node1 = Arc::new(Mutex::new(create_test_node().await));
        let node2 = Arc::new(Mutex::new(create_test_node().await));

        // Start both nodes
        let node1_clone = Arc::clone(&node1);
        let node1_handle = tokio::spawn(async move {
            node1_clone.lock().await.run().await.unwrap();
        });

        let node2_clone = Arc::clone(&node2);
        let node2_handle = tokio::spawn(async move {
            node2_clone.lock().await.run().await.unwrap();
        });

        // Give the nodes some time to start up
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Perform the ping with a timeout
        let ping_result = timeout(
            Duration::from_secs(5),
            node1.lock().await.ping(node2.lock().await.addr),
        )
        .await;

        assert!(ping_result.is_ok(), "Ping timed out");
        assert!(ping_result.unwrap().is_ok(), "Ping failed");

        // Stop the nodes
        node1_handle.abort();
        node2_handle.abort();
    }

    #[tokio::test]
    async fn test_add_node_to_routing_table() {
        let mut node1 = create_test_node().await;
        let node2 = create_test_node().await;

        node1.add_node_to_routing_table(node2.id, node2.addr).await;

        let routing_table = node1.routing_table.lock().await;
        assert!(routing_table.contains_key(&node2.id));
    }

    #[tokio::test]
    async fn test_find_closest_nodes() {
        let mut node = create_test_node().await;
        let mut added_nodes = Vec::new();

        // Add 10 random nodes to the routing table
        for _ in 0..10 {
            let other_node = create_test_node().await;
            node.add_node_to_routing_table(other_node.id, other_node.addr)
                .await;
            added_nodes.push(other_node);
        }

        let target_id = NodeId::new();
        let closest = node.find_closest_nodes(target_id, 5).await;

        assert_eq!(closest.len(), 5);
        // Verify that the returned nodes are actually the closest ones
        // (You might need to implement a method to calculate the actual closest nodes for comparison)
    }

    use tokio::time::timeout;

    #[tokio::test]
    async fn test_bootstrap() {
        let bootstrap_node = Arc::new(Mutex::new(create_test_node().await));
        let mut new_node = create_test_node().await;

        // Start the bootstrap node
        let bootstrap_node_clone = Arc::clone(&bootstrap_node);
        let bootstrap_handle = tokio::spawn(async move {
            bootstrap_node_clone.lock().await.run().await.unwrap();
        });

        // Give the bootstrap node some time to start up
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Perform bootstrap with a timeout
        let bootstrap_result = timeout(
            Duration::from_secs(5),
            new_node.bootstrap(vec![bootstrap_node.lock().await.addr]),
        )
        .await;

        assert!(bootstrap_result.is_ok(), "Bootstrap timed out");
        assert!(bootstrap_result.unwrap().is_ok(), "Bootstrap failed");

        let routing_table = new_node.routing_table.lock().await;
        assert!(!routing_table.is_empty());

        // Stop the bootstrap node
        bootstrap_handle.abort();
    }
    // #[tokio::test]
    // async fn test_handle_find_node() {
    //     let node1 = create_test_node().await;
    //     let node2 = create_test_node().await;

    //     let target_id = NodeId::new();
    //     let request = KademliaMessage::FindNode(target_id);
    //     let response = node1.handle_message(request, node2.addr).await;

    //     match response {
    //         KademliaMessage::FindNodeResponse(nodes) => {
    //             assert!(!nodes.is_empty());
    //             // Add more specific assertions about the returned nodes
    //         }
    //         _ => panic!("Unexpected response type"),
    //     }
    // }

    // #[tokio::test]
    // async fn test_node_lookup() {
    //     let mut nodes = Vec::new();
    //     for _ in 0..20 {
    //         nodes.push(create_test_node().await);
    //     }

    //     // Connect all nodes in a ring topology
    //     for i in 0..nodes.len() {
    //         let next = (i + 1) % nodes.len();
    //         let (left, right) = nodes.split_at_mut(next);
    //         let (current_node, next_node) = if i < next {
    //             (&mut left[i], &mut right[0])
    //         } else {
    //             (&mut right[i - next], &mut left[0])
    //         };

    //         current_node
    //             .add_node_to_routing_table(next_node.id, next_node.addr)
    //             .await;

    //         // nodes[i]
    //         //     .add_node_to_routing_table(nodes[next].id, nodes[next].addr)
    //         //     .await;
    //     }

    //     let target_id = NodeId::new();
    //     let result = nodes[0].node_lookup(target_id).await;

    //     assert!(!result.is_empty());
    //     // Add more specific assertions about the lookup result
    // }
    #[tokio::test]
    async fn test_node_lookup() {
        let mut nodes = Vec::new();
        for _ in 0..20 {
            nodes.push(Arc::new(Mutex::new(
                Node::new("127.0.0.1:0").await.unwrap(),
            )));
        }

        // Connect all nodes in a ring topology
        for i in 0..nodes.len() {
            let next = (i + 1) % nodes.len();
            let mut node = nodes[i].lock().await;
            let next_node = nodes[next].lock().await;
            node.add_node_to_routing_table(next_node.id, next_node.addr)
                .await;
        }

        // Perform node lookup
        let target_id = NodeId::new();
        let result = nodes[0].lock().await.node_lookup(target_id).await;

        assert!(!result.is_empty());
        // Add more specific assertions about the lookup result
    }

    #[test]
    async fn test_calculate_xor_distance() {
        // Create two nodes with known IDs for testing
        let node1_id = NodeId([0xAA; 32]); // Example Node ID with all bytes set to 0xAA
        let node2_id = NodeId([0x55; 32]); // Example Node ID with all bytes set to 0x55

        // Initialize 128 empty buckets (one for each possible XOR distance range)
        let buckets = Arc::new(Mutex::new(vec![Vec::new(); 128]));

        let node1 = Node {
            id: node1_id,
            addr: "127.0.0.1:0".parse().unwrap(),
            routing_table: Arc::new(Mutex::new(HashMap::new())),
            socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
            buckets: buckets,
        };

        let xor_distance = node1.calculate_xor_distance(node2_id);

        // Expected distance: 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF (for first 16 bytes)
        assert_eq!(xor_distance, u128::MAX);
    }

    // #[tokio::test]
    // async fn test_get_bucket_index() {
    //     // Create two nodes with known IDs for testing
    //     let node1_id = NodeId([0x00; 32]); // All zeros
    //     let node2_id = NodeId([0x80; 32]); // Leading bit set to 1 (in the first byte)

    //     // Initialize 128 empty buckets (one for each possible XOR distance range)
    //     let buckets = Arc::new(Mutex::new(vec![Vec::new(); 128]));

    //     let node1 = Node {
    //         id: node1_id,
    //         addr: "127.0.0.1:0".parse().unwrap(),
    //         routing_table: Arc::new(Mutex::new(HashMap::new())),
    //         socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
    //         buckets: buckets,
    //     };

    //     let bucket_index = node1.get_bucket_index(node2_id);

    //     // Expected bucket index: 127 (because leading zeros are 0, highest distance)
    //     assert_eq!(bucket_index, 127);
    // }

    #[tokio::test]
    async fn test_add_to_bucket() {
        let mut node = Node::new("127.0.0.1:0").await.unwrap();
        let peer_id = NodeId([0x80; 32]);
        let peer_addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();

        node.add_to_bucket(peer_id, peer_addr).await;

        let buckets = node.buckets.lock().await;
        let bucket_index = node.get_bucket_index(peer_id);
        assert!(buckets[bucket_index].contains(&(peer_id, peer_addr)));
    }
}
