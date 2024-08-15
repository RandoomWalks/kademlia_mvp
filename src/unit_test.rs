use crate::message::{FindValueResult, Message};
use crate::node::KademliaNode;
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use log::{debug, info};



// CMD TO RUN TESTS W/ CLEAR DEBUG OUTPUT
// $ RUST_LOG=debug cargo unit  -- --nocapture  --test-threads=1

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
        debug!("Starting test: test_node_id_distance");

        let id1 = NodeId([0u8; 32]);
        let id2 = NodeId([255u8; 32]);
        let distance = id1.distance(&id2);

        debug!("id1: {:?}, id2: {:?}, distance: {:?}", id1, id2, distance);

        for byte in distance.as_bytes() {
            assert_eq!(*byte, 255u8);
        }
        
        debug!("Completed test: test_node_id_distance");
    }

    #[tokio::test]
    async fn test_node_id_ordering() {
        init_logger();
        debug!("Starting test: test_node_id_ordering");

        let id1 = NodeId([1u8; 32]);
        let id2 = NodeId([2u8; 32]);

        debug!("id1: {:?}, id2: {:?}", id1, id2);
        assert!(id1 < id2);

        debug!("Completed test: test_node_id_ordering");
    }

    #[tokio::test]
    async fn test_routing_table_update() {
        init_logger();
        debug!("Starting test: test_routing_table_update");

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
        assert!(bucket.entries.iter().any(|entry| entry.node_id == node_to_add));

        debug!("Completed test: test_routing_table_update");
    }

    #[tokio::test]
    async fn test_routing_table_find_closest() {
        init_logger();
        debug!("Starting test: test_routing_table_find_closest");

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

        debug!("Completed test: test_routing_table_find_closest");
    }

    #[tokio::test]
    async fn test_kademlia_node_store_and_find_value() {
        init_logger();
        debug!("Starting test: test_kademlia_node_store_and_find_value");

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

        debug!("Completed test: test_kademlia_node_store_and_find_value");
    }

    #[tokio::test]
    async fn test_kademlia_node_ping() {
        init_logger();
        debug!("Starting test: test_kademlia_node_ping");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let ping_addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        debug!("Sending ping to address: {:?}", ping_addr);

        let result = node.ping(ping_addr).await;
        assert!(result.is_ok());

        debug!("Ping result: {:?}", result);
        debug!("Completed test: test_kademlia_node_ping");
    }

    #[tokio::test]
    async fn test_kademlia_node_put_and_get() {
        init_logger();
        debug!("Starting test: test_kademlia_node_put_and_get");

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

        debug!("Completed test: test_kademlia_node_put_and_get");
    }

    #[tokio::test]
    async fn test_kademlia_node_handle_message_ping() {
        init_logger();
        debug!("Starting test: test_kademlia_node_handle_message_ping");

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

        debug!("Completed test: test_kademlia_node_handle_message_ping");
    }

    #[tokio::test]
    async fn test_kademlia_node_handle_message_store() {
        init_logger();
        debug!("Starting test: test_kademlia_node_handle_message_store");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let key = b"mykey".to_vec();
        let value = b"myvalue".to_vec();
        let src: SocketAddr = "127.0.0.1:8081".parse().unwrap();

        debug!("Handling Store message from source: {:?}, key: {:?}, value: {:?}", src, key, value);

        node.handle_message(
            Message::Store {
                key: key.clone(),
                value: value.clone(),
            },
            src,
        )
        .await
        .unwrap();

        let stored_value = node.storage.get(&KademliaNode::hash_key(&key));
        debug!("Stored value in node: {:?}", stored_value);

        assert_eq!(stored_value, Some(&value));

        debug!("Completed test: test_kademlia_node_handle_message_store");
    }

    #[tokio::test]
    async fn test_kademlia_node_find_node() {
        init_logger();
        debug!("Starting test: test_kademlia_node_find_node");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let target_id = NodeId::new();
        debug!("Target NodeId: {:?}", target_id);

        let closest_nodes = node.find_node(&target_id);
        debug!("Closest nodes found: {:?}", closest_nodes);

        // Test if closest nodes are returned, initially should be empty
        assert_eq!(closest_nodes.len(), 0);

        debug!("Completed test: test_kademlia_node_find_node");
    }

    #[tokio::test]
    async fn test_kademlia_node_refresh_buckets() {
        init_logger();
        debug!("Starting test: test_kademlia_node_refresh_buckets");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut node, _) = KademliaNode::new(addr).await.unwrap();
        let actual_addr = node.socket.local_addr().unwrap();

        debug!("Node address: {:?}", actual_addr);

        let result = node.refresh_buckets().await;
        debug!("Refresh buckets result: {:?}", result);

        assert!(result.is_ok());

        debug!("Completed test: test_kademlia_node_refresh_buckets");
    }
}