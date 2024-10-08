use kademlia_mvp::message::{Message, FindValueResult};
use kademlia_mvp::node::KademliaNode;
use kademlia_mvp::utils::NodeId;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use log::{debug, info};


// CMD TO RUN ALL TESTS(unit/intg) W/ CLEAR DEBUG OUTPUT
// $ RUST_LOG=debug cargo test  -- --nocapture  --test-threads=1

// Specifiy single test file 
// $ RUST_LOG=debug cargo test --test integration_tests  -- --nocapture  --test-threads=1

// Specifiy single test in single test file 
// $ RUST_LOG=debug cargo test test_node_discovery --test integration_tests  -- --nocapture  --test-threads=1

// Specifiy single test in single test file w/ backtrace output
// $ RUST_BACKTRACE=1 RUST_LOG=debug cargo test test_node_discovery --test integration_tests  -- --nocapture  --test-threads=1

// STEPS TO RUN IN VSCODE DEBUGGER (set breakpt on desired test, comment other tests out)
// 1. BUILD test w/o running it (integration tests are linked against the compiled main source code)-An executable is created for the integration tests, located in target/debug/deps/.
//      $ cargo test --no-run --test integration_tests --package=kademlia_mvp
//      Finished `test` profile [unoptimized + debuginfo] target(s) in 4.16s
//      Executable tests/integration_tests.rs (target/debug/deps/integration_tests-112b4aecda8f7c8f)
// 2. Binary ID is (112b4aecda8f7c8f)
// 3. add to launch.json
//            "program": "${workspaceFolder}/target/debug/deps/integration_tests-112b4aecda8f7c8f",
// 4. start "run and debug" for that task

fn init_logger() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Trace) // Set the desired log level
        .try_init();
}

#[tokio::test]
async fn test_node_discovery() {
    init_logger();
    debug!("\n\n\n\n\n");
    debug!("============================");
    debug!("Starting test: test_node_discovery");
    debug!("============================");

    let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let addr3: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let (mut node1, _) = KademliaNode::new(addr1).await.unwrap();
    let (mut node2, _) = KademliaNode::new(addr2).await.unwrap();
    let (mut node3, _) = KademliaNode::new(addr3).await.unwrap();
    
    node2.bootstrap().await.unwrap();
    debug!("node2 contents: {:#?}", node2);
    node3.bootstrap().await.unwrap();
    debug!("node3 contents: {:#?}", node3);

    // let target_id = node3.id;
    // let nodes = node1.find_node(&target_id);

    // debug!("Discovered nodes: {:?}", nodes);

    // assert!(nodes.iter().any(|(node_id, _)| *node_id == node3.id));

    debug!("============================");
    debug!("Completed test: test_node_discovery");
    debug!("============================");
}

// #[tokio::test]
// async fn test_value_storage_and_retrieval() {
//     init_logger();
//     debug!("\n\n\n\n\n");
//     debug!("============================");
//     debug!("Starting test: test_value_storage_and_retrieval");
//     debug!("============================");

//     let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();

//     let (mut node1, _) = KademliaNode::new(addr1).await.unwrap();
//     let (mut node2, _) = KademliaNode::new(addr2).await.unwrap();

//     node2.bootstrap().await.unwrap();

//     let key = b"test_key";
//     let value = b"test_value";

//     debug!("Node1 storing key: {:?}, value: {:?}", key, value);
//     node1.put(key, value).await.unwrap();

//     debug!("Node2 retrieving key: {:?}", key);
//     let retrieved_value = node2.get(key).await.unwrap();

//     debug!("Retrieved value: {:?}", retrieved_value);
//     assert_eq!(retrieved_value, Some(value.to_vec()));

//     debug!("============================");
//     debug!("Completed test: test_value_storage_and_retrieval");
//     debug!("============================");
// }

// #[tokio::test]
// async fn test_message_handling_ping_pong() {
//     init_logger();
//     debug!("\n\n\n\n\n");
//     debug!("============================");
//     debug!("Starting test: test_message_handling_ping_pong");
//     debug!("============================");

