use crate::message::{FindValueResult, Message};
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::{ UdpSocket};
use std::net::SocketAddr;

use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::utils::{ALPHA, BOOTSTRAP_NODES, K};
use bincode::{deserialize, serialize};

pub struct KademliaNode {
    id: NodeId,
    addr: SocketAddr,
    routing_table: RoutingTable,
    storage: HashMap<Vec<u8>, Vec<u8>>,
    socket: Arc<UdpSocket>,
    shutdown: mpsc::Receiver<()>,
}

impl KademliaNode {
    pub async fn new(addr: SocketAddr) -> std::io::Result<(Self, mpsc::Sender<()>)> {
        let id = NodeId::new();
        let socket = UdpSocket::bind(addr).await?;
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        Ok((
            KademliaNode {
                id: id.clone(),
                addr,
                routing_table: RoutingTable::new(id),
                storage: HashMap::new(),
                socket: Arc::new(socket),
                shutdown: shutdown_receiver,
            },
            shutdown_sender,
        ))
    }

    pub async fn bootstrap(&mut self) -> std::io::Result<()> {
        for &bootstrap_addr in BOOTSTRAP_NODES.iter() {
            match bootstrap_addr.parse() {
                Ok(addr) => {
                    if let Err(e) = self.ping(addr).await {
                        error!(
                            "Failed to ping bootstrap node {:#?}: {:?}",
                            bootstrap_addr, e
                        );
                    }
                }
                Err(e) => error!("Invalid bootstrap address {:#?}: {:?}", bootstrap_addr, e),
            }
        }
        Ok(())
    }

    pub async fn run(&mut self) -> std::io::Result<()> {
        let mut buf = vec![0u8; 1024];
        let mut refresh_interval = interval(Duration::from_secs(3600)); // Refresh every hour

        loop {
            tokio::select! {
                Ok((size, src)) = self.socket.recv_from(&mut buf) => {
                    match deserialize(&buf[..size]) {
                        Ok(message) => {
                            if let Err(e) = self.handle_message(message, src).await {
                                error!("Failed to handle message from {:#?}: {:?}", src, e);
                            }
                        }
                        Err(e) => error!("Failed to deserialize message from {:#?}: {:?}", src, e),
                    }
                }
                _ = refresh_interval.tick() => {
                    if let Err(e) = self.refresh_buckets().await {
                        error!("Failed to refresh buckets: {:?}", e);
                    }
                }
                _ = self.shutdown.recv() => {
                    info!("Received shutdown signal, stopping node");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_message(&mut self, message: Message, src: SocketAddr) -> std::io::Result<()> {
        match message {
            Message::Ping { sender } => {
                self.routing_table.update(sender, src);
                self.send_message(&Message::Pong { sender: self.id }, src).await?;
                info!("Received Ping from {:#?}, responded with Pong", src);
            }
            Message::Pong { sender } => {
                self.routing_table.update(sender, src);
                info!("Received Pong from {:#?}", src);
            }
            Message::Store { key, value } => {
                self.store(&key, &value);
                self.send_message(&Message::Stored, src).await?;
                info!("Stored value for key {:?} from {:#?}", key, src);
            }
            Message::FindNode { target } => {
                let nodes = self.find_node(&target);
                self.send_message(&Message::NodesFound(nodes), src).await?;
                info!(
                    "Received FindNode from {:#?}, responded with NodesFound",
                    src
                );
            }
            Message::FindValue { key } => match self.find_value(&key) {
                FindValueResult::Value(value) => {
                    self.send_message(&Message::ValueFound(value), src).await?;
                    info!("Found value for key {:?} from {:#?}", key, src);
                }
                FindValueResult::Nodes(nodes) => {
                    self.send_message(&Message::NodesFound(nodes), src).await?;
                    info!("NodesFound for key {:?} from {:#?}", key, src);
                }
            },
            _ => warn!("Received unknown message type from {:#?}", src),
        }
        Ok(())
    }

    async fn send_message(&self, message: &Message, dst: SocketAddr) -> std::io::Result<()> {
        let serialized = serialize(message).map_err(|e| {
            error!("Failed to serialize message: {:?}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Serialization error")
        })?;
        self.socket.send_to(&serialized, dst).await?;
        debug!("Sent message to {:#?}", dst);
        Ok(())
    }

    pub fn store(&mut self, key: &[u8], value: &[u8]) {
        let hash = Self::hash_key(key);
        self.storage.insert(hash.to_vec(), value.to_vec());
        info!("Stored value for key: {:?}", hash);
    }

    fn hash_key(key: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.finalize().to_vec()
    }

    pub fn find_node(&self, target: &NodeId) -> Vec<(NodeId, SocketAddr)> {
        self.routing_table.find_closest(target, K)
    }

    pub fn find_value(&self, key: &[u8]) -> FindValueResult {
        let hash = Self::hash_key(key);
        if let Some(value) = self.storage.get(&hash) {
            FindValueResult::Value(value.clone())
        } else {
            let hash_array: [u8; 32] = hash[..].try_into().expect("Hash length is not 32 bytes");
            let target = NodeId::from_slice(&hash_array);
            FindValueResult::Nodes(self.find_node(&target))
        }
    }

    pub async fn ping(&mut self, addr: SocketAddr) -> std::io::Result<()> {
        self.send_message(&Message::Ping { sender: self.id }, addr).await
    }

    pub async fn put(&mut self, key: &[u8], value: &[u8]) -> std::io::Result<()> {
        let hash = Self::hash_key(key);
        let hash_array: [u8; 32] = hash[..].try_into().expect("Hash length is not 32 bytes");
        let target = NodeId::from_slice(&hash_array);

        let nodes = self.find_node(&target);

        for (_, addr) in nodes.iter().take(ALPHA) {
            if let Err(e) = self.send_message(
                &Message::Store {
                    key: key.to_vec(),
                    value: value.to_vec(),
                },
                *addr,
            ).await {
                error!("Failed to send Store message to {:#?}: {:?}", addr, e);
            }
        }

        self.store(key, value);
        Ok(())
    }

    pub async fn get(&mut self, key: &[u8]) -> std::io::Result<Option<Vec<u8>>> {
        if let Some(value) = self.storage.get(&Self::hash_key(key)) {
            return Ok(Some(value.clone()));
        }

        let hash = Self::hash_key(key);
        let hash_array: [u8; 32] = hash[..].try_into().expect("Hash length is not 32 bytes");
        let target = NodeId::from_slice(&hash_array);

        let nodes = self.find_node(&target);

        for (_, addr) in nodes.iter().take(ALPHA) {
            if let Err(e) = self.send_message(&Message::FindValue { key: key.to_vec() }, *addr).await {
                error!("Failed to send FindValue message to {:#?}: {:?}", addr, e);
            }
        }

        // For simplicity, we're just returning None here
        // In a real implementation, we would wait for responses and return the value if found
        Ok(None)
    }

    async fn refresh_buckets(&mut self) -> std::io::Result<()> {
        // Implement bucket refreshing logic here
        info!("Refreshing buckets");
        Ok(())
    }
}