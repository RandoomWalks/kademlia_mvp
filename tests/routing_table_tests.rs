use super::*;
use tokio;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;


// Helper function to create a random NodeId
fn random_node_id() -> NodeId {
    NodeId::new()
}

// Helper function to create a random SocketAddr
fn random_socket_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), rand::random::<u16>())
}

#[tokio::test]
async fn test_add_single_node() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let node_id = random_node_id();
    let addr = random_socket_addr();

    routing_table.update(node_id, addr);

    assert_eq!(routing_table.find_closest(&node_id, 1).len(), 1);
    assert_eq!(routing_table.find_closest(&node_id, 1)[0].0, node_id);
}

#[tokio::test]
async fn test_add_multiple_nodes() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let num_nodes = 10;

    for _ in 0..num_nodes {
        routing_table.update(random_node_id(), random_socket_addr());
    }

    assert_eq!(routing_table.find_closest(&random_node_id(), num_nodes).len(), num_nodes);
}

#[tokio::test]
async fn test_add_node_to_full_k_bucket() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let bucket_index = 0;
    let k = 20; // Assuming K = 20

    // Fill a k-bucket
    for _ in 0..k {
        let node_id = NodeId::from_slice(&[0u8; 32]); // Ensure all nodes go to the same bucket
        routing_table.update(node_id, random_socket_addr());
    }

    // Try to add one more node
    let new_node_id = NodeId::from_slice(&[0u8; 32]);
    let new_addr = random_socket_addr();
    routing_table.update(new_node_id, new_addr);

    // Verify that the bucket still has only K nodes
    assert_eq!(routing_table.buckets[bucket_index].entries.len(), k);
}

#[tokio::test]
async fn test_update_existing_node() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let node_id = random_node_id();
    let addr1 = random_socket_addr();
    let addr2 = random_socket_addr();

    routing_table.update(node_id, addr1);
    routing_table.update(node_id, addr2);

    let closest = routing_table.find_closest(&node_id, 1);
    assert_eq!(closest.len(), 1);
    assert_eq!(closest[0].1, addr2);
}

#[tokio::test]
async fn test_find_k_closest_nodes() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let num_nodes = 100;
    let k = 20;

    for _ in 0..num_nodes {
        routing_table.update(random_node_id(), random_socket_addr());
    }

    let target = random_node_id();
    let closest = routing_table.find_closest(&target, k);

    assert_eq!(closest.len(), k);
    // Verify that nodes are sorted by distance
    for i in 1..k {
        assert!(target.distance(&closest[i-1].0) <= target.distance(&closest[i].0));
    }
}

#[tokio::test]
async fn test_routing_table_consistency() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let num_operations = 1000;

    for _ in 0..num_operations {
        match rand::random::<u8>() % 2 {
            0 => routing_table.update(random_node_id(), random_socket_addr()),
            _ => {
                let target = random_node_id();
                routing_table.find_closest(&target, 20);
            }
        }
    }

    // Verify that all nodes are in the correct buckets
    for (i, bucket) in routing_table.buckets.iter().enumerate() {
        for entry in &bucket.entries {
            let distance = local_id.distance(&entry.node_id);
            let expected_bucket = 255 - distance.as_bytes().iter().position(|&b| b != 0).unwrap_or(255);
            assert_eq!(i, expected_bucket);
        }
    }
}

#[tokio::test]
async fn test_large_scale_addition() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let num_nodes = 10000;

    let start = Instant::now();
    for _ in 0..num_nodes {
        routing_table.update(random_node_id(), random_socket_addr());
    }
    let duration = start.elapsed();

    println!("Time taken to add {} nodes: {:?}", num_nodes, duration);
    // You might want to set a specific threshold here
    assert!(duration.as_secs() < 10, "Adding nodes took too long");
}

#[tokio::test]
async fn test_lookup_performance() {
    let local_id = random_node_id();
    let mut routing_table = RoutingTable::new(local_id);
    let num_nodes = 10000;
    let num_lookups = 1000;

    // Populate the routing table
    for _ in 0..num_nodes {
        routing_table.update(random_node_id(), random_socket_addr());
    }

    let start = Instant::now();
    for _ in 0..num_lookups {
        let target = random_node_id();
        routing_table.find_closest(&target, 20);
    }
    let duration = start.elapsed();

    println!("Time taken for {} lookups: {:?}", num_lookups, duration);
    let avg_lookup_time = duration.as_nanos() as f64 / num_lookups as f64;
    println!("Average lookup time: {} ns", avg_lookup_time);
    // You might want to set a specific threshold here
    assert!(avg_lookup_time < 1_000_000.0, "Lookups are too slow");
}