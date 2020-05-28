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
