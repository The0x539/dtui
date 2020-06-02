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
use cursive::views::{
    TextView,
    LinearLayout,
    DummyView,
    TextContent,
};

fn column(rows: &[&str], h_align: HAlign) -> (impl View, TextContent) {
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

mod status;
mod details;

struct TorrentTabsViewThread {
    session: Arc<Session>,
    selected_recv: watch::Receiver<Option<InfoHash>>,
    active_tab_recv: watch::Receiver<Tab>,
    status_data: status::StatusData,
    details_data: details::DetailsData,
}

pub(crate) struct TorrentTabsView {
    view: TabPanel<Tab>,
    active_tab: Tab,
    active_tab_send: watch::Sender<Tab>,
    thread: JoinHandle<deluge_rpc::Result<()>>,
}

#[async_trait]
impl ViewThread for TorrentTabsViewThread {
    async fn do_update(&mut self) -> deluge_rpc::Result<()> {
        let now = time::Instant::now();

        let hash = match *self.selected_recv.borrow() {
            Some(hash) => hash,
            None => return Ok(()),
        };

        let active_tab = *self.active_tab_recv.borrow();

        match active_tab {
            Tab::Status => self.status_data.update(&self.session, hash).await?,
            Tab::Details => self.details_data.update(&self.session, hash).await?,
            _ => (),
        }

        let new_selection = self.selected_recv.recv();
        let new_active_tab = self.active_tab_recv.recv();
        tokio::select! {
            _ = new_selection => (),
            _ = new_active_tab => (),
            _ = time::delay_until(now + time::Duration::from_secs(1)) => (),
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
        let (status_tab, status_data) = status::status();
        let (details_tab, details_data) = details::details();

        let active_tab = Tab::Status;
        let (active_tab_send, active_tab_recv) = watch::channel(active_tab);

        let options_tab = TextView::new("Torrent options (todo)");
        let files_tab = TextView::new("Torrent files (todo)");
        let peers_tab = TextView::new("Torrent peers (todo)");
        let trackers_tab = TextView::new("Torrent trackers (todo)");

        let thread_obj = TorrentTabsViewThread {
            session,
            selected_recv,
            active_tab_recv,
            status_data,
            details_data,
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

        Self { view, active_tab, active_tab_send, thread }
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
}
