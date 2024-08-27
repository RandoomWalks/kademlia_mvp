use kademlia_mvp::{
    message::Message,
    node::KademliaNode,
    utils::{Config, NodeId, BOOTSTRAP_NODES},
};
use log::{debug, error, info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use std::collections::HashMap;


async fn udp_echo_test() -> Result<(), Box<dyn std::error::Error>> {
    let server_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let server_addr = server_socket.local_addr()?;
    println!("UDP echo server listening on {}", server_addr);

    let client_socket = UdpSocket::bind("127.0.0.1:0").await?;
    println!("UDP echo client bound to {}", client_socket.local_addr()?);

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            let (len, addr) = server_socket.recv_from(&mut buf).await.unwrap();
            server_socket.send_to(&buf[..len], addr).await.unwrap();
        }
    });

    let message = b"Hello, UDP!";
    client_socket.send_to(message, server_addr).await?;

    let mut buf = [0u8; 1024];
    let (len, _) = client_socket.recv_from(&mut buf).await?;

    assert_eq!(&buf[..len], message);
    println!("UDP echo test successful!");

    Ok(())
}

async fn test_udp_basic() -> Result<(), Box<dyn std::error::Error>> {
    // Create a server socket bound to a dynamic port
    let server_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let server_addr = server_socket.local_addr()?;
    println!("Server listening on: {}", server_addr);

    // Spawn the server task
    let server_socket_clone = server_socket.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            // Receive a message
            let (len, src) = server_socket_clone.recv_from(&mut buf).await.unwrap();
            println!("Server received: {}", String::from_utf8_lossy(&buf[..len]));

            // Echo the message back to the client
            server_socket_clone.send_to(&buf[..len], src).await.unwrap();
            println!("Server echoed the message back to {}", src);
        }
    });

    // Create a client socket bound to a dynamic port
    let client_socket = UdpSocket::bind("127.0.0.1:0").await?;
    println!("Client bound to: {}", client_socket.local_addr()?);

    // The client sends a message to the server
    let message = b"Hello, UDP!";
    client_socket.send_to(message, server_addr).await?;
    println!("Client sent: {}", String::from_utf8_lossy(message));

    // The client waits to receive the echoed message
    let mut buf = [0u8; 1024];
    let (len, _) = client_socket.recv_from(&mut buf).await?;
    println!("Client received: {}", String::from_utf8_lossy(&buf[..len]));

    // Verify the echoed message is correct
    assert_eq!(&buf[..len], message);
    println!("UDP communication test successful!");

    Ok(())
}

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // Initialize logging
//     env_logger::init();

//     // Create a custom configuration
//     let config = Config::default();

//     // Create multiple nodes
//     let mut nodes = Vec::new();
//     let mut bootstrap_addr = None;

//     for i in 0..5 {
//         let socket = UdpSocket::bind("127.0.0.1:0").await?;
//         let addr = socket.local_addr()?;
//         println!("Successfully bound to {}", addr);

//         let socket = Arc::new(socket);
//         match KademliaNode::new(
//             socket.clone(),
//             Some(config.clone()),
//             bootstrap_addr.map_or_else(|| vec![], |addr| vec![addr]),
//         )
//         .await
//         {
//             Ok((node, _)) => {
//                 // Add a small delay before allowing other nodes to join.
//                 if bootstrap_addr.is_none() {
//                     bootstrap_addr = Some(addr);
//                     sleep(Duration::from_secs(1)).await; // Let the first node stabilize
//                 }
                
//                 nodes.push(node);
//                 println!("Created node at address: {}", addr);
                
//             }
//             Err(e) => {
//                 eprintln!("Failed to create node at address {}: {:?}", addr, e);
//             }
//         }
//         sleep(Duration::from_millis(100)).await;
//     }

//     if nodes.is_empty() {
//         return Err("Failed to create any nodes".into());
//     }

//     // Bootstrap nodes
//     for node in &mut nodes {
//         if let Err(e) = node.bootstrap().await {
//             eprintln!("Failed to bootstrap node: {:?}", e);
//         }
//     }

