use std::cmp::{PartialEq, Ordering};
use std::sync::{Arc, RwLock};
use std::ops::Deref;

use super::{
    table::{TableViewData, TableView},
    labeled_checkbox::LabeledCheckbox,
};
use crate::config;
use crate::form::Form;
use crate::SessionHandle;

use deluge_rpc::Session;

use cursive::{
    Printer,
    views::{Button, LinearLayout, Panel, DummyView},
    view::ViewWrapper,
};
use uuid::Uuid;

type FnvIndexMap<K, V> = indexmap::IndexMap<K, V, fnv::FnvBuildHasher>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column { Status, Host, Version }
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
        let config::Host { username, password, port, address } = val;
        let (version, session) = Default::default();
        Self { username, password, port, address, version, session }
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

    fn sort_column(&self) -> Self::Column { Column::Host }
    fn descending_sort(&self) -> bool { true }

    fn rows(&self) -> &Self::Rows { &self.rows }
    fn rows_mut(&mut self) -> &mut Self::Rows { &mut self.rows }
    fn set_rows(&mut self, val: Self::Rows) { self.rows = val; }

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
            },
            Column::Host => print(&format!("{}@{}:{}", connection.username, connection.address, connection.port)),
            Column::Version => {
                if let Some(s) = connection.version.read().unwrap().deref() {
                    print(s);
                }
            },
        }
    }
}

pub(crate) struct ConnectionManagerView {
    inner: LinearLayout,
}

impl ConnectionManagerView {
    pub fn new(current_host: SessionHandle) -> Self {
        let cfg = config::get_config();
        let cmgr = &cfg.read().unwrap().connection_manager;

        let connections: FnvIndexMap<Uuid, Connection> = cmgr.hosts
            .iter()
            .map(|(id, host)| (*id, host.clone().into()))
            .collect();

        let autoconnect_host = None; // TODO: read from config
        let hide_dialog = false; // TODO: read from config

        let auto_connect = current_host.as_ref().map(|x| x.0) == autoconnect_host;

        let cols = vec![(Column::Status, 9), (Column::Host, 50), (Column::Version, 11)];
        let table = TableView::<ConnectionTableData>::new(cols);
        let table_data = table.get_data();
        {
            let mut data = table_data.write().unwrap();

            data.rows = connections.keys().copied().collect();
            data.connections = connections;
            data.autoconnect_host = autoconnect_host;
            if let Some((id, session)) = current_host {
                data.current_host = Some(id);
                data.connections[&id].session.write().unwrap().replace(session);
            }
        }

        let buttons = LinearLayout::horizontal()
            .child(Button::new("Add", |_| ()))
            .child(Button::new("Edit", |_| ()))
            .child(Button::new("Remove", |_| ()))
            .child(Button::new("Refresh", |_| ()))
            .child(DummyView)
            .child(Button::new("Stop Daemon", |_| ()));

        let startup_options = {
            let auto_connect_checkbox = LabeledCheckbox::new("Auto-connect to selected daemon")
                .with_checked(auto_connect);

            let hide_dialog_checkbox = LabeledCheckbox::new("Hide this dialog")
                .with_checked(hide_dialog);

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
    type Data = SessionHandle;

    fn into_data(self) -> Self::Data {
        let Self { mut inner } = self;

        let table = inner
            .remove_child(0)
            .unwrap()
            .downcast::<TableView<ConnectionTableData>>()
            .ok()
            .unwrap();

        let data = table.get_data();

        // TODO: Save prefs BEFORE THIS POINT.
        // Starting now, there will be early returns.
        let selected: Uuid = table.get_selection().copied()?;

        std::mem::drop(table);
        let data = Arc::try_unwrap(data) // Arc<RwLock<T>> -> Result<RwLock<T>, Arc<RwLock<T>>>
            .ok()                        // Result<RwLock<T>, Arc<RwLock<T>>> -> Option<RwLock<T>>
            .unwrap()                    // Option<RwLock<T>> -> RwLock<T>
            .into_inner()                // RwLock<T> -> LockResult<T>
            .unwrap();                   // LockResult<T> -> T

        if data.current_host.contains(&selected) {
            assert!(data.connections[&selected].session.read().unwrap().is_some());
            None // Disconnect from current session
        } else if let Some(session) = data.connections[&selected].session.read().unwrap().clone() {
            Some((selected, session))
        } else {
            todo!("The connect button should be disabled.")
        }
    }
}
