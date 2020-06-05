use std::sync::Arc;
use deluge_rpc::{Session, InfoHash};
use super::thread::ViewThread;
use async_trait::async_trait;
use tokio::sync::{RwLock as AsyncRwLock, watch};
use tokio::time;
use cursive_tabs::TabPanel;
use tokio::task::{self, JoinHandle};
use cursive::traits::*;
use cursive::view::ViewWrapper;
use cursive::align::HAlign;
use cursive::vec::Vec2;
use cursive::views::{
    TextView,
    LinearLayout,
    DummyView,
    TextContent,
};

use crate::views::labeled_checkbox::LabeledCheckbox;

fn column(rows: &[&str], h_align: HAlign) -> (LinearLayout, TextContent) {
    let labels = TextView::new(rows.join("\n")).effect(cursive::theme::Effect::Bold);

    let content = TextContent::new("");
    let values = TextView::new_with_content(content.clone()).h_align(h_align);

    let view = LinearLayout::horizontal()
        .child(labels)
        .child(DummyView.fixed_width(1))
        .child(values);

    (view, content)
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) enum Tab { Status, Details, Options, Files, Peers, Trackers }

impl std::fmt::Display for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[async_trait]
pub(self) trait TabData {
    type V: View;

    fn view() -> (Self::V, Self);

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()>;
}

mod status;
mod details;
mod options;

struct TorrentTabsViewThread {
    session: Arc<Session>,
    selected_recv: watch::Receiver<Option<InfoHash>>,
    active_tab_recv: watch::Receiver<Tab>,
    status_data: status::StatusData,
    details_data: details::DetailsData,
    options_data: options::OptionsData,
}

pub(crate) struct TorrentTabsView {
    view: TabPanel<Tab>,
    active_tab: Tab,
    active_tab_send: watch::Sender<Tab>,
    thread: JoinHandle<deluge_rpc::Result<()>>,

    options_field_names: options::OptionsNames,
    current_options_recv: watch::Receiver<options::OptionsQuery>,
}

#[async_trait]
impl ViewThread for TorrentTabsViewThread {
    async fn do_update(&mut self) -> deluge_rpc::Result<()> {
        let tick = time::Instant::now() + time::Duration::from_secs(1);

        let opt_hash = *self.selected_recv.borrow();

        if let Some(hash) = opt_hash {

            let active_tab = *self.active_tab_recv.borrow();

            match active_tab {
                Tab::Status => self.status_data.update(&self.session, hash),
                Tab::Details => self.details_data.update(&self.session, hash),
                Tab::Options => self.options_data.update(&self.session, hash),
                _ => Box::pin(async { deluge_rpc::Result::Ok(()) }),
            }.await?;
        }

        let new_selection = self.selected_recv.recv();
        let new_active_tab = self.active_tab_recv.recv();
        tokio::select! {
            _ = new_selection => (),
            _ = new_active_tab => (),
            _ = time::delay_until(tick) => (),
        }

        Ok(())
    }
}

impl TorrentTabsView {
    pub(crate) fn new(
        session: Arc<Session>,
        selected_recv: watch::Receiver<Option<InfoHash>>,
        shutdown: Arc<AsyncRwLock<()>>,
    ) -> Self {
        let (status_tab, status_data) = status::StatusData::view();
        let (details_tab, details_data) = details::DetailsData::view();
        let (options_tab, options_data) = options::OptionsData::view();

        let options_field_names = options_data.names.clone();
        let current_options_recv = options_data.current_options_recv.clone();

        let active_tab = Tab::Status;
        let (active_tab_send, active_tab_recv) = watch::channel(active_tab);

        let files_tab = TextView::new("Torrent files (todo)");
        let peers_tab = TextView::new("Torrent peers (todo)");
        let trackers_tab = TextView::new("Torrent trackers (todo)");

        let thread_obj = TorrentTabsViewThread {
            session,
            selected_recv,
            active_tab_recv,
            status_data,
            details_data,
            options_data,
        };
        let thread = task::spawn(thread_obj.run(shutdown));

        let view = TabPanel::new()
            .with_tab(Tab::Status, status_tab)
            .with_tab(Tab::Details, details_tab)
            .with_tab(Tab::Options, options_tab)
            .with_tab(Tab::Files, files_tab)
            .with_tab(Tab::Peers, peers_tab)
            .with_tab(Tab::Trackers, trackers_tab)
            //.with_bar_placement(cursive_tabs::Placement::VerticalLeft)
            .with_active_tab(active_tab).unwrap();

        Self {
            view,
            active_tab,
            active_tab_send,
            thread,
            options_field_names,
            current_options_recv,
        }
    }

    pub fn take_thread(&mut self) -> JoinHandle<deluge_rpc::Result<()>> {
        let dummy_fut = async { Ok(()) };
        let replacement = task::spawn(dummy_fut);
        std::mem::replace(&mut self.thread, replacement)
    }
}

use cursive::event::{Event, EventResult};

impl ViewWrapper for TorrentTabsView {
    cursive::wrap_impl!(self.view: TabPanel<Tab>);

    fn wrap_on_event(&mut self, event: Event) -> EventResult {
        let old_tab = self.active_tab;
        let result = self.view.on_event(event);
        if let Some(new_tab) = self.view.active_tab() {
            if new_tab != old_tab {
                self.active_tab = new_tab;
                self.active_tab_send.broadcast(new_tab).unwrap();
            }
        }

        result
    }

    fn wrap_layout(&mut self, size: Vec2) {
        if self.active_tab == Tab::Options {
            let names = &self.options_field_names;
            let co = self.current_options_recv.borrow().clone();
            let view = &mut self.view;

            view.call_on_name(
                &names.auto_managed,
                |v: &mut LabeledCheckbox| v.set_checked(co.auto_managed),
            ).unwrap();
        }

        self.view.layout(size)
    }
}
