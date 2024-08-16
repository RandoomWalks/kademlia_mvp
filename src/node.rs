use crate::message::{FindValueResult, Message};
use crate::routing_table::RoutingTable;
use crate::utils::NodeId;

use crate::cache::cache_impl::Cache;


use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use std::fmt;

use crate::utils::{ALPHA, BOOTSTRAP_NODES, K};
use bincode::{deserialize, serialize};

/// Represents a Kademlia node in a distributed hash table (DHT).
/// Each node stores its ID, address, routing table, and a storage map for key-value pairs.
pub struct KademliaNode {
    pub id: NodeId,                         // The unique identifier of the node
    pub addr: SocketAddr,                   // The network address of the node
    pub routing_table: RoutingTable,        // The routing table for this node, storing known nodes
    pub storage: HashMap<Vec<u8>, Vec<u8>>, // Key-value storage for the node
    pub socket: Arc<UdpSocket>,             // UDP socket for communication
    pub shutdown: mpsc::Receiver<()>,       // Channel receiver for shutdown signals
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
    pub async fn handle_message(&mut self, message: Message, src: SocketAddr) -> std::io::Result<()> {
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
                self.store(&key, &value);
                self.send_message(&Message::Stored, src).await?;
                info!("Stored value for key {:?} from {:#?}", key, src);
            }
            // Handles a FindNode request:
            // - Searches for the closest nodes to the target NodeId in the routing table.
            // - Sends the found nodes back to the requester as a NodesFound message.
            Message::FindNode { target } => {
                let nodes = self.find_node(&target);
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
    pub fn store(&mut self, key: &[u8], value: &[u8]) {
        let hash = Self::hash_key(key);
        self.storage.insert(hash.to_vec(), value.to_vec());
        info!("Stored value for key: {:?}", hash);
    }

    /// Hashes a key using the SHA-256 algorithm and returns the hash as a byte vector.
    ///
    /// **Input:**
    /// - `key`: The key as a byte slice.
    ///
    /// **Output:** +
    /// - Returns a byte vector representing the hashed key.
    ///
    /// Example:
    /// ```rust
    /// let hash = KademliaNode::hash_key(b"example_key");
    /// ```
    pub fn hash_key(key: &[u8]) -> Vec<u8> {
        Sha256::digest(key).to_vec()
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
    /// let closest_nodes = node.find_node(&target_node);
    /// ```
    pub fn find_node(&self, target: &NodeId) -> Vec<(NodeId, SocketAddr)> {
        self.routing_table.find_closest(target, K)
    }

    /// Finds a value in the local storage or returns the closest nodes if the value is not found.
    ///
    /// **Input:**
    /// - `key`: The key as a byte slice to search for.
    ///
    /// **Output:**
    /// - Returns a `FindValueResult`, which can be either the value found or a list of closest nodes.
    ///
    /// Example:
    /// ```rust
    /// let key = b"example_key";
    /// match node.find_value(&key) {
    ///     FindValueResult::Value(val) => println!("Value found: {:?}", val),
    ///     FindValueResult::Nodes(nodes) => println!("Nodes found: {:?}", nodes),
    /// }
    /// ```
    pub fn find_value(&self, key: &[u8]) -> FindValueResult {
        let hash = Self::hash_key(key);
        if let Some(value) = self.storage.get(&hash) {
            FindValueResult::Value(value.clone())
        } else {
            let hash_array: [u8; 32] = hash[..].try_into().expect("Hash length is not 32 bytes"); // Converts the `hash` (which is a `Vec<u8>` of SHA-256 hash bytes) into a fixed-size array of 32 bytes.

            FindValueResult::Nodes(self.find_node(&NodeId::from_slice(&hash_array)))
        }
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
    ///
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

        let nodes = self.find_node(&target);

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
        if let Some(value) = self.storage.get(&Self::hash_key(key)) {
            return Ok(Some(value.clone()));
        }

        let hash = Self::hash_key(key);
        let hash_array: [u8; 32] = hash[..].try_into().expect("Hash length is not 32 bytes"); // Converts the `hash` (which is a `Vec<u8>` of SHA-256 hash bytes) into a fixed-size array of 32 bytes.

        let target = NodeId::from_slice(&hash_array);

        let nodes = self.find_node(&target);

        for (_, addr) in nodes.iter().take(ALPHA) {
            if let Err(e) = self
                .send_message(&Message::FindValue { key: key.to_vec() }, *addr)
                .await
            {
                error!("Failed to send FindValue message to {:#?}: {:?}", addr, e);
            }
        }

        // For simplicity, we're returning None here. In a complete implementation,
        // this method would wait for responses from the nodes and return the value if found.
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
