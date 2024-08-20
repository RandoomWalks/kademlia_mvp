use crate::cache::cache_impl;
use crate::cache::entry;
use crate::cache::policy;
use crate::message::{FindValueResult, Message};
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::utils::{ALPHA, BOOTSTRAP_NODES, K};
use bincode::{deserialize, serialize};

// Error types
#[derive(Debug)]
pub enum KademliaError {
    Network(std::io::Error),
    Serialization(bincode::Error),
    Timeout,
    UnexpectedResponse,
    InvalidData(&'static str),
}

impl From<std::io::Error> for KademliaError {
    fn from(error: std::io::Error) -> Self {
        KademliaError::Network(error)
    }
}

impl From<bincode::Error> for KademliaError {
    fn from(error: bincode::Error) -> Self {
        KademliaError::Serialization(error)
    }
}

// Traits
pub trait NetworkInterface: Send + Sync {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> std::io::Result<usize>;
    fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)>;
}

pub trait TimeProvider: Send + Sync {
    fn now(&self) -> SystemTime;
}

pub trait Delay: Send + Sync {
    fn delay(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

// Configuration
#[derive(Clone, Debug)]
pub struct Config {
    pub k: usize,
    pub alpha: usize,
    pub request_timeout: Duration,
    pub cache_size: usize,
    pub cache_ttl: Duration,
    pub maintenance_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            k: 20,
            alpha: 3,
            request_timeout: Duration::from_secs(5),
            cache_size: 1000,
            cache_ttl: Duration::from_secs(3600),
            maintenance_interval: Duration::from_secs(3600),
        }
    }
}

// Network Manager
pub struct NetworkManager {
    socket: Arc<dyn NetworkInterface>,
    config: Arc<Config>,
}

impl NetworkManager {
    pub fn new(socket: Arc<dyn NetworkInterface>, config: Arc<Config>) -> Self {
        NetworkManager { socket, config }
    }

    pub async fn send_message(
        &self,
        message: &Message,
        dst: SocketAddr,
    ) -> Result<(), KademliaError> {
        let serialized = bincode::serialize(message)?;
        self.socket.send_to(&serialized, dst)?;
        Ok(())
    }

    pub async fn receive_message(&self) -> Result<(Message, SocketAddr), KademliaError> {
        let mut buf = vec![0u8; 1024];
        let (size, src) = self.socket.recv_from(&mut buf)?;
        let message: Message = bincode::deserialize(&buf[..size])?;
        Ok((message, src))
    }
}

// Storage Manager
pub struct StorageManager {
    storage: HashMap<Vec<u8>, Vec<u8>>,
    cache: cache_impl::Cache<Vec<u8>, Vec<u8>>,
    config: Arc<Config>,
}

impl StorageManager {
    pub fn new(config: Arc<Config>) -> Self {
        StorageManager {
            storage: HashMap::new(),
            cache: cache_impl::Cache::with_policy(policy::EvictionPolicy::LRU, config.cache_size),
            config,
        }
    }

    pub async fn store(&mut self, key: &[u8], value: &[u8]) -> Result<(), KademliaError> {
        let hash = Self::hash_key(key);
        self.storage.insert(hash.clone(), value.to_vec());
        self.cache
            .put(hash, value.to_vec(), self.config.cache_ttl)
            .await
            .map_err(|_| KademliaError::InvalidData("Failed to update cache"))?;
        Ok(())
    }

    pub async fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let hash = Self::hash_key(key);
        if let Ok(value) = self.cache.get(&hash).await {
            Some(value)
        } else {
            self.storage.get(&hash).cloned()
        }
    }

    fn hash_key(key: &[u8]) -> Vec<u8> {
        Sha256::digest(key).to_vec()
    }
}

