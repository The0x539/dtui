use cursive::traits::*;
use cursive::Printer;
use fnv::FnvHashMap;
use std::collections::BTreeMap;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::vec::Vec2;
use tokio::sync::mpsc;
use deluge_rpc::{FilterKey, FilterDict};

use super::scroll::ScrollInner;
use super::refresh::Refreshable;

use crate::views::torrents::Update as TorrentsUpdate;
use crate::SessionCommand;
use crate::util::digit_width;
use crate::UpdateSenders;

#[derive(Debug)]
pub enum Update {
    ReplaceTree(FnvHashMap<FilterKey, Vec<(String, u64)>>),
}

struct Category {
    filters: Vec<(String, u64)>,
    collapsed: bool,
}

enum Row {
    Parent(FilterKey),
    Child(FilterKey, usize),
}

pub(crate) struct FiltersView {
    active_filters: FilterDict,
    categories: BTreeMap<FilterKey, Category>,
    update_send: UpdateSenders,
    update_recv: mpsc::Receiver<Update>,
}

impl FiltersView {
    pub(crate) fn new(
        update_send: UpdateSenders,
        update_recv: mpsc::Receiver<Update>,
    ) -> Self {
        Self {
            active_filters: FilterDict::default(),
            categories: BTreeMap::new(),
            update_send,
            update_recv,
        }
    }

    fn replace_tree(&mut self, mut new_tree: FnvHashMap<FilterKey, Vec<(String, u64)>>) {
        let pruned_keys = self.categories
            .keys()
            .filter(|key| !new_tree.contains_key(key))
            .copied()
            .collect::<Vec<FilterKey>>();

        for key in pruned_keys.into_iter() {
            self.categories.remove(&key);
        }

        for (key, category) in self.categories.iter_mut() {
            category.filters = new_tree.remove(key).unwrap();
        }

        for (key, filters) in new_tree.into_iter() {
            self.categories.insert(key, Category { filters, collapsed: false });
        }

        if let Some(owners) = self.categories.get_mut(&FilterKey::Owner) {
            let no_owner = (String::new(), 0);
            if !owners.filters.contains(&no_owner) {
                owners.filters.insert(0, no_owner);
            }
        }
    }

    fn active_filters(&self) -> FilterDict {
        self.active_filters
            .iter()
            .filter(|(key, val)| match (key, val.as_str()) {
                (FilterKey::Owner, "") => false,
                (FilterKey::Owner, "All") => true,
                (_, "All") => false,
                _ => true,
            })
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }
    
    fn update_filters(&mut self) {
        let filters = self.active_filters();

        let update = TorrentsUpdate::NewFilters(filters.clone());
        self.update_send
            .torrents
            .try_send(update)
            .expect("couldn't send new filters to torrents view");

        let cmd = SessionCommand::NewFilters(filters);
        self.update_send
            .session
            .try_send(cmd)
            .expect("couldn't send new filters to session thread");
    }

    fn get_row(&self, mut y: usize) -> Option<Row> {
        for (key, category) in self.categories.iter() {
            if y == 0 {
                return Some(Row::Parent(*key));
            } else {
                y -= 1;
            }

            if category.collapsed {
                continue;
            } else if y < category.filters.len() {
                return Some(Row::Child(*key, y));
            } else {
                y -= category.filters.len();
            }
        }
        None
    }

    fn click(&mut self, y: usize) {
        match self.get_row(y) {
            Some(Row::Parent(key)) => {
                let x = &mut self.categories.get_mut(&key).unwrap().collapsed;
                *x = !*x;
            },
            Some(Row::Child(key, idx)) => {
                let filter = self.categories[&key].filters[idx].0.clone();
                self.active_filters.insert(key, filter);
                self.update_filters();
            },
            None => (),
        }
    }

    fn content_width(&self) -> usize {
        let mut w = 0;
        for (key, category) in self.categories.iter() {
            w = w.max(2 + key.as_str().len());
            for (filter, hits) in category.filters.iter() {
                w = w.max(3 + filter.len() + 1 + digit_width(*hits));
            }
        }
        w
    }

    fn content_height(&self) -> usize {
        let mut h = 0;
        for (_, category) in self.categories.iter() {
            h += 1;
            if !category.collapsed {
                h += category.filters.len();
            }
        }
        h
    }
}

impl Refreshable for FiltersView {
    type Update = Update;

    fn get_receiver(&mut self) -> &mut mpsc::Receiver<Update> {
        &mut self.update_recv
    }

    fn perform_update(&mut self, update: Update) {
        match update {
            Update::ReplaceTree(changes) => {
                self.replace_tree(changes);
            }
        }
    }
}

impl ScrollInner for FiltersView {
    fn draw_row(&self, printer: &Printer, y: usize) {
        match self.get_row(y) {
            Some(Row::Parent(key)) => {
                let c = if self.categories[&key].collapsed {
                    '▸'
                } else {
                    '▾'
                };
                printer.print((0, 0), &format!("{} {}", c, key));
            },
            Some(Row::Child(key, idx)) => {
                let (filter, hits) = &self.categories[&key].filters[idx];
                let c = if self.active_filters.get(&key) == Some(filter) {
                    '●'
                } else {
                    '◌'
                };
                let filter = match (key, filter.as_str()) {
                    (FilterKey::Owner, "") => "All",
                    (FilterKey::Tracker, "") => "No Tracker",
                    (FilterKey::Label, "") => "No Label",
                    (_, s) => s,
                };
                let nspaces = printer.size.x - (3 + filter.len() + digit_width(*hits));
                let spaces = " ".repeat(nspaces);
                printer.print((0, 0), &format!(" {} {}{}{}", c, filter, spaces, hits));
            },
            None => (),
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
