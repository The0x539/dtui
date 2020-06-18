#![feature(vec_remove_item)]
#![feature(bool_to_option)]
#![feature(option_result_contains)]
#![feature(drain_filter)]
#![feature(trait_alias)]

use deluge_rpc::*;
use tokio::sync::{RwLock as AsyncRwLock, watch, Notify};
use uuid::Uuid;
use cursive::Cursive;
use cursive::traits::*;
use cursive::views::{LinearLayout, Panel};
use cursive::direction::Orientation;
use cursive::menu::MenuTree;
use std::sync::{Arc, RwLock};
use futures::executor::block_on;

pub mod views;
use views::{
    filters::FiltersView,
    torrents::TorrentsView,
    statusbar::StatusBarView,
    tabs::TorrentTabsView,

    scroll::ScrollInner,
};

mod util;
mod themes;
mod form;
mod menu;
mod config;

type Selection = Arc<RwLock<Option<InfoHash>>>;

type SessionHandle = Option<(Uuid, Arc<Session>)>;
struct AppState {
    tx: watch::Sender<SessionHandle>,
    val: SessionHandle,
}
impl AppState {
    fn get(&self) -> &SessionHandle {
        &self.val
    }

    fn set(&mut self, val: SessionHandle) {
        self.val = val;
        self.tx.broadcast(self.val.clone()).unwrap();
    }

    fn replace(&mut self, val: SessionHandle) {
        let old = self.val.take();
        self.val = val;
        self.tx.broadcast(self.val.clone()).unwrap();
        if let Some((_, session)) = old {
            assert_eq!(Arc::strong_count(&session), 1);
            let fut = Arc::try_unwrap(session)
                .unwrap()
                .disconnect();

            block_on(fut).unwrap();
        }
    }
}

#[tokio::main]
async fn main() -> deluge_rpc::Result<()> {
    let (session_send, session_recv) = watch::channel(None);
    let mut app_state = AppState {
        tx: session_send,
        val: None,
    };

    {
        let cfg = config::get_config();
        let cmgr = &cfg.read().unwrap().connection_manager;
        if let Some(id) = cmgr.autoconnect {
            let host = &cmgr.hosts[&id];
            let endpoint = (host.address.as_str(), host.port);

            let mut ses = Session::connect(endpoint).await?;

            let auth_level = ses.login(&host.username, &host.password).await?;
            assert!(auth_level >= AuthLevel::Normal);

            let session = Arc::new(ses);
            app_state.set(Some((id, session.clone())));
        }
    }

    let shutdown = Arc::new(AsyncRwLock::new(()));
    let shutdown_write_handle = shutdown.write().await;

    let (filters_send, filters_recv) = watch::channel(FilterDict::default());
    let filters_notify = Arc::new(Notify::new());

    let selection = Arc::new(RwLock::new(None));
    let selection_notify = Arc::new(Notify::new());

    let torrents = TorrentsView::new(
        session_recv.clone(),
        selection.clone(),
        selection_notify.clone(),
        filters_recv.clone(),
        filters_notify.clone(),
        shutdown.clone(),
    ).with_name("torrents");

    let filters = FiltersView::new(
        session_recv.clone(),
        filters_send,
        filters_recv.clone(),
        filters_notify,
        shutdown.clone(),
    ).with_name("filters").into_scroll_wrapper();

    let status_bar = StatusBarView::new(session_recv.clone(), shutdown.clone())
        .with_name("status");

    let torrents_ui = LinearLayout::new(Orientation::Horizontal)
        .child(Panel::new(filters).title("Filters"))
        .child(Panel::new(torrents).title("Torrents"));

    let torrent_tabs = TorrentTabsView::new(
        session_recv.clone(),
        selection,
        selection_notify,
        shutdown.clone(),
    ).with_name("tabs");

    let main_ui = LinearLayout::new(Orientation::Vertical)
        .child(torrents_ui)
        .child(torrent_tabs)
        .child(status_bar);

    let mut siv = cursive::Cursive::new(|| {
        cursive::backends::crossterm::Backend::init()
            .map(cursive_buffered_backend::BufferedBackend::new)
            .map(Box::new)
            .unwrap()
    });
    siv.set_fps(4);
    siv.set_autohide_menu(false);
    siv.set_theme(themes::dracula());
    siv.set_user_data(app_state);

    siv.add_global_callback('q', Cursive::quit);
    siv.add_global_callback(cursive::event::Key::Esc, |siv| {
        if siv.screen().len() > 1 { siv.pop_layer(); }
    });
    siv.add_global_callback(cursive::event::Event::Refresh, Cursive::clear);

    siv.menubar()
        .add_subtree("File",
            MenuTree::new()
                .leaf("Add torrent", menu::add_torrent_dialog)
                .leaf("Create torrent", |_| ())
                .delimiter()
                .leaf("Quit and shutdown daemon", menu::quit_and_shutdown_daemon)
                .delimiter()
                .leaf("Quit", Cursive::quit))
        .add_subtree("Edit",
            MenuTree::new()
                .leaf("Preferences", |_| ())
                .leaf("Connection Manager", menu::show_connection_manager));

    siv.add_fullscreen_layer(main_ui);
    
    siv.run();

    std::mem::drop(shutdown_write_handle);

    let mut app_state = siv.take_user_data::<AppState>().unwrap();
    app_state.replace(None);

    let torrents_thread = siv.call_on_name("torrents", TorrentsView::take_thread).unwrap();
    let filters_thread = siv.call_on_name("filters", FiltersView::take_thread).unwrap();
    let statusbar_thread = siv.call_on_name("status", StatusBarView::take_thread).unwrap();
    let tabs_thread = siv.call_on_name("tabs", TorrentTabsView::take_thread).unwrap();

    let (
        torrents_result,
        filters_result,
        statusbar_result,
        tabs_result,
    ) = tokio::try_join!(
        torrents_thread,
        filters_thread,
        statusbar_thread,
        tabs_thread,
    ).unwrap();

    torrents_result?;
    filters_result?;
    statusbar_result?;
    tabs_result?;

    Ok(())
}
