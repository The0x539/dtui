#![feature(vec_remove_item)]
#![feature(bool_to_option)]

use deluge_rpc::*;
use tokio::sync::{RwLock as AsyncRwLock, watch};
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
    statusbar::StatusBarView,

    scroll::ScrollInner,
};

pub mod util;

mod themes;
mod menu;

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

#[tokio::main]
async fn main() -> deluge_rpc::Result<()> {
    let endpoint = util::read_file("./experiment/endpoint");
    let mut session = Session::connect(endpoint).await?;

    let user = util::read_file("./experiment/username");
    let pass = util::read_file("./experiment/password");
    let auth_level = session.login(&user, &pass).await?;
    assert!(auth_level >= AuthLevel::Normal);

    let session = Arc::new(session);
    
    let shutdown = Arc::new(AsyncRwLock::new(()));
    let shutdown_write_handle = shutdown.write().await;

    let (filters_send, filters_recv) = watch::channel(FilterDict::default());

    let torrents = {
        TorrentsView::new(session.clone(), filters_recv.clone(), shutdown.clone())
            .with_name("torrents")
    };
    let filters = {
        FiltersView::new(session.clone(), filters_send, shutdown.clone())
            .with_name("filters")
            .into_scroll_wrapper()
    };
    let status_bar = {
        StatusBarView::new(session.clone(), shutdown.clone())
            .with_name("status")
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

    let torrents_ui = LinearLayout::new(Orientation::Horizontal)
        .child(Panel::new(filters).title("Filters"))
        .child(Panel::new(torrents).title("Torrents"));

    let main_ui = LinearLayout::new(Orientation::Vertical)
        .child(torrents_ui)
        .child(torrent_tabs)
        .child(status_bar);

    let mut siv = cursive::Cursive::new(|| {
        cursive::backend::crossterm::Backend::init()
            .map(cursive_buffered_backend::BufferedBackend::new)
            .map(Box::new)
            .unwrap()
    });
    siv.set_autorefresh(true);
    siv.set_autohide_menu(false);
    siv.set_theme(themes::dracula());
    siv.set_user_data(session.clone());

    siv.add_global_callback('q', Cursive::quit);

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

    std::mem::drop(shutdown_write_handle);

    siv.take_user_data::<Arc<Session>>().unwrap();

    let torrents_thread = siv.call_on_name("torrents", TorrentsView::take_thread).unwrap();
    let filters_thread = siv.call_on_name("filters", FiltersView::take_thread).unwrap();
    let statusbar_thread = siv.call_on_name("status", StatusBarView::take_thread).unwrap();

    let (
        torrents_result,
        filters_result,
        statusbar_result,
    ) = tokio::try_join!(
        torrents_thread,
        filters_thread,
        statusbar_thread,
    ).unwrap();

    torrents_result?;
    filters_result?;
    statusbar_result?;

    let session = Arc::try_unwrap(session).unwrap();
    
    session.disconnect().await.map_err(|(_stream, err)| err)?;

    Ok(())
}
