use cursive::traits::*;
use deluge_rpc::{Session, Query, InfoHash, FilterKey, FilterDict, TorrentState};
use cursive::Printer;
use tokio::sync::{watch, Notify};
use std::sync::{Arc, RwLock};
use cursive::utils::Counter;
use cursive::views::ProgressBar;
use tokio::task::JoinHandle;
use tokio::time;
use fnv::FnvHashMap;
use futures::FutureExt;
use async_trait::async_trait;
use super::thread::ViewThread;
use cursive::view::ViewWrapper;
use crate::menu;
use crate::{Selection, SessionHandle};

use super::table::{TableViewData, TableView};

use crate::util::fmt_bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column { Name, State, Size, Speed }
impl AsRef<str> for Column {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::State => "State",
            Self::Size => "Size",
            Self::Speed => "Speed",
        }
    }
}

impl Default for Column {
    fn default() -> Self { Self::Name }
}

#[derive(Clone, Debug, serde::Deserialize, Query)]
pub(crate) struct Torrent {
    hash: InfoHash,
    name: String,
    state: TorrentState,
    total_size: u64,
    progress: f32,
    upload_payload_rate: u64,
    download_payload_rate: u64,
    label: String,
    owner: String,
    tracker_host: String,
    tracker_status: String,
}

type TorrentDiff = <Torrent as Query>::Diff;

impl Torrent {
    pub fn matches_filters(&self, filters: &FilterDict) -> bool {
        for (key, val) in filters.iter() {
            let cmp_val = match key {
                FilterKey::State if val == "Active" => if self.is_active() {
                    continue;
                } else {
                    return false;
                }

                FilterKey::Tracker if val == "Error" => if self.has_tracker_error() {
                    continue;
                } else {
                    return false;
                }

                FilterKey::State   => self.state.as_str(),
                FilterKey::Owner   => self.owner.as_str(),
                FilterKey::Label   => self.label.as_str(),
                FilterKey::Tracker => self.tracker_host.as_str(),
            };
            if val != cmp_val { return false; }
        }
        true
    }

    pub fn has_tracker_error(&self) -> bool {
        self.tracker_status.starts_with("Error:")
    }

