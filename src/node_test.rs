use crate::cache::{cache_impl, policy};
use crate::message::{FindValueResult, Message};
use crate::routing_table::RoutingTable;
use crate::utils::{NodeId,KademliaError};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

use crate::node::{
    Config, Delay, KademliaNode, NetworkInterface, StorageManager, TimeProvider,
};
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

impl MockNetworkInterface {
    fn new() -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            received_messages: Arc::new(Mutex::new(Vec::new())),
            stored_data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl NetworkInterface for MockNetworkInterface {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> std::io::Result<usize> {
        let mut sent_messages = futures::executor::block_on(self.sent_messages.lock());
        sent_messages.push((buf.to_vec(), addr));
        debug!("Mock send_to: buf length {}, addr: {:?}", buf.len(), addr);

        // Simulate network response
        let message: Message = bincode::deserialize(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        match message {
            Message::Store { key, value, .. } => {
                let mut stored_data = futures::executor::block_on(self.stored_data.lock());
                stored_data.insert(key.clone(), value.clone());
                debug!("Mock P2P store: key {:?}, value {:?}", key, value);

                let response = Message::StoreResponse {
                    success: true,
                    error_message: None,
                };

                // let serialized_response = bincode::serialize(&response).unwrap();

                let serialized_response = bincode::serialize(&response)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

                let mut received_messages =
                    futures::executor::block_on(self.received_messages.lock());

                received_messages.push((serialized_response, addr));
            }

            Message::FindNode { .. } => {
                let response = Message::NodesFound(vec![(NodeId::new(), addr)]);

                let serialized_response = bincode::serialize(&response)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

                let mut received_messages =
                    futures::executor::block_on(self.received_messages.lock());

                received_messages.push((serialized_response, addr));
            }
            _ => {
                debug!("Mock send_to: Unhandled message type");
            }
        }

        Ok(buf.len())
    }

    fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        let mut received_messages = futures::executor::block_on(self.received_messages.lock());
        if let Some((message, addr)) = received_messages.pop() {
            buf[..message.len()].copy_from_slice(&message);
            debug!(
                "Mock recv_from: message length {}, addr: {:?}",
                message.len(),
                addr
            );
            Ok((message.len(), addr))
        } else {
            debug!("Mock recv_from: No messages available");
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "No messages",
            ))
        }
    }
}

// impl NetworkInterface for MockNetworkInterface {
//     fn send_to(&self, buf: &[u8], addr: SocketAddr) -> std::io::Result<usize> {
//         let mut sent_messages = futures::executor::block_on(self.sent_messages.lock());
//         sent_messages.push((buf.to_vec(), addr));

//         // Simulate network response
//         let message: Message = bincode::deserialize(buf)
//             .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
//         match message {
//             Message::Store { key, value, .. } => {
//                 let mut stored_data = futures::executor::block_on(self.stored_data.lock());
//                 stored_data.insert(key, value);
//                 let response = Message::StoreResponse {
//                     success: true,
//                     error_message: None,
//                 };
//                 let serialized_response = bincode::serialize(&response)
//                     .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
//                 let mut received_messages =
//                     futures::executor::block_on(self.received_messages.lock());
//                 received_messages.push((serialized_response, addr));
//             }
//             Message::FindNode { .. } => {
//                 let response = Message::NodesFound(vec![(NodeId::new(), addr)]);
//                 let serialized_response = bincode::serialize(&response)
//                     .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
//                 let mut received_messages =
//                     futures::executor::block_on(self.received_messages.lock());
//                 received_messages.push((serialized_response, addr));
//             }
//             _ => {}
//         }

//         Ok(buf.len())
//     }

