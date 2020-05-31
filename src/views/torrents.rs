use cursive::traits::*;
use deluge_rpc::*;
use crate::Torrent;
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

use crate::util::fmt_bytes;

#[derive(Clone, Copy)]
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

fn draw_cell(printer: &Printer, tor: &Torrent, col: Column) {
    match col {
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
        Column::Size => printer.print((0, 0), &fmt_bytes(tor.total_size, "")),
        Column::Speed => printer.print((0, 0), &fmt_bytes(tor.upload_payload_rate, "/s")),
    };
}

#[derive(Debug, Default, Clone)]
struct ViewData {
    rows: Vec<InfoHash>,
    torrents: FnvHashMap<InfoHash, Torrent>,
}

pub(crate) struct TorrentsView {
    data: Arc<RwLock<ViewData>>,
    columns: Vec<(Column, usize)>,
    scrollbase: ScrollBase,
    thread: JoinHandle<deluge_rpc::Result<()>>,
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

        let mut data = self.data.write().unwrap();

        for (hash, diff) in delta.into_iter() {
            if diff == Default::default() {
                continue;
            } else if let Some(torrent) = data.torrents.get_mut(&hash) {
                let did_match = torrent.matches_filters(&self.filters);
                torrent.update(diff);
                let does_match = torrent.matches_filters(&self.filters);

                if did_match != does_match {
                    toggled_rows.push(hash);
                }
            } else {
                self.missed_torrents.push(hash);
            }
        }

        for hash in toggled_rows.into_iter() {
            Self::toggle_visibility(&mut data, hash);
        }
    }

    fn toggle_visibility(data: &mut ViewData, hash: InfoHash) {
        let val = &data.torrents[&hash].name;

        match data.rows.binary_search_by(|b| data.torrents[b].name.cmp(&val)) {
            Ok(idx) => {
                data.rows.remove(idx);
            },
            Err(idx) => {
                data.rows.insert(idx, hash);
            },
        }
    }

    fn replace_filters(&mut self, new_filters: FilterDict) {
        self.filters = new_filters;

        let mut data = self.data.write().unwrap();

        data.rows = data.torrents
            .iter()
            .filter_map(|(hash, torrent)| torrent.matches_filters(&self.filters).then_some(*hash))
            .collect();
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
                Self::toggle_visibility(&mut data, hash);
            }

            return;
        }

        if data.torrents[&hash].matches_filters(&self.filters) {
            let val = &data.torrents[&hash].name;

            let idx = match data.rows.binary_search_by(|b| data.torrents[b].name.cmp(&val)) {
                Ok(i) => i, // Found something with the same name. No big deal.
                Err(i) => i,
            };

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
        let now = time::Instant::now();

        if let Ok(new_filters) = time::timeout_at(now, self.filters_recv.recv()).await {
            self.replace_filters(new_filters.unwrap());
        }

        if let Ok(event) = time::timeout_at(now, self.events_recv.recv()).await {
            match event.unwrap() {
                deluge_rpc::Event::TorrentAdded(hash, _from_state) => {
                    self.add_torrent_by_hash(hash).await?;
                },
                deluge_rpc::Event::TorrentRemoved(hash) => {
                    self.remove_torrent(hash);
                },
                _ => (),
            }
        }

        let delta = self.session.get_torrents_status_diff::<Torrent>(Some(&self.filters)).await?;
        self.apply_delta(delta);

        while let Some(hash) = self.missed_torrents.pop() {
            self.add_torrent_by_hash(hash).await?;
        }

        time::delay_until(now + time::Duration::from_secs(1)).await;

        Ok(())
    }
}

impl TorrentsView {
    pub(crate) fn new(
        session: Arc<Session>,
        filters_recv: watch::Receiver<FilterDict>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Self {
        let columns = vec![
            (Column::Name, 30),
            (Column::State, 15),
            (Column::Size, 15),
            (Column::Speed, 15),
        ];
        let data = Arc::new(RwLock::new(ViewData::default()));
        let thread_obj = TorrentsViewThread::new(session.clone(), data.clone(), filters_recv);
        let thread = tokio::spawn(thread_obj.run(shutdown));
        Self {
            data,
            columns,
            thread,
            scrollbase: ScrollBase::default(),
        }
    }

    fn draw_header(&self, printer: &Printer) {
        let mut x = 0;
        for (column, width) in &self.columns {
            printer.offset((x, 0)).cropped((*width, 1)).print((0, 0), column.as_ref());
            x += width + 1;
        }
    }

    fn draw_row(&self, printer: &Printer, torrent: &Torrent) {
        let mut x = 0;
        for (column, width) in &self.columns {
            draw_cell(&printer.offset((x, 0)).cropped((*width, 1)), torrent, *column);
            x += width + 1;
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
        let mut x = 0;
        for (_column, width) in &self.columns {
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
        self.draw_header(printer);

        let data = self.data.read().unwrap();

        self.scrollbase.draw(&printer.offset((0, 2)), |p, i| {
            if let Some(hash) = data.rows.get(i) {
                self.draw_row(p, &data.torrents[&hash]);
            }
        });
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        constraint
    }

    fn layout(&mut self, constraint: Vec2) {
        self.columns[0].1 = constraint.x - 49;
        self.scrollbase.view_height = constraint.y - 2;
        self.scrollbase.content_height = self.data.read().unwrap().rows.len();
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
                    pos.y = pos.y.saturating_sub(2);
                    if self.scrollbase.content_height > self.scrollbase.view_height {
                        self.scrollbase.start_drag(pos, self.width());
                    }
                    EventResult::Consumed(None)
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
