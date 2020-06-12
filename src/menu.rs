use cursive::Cursive;
use cursive::views::{EditView, ResizedView, Dialog, MenuPopup};
use cursive::traits::*;
use cursive::event::Callback;
use cursive::Vec2;
use cursive::menu::MenuTree;
use cursive::views::LayerPosition;
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

fn get_dialog(siv: &mut Cursive) -> Option<&Dialog> {
    let dialog = siv
        .screen()
        .get(LayerPosition::FromFront(0))?
        .downcast_ref::<Dialog>()?;

    Some(dialog)
}

fn get_edit_dialog_contents(siv: &mut Cursive) -> Option<Rc<String>> {
    let text = get_dialog(siv)?
        .get_content()
        .downcast_ref::<ResizedView<EditView>>()?
        .get_inner()
        .get_content();

    Some(text)
}

fn add_torrent(siv: &mut Cursive, text: &str) {
    let options = TorrentOptions::default();
    let http_headers = None;

    let fut = siv.session().add_torrent_url(text, &options, http_headers);
    block_on(fut).unwrap();

    siv.pop_layer();
}

pub fn add_torrent_dialog(siv: &mut Cursive) {
    let edit_view = EditView::new()
        .on_submit(add_torrent)
        .min_width(80);

    let dialog = Dialog::around(edit_view)
        .title("Add Torrent")
        .dismiss_button("Cancel")
        .button("Add", |siv| {
            let text = get_edit_dialog_contents(siv).unwrap();
            add_torrent(siv, text.as_str());
        });

    siv.add_layer(dialog);
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

async fn set_multi_file_priority(
    session: &Session,
    hash: InfoHash,
    indices: &[usize],
    priority: FilePriority,
) -> deluge_rpc::Result<()> {
    #[derive(Debug, Clone, Deserialize, Query)]
    struct FilePriorities { file_priorities: Vec<FilePriority> }

    let mut priorities = {
        let response = session.get_torrent_status::<FilePriorities>(hash).await;
        response?.file_priorities
    };
    for index in indices {
        priorities[*index] = priority;
    }

    let options = TorrentOptions {
        file_priorities: Some(priorities),
        ..TorrentOptions::default()
    };

    session.set_torrent_options(&[hash], &options).await
}

fn rename_file(siv: &mut Cursive, hash: InfoHash, index: usize, new_name: &str) {
    let renames = &[(index as u64, new_name)];
    let fut = siv.session().rename_files(hash, renames);
    block_on(fut).unwrap();
    siv.pop_layer();
}

fn rename_file_dialog(siv: &mut Cursive, hash: InfoHash, index: usize, old_name: &str) {
    let edit_view = EditView::new()
        .content(old_name)
        .with(|v| v.set_cursor(old_name.len()))
        .on_submit(move |siv, new_name| rename_file(siv, hash, index, new_name))
        .min_width(80);

    let dialog = Dialog::around(edit_view)
        .dismiss_button("Cancel")
        .title("Rename File")
        .button("Rename", move |siv| {
            let new_name = get_edit_dialog_contents(siv).unwrap();
            rename_file(siv, hash, index, &new_name);
        });

    siv.add_layer(dialog);
}

fn rename_folder(siv: &mut Cursive, hash: InfoHash, old_name: &str, new_name: &str) {
    let fut = siv.session().rename_folder(hash, old_name, new_name);
    block_on(fut).unwrap();
    siv.pop_layer();
}

fn rename_folder_dialog(siv: &mut Cursive, hash: InfoHash, name: Rc<str>) {
    let name_clone = name.clone();
    let edit_view = EditView::new()
        .content(name.as_ref())
        .with(|v| v.set_cursor(name.len()))
        .on_submit(move |siv, new_name| rename_folder(siv, hash, &name_clone, new_name))
        .min_width(80);

    let dialog = Dialog::around(edit_view)
        .dismiss_button("Cancel")
        .title("Rename Folder")
        .button("Rename", move |siv| {
            let new_name = get_edit_dialog_contents(siv).unwrap();
            rename_folder(siv, hash, &name, &new_name);
        });

    siv.add_layer(dialog);
}

pub fn files_tab_file_menu(
    hash: InfoHash,
    index: usize,
    old_name: &str,
    position: Vec2,
) -> Callback {
    let make_cb = move |priority| move |siv: &mut Cursive| {
        let fut = set_single_file_priority(siv.session(), hash, index, priority);
        block_on(fut).unwrap();
    };

    let old_name = Rc::from(old_name);
    let cb = move |siv: &mut Cursive| {
        let old_name = Rc::clone(&old_name);
        let menu_tree = MenuTree::new()
            .leaf("Rename", move |siv| rename_file_dialog(siv, hash, index, &old_name))
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

pub fn files_tab_folder_menu(
    hash: InfoHash,
    files: &[usize],
    name: &str,
    position: Vec2,
) -> Callback {
    let files = Rc::from(files);
    let make_cb = move |priority| {
        let files = Rc::clone(&files);
        move |siv: &mut Cursive| {
            let fut = set_multi_file_priority(siv.session(), hash, &files, priority);
            block_on(fut).unwrap();
        }
    };

    let name = Rc::<str>::from(name);
    let cb = move |siv: &mut Cursive| {
        let name = name.clone();
        let menu_tree = MenuTree::new()
            .leaf("Rename", move |siv| rename_folder_dialog(siv, hash, name.clone()))
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
