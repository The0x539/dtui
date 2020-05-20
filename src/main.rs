use deluge_rpc::*;
use deluge_rpc_macro::Query;
use tokio::sync::mpsc;
use cursive::event::Event;
use std::collections::HashMap;
use cursive::traits::*;
use cursive::views::{LinearLayout, TextView};
use cursive::direction::Orientation;

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
    mut shutdown: mpsc::Receiver<()>,
) -> Session {
    let mut filter_dict = None;
    loop {
        tokio::select! {
            new_filters = filters.recv() => {
                filter_dict = Some(new_filters.unwrap()
                    .into_iter()
                    .map(|(k, v)| (k, serde_yaml::Value::from(v.as_str())))
                    .collect());
                let new_torrents = session.get_torrents_status(filter_dict.clone()).await.unwrap();
                torrents.send(TorrentsUpdate::Replace(new_torrents)).await.unwrap();
            }
            _ = shutdown.recv() => break,
            _ = tokio::time::delay_for(tokio::time::Duration::from_secs(1)) => {
                // TODO: change API to accept an &Option?
                let delta = session.get_torrents_status_diff::<Torrent>(filter_dict.clone()).await.unwrap();
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

    let torrents = TorrentsView::new(session.get_torrents_status(None).await?, torrent_recv).with_name("torrents");
    let filters = FiltersView::new(session.get_filter_tree(true, &[]).await?, filter_send);
    let details = TextView::new("torrent details");

    let torrents_ui = LinearLayout::new(Orientation::Horizontal).child(filters).child(torrents);
    let main_ui = LinearLayout::new(Orientation::Vertical).child(torrents_ui).child(details);

    let session_thread = tokio::spawn(manage_session(session, filter_recv, torrent_send, shutdown_recv));

    let mut siv = cursive::default();
    siv.set_fps(1);
    siv.add_global_callback('q', |s| s.quit());
    siv.add_global_callback(Event::Refresh, |s| { s.call_on_name("torrents", TorrentsView::refresh); });
    siv.add_fullscreen_layer(main_ui);
    
    siv.run();

    shutdown_send.send(()).await.unwrap();

    session_thread.await.unwrap().close().await
}
