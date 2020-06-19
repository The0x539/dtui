use cursive::event::Callback;
use cursive::menu::MenuTree;
use cursive::traits::*;
use cursive::views::{MenuPopup, TextArea};
use cursive::Cursive;
use cursive::Vec2;
use futures::executor::block_on;
use serde::Deserialize;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use tokio::task;
use uuid::Uuid;

use crate::form::Form;
use crate::{AppState, SessionHandle};

use crate::views::{
    connection_manager::ConnectionManagerView, remove_torrent::RemoveTorrentPrompt,
};

use deluge_rpc::{FilePriority, InfoHash, Query, Session, TorrentOptions};

trait CursiveWithSession<'a> {
    fn session(&'a mut self) -> &'a Session;

    fn with_session<T, F: FnOnce(&'a Session) -> T>(&'a mut self, f: F) -> T {
        f(&self.session())
    }

    fn with_session_blocking<T: Future, F: FnOnce(&'a Session) -> T>(
        &'a mut self,
        f: F,
    ) -> T::Output {
        block_on(self.with_session(f))
    }
}

macro_rules! wsbu {
    ($siv:expr, $f:expr) => {
        $siv.with_session_blocking($f).unwrap()
    };

    ($f:expr) => {
        move |siv: &mut Cursive| wsbu!(siv, $f)
    };
}