//     // Test node discovery
//     println!("Testing node discovery...");
//     for (i, node) in nodes.iter().enumerate() {
//         println!(
//             "Node {} routing table size: {}",
//             i,
//             node.get_routing_table_size()
//         );
//     }

//     // Test data storage and retrieval
//     println!("\nTesting data storage and retrieval...");
//     let test_key = "test_key".as_bytes().to_vec();
//     let test_value = "test_value".as_bytes().to_vec();

//     // Store data using the first node
//     if let Err(e) = nodes[0].put(&test_key, &test_value).await {
//         eprintln!("Failed to store data: {:?}", e);
//     } else {
//         println!("Data stored by node 0");

//         // Retrieve data using other nodes
//         for (i, node) in nodes.iter_mut().enumerate().skip(1) {
//             match node.get(&test_key).await {
//                 Ok(Some(value)) => println!(
//                     "Node {} retrieved value: {:?}",
//                     i,
//                     String::from_utf8(value)?
//                 ),
//                 Ok(None) => println!("Node {} failed to retrieve value: Not found", i),
//                 Err(e) => println!("Node {} failed to retrieve value: {:?}", i, e),
//             }
//         }
//     }

//     Ok(())
// }
// use std::collections::HashMap;
use std::net::{IpAddr};
// use std::sync::Arc;
// use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{timeout};

// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// struct NodeId([u8; 32]);

// impl NodeId {
//     fn new() -> Self {
//         NodeId(rand::random())
//     }
// }

// #[derive(Clone)]
// struct Node {
//     id: NodeId,
//     addr: SocketAddr,
//     routing_table: Arc<Mutex<HashMap<NodeId, SocketAddr>>>,
//     socket: Arc<UdpSocket>,
// }

// impl Node {
//     async fn new(addr: &str) -> std::io::Result<Self> {
//         let socket = Arc::new(UdpSocket::bind(addr).await?);
//         let actual_addr = socket.local_addr()?;
//         println!("Bound to address: {}", actual_addr);
//         Ok(Node {
//             id: NodeId::new(),
//             addr: actual_addr,
//             routing_table: Arc::new(Mutex::new(HashMap::new())),
//             socket,
//         })
//     }

//     async fn bootstrap(&self, known_node: Option<SocketAddr>) -> Result<(), Box<dyn std::error::Error>> {
//         println!("Node {:?} starting bootstrap process", self.id);

//         if let Some(bootstrap_addr) = known_node {
//             for _ in 0..3 {  // Try 3 times
//                 match self.ping(bootstrap_addr).await {
//                     Ok(()) => {
//                         println!("Successfully pinged bootstrap node at {}", bootstrap_addr);
//                         self.routing_table.lock().await.insert(NodeId::new(), bootstrap_addr);
//                         return Ok(());
//                     }
//                     Err(e) => {
//                         println!("Failed to ping bootstrap node at {}: {:?}", bootstrap_addr, e);
//                         sleep(Duration::from_secs(1)).await;
//                     }
//                 }
//             }
//         }

//         println!("No bootstrap nodes responded. This node is the first in the network.");
//         Ok(())
//     }

//     async fn ping(&self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
//         println!("Pinging node at {} from {}", addr, self.addr);
//         let msg = b"PING";
//         self.socket.send_to(msg, addr).await?;
//         println!("Sent PING to {}", addr);

//         let mut buf = [0u8; 1024];
//         match timeout(Duration::from_secs(5), self.socket.recv_from(&mut buf)).await {
//             Ok(Ok((_, src))) => {
//                 println!("Received response from {}", src);
//                 if self.is_same_node(addr, src) {
//                     println!("Received pong from {}", src);
//                     Ok(())
//                 } else {
//                     Err(format!("Received response from unexpected address: {}", src).into())
//                 }
//             }
//             Ok(Err(e)) => Err(format!("Error receiving response: {:?}", e).into()),
//             Err(_) => Err("Ping timed out".into()),
//         }
//     }

