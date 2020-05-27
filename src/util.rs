use human_format::{Formatter, Scales};

pub fn fmt_bytes(amt: u64, units: &str) -> String {
    Formatter::new()
        .with_scales(Scales::Binary())
        .with_units(units)
        .format(amt as f64)
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
