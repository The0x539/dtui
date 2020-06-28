use crate::config::Host;
use crate::form::Form;

use crate::views::{linear_panel::LinearPanel, spin::SpinView};

use cursive::view::ViewWrapper;
use cursive::views::{LinearLayout, TextArea, TextView};

type PortSpinView = SpinView<u16, std::ops::RangeFull>;

pub struct EditHostView {
    inner: LinearPanel,
}

impl EditHostView {
    pub fn new(hostname: &str, port: u16, username: &str, password: &str) -> Self {
        let host_row = LinearLayout::horizontal()
            .child(TextView::new("Hostname: "))
            .child(TextArea::new().content(hostname))
            .child(SpinView::new(Some("Port: "), None, ..).with_val(port));

        let username_row = LinearLayout::horizontal()
            .child(TextView::new("Username: "))
            .child(TextArea::new().content(username));

        let password_row = LinearLayout::horizontal()
            .child(TextView::new("Password: "))
            .child(TextArea::new().content(password));

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

fn take_row(rows: &mut LinearPanel, index: usize) -> Box<LinearLayout> {
    rows.remove_child(index)
        .unwrap()
        .downcast::<LinearLayout>()
        .ok()
        .unwrap()
}

fn take_content<T: Form>(row: &mut LinearLayout, index: usize) -> T::Data {
    row.remove_child(index)
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

        let mut password_row = take_row(&mut inner, 2);
        let mut username_row = take_row(&mut inner, 1);
        let mut host_row = take_row(&mut inner, 0);

        let username: String = take_content::<TextArea>(&mut username_row, 1);
        let password: String = take_content::<TextArea>(&mut password_row, 1);
        let port: u16 = take_content::<PortSpinView>(&mut host_row, 2);
        let address: String = take_content::<TextArea>(&mut host_row, 1);

        Host {
            address,
            port,
            username,
            password,
        }
    }
}
