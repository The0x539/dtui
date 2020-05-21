use cursive::traits::*;
use cursive::vec::Vec2;
use cursive::Printer;

pub(crate) enum BorderType { Vertical, Horizontal, Cell }
pub(crate) struct BorderView(pub BorderType, pub &'static str);

impl View for BorderView {
    fn draw(&self, printer: &Printer) {
        match self.0 {
            BorderType::Vertical => printer.print_vline((0, 0), printer.output_size.y, self.1),
            BorderType::Horizontal => printer.print_hline((0, 0), printer.output_size.x, self.1),
            BorderType::Cell => printer.print((0, 0), self.1),
        }
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        match self.0 {
            BorderType::Vertical => (1, constraint.y),
            BorderType::Horizontal => (constraint.x, 1),
            BorderType::Cell => (1, 1),
        }.into()
    }
}
