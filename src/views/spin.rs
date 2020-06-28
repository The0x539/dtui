use crate::form::Form;
use crate::util::digit_width;
use cursive::event::{AnyCb, Callback, Event, EventResult};
use cursive::traits::*;
use cursive::view::{Selector, SizeConstraint, ViewWrapper};
use cursive::views::{Button, DummyView, EditView, LinearLayout, TextView};
use cursive::Cursive;
use std::rc::Rc;
use uuid::Uuid;

use std::{
    cmp::PartialEq,
    cmp::PartialOrd,
    convert::From,
    fmt::Display,
    ops::{Bound, RangeBounds},
    str::FromStr,
};

pub trait Spinnable:
    Default + PartialEq + PartialOrd + From<u8> + Copy + Display + FromStr
{
    const MAX_WIDTH: Option<usize>;

    fn is_float() -> bool;

    fn max_width() -> Option<usize> {
        None
    }

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
    const MAX_WIDTH: Option<usize> = Some(digit_width(u64::MAX));

    fn is_float() -> bool {
        false
    }
    fn checked_incr(self) -> Option<Self> {
        self.checked_add(1)
    }
    fn checked_decr(self) -> Option<Self> {
        self.checked_sub(1)
    }
    fn allows_negative(_: &impl RangeBounds<Self>) -> bool {
        false
    }
}

impl Spinnable for i64 {
    const MAX_WIDTH: Option<usize> = Some(1 + digit_width(i64::MAX as u64));

    fn is_float() -> bool {
        false
    }
    fn checked_incr(self) -> Option<Self> {
        self.checked_add(1)
    }
    fn checked_decr(self) -> Option<Self> {
        self.checked_sub(1)
    }
}

impl Spinnable for u16 {
    const MAX_WIDTH: Option<usize> = Some(digit_width(u16::MAX as u64));

    fn is_float() -> bool {
        false
    }
    fn checked_incr(self) -> Option<Self> {
        self.checked_add(1)
    }
    fn checked_decr(self) -> Option<Self> {
        self.checked_sub(1)
    }
    fn allows_negative(_: &impl RangeBounds<Self>) -> bool {
        false
    }
}

impl Spinnable for f64 {
    const MAX_WIDTH: Option<usize> = Some(20); // ¯\_(ツ)_/¯

    fn is_float() -> bool {
        true
    }
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
    own_id: String,
    edit_id: String,
    inner: LinearLayout,
    on_modify: Option<Rc<dyn Fn(&mut Cursive, T)>>,
}