// Kademlia Node
pub struct KademliaNode {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub routing_table: RoutingTable,
    network_manager: NetworkManager,
    storage_manager: StorageManager,
    pub shutdown: mpsc::Receiver<()>,
    config: Arc<Config>,
    time_provider: Arc<dyn TimeProvider>,
    delay_provider: Arc<dyn Delay>,
}

impl fmt::Debug for KademliaNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KademliaNode")
            .field("id", &format!("{:x?}", self.id.0))
            .field("addr", &self.addr)
            .field("routing_table", &self.routing_table)
            .finish()
    }
}

impl KademliaNode {
    pub async fn new(
        addr: SocketAddr,
        config: Option<Config>,
        socket: Arc<dyn NetworkInterface>,
        time_provider: Arc<dyn TimeProvider>,
        delay_provider: Arc<dyn Delay>,
    ) -> Result<(Self, mpsc::Sender<()>), KademliaError> {
        let id = NodeId::new();
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        let config = Arc::new(config.unwrap_or_default());

        Ok((
            KademliaNode {
                id: id.clone(),
                addr,
                routing_table: RoutingTable::new(id),
                network_manager: NetworkManager::new(socket, config.clone()),
                storage_manager: StorageManager::new(config.clone()),
                shutdown: shutdown_receiver,
                config,
                time_provider,
                delay_provider,
            },
            shutdown_sender,
        ))
    }