//     fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
//         let mut received_messages = futures::executor::block_on(self.received_messages.lock());
//         if let Some((message, addr)) = received_messages.pop() {
//             buf[..message.len()].copy_from_slice(&message);
//             Ok((message.len(), addr))
//         } else {
//             Err(std::io::Error::new(
//                 std::io::ErrorKind::WouldBlock,
//                 "No messages",
//             ))
//         }
//     }
// }

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
async fn test_store_and_retrieve_value() {
    // RUN CMD:
    //  RUST_LOG=debug  RUST_BACKTRACE=full cargo test test_store_and_retrieve_value   -- --nocapture

    env_logger::init(); // Initialize logging (only needed once if multiple tests use logging)

    let mock_network = Arc::new(MockNetworkInterface {
        sent_messages: Arc::new(Mutex::new(Vec::new())),
        received_messages: Arc::new(Mutex::new(Vec::new())),
        stored_data: Arc::new(Mutex::new(HashMap::new())),
    });

    let config = Arc::new(Config::default());
    let mut storage_manager = StorageManager::new(config.clone());
    info!("Starting test: test_store_and_retrieve_value");

    // Store a value
    let key = b"test_key";
    let value = b"test_value";
    debug!("Storing value for key: {:?}", key);

    storage_manager.store(key, value).await.unwrap();
    debug!("Retrieving value for key: {:?}", key);

    // Retrieve the value
    let retrieved_value = storage_manager.get(key).await;
    assert_eq!(
        retrieved_value,
        Some(value.to_vec()),
        "Value should be retrieved correctly from storage"
    );
    info!("Test passed: test_store_and_retrieve_value");
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
    assert_eq!(
        Some(value.to_vec()),
        retrieved_value,
        "Retrieved value does not match put value"
    );

    // Verify network operations
    let sent_messages = mock_network.sent_messages.lock().await;
    assert!(
        !sent_messages.is_empty(),
        "No messages were sent during operations"
    );

    // Verify stored data
    let stored_data = mock_network.stored_data.lock().await;
    // assert_eq!(stored_data.get(key), Some(&value.to_vec()), "Value was not stored in the mock network"); // types. A Vec<u8> can be borrowed as [u8] (a slice), but not as [u8; 8] (a fixed-size array).

    assert_eq!(
        stored_data.get(&key[..]),
        Some(&value.to_vec()),
        "Value was not stored in the mock network"
    );

    println!("All tests passed!");
}

