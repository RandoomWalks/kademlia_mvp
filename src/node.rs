use crate::message::{FindValueResult, Message};
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;

use crate::cache::cache_impl;
use crate::cache::entry;
use crate::cache::policy;
// use crate::cache::cache_impl::Cache;

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use std::collections::{BinaryHeap, HashSet};
use std::collections::{VecDeque};
use std::cmp::Ordering;
use tokio::sync::Mutex;


use std::fmt;

use crate::utils::{ALPHA, BOOTSTRAP_NODES, K};
use bincode::{deserialize, serialize};



// Add this new struct to manage lookup state

struct NodeInfo {
    id: NodeId,
    addr: SocketAddr,
    distance: NodeId,
}

impl PartialEq for NodeInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for NodeInfo {}

impl PartialOrd for NodeInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.distance.compare_distance(&other.distance))
    }
}

impl Ord for NodeInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance.compare_distance(&other.distance)
    }
}

struct LookupState {
    target: NodeId,
    closest_nodes: Vec<NodeInfo>,
    queried_nodes: HashSet<NodeId>,
    pending_queries: HashSet<NodeId>,
}

impl LookupState {
    fn new(target: NodeId, initial_nodes: Vec<(NodeId, SocketAddr)>) -> Self {
        let mut closest_nodes = Vec::new();
        for (id, addr) in initial_nodes {
            let distance = target.distance(&id);
            closest_nodes.push(NodeInfo { id, addr, distance });
        }
        closest_nodes.sort_unstable();

        LookupState {
            target,
            closest_nodes,
            queried_nodes: HashSet::new(),
            pending_queries: HashSet::new(),
        }
    }

    fn update_with_new_nodes(&mut self, new_nodes: Vec<(NodeId, SocketAddr)>) {
        for (id, addr) in new_nodes {
            if !self.queried_nodes.contains(&id) && !self.pending_queries.contains(&id) {
                let distance = self.target.distance(&id);
                self.closest_nodes.push(NodeInfo { id, addr, distance });
            }
        }

        self.closest_nodes.sort_unstable();
        self.closest_nodes.truncate(K);
    }

    fn select_next_nodes(&mut self, alpha: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut selected = Vec::new();

        for node_info in self.closest_nodes.iter() {
            if selected.len() >= alpha {
                break;
            }
            if !self.queried_nodes.contains(&node_info.id) && !self.pending_queries.contains(&node_info.id) {
                selected.push((node_info.id.clone(), node_info.addr));
                self.pending_queries.insert(node_info.id.clone());
            }
        }

        selected
    }
}
/// Represents a Kademlia node in a distributed hash table (DHT).
/// Each node stores its ID, address, routing table, and a storage map for key-value pairs.
pub struct KademliaNode {
    pub id: NodeId,                         // The unique identifier of the node
    pub addr: SocketAddr,                   // The network address of the node
    pub routing_table: RoutingTable,        // The routing table for this node, storing known nodes
    pub storage: HashMap<Vec<u8>, Vec<u8>>, // Key-value storage for the node
    pub socket: Arc<UdpSocket>,             // UDP socket for communication
    pub shutdown: mpsc::Receiver<()>,       // Channel receiver for shutdown signals

    pub cache: cache_impl::Cache<Vec<u8>, Vec<u8>>,
    pub cache_config: policy::CacheConfig,

}

impl fmt::Debug for KademliaNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KademliaNode")
            .field("id", &format!("{:x?}", self.id.0))
            .field("addr", &self.addr)
            .field("routing_table", &self.routing_table)
            .field("storage_size", &self.storage.len()) // Display storage size instead of full data
            .field("socket", &"UDP Socket")
            .field("shutdown", &"Receiver<()>")
            .finish()
    }
}

impl KademliaNode {
    /// Creates a new Kademlia node bound to the specified address.
    ///
    /// **Input:**
    /// - `addr`: The `SocketAddr` where the node should bind its UDP socket.
    ///
    /// **Output:**
    /// - Returns a `Result` containing the `KademliaNode` instance and a shutdown sender channel.
    ///
    /// Example:
    /// ```rust
    /// let addr = "127.0.0.1:8080".parse().unwrap();
    /// let (node, shutdown_sender) = KademliaNode::new(addr).await.unwrap();
    /// ```
    
