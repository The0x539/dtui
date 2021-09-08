use super::thread::ViewThread;
use crate::SessionHandle;
use async_trait::async_trait;
use cursive::event::{Event, EventResult, MouseButton, MouseEvent};
use cursive::traits::*;
use cursive::vec::Vec2;
use cursive::Printer;
use deluge_rpc::{FilterDict, FilterKey, Session};
use fnv::FnvHashMap;
use once_cell::sync::Lazy;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{watch, Notify};

//use super::scroll::ScrollInner;

use crate::util::digit_width;

#[derive(Debug)]
pub(crate) struct Category {
    pub filters: Vec<(String, u64)>,
    pub collapsed: bool,
}

pub(crate) type Categories = BTreeMap<FilterKey, Category>;

enum Row {
    Parent(FilterKey),
    Child(FilterKey, usize),
}

pub(crate) struct FiltersView {
    // TODO: figure out how to remove filters that vanish.
    active_filters: FilterDict,
    categories: &'static RwLock<Categories>,
    filters_send: watch::Sender<FilterDict>,
    filters_notify: Arc<Notify>,
}

pub(crate) static FILTER_CATEGORIES: Lazy<RwLock<Categories>> = Lazy::new(Default::default);

struct FiltersViewThread {
    categories: &'static RwLock<Categories>,
    filters_recv: watch::Receiver<FilterDict>,
    update_notifier: Arc<Notify>,
}

impl FiltersViewThread {
    fn new(
        categories: &'static RwLock<Categories>,
        filters_recv: watch::Receiver<FilterDict>,
    ) -> Self {
        let update_notifier = Arc::new(Notify::new());
        Self {
            categories,
            filters_recv,
            update_notifier,
        }
    }

    fn should_show(&self, key: FilterKey, filter: &(String, u64)) -> bool {
        let (val, hits) = filter;

        if *hits > 0 || false
        /* TODO: "show zero hits" pref */
        {
            true
        } else if self.filters_recv.borrow().get(&key) == Some(val) {
            true
        } else {
            false
        }
    }

    fn replace_tree(&mut self, mut new_tree: FnvHashMap<FilterKey, Vec<(String, u64)>>) {
        let mut categories = self.categories.write().unwrap();

        let pruned_keys = categories
            .keys()
            .filter(|key| !new_tree.contains_key(key))
            .copied()
            .collect::<Vec<FilterKey>>();

        for key in pruned_keys.into_iter() {
            categories.remove(&key);
        }

        for (key, category) in categories.iter_mut() {
            let new_filters = new_tree
                .remove(key)
                .unwrap()
                .into_iter()
                .filter(|filter| self.should_show(*key, filter));

            category.filters.clear();
            category.filters.extend(new_filters);
        }

        for (key, mut filters) in new_tree.into_iter() {
            filters.retain(|filter| self.should_show(key, filter));
            categories.insert(
                key,
                Category {
                    filters,
                    collapsed: false,
                },
            );
        }

        if let Some(owners) = categories.get_mut(&FilterKey::Owner) {
            let no_owner = (String::new(), 0);
            if !owners.filters.contains(&no_owner) {
                owners.filters.insert(0, no_owner);
            }
        }
    }
}

#[async_trait]
impl ViewThread for FiltersViewThread {
    async fn reload(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let interested = deluge_rpc::events![TorrentAdded, TorrentRemoved, TorrentStateChanged];
        session.set_event_interest(&interested).await?;
        Ok(())
    }

    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let new_tree = session.get_filter_tree(true, &[]).await?;
        self.replace_tree(new_tree);
        Ok(())
    }

    async fn on_event(&mut self, _: &Session, event: deluge_rpc::Event) -> deluge_rpc::Result<()> {
        use deluge_rpc::EventKind::*;
        if let TorrentAdded | TorrentRemoved | TorrentStateChanged = event.into() {
            self.update_notifier.notify_one();
        }
        Ok(())
    }

    fn update_notifier(&self) -> Arc<Notify> {
        self.update_notifier.clone()
    }

    fn clear(&mut self) {
        self.replace_tree(Default::default());
    }
}

