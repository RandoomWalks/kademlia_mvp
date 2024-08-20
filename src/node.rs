use crate::message::{FindValueResult, Message};
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;
use crate::cache::cache_impl;
use crate::cache::entry;
use crate::cache::policy;

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::collections::{BinaryHeap, HashSet};
use tokio::sync::Mutex;
use std::time::SystemTime;
use std::future::Future;
use std::pin::Pin;
use std::fmt;

use crate::utils::{ALPHA, BOOTSTRAP_NODES, K};
use bincode::{deserialize, serialize};

// New traits for dependency injection
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

// New struct for network management
pub struct NetworkManager {
    socket: Arc<dyn NetworkInterface>,
}

impl NetworkManager {
    pub fn new(socket: Arc<dyn NetworkInterface>) -> Self {
        NetworkManager { socket }
    }

    pub async fn send_message(&self, message: &Message, dst: SocketAddr) -> std::io::Result<()> {
        let serialized = bincode::serialize(message)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.socket.send_to(&serialized, dst)?;
        Ok(())
    }

    pub async fn receive_message(&self) -> std::io::Result<(Message, SocketAddr)> {
        let mut buf = vec![0u8; 1024];
        let (size, src) = self.socket.recv_from(&mut buf)?;
        let message: Message = bincode::deserialize(&buf[..size])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok((message, src))
    }
}

// New struct for configurable constants
pub struct KademliaConfig {
    pub k: usize,
    pub alpha: usize,
    pub request_timeout: Duration,
}

impl Default for KademliaConfig {
    fn default() -> Self {
        KademliaConfig {
            k: 20,
            alpha: 3,
            request_timeout: Duration::from_secs(5),
        }
    }
}

pub struct KademliaNode {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub routing_table: RoutingTable,
    pub storage: HashMap<Vec<u8>, Vec<u8>>,
    pub network_manager: NetworkManager,
    pub shutdown: mpsc::Receiver<()>,
    pub cache: cache_impl::Cache<Vec<u8>, Vec<u8>>,
    pub cache_config: policy::CacheConfig,
    pub config: KademliaConfig,
    time_provider: Arc<dyn TimeProvider>,
    delay_provider: Arc<dyn Delay>,
}

impl fmt::Debug for KademliaNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KademliaNode")
            .field("id", &format!("{:x?}", self.id.0))
            .field("addr", &self.addr)
            .field("routing_table", &self.routing_table)
            .field("storage_size", &self.storage.len())
            // .field("cache_size", &self.cache.cache_store.read().await.len())
            .finish()
    }
}

impl KademliaNode {
    pub async fn new(
        addr: SocketAddr,
        cache_config: Option<policy::CacheConfig>,
        kademlia_config: Option<KademliaConfig>,
        socket: Arc<dyn NetworkInterface>,
        time_provider: Arc<dyn TimeProvider>,
        delay_provider: Arc<dyn Delay>,
    ) -> std::io::Result<(Self, mpsc::Sender<()>)> {
        let id = NodeId::new();
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        let config = cache_config.unwrap_or_default();
        let kademlia_config = kademlia_config.unwrap_or_default();
        let cache: cache_impl::Cache<Vec<u8>, Vec<u8>> = cache_impl::Cache::with_config(&config);

        Ok((
            KademliaNode {
                id: id.clone(),
                addr,
                routing_table: RoutingTable::new(id),
                storage: HashMap::new(),
                network_manager: NetworkManager::new(socket),
                shutdown: shutdown_receiver,
                cache: cache_impl::Cache::with_policy(policy::EvictionPolicy::LRU, 10),
                cache_config: config,
                config: kademlia_config,
                time_provider,
                delay_provider,
            },
            shutdown_sender,
        ))
    }

    async fn start_cache_maintenance(&self) {
        let mut interval = tokio::time::interval(self.cache_config.maintenance_interval);
        loop {
            interval.tick().await;
            debug!("Performing cache maintenance");
            let evicted_count = self.cache.evict().await;
            info!("Evicted {:#?} items from cache", evicted_count);
        }
    }

    pub async fn cache_hit_count(&self) -> usize {
        self.cache.metrics.read().await.hits
    }

    pub async fn cache_size(&self) -> usize {
        self.cache.cache_store.read().await.len()
    }