    pub async fn new(addr: SocketAddr, cache_config: Option<policy::CacheConfig>) -> std::io::Result<(Self, mpsc::Sender<()>)> {
        let id = NodeId::new();
        let socket = UdpSocket::bind(addr).await?;
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        let config = cache_config.unwrap_or_default();
        let cache: cache_impl::Cache<Vec<u8>, Vec<u8>> = cache_impl::Cache::with_config(&config);
        
        Ok((
            KademliaNode {
                id: id.clone(),
                addr,
                routing_table: RoutingTable::new(id),
                storage: HashMap::new(),
                socket: Arc::new(socket),
                shutdown: shutdown_receiver,
                cache: cache_impl::Cache::with_policy(policy::EvictionPolicy::LRU,10),
                cache_config: config,

            },
            shutdown_sender,
        ))
    }

    async fn start_cache_maintenance(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Run every hour
        loop {
            interval.tick().await;
            debug!("Performing cache maintenance");
            // if let Err(e) = self.cache.evict().await {
            //     warn!("Error during cache eviction: {:?}", e);
            // }
        }
    }
    
    pub async  fn cache_hit_count(&self) -> usize {
        // Return the cache hit count from your metrics
        self.cache.metrics.read().await.hits
    }
    
    pub async  fn cache_size(&self) -> usize {
        // Return the current size of the cache
        self.cache.cache_store.read().await.len()
    }
    /// Bootstraps the node by pinging a list of predefined bootstrap nodes.
    ///
    /// **Input:** None
    ///
    /// **Output:**
    /// - Returns a `Result` indicating success or failure of the bootstrap process.
    ///
    /// Example:
    /// ```rust
    /// node.bootstrap().await.unwrap();
    /// ```
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

