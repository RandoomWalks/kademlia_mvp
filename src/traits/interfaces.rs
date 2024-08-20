use std::net::SocketAddr;
use async_trait::async_trait;
use tokio::io::Result;
use std::time::SystemTime;
use tokio::sync::{Mutex, mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender}};

#[async_trait]
pub trait NetworkInterface: Send + Sync {
    async fn bind(&mut self, address: SocketAddr) -> Result<()>;
    async fn send(&self, destination: SocketAddr, data: &[u8]) -> Result<()>;
    async fn receive(&self) -> Result<(SocketAddr, Vec<u8>)>;
}

pub trait TimeProvider: Send + Sync {
    fn now(&self) -> SystemTime;
}


use tokio::net::UdpSocket;

pub struct UdpNetwork {
    pub socket: UdpSocket,
}

#[async_trait]
impl NetworkInterface for UdpNetwork {
    async fn bind(&mut self, address: SocketAddr) -> Result<()> {
        let socket = UdpSocket::bind(address).await?;
        self.socket = socket;
        Ok(())
    }

    async fn send(&self, destination: SocketAddr, data: &[u8]) -> Result<()> {
        self.socket.send_to(data, destination).await?;
        Ok(())
    }

    async fn receive(&self) -> Result<(SocketAddr, Vec<u8>)> {
        let mut buf = vec![0u8; 1024];
        let (len, addr) = self.socket.recv_from(&mut buf).await?;
        buf.truncate(len);
        Ok((addr, buf))
    }
}

pub struct MockNetwork {
    pub sender: UnboundedSender<(SocketAddr, Vec<u8>)>,
    pub receiver: Mutex<UnboundedReceiver<(SocketAddr, Vec<u8>)>>,
}

impl MockNetwork {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded_channel();
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }
}

#[async_trait]
impl NetworkInterface for MockNetwork {
    async fn bind(&mut self, _address: SocketAddr) -> Result<()> {
        // No-op for the mock implementation
        Ok(())
    }

    async fn send(&self, destination: SocketAddr, data: &[u8]) -> Result<()> {
        self.sender.send((destination, data.to_vec())).unwrap();
        Ok(())
    }

    async fn receive(&self) -> Result<(SocketAddr, Vec<u8>)> {
        let mut receiver = self.receiver.lock().await;
        if let Some((addr, data)) = receiver.recv().await {
            Ok((addr, data))
        } else {
            Err(tokio::io::Error::new(tokio::io::ErrorKind::Other, "Receive error"))
        }
    }
}


