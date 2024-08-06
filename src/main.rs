// src/main.rs

use kademlia_mvp::*;
use tokio::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create two nodes
    let addr1 = "127.0.0.1:8001".parse()?;
    let addr2 = "127.0.0.1:8002".parse()?;
    let mut node1 = KademliaNode::new(addr1).await;
    let node2 = KademliaNode::new(addr2).await;

    // Start node2 in the background
    tokio::spawn(async move {
        node2.start().await.unwrap();
    });

    // Allow some time for node2 to start
    sleep(Duration::from_millis(100)).await;

    // Node1 pings Node2
    node1.ping(addr2).await?;

    // Bootstrap node1 using node2 as the bootstrap node
    node1.bootstrap(vec![addr2]).await?;

    // Print Node1's routing table
    let closest_nodes = node1.get_closest_nodes(&node1.id, 10);
    println!("Node1's closest nodes: {:?}", closest_nodes);

    Ok(())
}
