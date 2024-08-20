use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use crate::cache::{cache_impl, policy};
use crate::message::{Message, FindValueResult};
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;

use crate::node::{Config, KademliaError, KademliaNode, NetworkInterface, TimeProvider, Delay};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::interval;

use bincode::{deserialize, serialize};

// Mock implementations
struct MockNetworkInterface {
    sent_messages: Arc<Mutex<Vec<(Vec<u8>, SocketAddr)>>>,
    received_messages: Arc<Mutex<Vec<(Vec<u8>, SocketAddr)>>>,
    stored_data: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
}

impl NetworkInterface for MockNetworkInterface {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> std::io::Result<usize> {
        let mut sent_messages = futures::executor::block_on(self.sent_messages.lock());
        sent_messages.push((buf.to_vec(), addr));

        // Simulate network response
        let message: Message = bincode::deserialize(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        match message {
            Message::Store { key, value, .. } => {
                let mut stored_data = futures::executor::block_on(self.stored_data.lock());
                stored_data.insert(key, value);
                let response = Message::StoreResponse { success: true, error_message: None };
                let serialized_response = bincode::serialize(&response).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let mut received_messages = futures::executor::block_on(self.received_messages.lock());
                received_messages.push((serialized_response, addr));
            },
            Message::FindNode { .. } => {
                let response = Message::NodesFound(vec![(NodeId::new(), addr)]);
                let serialized_response = bincode::serialize(&response).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let mut received_messages = futures::executor::block_on(self.received_messages.lock());
                received_messages.push((serialized_response, addr));
            },
            _ => {}
        }

        Ok(buf.len())
    }

    fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        let mut received_messages = futures::executor::block_on(self.received_messages.lock());
        if let Some((message, addr)) = received_messages.pop() {
            buf[..message.len()].copy_from_slice(&message);
            Ok((message.len(), addr))
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "No messages"))
        }
    }
}

struct MockTimeProvider;

impl TimeProvider for MockTimeProvider {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

struct MockDelay;

impl Delay for MockDelay {
    fn delay(&self, _duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {})
    }
}

#[tokio::test]
async fn test_put_and_get() {
    // Setup
    let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
    let bootstrap_addr: SocketAddr = "127.0.0.1:8001".parse().unwrap();
    let mock_network = Arc::new(MockNetworkInterface {
        sent_messages: Arc::new(Mutex::new(Vec::new())),
        received_messages: Arc::new(Mutex::new(Vec::new())),
        stored_data: Arc::new(Mutex::new(HashMap::new())),
    });
    let mock_time_provider = Arc::new(MockTimeProvider);
    let mock_delay_provider = Arc::new(MockDelay);

    let config = Config {
        k: 20,
        alpha: 3,
        request_timeout: Duration::from_secs(1),
        cache_size: 100,
        cache_ttl: Duration::from_secs(3600),
        maintenance_interval: Duration::from_secs(3600),
    };

    let bootstrap_nodes = vec![bootstrap_addr];

    let (mut node, _shutdown_sender) = KademliaNode::new(
        addr,
        Some(config),
        mock_network.clone(),
        mock_time_provider,
        mock_delay_provider,
        bootstrap_nodes,
    )
    .await
    .expect("Failed to create KademliaNode");

    // Bootstrap the node
    node.bootstrap().await.expect("Failed to bootstrap node");

    // Test put
    let key = b"test_key";
    let value = b"test_value";
    node.put(key, value).await.expect("Failed to put value");

    // Test get
    let retrieved_value = node.get(key).await.expect("Failed to get value");
    assert_eq!(Some(value.to_vec()), retrieved_value, "Retrieved value does not match put value");

    // Verify network operations
    let sent_messages = mock_network.sent_messages.lock().await;
    assert!(!sent_messages.is_empty(), "No messages were sent during operations");

    // Verify stored data
    let stored_data = mock_network.stored_data.lock().await;
    // assert_eq!(stored_data.get(key), Some(&value.to_vec()), "Value was not stored in the mock network"); // types. A Vec<u8> can be borrowed as [u8] (a slice), but not as [u8; 8] (a fixed-size array).

    assert_eq!(stored_data.get(&key[..]), Some(&value.to_vec()), "Value was not stored in the mock network");

    println!("All tests passed!");
}

#[tokio::test]
async fn test_client_store_and_get() -> Result<(), KademliaError> {
    // Create a mock config
    let config = Config::default();

    // Create a mock socket
    // let socket: Arc<dyn NetworkInterface> = Arc::new(MockNetworkInterface);

    // // Create mock time and delay providers
    // let time_provider = Arc::new(MockTimeProvider);
    // let delay_provider = Arc::new(MockDelayProvider);

    let mock_network = Arc::new(MockNetworkInterface {
        sent_messages: Arc::new(Mutex::new(Vec::new())),
        received_messages: Arc::new(Mutex::new(Vec::new())),
        stored_data: Arc::new(Mutex::new(HashMap::new())),
    });
    let mock_time_provider = Arc::new(MockTimeProvider);
    let mock_delay_provider = Arc::new(MockDelay);


    // Create a test node
    let (mut node, _shutdown_sender) = KademliaNode::new(
        "127.0.0.1:8000".parse().unwrap(),
        Some(config),
        mock_network,
        mock_time_provider,
        mock_delay_provider,
        vec!["127.0.0.1:8001".parse().unwrap()], // Mock bootstrap nodes
    ).await?;

    // Test client STORE
    let key = b"test_key".to_vec();
    let value = b"test_value".to_vec();
    let client_addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();

    node.handle_client_message(
        Message::ClientStore {
            key: key.clone(),
            value: value.clone(),
            sender: NodeId::new(),
        },
        client_addr,
    ).await?;

    // Test client GET
    let get_result = node.handle_client_message(
        Message::ClientGet {
            key: key.clone(),
            sender: NodeId::new(),
        },
        client_addr,
    ).await?;

    // Assert that the GET result matches the stored value
    match get_result {
        _ => panic!("Unexpected response from client GET"),
    }

    Ok(())
}