use bytesize::ByteSize;
use pretty_dtoa::{ftoa, FmtFloatConfig};
use std::cell::RefCell;
use std::fmt::Display;
use tokio::sync::oneshot;

pub fn fmt_bytes(amt: u64) -> String {
    ByteSize(amt).to_string_as(true)
}

pub fn fmt_bytes_limit(amt: f64) -> String {
    ByteSize((amt * 1024.0) as u64)
        .to_string_as(true)
        .replace(".0", "")
}

pub fn fmt_speed_pair(val: u64, max: f64) -> String {
    if max <= 0.0 {
        fmt_bytes(val) + "/s"
    } else {
        format!("{}/s ({}/s)", fmt_bytes(val), fmt_bytes_limit(max))
    }
}

pub fn fmt_pair<T, U: Display, F: FnMut(T) -> U>(mut f: F, a: T, b: Option<T>) -> String {
    match b {
        Some(b) => format!("{} ({})", f(a), f(b)),
        None => f(a).to_string(),
    }
}

pub fn fmt_percentage(val: f32) -> String {
    if val == 0.0 {
        return String::from("0");
    } else if val == 100.0 {
        return String::from("100");
    } else if !(0.0..=100.0).contains(&val) {
        return String::from("???");
    }

    let config = FmtFloatConfig {
        max_decimal_digits: Some(2),
        add_point_zero: true,
        force_no_e_notation: true,
        ..FmtFloatConfig::default()
    };

    ftoa(val, config)
}

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

pub fn ftime(mut secs: u64) -> String {
    let mut mins = secs / 60;
    secs %= 60;

    let mut hours = mins / 60;
    mins %= 60;

    let mut days = hours / 24;
    hours %= 24;

    let years = days / 365;
    days %= 365;

    let weeks = days / 7;
    days %= 7;

    let mut units = (None, None);

    let amounts = [years, weeks, days, hours, mins, secs];

    for (amount, suffix) in amounts.iter().copied().zip("ywdhms".chars()) {
        if amount > 0 {
            if units.0.is_none() {
                units.0 = Some((amount, suffix));
            } else {
                units.1 = Some((amount, suffix));
                break;
            }
        }
    }

    match units {
        (None, None) => String::from("now"),
        (Some((amt, sfx)), None) => format!("{}{}", amt, sfx),
        (Some((amt1, sfx1)), Some((amt2, sfx2))) => format!("{}{} {}{}", amt1, sfx1, amt2, sfx2),
        (None, Some(_)) => unreachable!(),
    }
}

pub fn ftime_or_dash(secs: i64) -> String {
    if secs <= 0 {
        String::from("-")
    } else {
        ftime(secs as u64)
    }
}

pub fn fdate(t: i64) -> String {
    epochs::unix(t).unwrap().to_string()
}

pub fn fdate_or_dash(t: i64) -> String {
    if t == 0 || t == -1 {
        String::from("-")
    } else {
        fdate(t)
    }
}

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
