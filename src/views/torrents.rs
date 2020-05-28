use std::collections::HashMap;
use cursive::traits::*;
use deluge_rpc::*;
use crate::Torrent;
use cursive::Printer;
use cursive::vec::Vec2;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::view::ScrollBase;
use tokio::sync::mpsc;
use crate::UpdateSenders;
use cursive::utils::Counter;
use cursive::views::ProgressBar;
use fnv::FnvHashMap;

use super::refresh::Refreshable;

use crate::views::filters::Update as FiltersUpdate;

use crate::util::fmt_bytes;

#[derive(Debug)]
pub(crate) enum Update {
    NewFilters(FilterDict),
    Delta(FnvHashMap<InfoHash, <Torrent as Query>::Diff>),
    TorrentRemoved(InfoHash),
}

pub(crate) struct TorrentsView {
    torrents: FnvHashMap<InfoHash, Torrent>,
    filters: FilterDict,
    rows: Vec<InfoHash>,
    columns: Vec<(Column, usize)>,
    scrollbase: ScrollBase,
    update_recv: mpsc::Receiver<Update>,
    update_send: UpdateSenders,
}

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
        Column::Size => printer.print((0, 0), &fmt_bytes(tor.total_size, "B")),
        Column::Speed => printer.print((0, 0), &fmt_bytes(tor.upload_payload_rate, "B/s")),
    };
}

impl TorrentsView {
    pub(crate) fn new(
        update_send: UpdateSenders,
        update_recv: mpsc::Receiver<Update>,
    ) -> Self {
        let columns = vec![
            (Column::Name, 30),
            (Column::State, 15),
            (Column::Size, 15),
            (Column::Speed, 15),
        ];
        Self {
            torrents: FnvHashMap::default(),
            rows: Vec::new(),
            columns,
            scrollbase: ScrollBase::default(),
            filters: FnvHashMap::default(),
            update_send,
            update_recv,
        }
    }

    fn sort(&mut self) {
        // TODO: choose column and direction
        let rows = &mut self.rows;
        let torrents = &self.torrents;
        rows.sort_by_key(|h| &torrents[h].name);
    }

    fn replace_filters(&mut self, filters: FilterDict) {
        self.rows = self.torrents
            .iter()
            .filter_map(|(k, v)| if v.matches_filters(&filters) { Some(k) } else { None })
            .copied()
            .collect();
        self.sort();

        self.filters = filters;

        self.scrollbase.content_height = self.rows.len();
        self.scrollbase.start_line = 0;
    }

    fn insert_row(&mut self, idx: usize, hash: InfoHash) {
        debug_assert!(self.torrents.contains_key(&hash));
        debug_assert!(self.torrents[&hash].matches_filters(&self.filters));
        self.scrollbase.content_height += 1;
        if idx < self.scrollbase.start_line {
            self.scrollbase.start_line += 1;
        }
        self.rows.insert(idx, hash);
    }

    fn remove_row(&mut self, idx: usize) {
        self.scrollbase.content_height -= 1;
        if idx < self.scrollbase.start_line {
            self.scrollbase.start_line -= 1;
        }
        self.rows.remove(idx);
    }

    fn get_deltas(tor: &Torrent) -> Vec<(FilterKey, &str)> {
        let mut ret = vec![
            (FilterKey::State,   tor.state.as_str()),
            (FilterKey::Owner,   tor.owner.as_str()),
            (FilterKey::Tracker, tor.tracker_host.as_str()),
            (FilterKey::Label,   tor.label.as_str()),

            (FilterKey::State,   "All"),
            (FilterKey::Tracker, "All"),
            (FilterKey::Label,   "All"),
        ];

        if tor.is_active() {
            ret.push((FilterKey::State, "Active"));
        }

        if tor.has_tracker_error() {
            ret.push((FilterKey::Tracker, "Error"));
        }

        ret
    }

    fn add_torrents(&mut self, torrents: Vec<(InfoHash, Torrent)>) {
        let mut delta = HashMap::new();
        for (hash, tor) in torrents.into_iter() {
            debug_assert!(!self.torrents.contains_key(&hash));

            for (key, val) in Self::get_deltas(&tor).into_iter() {
                *delta.entry((key, val.to_owned())).or_insert(0) += 1;
            }

            self.torrents.insert(hash, tor);

            let tor = &self.torrents[&hash];
            if tor.matches_filters(&self.filters) {
                let val = &tor.name;
                let idx = match self.rows.binary_search_by_key(&val, |h| &self.torrents[h].name) {
                    Ok(i) => i, // Found something with the same name. No big deal.
                    Err(i) => i,
                };
                self.insert_row(idx, hash);
            }
        }

        self.update_send
            .filters
            .try_send(FiltersUpdate::UpdateMatches(delta))
            .expect("couldn't send filter delta");
    }