#[tokio::test]
async fn test_bootstrap_process() {
    env_logger::init();

    info!("Starting test: test_bootstrap_process");

    let mock_network = Arc::new(MockNetworkInterface {
        sent_messages: Arc::new(Mutex::new(Vec::new())),
        received_messages: Arc::new(Mutex::new(Vec::new())),
        stored_data: Arc::new(Mutex::new(HashMap::new())),
    });

    let config = Config::default();
    // let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

    let mut node = KademliaNode::new(
        "127.0.0.1:8080".parse().unwrap(),
        Some(config.clone()),
        mock_network.clone(),
        Arc::new(MockTimeProvider),
        Arc::new(MockDelay),
        vec!["127.0.0.1:8081".parse().unwrap()],
    )
    .await
    .unwrap()
    .0;

    info!("Adding a mock bootstrap node to simulate a response");
    let bootstrap_node_id = NodeId::new();
    // mock_network.add_received_message(
    //     bincode::serialize(&Message::Pong { sender: bootstrap_node_id }).unwrap(),
    //     "127.0.0.1:8081".parse().unwrap(),
    // );

    debug!("Running bootstrap process");
    node.bootstrap().await.unwrap();

    // Verify that the bootstrap node was added to the routing table
    let closest_nodes = node
        .routing_table
        .find_closest(&bootstrap_node_id, config.k);
    assert!(closest_nodes.iter().any(|(id, _)| id == &bootstrap_node_id));

    info!("Test passed: test_bootstrap_process");
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
    )
    .await?;

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
    )
    .await?;

    // Test client GET
    let get_result = node
        .handle_client_message(
            Message::ClientGet {
                key: key.clone(),
                sender: NodeId::new(),
            },
            client_addr,
        )
        .await?;

    // Assert that the GET result matches the stored value
    match get_result {
        _ => panic!("Unexpected response from client GET"),
    }

    Ok(())
}
#[tokio::test]
async fn test_handle_client_store_request() -> Result<(), KademliaError> {
    env_logger::init(); // Ensure logging is initialized

    // Setup your mock components
    let mock_network = Arc::new(MockNetworkInterface::new());

    let mock_time_provider = Arc::new(MockTimeProvider);
    let mock_delay_provider = Arc::new(MockDelay);

    let config = Config::default();

    debug!("Initializing Kademlia node with test configuration.");
    let (mut node, _shutdown_sender) = KademliaNode::new(
        "127.0.0.1:8000".parse().unwrap(),
        Some(config),
        mock_network.clone(),
        mock_time_provider,
        mock_delay_provider,
        vec!["127.0.0.1:8001".parse().unwrap()],
    )
    .await?;

    debug!("Node initialized successfully.");

    // Manually add a node to the routing table for testing
    let test_node_id = NodeId::new();
    let test_node_addr: SocketAddr = "127.0.0.1:8002".parse().unwrap();
    node.routing_table.update(test_node_id, test_node_addr);
    debug!(
        "Added test node to routing table: {:?} at {:?}",
        test_node_id, test_node_addr
    );

    // Create a ClientStore message
    let key = NodeId::new();
    let value = b"client_value".to_vec();
    let client_addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();

    debug!(
        "Sending ClientStore message with key: {:?}, value: {:?}",
        key, value
    );

    // Send the ClientStore message
    node.handle_client_message(
        Message::ClientStore {
            key: key.0.to_vec(),
            value: value.clone(),
            sender: NodeId::new(),
        },
        client_addr,
    )
    .await?;

    debug!("ClientStore message handled. Verifying stored data...");

    // Add a small delay to ensure async operations complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Check that the data was stored correctly
    let stored_data = mock_network.stored_data.lock().await;
    debug!("Current stored data: {:?}", *stored_data);

    assert_eq!(
        stored_data.get(&key.0.to_vec()),
        Some(&value),
        "ClientStore data was not stored correctly."
    );
    debug!("Stored data verified successfully.");

    // Verify that a successful ClientStoreResponse was sent back
    let sent_messages = mock_network.sent_messages.lock().await;
    debug!("Sent messages: {:?}", *sent_messages);

    let response_message = sent_messages.iter().find(|(msg, _)| {
        let deserialized: Message = bincode::deserialize(&msg).unwrap();
        matches!(
            deserialized,
            Message::ClientStoreResponse { success: true, .. }
        )
    });
    assert!(
        response_message.is_some(),
        "No ClientStoreResponse was sent or it was unsuccessful."
    );
    debug!("ClientStoreResponse verified successfully.");

    Ok(())
}
#[tokio::test]
async fn test_handle_p2p_store_request() -> Result<(), KademliaError> {
    env_logger::init(); // Ensure logging is initialized

    // Setup your mock components: network interface, time provider, and delay provider.
    let mock_network = Arc::new(MockNetworkInterface {
        sent_messages: Arc::new(Mutex::new(Vec::new())),
        received_messages: Arc::new(Mutex::new(Vec::new())),
        stored_data: Arc::new(Mutex::new(HashMap::new())),
    });
    let mock_time_provider = Arc::new(MockTimeProvider);
    let mock_delay_provider = Arc::new(MockDelay);

    let config = Config::default();

    debug!("Initializing Kademlia node with test configuration.");
    let (mut node, _shutdown_sender) = KademliaNode::new(
        "127.0.0.1:8000".parse().unwrap(),
        Some(config),
        mock_network.clone(),
        mock_time_provider,
        mock_delay_provider,
        vec!["127.0.0.1:8001".parse().unwrap()],
    )
    .await?;

    debug!("Node initialized successfully.");

    // Create a P2P Store message.
    let key: Vec<u8> = b"p2p_key".to_vec();
    let key = NodeId::new();
    let keyA: Vec<u8> = key.into();

    let value = b"p2p_value".to_vec();
    let p2p_addr: SocketAddr = "127.0.0.1:8002".parse().unwrap();

    debug!(
        "Sending P2P Store message with key: {:?}, value: {:?}",
        key, value
    );

    // Send the P2P Store message.
    node.handle_message(
        Message::Store {
            key: keyA.clone(),
            value: value.clone(),
            sender: NodeId::new(),
            timestamp: SystemTime::now(),
        },
        p2p_addr,
    )
    .await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    debug!("P2P Store message handled. Verifying stored data...");

    // Check that the data was stored correctly.
    let stored_data = mock_network.stored_data.lock().await;
    debug!("Current stored data: {:?}", *stored_data);

    assert_eq!(
        stored_data.get(&keyA),
        Some(&value),
        "P2P Store data was not stored correctly."
    );

    debug!("Stored data verified successfully.");

    // Verify that a successful StoreResponse was sent back.
    let sent_messages = mock_network.sent_messages.lock().await;
    let response_message = sent_messages.iter().find(|(msg, _)| {
        let deserialized: Message = bincode::deserialize(&msg).unwrap();
        matches!(deserialized, Message::StoreResponse { success: true, .. })
    });
    assert!(
        response_message.is_some(),
        "No StoreResponse was sent or it was unsuccessful."
    );
    debug!("StoreResponse verified successfully.");

    Ok(())
}

