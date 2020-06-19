use super::thread::ViewThread;
use async_trait::async_trait;
use cursive::align::HAlign;
use cursive::traits::*;
use cursive::vec::Vec2;
use cursive::view::ViewWrapper;
use cursive::views::{
    Button, DummyView, EditView, EnableableView, LinearLayout, Panel, TextContent, TextView,
};
use cursive_tabs::TabPanel;
use deluge_rpc::{InfoHash, Session};
use futures::FutureExt;
use std::sync::{Arc, RwLock};
use tokio::sync::{watch, Notify};
use tokio::task::{self, JoinHandle};

use crate::{Selection, SessionHandle};

use crate::views::{labeled_checkbox::LabeledCheckbox, spin::SpinView};

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
pub(crate) enum Tab {
    Status,
    Details,
    Options,
    Files,
    Peers,
    Trackers,
}

impl std::fmt::Display for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

trait TabData: ViewThread {
    fn set_selection(&mut self, selection: InfoHash);
}

trait BuildableTabData: TabData + Sized {
    type V: View;
    fn view() -> (Self::V, Self);
}

mod details;
pub(crate) mod files;
mod options;
mod peers;
mod status;
mod trackers;

struct TorrentTabsViewThread {
    last_selection: Option<InfoHash>,
    selection: Selection,
    selection_notify: Arc<Notify>,
    active_tab_recv: watch::Receiver<Tab>,
    active_tab: Tab,
    should_reload: bool,

    status_data: status::StatusData,
    details_data: details::DetailsData,
    options_data: options::OptionsData,
    files_data: files::FilesData,
    peers_data: peers::PeersData,
    trackers_data: trackers::TrackersData,
}

pub(crate) struct TorrentTabsView {
    view: TabPanel<Tab>,
    active_tab: Tab,
    active_tab_send: watch::Sender<Tab>,
    thread: JoinHandle<deluge_rpc::Result<()>>,
    // TODO: name all these Notify structs based on who's being notified
    // Right now, they're named based on what's updating, and in this case, that's either of two things.
    thread_notifier: Arc<Notify>,

    options_field_names: options::OptionsNames,
    current_options_recv: watch::Receiver<options::OptionsQuery>,
    pending_options: Arc<RwLock<Option<options::OptionsQuery>>>,
}

impl TorrentTabsViewThread {
    fn get_active_tab(&self) -> &dyn TabData {
        match self.active_tab {
            Tab::Status => &self.status_data,
            Tab::Details => &self.details_data,
            Tab::Options => &self.options_data,
            Tab::Files => &self.files_data,
            Tab::Peers => &self.peers_data,
            Tab::Trackers => &self.trackers_data,
        }
    }

    fn get_active_tab_mut(&mut self) -> &mut dyn TabData {
        match self.active_tab {
            Tab::Status => &mut self.status_data,
            Tab::Details => &mut self.details_data,
            Tab::Options => &mut self.options_data,
            Tab::Files => &mut self.files_data,
            Tab::Peers => &mut self.peers_data,
            Tab::Trackers => &mut self.trackers_data,
        }
    }
}

#[async_trait]
impl ViewThread for TorrentTabsViewThread {
    async fn reload(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        let evs = deluge_rpc::events![TorrentFileRenamed, TorrentFolderRenamed];
        session.set_event_interest(&evs).await?;
        Ok(())
    }

    async fn on_event(
        &mut self,
        session: &Session,
        event: deluge_rpc::Event,
    ) -> deluge_rpc::Result<()> {
        if self.selection.read().unwrap().is_some() {
            self.get_active_tab_mut().on_event(session, event).await?;
        }
        Ok(())
    }

    async fn update(&mut self, session: &Session) -> deluge_rpc::Result<()> {
        {
            let lock = self.selection.read().unwrap();
            if *lock != self.last_selection {
                self.last_selection = *lock;
                self.should_reload = true;
            }
        }

        if let Some(tab) = self.active_tab_recv.recv().now_or_never() {
            self.active_tab = tab.unwrap();
            self.should_reload = true;
        }

        let selection = self.last_selection;
        if self.should_reload {
            self.should_reload = false;
            let tab = self.get_active_tab_mut();
            if let Some(sel) = selection {
                tab.set_selection(sel);
                tab.reload(session).await?;
            } else {
                // TODO
                // tab.clear();
            }
        } else if selection.is_some() {
            self.get_active_tab_mut().update(session).await?;
        }

        Ok(())
    }

    fn update_notifier(&self) -> Arc<Notify> {
        self.selection_notify.clone()
    }

    fn tick(&self) -> tokio::time::Duration {
        self.get_active_tab().tick()
    }
}

