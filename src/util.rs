pub mod eventual;
pub mod fmt;
pub mod simple_slab;

pub const fn digit_width(mut n: u64) -> usize {
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
