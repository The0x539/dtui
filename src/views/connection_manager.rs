use std::cell::Cell;
use std::cmp::{Ordering, PartialEq};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use super::{
    edit_host::EditHostView,
    labeled_checkbox::LabeledCheckbox,
    static_linear_layout::StaticLinearLayout,
    table::{TableCallback, TableView, TableViewData},
};
use crate::config;
use crate::form::Form;
use crate::util::eventual::Eventual;
use crate::SessionHandle;

use tokio::sync::oneshot;
use tokio::task;

use deluge_rpc::Session;

use cursive::{
    event::Callback,
    view::ViewWrapper,
    views::{Button, DummyView, Panel},
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
        let (session, ses_tx) = Eventual::new();
        let (version, ver_tx) = Eventual::new();
        let fut = connect(host.address.clone(), host.port, ses_tx, ver_tx);
        task::spawn(fut);

        Self {
            address: host.address.clone(),
            port: host.port,
            username: host.username.clone(),
            password: host.password.clone(),
            version,
            session,
        }
    }

    fn existing(host: &config::Host, ses: Arc<Session>) -> Self {
        let (version, mut ver_tx) = Eventual::new();
        let session = Eventual::ready(ses.clone());

        let fut = async move {
            tokio::select! {
                result = ses.daemon_info() => match result {
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
            version,
            session,
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
    #[allow(dead_code)]
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
                if self.get_current_host() == Some(connection) {
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

type ConnectionManagerButtons =
    StaticLinearLayout<(Button, Button, Button, Button, DummyView, Button)>;

type StartupOptions = StaticLinearLayout<(LabeledCheckbox, LabeledCheckbox)>;

type ConnectionManagerLayout = StaticLinearLayout<(
    TableView<ConnectionTableData>,
    ConnectionManagerButtons,
    Panel<StartupOptions>,
)>;

pub(crate) struct ConnectionManagerView {
    inner: ConnectionManagerLayout,
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

fn selection_change_cb(
    selected_connection: Rc<Cell<Option<Uuid>>>,
) -> impl TableCallback<ConnectionTableData> {
    move |_: &mut _, id: &Uuid, _, _| {
        selected_connection.set(Some(*id));
        Callback::dummy()
    }
}

fn add_button_cb(table_data: Arc<RwLock<ConnectionTableData>>) -> impl Fn(&mut Cursive) {
    move |siv: &mut Cursive| {
        let table_data = table_data.clone();

        let save_host = move |_: &mut _, host: config::Host| {
            let id = Uuid::new_v4();

            let mut data = table_data.write().unwrap();

            data.connections.insert(id, Connection::new(&host));
            data.rows.push(id);

            let mut cfg = config::write();
            cfg.connection_manager.hosts.insert(id, host);
            cfg.save();
        };

        let dialog = EditHostView::default()
            .into_dialog("Cancel", "Save", save_host)
            .title("Add Host");

        siv.add_layer(dialog)
    }
}

fn edit_button_cb(
    table_data: Arc<RwLock<ConnectionTableData>>,
    selected_connection: Rc<Cell<Option<Uuid>>>,
) -> impl Fn(&mut Cursive) {
    let cb = move |siv: &mut Cursive| {
        let id = selected_connection
            .get()
            .expect("No selection; edit button should be disabled");

        let conn = &table_data.read().unwrap().connections[&id];
        let view = EditHostView::new(&conn.address, conn.port, &conn.username, &conn.password);
        drop(conn);

        let table_data = table_data.clone();

        let save_host = move |_: &mut _, host: config::Host| {
            table_data
                .write()
                .unwrap()
                .connections
                .insert(id, Connection::new(&host));

            let mut cfg = config::write();
            cfg.connection_manager.hosts.insert(id, host);
            cfg.save();
        };

        let dialog = view
            .into_dialog("Cancel", "Save", save_host)
            .title("Edit Host");

        siv.add_layer(dialog);
    };

    cursive::immut1!(cb)
}

fn remove_button_cb(
    table_data: Arc<RwLock<ConnectionTableData>>,
    selected_connection: Rc<Cell<Option<Uuid>>>,
) -> impl Fn(&mut Cursive) {
    move |_| {
        let id = selected_connection
            .get()
            .expect("No selection; remove button should be disabled");

        let mut data = table_data.write().unwrap();

        if data.current_host == Some(id) {
            data.current_host = None;
        }

        data.connections
            .remove(&id)
            .expect("Tried to remove nonexistent connection");
    }
}

impl ConnectionManagerView {
    pub fn new(current_host: SessionHandle) -> Self {
        let cfg = config::read();
        let cmgr = &cfg.connection_manager;

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

        table.set_on_selection_change(selection_change_cb(Rc::clone(&selected_connection)));

        let table_data = table.get_data();

        let mut data = table_data.write().unwrap();

        data.rows = cmgr.hosts.keys().copied().collect();
        let len = data.rows.len();
        data.connections.reserve(len);
        data.autoconnect_host = autoconnect_host;

        let current_id = current_host.get_id();
        data.current_host = current_id;

        for (id, host) in &cmgr.hosts {
            let conn = if current_id == Some(*id) {
                let session = current_host.get_session().unwrap().clone();
                Connection::existing(host, session)
            } else {
                Connection::new(host)
            };

            data.connections.insert(*id, conn);
        }

        drop(data);
        drop(cfg);

        let add_button = add_button_cb(table_data.clone());
        let edit_button = edit_button_cb(table_data.clone(), selected_connection.clone());
        let remove_button = remove_button_cb(table_data, selected_connection);

        let buttons = ConnectionManagerButtons::horizontal((
            Button::new("Add", add_button),
            Button::new("Edit", edit_button),
            Button::new("Remove", remove_button),
            Button::new("Refresh", |_| ()),
            DummyView,
            Button::new("Stop Daemon", |_| ()),
        ));

        let startup_options = {
            let content = StartupOptions::vertical((
                LabeledCheckbox::new("Auto-connect to selected daemon").with_checked(auto_connect),
                LabeledCheckbox::new("Hide this dialog").with_checked(hide_dialog),
            ));

            Panel::new(content).title("Startup Options")
        };

        let inner = ConnectionManagerLayout::vertical((table, buttons, startup_options));
        Self { inner }
    }
}

impl ViewWrapper for ConnectionManagerView {
    cursive::wrap_impl!(self.inner: ConnectionManagerLayout);
}

impl Form for ConnectionManagerView {
    type Data = Option<(Uuid, Arc<Session>, String, String)>;

    fn into_data(self) -> Self::Data {
        let table: TableView<ConnectionTableData> = self.inner.into_children().0;
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

        if data.current_host == Some(selected) {
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
