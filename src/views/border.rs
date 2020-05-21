use cursive::traits::*;
use cursive::vec::Vec2;
use cursive::Printer;
use cursive::view::ViewWrapper;

pub(crate) struct BottomBorderedView<V: View> {
    inner: V,
    border: &'static str,
}

impl<V: View> ViewWrapper for BottomBorderedView<V> {
    cursive::wrap_impl!(self.inner: V);

    fn wrap_required_size(&mut self, constraint: Vec2) -> Vec2 {
        self.inner.required_size(constraint - (0, 1)) + (0, 1)
    }

    fn wrap_layout(&mut self, size: Vec2) {
        self.inner.layout(size - (0, 1));
    }

    fn wrap_draw(&self, printer: &Printer) {
        self.inner.draw(&printer.shrinked((0, 1)));
        printer.print_hline((0, printer.size.y - 1), printer.output_size.x, self.border);
    }
}

pub(crate) trait Borderable: View + Sized {
    fn with_bottom_border(self, border: &'static str) -> BottomBorderedView<Self> {
        BottomBorderedView { inner: self, border }
    }
}

impl<V: View> Borderable for V {}

pub(crate) struct VerticalBorderView(pub &'static str);

impl View for VerticalBorderView {
    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        Vec2 { x: 1, y: constraint.y }
    }

    fn draw(&self, printer: &Printer) {
        printer.print_vline((0, 0), printer.output_size.y, self.0);
    }
}
