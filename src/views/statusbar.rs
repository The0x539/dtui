use cursive::traits::*;
use cursive::Printer;
use crate::util::fmt_bytes;
use std::net::IpAddr;
use serde::Deserialize;
use deluge_rpc::{Session, Query};
use tokio::task::JoinHandle;
use tokio::sync::RwLock as AsyncRwLock;
use std::sync::{Arc, RwLock};
use std::fmt::{Display, Formatter, self};
use super::thread::ViewThread;
use async_trait::async_trait;

#[derive(Default, Debug, Clone, Copy)]
struct StatusBarData {
    num_peers: u64,
    max_peers: Option<u64>,
    download_rate: f64,
    max_download_rate: Option<f64>,
    upload_rate: f64,
    max_upload_rate: Option<f64>,
    protocol_traffic: (f64, f64),
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
        write!(f, " ⇄ {} ", self.num_peers)?;
        if let Some(max_peers) = self.max_peers {
            write!(f, "({}) ", max_peers)?;
        }

        write!(f, " ↓ {} ", fmt_bytes(self.download_rate as u64, "/s"))?;
        if let Some(max_download_rate) = self.max_download_rate {
            write!(f, "({}) ", fmt_bytes(max_download_rate as u64 * 1024, "/s"))?;
        }

        write!(f, " ↑ {} ", fmt_bytes(self.upload_rate as u64, "/s"))?;
        if let Some(max_upload_rate) = self.max_upload_rate {
            write!(f, "({}) ", fmt_bytes(max_upload_rate as u64 * 1024, "/s"))?;
        }

        write!(f, " ⇵ {}:{} B/s ", self.protocol_traffic.0, self.protocol_traffic.1)?;

        write!(f, " 💾 {} ", fmt_bytes(self.free_space, ""))?;

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
    session: Arc<Session>,
    data: Arc<RwLock<StatusBarData>>,
}

impl StatusBarViewThread {
    pub(crate) fn new(
        session: Arc<Session>,
        data: Arc<RwLock<StatusBarData>>,
    ) -> Self {
        Self { session, data }
    }
}

#[async_trait]
impl ViewThread for StatusBarViewThread {
    async fn init(&mut self) -> deluge_rpc::Result<()> {
        Ok(())
    }

    async fn do_update(&mut self) -> deluge_rpc::Result<()> {
        let (status, config, ip, space) = tokio::try_join!(
            self.session.get_session_status::<StatusQuery>(),
            self.session.get_config_values::<ConfigQuery>(),
            self.session.get_external_ip(),
            self.session.get_free_space(None),
        )?;

        let mut data = self.data.write().unwrap();

        data.ip = Some(ip);
        data.free_space = space;

        data.num_peers = status.num_peers_connected;
        data.download_rate = status.payload_download_rate;
        data.upload_rate = status.payload_upload_rate;
        data.dht_nodes = status.dht_nodes;

        data.protocol_traffic.0 = status.download_rate - status.payload_download_rate;
        data.protocol_traffic.0 = status.upload_rate - status.payload_upload_rate;

        macro_rules! positive {
            ($val:expr, $ty:ty) => {
                ($val > 0.into()).then_some($val as $ty)
            }
        }

        data.max_peers = positive!(config.max_connections_global, u64);
        data.max_download_rate = positive!(config.max_download_speed, f64);
        data.max_upload_rate = positive!(config.max_upload_speed, f64);

        Ok(())
    }
}

impl StatusBarView {
    pub fn new(session: Arc<Session>, shutdown: Arc<AsyncRwLock<()>>) -> Self {
        let data = Arc::new(RwLock::new(StatusBarData::default()));
        let thread_obj = StatusBarViewThread::new(session, data.clone());
        let thread = tokio::spawn(thread_obj.run(shutdown));
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