#[tokio::test]
async fn test_node_lookup() -> Result<(), KademliaError> {
    // Initialize logging
    let _ = env_logger::try_init();

    // Create a mock network with multiple nodes
    let mock_network = Arc::new(MockNetworkInterface::new());
    let config = Arc::new(Config::default());

    // Create the main node we'll be testing
    let (mut main_node, _) =
        create_node("127.0.0.1:8000", mock_network.clone(), config.clone()).await?;

    // Create and add several other nodes to the network
    let mut all_nodes = vec![main_node.id];
    for i in 1..10 {
        let (node, _) = create_node(
            &format!("127.0.0.1:800{}", i),
            mock_network.clone(),
            config.clone(),
        )
        .await?;
        all_nodes.push(node.id);
        main_node.routing_table.update(node.id, node.addr);
    }

    // Sort nodes by their distance to the main node
    all_nodes.sort_by(|a, b| main_node.id.distance(a).cmp(&main_node.id.distance(b)));

    // Perform a node lookup
    let target_id = NodeId::new(); // Random target ID
    let closest_nodes = main_node.find_node(target_id).await?;

    // Verify the results
    assert_eq!(
        closest_nodes.len(),
        config.k.min(all_nodes.len() - 1),
        "Node lookup should return k closest nodes"
    );

    // Check if the returned nodes are actually the closest ones
    let mut expected_closest = all_nodes.clone();
    expected_closest.sort_by(|a, b| target_id.distance(a).cmp(&target_id.distance(b)));
    expected_closest.truncate(config.k);

    for (i, (node_id, _)) in closest_nodes.iter().enumerate() {
        assert_eq!(
            *node_id, expected_closest[i],
            "Node at position {} should be {:?}, but got {:?}",
            i, expected_closest[i], node_id
        );
    }

    Ok(())
}

// Helper function to create a node with mock components
async fn create_node(
    addr: &str,
    mock_network: Arc<MockNetworkInterface>,
    config: Arc<Config>,
) -> Result<(KademliaNode, mpsc::Sender<()>), KademliaError> {
    let addr: SocketAddr = addr.parse().unwrap();
    let mock_time_provider = Arc::new(MockTimeProvider);
    let mock_delay_provider = Arc::new(MockDelay);

    KademliaNode::new(
        addr,
        Some(config.as_ref().clone()),
        mock_network,
        mock_time_provider,
        mock_delay_provider,
        vec![], // No bootstrap nodes for this test
    )
    .await
}

