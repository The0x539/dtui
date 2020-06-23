use std::cell::RefCell;
use std::cmp::{Ordering, PartialEq};
use std::ops::Deref;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use super::{
    edit_host::EditHostView,
    labeled_checkbox::LabeledCheckbox,
    table::{TableView, TableViewData},
};
use crate::config;
use crate::form::Form;
use crate::SessionHandle;

use tokio::task;

use deluge_rpc::Session;

use cursive::{
    event::Callback,
    view::ViewWrapper,
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
    username: String,
    password: String, // ¯\_(ツ)_/¯
    address: String,
    port: u16,
    version: Arc<RwLock<Option<String>>>,
    session: Arc<RwLock<Option<Arc<Session>>>>,
}

// TODO: helper EqByKey trait in util?
impl Connection {
    fn eq_key<'a>(&'a self) -> impl 'a + Eq {
        (&self.username, &self.address, self.port)
    }
}

impl From<config::Host> for Connection {
    fn from(val: config::Host) -> Self {
        let config::Host {
            username,
            password,
            port,
            address,
        } = val;
        let (version, session) = Default::default();
        Self {
            username,
            password,
            port,
            address,
            version,
            session,
        }
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
                    assert!(connection.session.read().unwrap().is_some());
                    print("Connected");
                } else if connection.session.read().unwrap().is_some() {
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
                if let Some(s) = connection.version.read().unwrap().deref() {
                    print(s);
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
    session_handle: Arc<RwLock<Option<Arc<Session>>>>,
    version_handle: Arc<RwLock<Option<String>>>,
) -> deluge_rpc::Result<()> {
    let endpoint = (address.as_str(), port);
    let session = Session::connect(endpoint).await?;
    let version = session.daemon_info().await?;

    if let (Ok(mut ses), Ok(mut ver)) = (session_handle.write(), version_handle.write()) {
        ses.replace(Arc::new(session));
        ver.replace(version);
    }

    Ok(())
}

impl ConnectionManagerView {
    pub fn new(current_host: SessionHandle) -> Self {
        // TODO: where did this handle come from?
        // is it an additional ref not listed in main.rs?

        let cmgr = &config::read().connection_manager;

        let connections: FnvIndexMap<Uuid, Connection> = cmgr
            .hosts
            .iter()
            .map(|(id, host)| (*id, host.clone().into()))
            .collect();

        let mut threads = Vec::with_capacity(connections.len());

        let autoconnect_host = cmgr.autoconnect;
        let hide_dialog = cmgr.hide_on_start;
        drop(cmgr);

        let auto_connect = current_host.get_id() == autoconnect_host;

        let cols = vec![
            (Column::Status, 9),
            (Column::Host, 50),
            (Column::Version, 11),
        ];
        let mut table = TableView::<ConnectionTableData>::new(cols);

        let selected_connection = Rc::new(RefCell::new(None));

        let sel_clone_change = selected_connection.clone();
        let on_sel_change = move |_: &mut _, sel: &Uuid, _, _| {
            sel_clone_change.replace(Some(*sel));
            Callback::dummy()
        };
        table.set_on_selection_change(on_sel_change);

        let table_data = table.get_data();
        {
            let mut data = table_data.write().unwrap();

            data.rows = connections.keys().copied().collect();
            data.connections = connections;
            data.autoconnect_host = autoconnect_host;
            if let Some((id, session)) = current_host.into_both() {
                data.current_host.replace(id);
                data.connections[&id]
                    .session
                    .write()
                    .unwrap()
                    .replace(session);
            }

            for connection in data.connections.values_mut() {
                if connection.session.read().unwrap().is_some() {
                    continue;
                }
                let fut = connect(
                    connection.address.clone(),
                    connection.port,
                    connection.session.clone(),
                    connection.version.clone(),
                );
                threads.push(task::spawn(fut));
            }
        }

        let sel_clone_edit = selected_connection.clone();
        let table_data_clone_edit = table_data.clone();
        let edit_button = move |siv: &mut Cursive| {
            let sel = sel_clone_edit.borrow();
            let id = sel.expect("Edit button should be disabled");

            let conn = &table_data_clone_edit.read().unwrap().connections[&id];

            let view = EditHostView::new(&conn.address, conn.port, &conn.username, &conn.password);
            drop(conn);

            let table_data_clone_edit = table_data_clone_edit.clone();

            let save_host = move |_: &mut _, host: config::Host| {
                table_data_clone_edit
                    .write()
                    .unwrap()
                    .connections
                    .insert(id, Connection::from(host.clone()));

                let mut cfg = config::write();
                cfg.connection_manager.hosts.insert(id, host);
                cfg.save();
            };

            let dialog = view
                .into_dialog("Cancel", "Save", save_host)
                .title("Edit Host");

            siv.add_layer(dialog)
        };

        let buttons = LinearLayout::horizontal()
            .child(Button::new("Add", |_| ()))
            .child(Button::new("Edit", edit_button))
            .child(Button::new("Remove", |_| ()))
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

impl Form for ConnectionManagerView {
    type Data = Option<(Uuid, Arc<Session>, String, String)>;

    fn into_data(self) -> Self::Data {
        let Self { mut inner } = self;

        let table = inner
            .remove_child(0)
            .unwrap()
            .downcast::<TableView<ConnectionTableData>>()
            .ok()
            .unwrap();

        let data: Arc<RwLock<ConnectionTableData>> = table.get_data();

        // TODO: Save prefs BEFORE THIS POINT.
        // Starting now, there will be early returns.
        let selected: Uuid = table.get_selection().copied()?;

        drop(table);
        let mut data = Arc::try_unwrap(data).ok().unwrap().into_inner().unwrap();

        let connection = data
            .connections
            .remove(&selected)
            .expect("No selection; the connection button ought to be disabled.");

        if data.current_host.contains(&selected) {
            assert!(connection.session.read().unwrap().is_some());
            None // Disconnect from current session
        } else if let Some(session) = connection.session.write().unwrap().take() {
            assert_eq!(Arc::strong_count(&session), 1);
            Some((selected, session, connection.username, connection.password))
        } else {
            todo!("No successfully connected session; the connect button should be disabled.")
        }
    }
}