impl<'a> CursiveWithSession<'a> for Cursive {
    fn session(&'a mut self) -> &'a Session {
        self.user_data::<AppState>()
            .map(|app_state| app_state.get())
            .expect("Cursive object must contain an AppState")
            .get_session()
            .expect("SessionHandle was unexpectedly empty")
    }
}

fn add_torrent(siv: &mut Cursive, text: impl AsRef<str>) {
    let text: &str = text.as_ref();
    let options = TorrentOptions::default();
    let http_headers = None;

    wsbu!(siv, |ses| ses.add_torrent_url(text, &options, http_headers));
}

pub fn add_torrent_dialog(siv: &mut Cursive) {
    let dialog = TextArea::new()
        .into_dialog("Cancel", "Add", add_torrent)
        .title("Add Torrent");

    siv.add_layer(dialog);
}

fn replace_session(siv: &mut Cursive, new: Option<(Uuid, Arc<Session>, String, String)>) {
    let handle = match new {
        Some((id, mut session, username, password)) => {
            assert_eq!(Arc::strong_count(&session), 1);
            let fut = Arc::get_mut(&mut session)
                .unwrap()
                .login(&username, &password);

            block_on(fut).unwrap();
            SessionHandle::new(id, session)
        }
        None => SessionHandle::default(),
    };
    siv.with_user_data(|app_state: &mut AppState| {
        task::block_in_place(|| block_on(app_state.replace(handle))).unwrap();
    })
    .unwrap();
}

pub fn show_connection_manager(siv: &mut Cursive) {
    let app_state = siv.user_data::<AppState>().unwrap();
    let session_handle = app_state.get().clone();
    let dialog = ConnectionManagerView::new(session_handle)
        .max_size((80, 20))
        .into_dialog("Close", "Connect/Disconnect", replace_session)
        .title("Connection Manager");

    siv.add_layer(dialog);
}

async fn set_single_file_priority(
    session: &Session,
    hash: InfoHash,
    index: usize,
    priority: FilePriority,
) -> deluge_rpc::Result<()> {
    #[derive(Debug, Clone, Deserialize, Query)]
    struct FilePriorities {
        file_priorities: Vec<FilePriority>,
    }

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
    struct FilePriorities {
        file_priorities: Vec<FilePriority>,
    }

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

fn rename_file_dialog(siv: &mut Cursive, hash: InfoHash, index: usize, old_name: &str) {
    let dialog = TextArea::new()
        .content(old_name)
        .with(|v| v.set_cursor(old_name.len()))
        .into_dialog("Cancel", "Rename", move |siv, new_name| {
            let renames = &[(index as u64, new_name.as_str())];
            wsbu!(siv, |ses| ses.rename_files(hash, renames));
        })
        .title("Rename File");

    siv.add_layer(dialog);
}

fn rename_folder_dialog(siv: &mut Cursive, hash: InfoHash, old_name: Rc<str>) {
    let dialog = TextArea::new()
        .content(old_name.as_ref())
        .with(|v| v.set_cursor(old_name.len()))
        .into_dialog("Cancel", "Rename", move |siv, new_name| {
            wsbu!(siv, |ses| ses.rename_folder(hash, &old_name, &new_name));
        })
        .title("Rename Folder");

    siv.add_layer(dialog);
}

pub fn files_tab_file_menu(
    hash: InfoHash,
    index: usize,
    old_name: &str,
    position: Vec2,
) -> Callback {
    let make_cb =
        move |priority| wsbu!(|ses| { set_single_file_priority(ses, hash, index, priority) });

    let old_name = Rc::from(old_name);
    let cb = move |siv: &mut Cursive| {
        let old_name = Rc::clone(&old_name);
        let menu_tree = MenuTree::new()
            .leaf("Rename", move |siv| {
                rename_file_dialog(siv, hash, index, &old_name)
            })
            .delimiter()
            .leaf("Skip", make_cb(FilePriority::Skip))
            .leaf("Low", make_cb(FilePriority::Low))
            .leaf("Normal", make_cb(FilePriority::Normal))
            .leaf("High", make_cb(FilePriority::High));

        let menu_popup = MenuPopup::new(Rc::new(menu_tree));

        siv.screen_mut()
            .add_layer_at(cursive::XY::absolute(position), menu_popup);
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
        wsbu!(|ses| { set_multi_file_priority(ses, hash, &files, priority) })
    };

    let name = Rc::<str>::from(name);
    let cb = move |siv: &mut Cursive| {
        let name = name.clone();
        let menu_tree = MenuTree::new()
            .leaf("Rename", move |siv| {
                rename_folder_dialog(siv, hash, name.clone())
            })
            .delimiter()
            .leaf("Skip", make_cb(FilePriority::Skip))
            .leaf("Low", make_cb(FilePriority::Low))
            .leaf("Normal", make_cb(FilePriority::Normal))
            .leaf("High", make_cb(FilePriority::High));

        let menu_popup = MenuPopup::new(Rc::new(menu_tree));

        siv.screen_mut()
            .add_layer_at(cursive::XY::absolute(position), menu_popup);
    };
    Callback::from_fn(cb)
}

fn remove_torrent_dialog(siv: &mut Cursive, hash: InfoHash, name: impl AsRef<str>) {
    let dialog = RemoveTorrentPrompt::new_single(name.as_ref())
        .into_dialog("Cancel", "OK", move |siv, remove_data| {
            wsbu!(siv, |ses| ses.remove_torrent(hash, remove_data));
        })
        .title("Remove Torrent");

    siv.add_layer(dialog);
}

pub fn torrent_context_menu(hash: InfoHash, name: &str, position: Vec2) -> Callback {
    let name = Rc::<str>::from(name); // ugh, I hate doing this
    let cb = move |siv: &mut Cursive| {
        let name = Rc::clone(&name);
        let hash_slice = [hash];
        let menu_tree = MenuTree::new()
            .leaf("Pause", wsbu!(|ses| ses.pause_torrent(hash)))
            .leaf("Resume", wsbu!(|ses| ses.resume_torrent(hash)))
            .delimiter()
            .subtree("Options", MenuTree::new().delimiter())
            .delimiter()
            .subtree("Queue", MenuTree::new().delimiter())
            .delimiter()
            .leaf("Update Tracker", wsbu!(|s| s.force_reannounce(&hash_slice)))
            .leaf("Edit Trackers", |_| todo!())
            .delimiter()
            .leaf("Remove Torrent", move |siv| {
                remove_torrent_dialog(siv, hash, &name)
            })
            .delimiter()
            .leaf("Force Re-check", wsbu!(|s| s.force_recheck(&hash_slice)))
            .leaf("Move Download Folder", |_| todo!())
            .subtree("Label", MenuTree::new().delimiter());

        let menu_popup = MenuPopup::new(Rc::new(menu_tree));

        siv.screen_mut()
            .add_layer_at(cursive::XY::absolute(position), menu_popup);
    };
    Callback::from_fn(cb)
}

pub fn quit_and_shutdown_daemon(siv: &mut Cursive) {
    wsbu!(siv, Session::shutdown);
    siv.quit();
}
