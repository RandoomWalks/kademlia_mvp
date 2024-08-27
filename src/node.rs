use crate::cache::cache_impl;
use crate::cache::entry;
use crate::cache::policy;
use crate::message::{FindValueResult, Message};
use crate::routing_table::RoutingTable;
use crate::utils::{KademliaError, NodeId};

use crate::utils::{Config, ALPHA, BOOTSTRAP_NODES, K};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::time::{interval, Duration};

use async_trait::async_trait;
use bincode::{deserialize, serialize};

// Network Manager
pub struct NetworkManager {
    socket: Arc<UdpSocket>,
    config: Arc<Config>,
}

impl NetworkManager {
    pub fn new(socket: Arc<UdpSocket>, config: Arc<Config>) -> Self {
        NetworkManager { socket, config }
    }

    pub async fn send_message(
        &self,
        message: &Message,
        dst: SocketAddr,
    ) -> Result<(), KademliaError> {
        debug!("Sending message to {}: {:?}", dst, message);
        let serialized = bincode::serialize(message)?;
        self.socket.send_to(&serialized, dst).await?;
        Ok(())
    }

    pub async fn receive_message(&self) -> Result<(Message, SocketAddr), KademliaError> {
        let mut buf = vec![0u8; 1024];
        let (size, src) = self.socket.recv_from(&mut buf).await?;
        let message: Message = bincode::deserialize(&buf[..size])?;
        debug!("Received message from {}: {:?}", src, message);
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
        debug!("StorageManager: Attempting to store key: {:?}", key);
        let hash = Self::hash_key(key);
        self.storage.insert(hash.clone(), value.to_vec());
        self.cache
            .put(hash.clone(), value.to_vec(), self.config.cache_ttl)
            .await
            .map_err(|_| KademliaError::InvalidData("Failed to update cache"))?;
        debug!("StorageManager: Successfully stored key: {:?}", key);
        Ok(())
    }

    pub async fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        debug!("StorageManager: Attempting to retrieve key: {:?}", key);
        let hash = Self::hash_key(key);
        let result = if let Ok(value) = self.cache.get(&hash).await {
            debug!(
                "StorageManager: Retrieved value from cache for key: {:?}",
                key
            );
            Some(value)
        } else {
            let value = self.storage.get(&hash).cloned();
            if value.is_some() {
                debug!(
                    "StorageManager: Retrieved value from storage for key: {:?}",
                    key
                );
            } else {
                debug!("StorageManager: No value found for key: {:?}", key);
            }
            value
        };
        result
    }

    pub fn hash_key(key: &[u8]) -> Vec<u8> {
        Sha256::digest(key).to_vec()
    }
}

