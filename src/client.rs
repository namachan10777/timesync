use std::{collections::VecDeque, io, net::SocketAddr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, error::SendError},
};
use tracing::{debug, info, trace, warn};

use crate::{server, TimeOffset};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    DelayReq,
}

pub struct Server {
    sock: UdpSocket,
    notify_sender: mpsc::Sender<Notify>,
    mean_window: usize,
}

#[derive(Debug)]
pub enum Notify {
    ChangeOffset(super::TimeOffset),
    InvalidMessageSequence,
    InvalidMessageFormat(rmp_serde::decode::Error),
    Io(io::Error),
}

pub struct Config {
    pub notify_channel_buffer_size: usize,
    pub mean_window: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notify_channel_buffer_size: 1024,
            mean_window: 64,
        }
    }
}

#[derive(Debug)]
enum State {
    Cleared,
    SyncRecieved {
        t1_: DateTime<Utc>,
    },
    FollowUpRecieved {
        t1_: DateTime<Utc>,
        t1: DateTime<Utc>,
        t2: DateTime<Utc>,
    },
}

impl Server {
    pub async fn new(
        config: Config,
        addr: SocketAddr,
    ) -> io::Result<(Self, mpsc::Receiver<Notify>)> {
        let sock = UdpSocket::bind(addr).await?;
        sock.set_broadcast(true)?;
        let (tx, rx) = mpsc::channel(config.notify_channel_buffer_size);
        Ok((
            Self {
                sock,
                notify_sender: tx,
                mean_window: config.mean_window,
            },
            rx,
        ))
    }

    pub async fn serve(self) -> Result<(), SendError<Notify>> {
        let mut buf = Vec::new();
        let mut state = State::Cleared;
        info!("start timesync slave");
        let mut queue = VecDeque::new();
        loop {
            buf.clear();
            buf.resize(1024, 0);
            match self.sock.recv_from(&mut buf).await {
                Ok((size, from)) => {
                    let msg = match rmp_serde::from_slice::<server::Message>(&buf[0..size]) {
                        Ok(msg) => msg,
                        Err(e) => {
                            warn!("invalid data: {}", e);
                            self.notify_sender
                                .send(Notify::InvalidMessageFormat(e))
                                .await?;
                            continue;
                        }
                    };
                    trace!("msg: {:?}, state: {:?}", msg, state);
                    match msg {
                        server::Message::Sync => {
                            trace!("sync");
                            if !matches!(state, State::Cleared) {
                                warn!("invalid message sequence (sync)");
                                self.notify_sender
                                    .send(Notify::InvalidMessageSequence)
                                    .await?;
                                state = State::Cleared;
                            } else {
                                state = State::SyncRecieved { t1_: Utc::now() };
                            }
                        }
                        server::Message::FollowUp(t1) => {
                            if let State::SyncRecieved { t1_ } = state {
                                trace!("folowup master: {}, local: {}", t1, t1_);
                                buf.clear();
                                rmp_serde::encode::write(&mut buf, &Message::DelayReq).unwrap();
                                if let Err(e) = self.sock.send_to(&buf, from).await {
                                    self.notify_sender.send(Notify::Io(e)).await?;
                                }
                                let t2 = Utc::now();
                                state = State::FollowUpRecieved { t1_, t1, t2 };
                            } else {
                                warn!("invalid messsage sequence (followup)");
                                self.notify_sender
                                    .send(Notify::InvalidMessageSequence)
                                    .await?;
                                state = State::Cleared;
                            }
                        }
                        server::Message::DelayResp(t2_) => {
                            if let State::FollowUpRecieved { t1_, t1, t2 } = state {
                                let diff_t1 = TimeOffset::diff(t1_, t1);
                                let diff_t2 = TimeOffset::diff(t2, t2_);
                                let offset = diff_t1 + diff_t2 / 2;

                                debug!("t1  {}", t1);
                                debug!("t1' {}", t1_);
                                debug!("t2  {}", t2);
                                debug!("t2' {}", t2_);

                                debug!("offset: {:?}", offset);

                                queue.push_back(offset);
                                if queue.len() > self.mean_window {
                                    queue.pop_front();
                                }

                                let mut total_offset = TimeOffset::Later(chrono::Duration::zero());
                                for offset in &queue {
                                    total_offset += *offset;
                                }
                                let mean_offset = total_offset / queue.len() as i32;

                                debug!("mean-offset: {:?} / sample: {}", mean_offset, queue.len());

                                self.notify_sender
                                    .send(Notify::ChangeOffset(mean_offset))
                                    .await?;
                            } else {
                                warn!("invalid messsage sequence (delay_resp)");
                                self.notify_sender
                                    .send(Notify::InvalidMessageSequence)
                                    .await?;
                            }
                            state = State::Cleared;
                        }
                    }
                }
                Err(e) => {
                    warn!("general io: {}", e);
                    self.notify_sender.send(Notify::Io(e)).await?;
                }
            }
        }
    }
}
