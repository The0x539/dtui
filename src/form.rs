use std::rc::Rc;

use cursive::Cursive;
use cursive::view::{View, Resizable};
use cursive::views::{Dialog, EditView, TextArea, ResizedView};

pub trait Form: View + Sized + 'static {
    type Data;

    fn into_data(self) -> Self::Data;

    fn replacement() -> Self;

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
                    let mut dialog: Box<Dialog> = siv
                        .pop_layer()
                        .expect("no layer")
                        .downcast::<Dialog>()
                        .ok()
                        .expect("top layer wasn't a Dialog");

                    let form_ref: &mut Self = dialog
                        .get_content_mut()
                        .downcast_mut::<Self>()
                        .expect("dialog's contents weren't Self");

                    let form = std::mem::replace(form_ref, Self::replacement());

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

    fn replacement() -> Self { Self::default() }

    fn into_data(self) -> Self::Data {
        let content = self.get_content();
        std::mem::drop(self);
        assert_eq!(Rc::strong_count(&content), 1);
        Rc::try_unwrap(content).unwrap()
    }
}

impl Form for TextArea {
    type Data = String;

    fn replacement() -> Self { Self::default() }

    fn into_data(self) -> Self::Data {
        String::from(self.get_content())
    }
}

// This would be generic across all implementors of ViewWrapper, but rustc complains.
impl<V: Form + Default> Form for ResizedView<V> {
    type Data = V::Data;

    fn replacement() -> Self { V::default().min_height(0) }

    fn into_data(mut self) -> Self::Data {
        std::mem::take(self.get_inner_mut()).into_data()
    }
}
