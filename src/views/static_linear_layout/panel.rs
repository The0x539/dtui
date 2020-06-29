use super::StaticLinearLayout;

use cursive::{
    direction::Orientation,
    view::{View, ViewWrapper},
    views::PaddedView,
    Printer, Vec2,
};

pub struct Child<T> {
    inner: PaddedView<T>,
    orientation: Orientation,
    title: Option<String>,
}

impl<T: View> Child<T> {
    fn new(view: T, orientation: Orientation, title: Option<String>) -> Self {
        let (l, r, t, b) = match orientation {
            Orientation::Vertical => (1, 1, 1, 0),
            Orientation::Horizontal => (1, 0, 1, 1),
        };
        let inner = PaddedView::lrtb(l, r, t, b, view);
        Self {
            inner,
            orientation,
            title,
        }
    }

    #[allow(dead_code)]
    pub fn get_inner(&self) -> &T {
        self.inner.get_inner()
    }

    pub fn get_inner_mut(&mut self) -> &mut T {
        self.inner.get_inner_mut()
    }

    #[allow(dead_code)]
    pub fn into_inner(self) -> T {
        self.inner.into_inner().ok().unwrap()
    }
}

impl<T: View> ViewWrapper for Child<T> {
    cursive::wrap_impl!(self.inner: PaddedView<T>);

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

pub struct StaticLinearPanel<T> {
    inner: PaddedView<StaticLinearLayout<T>>,
    //orientation: Orientation,
}

macro_rules! impls {
    ($($(@$n:tt $name:ident)+),+$(,)?) => {
        $(
            impl<$($name),+> StaticLinearPanel<($(Child<$name>,)+)>
            where
                $($name: View),+
            {
                pub fn new_with_titles(orientation: Orientation, children: ($(($name, Option<String>),)+)) -> Self {
                    let (l, r, t, b) = match orientation {
                        Orientation::Vertical => (0, 0, 0, 1),
                        Orientation::Horizontal => (0, 1, 0, 0),
                    };
                    let children = (
                        $(
                            Child::new(children.$n.0, orientation, children.$n.1),
                        )+
                    );
                    let inner =
                        PaddedView::lrtb(l, r, t, b, StaticLinearLayout::new(orientation, children));
                    Self { inner }
                }

                pub fn new(orientation: Orientation, children: ($($name,)+)) -> Self {
                    Self::new_with_titles(
                        orientation,
                        ($((children.$n, None),)+),
                    )
                }

                #[allow(dead_code)]
                pub fn horizontal(children: ($($name,)+)) -> Self {
                    Self::new(Orientation::Horizontal, children)
                }

                #[allow(dead_code)]
                pub fn vertical(children: ($($name,)+)) -> Self {
                    Self::new(Orientation::Vertical, children)
                }

                #[allow(dead_code)]
                pub fn get_children(&self) -> &($(Child<$name>,)+) {
                    self.inner.get_inner().get_children()
                }

                #[allow(dead_code)]
                pub fn get_children_mut(&mut self) -> &mut ($(Child<$name>,)+) {
                    self.inner.get_inner_mut().get_children_mut()
                }
            }

            impl<$($name),+> ViewWrapper for StaticLinearPanel<($(Child<$name>,)+)>
            where
                $($name: View + 'static),+
            {
                cursive::wrap_impl!(
                    self.inner: PaddedView<StaticLinearLayout<($(Child<$name>,)+)>>
                );

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
        )+
    };
}

impls! {
    @0 V0,
    @0 V0 @1 V1,
    @0 V0 @1 V1 @2 V2,
    @0 V0 @1 V1 @2 V2 @3 V3,
}
