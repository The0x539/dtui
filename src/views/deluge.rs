use cursive::traits::*;
use cursive::vec::Vec2;
use cursive::Printer;
use cursive::view::Selector;
use cursive::event::{Event, AnyCb, EventResult};
use cursive::Rect;
use cursive::direction::{Direction, Relative};

use super::*;

enum Focused { Torrents, Filters }

pub(crate) struct DelugeView {
    torrents: TorrentsView,
    torrents_rect: Rect,

    filters: FiltersView,
    filters_rect: Rect,

    focused: Focused,
}

impl DelugeView {
    pub(crate) fn new(torrents: TorrentsView, filters: FiltersView) -> Self {
        Self {
            filters,
            filters_rect: Rect::from_size((1, 1), (1, 1)),

            torrents,
            torrents_rect: Rect::from_size((1, 1), (1, 1)),

            focused: Focused::Torrents,
        }
    }

    fn get_focused_mut(&mut self) -> (&mut dyn View, &mut Rect) {
        match self.focused {
            Focused::Torrents => (&mut self.torrents, &mut self.torrents_rect),
            Focused::Filters => (&mut self.filters, &mut self.filters_rect),
        }
    }
}

const FRONT: Direction = Direction::Rel(Relative::Front);

impl View for DelugeView {
    fn draw(&self, printer: &Printer) {
        self.filters.draw(&printer.offset(self.filters_rect.top_left()).cropped(self.filters_rect.size()));
        self.torrents.draw(&printer.offset(self.torrents_rect.top_left()).cropped(self.torrents_rect.size()));
        printer.print_vline(self.filters_rect.top_right() + (1, 0), printer.output_size.y, "|");
    }

    fn layout(&mut self, constraint: Vec2) {
        let filters_size = self.filters.required_size(constraint);
        let torrents_size = self.torrents.required_size((constraint.x - filters_size.x - 1, constraint.y).into());

        self.filters_rect = Rect::from_size((0, 0), filters_size);
        self.torrents_rect = Rect::from_size((self.filters_rect.right() + 2, 0), torrents_size);

        self.filters.layout(self.filters_rect.size());
        self.torrents.layout(self.torrents_rect.size());
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 { constraint }

    fn call_on_any<'a>(&mut self, selector: &Selector, cb: AnyCb<'a>) {
        self.torrents.call_on_any(selector, cb);
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if let Event::Mouse { offset, position, .. } = event {
            let pos = position.saturating_sub(offset);
            if self.torrents_rect.contains(pos) && self.torrents.take_focus(FRONT) {
                self.focused = Focused::Torrents;
            } else if self.filters_rect.contains(pos) && self.filters.take_focus(FRONT) {
                self.focused = Focused::Filters;
            }
        }

        let (view, rect) = self.get_focused_mut();
        view.on_event(event.relativized(rect.top_left()))
    }
}