//     let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();

//     let (mut node1, _) = KademliaNode::new(addr1).await.unwrap();
//     let (mut node2, _) = KademliaNode::new(addr2).await.unwrap();

//     debug!("Node1 sending Ping to Node2 at {:?}", node2.addr);
//     node1.ping(node2.addr).await.unwrap();

//     let bucket_index = node2
//         .id
//         .distance(&node1.id)
//         .as_bytes()
//         .iter()
//         .position(|&x| x != 0)
//         .unwrap_or(255);
//     let bucket = &node2.routing_table.buckets[bucket_index];

//     debug!("Bucket after Ping: {:?}", bucket);
//     assert!(bucket.entries.iter().any(|entry| entry.node_id == node1.id));

//     debug!("============================");
//     debug!("Completed test: test_message_handling_ping_pong");
//     debug!("============================");
// }

// #[tokio::test]
// async fn test_routing_table_update_on_message() {
//     init_logger();
//     debug!("\n\n\n\n\n");
//     debug!("============================");
//     debug!("Starting test: test_routing_table_update_on_message");
//     debug!("============================");

//     let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();

//     let (mut node1, _) = KademliaNode::new(addr1).await.unwrap();
//     let (mut node2, _) = KademliaNode::new(addr2).await.unwrap();

//     let key = b"test_key".to_vec();
//     let value = b"test_value".to_vec();

//     debug!("Node1 sending Store message to Node2");
//     node2
//         .handle_message(Message::Store { key: key.clone(), value: value.clone() }, node1.addr)
//         .await
//         .unwrap();

//     let bucket_index = node2
//         .id
//         .distance(&node1.id)
//         .as_bytes()
//         .iter()
//         .position(|&x| x != 0)
//         .unwrap_or(255);
//     let bucket = &node2.routing_table.buckets[bucket_index];

//     debug!("Bucket after Store message: {:?}", bucket);
//     assert!(bucket.entries.iter().any(|entry| entry.node_id == node1.id));

//     debug!("============================");
//     debug!("Completed test: test_routing_table_update_on_message");
//     debug!("============================");
// }

// #[tokio::test]
// async fn test_find_value_via_multiple_nodes() {
//     init_logger();
//     debug!("\n\n\n\n\n");
//     debug!("============================");
//     debug!("Starting test: test_find_value_via_multiple_nodes");
//     debug!("============================");

//     let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let addr3: SocketAddr = "127.0.0.1:0".parse().unwrap();

//     let (mut node1, _) = KademliaNode::new(addr1).await.unwrap();
//     let (mut node2, _) = KademliaNode::new(addr2).await.unwrap();
//     let (mut node3, _) = KademliaNode::new(addr3).await.unwrap();

//     node2.bootstrap().await.unwrap();
//     node3.bootstrap().await.unwrap();

//     let key = b"test_key";
//     let value = b"test_value";

//     debug!("Node1 storing key: {:?}, value: {:?}", key, value);
//     node1.put(key, value).await.unwrap();

//     debug!("Node3 attempting to retrieve the value");
//     let retrieved_value = node3.get(key).await.unwrap();

//     debug!("Retrieved value: {:?}", retrieved_value);
//     assert_eq!(retrieved_value, Some(value.to_vec()));

//     debug!("============================");
//     debug!("Completed test: test_find_value_via_multiple_nodes");
//     debug!("============================");
// }

// #[tokio::test]
// async fn test_refresh_buckets() {
//     init_logger();
//     debug!("\n\n\n\n\n");
//     debug!("============================");
//     debug!("Starting test: test_refresh_buckets");
//     debug!("============================");

//     let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let (mut node, _) = KademliaNode::new(addr1).await.unwrap();

//     debug!("Refreshing buckets");
//     node.refresh_buckets().await.unwrap();

//     debug!("Buckets refreshed successfully");

//     assert!(true);

//     debug!("============================");
//     debug!("Completed test: test_refresh_buckets");
//     debug!("============================");
// }
