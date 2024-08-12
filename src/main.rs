mod node;
mod routing_table;
mod kbucket;
mod message;
mod utils;
use log::{info, warn, debug, error};

use node::KademliaNode;
use std::net::SocketAddr;

fn main() -> std::io::Result<()> {
    env_logger::init(); // Initialize the logger
    info!("Starting Kademlia node...");

    let addr: SocketAddr = "127.0.0.1:33334".parse().expect("Failed to parse socket address");
    info!("Parsed socket address: {}", addr);

    let mut node = match KademliaNode::new(addr) {
        Ok(node) => {
            info!("Successfully created Kademlia node.");
            node
        }
        Err(e) => {
            error!("Failed to create Kademlia node: {:?}", e);
            return Err(e);
        }
    };

    info!("Bootstrapping node...");
    if let Err(e) = node.bootstrap() {
        error!("Bootstrap failed: {:?}", e);
        return Err(e);
    }

    info!("Running the node...");
    if let Err(e) = node.run() {
        error!("Node run failed: {:?}", e);
        return Err(e);
    }

    info!("Node shut down gracefully.");
    Ok(())
}
