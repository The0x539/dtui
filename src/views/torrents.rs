use cursive::traits::*;
use deluge_rpc::*;
use cursive::Printer;
use cursive::vec::Vec2;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::view::ScrollBase;
use tokio::sync::{RwLock as AsyncRwLock, broadcast, watch};
use std::sync::{Arc, RwLock};
use cursive::utils::Counter;
use cursive::views::ProgressBar;
use tokio::task::JoinHandle;
use fnv::FnvHashMap;
use tokio::time;
use async_trait::async_trait;
use super::thread::ViewThread;

use super::table::{TableViewData, TableView};

use crate::util::fmt_bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column { Name, State, Size, Speed }
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
struct Torrent {
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
struct ViewData {
    rows: Vec<InfoHash>,
    torrents: FnvHashMap<InfoHash, Torrent>,
    sort_column: Column,
    descending_sort: bool,
}

impl TableViewData for ViewData {
    type Column = Column;
    type Row = InfoHash;
    type Rows = Vec<InfoHash>;

    fn sort_column(&self) -> Self::Column { self.sort_column }
    fn set_sort_column(&mut self, val: Self::Column) {
        self.sort_column = val;
        self.sort_stable();
    }

    fn descending_sort(&self) -> bool { self.descending_sort }
    fn set_descending_sort(&mut self, val: bool) {
        if val != self.descending_sort {
            self.rows.reverse();
        }
        self.descending_sort = val;
    }

    fn rows(&self) -> &Self::Rows { &self.rows }
    fn rows_mut(&mut self) -> &mut Self::Rows { &mut self.rows }
    fn set_rows(&mut self, val: Self::Rows) { self.rows = val; }

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

