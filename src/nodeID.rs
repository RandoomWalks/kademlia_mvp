// NodeId Implementation

// Create a NodeId struct that represents a unique identifier for nodes in the network.
// Implement methods for creating a new random NodeId, generating a NodeId from a key, and calculating the distance between two NodeIds.
// Add debug and display formatting for NodeId.

#[derive(Debug,PartialEq, Eq, PartialOrd, Ord,Hash,Clone, Copy)]
struct NodeID {
    arr:[u8; 32] ,
}

impl NodeID {
    
    
    fn new()->Self {
        
        Self {
            arr:generate_random_id(),
        }
    }
    
    fn new(data: &[u8])->Self {
        Self {
            arr:generate_256bit_node_id(data),
        }
    }

    fn generate_random_id() -> [u8; 32] {
        let mut rng = rand::thread_rng();
        let mut id = [0u8; 32];
        rng.fill(&mut id);
        id
    }
    
    fn distance(&self, other: &NodeId) -> NodeId {
        // Calculate the XOR distance between two NodeIds.
        // Return the result as a new NodeId.
        let mut _tmp: [u8; 32] = [0;32];
        let mut _arr: [u8; 32] = self.arr;
        
        let mut _retVec:Vec<u8>  = [] ;
        
        for i in 0.._arr.len() {
            // let _xor:   =  _arr[i] ^ other[i];

            _tmp[i] =  _arr[i] ^ other[i];
            // _retVec.append( _xor );
            
        }
        Self{
            arr:_tmp,
        }
    }

    
    
}

struct Node {
    id:NodeID,
}

struct KBucket   {
    // Each entry in a k-bucket represents a node in the network, identified by its unique node ID.

    map: Hashmap<NodeID,Node>,

    // organized based on the distance between the local node and the other nodes in the network.
    
}

use sha2::{Sha256, Digest};

fn generate_256bit_node_id(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut node_id = [0u8; 32];
    node_id.copy_from_slice(&result);
    node_id
}


struct PingMessage {
    sender_id: String,
    sender_addr: SocketAddr,
}


// Serialization example (using serde or similar library)
impl PingMessage {
    fn to_bytes(&self) -> Vec<u8> {
        // Serialize self to bytes
        
    }
    
    fn from_bytes(bytes: &[u8]) -> PingMessage {
        // Deserialize bytes to PingMessage
        PingMessage{
            sender_id:"",
            sender_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        }
    }
}

