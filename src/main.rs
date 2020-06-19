#![feature(vec_remove_item)]
#![feature(bool_to_option)]
#![feature(option_result_contains)]
#![feature(drain_filter)]
#![feature(trait_alias)]

use cursive::direction::Orientation;
use cursive::menu::MenuTree;
use cursive::traits::*;
use cursive::views::{LinearLayout, Panel};
use cursive::Cursive;
use deluge_rpc::{AuthLevel, FilterDict, InfoHash, Session};
use futures::FutureExt;
use std::sync::{Arc, RwLock};
use tokio::sync::{watch, Barrier, Notify};
use uuid::Uuid;

mod views;
use views::{
    filters::FiltersView, scroll::ScrollInner, statusbar::StatusBarView, tabs::TorrentTabsView,
    torrents::TorrentsView,
};

mod config;
mod form;
mod menu;
mod themes;
mod util;

type Selection = Arc<RwLock<Option<InfoHash>>>;

// App state, torrents, filters, tabs, status bar.
const SESSION_HANDLE_REF_COUNT: usize = 5;

#[derive(Debug, Clone)]
pub(crate) enum SessionHandle {
    Connected {
        id: Uuid,
        session: Arc<Session>,
        barrier: Arc<Barrier>,
    },
    Disconnected,
}
impl SessionHandle {
    fn new(id: Uuid, session: Arc<Session>) -> Self {
        let barrier = Arc::new(Barrier::new(SESSION_HANDLE_REF_COUNT));
        Self::Connected {
            id,
            session,
            barrier,
        }
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

    fn into_both(self) -> Option<(Uuid, Arc<Session>)> {
        match self {
            Self::Connected { id, session, .. } => Some((id, session)),
            Self::Disconnected => None,
        }
    }

    async fn claim(self) -> Option<Session> {
        if let Self::Connected {
            session, barrier, ..
        } = self
        {
            barrier.wait().await;
            assert_eq!(Arc::strong_count(&session), 1);
            Some(Arc::try_unwrap(session).unwrap())
        } else {
            None
        }
    }

    async fn relinquish(self) {
        if let Self::Connected {
            session, barrier, ..
        } = self
        {
            assert_ne!(Arc::strong_count(&session), 1);
            drop(session);
            barrier.wait().await;
        }
    }
}
impl Default for SessionHandle {
    fn default() -> Self {
        Self::Disconnected
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

    async fn replace(&mut self, val: SessionHandle) -> deluge_rpc::Result<()> {
        let old = std::mem::replace(&mut self.val, val);

        self.tx.broadcast(self.val.clone()).unwrap();

        if let Some(session) = old.claim().await {
            session.disconnect().await.map_err(|(_stream, err)| err)?;
        }

        Ok(())
    }

    async fn shutdown(self) -> deluge_rpc::Result<()> {
        let Self { tx, val } = self;

        drop(tx);

        if let Some(session) = val.claim().await {
            session.disconnect().await.map_err(|(_stream, err)| err)?;
        }

        Ok(())
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

    let torrents_ui = LinearLayout::new(Orientation::Horizontal)
        .child(Panel::new(filters).title("Filters"))
        .child(Panel::new(torrents).title("Torrents"));

    let torrent_tabs =
        TorrentTabsView::new(session_recv.clone(), selection, selection_notify).with_name("tabs");

    // No more cloning the receiver after this point.
    // It's important to drop so that we can unwrap the Arc<SessionHandle> on close.
    drop(session_recv);

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

    siv.run();

    let app_state = siv.take_user_data::<AppState>().unwrap();
    let disconnected = app_state.shutdown();

    let hs = (
        siv.call_on_name("torrents", TorrentsView::take_thread)
            .unwrap(),
        siv.call_on_name("filters", FiltersView::take_thread)
            .unwrap(),
        siv.call_on_name("status", StatusBarView::take_thread)
            .unwrap(),
        siv.call_on_name("tabs", TorrentTabsView::take_thread)
            .unwrap(),
    );

    type R = deluge_rpc::Result<()>;
    let threads_done = futures::future::try_join4(hs.0, hs.1, hs.2, hs.3)
        .map(Result::<(R, R, R, R), tokio::task::JoinError>::unwrap)
        .map(|(a, b, c, d)| a.and(b).and(c).and(d));

    tokio::try_join!(threads_done, disconnected)?;

    Ok(())
}
