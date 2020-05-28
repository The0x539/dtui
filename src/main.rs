use deluge_rpc::*;
use tokio::sync::{broadcast, mpsc};
use cursive::event::Event;
use cursive::Cursive;
use cursive::traits::*;
use cursive::views::{LinearLayout, TextView, Panel};
use cursive::direction::Orientation;
use cursive::menu::MenuTree;
use cursive_tabs::TabPanel;
use std::sync::Arc;

pub mod views;
use views::{
    filters::FiltersView,
    torrents::TorrentsView,

    scroll::ScrollInner,
    refresh::Refreshable,

    filters::Update as FiltersUpdate,
    torrents::Update as TorrentsUpdate,
};

pub mod util;

mod themes;
mod menu;

#[derive(Debug)]
pub enum SessionCommand {
    AddTorrentUrl(String),
    Shutdown,
}

#[derive(Debug)]
pub enum SessionUpdate {
    NewFilters(FilterDict),
}

#[derive(Clone, Debug)]
pub(crate) struct UpdateSenders {
    pub filters: mpsc::Sender<FiltersUpdate>,
    pub torrents: mpsc::Sender<TorrentsUpdate>,
    pub session_updates: mpsc::Sender<SessionUpdate>,
}

#[derive(Clone, Debug, serde::Deserialize, Query)]
struct Torrent {
    hash: InfoHash,
    name: String,
    state: TorrentState,
    total_size: u64,
    progress: f32,
    upload_payload_rate: u64,
    download_payload_rate: u64,
    label: String,
    owner: String,
    tracker_host: String,
    tracker_status: String,
}

impl Torrent {
    pub fn matches_filters(&self, filters: &FilterDict) -> bool {
        for (key, val) in filters.iter() {
            let cmp_val = match key {
                FilterKey::State if val == "Active" => if self.is_active() {
                    continue;
                } else {
                    return false;
                }

                FilterKey::Tracker if val == "Error" => if self.has_tracker_error() {
                    continue;
                } else {
                    return false;
                }

                FilterKey::State   => self.state.as_str(),
                FilterKey::Owner   => self.owner.as_str(),
                FilterKey::Label   => self.label.as_str(),
                FilterKey::Tracker => self.tracker_host.as_str(),
            };
            if val != cmp_val { return false; }
        }
        true
    }

    pub fn has_tracker_error(&self) -> bool {
        self.tracker_status.starts_with("Error:")
    }

    pub fn is_active(&self) -> bool {
        self.download_payload_rate > 0 || self.upload_payload_rate > 0
    }
}

async fn manage_events(
    session: Arc<Session>,
    mut update_send: UpdateSenders,
    mut shutdown: broadcast::Receiver<()>,
) -> deluge_rpc::Result<()> {
    let mut events = session.subscribe_events();
    let interested = deluge_rpc::events![TorrentRemoved];
    session.set_event_interest(&interested).await?;
    loop {
        tokio::select! {
            event = events.recv() => {
                match event.expect("event channel closed") {
                    deluge_rpc::Event::TorrentRemoved(hash) => {
                        update_send.torrents
                            .send(TorrentsUpdate::TorrentRemoved(hash))
                            .await
                            .expect("update channel closed");
                    },
                    e => panic!("Received unexpected event: {:?}", e),
                }
            },
            _ = shutdown.recv() => return Ok(()),
        }
    }
}

async fn manage_updates(
    session: Arc<Session>,
    mut update_send: UpdateSenders,
    mut update_recv: mpsc::Receiver<SessionUpdate>,
    mut shutdown: broadcast::Receiver<()>,
) -> deluge_rpc::Result<()> {
    let mut filter_dict = None;
    // TODO: something smarter than this
    loop {
        tokio::select! {
            update = update_recv.recv() => {
                match update.expect("update channel closed") {
                    SessionUpdate::NewFilters(new_filters) => {
                        filter_dict.replace(new_filters);
                    }
                }
            },
            _ = tokio::time::delay_for(tokio::time::Duration::from_secs(1)) => {
                let delta = session.get_torrents_status_diff::<Torrent>(filter_dict.as_ref()).await?;
                let new_tree = session.get_filter_tree(false, &[]).await?;
                update_send.torrents
                    .send(TorrentsUpdate::Delta(delta))
                    .await
                    .expect("update channel closed");
                update_send.filters
                    .send(FiltersUpdate::ReplaceTree(new_tree))
                    .await
                    .expect("update channel closed");
            }
            _ = shutdown.recv() => return Ok(()),
        }
    }
}

