use bincode;
use log::{debug};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use crate::node::NodeId;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum KademliaMessage {
    Ping(NodeId),
    Pong(NodeId),
    FindNode(NodeId, NodeId), // (sender_id, target_id)
    FindNodeResponse(NodeId, Vec<(NodeId, SocketAddr)>),
    Store(NodeId, Vec<u8>, Vec<u8>), // (sender_id, key, value)
    FindValue(NodeId, Vec<u8>),      // (sender_id, key)
    FindValueResponse(NodeId, Option<Vec<u8>>, Vec<(NodeId, SocketAddr)>), // (sender_id, value, closest_nodes)
}

// Implementation of KademliaMessage (serialize, deserialize)
// ... (rest of the message logic)


impl KademliaMessage {
    pub fn serialize(&self) -> Vec<u8> {
        let serialized = bincode::serialize(self).unwrap();
        debug!("Serialized message: {:?} to bytes: {:?}", self, serialized);
        serialized
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let deserialized: Self = bincode::deserialize(data)?;
        debug!("Deserialized bytes: {:?} into message: {:?}", data, deserialized);
        Ok(deserialized)
    }
}