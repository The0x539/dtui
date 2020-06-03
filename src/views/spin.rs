#![allow(unused)]

use cursive::traits::*;
use cursive::views::{EditView, LinearLayout, TextView, Button, NamedView, Panel};
use cursive::Printer;
use cursive::vec::Vec2;
use cursive::view::ViewWrapper;
use uuid::Uuid;
use cursive::event::{Event, EventResult, Callback};
use cursive::align::HAlign;

use std::{
    convert::From,
    cmp::PartialOrd,
    cmp::PartialEq,
    ops::{RangeBounds, Bound},
    fmt::Display,
};

pub trait Spinnable: Default + PartialEq + PartialOrd + From<u8> + Copy + Display {
    fn is_float() -> bool;

    fn checked_incr(self) -> Option<Self>;
    fn checked_decr(self) -> Option<Self>;

    fn allows_negative(bounds: &impl RangeBounds<Self>) -> bool {
        let zero = Self::from(0u8);

        match bounds.start_bound() {
            Bound::Excluded(min) | Bound::Included(min) => *min < zero,
            Bound::Unbounded => zero.checked_decr().is_some(),
        }
    }

    fn clamped_incr(self, bounds: &impl RangeBounds<Self>) -> Self {
        match (self.checked_incr(), bounds.end_bound()) {
            // If incrementing overflows, but the end bound is inclusive, use that.
            (None, Bound::Included(max)) => *max,

            // Otherwise, if incrementing doesn't overflow and the bounds allow it, use that.
            (Some(v), _) if bounds.contains(&v) => v,

            // Otherwise, we can't safely increment without overflowing or exceeding the bounds.
            // (3.5f64).clamped_incr(..4.0f64) == 3.5f64
            _ => self,
        }
    }

    fn clamped_decr(self, bounds: &impl RangeBounds<Self>) -> Self {
        // See above
        match (self.checked_decr(), bounds.start_bound()) {
            (None, Bound::Included(min)) => *min,
            (Some(v), _) if bounds.contains(&v) => v,
            _ => self,
        }
    }
}

impl Spinnable for u64 {
    fn is_float() -> bool { false }
    fn checked_incr(self) -> Option<Self> { self.checked_add(1) }
    fn checked_decr(self) -> Option<Self> { self.checked_sub(1) }
    fn allows_negative(_: &impl RangeBounds<Self>) -> bool { false }
}

impl Spinnable for i64 {
    fn is_float() -> bool { false }
    fn checked_incr(self) -> Option<Self> { self.checked_add(1) }
    fn checked_decr(self) -> Option<Self> { self.checked_sub(1) }
}

impl Spinnable for f64 {
    fn is_float() -> bool { true }
    fn checked_incr(self) -> Option<Self> {
        Some(self + 1.0).filter(|x| x.is_finite())
    }
    fn checked_decr(self) -> Option<Self> {
        Some(self - 1.0).filter(|x| x.is_finite())
    }
}

pub(crate) struct SpinView<T: Spinnable, B: RangeBounds<T>> {
    bounds: B,
    val: T,
    edit_id: String,
    panel: Panel<LinearLayout>,
}

impl<T: Spinnable, B: RangeBounds<T>> SpinView<T, B> where Self: 'static {
    pub(crate) fn new(title: Option<String>, bounds: B) -> NamedView<Self> {
        
        let val = T::default();

        let id = Uuid::new_v4().to_string();
        let (id2, id3) = (id.clone(), id.clone());

        let edit_id = Uuid::new_v4().to_string();

        let edit = EditView::new().content(val.to_string());
        let decr = Button::new_raw(" - ", move |s| {
            let cb = s.call_on_name(&id2, Self::decr).unwrap();
            cb(s)
        });
        let incr = Button::new_raw(" + ", move |s| {
            let cb = s.call_on_name(&id3, Self::incr).unwrap();
            cb(s)
        });

        let views = LinearLayout::horizontal()
            .child(edit.with_name(&edit_id).full_width())
            .child(TextView::new("│"))
            .child(decr)
            .child(TextView::new("│"))
            .child(incr);

        let mut panel = Panel::new(views);

        if let Some(title) = title {
            panel.set_title(title);
            panel.set_title_position(HAlign::Left);
        }

        Self { bounds, val, edit_id, panel }.with_name(id)
    }

    fn set_val(&mut self, new_val: T) -> Callback {
        let val_str = new_val.to_string();
        self.val = new_val;
        self.panel
            .call_on_name(&self.edit_id, |v: &mut EditView| v.set_content(val_str))
            .unwrap()
    }

    fn decr(&mut self) -> Callback {
        self.set_val(self.val.clamped_decr(&self.bounds))
    }

    fn incr(&mut self) -> Callback {
        self.set_val(self.val.clamped_incr(&self.bounds))
    }
}

impl<T: Spinnable, B: RangeBounds<T>> ViewWrapper for SpinView<T, B>
where Self: 'static {
    cursive::wrap_impl!(self.panel: Panel<LinearLayout>);

    fn wrap_required_size(&mut self, mut constraint: Vec2) -> Vec2 {
        constraint.y = 3; // no tallness allowed
        self.panel.required_size(constraint)
    }

    fn wrap_draw(&self, printer: &Printer) {
        self.panel.draw(printer);

        printer.print((printer.size.x - 9, 0), "┬───┬");
        //                                      │ + │
        printer.print((printer.size.x - 9, 2), "┴───┴");
    }
}
