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
use uuid::Uuid;

use crate::form::Form;
use crate::{AppState, SessionHandle};

use crate::views::{
    connection_manager::ConnectionManagerView, remove_torrent::RemoveTorrentPrompt,
    tabs::files::FileKey,
};

use deluge_rpc::{FilePriority, InfoHash, Query, Session, TorrentOptions};

trait CursiveWithSession<'a> {
    type Ref: 'a;

    fn session(&'a mut self) -> Self::Ref;

    fn with_session<T, F>(&'a mut self, f: F) -> T
    where
        F: FnOnce(Self::Ref) -> T,
    {
        f(self.session())
    }

    fn with_session_blocking<T, F>(&'a mut self, f: F) -> T::Output
    where
        T: Future,
        F: FnOnce(Self::Ref) -> T,
    {
        block_on(self.with_session(f))
    }
}

// "with session blocking + unwrap"
// Simple macro for more concisely performing RPC inside of Cursive callbacks.
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
// If the invocation starts with `@siv;`, then wsbu invocation A will be used.
// Otherwise, wsbu invocation B will be used.
macro_rules! wsbuf {
    // Invocation A: A method.
    ($(@$siv:expr;)? :$method:ident $(, $arg:expr)*) => {
        wsbu!($($siv,)? async move |ses| ses.$method($($arg),*).await)
    };

    // Invocation B: A function.
    ($(@$siv:expr;)? $func:path $(, $arg:expr)*) => {
        wsbu!($($siv,)? async move |ses| $func(ses $(, $arg)*).await)
    };
}

impl<'a> CursiveWithSession<'a> for Cursive {
    type Ref = &'a Arc<Session>;

    fn session(&'a mut self) -> Self::Ref {
        self.user_data::<AppState>()
            .expect("Cursive object must contain an AppState")
            .get()
            .get_session()
            .expect("SessionHandle was unexpectedly empty")
    }
}

fn add_torrent(siv: &mut Cursive, text: String) {
    let options = TorrentOptions::default();
    let http_headers = None;

    wsbuf!(@siv; :add_torrent_url, &text, &options, http_headers);
}

pub fn add_torrent_dialog(siv: &mut Cursive) {
    let dialog = TextArea::new()
        .into_dialog("Cancel", "Add", add_torrent)
        .title("Add Torrent");

    siv.add_layer(dialog);
}

fn replace_session(siv: &mut Cursive, new: Option<(Uuid, Arc<Session>, String, String)>) {
    let handle = new
        .map(|(id, mut session, user, pass)| {
            assert_eq!(Arc::strong_count(&session), 1);
            let fut = Arc::get_mut(&mut session).unwrap().login(&user, &pass);
            block_on(fut).unwrap();
            SessionHandle::new(id, session)
        })
        .unwrap_or_default();

    let app_state = siv.user_data::<AppState>().unwrap();
    let fut = app_state.replace(handle);
    block_on(fut).unwrap();
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

    let mut name = Some(Rc::from(name));
    let cb = move |siv: &mut Cursive| {
        let name = name.take().unwrap();
        let menu_tree = MenuTree::new()
            .leaf("Rename", move |siv| {
                rename_folder_dialog(siv, hash, Rc::clone(&name))
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
    Callback::from_fn_mut(cb)
}

fn remove_torrent_dialog(siv: &mut Cursive, hash: InfoHash, name: &str) {
    let dialog = RemoveTorrentPrompt::new_single(name)
        .into_dialog("Cancel", "OK", move |siv, remove_data| {
            wsbuf!(@siv; :remove_torrent, hash, remove_data);
        })
        .title("Remove Torrent");

    siv.add_layer(dialog);
}

pub fn torrent_context_menu(hash: InfoHash, name: &str, position: Vec2) -> Callback {
    let mut name = Some(Box::from(name)); // It's so dumb that this is necessary.
    let cb = move |siv: &mut Cursive| {
        let name = name.take().unwrap();

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
    Callback::from_fn_mut(cb)
}

pub fn quit_and_shutdown_daemon(siv: &mut Cursive) {
    wsbuf!(@siv; :shutdown);
    siv.quit();
}
