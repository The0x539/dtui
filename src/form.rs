use std::rc::Rc;

use cursive::view::{View, ViewWrapper};
use cursive::views::{Dialog, EditView, ResizedView, TextArea};
use cursive::Cursive;

pub trait Form: View + Sized + 'static {
    type Data;

    fn into_data(self) -> Self::Data;

    fn into_dialog(
        self,
        dismiss_label: impl Into<String>,
        submit_label: impl Into<String>,
        on_submit: impl FnOnce(&mut Cursive, Self::Data) + 'static,
    ) -> Dialog {
        let cb = {
            let mut f = Some(on_submit);
            move |siv: &mut Cursive| {
                if let Some(f) = f.take() {
                    let dialog: Box<Dialog> = siv
                        .pop_layer()
                        .expect("no layer")
                        .downcast::<Dialog>()
                        .ok()
                        .expect("top layer wasn't a Dialog");

                    let form: Box<Self> = dialog
                        .into_content()
                        .downcast::<Self>()
                        .ok()
                        .expect("dialog's contents weren't Self");

                    let data = form.into_data();

                    f(siv, data);
                } else {
                    unreachable!("submit callback was called twice");
                }
            }
        };

        Dialog::around(self)
            .button(submit_label, cursive::immut1!(cb))
            .dismiss_button(dismiss_label)
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
