use crate::utils::NodeId;
use std::net::SocketAddr;
use serde::{Serialize, Deserialize};

/// Enum representing different types of messages exchanged in the network.
#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Ping { sender: NodeId },                          // Ping message to check if a node is alive
    Pong { sender: NodeId },                          // Pong response to a ping
    Store { key: Vec<u8>, value: Vec<u8> },           // Store a key-value pair on a node
    FindNode { target: NodeId },                      // Request to find nodes closest to the target `NodeId`
    FindValue { key: Vec<u8> },                       // Request to find the value associated with a key
    NodesFound(Vec<(NodeId, SocketAddr)>),            // Response containing a list of nodes closest to the target
    ValueFound(Vec<u8>),                              // Response containing the value associated with the key
    Stored,                                           // Acknowledgment that a key-value pair has been stored
}


/// Enum representing the result of a `FindValue` query in the network.
#[derive(Debug)]
pub enum FindValueResult {
    Value(Vec<u8>),                                   // The value found for the requested key
    Nodes(Vec<(NodeId, SocketAddr)>),                 // A list of nodes closer to the target, if the value was not found
}
