pub mod panel;
mod view_tuple;
pub use view_tuple::ViewTuple;

use cursive::{
    direction,
    event::{AnyCb, Event, EventResult, Key},
    view::{Selector, SizeCache, View, ViewNotFound},
    Printer, Rect, Vec2, XY,
};

use std::cmp::min;

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
    #[allow(dead_code)]
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

fn cap<'a, I: Iterator<Item = &'a mut usize>>(iter: I, max: usize) {
    let mut available = max;
    for item in iter {
        if *item > available {
            *item = available;
        }
        available -= *item;
    }
}

#[allow(dead_code)]
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
        if index >= self.len() {
            Err(())
        } else if self
            .children
            .take_focus(index, direction::Direction::none())
        {
            self.focus = index;
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn get_focus_index(&mut self) -> usize {
        self.focus
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

    pub fn into_children(self) -> T {
        self.children
    }

    pub fn with_child<F: FnOnce(&dyn View) -> O, O>(&self, i: usize, f: F) -> O {
        f(self.children.get(i))
    }

    pub fn with_child_mut<F: FnOnce(&mut dyn View) -> O, O>(&mut self, i: usize, f: F) -> O {
        f(self.children.get_mut(i))
    }

    pub fn with_focused<F: FnOnce(&dyn View) -> O, O>(&self, f: F) -> O {
        self.with_child(self.focus, f)
    }

    pub fn with_focused_mut<F: FnOnce(&mut dyn View) -> O, O>(&mut self, f: F) -> O {
        self.with_child_mut(self.focus, f)
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
        for i in 0..self.len() {
            if self.children.needs_relayout(i) {
                return false;
            }
        }
        return true;
    }

    fn move_focus(&mut self, source: direction::Direction) -> EventResult {
        assert!(self.focus < T::LEN);
        let mut focus = self.focus;

        match source.relative(self.orientation) {
            Some(direction::Relative::Back) => loop {
                if focus == 0 {
                    break EventResult::Ignored;
                }
                focus -= 1;
                if self.children.take_focus(focus, source) {
                    self.focus = focus;
                    break EventResult::Consumed(None);
                }
            },
            Some(direction::Relative::Front) => loop {
                focus += 1;
                if focus == self.len() {
                    break EventResult::Ignored;
                }
                if self.children.take_focus(focus, source) {
                    self.focus = focus;
                    break EventResult::Consumed(None);
                }
            },
            None => EventResult::Ignored,
        }
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

            for item in ChildRefIter::new(
                self.child_metadata.iter().enumerate(),
                self.orientation,
                // TODO: get actual width (not super important)
                usize::MAX,
            ) {
                let child_size = item.child.last_size.get(self.orientation);
                if item.offset + child_size > position {
                    if self
                        .children
                        .take_focus(item.index, direction::Direction::none())
                    {
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
        for item in ChildRefIter::new(
            self.child_metadata.iter().enumerate(),
            self.orientation,
            *printer.size.get(self.orientation),
        ) {
            let printer = &printer
                .offset(self.orientation.make_vec(item.offset, 0))
                .cropped(item.child.last_size)
                .focused(item.index == self.focus);

            self.children.draw(item.index, &printer);
        }
    }

    fn needs_relayout(&self) -> bool {
        self.cache.is_none() || !self.children_are_sleeping()
    }

    fn layout(&mut self, size: Vec2) {
        if self.get_cache(size).is_none() {
            self.required_size(size);
        }

        let o = self.orientation;

        let mut sizes = Vec::with_capacity(self.len());

        for item in ChildRefIter::new(self.child_metadata.iter().enumerate(), o, *size.get(o)) {
            let size = size.with_axis(o, item.length);
            self.children.layout(item.index, size);
            sizes.push(size);
        }

        for (i, size) in sizes.into_iter().enumerate() {
            self.child_metadata[i].last_size = size;
        }
    }

    fn required_size(&mut self, req: Vec2) -> Vec2 {
        if let Some(size) = self.get_cache(req) {
            return size;
        }

        let o = self.orientation;

        let mut metadata = std::mem::take(&mut self.child_metadata);

        let ideal_sizes = self.children.with_each_mut(|t, i| {
            let required_size = t.required_size(i, req);
            metadata[i].required_size = required_size;
            required_size
        });
        let ideal = o.stack(ideal_sizes.iter().copied());

        if ideal.fits_in(req) {
            self.cache = Some(SizeCache::build(ideal, req));
            self.child_metadata = metadata;
            return ideal;
        }

        let budget_req = req.with_axis(o, 1);

        let min_sizes = self.children.with_each_mut(|t, i| {
            let required_size = t.required_size(i, budget_req);
            metadata[i].required_size = required_size;
            required_size
        });
        let desperate = o.stack(min_sizes.iter().copied());

        if desperate.get(o) > req.get(o) {
            cap(
                metadata.iter_mut().map(|c| c.required_size.get_mut(o)),
                *req.get(o),
            );

            self.cache = None;
            self.child_metadata = metadata;
            return desperate;
        }

        let mut available = o.get(&(req.saturating_sub(desperate)));

        let mut overweight: Vec<(usize, usize)> = ideal_sizes
            .iter()
            .map(|v| o.get(v))
            .zip(min_sizes.iter().map(|v| o.get(v)))
            .map(|(a, b)| a.saturating_sub(b))
            .enumerate()
            .collect();

        overweight.sort_by_key(|&(_, weight)| weight);
        let mut allocations = vec![0; overweight.len()];

        for (i, &(j, weight)) in overweight.iter().enumerate() {
            let remaining = overweight.len() - i;
            let budget = available / remaining;
            let spent = min(budget, weight);
            allocations[j] = spent;
            available -= spent;
        }

        let final_lengths: Vec<Vec2> = min_sizes
            .iter()
            .map(|v| o.get(v))
            .zip(allocations.iter())
            .map(|(a, b)| a + b)
            .map(|l| req.with_axis(o, l))
            .collect();

        for i in 0..self.len() {
            let size = self.children.required_size(i, final_lengths[i]);
            metadata[i].required_size = size;
        }

        let compromise = o.stack(metadata.iter().map(|c| c.required_size));

        self.cache = Some(SizeCache::build(compromise, req));
        self.child_metadata = metadata;

        compromise
    }

    fn take_focus(&mut self, source: direction::Direction) -> bool {
        if source.relative(self.orientation).is_some() {
            self.move_focus(source).is_consumed()
        } else {
            for i in 0..self.len() {
                if self.children.take_focus(i, source) {
                    return true;
                }
            }
            return false;
        }
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        if self.len() == 0 {
            return EventResult::Ignored;
        }

        self.check_focus_grab(&event);

        let o = self.orientation;

        let result = {
            let item = ChildRefIter::new(self.child_metadata.iter().enumerate(), o, usize::MAX)
                .nth(self.focus)
                .unwrap();

            let offset = o.make_vec(item.offset, 0);
            self.children
                .on_event(self.focus, event.relativized(offset))
        };

        if result.is_consumed() {
            return result;
        }

        use direction::{
            Direction,
            Orientation::{Horizontal, Vertical},
        };

        match event {
            Event::Shift(Key::Tab) if self.focus > 0 => self.move_focus(Direction::back()),
            Event::Key(Key::Tab) if self.focus + 1 < T::LEN => self.move_focus(Direction::front()),
            Event::Key(Key::Left) if o == Horizontal && self.focus > 0 => {
                self.move_focus(Direction::right())
            }
            Event::Key(Key::Up) if o == Vertical && self.focus > 0 => {
                self.move_focus(Direction::down())
            }
            Event::Key(Key::Right) if o == Horizontal && self.focus + 1 < T::LEN => {
                self.move_focus(Direction::left())
            }
            Event::Key(Key::Down) if self.orientation == Vertical && self.focus + 1 < T::LEN => {
                self.move_focus(Direction::up())
            }
            _ => EventResult::Ignored,
        }
    }

    fn call_on_any<'a>(&mut self, selector: &Selector<'_>, callback: AnyCb<'a>) {
        for i in 0..self.len() {
            self.children.call_on_any(i, selector, callback)
        }
    }

    fn focus_view(&mut self, selector: &Selector<'_>) -> Result<(), ViewNotFound> {
        for i in 0..self.len() {
            if self.children.focus_view(i, selector).is_ok() {
                return Ok(());
            }
        }
        Err(ViewNotFound)
    }

    fn important_area(&self, _: Vec2) -> Rect {
        if self.len() == 0 {
            return Rect::from((0, 0));
        }

        let item = ChildRefIter::new(
            self.child_metadata.iter().enumerate(),
            self.orientation,
            usize::MAX,
        )
        .nth(self.focus)
        .unwrap();

        let offset = self.orientation.make_vec(item.offset, 0);
        let rect = self
            .children
            .important_area(item.index, item.child.last_size);

        rect + offset
    }
}
