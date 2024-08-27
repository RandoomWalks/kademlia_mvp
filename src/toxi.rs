use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::time::sleep;
use toxiproxy_rust::{TOXIPROXY, proxy::Proxy};

use super::*;

const BASE_PORT: u16 = 9000;

async fn create_node(port: u16) -> Node {
    Node::new(&format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to create node")
}

#[tokio::test]
async fn test_latency() {
    let proxy1 = TOXIPROXY.find_proxy("node1").unwrap();
    let proxy2 = TOXIPROXY.find_proxy("node2").unwrap();

    let mut node1 = create_node(BASE_PORT + 1000).await;
    let mut node2 = create_node(BASE_PORT + 1001).await;

    // Add latency to the network
    proxy1
        .with_latency("downstream".into(), 100, 10, 1.0)
        .apply(|| {
            // This closure is synchronous
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let start = std::time::Instant::now();
                node2.bootstrap(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), BASE_PORT)])
                    .await
                    .expect("Failed to bootstrap node2");
                let duration = start.elapsed();
                assert!(duration >= Duration::from_millis(100));
            });
        });

    // Clean up
    proxy1.delete().expect("Failed to delete proxy1");
    proxy2.delete().expect("Failed to delete proxy2");
}

#[tokio::test]
async fn test_packet_loss() {
    let proxy1 = TOXIPROXY.find_proxy("node1").unwrap();
    let proxy2 = TOXIPROXY.find_proxy("node2").unwrap();

    let mut node1 = create_node(BASE_PORT + 1002).await;
    let mut node2 = create_node(BASE_PORT + 1003).await;

    // Add 50% packet loss
    proxy1
        .with_limit_data("downstream".into(), 0, 0.5)
        .apply(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut success = false;
                for _ in 0..10 {
                    if node2.bootstrap(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), BASE_PORT + 2)])
                        .await
                        .is_ok()
                    {
                        success = true;
                        break;
                    }
                    sleep(Duration::from_millis(100)).await;
                }
                assert!(success, "Failed to bootstrap after multiple attempts with packet loss");
            });
        });

    // Clean up
    proxy1.delete().expect("Failed to delete proxy1");
    proxy2.delete().expect("Failed to delete proxy2");
}

#[tokio::test]
async fn test_bandwidth_limit() {
    let proxy1 = TOXIPROXY.find_proxy("node1").unwrap();
    let proxy2 = TOXIPROXY.find_proxy("node2").unwrap();

    let mut node1 = create_node(BASE_PORT + 1004).await;
    let mut node2 = create_node(BASE_PORT + 1005).await;

    // Limit bandwidth to 1KB/s
    proxy1
        .with_bandwidth("downstream".into(), 1024, 1.0)
        .apply(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let key = vec![1; 32];
                let value = vec![2; 10 * 1024];

                let start = std::time::Instant::now();
                node1.store(key.clone(), value.clone()).await.expect("Failed to store value");
                let retrieved_value = node2.find_value(key).await.expect("Failed to find value");
                let duration = start.elapsed();

                assert_eq!(retrieved_value, Some(value));
                assert!(duration >= Duration::from_secs(10));
            });
        });

    // Clean up
    proxy1.delete().expect("Failed to delete proxy1");
    proxy2.delete().expect("Failed to delete proxy2");
}




#[tokio::test]
async fn test_network_partition() {
    let proxy1 = TOXIPROXY.find_proxy("node1").unwrap();
    let proxy2 = TOXIPROXY.find_proxy("node2").unwrap();

    let mut node1 = create_node(BASE_PORT + 1006).await;
    let mut node2 = create_node(BASE_PORT + 1007).await;

    // Bootstrap nodes
    node2.bootstrap(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), BASE_PORT + 6)])
        .await
        .expect("Failed to bootstrap node2");

    let key = vec![3; 32];
    let value = vec![4; 1024];
    node1.store(key.clone(), value.clone()).await.expect("Failed to store value");

    // Simulate network partition
    proxy1.with_down(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = node2.find_value(key.clone()).await;
            assert!(result.is_err() || result.unwrap().is_none());
        });
    });

    // Clean up
    proxy1.delete().expect("Failed to delete proxy1");
    proxy2.delete().expect("Failed to delete proxy2");
}

#[tokio::test]
async fn test_slow_close() {
    let proxy1 = TOXIPROXY.find_proxy("node1").unwrap();
    let proxy2 = TOXIPROXY.find_proxy("node2").unwrap();

    let mut node1 = create_node(BASE_PORT + 1008).await;
    let mut node2 = create_node(BASE_PORT + 1009).await;

    // Add slow close toxic
    proxy1
        .with_slow_close("downstream".into(), 1000, 1.0)
        .apply(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let start = std::time::Instant::now();
                node2.bootstrap(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), BASE_PORT + 8)])
                    .await
                    .expect("Failed to bootstrap node2");
                let duration = start.elapsed();
                assert!(duration >= Duration::from_secs(1));
            });
        });

    // Clean up
    proxy1.delete().expect("Failed to delete proxy1");
    proxy2.delete().expect("Failed to delete proxy2");
}