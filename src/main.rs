#![feature(vec_remove_item)]
#![feature(bool_to_option)]
#![feature(option_result_contains)]
#![feature(drain_filter)]
#![feature(trait_alias)]

use deluge_rpc::*;
use tokio::sync::{RwLock as AsyncRwLock, watch};
use cursive::Cursive;
use cursive::traits::*;
use cursive::views::{LinearLayout, Panel};
use cursive::direction::Orientation;
use cursive::menu::MenuTree;
use std::sync::Arc;

pub mod views;
use views::{
    filters::FiltersView,
    torrents::TorrentsView,
    statusbar::StatusBarView,
    tabs::TorrentTabsView,

    scroll::ScrollInner,
};

pub mod util;

mod themes;
mod menu;

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
    let (selected_send, selected_recv) = watch::channel(None);

    let torrents = {
        TorrentsView::new(session.clone(), selected_send, filters_recv.clone(), shutdown.clone())
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

    let torrents_ui = LinearLayout::new(Orientation::Horizontal)
        .child(Panel::new(filters).title("Filters"))
        .child(Panel::new(torrents).title("Torrents"));

    let torrent_tabs = TorrentTabsView::new(session.clone(), selected_recv.clone(), shutdown.clone())
        .with_name("tabs");

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
    siv.set_user_data(session.clone());

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
                .leaf("Quit", Cursive::quit));

    siv.add_fullscreen_layer(main_ui);
    
    siv.run();

    std::mem::drop(shutdown_write_handle);

    siv.take_user_data::<Arc<Session>>().unwrap();

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

    let session = Arc::try_unwrap(session).unwrap();
    
    session.disconnect().await.map_err(|(_stream, err)| err)?;

    Ok(())
}
