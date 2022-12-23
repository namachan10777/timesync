use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
    time::Instant,
};
use tokio::net::UdpSocket;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Sync,
    FollowUp(chrono::DateTime<Utc>),
    DelayResp(chrono::DateTime<Utc>),
}

pub struct Server {
    sock: UdpSocket,
    config: Config,
}

pub struct Config {
    pub sync_duration: std::time::Duration,
    pub target: SocketAddr,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sync_duration: std::time::Duration::from_millis(500),
            target: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(255, 255, 255, 255), 13001)),
        }
    }
}

impl Server {
    pub async fn new(config: Config, addr: SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind(addr).await?;
        sock.set_broadcast(true)?;
        Ok(Self { sock, config })
    }

    pub async fn serve(self) -> io::Result<()> {
        let Self { sock, config } = self;
        let sock = Arc::new(sock);
        let sock_for_sync = sock.clone();
        tokio::spawn(async move {
            let sync_msg = rmp_serde::to_vec(&Message::Sync).unwrap();
            let mut buf = Vec::new();
            let mut elapsed = config.sync_duration;
            loop {
                if elapsed < config.sync_duration {
                    tokio::time::sleep(config.sync_duration - elapsed).await;
                }
                buf.clear();
                let stopwatch = Instant::now();
                if let Err(e) = sock_for_sync.send_to(&sync_msg, config.target).await {
                    warn!("Send sync {}", e);
                }
                let now = Utc::now();
                if let Err(e) = rmp_serde::encode::write(&mut buf, &Message::FollowUp(now)) {
                    warn!("Create FollowUp message: {}", e);
                }
                if let Err(e) = sock_for_sync.send_to(&buf, config.target).await {
                    warn!("Send FollowUp {}", e);
                }
                elapsed = stopwatch.elapsed();
            }
        });

        let mut buf = Vec::new();
        loop {
            buf.clear();
            buf.resize(1024, 0);
            match sock.recv_from(&mut buf).await {
                Ok((size, from)) => {
                    let msg = match rmp_serde::from_slice::<client::Message>(&buf[0..size]) {
                        Ok(msg) => msg,
                        Err(e) => {
                            warn!("Invalid message: {}", e);
                            continue;
                        }
                    };
                    match msg {
                        client::Message::DelayReq => {
                            buf.clear();
                            if let Err(e) =
                                rmp_serde::encode::write(&mut buf, &Message::DelayResp(Utc::now()))
                            {
                                warn!("Create DelayResp: {}", e);
                                continue;
                            }
                            if let Err(e) = sock.send_to(&buf, from).await {
                                warn!("Response DelayReq: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Recv: {}", e);
                }
            }
        }
    }
}
