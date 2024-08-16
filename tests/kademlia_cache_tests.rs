use kademlia_mvp::message::{Message, FindValueResult};
use kademlia_mvp::node::KademliaNode;
use kademlia_mvp::utils::NodeId;
use kademlia_mvp::cache::policy::{CacheConfig, EvictionPolicy};

// use kademlia_mvp::{KademliaNode, NodeId, Message};
use std::net::SocketAddr;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};


async fn create_node(addr: SocketAddr, config: Option<CacheConfig>) -> KademliaNode {
    let (node, _) = KademliaNode::new(addr, config).await.unwrap();
    node
}

#[tokio::test]
async fn test_store_and_get_default_config() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let mut node = create_node(addr, None).await;
    
    let key = b"test_key";
    let value = b"test_value";
    
    node.store(key, value).await;
    let retrieved = node.get(key).await.unwrap();
    assert_eq!(Some(value.to_vec()), retrieved);
}

#[tokio::test]
async fn test_cache_hit_custom_config() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = CacheConfig {
        eviction_policy: EvictionPolicy::LRU,
        ttl: Duration::from_secs(10),
        max_size: 100,
        maintenance_interval: Duration::from_secs(5),
    };
    let mut node = create_node(addr, Some(config)).await;
    
    let key = b"cache_test_key";
    let value = b"cache_test_value";
    
    node.store(key, value).await;
    
    for _ in 0..5 {
        let retrieved = node.get(key).await.unwrap();
        assert_eq!(Some(value.to_vec()), retrieved);
    }
    
    assert!(node.cache_hit_count().await > 0);
}

#[tokio::test]
async fn test_cache_eviction_custom_config() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = CacheConfig {
        eviction_policy: EvictionPolicy::LRU,
        ttl: Duration::from_secs(1),
        max_size: 50,
        maintenance_interval: Duration::from_secs(2),
    };
    let mut node = create_node(addr, Some(config)).await;
    
    for i in 0..100 {
        let key = format!("key_{}", i).into_bytes();
        let value = format!("value_{}", i).into_bytes();
        node.store(&key, &value).await;
    }
    
    sleep(Duration::from_secs(3)).await;
    
    assert!(node.cache_size().await <= 50);
}

#[tokio::test]
async fn test_different_eviction_policies() {
    let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    let lru_config = CacheConfig {
        eviction_policy: EvictionPolicy::LRU,
        ttl: Duration::from_secs(10),
        max_size: 5,
        maintenance_interval: Duration::from_secs(1),
    };
    let fifo_config = CacheConfig {
        eviction_policy: EvictionPolicy::FIFO,
        ttl: Duration::from_secs(10),
        max_size: 5,
        maintenance_interval: Duration::from_secs(1),
    };
    
    let mut lru_node = create_node(addr1, Some(lru_config)).await;
    let mut fifo_node = create_node(addr2, Some(fifo_config)).await;
    
    for i in 0..10 {
        let key = format!("key_{}", i).into_bytes();
        let value = format!("value_{}", i).into_bytes();
        lru_node.store(&key, &value).await;
        fifo_node.store(&key, &value).await;
    }
    
    // Access some keys in LRU to change their order
    lru_node.get(b"key_0").await.unwrap();
    lru_node.get(b"key_1").await.unwrap();
    
    sleep(Duration::from_secs(2)).await;
    
    // LRU should have kept the most recently used items
    assert!(lru_node.get(b"key_0").await.unwrap().is_some());
    assert!(lru_node.get(b"key_1").await.unwrap().is_some());
    
    // FIFO should have kept the first items inserted
    assert!(fifo_node.get(b"key_0").await.unwrap().is_some());
    assert!(fifo_node.get(b"key_1").await.unwrap().is_some());
    assert!(fifo_node.get(b"key_9").await.unwrap().is_none());
}

