use super::*;
use mockall::predicate::*;
use mockall::mock;
use std::net::{IpAddr, Ipv4Addr};
use tokio::sync::Mutex;
// use toxiproxy_rust::Toxiproxy;
use toxiproxy_rust::{TOXIPROXY, proxy::ProxyPack};
use crate::node::NodeId;
use crate::message::KademliaMessage;
use crate::routing::{KBucket, RoutingTable};


async fn create_node(port: u16) -> Node {
    Node::new(&format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to create node")
}

mock! {
    pub Node {
        fn id(&self) -> NodeId;
        fn addr(&self) -> SocketAddr;
        async fn ping(&self, addr: SocketAddr) -> Result<NodeId, Box<dyn std::error::Error>>;
        async fn find_node(&self, id: NodeId, addr: SocketAddr, target_id: NodeId) -> Result<Vec<(NodeId, SocketAddr)>, Box<dyn std::error::Error>>;
        async fn store(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), Box<dyn std::error::Error>>;
        async fn find_value(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>>;
    }

    impl Clone for Node {
        fn clone(&self) -> Self;
    }
}

mock! {
    pub RoutingTableTest {
        fn new(node_id: NodeId) -> Self;
        fn add_node(&mut self, node: (NodeId, SocketAddr)) -> bool;
        fn find_closest_nodes(&self, target: &NodeId, count: usize) -> Vec<(NodeId, SocketAddr)>;
    }

    impl Clone for RoutingTableTest {
        fn clone(&self) -> Self;
    }
}

// impl Default for MockRoutingTableTest {
//     fn default() -> Self {
//         Self::new(NodeId::new())
//     }
// }

#[tokio::test]
async fn test_node_ping() {
    let mut mock_node = MockNode::new();
    let target_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let expected_id = NodeId::new();

    mock_node
        .expect_ping()
        .with(eq(target_addr))
        .times(1)
        .returning(move |_| Ok(expected_id));

    let result = mock_node.ping(target_addr).await.unwrap();
    assert_eq!(result, expected_id);
}

#[tokio::test]
async fn test_node_find_node() {
    let mut mock_node = MockNode::new();
    let target_id = NodeId::new();
    let target_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let search_target = NodeId::new();
    let expected_result = vec![(NodeId::new(), target_addr)];

    mock_node
        .expect_find_node()
        .with(eq(target_id), eq(target_addr), eq(search_target))
        .times(1)
        .returning(move |_, _, _| Ok(expected_result.clone()));

    let result = mock_node.find_node(target_id, target_addr, search_target).await.unwrap();
    assert_eq!(result, vec![(NodeId::new(), target_addr)]);
}

#[tokio::test]
async fn test_node_store() {
    let mut mock_node = MockNode::new();
    let key = vec![1, 2, 3];
    let value = vec![4, 5, 6];

    mock_node
        .expect_store()
        .with(eq(key.clone()), eq(value.clone()))
        .times(1)
        .returning(|_, _| Ok(()));

    let result = mock_node.store(key, value).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_node_find_value() {
    let mut mock_node = MockNode::new();
    let key = vec![1, 2, 3];
    let expected_value = Some(vec![4, 5, 6]);

    mock_node
        .expect_find_value()
        .with(eq(key.clone()))
        .times(1)
        .returning(move |_| Ok(expected_value.clone()));

    let result = mock_node.find_value(key).await.unwrap();
    assert_eq!(result, Some(vec![4, 5, 6]));
}

#[test]
fn test_routing_table_add_node() {
    let mut mock_routing_table = MockRoutingTableTest::default();
    let new_node = (NodeId::new(), SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080));

    mock_routing_table
        .expect_add_node()
        .with(eq(new_node))
        .times(1)
        .returning(|_| true);

    let result = mock_routing_table.add_node(new_node);
    assert!(result);
}

#[test]
fn test_routing_table_find_closest_nodes() {
    let mut mock_routing_table = MockRoutingTableTest::default();
    let target = NodeId::new();
    let count = 3;
    let expected_result = vec![
        (NodeId::new(), SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)),
        (NodeId::new(), SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081)),
        (NodeId::new(), SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8082)),
    ];
    let expected_result_clone = expected_result.clone();

    mock_routing_table
        .expect_find_closest_nodes()
        .with(eq(target), eq(count))
        .times(1)
        .returning(move |_, _| expected_result_clone.clone());

    let result = mock_routing_table.find_closest_nodes(&target, count);
    assert_eq!(result, expected_result);
}

#[tokio::test]
async fn test_node_bootstrap() {
    let mut mock_node = MockNode::new();
    let bootstrap_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let bootstrap_id = NodeId::new();

    mock_node
        .expect_ping()
        .with(eq(bootstrap_addr))
        .times(1)
        .returning(move |_| Ok(bootstrap_id));

    mock_node
        .expect_find_node()
        .times(1)
        .returning(|_, _, _| Ok(vec![]));

    let mut node = Node {
        id: NodeId::new(),
        addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081),
        routing_table: Arc::new(Mutex::new(RoutingTable::new(NodeId::new()))),
        socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
        storage: Arc::new(Mutex::new(HashMap::new())),
    };

    let result = node.bootstrap(vec![bootstrap_addr]).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_node_handle_ping() {
    let mut mock_node = MockNode::new();
    let sender_id = NodeId::new();
    let sender_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

    mock_node
        .expect_id()
        .returning(|| NodeId::new());

    let mut node = Node {
        id: NodeId::new(),
        addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081),
        routing_table: Arc::new(Mutex::new(RoutingTable::new(NodeId::new()))),
        socket: Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
        storage: Arc::new(Mutex::new(HashMap::new())),
    };

    let msg = KademliaMessage::Ping(sender_id).serialize();
    let result = node.handle_message(&msg, sender_addr).await;
    assert!(result.is_ok());
}

// Add more tests for other message types (FindNode, Store, FindValue)

#[test]
fn test_k_bucket_add_node() {
    let mut bucket = KBucket::new();
    let node = (NodeId::new(), SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080));

    assert!(bucket.add_node(node));
    assert_eq!(bucket.nodes.len(), 1);

    // Try adding the same node again
    assert!(!bucket.add_node(node));
    assert_eq!(bucket.nodes.len(), 1);

    // Fill the bucket
    for _ in 1..K {
        bucket.add_node((NodeId::new(), SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)));
    }

    // Try adding a node to a full bucket
    let new_node = (NodeId::new(), SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080));
    assert!(!bucket.add_node(new_node));
    assert_eq!(bucket.nodes.len(), K);
}

#[test]
fn test_node_id_distance() {
    let id1 = NodeId::new_val([0; 32]);
    let id2 = NodeId::new_val([255; 32]);
    let distance = id1.distance(&id2);
    assert_eq!(distance, [255; 32]);

    let id3 = NodeId::new_val([1; 32]);
    let distance = id1.distance(&id3);
    assert_eq!(distance, [1; 32]);
}

// Add more tests as needed