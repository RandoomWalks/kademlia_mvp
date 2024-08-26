use log::{debug, error, info, warn};
use kademlia_mvp::{
    node::KademliaNode,
    utils::{NodeId, Config, BOOTSTRAP_NODES},
    message::Message,
};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use std::sync::Arc;




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


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create a custom configuration
    let config = Config::default();

    // Create multiple nodes
    let mut nodes = Vec::new();
    let mut bootstrap_addr = None;

    for i in 0..5 {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        let addr = socket.local_addr()?;
        println!("Successfully bound to {}", addr);

        let socket = Arc::new(socket);
        match KademliaNode::new(
            socket.clone(),
            Some(config.clone()),
            bootstrap_addr.map_or_else(|| vec![], |addr| vec![addr]),
        ).await {
            Ok((node, _)) => {
                if bootstrap_addr.is_none() {
                    bootstrap_addr = Some(addr);
                }
                nodes.push(node);
                println!("Created node at address: {}", addr);
            },
            Err(e) => {
                eprintln!("Failed to create node at address {}: {:?}", addr, e);
            }
        }
        sleep(Duration::from_millis(100)).await;
    }

    if nodes.is_empty() {
        return Err("Failed to create any nodes".into());
    }

    // Bootstrap nodes
    for node in &mut nodes {
        if let Err(e) = node.bootstrap().await {
            eprintln!("Failed to bootstrap node: {:?}", e);
        }
    }

    // Test node discovery
    println!("Testing node discovery...");
    for (i, node) in nodes.iter().enumerate() {
        println!("Node {} routing table size: {}", i, node.get_routing_table_size());
    }

    // Test data storage and retrieval
    println!("\nTesting data storage and retrieval...");
    let test_key = "test_key".as_bytes().to_vec();
    let test_value = "test_value".as_bytes().to_vec();

    // Store data using the first node
    if let Err(e) = nodes[0].put(&test_key, &test_value).await {
        eprintln!("Failed to store data: {:?}", e);
    } else {
        println!("Data stored by node 0");

        // Retrieve data using other nodes
        for (i, node) in nodes.iter_mut().enumerate().skip(1) {
            match node.get(&test_key).await {
                Ok(Some(value)) => println!("Node {} retrieved value: {:?}", i, String::from_utf8(value)?),
                Ok(None) => println!("Node {} failed to retrieve value: Not found", i),
                Err(e) => println!("Node {} failed to retrieve value: {:?}", i, e),
            }
        }
    }

    Ok(())
}

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // Initialize logging
//     env_logger::init();

//     println!("Running UDP echo test...");
//     udp_echo_test().await?;

//     println!("Attempting to bind to a single socket...");
//     match UdpSocket::bind("127.0.0.1:0").await {
//         Ok(socket) => println!("Successfully bound to {}", socket.local_addr()?),
//         Err(e) => println!("Failed to bind to any port: {:?}", e),
//     }

//     // Create a custom configuration
//     let config = Config::default();

//     // Create multiple nodes
//     let mut nodes = Vec::new();
//     let mut bootstrap_addr = None;

//     for i in 0..10 {
//         let addr: SocketAddr = "127.0.0.1:0".parse()?;
//         match UdpSocket::bind(addr).await {
//             Ok(socket) => {
//                 let actual_addr = socket.local_addr()?;
//                 println!("Successfully bound to {}", actual_addr);
//                 match KademliaNode::new(
//                     actual_addr,
//                     Some(config.clone()),
//                     bootstrap_addr.map_or_else(|| vec![], |addr| vec![addr]),
//                 ).await {
//                     Ok((node, _)) => {
//                         if bootstrap_addr.is_none() {
//                             bootstrap_addr = Some(actual_addr);
//                         }
//                         nodes.push(node);
//                         println!("Created node at address: {}", actual_addr);
//                         if nodes.len() >= 5 {
//                             break;
//                         }
//                     },
//                     Err(e) => {
//                         eprintln!("Failed to create node at address {}: {:?}", actual_addr, e);
//                     }
//                 }
//             },
//             Err(e) => {
//                 eprintln!("Failed to bind to address {}: {:?}", addr, e);
//             }
//         }
//         sleep(Duration::from_millis(100)).await;
//     }