#[tokio::test]
async fn test_value_lookup() -> Result<(), KademliaError> {
    env_logger::init();

    // Set up a small network of nodes
    let mut nodes = Vec::new();
    let mut mock_networks = Vec::new();
    for i in 0..10 {
        let mock_network = Arc::new(MockNetworkInterface::new());
        let config = Config::default();
        let (node, _) = KademliaNode::new(
            format!("127.0.0.1:{}", 8000 + i).parse().unwrap(),
            Some(config.clone()),
            mock_network.clone(),
            Arc::new(MockTimeProvider),
            Arc::new(MockDelay),
            vec![format!("127.0.0.1:{}", 8000).parse().unwrap()], // Use first node as bootstrap
        )
        .await?;
        nodes.push(node);
        mock_networks.push(mock_network);
    }

    // Connect nodes
    for i in 0..10 {
        for j in 0..10 {
            if i != j {
                nodes[i].routing_table.update(nodes[j].id, nodes[j].addr);
            }
        }
    }

    // Store a known key-value pair on one of the nodes
    let key = b"test_key".to_vec();
    let value = b"test_value".to_vec();
    nodes[0].put(&key, &value).await?;

    // Perform a findValue operation from a different node for the known key
    let result = nodes[5].get(&key).await?;
    assert_eq!(
        result,
        Some(value.clone()),
        "Failed to retrieve stored value"
    );

    // Perform a findValue operation for a non-existent key
    let non_existent_key = b"non_existent_key".to_vec();
    let result = nodes[5].get(&non_existent_key).await?;
    assert!(
        result.is_none(),
        "Unexpectedly found value for non-existent key"
    );

    // Test lookup for a recently expired key
    let mock_time = Arc::new(Mutex::new(SystemTime::now()));
    let config = Config {
        cache_ttl: Duration::from_secs(1),
        ..Config::default()
    };
    let (mut node, _) = KademliaNode::new(
        "127.0.0.1:8100".parse().unwrap(),
        Some(config),
        mock_networks[0].clone(),
        Arc::new(MockTimeProvider {}),
        Arc::new(MockDelay),
        vec!["127.0.0.1:8000".parse().unwrap()],
    )
    .await?;

    let expiring_key = b"expiring_key".to_vec();
    let expiring_value = b"expiring_value".to_vec();
    node.put(&expiring_key, &expiring_value).await?;

    // Advance time
    *mock_time.lock().await = SystemTime::now() + Duration::from_secs(2);

    let result = node.get(&expiring_key).await?;
    assert!(result.is_none(), "Value did not expire as expected");

    Ok(())
}

#[tokio::test]
async fn test_store_operation() -> Result<(), KademliaError> {
    env_logger::init();

    // Set up a network of nodes
    let mut nodes: Vec<KademliaNode> = Vec::new();
    let mut mock_networks: Vec<Arc<MockNetworkInterface>> = Vec::new();

    for i in 0..20 {
        let mock_network = Arc::new(MockNetworkInterface::new());
        let config = Config::default();
        let (node, _) = KademliaNode::new(
            format!("127.0.0.1:{}", 8000 + i).parse().unwrap(),
            Some(config.clone()),
            mock_network.clone(),
            Arc::new(MockTimeProvider),
            Arc::new(MockDelay),
            vec![format!("127.0.0.1:{}", 8000).parse().unwrap()], // Use first node as bootstrap
        )
        .await?;
        nodes.push(node);
        mock_networks.push(mock_network);
    }

    // Connect nodes
    for i in 0..20 {
        for j in 0..20 {
            if i != j {
                nodes[i].routing_table.update(nodes[j].id, nodes[j].addr);
            }
        }
    }

    // Choose a random node and initiate a store operation
    let key = b"test_key".to_vec();
    let value = b"test_value".to_vec();
    let storing_node_index = 10; // Middle node
    nodes[storing_node_index].put(&key, &value).await?;

    // Verify that the value is stored on the initiating node
    let stored_value = nodes[storing_node_index].storage_manager.get(&key).await;
    assert_eq!(
        stored_value,
        Some(value.clone()),
        "Value not stored on initiating node"
    );

    // Find the k-closest nodes to the key's hash
    let target = NodeId::from_slice(&StorageManager::hash_key(&key)[..32].try_into().unwrap());
    let closest_nodes = nodes[storing_node_index].find_node(target).await?;

    // Verify that the value is replicated to these k-closest nodes
    for (node_id, _) in closest_nodes
        .iter()
        .take(nodes[storing_node_index].config.k)
    {
        let node_index = nodes.iter().position(|n| n.id == *node_id).unwrap();
        let replicated_value = nodes[node_index].storage_manager.get(&key).await;
        assert_eq!(
            replicated_value,
            Some(value.clone()),
            "Value not replicated to closest node"
        );
    }

    // Perform a findValue operation from a different node
    let retrieving_node_index: usize = (storing_node_index + 5) % 20;
    let retrieved_value = nodes[retrieving_node_index].get(&key).await?;
    assert_eq!(
        retrieved_value,
        Some(value.clone()),
        "Failed to retrieve stored value"
    );

    // Test storing a value when the closest node is offline
    let offline_node_index: NodeId = closest_nodes[0].0;
    
    nodes[offline_node_index] = KademliaNode::new(
        
        nodes[offline_node_index].addr,
        Some(Config::default()),
        Arc::new(MockNetworkInterface::new()), // New mock network to simulate offline
        Arc::new(MockTimeProvider),
        Arc::new(MockDelay),
        vec![],
    )
    .await?
    .0;

    let new_key = b"offline_test_key".to_vec();
    let new_value = b"offline_test_value".to_vec();
    nodes[storing_node_index].put(&new_key, &new_value).await?;

    // Verify that the value is still stored and retrievable
    let retrieved_value = nodes[retrieving_node_index].get(&new_key).await?;
    assert_eq!(
        retrieved_value,
        Some(new_value),
        "Failed to retrieve value with offline node"
    );

    Ok(())
}

