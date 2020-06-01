use bytesize::ByteSize;

pub fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

pub fn fmt_bytes(amt: u64, suffix: &str) -> String {
    ByteSize(amt).to_string_as(true) + suffix
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

    match (years, weeks, days, hours, mins, secs) {
        (0, 0, 0, 0, 0, s) => format!("{}s", s),

        (0, 0, 0, 0, m, s) => format!("{}m {}s", m, s),

        (0, 0, 0, h, 0, s) => format!("{}h {}s", h, s),
        (0, 0, 0, h, m, _) => format!("{}h {}m", h, m),

        (0, 0, d, 0, 0, s) => format!("{}d {}s", d, s),
        (0, 0, d, 0, m, _) => format!("{}d {}m", d, m),
        (0, 0, d, h, _, _) => format!("{}d {}h", d, h),

        (0, w, 0, 0, 0, s) => format!("{}w {}s", w, s),
        (0, w, 0, 0, m, _) => format!("{}w {}m", w, m),
        (0, w, 0, h, _, _) => format!("{}w {}h", w, h),
        (0, w, d, _, _, _) => format!("{}w {}d", w, d),

        (y, 0, 0, 0, 0, s) => format!("{}y {}s", y, s),
        (y, 0, 0, 0, m, _) => format!("{}y {}m", y, m),
        (y, 0, 0, h, _, _) => format!("{}y {}h", y, h),
        (y, 0, d, _, _, _) => format!("{}y {}d", y, d),
        (y, w, _, _, _, _) => format!("{}y {}w", y, w),
    }
}

pub fn ftime_or_dash(secs: i64) -> String {
    if secs <= 0 {
        String::from("-")
    } else {
        ftime(secs as u64)
    }
}
