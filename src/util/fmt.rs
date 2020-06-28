use std::fmt::Display;

use bytesize::ByteSize;
use pretty_dtoa::FmtFloatConfig;

pub fn bytes(amt: u64) -> String {
    ByteSize(amt).to_string_as(true)
}

pub fn bytes_limit(amt: f64) -> String {
    ByteSize((amt * 1024.0) as u64)
        .to_string_as(true)
        .replace(".0", "")
}

pub fn speed_pair(val: u64, max: f64) -> String {
    if max <= 0.0 {
        bytes(val) + "/s"
    } else {
        format!("{}/s ({}/s)", bytes(val), bytes_limit(max))
    }
}

pub fn pair<T, U: Display, F: FnMut(T) -> U>(mut f: F, a: T, b: Option<T>) -> String {
    match b {
        Some(b) => format!("{} ({})", f(a), f(b)),
        None => f(a).to_string(),
    }
}

pub fn percentage(val: f32) -> String {
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

    pretty_dtoa::ftoa(val, config)
}

pub fn duration(mut secs: u64) -> String {
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
        (Some((amt1, sfx1)), Some((amt2, sfx2))) => {
            format!("{}{} {}{}", amt1, sfx1, amt2, sfx2)
        }
        (None, Some(_)) => unreachable!(),
    }
}

pub fn time_or_dash(secs: i64) -> String {
    if secs <= 0 {
        String::from("-")
    } else {
        duration(secs as u64)
    }
}

pub fn date(t: i64) -> String {
    epochs::unix(t).unwrap().to_string()
}

pub fn date_or_dash(t: i64) -> String {
    if t == 0 || t == -1 {
        String::from("-")
    } else {
        date(t)
    }
}
