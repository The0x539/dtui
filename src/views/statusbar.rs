use super::thread::ViewThread;
use crate::util;
use crate::SessionHandle;
use async_trait::async_trait;
use cursive::traits::*;
use cursive::Printer;
use deluge_rpc::{Query, Session};
use serde::Deserialize;
use std::fmt::{self, Display, Formatter};
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time;

#[derive(Default, Debug, Clone, Copy)]
struct StatusBarData {
    num_peers: u64,
    max_peers: Option<u64>,
    download_rate: u64,
    max_download_rate: f64,
    upload_rate: u64,
    max_upload_rate: f64,
    protocol_traffic: (u64, u64),
    free_space: u64,
    ip: Option<IpAddr>,
    dht_nodes: u64,
}

#[derive(Debug, Clone, Deserialize, Query)]
struct StatusQuery {
    #[serde(rename = "peer.num_peers_connected")]
    num_peers_connected: u64,
    download_rate: f64,
    payload_download_rate: f64,
    upload_rate: f64,
    payload_upload_rate: f64,
    #[serde(rename = "dht.dht_nodes")]
    dht_nodes: u64,
}

#[derive(Debug, Clone, Deserialize, Query)]
struct ConfigQuery {
    max_connections_global: i64,
    max_download_speed: f64,
    max_upload_speed: f64,
}

impl Display for StatusBarData {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(" ⇄ ")?;
        f.write_str(&util::fmt_pair(
            util::fmt_bytes,
            self.num_peers,
            self.max_peers,
        ))?;
        f.write_str(" ")?;

        f.write_str(" ↓ ")?;
        f.write_str(&util::fmt_speed_pair(
            self.download_rate,
            self.max_download_rate,
        ))?;
        f.write_str(" ")?;

        f.write_str(" ↑ ")?;
        f.write_str(&util::fmt_speed_pair(
            self.upload_rate,
            self.max_upload_rate,
        ))?;
        f.write_str(" ")?;

        write!(
            f,
            " ⇵ {}:{} B/s ",
            self.protocol_traffic.0, self.protocol_traffic.1
        )?;

        write!(f, " 💾 {} ", util::fmt_bytes(self.free_space))?;

        if let Some(ip) = self.ip {
            write!(f, " IP: {} ", ip)?;
        } else {
            write!(f, " IP: N/A ")?;
        }

        write!(f, " DHT: {}", self.dht_nodes)?;

        Ok(())
    }
}

pub(crate) struct StatusBarView {
    data: Arc<RwLock<StatusBarData>>,
    thread: JoinHandle<deluge_rpc::Result<()>>,
}

struct StatusBarViewThread {
    data: Arc<RwLock<StatusBarData>>,
}

impl StatusBarViewThread {
    pub(crate) fn new(data: Arc<RwLock<StatusBarData>>) -> Self {
        Self { data }
    }
}

#[async_trait]
impl ViewThread for StatusBarViewThread {
    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let (status, config, ip, space) = tokio::try_join!(
            session.get_session_status::<StatusQuery>(),
            session.get_config_values::<ConfigQuery>(),
            session.get_external_ip(),
            session.get_free_space(None),
        )?;

        /* stupid async borrow checker */
        {
            let mut data = self.data.write().unwrap();

            data.ip = Some(ip);
            data.free_space = space;

            data.num_peers = status.num_peers_connected;
            data.download_rate = status.payload_download_rate as u64;
            data.upload_rate = status.payload_upload_rate as u64;
            data.dht_nodes = status.dht_nodes;

            data.protocol_traffic.0 = (status.download_rate - status.payload_download_rate) as u64;
            data.protocol_traffic.0 = (status.upload_rate - status.payload_upload_rate) as u64;

            data.max_peers = match config.max_connections_global {
                n if n > 0 => Some(n as u64),
                _ => None,
            };
            data.max_download_rate = config.max_download_speed;
            data.max_upload_rate = config.max_upload_speed;
        }

        Ok(())
    }

    fn tick(&self) -> time::Duration {
        time::Duration::from_secs(1)
    }
}

impl StatusBarView {
    pub fn new(session_recv: watch::Receiver<SessionHandle>) -> Self {
        let data = Arc::new(RwLock::new(StatusBarData::default()));
        let thread_obj = StatusBarViewThread::new(data.clone());
        let thread = tokio::spawn(thread_obj.run(session_recv));
        Self { data, thread }
    }

    pub fn take_thread(&mut self) -> JoinHandle<deluge_rpc::Result<()>> {
        let dummy_fut = async { Ok(()) };
        let replacement = tokio::spawn(dummy_fut);
        std::mem::replace(&mut self.thread, replacement)
    }
}

impl View for StatusBarView {
    fn draw(&self, printer: &Printer) {
        printer.print((0, 0), &self.data.read().unwrap().to_string());
    }
}