impl<T: Spinnable, B: RangeBounds<T>> SpinView<T, B>
where
    Self: 'static,
{
    pub(crate) fn new(title: Option<&str>, label: Option<&str>, bounds: B) -> Self {
        let val = T::default();

        let id = Rc::new(Uuid::new_v4().to_string());
        let (id0, id1, id2, id3) = (id.clone(), id.clone(), id.clone(), id.clone());

        let edit_id = Uuid::new_v4().to_string();

        let width_constraint = match T::MAX_WIDTH {
            Some(width) => SizeConstraint::AtMost(width),
            None => SizeConstraint::Free,
        };

        // TODO: figure out semantics of editing vs. being done editing
        let edit = EditView::new()
            .content(val.to_string())
            .on_edit(move |s, content, _| {
                s.call_on_name(&id0, |v: &mut Self| v.parse_content(content))
                    .unwrap();
            })
            .on_submit(move |s, _content| {
                let cb = s.call_on_name(&id1, Self::submit).unwrap();
                cb(s)
            })
            .with_name(&edit_id)
            .resized(SizeConstraint::Full, SizeConstraint::Fixed(1))
            .resized(width_constraint, SizeConstraint::Fixed(1));

        let decr = Button::new_raw(" - ", move |s| {
            let cb = s.call_on_name(&id2, Self::decr).unwrap();
            cb(s)
        });

        let incr = Button::new_raw(" + ", move |s| {
            let cb = s.call_on_name(&id3, Self::incr).unwrap();
            cb(s)
        });

        let mut inner = LinearLayout::horizontal();
        if let Some(title) = title {
            let mut v = TextView::new(title).no_wrap();
            v.append(": ");
            inner.add_child(v);
        }
        inner.add_child(edit);
        inner.add_child(DummyView);
        if let Some(label) = label {
            inner.add_child(TextView::new(label).no_wrap());
            inner.add_child(DummyView);
        }
        inner.add_child(decr);
        inner.add_child(incr);

        let own_id = String::clone(id.as_ref());

        Self {
            bounds,
            val,
            own_id,
            edit_id,
            inner,
            on_modify: None,
        }
    }

    pub fn get_val(&self) -> T {
        self.val
    }

    pub fn set_val(&mut self, new_val: T) -> Callback {
        self.val = new_val;
        let cb = self.call_on_edit_view(|v| v.set_content(new_val.to_string()));
        if let Some(f) = self.on_modify.as_ref() {
            let f = f.clone();
            let val = self.val;
            Callback::from_fn(move |s| {
                cb(s);
                f(s, val);
            })
        } else {
            cb
        }
    }

    pub fn with_val(mut self, val: T) -> Self {
        let _: Callback = self.set_val(val);
        self
    }

    pub fn set_on_modify<F: Fn(&mut Cursive, T) + 'static>(&mut self, cb: F) {
        self.on_modify = Some(Rc::new(cb));
    }

    pub fn on_modify<F: Fn(&mut Cursive, T) + 'static>(mut self, cb: F) -> Self {
        self.set_on_modify(cb);
        self
    }

    fn call_on_edit_view<F: FnOnce(&mut EditView) -> R, R>(&mut self, f: F) -> R {
        self.inner.call_on_name(&self.edit_id, f).unwrap()
    }

    fn get_content(&mut self) -> Rc<String> {
        self.call_on_edit_view(|v| v.get_content())
    }

    fn parse_content(&mut self, content: &str) {
        if let Ok(v) = content.parse::<T>() {
            if self.bounds.contains(&v) {
                self.val = v;
            }
        } else if T::is_float() && content.parse::<i128>().is_ok() {
            // Special case because Rust is picky about floats and we're not
            if let Ok(v) = (content.to_owned() + ".0").parse::<T>() {
                if self.bounds.contains(&v) {
                    self.val = v;
                }
            }
        }
    }

    fn decr(&mut self) -> Callback {
        let new_val = self.val.clamped_decr(&self.bounds);
        self.set_val(new_val)
    }

    fn incr(&mut self) -> Callback {
        let new_val = self.val.clamped_incr(&self.bounds);
        self.set_val(new_val)
    }

    fn submit(&mut self) -> Callback {
        self.set_val(self.val)
    }
}

impl<T: Spinnable, B: RangeBounds<T>> ViewWrapper for SpinView<T, B>
where
    Self: 'static,
{
    cursive::wrap_impl!(self.inner: LinearLayout);

    fn wrap_on_event(&mut self, event: Event) -> EventResult {
        if self.inner.get_focus_index() == 0 {
            if let Event::Char(ch) = event {
                match ch {
                    '0'..='9' => (),

                    '.' if T::is_float() && !self.get_content().contains('.') => (),

                    '-' if T::allows_negative(&self.bounds)
                        && !self.get_content().contains('-') =>
                    {
                        ()
                    }

                    _ => return EventResult::Ignored,
                }
            }
        }

        self.inner.on_event(event)
    }

    fn wrap_call_on_any(&mut self, sel: &Selector, cb: AnyCb) {
        match sel {
            Selector::Name(name) if name == &self.own_id => cb(self),
            sel => self.inner.call_on_any(sel, cb),
        }
    }
}

impl<T, B> Form for SpinView<T, B>
where
    T: Spinnable,
    B: RangeBounds<T>,
    Self: 'static,
{
    type Data = T;

    fn into_data(self) -> Self::Data {
        self.get_val()
    }
}