    pub async fn bootstrap(&mut self) -> std::io::Result<()> {
        for &bootstrap_addr in BOOTSTRAP_NODES.iter() {
            match bootstrap_addr.parse::<SocketAddr>() {
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
        let mut refresh_interval = interval(Duration::from_secs(3600));

        loop {
            tokio::select! {
                Ok((message, src)) = self.network_manager.receive_message() => {
                    if let Err(e) = self.handle_message(message, src).await {
                        error!("Failed to handle message from {:#?}: {:?}", src, e);
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

    fn validate_key_value(&self, key: &[u8], value: &[u8]) -> Result<(), &'static str> {
        const MAX_KEY_SIZE: usize = 32;
        const MAX_VALUE_SIZE: usize = 1024;

        if key.len() > MAX_KEY_SIZE {
            return Err("Key size exceeds maximum allowed");
        }
        if value.len() > MAX_VALUE_SIZE {
            return Err("Value size exceeds maximum allowed");
        }
        Ok(())
    }

    async fn handle_ping(&mut self, sender: NodeId, src: SocketAddr) -> std::io::Result<()> {
        self.routing_table.update(sender, src);
        self.network_manager.send_message(&Message::Pong { sender: self.id }, src).await
    }

    async fn handle_store(&mut self, key: Vec<u8>, value: Vec<u8>, sender: NodeId, timestamp: SystemTime, src: SocketAddr) -> std::io::Result<()> {
        match self.validate_key_value(&key, &value) {
            Ok(()) => {
                self.store(&key, &value).await;
                if let Err(e) = self.cache.put(key.clone(), value.clone(), self.cache_config.ttl).await {
                    warn!("Failed to update cache: {:?}", e);
                }
                self.send_store_response(true, None, src).await?;
            },
            Err(e) => {
                warn!("Invalid STORE request from {:?}: {}", src, e);
                self.send_store_response(false, Some(e), src).await?;
            }
        }
        self.routing_table.update(sender, src);
        Ok(())
    }

    async fn handle_find_node(&self, target: NodeId, src: SocketAddr) -> std::io::Result<()> {
        let closest_nodes = self.routing_table.find_closest(&target, self.config.k);
        let response = Message::NodesFound(closest_nodes.clone());
        self.network_manager.send_message(&response, src).await?;
        Ok(())
    }

    async fn handle_find_value(&self, key: Vec<u8>, src: SocketAddr) -> std::io::Result<()> {
        match self.find_value(&key).await {
            FindValueResult::Value(value) => {
                self.network_manager.send_message(&Message::ValueFound(value), src).await?;
            }
            FindValueResult::Nodes(nodes) => {
                self.network_manager.send_message(&Message::NodesFound(nodes), src).await?;
            }
        }
        Ok(())
    }

    pub async fn handle_message(&mut self, message: Message, src: SocketAddr) -> std::io::Result<()> {
        match message {
            Message::Ping { sender } => self.handle_ping(sender, src).await,
            Message::Store { key, value, sender, timestamp } => self.handle_store(key, value, sender, timestamp, src).await,
            Message::FindNode { target } => self.handle_find_node(target, src).await,
            Message::FindValue { key } => self.handle_find_value(key, src).await,
            _ => {
                warn!("Received unknown message type from {:#?}", src);
                Ok(())
            }
        }
    }
    
    pub async fn store(&mut self, key: &[u8], value: &[u8]) {
        let hash = Self::hash_key(key);
        match self.storage.entry(hash.clone()) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if entry.get() != value {
                    warn!("Key collision detected. Overwriting existing value.");
                    entry.insert(value.to_vec());
                }
            },
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(value.to_vec());
            },
        }

        info!("Storing value for key: {:?}", hash);
        if let Err(e) = self.cache.put(hash, value.to_vec(), self.cache_config.ttl).await {
            warn!("Failed to update cache: {:?}", e);
        }
    }

    async fn send_store_message(&self, key: &[u8], value: &[u8], addr: SocketAddr) -> Result<bool, std::io::Error> {
        let message = Message::Store {
            key: key.to_vec(),
            value: value.to_vec(),
            sender: self.id,
            timestamp: self.time_provider.now(),
        };
        self.network_manager.send_message(&message, addr).await?;

        match tokio::time::timeout(self.config.request_timeout, self.network_manager.receive_message()).await {
            Ok(Ok((Message::StoreResponse { success, error_message }, _))) => {
                if !success {
                    warn!("Store failed on node {:?}: {:?}", addr, error_message);
                }
                Ok(success)
            },
            Ok(Err(e)) => Err(e),
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Store response timed out")),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Other, "Unexpected response")),
        }
    }
    
    async fn send_store_response(&self, success: bool, error_message: Option<&str>, dst: SocketAddr) -> std::io::Result<()> {
        let response = Message::StoreResponse {
            success,
            error_message: error_message.map(String::from),
        };
        self.network_manager.send_message(&response, dst).await
    }

    pub fn hash_key(key: &[u8]) -> Vec<u8> {
        Sha256::digest(key).to_vec()
    }

    pub async fn find_value(&self, key: &[u8]) -> FindValueResult {
        let hash = Self::hash_key(key);

        if let Ok(value) = self.cache.get(&hash).await {
            return FindValueResult::Value(value);
        }

        if let Some(value) = self.storage.get(&hash) {
            if let Err(e) = self
                .cache
                .put(hash.clone(), value.clone(), Duration::from_secs(3600))
                .await
            {
                warn!("Failed to store in cache: {:?}", e);
            }
            return FindValueResult::Value(value.clone());
        }

        match self
            .find_node(NodeId::from_slice(
                hash[..].try_into().expect("Hash length is not 32 bytes"),
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

    pub async fn ping(&self, addr: SocketAddr) -> std::io::Result<()> {
        self.network_manager.send_message(&Message::Ping { sender: self.id }, addr).await
    }

    pub async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), std::io::Error> {
        let hash = Self::hash_key(key);
        let target = NodeId::from_slice(&hash[..].try_into().expect("Hash is not 32 bytes long"));

        self.store(key, value).await;

        let closest_nodes = self.find_node(target).await?;

        let mut success_count = 0;
        for (node_id, addr) in closest_nodes.iter().take(self.config.alpha) {
            if *node_id != self.id {
                match self.send_store_message(key, value, *addr).await {
                    Ok(true) => success_count += 1,
                    Ok(false) => warn!("Store operation failed on node {:?}", node_id),
                    Err(e) => warn!("Error sending STORE to node {:?}: {:?}", node_id, e),
                }
            }
        }

        if success_count == 0 {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to store on any node"))
        } else {
            Ok(())
        }
    }

    pub async fn get(&mut self, key: &[u8]) -> std::io::Result<Option<Vec<u8>>> {
        let hash = Self::hash_key(key);

        if let Ok(value) = self.cache.get(&hash).await {
            debug!("Cache hit for key: {:?}", hash);
            return Ok(Some(value));
        }

        if let Some(value) = self.storage.get(&hash) {
            debug!("Storage hit for key: {:?}", hash);
            if let Err(e) = self
                .cache
                .put(hash.clone(), value.clone(), Duration::from_secs(3600))
                .await
            {
                warn!("Failed to store in cache: {:?}", e);
            }
            return Ok(Some(value.clone()));
        }

        debug!("Key not found locally, performing network lookup: {:?}", hash);
        
        let res: &[u8; 32] = &hash[..32].try_into().expect("Slice with incorrect length");
        let target = NodeId::from_slice(res);

        let nodes = self.find_node(target).await?;

        for (_, addr) in nodes.iter().take(self.config.alpha) {
            if let Err(e) = self
                .network_manager
                .send_message(&Message::FindValue { key: key.to_vec() }, *addr)
                .await
            {
                warn!("Failed to send FindValue message to {:#?}: {:?}", addr, e);
                continue;
            }

            match tokio::time::timeout(
                self.config.request_timeout,
                self.network_manager.receive_message(),
            )
            .await
            {
                Ok(Ok((Message::ValueFound(value), _))) => {
                    // Store the found value in local storage and cache
                    self.store(key, &value).await;
                    return Ok(Some(value));
                }
                Ok(Ok((Message::NodesFound(_), _))) => {
                    // Continue to the next node
                }
                Ok(Err(e)) => warn!("Error receiving message: {:?}", e),
                Err(_) => warn!("Timeout waiting for response from {:?}", addr),
                _ => warn!("Unexpected message received from {:?}", addr),
            }
        }

        Ok(None)
    }

    pub async fn refresh_buckets(&mut self) -> std::io::Result<()> {
        for bucket in &self.routing_table.buckets {
            if let Some(node) = bucket.entries.first() {
                if let Err(e) = self.ping(node.addr).await {
                    warn!("Failed to ping node at {:#?}: {:?}", node.addr, e);
                }
            }
        }

        info!("Buckets refreshed");
        Ok(())
    }

    pub async fn find_node(&self, target: NodeId) -> Result<Vec<(NodeId, SocketAddr)>, std::io::Error> {
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

            let query_results = futures::future::join_all(new_queries.into_iter().map(|(node_id, addr)| {
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
                        if !queried_nodes.contains(&node_id) && !pending_queries.contains(&node_id) {
                            closest_nodes.push((node_id, addr));
                        }
                    }
                }
            }

            closest_nodes.sort_by(|a, b| {
                target.distance(&a.0).cmp(&target.distance(&b.0))
            });

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
    ) -> Result<Vec<(NodeId, SocketAddr)>, std::io::Error> {
        self.network_manager.send_message(&Message::FindNode { target }, addr).await?;

        match tokio::time::timeout(
            self.config.request_timeout,
            self.network_manager.receive_message(),
        )
        .await
        {
            Ok(Ok((Message::NodesFound(nodes), _))) => Ok(nodes),
            Ok(Ok(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unexpected response",
            )),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "RPC timed out",
            )),
        }
    }

    // New methods for state inspection
    pub fn get_routing_table_size(&self) -> usize {
        self.routing_table.buckets.iter().map(|bucket| bucket.entries.len()).sum()
    }

    pub fn get_storage_size(&self) -> usize {
        self.storage.len()
    }

    pub async fn get_cached_entries_count(&self) -> usize {
        self.cache.cache_store.read().await.len()
    }
}