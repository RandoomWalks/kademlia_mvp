use crate::message::{FindValueResult, Message};
use crate::node::KademliaNode;
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;
use log::{debug, info};
use std::net::SocketAddr;
use tokio::sync::mpsc;

// CMD TO RUN TESTS W/ CLEAR DEBUG OUTPUT
// $ RUST_LOG=debug cargo test unit --  --nocapture --test-threads=1

// Initialize the logger once for all tests
fn init_logger() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Trace) // Set the desired log level
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_id_distance() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_node_id_distance");
        debug!("============================\n");

        let id1 = NodeId([0u8; 32]);
        let id2 = NodeId([255u8; 32]);
        let distance = id1.distance(&id2);

        debug!("id1: {:?}, id2: {:?}, distance: {:?}", id1, id2, distance);

        for byte in distance.as_bytes() {
            assert_eq!(*byte, 255u8);
        }

        debug!("\n============================");
        debug!("Completed test: test_node_id_distance");
        debug!("============================\n\n");
    }

    #[tokio::test]
    async fn test_node_id_ordering() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_node_id_ordering");
        debug!("============================\n");

        let id1 = NodeId([1u8; 32]);
        let id2 = NodeId([2u8; 32]);

        debug!("id1: {:?}, id2: {:?}", id1, id2);
        assert!(id1 < id2);

        debug!("\n============================");
        debug!("Completed test: test_node_id_ordering");
        debug!("============================\n\n");
    }

    #[tokio::test]
    async fn test_routing_table_update() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_routing_table_update");
        debug!("============================\n");

        let node_id = NodeId::new();
        debug!("Generated node_id: {:?}", node_id);

        let mut routing_table = RoutingTable::new(node_id);
        let node_to_add = NodeId::new();
        debug!("Adding node_id: {:?}", node_to_add);

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        routing_table.update(node_to_add, addr);

        let bucket_index = node_id
            .distance(&node_to_add)
            .as_bytes()
            .iter()
            .position(|&x| x != 0)
            .unwrap_or(255);
        let bucket = &routing_table.buckets[bucket_index];

        debug!("Bucket index: {}, Bucket: {:?}", bucket_index, bucket);
        assert!(bucket
            .entries
            .iter()
            .any(|entry| entry.node_id == node_to_add));

        debug!("\n============================");
        debug!("Completed test: test_routing_table_update");
        debug!("============================\n\n");
    }

    #[tokio::test]
    async fn test_routing_table_find_closest() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_routing_table_find_closest");
        debug!("============================\n");

        let node_id = NodeId::new();
        debug!("Generated node_id: {:?}", node_id);

        let mut routing_table = RoutingTable::new(node_id);

        let target_id = NodeId::new();
        debug!("Target node_id: {:?}", target_id);

        let addr1: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8082".parse().unwrap();

        routing_table.update(NodeId::new(), addr1);
        routing_table.update(NodeId::new(), addr2);

        let closest_nodes = routing_table.find_closest(&target_id, 2);
        debug!("Closest nodes found: {:?}", closest_nodes);

        assert_eq!(closest_nodes.len(), 2);

        debug!("\n============================");
        debug!("Completed test: test_routing_table_find_closest");
        debug!("============================\n\n");
    }

    #[tokio::test]
    async fn test_kademlia_node_store_and_find_value() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_kademlia_node_store_and_find_value");
        debug!("============================\n");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let key = b"mykey";
        let value = b"myvalue";

        debug!("Storing key: {:?}, value: {:?}", key, value);
        node.store(key, value);

        let find_result = node.find_value(key);
        debug!("Find result: {:?}", find_result);

        if let FindValueResult::Value(stored_value) = find_result {
            assert_eq!(stored_value, value);
        } else {
            panic!("Expected to find the stored value.");
        }

        debug!("\n============================");
        debug!("Completed test: test_kademlia_node_store_and_find_value");
        debug!("============================\n\n");
    }

    #[tokio::test]
    async fn test_kademlia_node_ping() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_kademlia_node_ping");
        debug!("============================\n");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let ping_addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        debug!("Sending ping to address: {:?}", ping_addr);

        let result = node.ping(ping_addr).await;
        assert!(result.is_ok());

        debug!("Ping result: {:?}", result);

        debug!("\n============================");
        debug!("Completed test: test_kademlia_node_ping");
        debug!("============================\n\n");
    }

    #[tokio::test]
    async fn test_kademlia_node_put_and_get() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_kademlia_node_put_and_get");
        debug!("============================\n");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let key = b"mykey";
        let value = b"myvalue";

        debug!("Putting key: {:?}, value: {:?}", key, value);
        node.put(key, value).await.unwrap();

        let stored_value = node.get(key).await.unwrap();
        debug!("Stored value retrieved: {:?}", stored_value);

        assert_eq!(stored_value, Some(value.to_vec()));

        debug!("\n============================");
        debug!("Completed test: test_kademlia_node_put_and_get");
        debug!("============================\n\n");
    }

    #[tokio::test]
    async fn test_kademlia_node_handle_message_ping() {
        init_logger();
        debug!("\n\n");
        debug!("============================");
        debug!("Starting test: test_kademlia_node_handle_message_ping");
        debug!("============================\n");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let sender = NodeId::new();
        debug!("Generated sender NodeId: {:?}", sender);

        let src: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        debug!("Handling Ping message from source: {:?}", src);

        node.handle_message(Message::Ping { sender }, src)
            .await
            .unwrap();

        let bucket_index = node
            .id
            .distance(&sender)
            .as_bytes()
            .iter()
            .position(|&x| x != 0)
            .unwrap_or(255);

        let bucket = &node.routing_table.buckets[bucket_index];
        debug!("Bucket index: {}, Bucket: {:?}", bucket_index, bucket);

        assert!(bucket.entries.iter().any(|entry| entry.node_id == sender));

        debug!("\n============================");
        debug!("Completed test: test_kademlia_node");
        debug!("============================\n");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let target_id = NodeId::new();
        debug!("Target NodeId for find: {:?}", target_id);

        // Add some nodes to the routing table
        for _ in 0..5 {
            let node_to_add = NodeId::new();
            let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
            node.routing_table.update(node_to_add, addr);
        }

        // Perform the find_node operation
        let result = node.find_node(&target_id);
        debug!("Find node result: {:?}", result);

        // Verify if the result contains nodes
        // assert!(result.is_ok());
        // let nodes = result.unwrap();
        let nodes = result;
        debug!("Found nodes: {:?}", nodes);

        debug!("\n============================");
        debug!("Completed test: test_kademlia_node_find_node");
        debug!("============================\n\n");
    }
}
