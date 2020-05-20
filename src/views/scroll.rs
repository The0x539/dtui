use cursive::traits::*;
use cursive::view::{ScrollBase, ViewWrapper};
use cursive::vec::Vec2;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::Printer;

// This entire module only exists because ScrollView has janky mouse support.

pub(crate) trait ScrollInner: View + Sized {
    fn draw_row(&self, printer: &Printer, row: usize);

    fn into_scroll_wrapper(self) -> ScrollWrapper<Self> {
        ScrollWrapper::new(self)
    }
}

pub(crate) struct ScrollWrapper<V: ScrollInner> {
    inner: V,
    scrollbase: ScrollBase,
    width: usize,
}

impl<V: ScrollInner> ScrollWrapper<V> {
    fn new(inner: V) -> Self {
        Self { inner, scrollbase: ScrollBase::new(), width: 0 }
    }
}

impl<V: ScrollInner> ViewWrapper for ScrollWrapper<V> {
    cursive::wrap_impl!(self.inner: V);

    fn wrap_required_size(&mut self, req: Vec2) -> Vec2 {
        self.scrollbase.view_height = req.y;
        let inner_req = self.inner.required_size(req);
        self.scrollbase.content_height = inner_req.y;
        let additional_width = self.scrollbase.scrollable() as usize;
        (inner_req.x + additional_width, req.y).into()
    }

    fn wrap_layout(&mut self, size: Vec2) {
        let sb = &mut self.scrollbase;

        sb.view_height = size.y;
        let additional_width = sb.scrollable() as usize;
        self.inner.layout((size.x - additional_width, sb.content_height).into());
        self.width = size.x;

        if sb.start_line + sb.view_height > sb.content_height {
            sb.start_line = sb.content_height.saturating_sub(sb.view_height);
        }
    }

    fn wrap_draw(&self, printer: &Printer) {
        self.scrollbase.draw(printer, |p, r| self.inner.draw_row(p, r));
    }

    fn wrap_on_event(&mut self, event: Event) -> EventResult {
        let sb = &mut self.scrollbase;
        match event {
            Event::Mouse { offset, position, event } => {
                let pos = position.saturating_sub(offset);
                // If the click is on the scrollbar, don't tell the inner view about it
                // Otherwise, give it a chance to consume the event
                if pos.x != sb.scrollbar_x(self.width) || !sb.scrollable() {
                    // Event.relativize is the negation of what we want to do here
                    // All involved ints are unsigned, so this must be done manually instead
                    let e = Event::Mouse {
                        offset,
                        position: position + (0, sb.start_line),
                        event,
                    };
                    if let r @ EventResult::Consumed(_) = self.inner.on_event(e) {
                        return r;
                    }
                }
                match event {
                    MouseEvent::WheelUp => {
                        sb.scroll_up(1);
                        EventResult::Consumed(None)
                    },
                    MouseEvent::WheelDown => {
                        sb.scroll_down(1);
                        EventResult::Consumed(None)
                    },
                    MouseEvent::Press(MouseButton::Left) => {
                        sb.start_drag(pos, self.width);
                        EventResult::Consumed(None)
                    },
                    MouseEvent::Hold(MouseButton::Left) => {
                        sb.drag(pos);
                        EventResult::Consumed(None)
                    },
                    MouseEvent::Release(MouseButton::Left) => {
                        sb.release_grab();
                        EventResult::Consumed(None)
                    },
                    _ => EventResult::Ignored,
                }
            },

            // TODO: keyboard scrolling

            // Any other events get forwarded unconditionally.
            _ => self.inner.on_event(event),
        }
    }
}
