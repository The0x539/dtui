use deluge_rpc::*;
use tokio::sync::{mpsc, Notify};
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

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[derive(Debug)]
pub enum SessionCommand {
    AddTorrentUrl(String),
    Shutdown,
}

#[derive(Clone, Debug)]
pub(crate) struct UpdateSenders {
    pub filters: mpsc::Sender<FiltersUpdate>,
    pub torrents: mpsc::Sender<TorrentsUpdate>,
}

#[derive(Clone, Debug, serde::Deserialize, Query)]
struct Torrent {
    hash: InfoHash,
    name: String,
    state: TorrentState,
    total_size: u64,
    progress: f32,
    upload_payload_rate: u64,
    label: String,
    owner: String,
    tracker_host: String,
    tracker_status: String,
}

impl Torrent {
    pub fn matches_filters(&self, filters: &FilterDict) -> bool {
        for (key, val) in filters.iter() {
            let cmp_val = match key {
                FilterKey::State   => self.state.into(),
                FilterKey::Owner   => self.owner.as_str(),
                FilterKey::Label   => self.label.as_str(),
                FilterKey::Tracker if val == "Error" => {
                    if self.tracker_status.starts_with("Error:") {
                        continue;
                    } else {
                        return false;
                    }
                },
                FilterKey::Tracker => self.tracker_host.as_str(),
            };
            if val != cmp_val { return false; }
        }
        true
    }
}

async fn manage_session(
    mut session: Session,
    mut updates: UpdateSenders,
    mut commands: mpsc::Receiver<SessionCommand>,
    shutdown: Arc<Notify>,
) -> deluge_rpc::Result<Session> {
    let mut events = session.subscribe_events();
    let interested = deluge_rpc::events![TorrentRemoved];
    session.set_event_interest(&interested).await?;
    loop {
        tokio::select! {
            command = commands.recv() => {
                match command.expect("command channel closed") {
                    SessionCommand::AddTorrentUrl(url) => {
                        let options = TorrentOptions::default();
                        let http_headers = None;
                        session.add_torrent_url(&url, &options, http_headers).await?;
                    },
                    SessionCommand::Shutdown => {
                        session.shutdown().await?;
                    },
                }
            }
            event = events.recv() => {
                match event.expect("event channel closed") {
                    deluge_rpc::Event::TorrentRemoved(_hash) => todo!(),
                    e => panic!("Received unexpected event: {:?}", e),
                }
            }
            _ = tokio::time::delay_for(tokio::time::Duration::from_secs(1)) => {
                let delta = session.get_torrents_status_diff::<Torrent>(None).await?;
                updates.torrents
                    .send(TorrentsUpdate::Delta(delta))
                    .await
                    .expect("update channel closed");
            }
            _ = shutdown.notified() => return Ok(session),
        }
    }
}

mod menu {
    use cursive::Cursive;
    use cursive::views::{EditView, Dialog};
    use cursive::traits::*;
    use futures::executor::block_on;
    use tokio::sync::mpsc;
    use super::SessionCommand;

    fn send_cmd(siv: &mut Cursive, cmd: SessionCommand) {
        siv.with_user_data(|chan: &mut mpsc::Sender<SessionCommand>| {
            let fut = chan.send(cmd);
            block_on(fut).expect("command channel closed");
        });
    }

    pub fn add_torrent(siv: &mut Cursive) {
        let edit_view = EditView::new()
            .on_submit(|siv, text| {
                let cmd = SessionCommand::AddTorrentUrl(text.to_string());
                send_cmd(siv, cmd);
            });
        siv.add_layer(Dialog::around(edit_view).min_width(80));
    }

    pub fn quit_and_shutdown_daemon(siv: &mut Cursive) {
        send_cmd(siv, SessionCommand::Shutdown);
        siv.quit();
    }
}

#[tokio::main]
async fn main() -> deluge_rpc::Result<()> {
    let mut session = Session::connect(read_file("./experiment/endpoint")).await?;

    let user = read_file("./experiment/username");
    let pass = read_file("./experiment/password");
    let auth_level = session.login(&user, &pass).await?;
    assert!(auth_level >= AuthLevel::Normal);
    
    let (filter_updates, torrent_updates, update_send) = {
        let f = mpsc::channel(20);
        let t = mpsc::channel(20);
        let u = UpdateSenders {
            filters: f.0,
            torrents: t.0,
        };
        (f.1, t.1, u)
    };

    let (command_send, command_recv) = mpsc::channel(20);

    let shutdown = Arc::new(Notify::new());

    // TODO: By getting this data before subscribing to events, we introduce a race condition.
    // Fix by starting out with empty data and having the session thread send out a big update.
    let torrents = {
        let status = session.get_torrents_status(None).await?;
        TorrentsView::new(status, update_send.clone(), torrent_updates)
            .with_name("torrents")
    };
    let filters = {
        let tree = session.get_filter_tree(true, &[]).await?;
        FiltersView::new(tree, update_send.clone(), filter_updates)
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

    let session_thread = tokio::spawn(manage_session(session, update_send, command_recv, shutdown.clone()));

    let mut siv = cursive::Cursive::new(|| {
        cursive::backend::crossterm::Backend::init()
            .map(cursive_buffered_backend::BufferedBackend::new)
            .map(Box::new)
            .unwrap()
    });
    siv.set_autorefresh(true);
    siv.set_autohide_menu(false);
    siv.set_user_data(command_send);

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

    shutdown.notify();

    session_thread.await.unwrap()?.disconnect().await.map_err(|(_stream, err)| err)?;

    Ok(())
}
