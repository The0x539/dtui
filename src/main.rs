use deluge_rpc::*;
use deluge_rpc_macro::Query;
use cursive_multiplex::Mux;
use cursive::traits::*;
use tokio::sync::mpsc;
use cursive::event::Event;
use std::collections::HashMap;

mod views;
use views::*;

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[derive(Clone, Debug, serde::Deserialize, Query)]
struct Torrent {
    name: String,
    state: TorrentState,
    total_size: u64,
    progress: f32,
}

async fn manage_session(
    mut session: Session,
    mut filters: mpsc::Receiver<HashMap<String, String>>,
    mut torrents: mpsc::Sender<HashMap<InfoHash, Torrent>>,
    mut shutdown: mpsc::Receiver<()>,
) -> Session {
    loop {
        tokio::select! {
            new_filters = filters.recv() => {
                let new_filters = new_filters.unwrap()
                    .into_iter()
                    .map(|(k, v)| (k, serde_yaml::Value::from(v.as_str())))
                    .collect();
                let new_torrents = session.get_torrents_status::<Torrent>(Some(new_filters)).await.unwrap();
                torrents.send(new_torrents).await.unwrap();
            }
            _ = shutdown.recv() => {
                break;
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
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

    let torrents = TorrentsView::new(session.get_torrents_status(None).await?, torrent_recv);
    let filters = FiltersView::new(session.get_filter_tree(true, &[]).await?, filter_send);

    let session_thread = tokio::spawn(manage_session(session, filter_recv, torrent_send, shutdown_recv));

    let mut mux = Mux::new();
    let main_pane = mux.add_right_of(torrents.with_name("torrents"), mux.root().build().unwrap()).unwrap();
    mux.add_left_of(filters, main_pane).unwrap();
    mux.set_focus(main_pane);

    let mut siv = cursive::default();
    siv.set_fps(1);
    siv.add_global_callback('q', |s| s.quit());
    siv.add_global_callback(Event::Refresh, |s| s.call_on_name("torrents", TorrentsView::update_torrents).unwrap());
    siv.add_fullscreen_layer(mux);
    
    siv.run();

    shutdown_send.send(()).await.unwrap();

    session_thread.await.unwrap().close().await
}
