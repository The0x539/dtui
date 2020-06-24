#![allow(unused_imports, dead_code)]

mod view_tuple;
pub use view_tuple::{ViewFn, ViewMutFn, ViewTuple};

use cursive::{
    direction,
    event::{AnyCb, Event, EventResult, Key},
    view::{IntoBoxedView, Selector, SizeCache, View},
    Printer, Rect, Vec2, With, XY,
};

use std::cmp::min;
use std::ops::Deref;

pub struct StaticLinearLayout<T> {
    children: T,
    child_metadata: Vec<ChildMetadata>,
    orientation: direction::Orientation,
    focus: usize,

    cache: Option<XY<SizeCache>>,
}

#[derive(Copy, Clone)]
struct ChildMetadata {
    required_size: Vec2,
    last_size: Vec2,
    weight: usize,
}

impl Default for ChildMetadata {
    fn default() -> Self {
        Self {
            required_size: Vec2::zero(),
            last_size: Vec2::zero(),
            weight: 0,
        }
    }
}

struct ChildRefIter<I> {
    inner: I,
    offset: usize,
    available: usize,
    orientation: direction::Orientation,
}

#[derive(Copy, Clone)]
struct ChildRefItem<'a> {
    index: usize,
    child: &'a ChildMetadata,
    offset: usize,
    length: usize,
}

impl<T> ChildRefIter<T> {
    fn new(inner: T, orientation: direction::Orientation, available: usize) -> Self {
        Self {
            inner,
            available,
            orientation,
            offset: 0,
        }
    }
}

impl<'a, I: Iterator<Item = (usize, &'a ChildMetadata)>> Iterator for ChildRefIter<I> {
    type Item = ChildRefItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(index, child)| {
            let offset = self.offset;
            let length = min(self.available, *child.required_size.get(self.orientation));

            self.available = self.available.saturating_sub(length);
            self.offset += length;

            ChildRefItem {
                index,
                child,
                offset,
                length,
            }
        })
    }
}

#[derive(Copy, Clone)]
struct GiveFocus(direction::Direction);
impl ViewMutFn for GiveFocus {
    type Output = bool;

    fn call_mut(&mut self, view: &mut impl View) -> Self::Output {
        view.take_focus(self.0)
    }
}

struct GetRequiredSize(Vec2);
impl ViewMutFn for GetRequiredSize {
    type Output = Vec2;

    fn call_mut(&mut self, view: &mut impl View) -> Self::Output {
        view.required_size(self.0)
    }
}

impl<T: ViewTuple> StaticLinearLayout<T> {
    pub fn new(orientation: direction::Orientation, children: T) -> Self {
        StaticLinearLayout {
            children,
            child_metadata: vec![ChildMetadata::default(); T::LEN],
            orientation,
            focus: 0,
            cache: None,
        }
    }

    pub fn set_weight(&mut self, i: usize, weight: usize) {
        self.child_metadata[i].weight = weight;
    }

    pub fn weight(mut self, i: usize, weight: usize) -> Self {
        self.set_weight(i, weight);
        self
    }

    pub fn len(&self) -> usize {
        T::LEN
    }

    pub fn set_focus_index(&mut self, index: usize) -> Result<(), ()> {
        let give_focus = GiveFocus(direction::Direction::none());

        if index >= self.len() {
            Err(())
        } else if self.children.with_elem_mut(index, give_focus) {
            self.focus = index;
            Ok(())
        } else {
            Err(())
        }
    }

    fn invalidate(&mut self) {
        self.cache = None;
    }

    pub fn vertical(children: T) -> Self {
        Self::new(direction::Orientation::Vertical, children)
    }

    pub fn horizontal(children: T) -> Self {
        Self::new(direction::Orientation::Horizontal, children)
    }

    pub fn get_children(&self) -> &T {
        &self.children
    }

    pub fn get_children_mut(&mut self) -> &mut T {
        &mut self.children
    }

    pub fn with_focused<F: ViewFn>(&self, f: F) -> F::Output {
        self.children.with_elem(self.focus, f)
    }

    pub fn with_focused_mut<F: ViewMutFn>(&mut self, f: F) -> F::Output {
        self.children.with_elem_mut(self.focus, f)
    }

