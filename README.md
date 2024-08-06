# Kademlia MVP

This project implements a Minimum Viable Product (MVP) for the Kademlia Distributed Hash Table (DHT) protocol in Rust. The goal is to provide a basic implementation of Kademlia nodes that can communicate, store, and retrieve data across a decentralized network.

## Table of Contents

1. [Introduction](#introduction)
2. [Features](#features)
3. [Getting Started](#getting-started)
   - [Prerequisites](#prerequisites)
   - [Installation](#installation)
4. [Usage](#usage)
   - [Running the Nodes](#running-the-nodes)
   - [Testing](#testing)
5. [Technical Architecture](#technical-architecture)
   - [Overview](#overview)
   - [Components](#components)
   - [Diagrams](#diagrams)
6. [Project Structure](#project-structure)
7. [Contributing](#contributing)
8. [License](#license)

## Introduction

Kademlia is a peer-to-peer distributed hash table used for decentralized storage and retrieval of data. This implementation is a simple MVP that demonstrates the core functionalities of the Kademlia protocol, including node creation, network communication, and basic routing table management.

## Features

- Node creation with unique identifiers
- Basic Kademlia message types (Ping, Pong, FindNode, FindNodeResponse)
- XOR-based distance metric for node identification
- Routing table with k-buckets
- Simple network communication using TCP
- Logging using `env_logger`

## Getting Started

### Prerequisites

To build and run this project, you need the following installed on your system:

- [Rust](https://www.rust-lang.org/tools/install)
- [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)

### Installation

Clone the repository and navigate to the project directory:

```sh
git clone https://github.com/yourusername/kademlia_mvp.git
cd kademlia_mvp
```

Build the project using Cargo:

```sh
cargo build
```

## Usage

### Running the Nodes

You can run the Kademlia nodes using the `cargo run` command. This example creates two nodes and demonstrates basic communication between them:

```sh
cargo run
```

### Testing

Run the tests to ensure the implementation works as expected:

```sh
cargo test
```

## Technical Architecture

### Overview

The Kademlia MVP project is structured around the core components of the Kademlia protocol: node identification, routing, and message handling. Each node operates independently, communicating with other nodes over TCP to maintain a decentralized network.

### Components

## NodeId

### Description

The `NodeId` struct represents a unique identifier for a node in the Kademlia network. It is implemented as a 256-bit (32-byte) array.

### Implementation

- **Random Generation**: Uses `rand::random` to generate a random 256-bit identifier.
- **SHA-256 Hashing**: Provides a method to create a `NodeId` from a given key using SHA-256 hashing.
- **Distance Calculation**: Implements the XOR metric for calculating the distance between two `NodeId` values.


## KBucket

### Description

A `KBucket` is a list of nodes stored in the routing table. Each k-bucket can store up to `K` nodes and is periodically refreshed.

### Implementation

- **Node Storage**: Uses a `VecDeque` to store `KBucketEntry` values.
- **Entry Management**: Provides methods to update the k-bucket with new nodes and manage the `last_seen` timestamps.
- **Refresh Checking**: Checks if the k-bucket needs to be refreshed based on a predefined interval.


## RoutingTable

### Description

The `RoutingTable` maintains the topology of the Kademlia network by storing multiple `KBucket` instances. It is responsible for managing and updating the network's routing information.

### Implementation

- **Bucket Management**: Initializes 256 k-buckets to cover all possible distances in the XOR metric.
- **Node Updates**: Updates the appropriate k-bucket based on the distance between the current node and the target node.
- **Closest Nodes Retrieval**: Retrieves the closest nodes to a given target `NodeId`.


## Message

### Description

The `Message` enum defines the types of messages exchanged between Kademlia nodes. These messages facilitate node discovery and communication within the network.

### Implementation

- **Message Types**: Includes `Ping`, `Pong`, `FindNode`, and `FindNodeResponse`.
- **Serialization**: Implements serialization and deserialization using `serde`.


## KademliaNode

### Description

The `KademliaNode` struct represents a node in the Kademlia network. It manages the node's identity, address, and routing table, and handles network communication.

### Implementation

- **Node Initialization**: Creates a new node with a random `NodeId` and a specified `SocketAddr`.
- **Network Listener**: Binds a TCP listener to the node's address to handle incoming connections.
- **Message Handling**: Processes incoming messages and responds appropriately.
- **Ping and FindNode Operations**: Implements the core Kademlia operations for node discovery and communication.


## Network Communication

### Description

Network communication is handled using asynchronous TCP connections provided by the `tokio` library. Nodes communicate by sending and receiving serialized `Message` structs.

### Implementation

- **TCP Listener**: Listens for incoming connections and spawns a new task to handle each connection.
- **Message Serialization**: Uses `bincode` for efficient binary serialization of messages.
- **Asynchronous I/O**: Uses `tokio::io` for non-blocking read and write operations.


## Bootstrap Process

### Description

The bootstrap process is responsible for initializing a node's routing table by discovering and communicating with a list of known bootstrap nodes.

### Implementation

- **Ping Bootstrap Nodes**: Sends a ping message to each bootstrap node to verify connectivity.
- **Update Routing Table**: Updates the routing table with nodes discovered during the bootstrap process.
- **Perform FindNode**: Uses the `FindNode` message to discover additional nodes and further populate the routing table.

---


### Diagrams

#### 1. Node Interaction Diagram

```plaintext
+------------+                +------------+
|  Node 1    |                |  Node 2    |
|------------|                |------------|
|            |  Ping          |            |
|            |--------------->|            |
|            |                |            |
|            |                |   Pong     |
|            |<---------------|            |
|            |                |            |
+------------+                +------------+
```

#### 2. Kademlia Routing Table

```plaintext
+------------------------+
|      RoutingTable      |
|------------------------|
| +--------------------+ |
| |      KBucket       | |
| |--------------------| |
| | NodeId, SocketAddr | |
| | NodeId, SocketAddr | |
| |        ...         | |
| +--------------------+ |
| +--------------------+ |
| |      KBucket       | |
| |--------------------| |
| | NodeId, SocketAddr | |
| | NodeId, SocketAddr | |
| |        ...         | |
| +--------------------+ |
|          ...           |
+------------------------+
```

#### 3. Node Lifecycle Diagram

```plaintext
+-----------------------+
|   Start Kadem

liaNode  |
+-----------+-----------+
            |
            v
+-----------+-----------+
|  Initialize NodeId    |
+-----------+-----------+
            |
            v
+-----------+-----------+
| Bind TCP Listener     |
+-----------+-----------+
            |
            v
+-----------+-----------+
|  Wait for Connections |
+-----------+-----------+
            |
+-----------+-----------+
|   Handle Connection   |<-----------------------+
+-----------+-----------+                        |
            |                                    |
+-----------+-----------+   Incoming Connection  |
|  Receive Message      |<-----------------------+
+-----------+-----------+                        |
            |                                    |
            v                                    |
+-----------+-----------+                        |
| Process Message       |                        |
+-----------+-----------+                        |
            |                                    |
            v                                    |
+-----------+-----------+                        |
| Send Response         |                        |
+-----------+-----------+                        |
            |                                    |
            +------------------------------------+
```

#### 4. Kademlia Message Flow

```plaintext
+---------------------+                     +---------------------+
|      Node A         |                     |      Node B         |
|---------------------|                     |---------------------|
| +-----------------+ |                     | +-----------------+ |
| | Send Ping       | |                     | | Receive Ping    | |
| +-----------------+ |                     | +-----------------+ |
| | Wait for Pong   | |                     | | Send Pong       | |
| +-----------------+ |                     | +-----------------+ |
| | Receive Pong    | |                     | |                  | |
| +-----------------+ |                     | +-----------------+ |
| | Send FindNode   | |                     | | Receive FindNode| |
| +-----------------+ |                     | +-----------------+ |
| | Wait for Response| |                     | | Send Response   | |
| +-----------------+ |                     | +-----------------+ |
| | Receive Response | |                     | |                  | |
| +-----------------+ |                     | +-----------------+ |
+---------------------+                     +---------------------+
```

#### 5. Detailed Kademlia Node Structure

```plaintext
+---------------------+
|   KademliaNode      |
|---------------------|
| +-----------------+ |
| | NodeId          | |
| +-----------------+ |
| | SocketAddr      | |
| +-----------------+ |
| | RoutingTable    | |
| +-----------------+ |
| | Start()         | |
| +-----------------+ |
| | HandleConnection| |
| +-----------------+ |
| | SendPing()      | |
| +-----------------+ |
| | SendFindNode()  | |
| +-----------------+ |
| | ...             | |
+---------------------+
```

#### 6. Routing Table and K-Buckets

```plaintext
+-------------------------------+
|         RoutingTable          |
|-------------------------------|
|  +-------------------------+  |
|  |        KBucket          |  |
|  |-------------------------|  |
|  | +---------------------+ |  |
|  | |  NodeId, SocketAddr  | |  |
|  | +---------------------+ |  |
|  | |  NodeId, SocketAddr  | |  |
|  | +---------------------+ |  |
|  |          ...            |  |
|  +-------------------------+  |
|  +-------------------------+  |
|  |        KBucket          |  |
|  |-------------------------|  |
|  | +---------------------+ |  |
|  | |  NodeId, SocketAddr  | |  |
|  | +---------------------+ |  |
|  | |  NodeId, SocketAddr  | |  |
|  | +---------------------+ |  |
|  |          ...            |  |
|  +-------------------------+  |
|             ...               |
+-------------------------------+
```

#### 7. Bootstrapping Process

```plaintext
+----------------------------+
|   Bootstrap KademliaNode   |
+-------------+--------------+
              |
              v
+-------------+--------------+
|  List of Bootstrap Nodes   |
+-------------+--------------+
              |
              v
+-------------+--------------+
|    Ping Each Bootstrap     |
+-------------+--------------+
              |
              v
+-------------+--------------+
|   Update Routing Table     |
+-------------+--------------+
              |
              v
+-------------+--------------+
|  Perform FindNode on Self  |
+-------------+--------------+
              |
              v
+-------------+--------------+
| Add Discovered Nodes to    |
| Routing Table              |
+-------------+--------------+
```

### Project Structure

```
.
├── .gitignore
├── Cargo.lock
├── Cargo.toml
├── README.md
├── src
│   ├── lib.rs
│   └── main.rs
├── tests
│   └── test1.rs
└── test_snapshot_thread.json
```

- **src/main.rs**: Contains the main entry point of the application.
- **src/lib.rs**: Contains the core logic of the Kademlia implementation.
- **tests/test1.rs**: Contains the test cases for the Kademlia implementation.