    fn draw_cell(&self, printer: &Printer, row: &Self::Row, column: Self::Column) {
        let tor = &self.torrents[row];
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

impl ViewData {
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
    data: Arc<RwLock<ViewData>>,
    columns: Vec<(Column, usize)>,
    scrollbase: ScrollBase,
    selected_send: watch::Sender<Option<InfoHash>>,
    thread: JoinHandle<deluge_rpc::Result<()>>,
    selected: Option<InfoHash>,
}

struct TorrentsViewThread {
    session: Arc<Session>,
    data: Arc<RwLock<ViewData>>,
    filters: FilterDict,
    filters_recv: watch::Receiver<FilterDict>,
    events_recv: broadcast::Receiver<deluge_rpc::Event>,
    missed_torrents: Vec<InfoHash>,
}

impl TorrentsViewThread {
    fn new(
        session: Arc<Session>,
        data: Arc<RwLock<ViewData>>,
        filters_recv: watch::Receiver<FilterDict>,
    ) -> Self {
        let events_recv = session.subscribe_events();
        let filters = filters_recv.borrow().clone();
        Self {
            session,
            data,
            filters,
            filters_recv,
            events_recv,
            missed_torrents: Vec::new(),
        }
    }

    fn apply_delta(&mut self, delta: FnvHashMap<InfoHash, <Torrent as Query>::Diff>) {
        let mut toggled_rows = Vec::new();
        let mut should_sort = false;

        let mut data = self.data.write().unwrap();

        for (hash, diff) in delta.into_iter() {
            let sorting_changed = match data.sort_column {
                Column::Name => diff.name.is_some(),
                Column::State => diff.state.is_some(),
                Column::Size => diff.total_size.is_some(),
                Column::Speed => diff.upload_payload_rate.is_some(),
            };

            if diff == Default::default() {
                continue;
            } else if let Some(torrent) = data.torrents.get_mut(&hash) {
                let did_match = torrent.matches_filters(&self.filters);
                torrent.update(diff);
                let does_match = torrent.matches_filters(&self.filters);

                if did_match != does_match {
                    toggled_rows.push(hash);
                }

                should_sort |= does_match && sorting_changed;
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

        data.rows = data.torrents
            .iter()
            .filter_map(|(hash, torrent)| torrent.matches_filters(&self.filters).then_some(*hash))
            .collect();

        data.sort_unstable();
    }

    async fn add_torrent_by_hash(&mut self, hash: InfoHash) -> deluge_rpc::Result<()> {
        let new_torrent = self.session.get_torrent_status::<Torrent>(hash).await?;
        self.add_torrent(hash, new_torrent);
        Ok(())
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
    async fn init(&mut self) -> deluge_rpc::Result<()> {
        self.session.set_event_interest(&deluge_rpc::events![TorrentAdded, TorrentRemoved]).await?;

        let initial_torrents = self.session.get_torrents_status::<Torrent>(None).await?;
        // TODO: do this more efficiently
        for (hash, torrent) in initial_torrents.into_iter() {
            self.add_torrent(hash, torrent);
        }

        Ok(())
    }

    async fn do_update(&mut self) -> deluge_rpc::Result<()> {
        let deadline = time::Instant::now() + time::Duration::from_secs(1);

        loop {
            enum ToHandle { Filters(FilterDict), Event(deluge_rpc::Event) }

            let filt_fut = self.filters_recv.recv();
            let ev_fut = self.events_recv.recv();

            let to_handle = tokio::select! {
                new_filters = filt_fut => ToHandle::Filters(new_filters.unwrap()),
                event = ev_fut => ToHandle::Event(event.unwrap()),
                _ = time::delay_until(deadline) => break,
            };

            match to_handle {
                ToHandle::Filters(new_filters) => self.replace_filters(new_filters),
                ToHandle::Event(event) => match event {
                    deluge_rpc::Event::TorrentAdded(hash, _from_state) => {
                        self.add_torrent_by_hash(hash).await?;
                    },
                    deluge_rpc::Event::TorrentRemoved(hash) => {
                        self.remove_torrent(hash);
                    },
                    _ => (),
                }
            }
        }

        let delta = self.session.get_torrents_status_diff::<Torrent>(Some(&self.filters)).await?;
        self.apply_delta(delta);

        while let Some(hash) = self.missed_torrents.pop() {
            self.add_torrent_by_hash(hash).await?;
        }

        Ok(())
    }
}

impl TorrentsView {
    pub(crate) fn new(
        session: Arc<Session>,
        selected_send: watch::Sender<Option<InfoHash>>,
        filters_recv: watch::Receiver<FilterDict>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Self {
        let columns = vec![
            (Column::Name, 30),
            (Column::State, 15),
            (Column::Size, 15),
            (Column::Speed, 15),
        ];
        let data = {
            let data = ViewData {
                descending_sort: true,
                ..ViewData::default()
            };
            Arc::new(RwLock::new(data))
        };
        let thread_obj = TorrentsViewThread::new(session.clone(), data.clone(), filters_recv);
        let thread = tokio::spawn(thread_obj.run(shutdown));
        selected_send.broadcast(None).unwrap();
        Self {
            data,
            columns,
            selected_send,
            selected: None,
            thread,
            scrollbase: ScrollBase::default(),
        }
    }

    fn click_header(&mut self, mut x: usize) {
        for (column, width) in &self.columns {
            if x < *width {
                self.data.write().unwrap().click_column(*column);
                return;
            } else if x == *width {
                // a column separator was clicked
                return;
            }
            x -= width + 1
        }
    }

    pub fn width(&self) -> usize {
        self.columns.iter().map(|(_, w)| w+1).sum::<usize>()
    }

    pub fn take_thread(&mut self) -> JoinHandle<deluge_rpc::Result<()>> {
        let dummy_fut = async { Ok(()) };
        let replacement = tokio::spawn(dummy_fut);
        std::mem::replace(&mut self.thread, replacement)
    }
}

impl View for TorrentsView {
    fn draw(&self, printer: &Printer) {
        let Vec2 { x: w, y: h } = printer.size;

        let data = self.data.read().unwrap();

        let mut x = 0;
        for (column, width) in &self.columns {
            let mut name = String::from(column.as_ref());

            if *column == data.sort_column {
                name.push_str(if data.descending_sort { " v" } else { " ^" });
            }

            printer.cropped((x+width, 1)).print((x, 0), &name);
            printer.print_hline((x, 1), *width, "─");
            x += width;
            if x == w - 1 {
                printer.print((x, 1), "─");
                break;
            }
            printer.print_vline((x, 0), h, "│");
            printer.print((x, 1), "┼");
            x += 1;
        }
        printer.print((0, 1), "╶");

        self.scrollbase.draw(&printer.offset((0, 2)), |p, i| {
            if let Some(hash) = data.rows.get(i) {
                p.with_selection(
                    self.selected.contains(hash),
                    |p| data.draw_row(p, &self.columns, hash),
                );
            }
        });
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        constraint
    }

    fn layout(&mut self, constraint: Vec2) {
        self.columns[0].1 = constraint.x - 49;

        let sb = &mut self.scrollbase;
        sb.view_height = constraint.y - 2;
        sb.content_height = self.data.read().unwrap().rows.len();
        sb.start_line = sb.start_line.min(sb.content_height.saturating_sub(sb.view_height));
    }

    fn take_focus(&mut self, _: cursive::direction::Direction) -> bool { true }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Mouse { offset, position, event } => match event {
                MouseEvent::WheelUp => {
                    self.scrollbase.scroll_up(1);
                    EventResult::Consumed(None)
                },
                MouseEvent::WheelDown => {
                    self.scrollbase.scroll_down(1);
                    EventResult::Consumed(None)
                },
                MouseEvent::Press(MouseButton::Left)=> {
                    let mut pos = position.saturating_sub(offset);

                    if pos.y == 0 {
                        self.click_header(pos.x);
                    }

                    pos.y = pos.y.saturating_sub(2);

                    if self.scrollbase.content_height > self.scrollbase.view_height {
                        if self.scrollbase.start_drag(pos, self.width()) {
                            return EventResult::Consumed(None);
                        }
                    }

                    if pos.y < self.scrollbase.view_height {
                        let i = pos.y + self.scrollbase.start_line;
                        if let Some(hash) = self.data.read().unwrap().rows.get(i) {
                            self.selected = Some(*hash);
                            self.selected_send.broadcast(self.selected).unwrap();
                            return EventResult::Consumed(None);
                        }
                    }

                    EventResult::Ignored
                },
                MouseEvent::Hold(MouseButton::Left) => {
                    let mut pos = position.saturating_sub(offset);
                    pos.y = pos.y.saturating_sub(2);
                    self.scrollbase.drag(pos);
                    EventResult::Consumed(None)
                },
                MouseEvent::Release(MouseButton::Left) => {
                    self.scrollbase.release_grab();
                    EventResult::Consumed(None)
                }
                _ => EventResult::Ignored,
            },
            _ => EventResult::Ignored,
        }
    }
}
