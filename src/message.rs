use crate::utils::NodeId;
use std::net::SocketAddr;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Ping { sender: NodeId },
    Pong { sender: NodeId },
    Store { key: Vec<u8>, value: Vec<u8> },
    FindNode { target: NodeId },
    FindValue { key: Vec<u8> },
    NodesFound(Vec<(NodeId, SocketAddr)>),
    ValueFound(Vec<u8>),
    Stored,
}

#[derive(Debug)]
pub enum FindValueResult {
    Value(Vec<u8>),
    Nodes(Vec<(NodeId, SocketAddr)>),
}