#[tokio::test]
async fn test_node_join() -> Result<(), KademliaError> {
    env_logger::init();

    // Set up an existing network of nodes
    let mut nodes = Vec::new();
    let mut mock_networks = Vec::new();
    for i in 0..50 {
        let mock_network = Arc::new(MockNetworkInterface::new());
        let config = Config::default();
        let (node, _) = KademliaNode::new(
            format!("127.0.0.1:{}", 8000 + i).parse().unwrap(),
            Some(config.clone()),
            mock_network.clone(),
            Arc::new(MockTimeProvider),
            Arc::new(MockDelay),
            vec![format!("127.0.0.1:{}", 8000).parse().unwrap()], // Use first node as bootstrap
        )
        .await?;
        nodes.push(node);
        mock_networks.push(mock_network);
    }

    // Connect existing nodes
    for i in 0..50 {
        for j in 0..50 {
            if i != j {
                nodes[i].routing_table.update(nodes[j].id, nodes[j].addr);
            }
        }
    }

    // Create a new node with a known NodeID
    let new_node_id = NodeId::new(); // You might want to use a specific ID for deterministic testing
    let new_node_mock_network = Arc::new(MockNetworkInterface::new());
    let (mut new_node, _) = KademliaNode::new(
        "127.0.0.1:9000".parse().unwrap(),
        Some(Config::default()),
        new_node_mock_network.clone(),
        Arc::new(MockTimeProvider),
        Arc::new(MockDelay),
        vec!["127.0.0.1:8000".parse().unwrap()], // Use first node as bootstrap
    )
    .await?;

    // Initiate the join process for the new node
    new_node.bootstrap().await?;

    // Allow some time for the join process to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Verify that the new node's routing table contains an appropriate number of entries
    let new_node_routing_table_size = new_node.get_routing_table_size();
    assert!(
        new_node_routing_table_size > 0,
        "New node's routing table is empty"
    );
    assert!(
        new_node_routing_table_size <= 50,
        "New node's routing table has too many entries"
    );

    // Check that the entries in the routing table are correct
    for bucket in &new_node.routing_table.buckets {
        for entry in &bucket.entries {
            assert!(
                nodes.iter().any(|n| n.id == entry.node_id),
                "Invalid node in new node's routing table"
            );
        }
    }

    // Perform a series of lookups from the new node
    for _ in 0..10 {
        let random_key = NodeId::new();
        let lookup_result = new_node.find_node(random_key).await?;
        assert!(!lookup_result.is_empty(), "Lookup failed from new node");
    }

    // Check that other nodes in the network have updated their routing tables
    let mut new_node_found_count = 0;
    for node in &nodes {
        if node.routing_table.find_closest(&new_node.id, 1)[0].0 == new_node.id {
            new_node_found_count += 1;
        }
    }
    assert!(
        new_node_found_count > 0,
        "New node not found in any existing node's routing table"
    );

    // Test join process when bootstrap nodes are offline
    let offline_bootstrap_mock_network = Arc::new(MockNetworkInterface::new());
    let (mut offline_bootstrap_node, _) = KademliaNode::new(
        "127.0.0.1:9001".parse().unwrap(),
        Some(Config::default()),
        offline_bootstrap_mock_network.clone(),
        Arc::new(MockTimeProvider),
        Arc::new(MockDelay),
        vec!["127.0.0.1:9999".parse().unwrap()], // Non-existent bootstrap node
    )
    .await?;

    // Attempt to bootstrap with offline bootstrap node
    let bootstrap_result = offline_bootstrap_node.bootstrap().await;
    assert!(
        bootstrap_result.is_err(),
        "Bootstrap should fail with offline bootstrap nodes"
    );

    Ok(())
}

use tokio::time::sleep;