#[tokio::test]
async fn test_ttl_expiration() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = CacheConfig {
        eviction_policy: EvictionPolicy::LRU,
        ttl: Duration::from_secs(2),
        max_size: 100,
        maintenance_interval: Duration::from_secs(1),
    };
    let mut node = create_node(addr, Some(config)).await;
    
    let key = b"ttl_test_key";
    let value = b"ttl_test_value";
    
    node.store(key, value).await;
    
    // Value should be present immediately
    assert!(node.get(key).await.unwrap().is_some());
    
    // Wait for TTL to expire
    sleep(Duration::from_secs(3)).await;
    
    // Value should be gone after TTL expiration and maintenance run
    assert!(node.get(key).await.unwrap().is_none());
}

// #[tokio::test]
// async fn test_store_and_get() {
//     let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let (mut node, _) = KademliaNode::new(addr).await.unwrap();
    
//     let key = b"test_key";
//     let value = b"test_value";
    
//     // Store the value
//     node.store(key, value).await;
    
//     // Retrieve the value
//     let retrieved = node.get(key).await.unwrap();
//     assert_eq!(Some(value.to_vec()), retrieved);
// }

// #[tokio::test]
// async fn test_cache_hit() {
//     let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let (mut node, _) = KademliaNode::new(addr).await.unwrap();
    
//     let key = b"cache_test_key";
//     let value = b"cache_test_value";
    
//     // Store the value
//     node.store(key, value).await;
    
//     // Retrieve the value multiple times
//     for _ in 0..5 {
//         let retrieved = node.get(key).await.unwrap();
//         assert_eq!(Some(value.to_vec()), retrieved);
//     }
    
//     // Check cache hit count
//     assert!(node.cache_hit_count().await > 0);
// }

// #[tokio::test]
// async fn test_cache_eviction() {
//     let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let (mut node, _) = KademliaNode::new(addr).await.unwrap();
    
//     // Store multiple key-value pairs
//     for i in 0..100 {
//         let key = format!("key_{}", i).into_bytes();
//         let value = format!("value_{}", i).into_bytes();
//         node.store(&key, &value).await;
//     }
    
//     // Wait for cache maintenance to run (assuming it runs every hour)
//     sleep(Duration::from_secs(3605)).await;
    
//     // Check that some entries have been evicted
//     assert!(node.cache_size().await < 100);
// }

// #[tokio::test]
// async fn test_network_lookup() {
//     let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
//     let (mut node1, _) = KademliaNode::new(addr1).await.unwrap();
//     let (mut node2, _) = KademliaNode::new(addr2).await.unwrap();
    
//     // Add node2 to node1's routing table
//     node1.routing_table.update(node2.id, node2.addr);
    
//     let key = b"network_test_key";
//     let value = b"network_test_value";
    
//     // Store the value in node2
//     node2.store(key, value).await;
    
//     // Try to retrieve the value from node1 (should trigger a network lookup)
//     let retrieved = node1.get(key).await.unwrap();
//     assert_eq!(Some(value.to_vec()), retrieved);
// }

// #[tokio::test]
// async fn test_handle_message() {
//     let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
//     let (mut node, _) = KademliaNode::new(addr).await.unwrap();
    
//     let key = b"message_test_key".to_vec();
//     let value = b"message_test_value".to_vec();
//     let src: SocketAddr = "127.0.0.1:8000".parse().unwrap();
    
//     // Test handling a Store message
//     let store_message = Message::Store { key: key.clone(), value: value.clone() };
//     node.handle_message(store_message, src).await.unwrap();
    
//     // Verify the value was stored
//     let retrieved = node.get(&key).await.unwrap();
//     assert_eq!(Some(value.clone()), retrieved);
    
//     // Test handling a FindValue message
//     let find_value_message = Message::FindValue { key: key.clone() };
//     node.handle_message(find_value_message, src).await.unwrap();
    
//     // You would need to mock the send_message function to verify the response
// }