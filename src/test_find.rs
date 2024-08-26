// use crate::cache::{cache_impl, policy};
// use crate::message::{FindValueResult, Message};
// use crate::routing_table::RoutingTable;
// use crate::utils::{NodeId,KademliaError};
// use std::collections::HashMap;
// use std::net::SocketAddr;
// use std::sync::Arc;
// use std::time::{Duration, SystemTime};
// use tokio::sync::Mutex;

// use crate::node::{
//      Delay, KademliaNode, NetworkInterface, StorageManager, TimeProvider,
// };

// use crate::utils::{Config};

// use log::{debug, error, info, warn};
// use serde::{Deserialize, Serialize};
// use sha2::{Digest, Sha256};
// use std::collections::HashSet;
// use std::fmt;
// use std::future::Future;
// use std::pin::Pin;
// use tokio::net::UdpSocket;
// use tokio::sync::mpsc;
// use tokio::time::interval;

// use bincode::{deserialize, serialize};


// #[cfg(test)]
// mod test {
//     struct MockNetworkIntf {
        
//         // sent msgs
//         sent_messages: Arc<Mutex< Vec<(Vec<u8>,SocketAddr)>>>,
//         // rcvd msgs
//         received_messages: Arc<Mutex< Vec<(Vec<u8>,SocketAddr)>>>,
    
//         // stored_data
//         stored_data:Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
//     }
    
//     #[tokio::test]
//     async fn test_find_node_rpc() {
//         // MockNetworkIntf
    
//         // generate target NodeId
    
//         // local host socket addr
    
//         // harness send serialized()
    
//         // harness.push( )
        
//         // NodesFound(Vec<(NodeId, SocketAddr)>), // Response containing a list of nodes closest to the target
        
//         // assert(rcvd_msg.1  , addr )
        
//         // node like 
//         // { Id, address , routing_table  }
//         // { id::NodeId::new(), addr:"127.0.0.1.1000".parse().unwrap() , routing_table : Arc::new(HashMap::new()) }
        
    
//         // let target_id = NodeId::new( NodeId(val),"127.0.0.1005");
//         // let mock_network_intf.send( (Message::FindNode{target_id  ,addr} )).await;
        
//         // let mock_network_intf.recv_from().await.unwrap();
        
//         // assert(rcvd_msg.1  , addr )
    
//         // = NodeId()
        
        
//         let cfg = Arc::new(Config::default());
//         let time_provider = Arc::new(MockTimeProvider);
//         let delay_provider = Arc::new(MockDelay);
//         let mock_network = Arc::new(MockNetworkIntf::new()); 
    
//         let node1 = KademliaNode::new(
//             "127.0.0.1.1000".parse().unwrap(),
//             Some(config.clone()),
//             time_provider.clone(),
//             delay_provider.clone(),
//             mock_network.clone(),
//             vec![],
//         ).await.unwrap();
        
//         let node1 = KademliaNode::new(
//             "127.0.0.1.1000".parse().unwrap(),
//             Some(config.clone()),
//             time_provider.clone(),
//             delay_provider.clone(),
//             mock_network.clone(),
//             vec![],
//         ).await.unwrap();
//         let node2 = KademliaNode::new(
//             "127.0.0.1.1001".parse().unwrap(),
//             Some(config.clone()),
//             time_provider.clone(),
//             delay_provider.clone(),
//             mock_network.clone(),
//             vec![],
//         ).await.unwrap();
//         let node3 = KademliaNode::new(
//             "127.0.0.1.1002".parse().unwrap(),
//             Some(config.clone()),
//             time_provider.clone(),
//             delay_provider.clone(),
//             mock_network.clone(),
//             vec![],
//         ).await.unwrap();
//         let node4 = KademliaNode::new(
//             "127.0.0.1.1003".parse().unwrap(),
//             Some(config.clone()),
//             time_provider.clone(),
//             delay_provider.clone(),
//             mock_network.clone(),
//             vec![],
//         ).await.unwrap();
    
//         // let node2_b = NodeId{0:node2.0};
    
//         node1.0.routing_table.update(node2.0.id.clone(), "127.0.0.1.1000".parse().unwrap());
//         node1.0.routing_table.update(node3.0.id.clone(), "127.0.0.1.1001".parse().unwrap());
//         node2.0.routing_table.update(node4.0.id.clone(), "127.0.0.1.1002".parse().unwrap());
//         node3.0.routing_table.update(node4.0.id.clone(), "127.0.0.1.1003".parse().unwrap());
    
//         let tgt_id = node4.id.clone();
    
//         // Message::
    
//         let closest_nodes = node1.find_node( tgt_id  ).await;
    
//         if let(k_closest ) = closest_nodes {
//             // assert correct
            
//             assert!(node2.routing_table.find_closest(&tgt_id,config.k ).contains(&(node4.id.clone() , node4.addr)) );
            
//         }
    
//     }
    


// }    

