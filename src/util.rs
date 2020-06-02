use bytesize::ByteSize;
use std::fmt::Display;

pub fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

pub fn fmt_bytes(amt: u64) -> String {
    ByteSize(amt).to_string_as(true)
}

pub fn fmt_bytes_limit(amt: f64) -> String {
    ByteSize((amt * 1024.0) as u64).to_string_as(true).replace(".0", "")
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

pub fn digit_width(mut n: u64) -> usize {
    if n == 0 { return 1; }

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
