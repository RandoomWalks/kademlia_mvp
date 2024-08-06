// src/main.rs

use kademlia_mvp::*;
use tokio::time::Duration;
use tokio::time::sleep;
use log::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create two nodes
    let addr1 = "127.0.0.1:8001".parse()?;
    let addr2 = "127.0.0.1:8002".parse()?;
    let mut node1 = KademliaNode::new(addr1).await;
    let node2 = KademliaNode::new(addr2).await;

    // Log the creation of nodes
    println!("Created node1 with address: {}", addr1);
    println!("Created node2 with address: {}", addr2);

    // Start node2 in the background
    tokio::spawn(async move {
        if let Err(e) = node2.start().await {
            println!("Failed to start node2: {:?}", e);
        } else {
            println!("Node2 started successfully");
        }
    });

    // Allow some time for node2 to start
    sleep(Duration::from_millis(100)).await;
    println!("Slept for 100ms to allow node2 to start");

    // Node1 pings Node2
    println!("Node1 attempting to ping Node2 at address: {}", addr2);
    if let Err(e) = node1.ping(addr2).await {
        println!("Node1 failed to ping Node2: {:?}", e);
    } else {
        println!("Node1 successfully pinged Node2");
    }

    // Bootstrap node1 using node2 as the bootstrap node
    println!("Node1 attempting to bootstrap using Node2 at address: {}", addr2);
    if let Err(e) = node1.bootstrap(vec![addr2]).await {
        println!("Node1 failed to bootstrap: {:?}", e);
    } else {
        println!("Node1 bootstrapped successfully using Node2");
    }

    // Print Node1's routing table
    let closest_nodes = node1.get_closest_nodes(&node1.id, 10);
    println!("Node1's closest nodes: {:?}", closest_nodes);
    println!("Node1's closest nodes: {:?}", closest_nodes);

    Ok(())
}