    pub fn is_active(&self) -> bool {
        self.download_payload_rate > 0 || self.upload_payload_rate > 0
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct TorrentsState {
    rows: Vec<InfoHash>,
    torrents: FnvHashMap<InfoHash, Torrent>,
    sort_column: Column,
    descending_sort: bool,
}

impl TableViewData for TorrentsState {
    type Column = Column;
    type RowIndex = InfoHash;
    type RowValue = Torrent;
    type Rows = Vec<InfoHash>;
    impl_table! {
        sort_column = self.sort_column;
        rows = self.rows;
        descending_sort = self.descending_sort;
    }

    fn get_row_value<'a>(&'a self, index: &'a InfoHash) -> &'a Torrent {
        &self.torrents[index]
    }

    fn set_sort_column(&mut self, val: Column) {
        self.sort_column = val;
        self.sort_stable();
    }

    fn set_descending_sort(&mut self, val: bool) {
        if val != self.descending_sort {
            self.rows.reverse();
        }
        self.descending_sort = val;
    }

    fn compare_rows(&self, a: &InfoHash, b: &InfoHash) -> std::cmp::Ordering {
        let (ta, tb) = (&self.torrents[a], &self.torrents[b]);

        let mut ord = match self.sort_column {
            Column::Name => ta.name.cmp(&tb.name).reverse(),
            Column::State => ta.state.cmp(&tb.state),
            Column::Size => ta.total_size.cmp(&tb.total_size),
            Column::Speed => ta.upload_payload_rate.cmp(&tb.upload_payload_rate),
        };

        // If the field used for comparison is identical, fall back to comparing infohashes
        // Arbitrary, but consistent and domain-appropriate.
        ord = ord.then(a.cmp(b));

        if self.descending_sort { ord = ord.reverse(); }

        ord
    }

    fn draw_cell(&self, printer: &Printer, tor: &Torrent, column: Column) {
        match column {
            Column::Name => printer.print((0, 0), &tor.name),
            Column::State => {
                let status = match tor.state {
                    TorrentState::Downloading => "DOWN",
                    TorrentState::Seeding => "SEED",
                    TorrentState::Paused => "PAUSE",
                    TorrentState::Checking => "CHECK",
                    TorrentState::Moving => "MOVE",
                    TorrentState::Allocating => "ALLOC",
                    TorrentState::Error => "ERROR",
                    TorrentState::Queued => "QUEUE",
                };
                let mut buf = ryu::Buffer::new();
                let progress = buf.format_finite(tor.progress);
                // TODO: draw my own damn progress bar
                let status_msg = format!("{} {}%", status, progress);
                ProgressBar::new()
                    .with_value(Counter::new(tor.progress as usize))
                    .with_label(move |_, _| status_msg.to_owned())
                    .draw(printer);
            },
            Column::Size => printer.print((0, 0), &fmt_bytes(tor.total_size)),
            Column::Speed => printer.print((0, 0), &(fmt_bytes(tor.upload_payload_rate) + "/s")),
        };
    }
}

impl TorrentsState {
    fn binary_search(&self, hash: &InfoHash) -> std::result::Result<usize, usize> {
        self.rows.binary_search_by(|hash2| self.compare_rows(hash2, hash))
    }

    fn toggle_visibility(&mut self, hash: InfoHash) {
        match self.binary_search(&hash) {
            Ok(idx) => {
                self.rows.remove(idx);
            },
            Err(idx) => {
                self.rows.insert(idx, hash);
            },
        }
    }
}

pub(crate) struct TorrentsView {
    inner: TableView<TorrentsState>,
    thread: JoinHandle<deluge_rpc::Result<()>>,
}

struct TorrentsViewThread {
    data: Arc<RwLock<TorrentsState>>,
    filters: FilterDict,
    filters_recv: watch::Receiver<FilterDict>,
    filters_notify: Arc<Notify>,
    missed_torrents: Vec<InfoHash>,
    selection: Selection,
    selection_notify: Arc<Notify>,
}

impl TorrentsViewThread {
    fn new(
        data: Arc<RwLock<TorrentsState>>,
        selection: Selection,
        selection_notify: Arc<Notify>,
        filters_recv: watch::Receiver<FilterDict>,
        filters_notify: Arc<Notify>,
    ) -> Self {
        let filters = filters_recv.borrow().clone();
        Self {
            data,
            filters,
            filters_recv,
            filters_notify,
            missed_torrents: Vec::new(),
            selection,
            selection_notify,
        }
    }

    fn apply_delta(&mut self, delta: FnvHashMap<InfoHash, TorrentDiff>) {
        let mut toggled_rows = Vec::new();
        let mut should_sort = false;

        let mut data = self.data.write().unwrap();

        for (hash, diff) in delta {
            let sorting_changed = match data.sort_column {
                Column::Name => diff.name.is_some(),
                Column::State => diff.state.is_some(),
                Column::Size => diff.total_size.is_some(),
                Column::Speed => diff.upload_payload_rate.is_some(),
            };

            if let Some(torrent) = data.torrents.get_mut(&hash) {
                if diff != TorrentDiff::default() {
                    let did_match = torrent.matches_filters(&self.filters);
                    torrent.update(diff);
                    let does_match = torrent.matches_filters(&self.filters);

                    if did_match != does_match {
                        toggled_rows.push(hash);
                    }

                    should_sort |= does_match && sorting_changed;
                }
            } else {
                self.missed_torrents.push(hash);
            }
        }

        for hash in toggled_rows.into_iter() {
            data.toggle_visibility(hash);
        }

        if should_sort {
            data.sort_stable();
        }
    }

    fn replace_filters(&mut self, new_filters: FilterDict) {
        self.filters = new_filters;

        let mut data = self.data.write().unwrap();

        let torrents = std::mem::take(&mut data.torrents);

        let iter = torrents
            .iter()
            .filter_map(|(hash, torrent)| torrent.matches_filters(&self.filters).then_some(*hash));

        data.rows.clear();
        data.rows.extend(iter);
        data.torrents = torrents;

        data.sort_unstable();
    }

    fn add_torrent(&mut self, hash: InfoHash, torrent: Torrent) {
        let mut data = self.data.write().unwrap();

        if let Some(old_torrent) = data.torrents.insert(hash, torrent) {
            // This was actually an update rather than an addition.
            // Toggle visibility if appropriate, then return.

            let did_match = old_torrent.matches_filters(&self.filters);
            let does_match = data.torrents[&hash].matches_filters(&self.filters);

            if did_match != does_match {
                data.toggle_visibility(hash);
            }

            return;
        }

        if data.torrents[&hash].matches_filters(&self.filters) {
            let idx = data
                .binary_search(&hash)
                .expect_err("rows vec contained infohash, but torrents hashmap didn't");

            data.rows.insert(idx, hash);
        }
    }

    fn remove_torrent(&mut self, hash: InfoHash) {
        let mut selection = self.selection.write().unwrap();
        if *selection == Some(hash) {
            *selection = None;
            self.selection_notify.notify();
        }

        let mut data = self.data.write().unwrap();
        let tor = &data.torrents[&hash];

        if tor.matches_filters(&self.filters) {
            data.rows
                .remove_item(&hash)
                .expect("infohash not found in rows despite torrent matching filters");
        }

        data.torrents.remove(&hash);
    }
}

#[async_trait]
impl ViewThread for TorrentsViewThread {
    async fn reload(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let interested = deluge_rpc::events![TorrentAdded, TorrentRemoved, TorrentStateChanged];
        session.set_event_interest(&interested).await?;

        let initial_torrents = session.get_torrents_status::<Torrent>(None).await?;
        // TODO: do this more efficiently
        for (hash, torrent) in initial_torrents.into_iter() {
            self.add_torrent(hash, torrent);
        }

        Ok(())
    }

    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        if let Some(new_filters) = self.filters_recv.recv().now_or_never() {
            self.replace_filters(new_filters.unwrap());
        }

        let delta = session.get_torrents_status_diff::<Torrent>(None).await?;
        self.apply_delta(delta);

        while let Some(hash) = self.missed_torrents.pop() {
            let new_torrent = session.get_torrent_status::<Torrent>(hash).await?;
            self.add_torrent(hash, new_torrent);
        }

        Ok(())
    }

    async fn on_event(
        &mut self,
        session: &Session,
        event: deluge_rpc::Event,
    ) -> deluge_rpc::Result<()> {
        match event {
            deluge_rpc::Event::TorrentAdded(hash, _from_state) => {
                let new_torrent = session.get_torrent_status::<Torrent>(hash).await?;
                self.add_torrent(hash, new_torrent);
            },
            deluge_rpc::Event::TorrentRemoved(hash) => {
                self.remove_torrent(hash);
            },
            deluge_rpc::Event::TorrentStateChanged(hash, state) => {
                let mut delta = FnvHashMap::default();
                let diff = TorrentDiff {
                    state: Some(state),
                    ..TorrentDiff::default()
                };
                delta.insert(hash, diff);
                self.apply_delta(delta);
            },
            _ => (),
        }
        Ok(())
    }

    fn update_notifier(&self) -> Arc<Notify> {
        self.filters_notify.clone()
    }

    fn tick(&self) -> time::Duration { time::Duration::from_secs(1) }
}

impl TorrentsView {
    pub(crate) fn new(
        session_recv: watch::Receiver<SessionHandle>,
        selection: Selection,
        selection_notify: Arc<Notify>,
        filters_recv: watch::Receiver<FilterDict>,
        filters_notify: Arc<Notify>,
    ) -> Self {
        let columns = vec![
            (Column::Name, 30),
            (Column::State, 15),
            (Column::Size, 15),
            (Column::Speed, 15),
        ];
        let selection_clone = Arc::clone(&selection);
        let selection_notify_clone = Arc::clone(&selection_notify);
        let mut inner = TableView::new(columns);
        inner.set_on_selection_change(move |_: &mut _, sel: &InfoHash, _, _| {
            selection_clone.write().unwrap().replace(*sel);
            selection_notify_clone.notify();
            cursive::event::Callback::dummy()
        });
        inner.set_on_right_click(|data: &mut TorrentsState, sel: &InfoHash, position, _| {
            let name = &data.torrents[sel].name;
            menu::torrent_context_menu(*sel, name, position)
        });

        let thread_obj = TorrentsViewThread::new(inner.get_data(), selection, selection_notify, filters_recv, filters_notify);
        let thread = tokio::spawn(thread_obj.run(session_recv));
        Self { inner, thread }
    }

    pub fn take_thread(&mut self) -> JoinHandle<deluge_rpc::Result<()>> {
        let dummy_fut = async { Ok(()) };
        let replacement = tokio::spawn(dummy_fut);
        std::mem::replace(&mut self.thread, replacement)
    }
}

impl ViewWrapper for TorrentsView {
    cursive::wrap_impl!(self.inner: TableView<TorrentsState>);
}