//     fn is_same_node(&self, addr1: SocketAddr, addr2: SocketAddr) -> bool {
//         if addr1 == addr2 {
//             return true;
//         }
//         match (addr1.ip(), addr2.ip()) {
//             (IpAddr::V4(ip1), IpAddr::V4(ip2)) => {
//                 (ip1.is_unspecified() || ip2.is_unspecified() || ip1 == ip2) && addr1.port() == addr2.port()
//             }
//             _ => false,
//         }
//     }

//     async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
//         println!("Node {:?} running on {}", self.id, self.addr);
//         let mut buf = [0u8; 1024];
//         loop {
//             match self.socket.recv_from(&mut buf).await {
//                 Ok((size, src)) => {
//                     println!("Received {} bytes from {}", size, src);
//                     if &buf[..4] == b"PING" {
//                         println!("Received PING from {}", src);
//                         match self.socket.send_to(b"PONG", src).await {
//                             Ok(_) => println!("Sent PONG to {}", src),
//                             Err(e) => println!("Error sending PONG to {}: {:?}", src, e),
//                         }
//                     }
//                 }
//                 Err(e) => println!("Error receiving data: {:?}", e),
//             }
//         }
//     }
// }

// use tokio::time::sleep;
// use std::time::Duration;

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // Initialize logger with RUST_LOG settings
//     env_logger::init();

//     let (mut node1, shutdown_sender1) = KademliaNode::new(None, vec![]).await?;
//     let node1_addr = node1.addr;
    
//     let node1_handle = tokio::spawn(async move {
//         if let Err(e) = node1.run().await {
//             eprintln!("Node1 error: {:?}", e);
//         }
//     });

//     sleep(Duration::from_secs(1)).await;

//     let (mut node2, shutdown_sender2) = KademliaNode::new(None, vec![node1_addr]).await?;
//     let node2_handle = tokio::spawn(async move {
//         if let Err(e) = node2.run().await {
//             eprintln!("Node2 error: {:?}", e);
//         }
//     });

//     // Wait for both nodes to finish or handle shutdown
//     tokio::try_join!(node1_handle, node2_handle)?;

//     Ok(())
// }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger with RUST_LOG settings
    env_logger::init();

    // Initialize node1
    // let (mut node1, shutdown_sender1) = KademliaNode::new(None, vec![]).await?;
    let node1 = Arc::new(Mutex::new(KademliaNode::new(None, vec![]).await?.0));

    let node1_addr = node1.lock().await.addr;
    
    // Run node1 in the main thread
    let node1_clone = Arc::clone(&node1);
    let node1_handle = tokio::spawn(async move {
        if let Err(e) = node1_clone.lock().await.run().await {
            eprintln!("Node1 error: {:?}", e);
        }
    });
    sleep(Duration::from_secs(1)).await;
    
    // Initialize node2 with node1 as a bootstrap node
    let (mut node2, shutdown_sender2) = KademliaNode::new(None, vec![node1_addr]).await?;
    node2.bootstrap().await?;  // Trigger bootstrap process for node 2

    // Store a value on node 2
    let key = b"demo_key".to_vec();
    let value = b"demo_value".to_vec();
    node2.put(&key, &value).await?;

    // Retrieve the value using node 1
    sleep(Duration::from_secs(1)).await; // Allow time for replication
    
    // Run retrieval using a separate instance of node1 for demonstration purposes
    if let Some(retrieved_value) = node1.lock().await.get(&key).await? {
        println!(
            "Node 1 successfully retrieved value: {:?}",
            String::from_utf8(retrieved_value).unwrap()
        );
    } else {
        println!("Node 1 failed to retrieve value.");
    }

    // Run node2 in a separate task
    tokio::spawn(async move {
        if let Err(e) = node2.run().await {
            eprintln!("Node2 error: {:?}", e);
        }
    });


    let node3 = Arc::new(Mutex::new(KademliaNode::new(None, vec![]).await?.0));
    let node3_addr = node3.lock().await.addr;

    let node3_clone = Arc::clone(&node3);
    let node3_handle = tokio::spawn(async move {
        if let Err(e) = node3_clone.lock().await.run().await {
            eprintln!("Node3 error: {:?}", e);
        }
    });


    // Prevent the main task from exiting immediately
    tokio::signal::ctrl_c().await?;
    Ok(())
}
