use cursive::view::ViewWrapper;
use cursive::views::{PaddedView, Checkbox};
use cursive::Printer;
use cursive::event::EventResult;
use cursive::Cursive;

use crate::form::Form;

pub struct LabeledCheckbox {
    inner: PaddedView<Checkbox>,
    label: String,
}

impl ViewWrapper for LabeledCheckbox {
    cursive::wrap_impl!(self.inner: PaddedView<Checkbox>);

    fn wrap_draw(&self, printer: &Printer) {
        self.inner.wrap_draw(printer);
        printer.print((4, 0), &self.label);
    }
}

#[allow(unused)]
impl LabeledCheckbox {
    pub fn new(label: impl Into<String>) -> Self {
        let label: String = label.into();
        let inner = PaddedView::lrtb(0, label.len() + 1, 0, 0, Checkbox::new());
        Self { inner, label }
    }

    pub fn disable(&mut self) {
        self.inner.get_inner_mut().disable()
    }

    pub fn disabled(mut self) -> Self {
        self.disable(); self
    }

    pub fn enable(&mut self) {
        self.inner.get_inner_mut().enable()
    }

    pub fn enabled(mut self) -> Self {
        self.enable(); self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.inner.get_inner_mut().set_enabled(enabled)
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.set_enabled(enabled); self
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.get_inner().is_enabled()
    }

    pub fn set_on_change<F: 'static + Fn(&mut Cursive, bool)>(&mut self, on_change: F) {
        self.inner.get_inner_mut().set_on_change(on_change)
    }

    pub fn on_change<F: 'static + Fn(&mut Cursive, bool)>(mut self, on_change: F) -> Self {
        self.set_on_change(on_change); self
    }

    pub fn toggle(&mut self) -> EventResult {
        self.inner.get_inner_mut().check()
    }

    pub fn check(&mut self) -> EventResult {
        self.inner.get_inner_mut().check()
    }

    pub fn checked(mut self) -> Self {
        self.check(); self
    }

    pub fn is_checked(&self) -> bool {
        self.inner.get_inner().is_checked()
    }

    pub fn uncheck(&mut self) -> EventResult {
        self.inner.get_inner_mut().uncheck()
    }

    pub fn unchecked(mut self) -> Self {
        self.uncheck(); self
    }

    pub fn set_checked(&mut self, checked: bool) -> EventResult {
        self.inner.get_inner_mut().set_checked(checked)
    }

    pub fn with_checked(mut self, checked: bool) -> Self {
        self.set_checked(checked); self
    }
}

impl Form for LabeledCheckbox {
    type Data = bool;

    fn into_data(self) -> Self::Data {
        self.is_checked()
    }
}
