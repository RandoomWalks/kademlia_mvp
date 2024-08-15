pub mod node;
pub mod routing_table;
pub mod kbucket;
pub mod message;
pub mod utils;
pub mod cache_prac;
pub mod KVStore;

use log::{info, warn, debug, error};

use node::KademliaNode;
// use tokio::net::SocketAddr;
use std::net::SocketAddr;
use tokio::time::{Duration, sleep};
use crate::utils::{NodeId, BOOTSTRAP_NODES};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;


#[tokio::main]
async fn main() -> std::io::Result<()> {
    
    
    // env_logger::init(); // Initialize the logger
    // info!("Starting Kademlia node...");

    // let addr: SocketAddr = "127.0.0.1:33334".parse().expect("Failed to parse socket address");
    // info!("Parsed socket address: {}", addr);

    // let (mut node, shutdown_sender) = match KademliaNode::new(addr).await {
    //     Ok((node, sender)) => {
    //         info!("Successfully created Kademlia node.");
    //         (node, sender)
    //     }
    //     Err(e) => {
    //         error!("Failed to create Kademlia node: {:?}", e);
    //         return Err(e);
    //     }
    // };

    // info!("Bootstrapping node...");
    // if let Err(e) = node.bootstrap().await {
    //     error!("Bootstrap failed: {:?}", e);
    //     return Err(e);
    // }

    // info!("Running the node...");
    // if let Err(e) = node.run().await {
    //     error!("Node run failed: {:?}", e);
    //     return Err(e);
    // }

    // info!("Node shut down gracefully.");

        // Setup initial addresses for nodes (e.g., for testing purposes)
        let node1_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let node2_addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
    
        // Create two Kademlia nodes for testing
        let (mut node1, shutdown_sender1) = KademliaNode::new(node1_addr).await?;
        let (mut node2, shutdown_sender2) = KademliaNode::new(node2_addr).await?;
    
        // Bootstrap nodes (assuming node2 will act as a bootstrap node for node1)
        node1.bootstrap().await?;
        
        // Give some time for bootstrapping
        sleep(Duration::from_secs(2)).await;
    
        // Test storing and retrieving a value
        let key = b"test_key";
        let value = b"test_value";
    
        // Node1 stores the value
        node1.put(key, value).await?;
        
        // Node2 should find the value from Node1
        match node2.get(key).await? {
            Some(found_value) => {
                println!("Node2 successfully retrieved value: {:?}", String::from_utf8(found_value).unwrap());
            }
            None => {
                println!("Node2 failed to retrieve value.");
            }
        }
    
        // Run the nodes concurrently
        let node1_handle = tokio::spawn(async move {
            if let Err(e) = node1.run().await {
                eprintln!("Node1 encountered an error: {:?}", e);
            }
        });
    
        let node2_handle = tokio::spawn(async move {
            if let Err(e) = node2.run().await {
                eprintln!("Node2 encountered an error: {:?}", e);
            }
        });
    
        // Simulate running the nodes for a period of time
        sleep(Duration::from_secs(10)).await;
    
        // Shutdown the nodes
        shutdown_sender1.send(()).await.expect("Failed to send shutdown signal to Node1");
        shutdown_sender2.send(()).await.expect("Failed to send shutdown signal to Node2");
    
        // Wait for both node handles to finish
        node1_handle.await?;
        node2_handle.await?;

        
    Ok(())
}