impl FiltersView {
    pub(crate) fn new(
        session_recv: watch::Receiver<SessionHandle>,
        filters_send: watch::Sender<FilterDict>,
        filters_recv: watch::Receiver<FilterDict>,
        filters_notify: Arc<Notify>,
    ) -> Self {
        let categories = &*FILTER_CATEGORIES;
        let thread_obj = FiltersViewThread::new(categories, filters_recv);
        tokio::spawn(thread_obj.run(session_recv));
        Self {
            active_filters: FilterDict::default(),
            categories,
            filters_send,
            filters_notify,
        }
    }

    fn get_active_filters(&self) -> FilterDict {
        self.active_filters
            .iter()
            .filter(|(key, val)| match (key, val.as_str()) {
                (FilterKey::Owner, "") => false,
                (FilterKey::Owner, "All") => true, // in case All is the name of a user
                (_, "All") => false,
                _ => true,
            })
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    fn get_row(categories: &Categories, mut y: usize) -> Option<Row> {
        for (key, category) in categories.iter() {
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
        let mut categories = self.categories.write().unwrap();

        match Self::get_row(&categories, y) {
            Some(Row::Parent(key)) => {
                let x = &mut categories.get_mut(&key).unwrap().collapsed;
                *x = !*x;
            }
            Some(Row::Child(key, idx)) => {
                let filters = &mut categories.get_mut(&key).unwrap().filters;

                let filter = filters[idx].0.clone();
                let old = self.active_filters.insert(key, filter);

                // Remove the empty category immediately, rather than waiting for the next update.
                // TODO: "show zero hits" pref
                if let Some(val) = old {
                    if (key, val.as_str()) != (FilterKey::Owner, "") {
                        for i in 0..filters.len() {
                            if filters[i].0 == val {
                                if filters[i].1 == 0 {
                                    filters.remove(i);
                                }
                                break;
                            }
                        }
                    }
                }

                let new_dict = self.get_active_filters();
                self.filters_send
                    .send(new_dict)
                    .expect("Couldn't send new view filters");

                self.filters_notify.notify_one();
            }
            None => (),
        }
    }

    fn content_width(categories: &Categories) -> usize {
        let mut w = 0;
        for (key, category) in categories.iter() {
            w = w.max(2 + key.as_str().len());
            for (filter, hits) in category.filters.iter() {
                w = w.max(3 + filter.len() + 1 + digit_width(*hits));
            }
        }
        w
    }

    fn content_height(categories: &Categories) -> usize {
        let mut h = 0;
        for (_, category) in categories.iter() {
            h += 1;
            if !category.collapsed {
                h += category.filters.len();
            }
        }
        h
    }

    fn draw_row(&self, printer: &Printer, y: usize) {
        let categories = self.categories.read().unwrap();

        match Self::get_row(&categories, y) {
            Some(Row::Parent(key)) => {
                let c = if categories[&key].collapsed {
                    '▸'
                } else {
                    '▾'
                };
                printer.print((0, 0), &format!("{} {}", c, key));
            }
            Some(Row::Child(key, idx)) => {
                let (filter, hits) = &categories[&key].filters[idx];
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
                let nspaces = printer
                    .size
                    .x
                    .saturating_sub(3 + filter.len() + digit_width(*hits));
                let spaces = " ".repeat(nspaces);
                printer.print((0, 0), &format!(" {} {}{}{}", c, filter, spaces, hits));
            }
            None => (),
        }
    }
}

impl View for FiltersView {
    fn draw(&self, printer: &Printer) {
        for y in 0..printer.output_size.y {
            let row = y + printer.content_offset.y;
            let printer = printer.offset((0, row)).cropped((printer.output_size.x, 1));
            self.draw_row(&printer, row);
        }
    }

    fn required_size(&mut self, _: Vec2) -> Vec2 {
        let categories = self.categories.read().unwrap();
        (
            Self::content_width(&categories),
            Self::content_height(&categories),
        )
            .into()
    }

    fn take_focus(&mut self, _: cursive::direction::Direction) -> bool {
        true
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Mouse {
                offset,
                position,
                event,
            } => match event {
                MouseEvent::Press(MouseButton::Left) => {
                    self.click(position.y.saturating_sub(offset.y));
                    EventResult::Consumed(None)
                }
                _ => EventResult::Ignored,
            },
            _ => EventResult::Ignored,
        }
    }
}