    pub async fn bootstrap(&mut self) -> Result<(), KademliaError> {
        for &bootstrap_addr in BOOTSTRAP_NODES.iter() {
            if let Ok(addr) = bootstrap_addr.parse::<SocketAddr>() {
                if let Err(e) = self.ping(addr).await {
                    warn!("Failed to ping bootstrap node {}: {:?}", bootstrap_addr, e);
                }
            } else {
                warn!("Invalid bootstrap address: {}", bootstrap_addr);
            }
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), KademliaError> {
        let mut refresh_interval = interval(self.config.maintenance_interval);

        loop {
            tokio::select! {
                result = self.network_manager.receive_message() => {
                    match result {
                        Ok((message, src)) => {
                            if let Err(e) = self.handle_message(message, src).await {
                                error!("Failed to handle message from {}: {:?}", src, e);
                            }
                        }
                        Err(e) => error!("Failed to receive message: {:?}", e),
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

    async fn handle_message(
        &mut self,
        message: Message,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        match message {
            Message::Ping { sender } => self.handle_ping(sender, src).await,
            Message::Store {
                key,
                value,
                sender,
                timestamp,
            } => self.handle_store(key, value, sender, timestamp, src).await,
            Message::FindNode { target } => self.handle_find_node(target, src).await,
            Message::FindValue { key } => self.handle_find_value(key, src).await,
            _ => {
                warn!("Received unknown message type from {}", src);
                Ok(())
            }
        }
    }

    async fn handle_ping(&mut self, sender: NodeId, src: SocketAddr) -> Result<(), KademliaError> {
        self.routing_table.update(sender, src);
        self.network_manager
            .send_message(&Message::Pong { sender: self.id }, src)
            .await
    }

    async fn handle_store(
        &mut self,
        key: Vec<u8>,
        value: Vec<u8>,
        sender: NodeId,
        timestamp: SystemTime,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        self.validate_key_value(&key, &value)?;
        self.storage_manager.store(&key, &value).await?;
        self.routing_table.update(sender, src);
        self.network_manager
            .send_message(
                &Message::StoreResponse {
                    success: true,
                    error_message: None,
                },
                src,
            )
            .await
    }

    async fn handle_find_node(&self, target: NodeId, src: SocketAddr) -> Result<(), KademliaError> {
        let closest_nodes = self.routing_table.find_closest(&target, self.config.k);
        let response = Message::NodesFound(closest_nodes);
        self.network_manager.send_message(&response, src).await
    }

    async fn handle_find_value(&self, key: Vec<u8>, src: SocketAddr) -> Result<(), KademliaError> {
        match self.find_value(&key).await {
            FindValueResult::Value(value) => {
                self.network_manager
                    .send_message(&Message::ValueFound(value), src)
                    .await
            }
            FindValueResult::Nodes(nodes) => {
                self.network_manager
                    .send_message(&Message::NodesFound(nodes), src)
                    .await
            }
        }
    }

    fn validate_key_value(&self, key: &[u8], value: &[u8]) -> Result<(), KademliaError> {
        const MAX_KEY_SIZE: usize = 32;
        const MAX_VALUE_SIZE: usize = 1024;

        if key.len() > MAX_KEY_SIZE {
            Err(KademliaError::InvalidData(
                "Key size exceeds maximum allowed",
            ))
        } else if value.len() > MAX_VALUE_SIZE {
            Err(KademliaError::InvalidData(
                "Value size exceeds maximum allowed",
            ))
        } else {
            Ok(())
        }
    }

    async fn find_value(&self, key: &[u8]) -> FindValueResult {
        if let Some(value) = self.storage_manager.get(key).await {
            FindValueResult::Value(value)
        } else {
            match self
                .find_node(NodeId::from_slice(
                    &StorageManager::hash_key(key)[..32].try_into().unwrap(),
                ))
                .await
            {
                Ok(nodes) => FindValueResult::Nodes(nodes),
                Err(e) => {
                    warn!("Failed to find nodes: {:?}", e);
                    FindValueResult::Nodes(vec![])
                }
            }
        }
    }

    async fn ping(&self, addr: SocketAddr) -> Result<(), KademliaError> {
        self.network_manager
            .send_message(&Message::Ping { sender: self.id }, addr)
            .await
    }

    pub async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), KademliaError> {
        self.validate_key_value(key, value)?;
        self.storage_manager.store(key, value).await?;

        let hash = StorageManager::hash_key(key);
        let target = NodeId::from_slice(&hash[..32].try_into().unwrap());
        let closest_nodes = self.find_node(target).await?;

        let mut success_count = 0;
        for (node_id, addr) in closest_nodes.iter().take(self.config.alpha) {
            if *node_id != self.id {
                match self.send_store_message(key, value, *addr).await {
                    Ok(true) => success_count += 1,
                    Ok(false) => warn!("Store operation failed on node {:#?}", node_id),
                    Err(e) => warn!("Error sending STORE to node {:#?}: {:#?}", node_id, e),
                }
            }
        }

        if success_count == 0 {
            Err(KademliaError::InvalidData("Failed to store on any node"))
        } else {
            Ok(())
        }
    }

    pub async fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, KademliaError> {
        if let Some(value) = self.storage_manager.get(key).await {
            return Ok(Some(value));
        }

        let hash = StorageManager::hash_key(key);
        let target = NodeId::from_slice(&hash[..32].try_into().unwrap());
        let nodes = self.find_node(target).await?;

        for (_, addr) in nodes.iter().take(self.config.alpha) {
            self.network_manager
                .send_message(&Message::FindValue { key: key.to_vec() }, *addr)
                .await?;

            match tokio::time::timeout(
                self.config.request_timeout,
                self.network_manager.receive_message(),
            )
            .await
            {
                Ok(Ok((Message::ValueFound(value), _))) => {
                    self.storage_manager.store(key, &value).await?;
                    return Ok(Some(value));
                }
                Ok(Ok((Message::NodesFound(_), _))) => continue,
                Ok(Err(e)) => warn!("Error receiving message: {:?}", e),
                Err(_) => warn!("Timeout waiting for response from {}", addr),
                _ => warn!("Unexpected message received from {}", addr),
            }
        }

        Ok(None)
    }

    async fn refresh_buckets(&mut self) -> Result<(), KademliaError> {
        for bucket in &self.routing_table.buckets {
            if let Some(node) = bucket.entries.first() {
                if let Err(e) = self.ping(node.addr).await {
                    warn!("Failed to ping node at {}: {:?}", node.addr, e);
                }
            }
        }

        info!("Buckets refreshed");
        Ok(())
    }

    pub async fn find_node(
        &self,
        target: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, KademliaError> {
        let mut closest_nodes = self.routing_table.find_closest(&target, self.config.k);
        let mut queried_nodes = HashSet::new();
        let mut pending_queries = HashSet::new();

        while !closest_nodes.is_empty() {
            let mut new_queries = Vec::new();

            for (node_id, addr) in closest_nodes.iter() {
                if queried_nodes.contains(node_id) || pending_queries.contains(node_id) {
                    continue;
                }

                new_queries.push((*node_id, *addr));
                pending_queries.insert(*node_id);

                if new_queries.len() >= self.config.alpha {
                    break;
                }
            }

            if new_queries.is_empty() {
                break;
            }

            let query_results =
                futures::future::join_all(new_queries.into_iter().map(|(node_id, addr)| {
                    let target_clone = target.clone();
                    async move {
                        match self.find_node_rpc(node_id, addr, target_clone).await {
                            Ok(nodes) => Some((node_id, nodes)),
                            Err(_) => None,
                        }
                    }
                }))
                .await;

            for result in query_results {
                if let Some((queried_node, nodes)) = result {
                    queried_nodes.insert(queried_node);
                    pending_queries.remove(&queried_node);

                    for (node_id, addr) in nodes {
                        if !queried_nodes.contains(&node_id) && !pending_queries.contains(&node_id)
                        {
                            closest_nodes.push((node_id, addr));
                        }
                    }
                }
            }

            closest_nodes.sort_by(|a, b| target.distance(&a.0).cmp(&target.distance(&b.0)));
            closest_nodes.truncate(self.config.k);

            if queried_nodes.len() >= self.config.k {
                break;
            }
        }

        Ok(closest_nodes)
    }

    async fn find_node_rpc(
        &self,
        node_id: NodeId,
        addr: SocketAddr,
        target: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, KademliaError> {
        self.network_manager
            .send_message(&Message::FindNode { target }, addr)
            .await?;

        match tokio::time::timeout(
            self.config.request_timeout,
            self.network_manager.receive_message(),
        )
        .await
        {
            Ok(Ok((Message::NodesFound(nodes), _))) => Ok(nodes),
            Ok(Ok(_)) => Err(KademliaError::UnexpectedResponse),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(KademliaError::Timeout),
        }
    }

    async fn send_store_message(
        &self,
        key: &[u8],
        value: &[u8],
        addr: SocketAddr,
    ) -> Result<bool, KademliaError> {
        let message = Message::Store {
            key: key.to_vec(),
            value: value.to_vec(),
            sender: self.id,
            timestamp: self.time_provider.now(),
        };
        self.network_manager.send_message(&message, addr).await?;

        match tokio::time::timeout(
            self.config.request_timeout,
            self.network_manager.receive_message(),
        )
        .await
        {
            Ok(Ok((
                Message::StoreResponse {
                    success,
                    error_message,
                },
                _,
            ))) => {
                if !success {
                    warn!("Store failed on node {}: {:?}", addr, error_message);
                }
                Ok(success)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(KademliaError::Timeout),
            _ => Err(KademliaError::UnexpectedResponse),
        }
    }

    // Methods for state inspection
    pub fn get_routing_table_size(&self) -> usize {
        self.routing_table
            .buckets
            .iter()
            .map(|bucket| bucket.entries.len())
            .sum()
    }

    pub async fn get_storage_size(&self) -> usize {
        self.storage_manager.storage.len()
    }

    pub async fn get_cached_entries_count(&self) -> usize {
        self.storage_manager.cache.cache_store.read().await.len()
    }
}

// Implement additional utility functions if needed

#[cfg(test)]
mod tests {
    use super::*;
    // Implement unit tests for KademliaNode and its components
}