//     if nodes.is_empty() {
//         return Err("Failed to create any nodes".into());
//     }

//     // ... (rest of the code remains the same)

//     Ok(())
// }



// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // Initialize logging
//     env_logger::init();

//     // Create a custom configuration
//     let config = Config::default();

//     // Create multiple nodes
//     let mut nodes = Vec::new();
//     let mut bootstrap_addr = None;

//     for i in 0..10 { // Increase the number of attempts
//         let addr: SocketAddr = "127.0.0.1:0".parse()?; // Use port 0 for dynamic allocation
//         match UdpSocket::bind(addr).await {
//             Ok(socket) => {
//                 let actual_addr = socket.local_addr()?;
//                 match KademliaNode::new(
//                     actual_addr,
//                     Some(config.clone()),
//                     bootstrap_addr.map_or_else(|| vec![], |addr| vec![addr]),
//                 ).await {
//                     Ok((node, _)) => {
//                         if bootstrap_addr.is_none() {
//                             bootstrap_addr = Some(actual_addr);
//                         }
//                         nodes.push(node);
//                         println!("Created node at address: {}", actual_addr);
//                         if nodes.len() >= 5 {
//                             break; // Stop after creating 5 nodes
//                         }
//                     },
//                     Err(e) => {
//                         eprintln!("Failed to create node at address {}: {:?}", actual_addr, e);
//                     }
//                 }
//             },
//             Err(e) => {
//                 eprintln!("Failed to bind to address {}: {:?}", addr, e);
//             }
//         }
//         sleep(Duration::from_millis(100)).await; // Add a small delay between attempts
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
//         println!("Node {} routing table size: {}", i, node.get_routing_table_size());
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
//                 Ok(Some(value)) => println!("Node {} retrieved value: {:?}", i, String::from_utf8(value)?),
//                 Ok(None) => println!("Node {} failed to retrieve value: Not found", i),
//                 Err(e) => println!("Node {} failed to retrieve value: {:?}", i, e),
//             }
//         }
//     }

//     // Test node failure and recovery
//     println!("\nTesting node failure and recovery...");
//     // We can't actually remove a node, but we can simulate it by not querying it
//     println!("Simulating Node 2 going offline (by not querying it)");

//     // Try to retrieve data again
//     for (i, node) in nodes.iter_mut().enumerate() {
//         if i != 2 { // Skip node 2 to simulate it being offline
//             match node.get(&test_key).await {
//                 Ok(Some(value)) => println!("Node {} retrieved value after simulated failure: {:?}", i, String::from_utf8(value)?),
//                 Ok(None) => println!("Node {} failed to retrieve value after simulated failure: Not found", i),
//                 Err(e) => println!("Node {} failed to retrieve value after simulated failure: {:?}", i, e),
//             }
//         }
//     }

//     // Test cache performance
//     println!("\nTesting cache performance...");
//     let start = std::time::Instant::now();
//     nodes[0].get(&test_key).await?;
//     let first_lookup_time = start.elapsed();
//     println!("First lookup time: {:?}", first_lookup_time);

//     let start = std::time::Instant::now();
//     nodes[0].get(&test_key).await?;
//     let second_lookup_time = start.elapsed();
//     println!("Second lookup time (should be faster due to caching): {:?}", second_lookup_time);

//     // Test cache eviction
//     println!("\nTesting cache eviction...");
//     for i in 0..1100 { // Assuming cache size is 1000
//         let key = format!("key_{}", i).into_bytes();
//         let value = format!("value_{}", i).into_bytes();
//         nodes[0].put(&key, &value).await?;
//     }
//     println!("Stored 1100 items, cache size should be 1000");
//     println!("Cached entries count: {}", nodes[0].get_cached_entries_count().await);

//     Ok(())
// }