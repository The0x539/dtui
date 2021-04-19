#![feature(async_closure)]
#![feature(bool_to_option)]
#![feature(option_result_contains)]
#![feature(drain_filter)]
#![feature(trait_alias)]
#![feature(const_fn)]

use cursive::menu::MenuTree;
use cursive::traits::*;
use cursive::views::Panel;
use cursive::Cursive;
use deluge_rpc::{AuthLevel, FilterDict, InfoHash, Session};
use std::sync::{Arc, RwLock};
use tokio::sync::{watch, Notify};
use uuid::Uuid;

#[macro_use]
mod util;

mod views;
use views::{
    filters::FiltersView, scroll::ScrollInner, static_linear_layout::StaticLinearLayout,
    statusbar::StatusBarView, tabs::TorrentTabsView, torrents::TorrentsView,
};

mod config;
mod form;
mod menu;
mod themes;

type Selection = Arc<RwLock<Option<InfoHash>>>;

#[derive(Debug, Clone)]
pub(crate) enum SessionHandle {
    Connected { id: Uuid, session: Arc<Session> },
    Disconnected,
}
impl SessionHandle {
    fn new(id: Uuid, session: Arc<Session>) -> Self {
        Self::Connected { id, session }
    }

    fn get_id(&self) -> Option<Uuid> {
        match self {
            Self::Connected { id, .. } => Some(*id),
            Self::Disconnected => None,
        }
    }

    fn get_session(&self) -> Option<&Arc<Session>> {
        match self {
            Self::Connected { session, .. } => Some(session),
            Self::Disconnected => None,
        }
    }
}

struct AppState {
    tx: watch::Sender<SessionHandle>,
    val: SessionHandle,
}
impl AppState {
    fn get(&self) -> &SessionHandle {
        &self.val
    }

    fn replace(&mut self, val: SessionHandle) {
        self.val = val;
        self.tx.broadcast(self.val.clone()).unwrap();
    }
}

#[tokio::main]
async fn main() -> deluge_rpc::Result<()> {
    let (session_send, session_recv) = watch::channel(SessionHandle::Disconnected);

    {
        let cfg = config::get_config();
        let cmgr = &cfg.read().unwrap().connection_manager;
        if let Some(id) = cmgr.autoconnect {
            let host = &cmgr.hosts[&id];
            let endpoint = (host.address.as_str(), host.port);

            let mut ses = Session::connect(endpoint).await?;

            let auth_level = ses.login(&host.username, &host.password).await?;
            // TODO: be interactive about this
            assert!(auth_level >= AuthLevel::Normal);

            let handle = SessionHandle::new(id, Arc::new(ses));
            session_send.broadcast(handle).unwrap();
        }
    }

    let app_state = AppState {
        tx: session_send,
        val: session_recv.borrow().clone(),
    };

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
    )
    .with_name("torrents");

    let filters = FiltersView::new(
        session_recv.clone(),
        filters_send,
        filters_recv.clone(),
        filters_notify,
    )
    .with_name("filters")
    .into_scroll_wrapper();

    let status_bar = StatusBarView::new(session_recv.clone()).with_name("status");

    let torrents_ui = StaticLinearLayout::horizontal((
        Panel::new(filters).title("Filters"),
        Panel::new(torrents).title("Torrents"),
    ));

    let torrent_tabs =
        TorrentTabsView::new(session_recv.clone(), selection, selection_notify).with_name("tabs");

    // No more cloning the receiver after this point.
    // It's important to drop so that we can unwrap the Arc<SessionHandle> on close.
    drop(session_recv);

    let main_ui = StaticLinearLayout::vertical((torrents_ui, torrent_tabs, status_bar));

    /*
    let mut siv = cursive::Cursive::new(|| {
    });
    */
    let mut siv = cursive::Cursive::new();
    siv.set_fps(4);
    siv.set_autohide_menu(false);
    siv.set_theme(themes::dracula());

    siv.add_global_callback('q', Cursive::quit);
    siv.add_global_callback(cursive::event::Key::Esc, |siv| {
        if siv.screen().len() > 1 {
            siv.pop_layer();
        }
    });
    siv.add_global_callback(cursive::event::Event::Refresh, Cursive::clear);

    siv.menubar()
        .add_subtree(
            "File",
            MenuTree::new()
                .leaf("Add torrent", menu::add_torrent_dialog)
                .leaf("Create torrent", |_| ())
                .delimiter()
                .leaf("Quit and shutdown daemon", menu::quit_and_shutdown_daemon)
                .delimiter()
                .leaf("Quit", Cursive::quit),
        )
        .add_subtree(
            "Edit",
            MenuTree::new()
                .leaf("Preferences", |_| ())
                .leaf("Connection Manager", menu::show_connection_manager),
        );

    siv.add_fullscreen_layer(main_ui);

    siv.set_user_data(app_state);

    siv.run_with(|| {
        cursive::backends::crossterm::Backend::init()
            .map(cursive_buffered_backend::BufferedBackend::new)
            .map(Box::new)
            .expect("Failed to initialize backend")
    });

    Ok(())
}
