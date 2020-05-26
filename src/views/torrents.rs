use std::collections::HashMap;
use cursive::traits::*;
use deluge_rpc::*;
use crate::Torrent;
use cursive::Printer;
use cursive::vec::Vec2;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::view::ScrollBase;
use std::cell::Cell;
use tokio::sync::broadcast;
use crate::Update;
use cursive::utils::Counter;
use cursive::views::ProgressBar;
use human_format::{Formatter, Scales};

type Receiver = broadcast::Receiver<Update>;

#[derive(Debug)]
pub(crate) struct TorrentsView {
    torrents: HashMap<InfoHash, Torrent>,
    filters: FilterDict,
    rows: Vec<InfoHash>,
    columns: Vec<(Column, usize)>,
    scrollbase: ScrollBase,
    // Don't trust the offset provided by on_event because of a bug in Mux
    offset: Cell<Vec2>,
    updates: Receiver,
}

#[derive(Debug, Copy, Clone)]
enum Column { Name, State, Size, Speed }
impl std::fmt::Display for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

// TODO: move to a more general scope; this will be useful elsewhere
fn fmt_bytes(amt: u64, units: &str) -> String {
    Formatter::new()
        .with_scales(Scales::Binary())
        .with_units(units)
        .format(amt as f64)
}

fn draw_cell(printer: &Printer, tor: &Torrent, col: Column) {
    let x = match col {
        Column::Name => format!("{} {}", tor.hash, tor.name),
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
            // TODO: draw my own damn progress bar
            let status_msg = format!("{} {:.2}%", status, tor.progress);
            ProgressBar::new()
                .with_value(Counter::new(tor.progress as usize))
                .with_label(move |_, _| status_msg.clone())
                .draw(printer);
            return;
        },
        Column::Size => fmt_bytes(tor.total_size, "B"),
        Column::Speed => fmt_bytes(tor.upload_payload_rate, "B/s"),
    };
    printer.print((0, 0), &x);
}

impl TorrentsView {
    pub(crate) fn new(torrents: HashMap<InfoHash, Torrent>, updates: Receiver) -> Self {
        let rows: Vec<InfoHash> = torrents.keys().copied().collect();
        let columns = vec![
            (Column::Name, 30),
            (Column::State, 15),
            (Column::Size, 15),
            (Column::Speed, 15),
        ];
        let scrollbase = ScrollBase { content_height: rows.len(), ..Default::default() };
        let offset = Cell::new(Vec2::zero());
        let filters = Default::default();
        let mut obj = Self { torrents, rows, columns, scrollbase, offset, filters, updates };
        obj.sort();
        obj
    }

    fn sort(&mut self) {
        // TODO: choose column and direction
        let rows = &mut self.rows;
        let torrents = &self.torrents;
        rows.sort_by_key(|t| &torrents[t].name);
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

    pub fn apply_delta(&mut self, delta: HashMap<InfoHash, <Torrent as Query>::Diff>) {
        for (hash, diff) in delta {
            if self.torrents.contains_key(&hash) {
                self.torrents.get_mut(&hash).unwrap().update(diff);
            }
        }
    }

    pub fn perform_update(&mut self, update: Update) {
        match update {
            Update::Delta(delta) => self.apply_delta(delta),
            Update::NewFilters(filters) => self.replace_filters(filters),
        }
    }

    pub fn refresh(&mut self) {
        loop {
            match self.updates.try_recv() {
                Ok(update) => self.perform_update(update),
                Err(broadcast::TryRecvError::Empty) => break,
                Err(_) => panic!(),
            }
        }
    }

    fn draw_header(&self, printer: &Printer) {
        let mut x = 0;
        for (column, width) in &self.columns {
            printer.offset((x, 0)).cropped((*width, 1)).print((0, 0), &column.to_string());
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

impl View for TorrentsView {
    fn draw(&self, printer: &Printer) {
        self.offset.set(printer.offset);
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
            Event::Mouse { offset: _, position, event } => match event {
                MouseEvent::WheelUp => {
                    self.scrollbase.scroll_up(1);
                    EventResult::Consumed(None)
                },
                MouseEvent::WheelDown => {
                    self.scrollbase.scroll_down(1);
                    EventResult::Consumed(None)
                },
                MouseEvent::Press(MouseButton::Left)=> {
                    let mut pos = position.saturating_sub(self.offset.get());
                    pos.y = pos.y.saturating_sub(2);
                    if self.scrollbase.content_height > self.scrollbase.view_height {
                        self.scrollbase.start_drag(pos, self.width());
                    }
                    EventResult::Consumed(None)
                },
                MouseEvent::Hold(MouseButton::Left) => {
                    let mut pos = position.saturating_sub(self.offset.get());
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
