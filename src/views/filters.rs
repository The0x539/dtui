use cursive::traits::*;
use cursive::Printer;
use std::collections::HashMap;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::vec::Vec2;
use cursive::theme::Effect;
use tokio::sync::mpsc;
use crate::SessionCommand;
use futures::executor::block_on;
use deluge_rpc::{FilterKey, FilterDict};
use super::ScrollInner;

type Sender = mpsc::Sender<SessionCommand>;

#[derive(Clone)]
struct Filter {
    key: FilterKey,
    value: String,
    hits: u64,
}

impl Filter {
    fn width(&self) -> usize {
        4 + self.value.len() + 1 + self.hits.to_string().len()
    }
}

#[derive(Clone)]
enum Row {
    CollapsedParent {
        key: FilterKey,
        children: Vec<Filter>,
    },
    ExpandedParent {
        key: FilterKey,
        n_children: usize,
    },
    Child(Filter),
}

impl Row {
    fn width(&self) -> usize {
        match self {
            Self::CollapsedParent { key, children } => {
                children
                    .iter()
                    .map(Filter::width)
                    .max()
                    .unwrap_or(0)
                    .max(2 + key.to_string().len())
            },
            Self::ExpandedParent { key, .. } => {
                2 + key.to_string().len()
            },
            Self::Child(filter) => {
                filter.width()
            },
        }
    }
}

pub(crate) struct FiltersView {
    active_filters: FilterDict,
    rows: Vec<Row>,
    commands: Sender,
}

impl FiltersView {
    pub(crate) fn new(filter_tree: HashMap<FilterKey, Vec<(String, u64)>>, commands: Sender) -> Self {
        let mut categories = Vec::with_capacity(filter_tree.len());

        for (key, values) in filter_tree.into_iter() {
            let mut filters = values
                .into_iter()
                .map(|(value, hits)| Filter { key, value, hits })
                .collect::<Vec<Filter>>();
            filters.sort_unstable_by_key(|f| f.value.clone());
            categories.push((key, filters));
        }

        categories.sort_unstable_by_key(|c| c.0);

        let rows = categories
            .into_iter()
            .flat_map(|(key, filters)| {
                let parent = Row::ExpandedParent {
                    key,
                    n_children: filters.len(),
                };
                let children = filters.into_iter().map(Row::Child);
                std::iter::once(parent).chain(children)
            })
            .collect();

        Self {
            active_filters: FilterDict::new(),
            rows,
            commands,
        }
    }

    fn active_filters(&self) -> FilterDict {
        self.active_filters
            .iter()
            .filter(|(key, val)| match (key, val.as_ref()) {
                (FilterKey::Owner, "") => false,
                (FilterKey::Owner, "All") => true,
                (_, "All") => false,
                _ => true,
            })
            .map(|(key, val)| (*key, val.clone()))
            .collect()
    }
    
    fn update_filters(&mut self) {
        let cmd = SessionCommand::NewFilters(self.active_filters());
        block_on(self.commands.send(cmd)).expect("command channel closed");
    }
    
    fn click(&mut self, y: usize) {
        self.rows[y] = match self.rows[y].clone() {
            Row::CollapsedParent { key, children } => {
                let n_children = children.len();
                self.rows.splice(y+1..y+1, children.into_iter().map(Row::Child));
                Row::ExpandedParent { key, n_children }
            },
            Row::ExpandedParent { key, n_children } => {
                let children = self.rows.splice(y+1..=y+n_children, std::iter::empty())
                    .map(|row| match row {
                        Row::Child(filter) => filter,
                        _ => unreachable!("a parent should never attempt to collapse a non-child"),
                    })
                    .collect();
                Row::CollapsedParent { key, children }
            },
            Row::Child(filter) => {
                self.active_filters.insert(filter.key, filter.value.clone());
                self.update_filters();
                Row::Child(filter)
            },
        }
    }

    fn content_width(&self) -> usize {
        self.rows
            .iter()
            .map(Row::width)
            .max()
            .unwrap_or(1)
    }

    fn content_height(&self) -> usize {
        self.rows.len()
    }
}

impl ScrollInner for FiltersView {
    fn draw_row(&self, printer: &Printer, y: usize) {
        if y >= self.rows.len() { return; }

        match &self.rows[y] {
            Row::CollapsedParent { key, .. } => {
                printer.print((0, 0), &format!("> {}", key));
            },
            Row::ExpandedParent { key, .. } => {
                printer.print((0, 0), &format!("v {}", key));
            },
            Row::Child(Filter { key, value, hits }) => {
                let e = if self.active_filters.get(&key) == Some(value) {
                    Effect::Reverse
                } else {
                    Effect::Simple
                };
                printer.with_effect(e, |p| p.print((2, 0), &format!("* {} {}", value, hits)));
            },
        }
    }
}

impl View for FiltersView {
    fn draw(&self, printer: &Printer) {
        for y in 0..printer.output_size.y {
            let row = y + printer.content_offset.y;
            let printer = printer
                .offset((0, row))
                .cropped((printer.output_size.x, 1));
            self.draw_row(&printer, row);
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
