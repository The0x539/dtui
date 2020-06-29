use crate::config::Host;
use crate::form::Form;

use crate::views::{
    linear_panel::LinearPanel, spin::SpinView, static_linear_layout::StaticLinearLayout,
};

use cursive::view::ViewWrapper;
use cursive::views::{TextArea, TextView};

type PortSpinView = SpinView<u16, std::ops::RangeFull>;

pub struct EditHostView {
    inner: LinearPanel,
}

type HostRow = StaticLinearLayout<(TextView, TextArea, PortSpinView)>;

impl Form for HostRow {
    type Data = (String, u16);

    fn into_data(self) -> Self::Data {
        let children = self.into_children();
        (children.1.into_data(), children.2.into_data())
    }
}

type TextRow = StaticLinearLayout<(TextView, TextArea)>;

impl Form for TextRow {
    type Data = String;

    fn into_data(self) -> Self::Data {
        self.into_children().1.into_data()
    }
}

impl EditHostView {
    pub fn new(hostname: &str, port: u16, username: &str, password: &str) -> Self {
        let host_row = HostRow::horizontal((
            TextView::new("Hostname: "),
            TextArea::new().content(hostname),
            SpinView::new(Some("Port: "), None, ..).with_val(port),
        ));

        let username_row = TextRow::horizontal((
            TextView::new("Username: "),
            TextArea::new().content(username),
        ));

        let password_row = TextRow::horizontal((
            TextView::new("Password: "),
            TextArea::new().content(password),
        ));

        let inner = LinearPanel::vertical()
            .child(host_row, None)
            .child(username_row, None)
            .child(password_row, None);

        Self { inner }
    }
}

impl From<&Host> for EditHostView {
    fn from(value: &Host) -> Self {
        Self::new(&value.address, value.port, &value.username, &value.password)
    }
}

impl Default for EditHostView {
    fn default() -> Self {
        Self::from(&Host::default())
    }
}

impl ViewWrapper for EditHostView {
    cursive::wrap_impl!(self.inner: LinearPanel);
}

fn take_row_content<T: Form>(rows: &mut LinearPanel, index: usize) -> T::Data {
    rows.remove_child(index)
        .unwrap()
        .downcast::<T>()
        .ok()
        .unwrap()
        .into_data()
}

impl Form for EditHostView {
    type Data = Host;

    fn into_data(self) -> Self::Data {
        let mut inner = self.inner;

        let password = take_row_content::<TextRow>(&mut inner, 2);
        let username = take_row_content::<TextRow>(&mut inner, 1);
        let (address, port) = take_row_content::<HostRow>(&mut inner, 0);

        Host {
            address,
            port,
            username,
            password,
        }
    }
}
