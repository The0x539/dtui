use std::cell::Cell;
use std::cmp::{Ordering, PartialEq};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use super::{
    edit_host::EditHostView,
    labeled_checkbox::LabeledCheckbox,
    table::{TableView, TableViewData},
};
use crate::config;
use crate::form::Form;
use crate::util::Eventual;
use crate::SessionHandle;

use tokio::sync::oneshot;
use tokio::task;

use deluge_rpc::Session;

use cursive::{
    event::Callback,
    view::{View, ViewWrapper},
    views::{Button, DummyView, LinearLayout, Panel},
    Cursive, Printer,
};
use uuid::Uuid;

type FnvIndexMap<K, V> = indexmap::IndexMap<K, V, fnv::FnvBuildHasher>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column {
    Status,
    Host,
    Version,
}
impl AsRef<str> for Column {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Host => "Host",
            Self::Version => "Version",
        }
    }
}

pub(crate) struct Connection {
    address: String,
    port: u16,
    username: String,
    password: String, // ¯\_(ツ)_/¯
    version: Eventual<String>,
    session: Eventual<Arc<Session>>,
}

// TODO: helper EqByKey trait in util?
impl Connection {
    fn new(host: &config::Host) -> Self {
        let (ses_tx, ses_rx) = oneshot::channel::<Arc<Session>>();
        let (ver_tx, ver_rx) = oneshot::channel::<String>();
        let fut = connect(host.address.clone(), host.port, ses_tx, ver_tx);
        task::spawn(fut);

        Self {
            address: host.address.clone(),
            port: host.port,
            username: host.username.clone(),
            password: host.password.clone(),
            version: Eventual::new(ver_rx),
            session: Eventual::new(ses_rx),
        }
    }

    fn existing(host: &config::Host, session: Arc<Session>) -> Self {
        let (mut ver_tx, ver_rx) = oneshot::channel();
        let (ses_tx, ses_rx) = oneshot::channel();
        ses_tx.send(session.clone()).unwrap();

        let fut = async move {
            tokio::select! {
                result = session.daemon_info() => match result {
                    Ok(ver) => ver_tx.send(ver).unwrap_or(()),
                    Err(_) => (),
                },
                _ = ver_tx.closed() => (),
            }
        };
        task::spawn(fut);

        Self {
            address: host.address.clone(),
            port: host.port,
            username: host.username.clone(),
            password: host.password.clone(),
            version: Eventual::new(ver_rx),
            session: Eventual::new(ses_rx),
        }
    }

    fn eq_key<'a>(&'a self) -> impl 'a + Eq {
        (&self.username, &self.address, self.port)
    }
}

impl PartialEq<Self> for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.eq_key() == other.eq_key()
    }
}

#[derive(Default)]
pub(crate) struct ConnectionTableData {
    rows: Vec<Uuid>,
    connections: FnvIndexMap<Uuid, Connection>,
    current_host: Option<Uuid>,
    autoconnect_host: Option<Uuid>,
}

impl ConnectionTableData {
    fn get_current_host(&self) -> Option<&Connection> {
        Some(&self.connections[&self.current_host?])
    }
}

impl TableViewData for ConnectionTableData {
    type Column = Column;
    // interesting exercise in an un-sorted table
    type RowIndex = Uuid;
    type RowValue = Connection;
    type Rows = Vec<Uuid>;

    fn sort_column(&self) -> Self::Column {
        Column::Host
    }
    fn descending_sort(&self) -> bool {
        true
    }

    fn rows(&self) -> &Self::Rows {
        &self.rows
    }
    fn rows_mut(&mut self) -> &mut Self::Rows {
        &mut self.rows
    }
    fn set_rows(&mut self, val: Self::Rows) {
        self.rows = val;
    }

    fn set_sort_column(&mut self, _: Self::Column) {}
    fn set_descending_sort(&mut self, _: bool) {}

