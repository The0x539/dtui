use cursive::Cursive;
use cursive::views::{EditView, Dialog};
use cursive::traits::*;
use futures::executor::block_on;
use std::sync::Arc;
use deluge_rpc::Session;
use deluge_rpc::TorrentOptions;

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

pub fn quit_and_shutdown_daemon(siv: &mut Cursive) {
    let session: Arc<Session> = siv.take_user_data().unwrap();
    let f = session.shutdown();
    block_on(f).unwrap();
    siv.quit();
}
