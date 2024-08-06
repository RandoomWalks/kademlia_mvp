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

1. **NodeId**: Represents a unique identifier for a node in the network, implemented using a 256-bit (32-byte) array. The NodeId is generated using random bytes or derived from a key using SHA-256 hashing.

2. **KBucket**: A k-bucket is a list of nodes stored in a routing table. Each k-bucket stores up to `K` nodes and is periodically refreshed to maintain active connections.

3. **RoutingTable**: The routing table is composed of multiple k-buckets. It is responsible for maintaining and updating the network's topology by storing the closest nodes to a given NodeId.

4. **Message**: Defines the types of messages exchanged between nodes, including Ping, Pong, FindNode, and FindNodeResponse.

5. **KademliaNode**: Represents a node in the Kademlia network. It handles the creation, startup, and communication with other nodes, including sending and receiving messages, and managing the routing table.

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
