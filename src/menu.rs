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

trait CursiveWithSession {
    fn session(&mut self) -> &Session;
}

impl CursiveWithSession for Cursive {
    fn session(&mut self) -> &Session {
        self.user_data::<Arc<Session>>()
            .expect("must actually contain a Session")
    }
}

macro_rules! session_cb {
    ($session:ident; $($stmt:stmt)*) => {
        {
            let f = move |$session: &Session| { $($stmt)* };
            let cb = move |siv: &mut Cursive| f(siv.session()).unwrap();
            Box::new(cb)
        }
    }
}

pub fn add_torrent(siv: &mut Cursive) {
    let edit_view = EditView::new()
        .on_submit(|siv, text| {
            let options = TorrentOptions::default();
            let http_headers = None;

            let fut = siv.session().add_torrent_url(text, &options, http_headers);
            block_on(fut).unwrap();
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
    let make_cb = move |priority: FilePriority| session_cb! {
        session;
        block_on(set_single_file_priority(session, hash, index, priority))
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
    let fut = siv.session().shutdown();
    block_on(fut).unwrap();
    siv.quit();
}
