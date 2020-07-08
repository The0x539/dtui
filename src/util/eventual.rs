use std::cell::RefCell;

use tokio::sync::oneshot;

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
    pub fn new() -> (Self, oneshot::Sender<T>) {
        let (tx, rx) = oneshot::channel();

        let state = EventualState::Pending(rx);
        let cell = RefCell::new(state);

        (Self(cell), tx)
    }

    pub fn ready(val: T) -> Self {
        let state = EventualState::Ready(val);
        let cell = RefCell::new(state);

        Self(cell)
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