const NODE_COUNT: usize = 10;
const TEST_DURATION: Duration = Duration::from_secs(60);
const NODE_LEAVE_TIME: Duration = Duration::from_secs(20);
const CHECK_INTERVAL: Duration = Duration::from_secs(5);

#[tokio::test]
async fn test_node_leave() {
    // Setup
    let mut nodes = setup_network(NODE_COUNT).await;
    let (leaving_node, leaving_addr) = nodes.remove(NODE_COUNT / 2);

    let leaving_id = leaving_node.id;
    

    // Start the test
    let test_start = tokio::time::Instant::now();

    // Run the network for a while
    tokio::spawn(async move {
        while tokio::time::Instant::now() - test_start < TEST_DURATION {
            simulate_network_activity(&nodes).await;
            sleep(CHECK_INTERVAL).await;
        }
    });

    // Simulate node leaving
    tokio::time::sleep(NODE_LEAVE_TIME).await;
    println!(
        "Node {:?} at {:?} is leaving the network",
        leaving_id, leaving_addr
    );

    // Periodic checks
    while tokio::time::Instant::now() - test_start < TEST_DURATION {
        check_network_state(&nodes, &leaving_id).await;
        sleep(CHECK_INTERVAL).await;
    }

    // Final verification
    verify_network_state(&nodes, &leaving_id).await;
}

async fn setup_network(count: usize) -> Vec<(Arc<Mutex<KademliaNode>>, SocketAddr)> {
    let mut nodes = Vec::new();
    let config = Arc::new(Config::default());

    for i in 0..count {
        let addr = format!("127.0.0.1:{}", 8000 + i).parse().unwrap();
        let mock_network = Arc::new(MockNetworkInterface::new());
        let mock_time = Arc::new(MockTimeProvider);
        let mock_delay = Arc::new(MockDelay);

        let (node, _) = KademliaNode::new(
            addr,
            Some(config.clone()),
            mock_network,
            mock_time,
            mock_delay,
            vec![], // Empty bootstrap list, we'll connect nodes manually
        )
        .await
        .unwrap();

        nodes.push((Arc::new(Mutex::new(node)), addr));
    }

    // Connect nodes
    for i in 0..count {
        for j in 0..count {
            if i != j {
                let (node, _) = &nodes[i];
                let (_, addr) = nodes[j];
                node.lock().await.routing_table.update(NodeId::new(), addr);
            }
        }
    }

    nodes
}

async fn simulate_network_activity(nodes: &[(Arc<Mutex<KademliaNode>>, SocketAddr)]) {
    for (node, _) in nodes {
        let node = node.lock().await;
        let key = NodeId::new().0.to_vec();
        let value = b"test_value".to_vec();
        let _ = node.put(&key, &value).await;
        let _ = node.get(&key).await;
    }
}

async fn check_network_state(
    nodes: &[(Arc<Mutex<KademliaNode>>, SocketAddr)],
    leaving_id: &NodeId,
) {
    for (node, addr) in nodes {
        let node = node.lock().await;
        let contains_leaving_node = node
            .routing_table
            .find_closest(leaving_id, 1)
            .iter()
            .any(|(id, _)| id == leaving_id);

        if contains_leaving_node {
            println!(
                "Warning: Node at {:?} still contains the leaving node in its routing table",
                addr
            );
        }
    }
}

async fn verify_network_state(
    nodes: &[(Arc<Mutex<KademliaNode>>, SocketAddr)],
    leaving_id: &NodeId,
) {
    for (node, addr) in nodes {
        let node = node.lock().await;
        let contains_leaving_node = node
            .routing_table
            .find_closest(leaving_id, 1)
            .iter()
            .any(|(id, _)| id == leaving_id);

        assert!(
            !contains_leaving_node,
            "Node at {:?} still contains the leaving node in its routing table",
            addr
        );

        // Verify that the network is still functional
        let key = NodeId::new().0.to_vec();
        let value = b"test_value".to_vec();
        assert!(
            node.put(&key, &value).await.is_ok(),
            "Failed to put value from node {:?}",
            addr
        );
        assert!(
            node.get(&key).await.is_ok(),
            "Failed to get value from node {:?}",
            addr
        );
    }

    println!("Network state verified successfully");
}