// Kademlia Node
pub struct KademliaNode {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub routing_table: RoutingTable,
    pub network_manager: NetworkManager,
    pub storage_manager: StorageManager,
    pub shutdown: mpsc::Receiver<()>,
    pub config: Arc<Config>,
    pub bootstrap_nodes: Vec<SocketAddr>,
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
        config: Option<Config>,
        bootstrap_nodes: Vec<SocketAddr>,
    ) -> Result<(Self, mpsc::Sender<()>), KademliaError> {
        let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
        let addr = socket.local_addr()?;
        let id = NodeId::new();
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        let config = Arc::new(config.unwrap_or_default());

        info!("KademliaNode bound to address: {}", addr);

        Ok((
            KademliaNode {
                id: id.clone(),
                addr,
                routing_table: RoutingTable::new(id),
                network_manager: NetworkManager::new(socket, config.clone()),
                storage_manager: StorageManager::new(config.clone()),
                shutdown: shutdown_receiver,
                config,
                bootstrap_nodes,
            },
            shutdown_sender,
        ))
    }

    fn is_same_node(&self, addr1: SocketAddr, addr2: SocketAddr) -> bool {
        if addr1 == addr2 {
            return true;
        }
        match (addr1.ip(), addr2.ip()) {
            (IpAddr::V4(ip1), IpAddr::V4(ip2)) => {
                (ip1.is_unspecified() || ip2.is_unspecified() || ip1 == ip2)
                    && addr1.port() == addr2.port()
            }
            _ => false,
        }
    }

    // pub async fn bootstrap(&mut self) -> Result<(), KademliaError> {
    //     info!("Node {:?} starting bootstrap process", self.id);

    //     let mut successful_contacts = Vec::new();

    //     for &bootstrap_addr in &self.bootstrap_nodes {
    //         for attempt in 1..=3 {
    //             match self.ping(bootstrap_addr).await {
    //                 Ok(_) => {
    //                     info!("Successfully pinged bootstrap node at {}", bootstrap_addr);
    //                     successful_contacts.push(bootstrap_addr);
    //                     break; // Break out of the retry loop on success
    //                 }
    //                 Err(e) => {
    //                     warn!(
    //                         "Failed to ping bootstrap node at {}: {:?}. Attempt {}/3",
    //                         bootstrap_addr, e, attempt
    //                     );
    //                     if attempt < 3 {
    //                         sleep(Duration::from_secs(1)).await;
    //                     }
    //                 }
    //             }
    //         }
    //     }

    //     if successful_contacts.is_empty() {
    //         warn!("No bootstrap nodes responded. Marking this node as the first in the network.");
    //         self.routing_table.update(self.id, self.addr); // Manually add itself to the routing table
    //         debug!(
    //             "Updated routing table with node {:?} at address {}",
    //             self.id, self.addr
    //         );
    //         return Ok(()); // Exit the bootstrap process as this node is now the first node
    //     }

    //     for &contact in &successful_contacts {
    //         match self.find_node(self.id).await {
    //             Ok(nodes) => {
    //                 info!(
    //                     "Found {} nodes from bootstrap node {}",
    //                     nodes.len(),
    //                     contact
    //                 );
    //                 for (node_id, addr) in nodes {
    //                     self.routing_table.update(node_id, addr);
    //                     debug!(
    //                         "Updated routing table with node {:?} at address {}",
    //                         node_id, addr
    //                     );

    //                     if self.ping(addr).await.is_ok() {
    //                         info!(
    //                             "Successfully pinged and added node {:?} at {} to routing table",
    //                             node_id, addr
    //                         );
    //                     }
    //                 }
    //             }
    //             Err(e) => {
    //                 warn!(
    //                     "Failed to find nodes from bootstrap node {}: {:?}",
    //                     contact, e
    //                 );
    //             }
    //         }
    //     }

    //     let table_size = self.get_routing_table_size();
    //     info!(
    //         "Bootstrap process completed for node {:?}. Routing table size: {}",
    //         self.id, table_size
    //     );

    //     if table_size == 0 {
    //         warn!("Routing table is empty after bootstrap. This might be the first node in the network.");
    //     }

    //     Ok(())
    // }
    pub async fn bootstrap(&mut self) -> Result<(), KademliaError> {
        info!("Node {:?} starting bootstrap process", self.id);

        let mut successful_contacts = Vec::new();

        for &bootstrap_addr in &self.bootstrap_nodes {
            for attempt in 1..=3 {
                match self.ping(bootstrap_addr).await {
                    Ok(_) => {
                        info!("Successfully pinged bootstrap node at {}", bootstrap_addr);
                        successful_contacts.push(bootstrap_addr);
                        break; // Break out of the retry loop on success
                    }
                    Err(e) => {
                        warn!(
                            "Failed to ping bootstrap node at {}: {:?}. Attempt {}/3",
                            bootstrap_addr, e, attempt
                        );
                        if attempt < 3 {
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }

        if successful_contacts.is_empty() {
            warn!("No bootstrap nodes responded. Marking this node as the first in the network.");
            self.routing_table.update(self.id, self.addr); // Manually add itself to the routing table
            debug!(
                "Updated routing table with node {:?} at address {}",
                self.id, self.addr
            );

            return Ok(()); // Exit the bootstrap process as this node is now the first node
        }

        for &contact in &successful_contacts {
            match self.find_node(self.id).await {
                Ok(nodes) => {
                    info!(
                        "Found {} nodes from bootstrap node {}",
                        nodes.len(),
                        contact
                    );
                    for (node_id, addr) in nodes {
                        self.routing_table.update(node_id, addr);
                        debug!(
                            "Updated routing table with node {:?} at address {}",
                            node_id, addr
                        );

                        if self.ping(addr).await.is_ok() {
                            info!(
                                "Successfully pinged and added node {:?} at {} to routing table",
                                node_id, addr
                            );
                        }
                    }

                    // Ensure that the contact node is added to the routing table if not already present
                    if !self
                        .routing_table
                        .find_closest(&self.id, 1)
                        .iter()
                        .any(|(_, addr)| *addr == contact)
                    {
                        self.routing_table.update(self.id, contact);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to find nodes from bootstrap node {}: {:?}",
                        contact, e
                    );
                }
            }
        }

        let table_size = self.get_routing_table_size();
        info!(
            "Bootstrap process completed for node {:?}. Routing table size: {}",
            self.id, table_size
        );

        if table_size == 0 {
            warn!("Routing table is empty after bootstrap. This might be the first node in the network.");
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
                            debug!("Received message from {}: {:?}", src, message);
                            match message {
                                Message::ClientStore { .. } | Message::ClientGet { .. } => {
                                    self.handle_client_message(message, src).await?;
                                },
                                _ => {
                                    self.handle_message(message, src).await?;
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                }
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

    pub async fn handle_message(
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

    pub async fn handle_ping(
        &mut self,
        sender: NodeId,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        self.routing_table.update(sender, src);
        debug!(
            "handle_ping:: Updated routing table with node {:?} at address {}",
            sender, src
        );

        self.network_manager
            .send_message(&Message::Pong { sender: self.id }, src)
            .await
    }

    pub async fn handle_store(
        &mut self,
        key: Vec<u8>,
        value: Vec<u8>,
        sender: NodeId,
        timestamp: SystemTime,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        debug!("Handling P2P STORE request for key: {:?}", key);
        self.validate_key_value(&key, &value)?;
        debug!("Storing data in P2P store");
        self.storage_manager.store(&key, &value).await?;

        self.routing_table.update(sender, src);
        debug!(
            "handle_store:: Updated routing table with node {:?} at address {}",
            sender, src
        );

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

    pub async fn handle_find_node(
        &self,
        target: NodeId,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        let closest_nodes = self.routing_table.find_closest(&target, self.config.k);
        debug!(
            "handle_find_node:: Found closest nodes for target {:?}: {:?}",
            target, closest_nodes
        );

        let response = Message::NodesFound(closest_nodes);
        self.network_manager.send_message(&response, src).await
    }

    pub async fn handle_find_value(
        &self,
        key: Vec<u8>,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        debug!("Handling FIND_VALUE for key: {:?}", key);
        match self.find_value(&key).await {
            FindValueResult::Value(value) => {
                debug!("Value found locally for key: {:?}", key);
                self.network_manager
                    .send_message(&Message::ValueFound(value), src)
                    .await
            }
            FindValueResult::Nodes(nodes) => {
                debug!(
                    "Value not found locally. Responding with closest nodes for key: {:?}",
                    key
                );
                self.network_manager
                    .send_message(&Message::NodesFound(nodes), src)
                    .await
            }
        }
    }

    pub async fn handle_client_message(
        &mut self,
        message: Message,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        match message {
            Message::ClientStore { key, value, sender } => {
                self.handle_client_store(key, value, sender, src).await
            }
            Message::ClientGet { key, sender } => self.handle_client_get(key, sender, src).await,
            _ => Err(KademliaError::InvalidMessage),
        }
    }

    pub async fn handle_client_store(
        &mut self,
        key: Vec<u8>,
        value: Vec<u8>,
        sender: NodeId,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        debug!("Handling client STORE request for key: {:?}", key);
        self.validate_key_value(&key, &value)?;

        debug!("Storing data locally");
        self.storage_manager.store(&key, &value).await?;

        debug!("Replicating store operation");
        self.replicate_store(key.clone(), value.clone()).await?;

        debug!("Sending client store response");
        self.send_client_store_response(true, None, src).await
    }

    pub async fn handle_client_get(
        &self,
        key: Vec<u8>,
        sender: NodeId,
        src: SocketAddr,
    ) -> Result<(), KademliaError> {
        debug!("Handling client GET request for key: {:?}", key);
        match self.storage_manager.get(&key).await {
            Some(value) => self.send_client_get_response(Some(value), None, src).await,
            None => {
                let result = self.find_value(&key).await;
                match result {
                    FindValueResult::Value(value) => {
                        self.send_client_get_response(Some(value), None, src).await
                    }
                    FindValueResult::Nodes(_) => {
                        self.send_client_get_response(
                            None,
                            Some("Value not found".to_string()),
                            src,
                        )
                        .await
                    }
                }
            }
        }
    }

    pub async fn replicate_store(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), KademliaError> {
        let node_id = NodeId::from_slice(key.as_slice().try_into().unwrap());
        debug!(
            "Finding closest nodes for replication, target: {:?}",
            node_id
        );
        let closest_nodes = self.find_node(node_id).await?;

        debug!("Closest nodes for replication: {:?}", closest_nodes);

        for (_, addr) in closest_nodes.iter().take(self.config.alpha) {
            debug!("Sending store message to node at {:?}", addr);
            if let Err(e) = self.send_store_message(&key, &value, *addr).await {
                warn!("Failed to replicate store to {}: {:?}", addr, e);
            }
        }
        Ok(())
    }

    pub async fn send_client_store_response(
        &self,
        success: bool,
        error_message: Option<String>,
        dst: SocketAddr,
    ) -> Result<(), KademliaError> {
        debug!(
            "Sending client store response: success={}, error_message={:?}",
            success, error_message
        );
        let response = Message::ClientStoreResponse {
            success,
            error_message,
        };
        self.network_manager.send_message(&response, dst).await
    }

    pub async fn send_client_get_response(
        &self,
        value: Option<Vec<u8>>,
        error_message: Option<String>,
        dst: SocketAddr,
    ) -> Result<(), KademliaError> {
        debug!(
            "Sending client get response: value={:?}, error_message={:?}",
            value, error_message
        );
        let response = Message::ClientGetResponse {
            value,
            error_message,
        };
        self.network_manager.send_message(&response, dst).await
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

    pub async fn find_value(&self, key: &[u8]) -> FindValueResult {
        debug!("Searching for value associated with key: {:?}", key);

        if let Some(value) = self.storage_manager.get(key).await {
            debug!("Value found locally for key: {:?}", key);
            return FindValueResult::Value(value);
        }

        let target = NodeId::from_slice(&StorageManager::hash_key(key)[..32].try_into().unwrap());
        let mut closest_nodes = self.routing_table.find_closest(&target, self.config.k);
        let mut queried_nodes = HashSet::new();

        while !closest_nodes.is_empty() {
            let mut new_queries = Vec::new();

            for (node_id, addr) in closest_nodes.iter().take(self.config.alpha) {
                if queried_nodes.contains(node_id) {
                    continue;
                }

                debug!(
                    "Querying node {:?} at address {:?} for value",
                    node_id, addr
                );
                match self.send_find_value_message(key, *addr).await {
                    Ok(Message::ValueFound(value)) => {
                        debug!("Value found on node {:?}: {:?}", node_id, value);
                        return FindValueResult::Value(value);
                    }
                    Ok(Message::NodesFound(nodes)) => {
                        debug!("Received closer nodes: {:?}", nodes);
                        new_queries.extend(nodes);
                    }
                    _ => {} // Handle other cases or errors
                }

                queried_nodes.insert(*node_id);
            }

            closest_nodes.extend(new_queries);
            closest_nodes.sort_by(|a, b| target.distance(&a.0).cmp(&target.distance(&b.0)));
            closest_nodes.truncate(self.config.k);

            if queried_nodes.len() >= self.config.k {
                break;
            }
        }

        debug!("Returning closest nodes for key: {:?}", key);
        FindValueResult::Nodes(closest_nodes)
    }

    pub async fn send_find_value_message(
        &self,
        key: &[u8],
        addr: SocketAddr,
    ) -> Result<Message, KademliaError> {
        let message = Message::FindValue { key: key.to_vec() };
        debug!("Sending FIND_VALUE message to {}: {:?}", addr, key);
        self.network_manager.send_message(&message, addr).await?;

        match tokio::time::timeout(
            self.config.request_timeout,
            self.network_manager.receive_message(),
        )
        .await
        {
            Ok(Ok((response, _))) => {
                debug!("Received response from {}: {:?}", addr, response);
                Ok(response)
            }
            Ok(Err(e)) => {
                error!("Error receiving response from {}: {:?}", addr, e);
                Err(e)
            }
            Err(_) => {
                warn!("Timeout waiting for response from {}", addr);
                Err(KademliaError::Timeout)
            }
        }
    }

    pub async fn ping(&self, addr: SocketAddr) -> Result<(), KademliaError> {
        debug!("Pinging node at {} from {}", addr, self.addr);
        self.network_manager
            .send_message(&Message::Ping { sender: self.id }, addr)
            .await?;

        match tokio::time::timeout(
            self.config.request_timeout,
            self.network_manager.receive_message(),
        )
        .await
        {
            Ok(Ok((Message::Pong { sender }, src))) => {
                if self.is_same_node(addr, src) {
                    debug!("Received pong from {:?} at {}", sender, src);
                    Ok(())
                } else {
                    error!("Unexpected response from {}", src);
                    Err(KademliaError::UnexpectedResponse)
                }
            }
            Ok(Ok(_)) => Err(KademliaError::UnexpectedResponse),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                warn!("Timeout waiting for pong from {}", addr);
                Err(KademliaError::Timeout)
            }
        }
    }

    pub async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), KademliaError> {
        info!("Attempting to store key: {:?}", key);
        self.validate_key_value(key, value)?;
        self.storage_manager.store(key, value).await?;

        let hash = StorageManager::hash_key(key);
        let target = NodeId::from_slice(&hash[..32].try_into().unwrap());
        let closest_nodes = self.find_node(target).await?;
        if closest_nodes.is_empty() {
            error!("No closest nodes found for storage");
            return Err(KademliaError::InvalidData("No nodes available for storage"));
        }

        info!(
            "Found {} closest nodes for replication",
            closest_nodes.len()
        );
        let mut success_count = 0;
        for (node_id, addr) in closest_nodes.iter().take(self.config.alpha) {
            if *node_id != self.id {
                match self.send_store_message(key, value, *addr).await {
                    Ok(true) => {
                        success_count += 1;
                        info!("Successfully stored data on node {:?} at {}", node_id, addr);
                    }
                    Ok(false) => warn!("Store operation failed on node {:?} at {}", node_id, addr),
                    Err(e) => warn!(
                        "Error sending STORE to node {:?} at {}: {:?}",
                        node_id, addr, e
                    ),
                }
            }
        }

        if success_count == 0 && !closest_nodes.is_empty() {
            error!("Failed to store on any node");
            Err(KademliaError::InvalidData("Failed to store on any node"))
        } else {
            info!("Successfully stored data on {} nodes", success_count);
            Ok(())
        }
    }

    pub async fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, KademliaError> {
        debug!("Attempting to retrieve key: {:?}", key);
        if let Some(value) = self.storage_manager.get(key).await {
            debug!("Found value locally for key: {:?}", key);
            return Ok(Some(value));
        }

        let hash = StorageManager::hash_key(key);
        let target = NodeId::from_slice(&hash[..32].try_into().unwrap());
        let nodes = self.find_node(target).await?;

        for (_, addr) in nodes.iter().take(self.config.alpha) {
            debug!("Sending FIND_VALUE request to node at {:?}", addr);
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
                    debug!("Value found at node {:?}: {:?}", addr, value);
                    self.storage_manager.store(key, &value).await?;
                    return Ok(Some(value));
                }
                Ok(Ok((Message::NodesFound(_), _))) => {
                    debug!("Received NodesFound response from {:?}", addr);
                    continue;
                }
                Ok(Err(e)) => {
                    warn!("Error receiving message from {}: {:?}", addr, e);
                }
                Err(_) => {
                    warn!("Timeout waiting for response from {}", addr);
                }
                _ => warn!("Unexpected message received from {}", addr),
            }
        }

        warn!("Failed to find value for key: {:?}", key);
        Ok(None)
    }

    pub async fn refresh_buckets(&mut self) -> Result<(), KademliaError> {
        debug!("Refreshing routing table buckets");
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
        debug!("Searching for closest nodes to target: {:?}", target);
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

        debug!(
            "Found closest nodes for target {:?}: {:?}",
            target, closest_nodes
        );
        Ok(closest_nodes)
    }

    pub async fn find_node_rpc(
        &self,
        node_id: NodeId,
        addr: SocketAddr,
        target: NodeId,
    ) -> Result<Vec<(NodeId, SocketAddr)>, KademliaError> {
        debug!(
            "Sending FIND_NODE request to node {:?} at {:?}",
            node_id, addr
        );
        self.network_manager
            .send_message(&Message::FindNode { target }, addr)
            .await?;

        match tokio::time::timeout(
            self.config.request_timeout,
            self.network_manager.receive_message(),
        )
        .await
        {
            Ok(Ok((Message::NodesFound(nodes), _))) => {
                debug!("Received nodes from {:?}: {:?}", addr, nodes);
                Ok(nodes)
            }
            Ok(Ok(_)) => Err(KademliaError::UnexpectedResponse),
            Ok(Err(e)) => {
                error!("Error receiving response from {:?}: {:?}", addr, e);
                Err(e)
            }
            Err(_) => {
                warn!("Timeout waiting for response from {}", addr);
                Err(KademliaError::Timeout)
            }
        }
    }

    pub async fn send_store_message(
        &self,
        key: &[u8],
        value: &[u8],
        addr: SocketAddr,
    ) -> Result<bool, KademliaError> {
        let message = Message::Store {
            key: key.to_vec(),
            value: value.to_vec(),
            sender: self.id,
            timestamp: SystemTime::now(),
        };
        debug!(
            "Sending STORE message to {} for key: {:?}, value: {:?}",
            addr, key, value
        );
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
            Ok(Err(e)) => {
                error!("Error receiving STORE response from {}: {:?}", addr, e);
                Err(e)
            }
            Err(_) => {
                warn!("Timeout waiting for STORE response from {}", addr);
                Err(KademliaError::Timeout)
            }
            _ => {
                error!(
                    "Unexpected response while waiting for STORE confirmation from {}",
                    addr
                );
                Err(KademliaError::UnexpectedResponse)
            }
        }
    }

    // Methods for state inspection
    pub fn get_routing_table_size(&self) -> usize {
        let size = self
            .routing_table
            .buckets
            .iter()
            .map(|bucket| bucket.entries.len())
            .sum();
        debug!("Routing table size: {}", size);
        size
    }

    pub async fn get_storage_size(&self) -> usize {
        let size = self.storage_manager.storage.len();
        debug!("Local storage size: {}", size);
        size
    }

    pub async fn get_cached_entries_count(&self) -> usize {
        let count = self.storage_manager.cache.cache_store.read().await.len();
        debug!("Cache entries count: {}", count);
        count
    }
}

// Implement additional utility functions if needed

#[cfg(test)]
mod tests {
    use super::*;
    // Implement unit tests for KademliaNode and its components
}
