use cursive::traits::*;
use deluge_rpc::*;
use crate::Torrent;
use cursive::Printer;
use cursive::vec::Vec2;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::view::ScrollBase;
use tokio::sync::{broadcast, watch};
use std::sync::Arc;
use cursive::utils::Counter;
use cursive::views::ProgressBar;
use dashmap::DashMap;
use tokio::task::JoinHandle;
use fnv::FnvHashMap;

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

pub(crate) struct TorrentsView {
    torrents: Arc<DashMap<InfoHash, Torrent>>,
    rows_recv: watch::Receiver<Vec<InfoHash>>,
    columns: Vec<(Column, usize)>,
    scrollbase: ScrollBase,
    thread: JoinHandle<deluge_rpc::Result<()>>,
}

struct TorrentsViewThread {
    session: Arc<Session>,
    torrents: Arc<DashMap<InfoHash, Torrent>>,
    filters: FilterDict,
    filters_recv: watch::Receiver<FilterDict>,
    events_recv: broadcast::Receiver<deluge_rpc::Event>,
    rows: Vec<InfoHash>,
    rows_send: watch::Sender<Vec<InfoHash>>,
    shutdown: broadcast::Receiver<()>,
}

impl TorrentsViewThread {
    fn new(
        session: Arc<Session>,
        torrents: Arc<DashMap<InfoHash, Torrent>>,
        filters_recv: watch::Receiver<FilterDict>,
        shutdown: broadcast::Receiver<()>,
    ) -> (Self, watch::Receiver<Vec<InfoHash>>) {
        let events_recv = session.subscribe_events();
        let (rows_send, rows_recv) = watch::channel(Vec::new());
        let filters = filters_recv.borrow().clone();
        let obj = Self {
            session,
            torrents,
            filters,
            filters_recv,
            events_recv,
            rows: Vec::new(),
            rows_send,
            shutdown,
        };
        (obj, rows_recv)
    }

    async fn run(mut self) -> deluge_rpc::Result<()> {
        self.session.set_event_interest(&deluge_rpc::events![TorrentRemoved]).await?;
        loop {
            tokio::select! {
                event = self.events_recv.recv() => {
                    match event.unwrap() {
                        deluge_rpc::Event::TorrentRemoved(hash) => {
                            self.remove_torrent(hash);
                        },
                        _ => (),
                    }
                },
                new_filters = self.filters_recv.recv() => {
                    self.filters = new_filters.unwrap();
                },
                _ = self.shutdown.recv() => return Ok(()),
                _ = tokio::time::delay_for(tokio::time::Duration::from_secs(5)) => (),
            }
            let delta = self.session.get_torrents_status_diff::<Torrent>(Some(&self.filters)).await?;
            self.apply_delta(delta)
        }
    }

    fn apply_delta(&mut self, delta: FnvHashMap<InfoHash, <Torrent as Query>::Diff>) {
        let mut new_torrents = Vec::new();
        let mut toggled_rows = Vec::new();

        for (hash, diff) in delta.into_iter() {
            if diff == Default::default() {
                continue;
            } else if self.torrents.contains_key(&hash) {
                let toggled_rows = &mut toggled_rows;
                self.torrents.update(&hash, |_, torrent| {
                    let mut torrent = torrent.clone();

                    let did_match = torrent.matches_filters(&self.filters);
                    torrent.update(diff.clone()); // WHY DO I NEED TO CLONE HERE
                    let does_match = torrent.matches_filters(&self.filters);

                    if did_match != does_match {
                        toggled_rows.push(hash);
                    }

                    torrent
                });
            } else {
                // New torrent, so should have all the fields
                // TODO: add a realize() method or something to derived Diffs
                let new_torrent = Torrent {
                    hash: diff.hash.unwrap_or(hash),
                    name: diff.name.unwrap(),
                    state: diff.state.unwrap(),
                    total_size: diff.total_size.unwrap(),
                    progress: diff.progress.unwrap(),
                    upload_payload_rate: diff.upload_payload_rate.unwrap(),
                    download_payload_rate: diff.download_payload_rate.unwrap(),
                    label: diff.label.unwrap(),
                    owner: diff.owner.unwrap(),
                    tracker_host: diff.tracker_host.unwrap(),
                    tracker_status: diff.tracker_status.unwrap(),
                };
                new_torrents.push((hash, new_torrent));
            }
        }

        for hash in toggled_rows.into_iter() {
            let val = self.torrents.get(&hash).unwrap().name.clone();
            match self.rows.binary_search_by(|b| self.torrents.get(b).unwrap().name.cmp(&val)) {
                Ok(idx) => {
                    self.rows.remove(idx);
                },
                Err(idx) => {
                    self.rows.insert(idx, hash);
                },
            }
        }

        self.add_torrents(new_torrents);

        self.rows_send.broadcast(self.rows.clone()).unwrap();
    }
    
    fn add_torrents(&mut self, torrents: Vec<(InfoHash, Torrent)>) {
        for (hash, torrent) in torrents.into_iter() {
            let guard = self.torrents.insert_and_get(hash, torrent);
            let val = guard.value().name.clone();

            let idx = match self.rows.binary_search_by(|b| self.torrents.get(b).unwrap().name.cmp(&val)) {
                Ok(i) => i, // Found something with the same name. No big deal.
                Err(i) => i,
            };
            self.rows.insert(idx, hash);
        }
    }

    fn remove_torrent(&mut self, hash: InfoHash) {
        let guard = self.torrents
            .remove_take(&hash)
            .expect("Tried to remove nonexistent torrent");

        let tor = guard.value();

        if tor.matches_filters(&self.filters) {
            let val = &tor.name;
            let idx = self.rows.binary_search_by(|b| self.torrents.get(b).unwrap().name.cmp(&val)).unwrap();
            self.rows.remove(idx);
        }

        self.torrents.remove(&hash);
    }
}

impl TorrentsView {
    pub(crate) fn new(
        session: Arc<Session>,
        filters_recv: watch::Receiver<FilterDict>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        let columns = vec![
            (Column::Name, 30),
            (Column::State, 15),
            (Column::Size, 15),
            (Column::Speed, 15),
        ];
        let torrents = Arc::new(DashMap::new());
        let (thread_obj, rows_recv) = TorrentsViewThread::new(session.clone(), torrents.clone(), filters_recv, shutdown);
        let thread = tokio::spawn(thread_obj.run());
        Self {
            torrents,
            columns,
            rows_recv,
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
        let rows = self.rows_recv.borrow();
        self.scrollbase.draw(&printer.offset((0, 2)), |p, i| {
            let hash = &rows[i];
            self.draw_row(p, self.torrents.get(hash).unwrap().value());
        });
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        constraint
    }

    fn layout(&mut self, constraint: Vec2) {
        self.columns[0].1 = constraint.x - 49;
        self.scrollbase.view_height = constraint.y - 2;
        // TODO: fix this obvious race condition
        // what if a row gets inserted between layout and draw
        // we need a lock of some sort
        self.scrollbase.content_height = self.rows_recv.borrow().len();
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
