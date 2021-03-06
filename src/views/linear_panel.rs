use cursive::direction::Orientation;
use cursive::vec::Vec2;
use cursive::view::{IntoBoxedView, View, ViewWrapper};
use cursive::views::{BoxedView, LinearLayout, PaddedView};
use cursive::Printer;

type PaddedBoxedView = PaddedView<BoxedView>;

struct Child {
    inner: PaddedBoxedView,
    orientation: Orientation,
    title: Option<String>,
}

impl Child {
    fn new(view: impl IntoBoxedView, orientation: Orientation, title: Option<String>) -> Self {
        let (l, r, t, b) = match orientation {
            Orientation::Vertical => (1, 1, 1, 0),
            Orientation::Horizontal => (1, 0, 1, 1),
        };
        let inner = PaddedView::lrtb(l, r, t, b, BoxedView::boxed(view));
        Self {
            inner,
            orientation,
            title,
        }
    }
}

impl ViewWrapper for Child {
    cursive::wrap_impl!(self.inner: PaddedBoxedView);

    fn wrap_draw(&self, printer: &Printer) {
        let Vec2 { x: px, y: py } = printer.size;
        let (px1, py1) = (px.saturating_sub(1), py.saturating_sub(1));
        match self.orientation {
            Orientation::Vertical => {
                printer.print_vline((0, 0), px, "│");
                printer.print_vline((px1, 0), py, "│");
                printer.print_hdelim((0, 0), px);
            }
            Orientation::Horizontal => {
                printer.print_hline((0, 0), px, "─");
                printer.print_hline((0, py1), px, "─");
                printer.print_vline((0, 0), py, "│");
                printer.print((0, 0), "┬");
                printer.print((0, py), "┴");
            }
        }

        let shrinkage = self.orientation.make_vec(0, 1);

        if let Some(title) = &self.title {
            let text = format!("┤{}├", title);
            printer
                .offset((1, 0))
                .shrinked(shrinkage)
                .print((0, 0), &text);
        }

        self.inner.draw(printer)
    }

    fn wrap_required_size(&mut self, req: Vec2) -> Vec2 {
        let mut req = self.inner.required_size(req);
        if let Some(title) = &self.title {
            req.x = req.x.max(title.len() + 4);
        }
        req
    }
}

pub struct LinearPanel {
    inner: PaddedView<LinearLayout>,
    orientation: Orientation,
}

impl LinearPanel {
    pub fn new(orientation: Orientation) -> Self {
        let (l, r, t, b) = match orientation {
            Orientation::Vertical => (0, 0, 0, 1),
            Orientation::Horizontal => (0, 1, 0, 0),
        };
        let inner = PaddedView::lrtb(l, r, t, b, LinearLayout::new(orientation));
        Self { inner, orientation }
    }

    #[allow(dead_code)]
    pub fn horizontal() -> Self {
        Self::new(Orientation::Horizontal)
    }

    pub fn vertical() -> Self {
        Self::new(Orientation::Vertical)
    }

    pub fn add_child(&mut self, view: impl IntoBoxedView, title: Option<&str>) {
        let child = Child::new(view, self.orientation, title.map(String::from));
        self.inner.get_inner_mut().add_child(child);
    }

    pub fn child(mut self, view: impl IntoBoxedView, title: Option<&str>) -> Self {
        self.add_child(view, title);
        self
    }

    pub fn remove_child(&mut self, i: usize) -> Option<Box<dyn View>> {
        let child_box = self.inner.get_inner_mut().remove_child(i)?;
        let child_view = child_box.downcast::<Child>().ok().unwrap();
        let padded = child_view.into_inner().ok().unwrap();
        let boxed = padded.into_inner().ok().unwrap();
        Some(BoxedView::unwrap(boxed))
    }
}

impl ViewWrapper for LinearPanel {
    cursive::wrap_impl!(self.inner: PaddedView<LinearLayout>);

    fn wrap_draw(&self, printer: &Printer) {
        self.inner.draw(printer);

        let (x, y) = printer.size.saturating_sub((1, 1)).pair();

        printer.print_hline((0, y), x, "─");

        for (pos, ch) in Iterator::zip(
            [(0, 0), (x, 0), (0, y), (x, y)].iter(),
            ["┌", "┐", "└", "┘"].iter(),
        ) {
            printer.print(*pos, ch);
        }
    }
}
