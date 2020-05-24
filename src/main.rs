use deluge_rpc::*;
use tokio::sync::mpsc;
use cursive::event::Event;
use cursive::Cursive;
use std::collections::HashMap;
use cursive::traits::*;
use cursive::views::{LinearLayout, TextView, Panel, EditView, Dialog};
use cursive::direction::Orientation;
use cursive::menu::MenuTree;
use cursive_tabs::TabPanel;

mod views;
use views::*;

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[derive(Debug)]
enum TorrentsUpdate {
    Replace(HashMap<InfoHash, Torrent>),
    Delta(HashMap<InfoHash, <Torrent as Query>::Diff>),
}

enum SessionCommand {
    AddTorrentUrl(String),
}

#[derive(Clone, Debug, serde::Deserialize, Query)]
struct Torrent {
    name: String,
    state: TorrentState,
    total_size: u64,
    progress: f32,
    upload_payload_rate: u64,
}

async fn manage_session(
    mut session: Session,
    mut filters: mpsc::Receiver<HashMap<String, String>>,
    mut torrents: mpsc::Sender<TorrentsUpdate>,
    mut commands: mpsc::Receiver<SessionCommand>,
    mut shutdown: mpsc::Receiver<()>,
) -> Session {
    let mut filter_dict = None;
    loop {
        tokio::select! {
            new_filters = filters.recv() => {
                filter_dict = Some(new_filters.unwrap());
                let new_torrents = session.get_torrents_status(filter_dict.clone()).await.unwrap();
                torrents.send(TorrentsUpdate::Replace(new_torrents)).await.unwrap();
            }
            command = commands.recv() => {
                match command.unwrap() {
                    SessionCommand::AddTorrentUrl(url) => {
                        let options = TorrentOptions::default();
                        let http_headers = None;
                        session.add_torrent_url(&url, &options, http_headers).await.unwrap();
                    }
                }
            }
            _ = shutdown.recv() => break,
            _ = tokio::time::delay_for(tokio::time::Duration::from_secs(1)) => {
                // TODO: change API to accept an &Option?
                let delta = session.get_torrents_status_diff::<Torrent, _>(filter_dict.clone()).await.unwrap();
                torrents.send(TorrentsUpdate::Delta(delta)).await.unwrap();
            }
        }
    }

    session
}

#[tokio::main]
async fn main() -> deluge_rpc::Result<()> {
    let mut session = Session::new(read_file("./experiment/endpoint")).await?;

    let user = read_file("./experiment/username");
    let pass = read_file("./experiment/password");
    let auth_level = session.login(&user, &pass).await?;
    assert!(auth_level >= AuthLevel::Normal);
    
    let (filter_send, filter_recv) = mpsc::channel(10);
    let (torrent_send, torrent_recv) = mpsc::channel(10);
    let (mut shutdown_send, shutdown_recv) = mpsc::channel(1);
    let (command_send, command_recv) = mpsc::channel(20);

    let torrents = TorrentsView::new(session.get_torrents_status::<_, ()>(None).await?, torrent_recv).with_name("torrents");
    let filters = FiltersView::new(session.get_filter_tree(true, &[]).await?, filter_send).into_scroll_wrapper();

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

    // This is so dumb. There should be a widget that draws these borders for me.
    let torrents_ui = LinearLayout::new(Orientation::Horizontal)
        .child(Panel::new(filters).title("Filters"))
        .child(Panel::new(torrents).title("Torrents"));

    let main_ui = LinearLayout::new(Orientation::Vertical)
        .child(torrents_ui)
        .child(torrent_tabs)
        .child(status_bar);

    let session_thread = tokio::spawn(manage_session(session, filter_recv, torrent_send, command_recv, shutdown_recv));

    let mut siv = cursive::Cursive::new(|| {
        cursive::backend::crossterm::Backend::init()
            .map(cursive_buffered_backend::BufferedBackend::new)
            .map(Box::new)
            .unwrap()
    });
    siv.set_fps(1);
    siv.set_autohide_menu(false);
    siv.set_user_data(command_send);

    siv.add_global_callback('q', |s| s.quit());
    siv.add_global_callback(Event::Refresh, |s| { s.call_on_name("torrents", TorrentsView::refresh); });

    siv.menubar()
        .add_subtree("File",
            MenuTree::new()
                .leaf("Add torrent", |s| {
                    let edit_view = EditView::new()
                        .on_submit(|s, x| {
                            s.with_user_data(|c: &mut mpsc::Sender<SessionCommand>| {
                                match c.try_send(SessionCommand::AddTorrentUrl(String::from(x))) {
                                    Ok(()) => (),
                                    Err(_) => panic!("ugh"),
                                }
                            });
                            s.pop_layer();
                        });
                    s.add_layer(Dialog::around(edit_view).min_width(80));
                })
                .leaf("Create torrent", |_| ())
                .delimiter()
                .leaf("Quit and shutdown daemon", |_| ())
                .delimiter()
                .leaf("Quit", Cursive::quit))
        ;

    siv.add_fullscreen_layer(main_ui);
    
    siv.run();

    shutdown_send.send(()).await.unwrap();

    session_thread.await.unwrap().close().await
}
