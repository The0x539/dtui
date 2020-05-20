use cursive::traits::*;
use cursive::Printer;
use std::collections::HashMap;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::vec::Vec2;
use cursive::theme::Effect;
use tokio::sync::mpsc;

#[derive(Default)]
struct Category {
    name: String,
    filters: Vec<(String, u64)>,
    active_filter: Option<String>,
    collapsed: bool,
}

enum ClickResult {
    KeepGoing(usize),
    Collapsed,
    UpdatedFilters,
}

impl Category {
    fn len(&self) -> usize { self.filters.len() }
    
    fn content_width(&self) -> usize {
        let mut w = 2 + self.name.len();
        for (filter, hits) in &self.filters {
            w = w.max(2 + 2 + filter.len() + 1 + hits.to_string().len());
        }
        w
    }

    fn content_height(&self) -> usize {
        if self.collapsed {
            1
        } else {
            1 + self.len()
        }
    }
    
    fn click(&mut self, row: usize) -> ClickResult {
        if row == 0 {
            self.collapsed = !self.collapsed;
            ClickResult::Collapsed
        } else if row < self.content_height() {
            self.active_filter = Some(self.filters[row - 1].0.clone());
            ClickResult::UpdatedFilters
        } else {
            ClickResult::KeepGoing(row - self.content_height())
        }
    }

    fn draw_row(&self, printer: &Printer, row: usize) -> Option<usize> {
        if row == 0 {
            let c = if self.collapsed { '>' } else { 'v' };
            printer.print((0, 0), &format!("{} {}", c, self.name));
            None
        } else if row < self.content_height() {
            let (filter, hits) = &self.filters[row-1];
            let e = if Some(filter) == self.active_filter.as_ref() {
                Effect::Reverse
            } else {
                Effect::Simple
            };
            printer.with_effect(e, |p| p.print((2, 0), &format!("* {} {}", filter, hits)));
            None
        } else {
            Some(row - self.content_height())
        }
    }
}

type Sender = mpsc::Sender<HashMap<String, String>>;

pub(crate) struct FiltersView {
    categories: Vec<Category>,
    filter_updates: Sender,
}

impl FiltersView {
    pub(crate) fn new(filter_tree: HashMap<String, Vec<(String, u64)>>, sender: Sender) -> Self {
        let mut categories = Vec::new();
        for (name, filters) in filter_tree {
            let category = Category { name, filters, ..Default::default() };
            categories.push(category);
        }
        Self {
            categories,
            filter_updates: sender,
        }
    }

    pub fn active_filters(&self) -> HashMap<String, String> {
        self.categories
            .iter()
            .filter(|c| c.active_filter.is_some())
            .map(|c| (c.name.clone(), c.active_filter.clone().unwrap()))
            .filter(|(c, f)| match (c.as_str(), f.as_str()) {
                ("owner", "") => false,
                ("owner", _) => true,
                (_, "All") => false,
                _ => true,
            })
            .collect()
    }
    
    fn update_filters(&mut self) {
        let active_filters = self.active_filters();
        self.filter_updates.try_send(active_filters).unwrap();
    }
    
    // This appears to be broken when scrolling. Ugh.
    fn click(&mut self, mut row: usize) {
        for category in &mut self.categories {
            match category.click(row) {
                ClickResult::KeepGoing(new_row) => row = new_row,
                ClickResult::Collapsed => break,
                ClickResult::UpdatedFilters => {
                    self.update_filters();
                    break;
                }
            }
        }
    }

    fn content_width(&self) -> usize {
        self.categories.iter().map(Category::content_width).max().unwrap_or(50)
    }

    fn content_height(&self) -> usize {
        self.categories.iter().map(Category::content_height).sum()
    }

    fn draw_row(&self, printer: &Printer, mut row: usize) -> bool {
        for category in &self.categories {
            if let Some(new_row) = category.draw_row(&printer, row) {
                row = new_row;
            } else {
                return false;
            }
        }
        return true;
    }
}

impl super::ScrollInner for FiltersView {
    fn draw_row(&self, printer: &Printer, row: usize) {
        self.draw_row(printer, row);
    }
}

impl View for FiltersView {
    fn draw(&self, printer: &Printer) {
        for y in 0..printer.output_size.y {
            let row = printer.content_offset.y + y;
            let printer = &printer
                .offset((0, row))
                .cropped((printer.output_size.x, 1));
            if self.draw_row(printer, row) {
                break;
            }
        }
    }

    fn required_size(&mut self, _: Vec2) -> Vec2 {
        (self.content_width(), self.content_height()).into()
    }

    fn take_focus(&mut self, _: cursive::direction::Direction) -> bool { true }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Mouse { offset, position, event } => match event {
                MouseEvent::Press(MouseButton::Left) => {
                    self.click(position.y.saturating_sub(offset.y));
                    EventResult::Consumed(None)
                },
                _ => EventResult::Ignored,
            },
            _ => EventResult::Ignored,
        }
    }
}
