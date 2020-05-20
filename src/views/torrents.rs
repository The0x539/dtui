use std::collections::HashMap;
use cursive::traits::*;
use deluge_rpc::*;
use crate::Torrent;
use cursive::Printer;
use cursive::vec::Vec2;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::view::ScrollBase;
use std::cell::Cell;

#[derive(Debug)]
pub(crate) struct TorrentsView {
    torrents: HashMap<InfoHash, Torrent>,
    rows: Vec<InfoHash>,
    columns: Vec<(Column, usize)>,
    scrollbase: ScrollBase,
    // Don't trust the offset provided by on_event because of a bug in Mux
    offset: Cell<Vec2>,
}

#[derive(Debug, Copy, Clone)]
enum Column { Name, State, Size, Progress }
impl std::fmt::Display for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

fn cell(tor: &Torrent, col: Column) -> String {
    match col {
        Column::Name => tor.name.clone(),
        Column::State => tor.state.to_string(),
        Column::Size => tor.total_size.to_string(),
        Column::Progress => format!("{:.2}%", tor.progress),
    }
}

impl TorrentsView {
    pub(crate) fn new(torrents: HashMap<InfoHash, Torrent>) -> Self {
        let mut rows: Vec<InfoHash> = torrents.keys().copied().collect();
        rows.sort_by(|a, b| torrents[a].name.cmp(&torrents[b].name));
        let columns = vec![
            (Column::Name, 30),
            (Column::State, 15),
            (Column::Size, 15),
            (Column::Progress, 15),
        ];
        let scrollbase = ScrollBase::new();
        let offset = Cell::new(Vec2::zero());
        Self { torrents, rows, columns, scrollbase, offset }
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
            printer.offset((x, 0)).cropped((*width, 1)).print((0, 0), &cell(torrent, *column));
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
            printer.print_hline((x, 1), *width, "-");
            x += width;
            if x == w - 1 {
                printer.print((x, 1), "-");
                break;
            }
            printer.print_vline((x, 0), h, "|");
            printer.print((x, 1), "+");
            x += 1;
        }
        self.draw_header(printer);
        self.scrollbase.draw(&printer.offset((0, 2)), |p, i| self.draw_row(p, i));
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        constraint
    }

    fn layout(&mut self, constraint: Vec2) {
        self.columns[0].1 = constraint.x - 49;
        self.scrollbase.set_heights(constraint.y - 2, self.rows.len());
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
                MouseEvent::Press(MouseButton::Left) => {
                    let mut pos = position.saturating_sub(self.offset.get());
                    pos.y = pos.y.saturating_sub(2);
                    self.scrollbase.start_drag(pos, self.width());
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