    fn compare_rows(&self, a: &Self::RowIndex, b: &Self::RowIndex) -> Ordering {
        a.cmp(b)
    }

    fn get_row_value<'a>(&'a self, index: &'a Self::RowIndex) -> &'a Self::RowValue {
        &self.connections[index]
    }

    fn draw_cell(&self, printer: &Printer, connection: &Self::RowValue, column: Self::Column) {
        let print = |s| printer.print((0, 0), s);
        match column {
            Column::Status => {
                // uuuuuuugh, the catch-22 of passing draw-cell a value vs an index
                if self.get_current_host().contains(&connection) {
                    assert!(connection.session.get().is_some());
                    print("Connected");
                } else if connection.session.get().is_some() {
                    print("Online");
                } else {
                    print("Offline");
                }
            }
            Column::Host => print(&format!(
                "{}@{}:{}",
                connection.username, connection.address, connection.port
            )),
            Column::Version => {
                if let Some(s) = connection.version.get() {
                    print(&s);
                }
            }
        }
    }
}

pub(crate) struct ConnectionManagerView {
    inner: LinearLayout,
}

async fn connect(
    address: String,
    port: u16,
    mut session_tx: oneshot::Sender<Arc<Session>>,
    mut version_tx: oneshot::Sender<String>,
) {
    let endpoint = (address.as_str(), port);

    let info = async {
        let session = Session::connect(endpoint).await?;
        let version = session.daemon_info().await?;
        deluge_rpc::Result::Ok((session, version))
    };

    let (ses, ver) = tokio::select! {
        result = info => match result {
            Ok(x) => x,
            Err(_) => return (),
        },
        _ = session_tx.closed() => return (),
        _ = version_tx.closed() => return (),
    };

    session_tx.send(Arc::new(ses)).unwrap_or(());
    version_tx.send(ver).unwrap_or(());
}

