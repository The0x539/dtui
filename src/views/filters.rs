use cursive::traits::*;
use cursive::Printer;
use std::collections::HashMap;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::vec::Vec2;
use tokio::sync::{broadcast, mpsc};
use crate::{SessionCommand, Update};
use futures::executor::block_on;
use deluge_rpc::{FilterKey, FilterDict};
use super::ScrollInner;
use std::convert::TryInto;

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

    fn get_filter(&self) -> &Filter {
        match self {
            Self::Child(filter) => filter,
            _ => panic!("Expected this row to be a child"),
        }
    }

    fn get_filter_mut(&mut self) -> &mut Filter {
        match self {
            Self::Child(filter) => filter,
            _ => panic!("Expected this row to be a child"),
        }
    }

    fn into_filter(self) -> Filter {
        match self {
            Self::Child(filter) => filter,
            _ => panic!("Expected this row to be a child"),
        }
    }
}

pub(crate) struct FiltersView {
    active_filters: FilterDict,
    rows: Vec<Row>,
    commands: mpsc::Sender<SessionCommand>,
    updates: broadcast::Receiver<Update>,
}

impl FiltersView {
    pub(crate) fn new(
        filter_tree: HashMap<FilterKey, Vec<(String, u64)>>,
        commands: mpsc::Sender<SessionCommand>,
        updates: broadcast::Receiver<Update>,
    ) -> Self {
        let mut categories = Vec::with_capacity(filter_tree.len());

        for (key, values) in filter_tree.into_iter() {
            let mut filters = values
                .into_iter()
                .map(|(value, hits)| Filter { key, value, hits: hits.try_into().unwrap() })
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
            updates,
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

    fn get_filter_idx(&mut self, the_key: FilterKey, val: &str) -> usize {
        let mut y = 0;
        while y < self.rows.len() {
            let range = match &mut self.rows[y] {
                Row::CollapsedParent { key, ref mut children } => {
                    if *key != the_key {
                        y += 1;
                        continue;
                    }
                    let idx = match children.binary_search_by_key(&val, |f| f.value.as_str()) {
                        Ok(i) => i,
                        Err(i) => {
                            let filter = Filter { key: *key, value: val.to_string(), hits: 0 };
                            children.insert(i, filter);
                            i
                        },
                    };
                    return idx;
                },
                Row::ExpandedParent { key, n_children: n } => {
                    if *key != the_key {
                        y += 1 + *n;
                        continue;
                    }
                    // This is the only case in which this match block neither returns nor continues.
                    y+1..=y+*n
                },
                Row::Child(_) => panic!("Expected a parent in this position"),
            };

            let idx = match self.rows[range].binary_search_by_key(&val, |r| r.get_filter().value.as_str()) {
                Ok(i) => y+1 + i,
                Err(i) => {
                    let filter = Filter { key: the_key, value: val.to_string(), hits: 0 };
                    self.rows.insert(y+1+i, Row::Child(filter));
                    match &mut self.rows[y] {
                        Row::ExpandedParent { n_children, .. } => *n_children += 1,
                        _ => unreachable!(),
                    }
                    y+1 + i
                },
            };
            return idx;
        }

        // TODO: Result/Option
        panic!("key not found: {}", the_key);
    }

    fn update_filter(&mut self, key: FilterKey, val: &str, incr: i64) {
        let idx = self.get_filter_idx(key, val);
        let filter = self.rows[idx].get_filter_mut();
        // TODO: fail better if decrementing past zero.
        // Probably switch to usize for hit count.
        if incr < 0 {
            filter.hits -= -incr as u64;
        } else {
            filter.hits += incr as u64;
        }
    }

    pub fn perform_update(&mut self, update: Update) {
        match update {
            Update::UpdateMatches(changes) => {
                for ((key, val), incr) in changes.into_iter() {
                    self.update_filter(key, &val, incr);
                }
            }
            Update::Delta(_) => (),
            Update::NewFilters(_) => (),
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
    
    fn click(&mut self, y: usize) {
        self.rows[y] = match self.rows[y].clone() {
            Row::CollapsedParent { key, children } => {
                let n_children = children.len();
                self.rows.splice(y+1..y+1, children.into_iter().map(Row::Child));
                Row::ExpandedParent { key, n_children }
            },
            Row::ExpandedParent { key, n_children } => {
                let children = self.rows.splice(y+1..=y+n_children, std::iter::empty())
                    .map(Row::into_filter)
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
                printer.print((0, 0), &format!("▸ {}", key));
            },
            Row::ExpandedParent { key, .. } => {
                printer.print((0, 0), &format!("▾ {}", key));
            },
            Row::Child(Filter { key, value, hits }) => {
                let bullet = if self.active_filters.get(&key) == Some(value) {
                    '●'
                } else {
                    '◌'
                };
                let value = match (key, value.as_str()) {
                    (FilterKey::Owner, "") => "All",
                    (FilterKey::Tracker, "") => "No Tracker",
                    (FilterKey::Label, "") => "No Label",
                    (_, v) => v,
                };
                let nspaces = printer.size.x - (3 + value.len() + hits.to_string().len());
                let spaces = " ".repeat(nspaces);
                printer.print((0, 0), &format!(" {} {}{}{}", bullet, value, spaces, hits));
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
