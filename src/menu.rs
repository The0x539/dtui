use cursive::event::Callback;
use cursive::menu::MenuTree;
use cursive::traits::*;
use cursive::views::{MenuPopup, TextArea};
use cursive::Cursive;
use cursive::Vec2;
use futures::executor::block_on;
use serde::Deserialize;
use std::cell::{Ref, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use tokio::task;
use uuid::Uuid;

use crate::form::Form;
use crate::{AppState, SessionHandle};

use crate::views::{
    connection_manager::ConnectionManagerView, remove_torrent::RemoveTorrentPrompt,
    tabs::files::FileKey,
};

use deluge_rpc::{FilePriority, InfoHash, Query, Session, TorrentOptions};

trait CursiveWithSession<'a> {
    fn session(&'a mut self) -> Ref<'a, Session>;

    fn with_session<T, F: FnOnce(Ref<'a, Session>) -> T>(&'a mut self, f: F) -> T {
        f(self.session())
    }

    fn with_session_blocking<T: Future, F: FnOnce(Ref<'a, Session>) -> T>(
        &'a mut self,
        f: F,
    ) -> T::Output {
        block_on(self.with_session(f))
    }
}

// "with session blocking + unwrap"
macro_rules! wsbu {
    // Invocation A: Using a Cursive object, execute a Session -> Future closure.
    ($siv:expr, $f:expr) => {
        $siv.with_session_blocking($f).unwrap()
    };

    // Invocation B: Convert a Session -> Future closure using Invocation A.
    ($f:expr) => {
        move |siv: &mut Cursive| wsbu!(siv, $f)
    };
}

// "wsbu + function"
// Simple wsbu wrapper for calling a function that accepts a Session and some other args
macro_rules! wsbuf {
    // Invocation A: a method. Needs &Session.
    ($(@$siv:expr;)? :$method:ident $(, $arg:expr)*) => {
        wsbu!($($siv,)? async move |ses: Ref<Session>| ses.$method($($arg),*).await)
    };

    // Invocation B: A function. Needs Ref<Session>.
    ($(@$siv:expr;)? $func:path $(, $arg:expr)*) => {
        wsbu!($($siv,)? async move |ses: Ref<Session>| $func(&ses $(, $arg)*).await)
    };
}

impl<'a> CursiveWithSession<'a> for Cursive {
    fn session(&'a mut self) -> Ref<'a, Session> {
        let state_ref: Ref<'a, AppState> = self
            .user_data::<RefCell<AppState>>()
            .expect("Cursive object must contain an AppState")
            .borrow();

        Ref::map(state_ref, |state: &AppState| {
            state
                .get()
                .get_session()
                .expect("SessionHandle was unexpectedly empty")
                .as_ref()
        })
    }
}

fn add_torrent(siv: &mut Cursive, text: impl AsRef<str>) {
    let text: &str = text.as_ref();
    let options = TorrentOptions::default();
    let http_headers = None;

    wsbuf!(@siv; :add_torrent_url, text, &options, http_headers);
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
    indices: &[FileKey],
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
        priorities[usize::from(*index)] = priority;
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
            wsbuf!(@siv; :rename_files, hash, renames);
        })
        .title("Rename File");

    siv.add_layer(dialog);
}

fn rename_folder_dialog(siv: &mut Cursive, hash: InfoHash, old_name: Rc<str>) {
    let dialog = TextArea::new()
        .content(old_name.as_ref())
        .with(|v| v.set_cursor(old_name.len()))
        .into_dialog(
            "Cancel",
            "Rename",
            move |siv, new_name| wsbuf!(@siv; :rename_folder, hash, &old_name, &new_name),
        )
        .title("Rename Folder");

    siv.add_layer(dialog);
}

pub fn files_tab_file_menu(
    hash: InfoHash,
    index: usize,
    old_name: &str,
    position: Vec2,
) -> Callback {
    let make_cb = move |priority| wsbuf!(set_single_file_priority, hash, index, priority);

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

pub(crate) fn files_tab_folder_menu(
    hash: InfoHash,
    files: &[FileKey],
    name: &str,
    position: Vec2,
) -> Callback {
    let files = Rc::from(files);
    let make_cb = move |priority| {
        let files = Rc::clone(&files);
        move |siv: &mut Cursive| {
            let files = Rc::clone(&files);
            wsbuf!(@siv; set_multi_file_priority, hash, &files, priority)
        }
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
            wsbuf!(@siv; :remove_torrent, hash, remove_data);
        })
        .title("Remove Torrent");

    siv.add_layer(dialog);
}

pub fn torrent_context_menu(hash: InfoHash, name: &str, position: Vec2) -> Callback {
    let name = Rc::<str>::from(name); // ugh, I hate doing this
    let cb = move |siv: &mut Cursive| {
        let name = Rc::clone(&name);
        let menu_tree = MenuTree::new()
            .leaf("Pause", wsbuf!(:pause_torrent, hash))
            .leaf("Resume", wsbuf!(:resume_torrent, hash))
            .delimiter()
            .subtree("Options", MenuTree::new().delimiter())
            .delimiter()
            .subtree("Queue", MenuTree::new().delimiter())
            .delimiter()
            .leaf("Update Tracker", wsbuf!(:force_reannounce, &[hash]))
            .leaf("Edit Trackers", |_| todo!())
            .delimiter()
            .leaf("Remove Torrent", move |siv| {
                remove_torrent_dialog(siv, hash, &name)
            })
            .delimiter()
            .leaf("Force Re-check", wsbuf!(:force_recheck, &[hash]))
            .leaf("Move Download Folder", |_| todo!())
            .subtree("Label", MenuTree::new().delimiter());

        let menu_popup = MenuPopup::new(Rc::new(menu_tree));

        siv.screen_mut()
            .add_layer_at(cursive::XY::absolute(position), menu_popup);
    };
    Callback::from_fn(cb)
}

pub fn quit_and_shutdown_daemon(siv: &mut Cursive) {
    wsbuf!(@siv; :shutdown);
    siv.quit();
}