impl TorrentTabsView {
    pub(crate) fn new(
        session_recv: watch::Receiver<SessionHandle>,
        selection: Selection,
        selection_notify: Arc<Notify>,
    ) -> Self {
        let (status_tab, status_data) = status::StatusData::view();
        let (details_tab, details_data) = details::DetailsData::view();
        let (options_tab, options_data) = options::OptionsData::view();
        let (files_tab, files_data) = files::FilesData::view();
        let (peers_tab, peers_data) = peers::PeersData::view();
        let (trackers_tab, trackers_data) = trackers::TrackersData::view();

        let options_field_names = options_data.names.clone();
        let current_options_recv = options_data.current_options_recv.clone();
        let pending_options = options_data.pending_options.clone();

        let active_tab = Tab::Status;
        let (active_tab_send, active_tab_recv) = watch::channel(active_tab);

        let thread_notifier = selection_notify.clone();

        let thread_obj = TorrentTabsViewThread {
            last_selection: None,
            selection,
            selection_notify,
            active_tab_recv,
            active_tab,
            should_reload: true,
            status_data,
            details_data,
            options_data,
            files_data,
            peers_data,
            trackers_data,
        };
        let thread = task::spawn(thread_obj.run(session_recv));

        let view = TabPanel::new()
            .with_tab(Tab::Status, status_tab)
            .with_tab(Tab::Details, details_tab)
            .with_tab(Tab::Options, options_tab)
            .with_tab(Tab::Files, files_tab)
            .with_tab(Tab::Peers, peers_tab)
            .with_tab(Tab::Trackers, trackers_tab)
            //.with_bar_placement(cursive_tabs::Placement::VerticalLeft)
            .with_active_tab(active_tab)
            .unwrap();

        Self {
            view,
            active_tab,
            active_tab_send,
            thread,
            thread_notifier,
            options_field_names,
            current_options_recv,
            pending_options,
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
                self.thread_notifier.notify();
            }
        }

        result
    }

    fn wrap_layout(&mut self, size: Vec2) {
        if self.active_tab == Tab::Options {
            if let Some(opts) =
                task::block_in_place(|| self.pending_options.read().unwrap().clone())
            {
                let names = &self.options_field_names;
                let view = &mut self.view;

                view.call_on_name(
                    &names.ratio_limit_panel,
                    |v: &mut EnableableView<Panel<LinearLayout>>| v.set_enabled(opts.stop_at_ratio),
                )
                .unwrap();

                view.call_on_name(&names.move_completed_path, |v: &mut EditView| {
                    v.set_enabled(opts.move_completed)
                })
                .unwrap();

                view.call_on_name(&names.apply_button, Button::enable)
                    .unwrap();

                return;
            } else if let Some(opts) = self.current_options_recv.recv().now_or_never() {
                let opts = opts.unwrap();
                let names = &self.options_field_names;
                let view = &mut self.view;

                // Intentionally ignoring the callbacks returned here.
                // In this case, those callbacks will update the pending options.
                // That is very much what we don't want. We're just tracking updates from the server,
                // so we don't want these updates to be treated like user input.

                use std::ops::RangeFrom;
                type Spin<T> = SpinView<T, RangeFrom<T>>;

                macro_rules! update {
                    ($type:ty, $method:ident($field:ident)) => {{
                        let cb = |v: &mut $type| v.$method(opts.$field);
                        view.call_on_name(&names.$field, cb).unwrap();
                    }};
                }

                update!(Spin<f64>, set_val(max_download_speed));
                update!(Spin<f64>, set_val(max_upload_speed));
                update!(Spin<i64>, set_val(max_connections));
                update!(Spin<i64>, set_val(max_upload_slots));

                update!(LabeledCheckbox, set_checked(auto_managed));
                update!(LabeledCheckbox, set_checked(stop_at_ratio));
                update!(Spin<f64>, set_val(stop_ratio));
                update!(LabeledCheckbox, set_checked(remove_at_ratio));

                update!(LabeledCheckbox, set_checked(shared));
                update!(LabeledCheckbox, set_checked(prioritize_first_last_pieces));
                update!(LabeledCheckbox, set_checked(sequential_download));
                update!(LabeledCheckbox, set_checked(super_seeding));
                update!(LabeledCheckbox, set_checked(move_completed));
                view.call_on_name(&names.move_completed_path, |v: &mut EditView| {
                    v.set_content(&opts.move_completed_path)
                })
                .unwrap();

                // And now for the "secondary" updates.

                view.call_on_name(
                    &names.ratio_limit_panel,
                    |v: &mut EnableableView<Panel<LinearLayout>>| v.set_enabled(opts.stop_at_ratio),
                )
                .unwrap();

                view.call_on_name(&names.move_completed_path, |v: &mut EditView| {
                    v.set_enabled(opts.move_completed)
                })
                .unwrap();

                view.call_on_name(&names.apply_button, Button::disable)
                    .unwrap();
            }
        }

        self.view.layout(size)
    }
}
