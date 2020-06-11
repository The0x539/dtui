use cursive::Cursive;
use cursive::views::{EditView, Dialog, MenuPopup};
use cursive::traits::*;
use cursive::event::Callback;
use cursive::Vec2;
use cursive::menu::MenuTree;
use futures::executor::block_on;
use serde::Deserialize;
use std::sync::Arc;
use std::rc::Rc;

use deluge_rpc::{Session, TorrentOptions, FilePriority, Query, InfoHash};

pub fn add_torrent(siv: &mut Cursive) {
    let edit_view = EditView::new()
        .on_submit(|siv, text| {
            let options = TorrentOptions::default();
            let http_headers = None;
            let session: Arc<Session> = siv.take_user_data().unwrap();
            let f = session.add_torrent_url(text, &options, http_headers);
            block_on(f).unwrap();
            siv.set_user_data(session);
        });
    siv.add_layer(Dialog::around(edit_view).min_width(80));
}

async fn set_single_file_priority(
    session: &Session,
    hash: InfoHash,
    index: usize,
    priority: FilePriority,
) -> deluge_rpc::Result<()> {
    #[derive(Debug, Clone, Deserialize, Query)]
    struct FilePriorities { file_priorities: Vec<FilePriority> }

    let mut priorities = {
        let response = session.get_torrent_status::<FilePriorities>(hash).await;
        response?.file_priorities
    };
    priorities[index] = priority;

    let options = TorrentOptions {
        file_priorities: Some(priorities),
        ..TorrentOptions::default()
    };

    session.set_torrent_options(&[hash], &options).await
}


pub fn files_tab_file_menu(
    hash: InfoHash,
    index: usize,
    position: Vec2,
) -> Callback {
    let make_cb = move |priority: FilePriority| move |siv: &mut Cursive| {
        let session: Arc<Session> = siv.take_user_data().unwrap();
        let f = set_single_file_priority(&session, hash, index, priority);
        block_on(f).unwrap();
        siv.set_user_data(session);
    };

    let cb = move |siv: &mut Cursive| {
        let menu_tree = MenuTree::new()
            .leaf("Rename", |_| todo!())
            .delimiter()
            .leaf("Skip",   make_cb(FilePriority::Skip))
            .leaf("Low",    make_cb(FilePriority::Low))
            .leaf("Normal", make_cb(FilePriority::Normal))
            .leaf("High",   make_cb(FilePriority::High));

        let menu_popup = MenuPopup::new(Rc::new(menu_tree));

        siv.screen_mut().add_layer_at(cursive::XY::absolute(position), menu_popup);
    };
    Callback::from_fn(cb)
}

pub fn quit_and_shutdown_daemon(siv: &mut Cursive) {
    let session: Arc<Session> = siv.take_user_data().unwrap();
    let f = session.shutdown();
    block_on(f).unwrap();
    siv.quit();
}
