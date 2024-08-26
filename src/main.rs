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
        )
        .await
        {
            Ok((node, _)) => {
                // Add a small delay before allowing other nodes to join.
                if bootstrap_addr.is_none() {
                    bootstrap_addr = Some(addr);
                    sleep(Duration::from_secs(1)).await; // Let the first node stabilize
                }
                
                nodes.push(node);
                println!("Created node at address: {}", addr);
                
            }
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
        println!(
            "Node {} routing table size: {}",
            i,
            node.get_routing_table_size()
        );
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
                Ok(Some(value)) => println!(
                    "Node {} retrieved value: {:?}",
                    i,
                    String::from_utf8(value)?
                ),
                Ok(None) => println!("Node {} failed to retrieve value: Not found", i),
                Err(e) => println!("Node {} failed to retrieve value: {:?}", i, e),
            }
        }
    }

    Ok(())
}
