use cursive::views::LinearLayout;
use cursive::view::{View, ViewWrapper};
use cursive::direction::Orientation;
use cursive::Printer;
use cursive::event::{Event, EventResult};
use cursive::vec::Vec2;

struct Child<V> {
    inner: V,
    orientation: Orientation,
    title: Option<String>,
}

impl<V: View> Child<V> {
    fn extra_size(&self) -> Vec2 {
        self.orientation.make_vec(1, 2)
    }

    fn inner_size(&self, size: Vec2) -> Vec2 {
        size.saturating_sub(self.extra_size())
    }
}

impl<V: View> ViewWrapper for Child<V> {
    cursive::wrap_impl!(self.inner: V);

    fn wrap_draw(&self, printer: &Printer) {
        let Vec2 {x: px, y: py} = printer.size;
        let (px1, py1) = (px.saturating_sub(1), py.saturating_sub(1));
        match self.orientation {
            Orientation::Vertical => {
                printer.print_vline((0, 0), px, "│");
                printer.print_vline((px1, 0), py, "│");
                printer.print_hdelim((0, 0), px);
            },
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
            printer.offset((1, 0)).shrinked(shrinkage).print((0, 0), &text);
        }

        self.inner.draw(&printer.offset((1, 1)).shrinked(shrinkage));
    }

    fn wrap_required_size(&mut self, req: Vec2) -> Vec2 {
        let mut req = self.inner.required_size(self.inner_size(req));
        if let Some(title) = &self.title {
            req.x = req.x.max(title.len() + 2);
        }
        req + self.extra_size()
    }

    fn wrap_layout(&mut self, size: Vec2) {
        self.inner.layout(size.saturating_sub(self.extra_size()))
    }

    fn wrap_on_event(&mut self, event: Event) -> EventResult {
        if let Event::Mouse { offset, position, .. } = event {
            if !position.saturating_sub(offset).strictly_gt((0, 0)) {
                return EventResult::Ignored;
            }
        }
        self.inner.on_event(event.relativized((1, 1)))
    }
}

pub struct LinearPanel {
    inner: LinearLayout,
    orientation: Orientation,
}

impl LinearPanel {
    pub fn new(orientation: Orientation) -> Self {
        Self {
            inner: LinearLayout::new(orientation),
            orientation,
        }
    }

    #[allow(dead_code)]
    pub fn horizontal() -> Self { Self::new(Orientation::Horizontal) }

    pub fn vertical()   -> Self { Self::new(Orientation::Vertical)   }

    pub fn add_child(&mut self, view: impl View, title: Option<&str>) {
        let child = Child {
            inner: view,
            orientation: self.orientation,
            title: title.map(String::from),
        };
        self.inner.add_child(child);
    }

    pub fn child(mut self, view: impl View, title: Option<&str>) -> Self {
        self.add_child(view, title);
        self
    }
}

impl ViewWrapper for LinearPanel {
    cursive::wrap_impl!(self.inner: LinearLayout);

    fn wrap_required_size(&mut self, req: Vec2) -> Vec2 {
        let extra = self.orientation.make_vec(1, 0);
        self.inner.required_size(req.saturating_sub(extra)) + extra
    }

    fn wrap_layout(&mut self, size: Vec2) {
        let extra = self.orientation.make_vec(1, 0);
        self.inner.layout(size.saturating_sub(extra))
    }

    fn wrap_draw(&self, printer: &Printer) {
        let extra = self.orientation.make_vec(1, 0);
        self.inner.draw(&printer.shrinked(extra));
        let (x, y) = printer.size.saturating_sub((1, 1)).pair();
        printer.print_hline((0, y), x, "─");
        for (pos, ch) in Iterator::zip(
            [(0, 0), (x, 0),
             (0, y), (x, y)].iter(),
            ["┌", "┐",
             "└", "┘"].iter(),
        ) {
            printer.print(*pos, ch);
        }
    }
}
