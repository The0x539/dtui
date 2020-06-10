use serde::Deserialize;
use std::net::SocketAddr;
use fnv::{FnvHashMap, FnvHashSet};
use deluge_rpc::{Query, InfoHash, Session};
use crate::views::table::{TableViewData, TableView};
use std::cmp::Ordering;
use cursive::Printer;
use std::sync::{Arc, RwLock};
use async_trait::async_trait;
use super::TabData;
use crate::util;

fn stupid_bool<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    u8::deserialize(deserializer).map(|v| v != 0)
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(super) struct Peer {
    client: String,
    country: String,
    down_speed: u64,
    #[serde(rename = "ip")]
    addr: SocketAddr,
    progress: f64,
    #[serde(deserialize_with = "stupid_bool")]
    seed: bool,
    up_speed: u64,
}

#[derive(Debug, Clone, Deserialize, Query)]
struct PeersQuery { peers: Vec<Peer> }

// TODO: stop reimplementing this. I already had a macro for it in deluge-rpc
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Column { Country, IsSeed, Address, Client, Progress, DownSpeed, UpSpeed }
impl AsRef<str> for Column {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::Country => "Country",
            Self::IsSeed => "Seed?",
            Self::Address => "Address",
            Self::Client => "Client",
            Self::Progress => "Progress",
            Self::DownSpeed => "Down Speed",
            Self::UpSpeed => "Up Speed",
        }
    }
}

impl Default for Column { fn default() -> Self { Self::Address } }

// TODO: establish a consistent naming convention for the various view-related structs
#[derive(Default)]
pub(super) struct PeersTableData {
    rows: Vec<SocketAddr>,
    peers: FnvHashMap<SocketAddr, Peer>,
    sort_column: Column,
    descending_sort: bool,
}

impl PeersTableData {
    fn clear(&mut self) {
        self.rows.clear();
        self.peers.clear();
    }

    fn populate(&mut self, peers: Vec<Peer>) {
        self.clear();

        self.rows.reserve(peers.len());
        self.peers.reserve(peers.len());

        for peer in peers.into_iter() {
            self.rows.push(peer.addr);
            self.peers.insert(peer.addr, peer);
        }

        self.sort_unstable();
    }

    fn update(&mut self, peers: Vec<Peer>) {
        self.peers.clear();
        self.peers.reserve(peers.len());

        // TODO: store things more persistently...?
        let old_addrs: FnvHashSet<_> = self.rows.iter().copied().collect();
        let new_addrs: FnvHashSet<_> = peers.iter().map(|peer| peer.addr).collect();

        self.rows.retain(|addr| new_addrs.contains(addr));
        self.rows.extend(new_addrs.difference(&old_addrs));

        for peer in peers.into_iter() {
            self.peers.insert(peer.addr, peer);
        }

        self.sort_stable();
    }
}

impl TableViewData for PeersTableData {
    type Column = Column;
    type RowIndex = SocketAddr;
    type RowValue = Peer;
    type Rows = Vec<SocketAddr>;

    impl_table! {
        sort_column = self.sort_column;
        rows = self.rows;
        descending_sort = self.descending_sort;
    }

    fn get_row_value<'a>(&'a self, addr: &'a SocketAddr) -> &'a Peer {
        &self.peers[addr]
    }

    fn set_sort_column(&mut self, val: Column) {
        self.sort_column = val;
        self.sort_stable();
    }

    fn set_descending_sort(&mut self, val: bool) {
        let old_val = self.descending_sort;
        self.descending_sort = val;
        if val != old_val {
            self.sort_stable();
        }
    }

    fn draw_cell(&self, printer: &Printer, peer: &Peer, col: Column) {
        let speed = |n| util::fmt_bytes(n) + "/s";
        let print = |s| printer.print((0, 0), s);
        match col {
            Column::Country   => print(&peer.country),
            Column::IsSeed    => print(&peer.seed.to_string()),
            Column::Address   => print(&peer.addr.to_string()),
            Column::Client    => print(&peer.client),
            Column::Progress  => print(&peer.progress.to_string()),
            Column::DownSpeed => print(&speed(peer.down_speed)),
            Column::UpSpeed   => print(&speed(peer.up_speed)),
        }
    }

    fn compare_rows(&self, a: &SocketAddr, b: &SocketAddr) -> Ordering {
        let ip_ord = a.ip().cmp(&b.ip());
        let port_ord = a.port().cmp(&b.port());
        let addr_ord = ip_ord.then(port_ord);

        let mut ord = {
            if self.sort_column == Column::Address {
                addr_ord // avoid the hashmap lookup
            } else {
                let (a, b) = (&self.peers[a], &self.peers[b]);

                match self.sort_column {
                    Column::Country => a.country.cmp(&b.country),
                    Column::IsSeed => a.seed.cmp(&b.seed),
                    Column::Address => unreachable!(),
                    Column::Client => a.client.cmp(&b.client),
                    Column::Progress => a.progress.partial_cmp(&b.progress).expect("well-behaved floats"),
                    Column::DownSpeed => a.down_speed.cmp(&b.down_speed),
                    Column::UpSpeed => a.up_speed.cmp(&b.up_speed),
                }
            }
        };

        ord = ord.then(addr_ord);

        if self.descending_sort { ord = ord.reverse(); }

        ord
    }
}

pub(super) struct PeersData {
    state: Arc<RwLock<PeersTableData>>,
    was_empty: bool,
    active_torrent: Option<InfoHash>,
}

#[async_trait]
impl TabData for PeersData {
    type V = TableView<PeersTableData>;

    fn view() -> (Self::V, Self) {
        let columns = vec![
            (Column::Address, 10),
            (Column::Client, 10),
            (Column::Country, 10),
            (Column::IsSeed, 5),
            (Column::Progress, 8),
            (Column::DownSpeed, 10),
            (Column::UpSpeed, 10),
        ];

        let view = TableView::new(columns);
        let state = view.data.clone();
        let data = PeersData { state, active_torrent: None, was_empty: true };

        (view, data)
    }

    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let hash = self.active_torrent.unwrap();

        let query = session.get_torrent_status::<PeersQuery>(hash).await?;

        if query.peers.is_empty() && !self.was_empty {
            self.was_empty = true;
            self.state.write().unwrap().clear();
        } else {
            self.was_empty = false;
            self.state.write().unwrap().update(query.peers);
        }

        Ok(())
    }

    async fn reload(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        self.active_torrent = Some(hash);

        // Get two different locks, so that we can have a moment of empty data.
        // The alternative is a moment of data for the old torrent.
        // I'd like to do this for the other tabs as well.
        self.state.write().unwrap().clear();

        let query = session.get_torrent_status::<PeersQuery>(hash).await?;

        if query.peers.is_empty() {
            self.was_empty = true;
        } else {
            self.was_empty = false;
            self.state.write().unwrap().populate(query.peers);
        }

        Ok(())
    }
}
