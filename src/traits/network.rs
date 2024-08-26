use std::net::SocketAddr;
use async_trait::async_trait;
use tokio::io::Result;

// #[async_trait]
// pub trait NetworkInterface: Send + Sync {
//     async fn bind(&mut self, address: SocketAddr) -> Result<()>;
//     async fn send(&self, destination: SocketAddr, data: &[u8]) -> Result<()>;
//     async fn receive(&self) -> Result<(SocketAddr, Vec<u8>)>;
// }

use tokio::net::UdpSocket;


pub struct UdpNetwork {
    socket: UdpSocket,
}

impl UdpNetwork {
    pub async fn bind(addr: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        Ok(Self { socket })
    }

    pub async fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize> {
        self.socket.send_to(buf, addr).await
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.socket.recv_from(buf).await
    }
}

// // use std::sync::Mutex;
use tokio::sync::{Mutex, mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender}};

// // use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

// pub struct MockNetwork {
//     pub sender: UnboundedSender<(SocketAddr, Vec<u8>)>,
//     pub receiver: Mutex<UnboundedReceiver<(SocketAddr, Vec<u8>)>>,
// }

// impl MockNetwork {
//     pub fn new() -> Self {
//         let (sender, receiver) = unbounded_channel();
//         Self {
//             sender,
//             receiver: Mutex::new(receiver),
//         }
//     }
// }

// #[async_trait]
// impl NetworkInterface for MockNetwork {
//     async fn bind(&mut self, _address: SocketAddr) -> Result<()> {
//         // No-op for the mock implementation
//         Ok(())
//     }

//     async fn send(&self, destination: SocketAddr, data: &[u8]) -> Result<()> {
//         self.sender.send((destination, data.to_vec())).unwrap();
//         Ok(())
//     }

//     async fn receive(&self) -> Result<(SocketAddr, Vec<u8>)> {
//         let mut receiver = self.receiver.lock().await;
//         if let Some((addr, data)) = receiver.recv().await {
//             Ok((addr, data))
//         } else {
//             Err(tokio::io::Error::new(tokio::io::ErrorKind::Other, "Receive error"))
//         }
//     }
// }