#[tokio::main]
async fn main() -> deluge_rpc::Result<()> {
    let endpoint = util::read_file("./experiment/endpoint");
    let mut session = Session::connect(endpoint).await?;

    let user = util::read_file("./experiment/username");
    let pass = util::read_file("./experiment/password");
    let auth_level = session.login(&user, &pass).await?;
    assert!(auth_level >= AuthLevel::Normal);
    
    let (shutdown, _) = broadcast::channel(1);

    let (filter_updates, torrent_updates, session_updates, update_send) = {
        let f = mpsc::channel(50);
        let t = mpsc::channel(50);
        let s = mpsc::channel(50);
        let u = UpdateSenders {
            filters: f.0,
            torrents: t.0,
            session_updates: s.0,
        };
        (f.1, t.1, s.1, u)
    };

    let torrents = {
        TorrentsView::new(update_send.clone(), torrent_updates)
            .with_name("torrents")
    };
    let filters = {
        FiltersView::new(update_send.clone(), filter_updates)
            .with_name("filters")
            .into_scroll_wrapper()
    };

    let status_tab = TextView::new("Torrent status (todo)");
    let details_tab = TextView::new("Torrent details (todo)");
    let options_tab = TextView::new("Torrent options (todo)");
    let files_tab = TextView::new("Torrent files (todo)");
    let peers_tab = TextView::new("Torrent peers (todo)");
    let trackers_tab = TextView::new("Torrent trackers (todo)");

    let torrent_tabs = TabPanel::new()
        .with_tab("Status", status_tab)
        .with_tab("Details", details_tab)
        .with_tab("Options", options_tab)
        .with_tab("Files", files_tab)
        .with_tab("Peers", peers_tab)
        .with_tab("Trackers", trackers_tab);

    let status_bar = TextView::new("Status bar (todo)");

    let torrents_ui = LinearLayout::new(Orientation::Horizontal)
        .child(Panel::new(filters).title("Filters"))
        .child(Panel::new(torrents).title("Torrents"));

    let main_ui = LinearLayout::new(Orientation::Vertical)
        .child(torrents_ui)
        .child(torrent_tabs)
        .child(status_bar);

    let session = Arc::new(session);

    let update_thread = tokio::spawn(manage_updates(session.clone(), update_send.clone(), session_updates, shutdown.subscribe()));
    let event_thread = tokio::spawn(manage_events(session.clone(), update_send.clone(), shutdown.subscribe()));

    let mut siv = cursive::Cursive::new(|| {
        cursive::backend::crossterm::Backend::init()
            .map(cursive_buffered_backend::BufferedBackend::new)
            .map(Box::new)
            .unwrap()
    });
    siv.set_autorefresh(true);
    siv.set_autohide_menu(false);
    siv.set_theme(themes::dracula());

    siv.add_global_callback('q', Cursive::quit);
    siv.add_global_callback(Event::Refresh, |s| {
        s.call_on_name::<TorrentsView, _, _>("torrents", Refreshable::refresh);
        s.call_on_name::<FiltersView, _, _>("filters", Refreshable::refresh);
    });

    siv.menubar()
        .add_subtree("File",
            MenuTree::new()
                .leaf("Add torrent", menu::add_torrent)
                .leaf("Create torrent", |_| ())
                .delimiter()
                .leaf("Quit and shutdown daemon", menu::quit_and_shutdown_daemon)
                .delimiter()
                .leaf("Quit", Cursive::quit));

    siv.add_fullscreen_layer(main_ui);
    
    siv.run();

    shutdown.send(()).unwrap();

    update_thread.await.unwrap()?;
    event_thread.await.unwrap()?;
        
    let session = Arc::try_unwrap(session).unwrap();
    
    session.disconnect().await.map_err(|(_stream, err)| err)?;

    Ok(())
}