    fn remove_torrent(&mut self, hash: InfoHash) {
        let tor = self.torrents.remove(&hash).expect("Tried to remove nonexistent torrent");

        let mut delta = HashMap::new();
        for (key, val) in Self::get_deltas(&tor).into_iter() {
            delta.insert((key, val.to_owned()), -1);
        }

        if tor.matches_filters(&self.filters) {
            let val = &tor.name;
            let idx = self.rows.binary_search_by_key(&val, |h| &self.torrents[h].name).unwrap();
            self.remove_row(idx);
        }

        self.torrents.remove(&hash);
        self.update_send
            .filters
            .try_send(FiltersUpdate::UpdateMatches(delta))
            .expect("couldn't send filter delta");
    }

    fn apply_delta(&mut self, delta: FnvHashMap<InfoHash, <Torrent as Query>::Diff>) {
        let mut filter_updates = HashMap::new();
        macro_rules! incr {
            ($key:ident[$val:expr] $oper:tt 1) => {
                let key = (FilterKey::$key, String::from($val));
                *filter_updates.entry(key).or_insert(0) $oper 1;
            }
        }

        macro_rules! mv {
            ($key:ident[$old:expr => $new:expr]) => {
                incr!($key[$old] -= 1);
                incr!($key[$new] += 1);
            }
        }

        macro_rules! mv_str {
            ($key:ident, $old:expr, $new:expr) => {
                if let Some(new_val) = $new.take() {
                    if new_val != $old {
                        mv!($key[$old => new_val.clone()]);
                        $old = new_val;
                    }
                }
            }
        }

        let mut new_torrents = Vec::new();

        for (hash, mut diff) in delta {
            if diff == Default::default() {
                continue;
            } else if let Some(mut torrent) = self.torrents.remove(&hash) {

                let did_match = torrent.matches_filters(&self.filters);
                let was_active = torrent.is_active();
                let had_error = torrent.has_tracker_error();

                if let Some(new_state) = diff.state.take() {
                    if new_state != torrent.state {
                        mv!(State[torrent.state.as_str() => new_state.as_str()]);
                        torrent.state = new_state;
                    }
                }
                mv_str!(Owner, torrent.owner, diff.owner);
                mv_str!(Label, torrent.label, diff.label);
                mv_str!(Tracker, torrent.tracker_host, diff.tracker_host);

                torrent.update(diff);

                let does_match = torrent.matches_filters(&self.filters);
                let is_active = torrent.is_active();
                let has_error = torrent.has_tracker_error();

                if is_active && !was_active {
                    incr!(State["Active"] += 1);
                } else if was_active && !is_active {
                    incr!(State["Active"] -= 1);
                }

                if has_error && !had_error {
                    incr!(Tracker["Error"] += 1);
                } else if had_error && !has_error {
                    incr!(Tracker["Error"] -= 1);
                }

                if did_match != does_match {
                    let val = &torrent.name;
                    match self.rows.binary_search_by_key(&val, |h| &self.torrents[h].name) {
                        Ok(idx) => {
                            debug_assert!(did_match && !does_match);
                            self.remove_row(idx);
                        },
                        Err(idx) => {
                            debug_assert!(does_match && !did_match);
                            self.insert_row(idx, hash);
                        },
                    }
                }

                self.torrents.insert(hash, torrent);
            } else {
                // New torrent, so should have all the fields
                // TODO: add a realize() method or something to derived Diffs
                let new_torrent = Torrent {
                    hash: diff.hash.unwrap(),
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

        self.add_torrents(new_torrents);

        if !filter_updates.is_empty() {
            filter_updates.shrink_to_fit();
            self.update_send
                .filters
                .try_send(FiltersUpdate::UpdateMatches(filter_updates))
                .expect("couldn't send filter delta");
        }
    }

    fn draw_header(&self, printer: &Printer) {
        let mut x = 0;
        for (column, width) in &self.columns {
            printer.offset((x, 0)).cropped((*width, 1)).print((0, 0), column.as_ref());
            x += width + 1;
        }
    }

    fn draw_row(&self, printer: &Printer, i: usize) {
        let torrent = &self.torrents[&self.rows[i]];
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

impl Refreshable for TorrentsView {
    type Update = Update;

    fn get_receiver(&mut self) -> &mut mpsc::Receiver<Update> {
        &mut self.update_recv
    }

    fn perform_update(&mut self, update: Update) {
        match update {
            Update::Delta(delta) => self.apply_delta(delta),
            Update::NewFilters(filters) => self.replace_filters(filters),
            Update::TorrentRemoved(hash) => self.remove_torrent(hash),
        }
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
        self.scrollbase.draw(&printer.offset((0, 2)), |p, i| self.draw_row(p, i));
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        constraint
    }

    fn layout(&mut self, constraint: Vec2) {
        self.columns[0].1 = constraint.x - 49;
        self.scrollbase.view_height = constraint.y - 2;
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
