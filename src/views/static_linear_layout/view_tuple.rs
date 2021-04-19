use cursive::{
    direction::Direction,
    event::{AnyCb, Event, EventResult},
    view::{Selector, View, ViewNotFound},
    Printer, Rect, Vec2,
};

pub trait ViewTuple {
    const LEN: usize;

    fn get(&self, i: usize) -> &dyn View;
    fn get_mut(&mut self, i: usize) -> &mut dyn View;

    fn draw(&self, i: usize, printer: &Printer);
    fn layout(&mut self, i: usize, size: Vec2);
    fn needs_relayout(&self, i: usize) -> bool;
    fn required_size(&mut self, i: usize, constraint: Vec2) -> Vec2;
    fn on_event(&mut self, i: usize, event: Event) -> EventResult;
    fn call_on_any<'a>(&mut self, i: usize, selector: &Selector, callback: AnyCb<'a>);
    fn focus_view(&mut self, i: usize, selector: &Selector) -> Result<(), ViewNotFound>;
    fn take_focus(&mut self, i: usize, source: Direction) -> bool;
    fn important_area(&self, i: usize, view_size: Vec2) -> Rect;

    fn with_each<F: FnMut(&Self, usize) -> T, T>(&self, mut f: F) -> Vec<T> {
        let mut outputs = Vec::<T>::with_capacity(Self::LEN);
        for i in 0..Self::LEN {
            outputs.push(f(self, i));
        }
        outputs
    }

    fn with_each_mut<F: FnMut(&mut Self, usize) -> T, T>(&mut self, mut f: F) -> Vec<T> {
        let mut outputs = Vec::<T>::with_capacity(Self::LEN);
        for i in 0..Self::LEN {
            outputs.push(f(self, i));
        }
        outputs
    }
}

macro_rules! tuple_impls {
    ($($len:literal => ($($n:tt $name:ident)+))+) => {
        $(
            impl<$($name: View),+> ViewTuple for ($($name,)+) {
                const LEN: usize = $len;

                fn get(&self, i: usize) -> &dyn View {
                    match i {
                        $($n => &self.$n),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn get_mut(&mut self, i: usize) -> &mut dyn View {
                    match i {
                        $($n => &mut self.$n),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn draw(&self, i: usize, printer: &Printer) {
                    match i {
                        $($n => self.$n.draw(printer)),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn layout(&mut self, i: usize, size: Vec2) {
                    match i {
                        $($n => self.$n.layout(size)),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn needs_relayout(&self, i: usize) -> bool {
                    match i {
                        $($n => self.$n.needs_relayout()),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn required_size(&mut self, i: usize, constraint: Vec2) -> Vec2 {
                    match i {
                        $($n => self.$n.required_size(constraint)),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn on_event(&mut self, i: usize, event: Event) -> EventResult {
                    match i {
                        $($n => self.$n.on_event(event)),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn call_on_any<'a>(&mut self, i: usize, selector: &Selector, callback: AnyCb<'a>) {
                    match i {
                        $($n => self.$n.call_on_any(selector, callback)),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn focus_view(&mut self, i: usize, selector: &Selector) -> Result<(), ViewNotFound> {
                    match i {
                        $($n => self.$n.focus_view(selector)),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn take_focus(&mut self, i: usize, source: Direction) -> bool {
                    match i {
                        $($n => self.$n.take_focus(source)),+,
                        _ => panic!("out of bounds"),
                    }
                }

                fn important_area(&self, i: usize, view_size: Vec2) -> Rect {
                    match i {
                        $($n => self.$n.important_area(view_size)),+,
                        _ => panic!("out of bounds"),
                    }
                }
            }
        )+
    };
}

tuple_impls! {
    1 => (0 V0)
    2 => (0 V0 1 V1)
    3 => (0 V0 1 V1 2 V2)
    4 => (0 V0 1 V1 2 V2 3 V3)
    5 => (0 V0 1 V1 2 V2 3 V3 4 V4)
    6 => (0 V0 1 V1 2 V2 3 V3 4 V4 5 V5)
    7 => (0 V0 1 V1 2 V2 3 V3 4 V4 5 V5 6 V6)
    8 => (0 V0 1 V1 2 V2 3 V3 4 V4 5 V5 6 V6 7 V7)
}
