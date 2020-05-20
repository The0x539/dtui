use deluge_rpc::*;
use deluge_rpc_macro::Query;
use cursive_multiplex::Mux;
use cursive::traits::*;

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

#[tokio::main]
async fn main() -> deluge_rpc::Result<()> {
    let mut session = Session::new(read_file("./experiment/endpoint")).await?;

    let user = read_file("./experiment/username");
    let pass = read_file("./experiment/password");
    let auth_level = session.login(&user, &pass).await?;
    assert!(auth_level >= AuthLevel::Normal);

    let torrents = TorrentsView::new(session.get_torrents_status(None).await?);
    let filters = FiltersView::new(session.get_filter_tree(true, &[]).await?);
    
    let mut mux = Mux::new();
    let main_pane = mux.add_right_of(torrents, mux.root().build().unwrap()).unwrap();
    let _filters_pane = mux.add_left_of(filters.scrollable(), main_pane).unwrap();
    mux.set_focus(main_pane);

    let mut siv = cursive::default();
    siv.set_fps(1);
    siv.add_global_callback('q', |s| s.quit());

    siv.add_fullscreen_layer(mux);
    
    siv.run();

    session.close().await
}
