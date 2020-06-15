use std::cmp::{PartialEq, Ordering};
use std::net::SocketAddr;
use std::sync::Arc;

use std::fmt;

use super::{
    table::{TableViewData, TableView},
    labeled_checkbox::LabeledCheckbox,
};
use crate::form::Form;

use deluge_rpc::Session;

use cursive::{
    Printer,
    views::{Button, LinearLayout, Panel, DummyView},
    view::ViewWrapper,
};
use serde::{Serialize, Deserialize};
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

#[derive(PartialEq, Eq, Serialize, Deserialize)]
enum HostAddr {
    Address(SocketAddr),
    Domain(String, Option<u16>),
}

impl fmt::Display for HostAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Address(addr) => write!(f, "{}", addr),
            Self::Domain(domain, None) => f.write_str(domain),
            Self::Domain(domain, Some(port)) => write!(f, "{}:{}", domain, port),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Connection {
    username: String,
    password: String, // ¯\_(ツ)_/¯
    host: HostAddr,
    #[serde(skip)]
    version: Option<String>,

    #[serde(skip)]
    session: Option<Arc<Session>>,
}

impl PartialEq<Self> for Connection {
    fn eq(&self, other: &Self) -> bool {
        (&self.username, &self.host) == (&other.username, &other.host)
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
                    assert!(connection.session.is_some());
                    print("Connected");
                } else if connection.session.is_some() {
                    print("Online");
                } else {
                    print("Offline");
                }
            },
            Column::Host => print(&format!("{}@{}", connection.username, connection.host)),
            Column::Version => { connection.version.as_ref().map(|s| print(s)); },
        }
    }
}

pub(crate) struct ConnectionManagerView {
    inner: LinearLayout,
}

impl ConnectionManagerView {
    #[allow(dead_code)]
    fn new(current_host: Option<(Uuid, Arc<Session>)>) -> Self {
        let connections: FnvIndexMap<Uuid, Connection> = Default::default(); // TODO: read from config
        let autoconnect_host = None; // TODO: read from config
        let hide_dialog = false; // TODO: read from config

        let auto_connect = current_host.as_ref().map(|x| x.0) == autoconnect_host;

        let cols = vec![(Column::Status, 9), (Column::Host, 50), (Column::Version, 11)];
        let table = TableView::<ConnectionTableData>::new(cols);
        {
            let table_data = table.get_data();
            let mut data = table_data.write().unwrap();

            data.rows = connections.keys().copied().collect();
            data.connections = connections;
            data.autoconnect_host = autoconnect_host;
            if let Some((id, session)) = current_host {
                data.current_host = Some(id);
                data.connections[&id].session = Some(session);
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
    type Data = Option<(Uuid, Arc<Session>)>;

    fn replacement() -> Self {
        Self { inner: LinearLayout::vertical() }
    }

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
            assert!(data.connections[&selected].session.is_some());
            None // Disconnect from current session
        } else if let Some(session) = data.connections[&selected].session.clone() {
            Some((selected, session))
        } else {
            todo!("The connect button should be disabled.")
        }
    }
}