impl ConnectionManagerView {
    pub fn new(current_host: SessionHandle) -> Self {
        // TODO: where did this handle come from?
        // is it an additional ref not listed in main.rs?

        let cmgr = &config::read().connection_manager;

        let autoconnect_host = cmgr.autoconnect;
        let hide_dialog = cmgr.hide_on_start;

        let auto_connect = current_host.get_id() == autoconnect_host;

        let cols = vec![
            (Column::Status, 9),
            (Column::Host, 50),
            (Column::Version, 11),
        ];
        let mut table = TableView::<ConnectionTableData>::new(cols);

        let selected_connection = Rc::new(Cell::new(None));

        let on_sel_change = enclose!(selected_connection in move |_: &mut _, sel: &Uuid, _, _| {
            selected_connection.set(Some(*sel));
            Callback::dummy()
        });
        table.set_on_selection_change(on_sel_change);

        let table_data = table.get_data();

        let mut data = table_data.write().unwrap();

        data.rows = cmgr.hosts.keys().copied().collect();
        let len = data.rows.len();
        data.connections.reserve(len);
        data.autoconnect_host = autoconnect_host;

        let current_id = current_host.get_id();
        data.current_host = current_id;

        for (id, host) in &cmgr.hosts {
            let conn = if current_id.contains(id) {
                let session = current_host.get_session().unwrap().clone();
                Connection::existing(host, session)
            } else {
                Connection::new(host)
            };

            data.connections.insert(*id, conn);
        }

        drop(data);
        drop(cmgr);

        let add_button = enclose!(table_data in move |siv: &mut Cursive| {
            let save_host = enclose!(table_data in move |_: &mut _, host: config::Host| {
                let id = Uuid::new_v4();

                let mut data = table_data.write().unwrap();

                data.connections.insert(id, Connection::new(&host));
                data.rows.push(id);

                let mut cfg = config::write();
                cfg.connection_manager.hosts.insert(id, host);
                cfg.save();
            });

            let dialog = EditHostView::default()
                .into_dialog("Cancel", "Save", save_host)
                .title("Add Host");

            siv.add_layer(dialog);
        });

        let edit_button = enclose!(selected_connection, table_data in move |siv: &mut Cursive| {
            let id = selected_connection
                .get()
                .expect("No selection; edit button should be disabled");

            let conn = &table_data.read().unwrap().connections[&id];

            let view = EditHostView::new(&conn.address, conn.port, &conn.username, &conn.password);
            drop(conn);

            let save_host = enclose!(table_data in move |_: &mut _, host: config::Host| {
                table_data
                    .write()
                    .unwrap()
                    .connections
                    .insert(id, Connection::new(&host));

                let mut cfg = config::write();
                cfg.connection_manager.hosts.insert(id, host);
                cfg.save();
            });

            let dialog = view
                .into_dialog("Cancel", "Save", save_host)
                .title("Edit Host");

            siv.add_layer(dialog);
        });

        let remove_button = enclose!(selected_connection, table_data in move |_: &mut _| {
            let id = selected_connection
                .get()
                .expect("No selection; remove button should be disabled");

            let mut data = table_data.write().unwrap();

            assert_eq!(data.current_host, Some(id));
            data.current_host = None;

            data.connections
                .remove(&id)
                .expect("Tried to remove nonexistent connection");
        });

        let buttons = LinearLayout::horizontal()
            .child(Button::new("Add", add_button))
            .child(Button::new("Edit", edit_button))
            .child(Button::new("Remove", remove_button))
            .child(Button::new("Refresh", |_| ()))
            .child(DummyView)
            .child(Button::new("Stop Daemon", |_| ()));

        let startup_options = {
            let auto_connect_checkbox =
                LabeledCheckbox::new("Auto-connect to selected daemon").with_checked(auto_connect);

            let hide_dialog_checkbox =
                LabeledCheckbox::new("Hide this dialog").with_checked(hide_dialog);

            let content = LinearLayout::vertical()
                .child(auto_connect_checkbox)
                .child(hide_dialog_checkbox);

            Panel::new(content).title("Startup Options")
        };

        let inner = LinearLayout::vertical()
            .child(table)
            .child(buttons)
            .child(startup_options);
        Self { inner }
    }
}

impl ViewWrapper for ConnectionManagerView {
    cursive::wrap_impl!(self.inner: LinearLayout);
}

// Entirely as soon as tuple-based layouts are implemented
fn remove_last<T: View>(view: &mut LinearLayout) -> Box<T> {
    assert_ne!(view.len(), 0);
    let child = view.remove_child(view.len() - 1).unwrap();

    assert!(child.is::<T>());
    child.downcast::<T>().ok().unwrap()
}

impl Form for ConnectionManagerView {
    type Data = Option<(Uuid, Arc<Session>, String, String)>;

    fn into_data(self) -> Self::Data {
        let mut inner = self.inner;

        remove_last::<Panel<LinearLayout>>(&mut inner); // startup options
        remove_last::<LinearLayout>(&mut inner); // table buttons
        let table = remove_last::<TableView<ConnectionTableData>>(&mut inner);

        let data: Arc<RwLock<ConnectionTableData>> = table.get_data();

        // TODO: Save prefs BEFORE THIS POINT.
        // Starting now, there will be early returns.
        let selected: Uuid = table.get_selection().copied()?;

        drop(table);
        assert_eq!(Arc::strong_count(&data), 1);
        let mut data = Arc::try_unwrap(data).ok().unwrap().into_inner().unwrap();

        let connection = data
            .connections
            .remove(&selected)
            .expect("No selection; the connection button ought to be disabled.");

        if data.current_host.contains(&selected) {
            assert!(connection.session.is_ready());
            None // Disconnect from current session
        } else if let Some(session) = connection.session.get() {
            assert_eq!(Arc::strong_count(&session), 2);
            Some((selected, session, connection.username, connection.password))
        } else {
            todo!("No successfully connected session; the connect button should be disabled.")
        }
    }
}
