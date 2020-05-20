use cursive::traits::*;
use cursive::Printer;
use std::collections::HashMap;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::vec::Vec2;
use cursive::theme::Effect;
use cursive::view::ScrollBase;

#[derive(Default)]
struct Category {
    name: String,
    filters: Vec<(String, u64)>,
    active_filter: Option<String>,
    collapsed: bool,
}

enum ClickResult {
    KeepGoing(usize),
    Collapsed(usize, bool),
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
            ClickResult::Collapsed(self.len(), self.collapsed)
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

#[derive(Default)]
pub(crate) struct FiltersView {
    categories: Vec<Category>,
    scrollbase: ScrollBase,
}

impl FiltersView {
    pub(crate) fn new(filter_tree: HashMap<String, Vec<(String, u64)>>) -> Self {
        let mut categories = Vec::new();
        for (name, filters) in filter_tree {
            let category = Category { name, filters, ..Default::default() };
            categories.push(category);
        }
        let mut obj = Self::default();
        obj.categories = categories;
        obj.scrollbase.content_height = obj.content_height();
        obj
    }

    pub fn active_filters(&self) -> Vec<(String, String)> {
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

    fn click(&mut self, mut row: usize) {
        for category in &mut self.categories {
            match category.click(row) {
                ClickResult::KeepGoing(new_row) => row = new_row,
                ClickResult::Collapsed(delta_h, should_subtract) => {
                    let h = self.scrollbase.content_height;
                    self.scrollbase.content_height = if should_subtract {
                        h.saturating_sub(delta_h)
                    } else {
                        h.saturating_add(delta_h)
                    };
                    break;
                },
                ClickResult::UpdatedFilters => {
                    todo!();
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

    fn draw_row(&self, printer: &Printer, mut row: usize) {
        for category in &self.categories {
            if let Some(new_row) = category.draw_row(&printer, row) {
                row = new_row;
            } else {
                break;
            }
        }
    }
}

impl View for FiltersView {
    fn draw(&self, printer: &Printer) {
        self.scrollbase.draw(printer, |p, r| self.draw_row(p, r));
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2 { x: self.content_width() + 1, y: constraint.y }
    }

    fn layout(&mut self, constraint: Vec2) {
        self.scrollbase.view_height = constraint.y;
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