    /// Runs the main event loop of the node, handling incoming messages, refreshing the routing table, and
    /// checking for shutdown signals.
    ///
    /// **Input:** None
    ///
    /// **Output:**
    /// - Returns a `Result` indicating success or failure of the event loop.
    ///
    /// Example:
    /// ```rust
    /// node.run().await.unwrap();
    /// ```
    pub async fn run(&mut self) -> std::io::Result<()> {
        let mut buf = vec![0u8; 1024];
        let mut refresh_interval = interval(Duration::from_secs(3600)); // Refresh every hour

        loop {
            tokio::select! {
                // Handle incoming messages
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
                // Refresh routing table periodically
                _ = refresh_interval.tick() => {
                    if let Err(e) = self.refresh_buckets().await {
                        error!("Failed to refresh buckets: {:?}", e);
                    }
                }
                // Handle shutdown signal
                _ = self.shutdown.recv() => {
                    info!("Received shutdown signal, stopping node");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Handles incoming Kademlia protocol messages and responds accordingly.
    ///
    /// **Input:**
    /// - `message`: The received `Message` to handle.
    /// - `src`: The `SocketAddr` of the message sender.
    ///
    /// **Output:**
    /// - Returns a `Result` indicating success or failure of message handling.
    ///
    /// Example:
    /// ```rust
    /// let message = Message::Ping { sender: NodeId::new() };
    /// node.handle_message(message, "127.0.0.1:8080".parse().unwrap()).await.unwrap();
    /// ```
    pub async fn handle_message(
        &mut self,
        message: Message,
        src: SocketAddr,
    ) -> std::io::Result<()> {
        match message {
            Message::Ping { sender } => {
                self.routing_table.update(sender, src);
                self.send_message(&Message::Pong { sender: self.id }, src)
                    .await?; // - Sends back a Pong response to confirm the node is active.

                info!("Received Ping from {:#?}, responded with Pong", src);
            }
            Message::Pong { sender } => {
                self.routing_table.update(sender, src); // - Updates the routing table with the sender's NodeId and address.

                info!("Received Pong from {:#?}", src);
            }
            Message::Store { key, value } => {
                // TODO: Find k closest nodes to myself (as opposed to the key of the storvalue)
                let hash = Self::hash_key(&key);
                // let target = NodeId::from_slice(&hash);
                
                // let sli_mut = hash.as_ref();
                let sli_mut: &[u8] = hash.as_slice();
                let fixed_sli:[u8; 32] = sli_mut.try_into().unwrap();
                // let sli:[u8;32] = hash.iter().map(|&i| i as u8).try_into().unwrap();

                // let hash_array: NodeId = hash[..].try_into().expect("Hash length is not 32 bytes"); // Converts the `hash` (which is a `Vec<u8>` of SHA-256 hash bytes) into a fixed-size array of 32 bytes.
                
                // let fixed_len_sli:NodeId = sli_mut.try_into();


                let k = 3;
                let n = NodeId(fixed_sli);

                let closest_nodes = self.routing_table.find_closest(&n , k);

                // Storing locally if we're one of the k closest
                for (node_id, addr) in closest_nodes {
                    // check not self
                    if node_id != self.id {
                        // send rpc
                        self.send_message(&Message::Store { key: key.clone(), value: value.clone() }, addr).await?;
                    }
                }
                self.send_message(&Message::Stored, src).await?;

                // TODO: Send STORE RPCs to each of these k nodes.


                
                // self.store(&key, &value).await;
                // self.send_message(&Message::Stored, src).await?;
                info!("Stored value for key {:?} from {:#?}", key, src);
            }
            // Handles a FindNode request:
            // - Searches for the closest nodes to the target NodeId in the routing table.
            // - Sends the found nodes back to the requester as a NodesFound message.
            Message::FindNode { target } => {
                let nodes = self.find_node(&target).await;
                self.send_message(&Message::NodesFound(nodes), src).await?;
                info!(
                    "Received FindNode from {:#?}, responded with NodesFound",
                    src
                );
            }
            // Handles a FindValue request:
            // - First checks if the value for the given key is stored locally.
            // - If found, sends the value back as a ValueFound message.
            // - If not found, sends back the closest nodes as a NodesFound message.
            Message::FindValue { key } => match self.find_value(&key).await {
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

    /// Sends a serialized message to the specified destination address.
    ///
    /// **Input:**
    /// - `message`: The `Message` to send.
    /// - `dst`: The `SocketAddr` of the destination node.
    ///
    /// **Output:**
    /// - Returns a `Result` indicating success or failure of sending the message.
    ///
    /// Example:
    /// ```rust
    /// node.send_message(&Message::Ping { sender: node.id }, "127.0.0.1:8080".parse().unwrap()).await.unwrap();
    /// ```
    async fn send_message(&self, message: &Message, dst: SocketAddr) -> std::io::Result<()> {
        let serialized = serialize(message).map_err(|e| {
            error!("Failed to serialize message: {:?}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Serialization error")
        })?;
        self.socket.send_to(&serialized, dst).await?;
        debug!("Sent message to {:#?}", dst);
        Ok(())
    }

    /// Stores a key-value pair in the node's local storage after hashing the key.
    ///
    /// **Input:**
    /// - `key`: The key as a byte slice.
    /// - `value`: The value as a byte slice.
    ///
    /// **Output:** None
    ///
    /// Example:
    /// ```rust
    /// node.store(b"example_key", b"example_value");
    /// ```
    pub async fn store(&mut self, key: &[u8], value: &[u8]) {
        let hash = Self::hash_key(key);
        self.storage.insert(hash.to_vec(), value.to_vec());
        
        // self.cache
        //     .put(hash.to_vec(), value.to_vec(), Duration::from_secs(3600));
        // Add this block to store in cache
        if let Err(e) = self
            .cache
            .put(hash.to_vec(), value.to_vec(), Duration::from_secs(3600)).await
        {
            error!("Failed to store in cache: {:?}", e);
        }
        info!("Stored value for key: {:?}", hash);
    }

    /// Hashes a key using the SHA-256 algorithm and returns the hash as a byte vector.
    ///
    /// **Input:**
    /// - `key`: The key as a byte slice.
    ///
    /// **Output:** +
    /// - Returns a byte vector representing the hashed key.
    ///  SHA-256 always produces a fixed-size output of 256 bits, which is equivalent to 32 bytes.
    /// Example:
    /// ```rust
    /// let hash = KademliaNode::hash_key(b"example_key");
    /// ```
    pub fn hash_key(key: &[u8]) -> Vec<u8> {
        Sha256::digest(key).to_vec()
    }


    pub async fn iterative_find_node(&self, target: &NodeId) -> Vec<(NodeId, SocketAddr)> {
        let initial_nodes = self.routing_table.find_closest(target, ALPHA);
        let state = Mutex::new(LookupState::new(target.clone(), initial_nodes));
        let mut round = 0;

        loop {
            let nodes_to_query = {
                let mut state_guard = state.lock().await;
                state_guard.select_next_nodes(ALPHA)
            };

            if nodes_to_query.is_empty() {
                break;
            }

            let mut query_futures = Vec::new();
            for (node_id, addr) in nodes_to_query.iter() {
                let future = self.find_node_rpc(node_id.clone(), addr.clone(), target.clone());
                query_futures.push(future);
            }

            let results = futures::future::join_all(query_futures).await;

            let mut new_nodes = Vec::new();
            for (result, (node_id, addr)) in results.into_iter().zip(nodes_to_query.iter()) {
                match result {
                    Ok(nodes) => new_nodes.extend(nodes),
                    Err(e) => warn!("Failed to query node {:?}: {:?}", node_id, e),
                }

                let mut state_guard = state.lock().await;
                state_guard.queried_nodes.insert(node_id.clone());
                state_guard.pending_queries.remove(node_id);
            }

            {
                let mut state_guard = state.lock().await;
                state_guard.update_with_new_nodes(new_nodes);
            }

            round += 1;
            if round >= 3 { // You might want to adjust this termination condition
                break;
            }
        }

        let state_guard = state.lock().await;
        state_guard.closest_nodes.iter()
            .take(K)
            .map(|node_info| (node_info.id.clone(), node_info.addr))
            .collect()
    }

    async fn find_node_rpc(&self, node_id: NodeId, addr: SocketAddr, target: NodeId) -> Result<Vec<(NodeId, SocketAddr)>, std::io::Error> {
        let message = Message::FindNode { target };
        self.send_message(&message, addr).await?;

        // In a real implementation, you'd wait for and process the response here
        // For now, we'll just return an empty vector
        Ok(Vec::new())
    }

    /// Finds the closest nodes to a given target node ID within the routing table.
    ///
    /// **Input:**
    /// - `target`: The `NodeId` of the target node to search for.
    ///
    /// **Output:**
    /// - Returns a `Vec<(NodeId, SocketAddr)>` containing the closest nodes found.
    ///
    /// Example:
    /// ```rust
    /// let target_node = NodeId::new();
    /// let closest_nodes = node.find_node(&target_node).await;
    /// ```
    pub async fn find_node(&self, target: &NodeId) -> Vec<(NodeId, SocketAddr)> {
        // self.routing_table.find_closest(target, K)
        self.iterative_find_node(target).await

    }

    pub async fn find_value(&self, key: &[u8]) -> FindValueResult {
        let hash = Self::hash_key(key);
        
        // Check cache first
        if let Ok(value) = self.cache.get(&hash).await {
            return FindValueResult::Value(value);
        }
        
        // Then check local storage
        if let Some(value) = self.storage.get(&hash) {
            // Store in cache for future use
            if let Err(e) = self.cache.put(hash.clone(), value.clone(), Duration::from_secs(3600)).await {
                warn!("Failed to store in cache: {:?}", e);
            }
            return FindValueResult::Value(value.clone());
        }
    
        // If not found locally, return closest nodes
        FindValueResult::Nodes(self.find_node(&NodeId::from_slice(hash[..].try_into().expect("Hash length is not 32 bytes"))).await )
    }

    /// Pings a remote node to check its availability.
    ///
    /// **Input:**
    /// - `addr`: The `SocketAddr` of the node to ping.
    ///
    /// **Output:**
    /// - Returns a `Result` indicating success or failure of the ping operation.
    ///
    /// This method sends a Ping message to the specified address.
    ///
    /// Example:
    /// ```rust
    /// node.ping("127.0.0.1:8080".parse().unwrap()).await.unwrap();
    /// ```
    pub async fn ping(&self, addr: SocketAddr) -> std::io::Result<()> {
        self.send_message(&Message::Ping { sender: self.id }, addr)
            .await
    }

    /// Stores a key-value pair in the local storage and informs nearby nodes.
    /// method is used when you want to ensure the key-value pair is stored across the network, not just locally
    /// **Input:**
    /// - `key`: The key as a byte slice to store.
    /// - `value`: The value as a byte slice to store.
    ///
    /// **Output:**
    /// - Returns a `Result` indicating success or failure of the store operation.
    ///
    /// This method also sends a Store message to the closest nodes responsible for the key.
    ///
    /// Example:
    /// ```rust
    /// node.put(b"example_key", b"example_value").await.unwrap();
    /// ```
    pub async fn put(&mut self, key: &[u8], value: &[u8]) -> std::io::Result<()> {
        let hash = Self::hash_key(key);
        let hash_array: [u8; 32] = hash[..].try_into().expect("Hash length is not 32 bytes"); // Converts the `hash` (which is a `Vec<u8>` of SHA-256 hash bytes) into a fixed-size array of 32 bytes.  If `hash` is not exactly 32 bytes, the `expect` call will panic

        let target = NodeId::from_slice(&hash_array);

        let nodes = self.find_node(&target).await;

        for (_, addr) in nodes.iter().take(ALPHA) {
            if let Err(e) = self
                .send_message(
                    &Message::Store {
                        key: key.to_vec(),
                        value: value.to_vec(),
                    },
                    *addr,
                )
                .await
            {
                error!("Failed to send Store message to {:#?}: {:?}", addr, e);
            }
        }
        
        self.store(key, value);
        Ok(())
    }

    /// Retrieves a value associated with a key from local storage or requests it from nearby nodes.
    ///
    /// **Input:**
    /// - `key`: The key as a byte slice to retrieve.
    ///
    /// **Output:**
    /// - Returns an `Option<Vec<u8>>` containing the value if found, or `None` if not found.
    ///
    /// This method sends FindValue messages to the closest nodes responsible for the key.
    ///
    /// Example:
    /// ```rust
    /// if let Some(value) = node.get(b"example_key").await.unwrap() {
    ///     println!("Found value: {:?}", value);
    /// } else {
    ///     println!("Value not found");
    /// }
    /// ```
    pub async fn get(&mut self, key: &[u8]) -> std::io::Result<Option<Vec<u8>>> {
        //! Network Lookup: The current implementation of get() doesn't actually perform a complete network lookup. It only checks the cache and local storage, then initiates a network lookup without waiting for the results. This means that the None case in the new code doesn't truly represent "not found in the network", but rather "not found locally".

        
        let hash = Self::hash_key(key);
        
        // Try to get from cache first
        if let Ok(value) = self.cache.get(&hash).await {
            debug!("Cache hit for key: {:?}", hash);
            return Ok(Some(value));
        }
        
        // If not in cache, check local storage
        if let Some(value) = self.storage.get(&hash) {
            debug!("Storage hit for key: {:?}", hash);
            // Store in cache for future use
            if let Err(e) = self.cache.put(hash.clone(), value.clone(), Duration::from_secs(3600)).await {
                warn!("Failed to store in cache: {:?}", e);
            }
            return Ok(Some(value.clone()));
        }

        debug!("Key not found locally, performing network lookup: {:?}", hash);
        // Perform network lookup (existing code)
        
        // taking the first 32 byte and
        //  convert the slice &hash[..32] into a fixed-size array [u8; 32].
        let res:&[u8; 32] = &hash[..32].try_into().expect("Slice with incorrect length");

        let target = NodeId::from_slice(res);

        let nodes = self.find_node(&target).await;

        for (_, addr) in nodes.iter().take(ALPHA) {
            if let Err(e) = self
                .send_message(&Message::FindValue { key: key.to_vec() }, *addr)
                .await
            {
                warn!("Failed to send FindValue message to {:#?}: {:?}", addr, e);
            }
        }

        // Note: This is a simplification. In a complete implementation,
        // you would wait for responses and return the value if found.
        // For now, we'll just return None to indicate the value wasn't found locally.
        Ok(None)
    }

    /// Refreshes the buckets in the routing table by ensuring that at least one node is active in each bucket.
    ///
    /// **Input:** None
    ///
    /// **Output:**
    /// - Returns a `Result` indicating success or failure of the refresh operation.
    ///
    /// This method might involve pinging nodes to ensure they are still active and potentially
    /// updating the routing table with new or recently active nodes.
    ///
    /// Example:
    /// ```rust
    /// node.refresh_buckets().await.unwrap();
    /// ```
    pub async fn refresh_buckets(&mut self) -> std::io::Result<()> {
        // let mut tasks = Vec::new();

        // for bucket in &self.routing_table.buckets {
        //     for entry in &bucket.entries {
        //         let addr = entry.addr;
        //         let task = tokio::spawn(async move {
        //             // Ping each node to check if it's still active
        //             if let Err(e) = UdpSocket::connect(addr).await {
        //                 warn!("Failed to ping node at {:#?}: {:?}", addr, e);
        //             }
        //         });
        //         tasks.push(task);
        //     }
        // }

        // // Await completion of all ping tasks
        // for task in tasks {
        //     task.await
        //         .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        // }

        info!("Buckets refreshed");
        Ok(())
    }
}