    fn get_cache(&self, req: Vec2) -> Option<Vec2> {
        let cache = &self.cache?;
        if cache.zip_map(req, SizeCache::accept).both() && self.children_are_sleeping() {
            Some(cache.map(|s| s.value))
        } else {
            None
        }
    }

    fn children_are_sleeping(&self) -> bool {
        struct NeedsRelayout;
        impl ViewFn for NeedsRelayout {
            type Output = bool;
            fn call(&mut self, view: &impl View) -> bool {
                view.needs_relayout()
            }
        }

        self.children.with_each(NeedsRelayout).contains(&true)
    }

    fn move_focus(&mut self, source: direction::Direction) -> EventResult {
        assert!(self.focus < T::LEN);
        let mut focus = self.focus;

        match source.relative(self.orientation) {
            Some(direction::Relative::Back) => loop {
                if focus == 0 {
                    break;
                }
                focus -= 1;
                if self.with_focused_mut(GiveFocus(source)) {
                    self.focus = focus;
                    break;
                }
            },
            Some(direction::Relative::Front) => loop {
                focus += 1;
                if focus == self.len() {
                    break;
                }
                if self.with_focused_mut(GiveFocus(source)) {
                    self.focus = focus;
                    break;
                }
            },
            None => (),
        }

        return EventResult::Consumed(None);
    }

    fn check_focus_grab(&mut self, event: &Event) {
        if let Event::Mouse {
            offset,
            position,
            event,
        } = *event
        {
            if !event.grabs_focus() {
                return;
            }

            let position = match position.checked_sub(offset) {
                None => return,
                Some(pos) => pos,
            };

            let position = *position.get(self.orientation);

            let give_focus = GiveFocus(direction::Direction::none());

            for item in ChildRefIter::new(
                self.child_metadata.iter().enumerate(),
                self.orientation,
                // TODO: get actual width (not super important)
                usize::MAX,
            ) {
                let child_size = item.child.last_size.get(self.orientation);
                if item.offset + child_size > position {
                    if self.children.with_elem_mut(item.index, give_focus) {
                        self.focus = item.index;
                    }
                    break;
                }
            }
        }
    }
}

impl<T: ViewTuple + 'static> View for StaticLinearLayout<T> {
    fn draw(&self, printer: &Printer) {
        struct Draw<'a, 'b, 'c>(&'a Printer<'b, 'c>);
        impl ViewFn for Draw<'_, '_, '_> {
            type Output = ();

            fn call(&mut self, view: &impl View) -> Self::Output {
                view.draw(self.0)
            }
        }

        for item in ChildRefIter::new(
            self.child_metadata.iter().enumerate(),
            self.orientation,
            *printer.size.get(self.orientation),
        ) {
            let printer = &printer
                .offset(self.orientation.make_vec(item.offset, 0))
                .cropped(item.child.last_size)
                .focused(item.index == self.focus);

            self.children.with_elem(item.index, Draw(&printer));
        }
    }

    fn needs_relayout(&self) -> bool {
        self.cache.is_none() || !self.children_are_sleeping()
    }

    fn layout(&mut self, size: Vec2) {
        struct Layout(Vec2);
        impl ViewMutFn for Layout {
            type Output = ();
            fn call_mut(&mut self, view: &mut impl View) -> Self::Output {
                view.layout(self.0)
            }
        }

        if self.get_cache(size).is_none() {
            self.required_size(size);
        }

        let o = self.orientation;

        for item in ChildRefIter::new(self.child_metadata.iter().enumerate(), o, *size.get(o)) {
            let size = size.with_axis(o, item.length);
            self.children.with_elem_mut(item.index, Layout(size));
        }
    }

    fn required_size(&mut self, _: Vec2) -> Vec2 {
        todo!()
    }

    fn take_focus(&mut self, _: direction::Direction) -> bool {
        todo!()
    }

    fn on_event(&mut self, _: Event) -> EventResult {
        todo!()
    }

    fn call_on_any<'a>(&mut self, _: &Selector<'_>, _: AnyCb<'a>) {
        todo!()
    }

    fn focus_view(&mut self, _: &Selector<'_>) -> Result<(), ()> {
        todo!()
    }

    fn important_area(&self, _: Vec2) -> Rect {
        todo!()
    }
}
