use std::cell::RefCell;
use tokio::sync::oneshot;

pub fn digit_width(mut n: u64) -> usize {
    if n == 0 {
        return 1;
    }

    let mut digits = 0;
    while n > 0 {
        n /= 10;
        digits += 1;
    }
    digits
}

pub mod fmt;
pub mod simple_slab;

enum EventualState<T> {
    Pending(oneshot::Receiver<T>),
    Ready(T),
    Closed,
}

impl<T> Default for EventualState<T> {
    fn default() -> Self {
        Self::Closed
    }
}

impl<T> EventualState<T> {
    fn poll(&mut self) {
        use oneshot::error::TryRecvError;

        if let Self::Pending(rx) = self {
            *self = match rx.try_recv() {
                Ok(v) => Self::Ready(v),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Closed) => Self::Closed,
            }
        }
    }

    fn get(&self) -> Option<&T> {
        match self {
            Self::Ready(v) => Some(v),
            _ => None,
        }
    }
}

pub struct Eventual<T>(RefCell<EventualState<T>>);

impl<T> Eventual<T> {
    pub fn new(rx: oneshot::Receiver<T>) -> Self {
        Self(RefCell::new(EventualState::Pending(rx)))
    }

    pub fn is_ready(&self) -> bool {
        self.0.borrow_mut().poll();
        self.0.borrow().get().is_some()
    }

    pub fn get(&self) -> Option<T>
    where
        T: Clone,
    {
        self.0.borrow_mut().poll();
        self.0.borrow().get().map(Clone::clone)
    }
}
