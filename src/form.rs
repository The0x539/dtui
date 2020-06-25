use std::rc::Rc;

use cursive::view::{View, ViewWrapper};
use cursive::views::{Dialog, EditView, ResizedView, TextArea};
use cursive::Cursive;

fn make_cb<T, F>(f: F) -> impl Fn(&mut Cursive)
where
    T: Form,
    F: FnOnce(&mut Cursive, T::Data),
{
    let mut f = Some(f);
    let cb = move |siv: &mut Cursive| {
        let f = match f.take() {
            Some(f) => f,
            None => return,
        };

        let dialog: Box<Dialog> = siv
            .pop_layer()
            .expect("no layer")
            .downcast::<Dialog>()
            .ok()
            .expect("top layer wasn't a Dialog");

        let form: Box<T> = dialog
            .into_content()
            .downcast::<T>()
            .ok()
            .expect("dialog's contents weren't Self");

        f(siv, form.into_data());
    };
    cursive::immut1!(cb)
}

pub trait Form: View + Sized + 'static {
    type Data;

    fn into_data(self) -> Self::Data;

    fn into_dialog(
        self,
        dismiss_label: impl Into<String>,
        submit_label: impl Into<String>,
        on_submit: impl FnOnce(&mut Cursive, Self::Data) + 'static,
    ) -> Dialog {
        Dialog::around(self)
            .button(submit_label, make_cb::<Self, _>(on_submit))
            .dismiss_button(dismiss_label)
    }

    fn into_dialog_custom_dismiss(
        self,
        dismiss_label: impl Into<String>,
        submit_label: impl Into<String>,
        on_submit: impl FnOnce(&mut Cursive, Self::Data) + 'static,
        on_dismiss: impl FnOnce(&mut Cursive, Self::Data) + 'static,
    ) -> Dialog {
        Dialog::around(self)
            .button(submit_label, make_cb::<Self, _>(on_submit))
            .button(dismiss_label, make_cb::<Self, _>(on_dismiss))
    }
}

impl Form for EditView {
    type Data = String;

    fn into_data(self) -> Self::Data {
        let content = self.get_content();
        std::mem::drop(self);
        assert_eq!(Rc::strong_count(&content), 1);
        Rc::try_unwrap(content).unwrap()
    }
}

impl Form for TextArea {
    type Data = String;

    fn into_data(self) -> Self::Data {
        String::from(self.get_content())
    }
}

// This would be generic across all implementors of ViewWrapper, but rustc complains.
impl<V: Form> Form for ResizedView<V> {
    type Data = V::Data;

    fn into_data(self) -> Self::Data {
        self.into_inner().ok().unwrap().into_data()
    }
}
