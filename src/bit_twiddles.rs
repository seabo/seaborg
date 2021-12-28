/// Returns the positive difference between two unsigned u8s.
#[inline(always)]
pub fn diff(x: u8, y: u8) -> u8 {
    if x < y {
        y - x
    } else {
        x - y
    }
}

/// Returns if there are more than one bits in a u64.
#[inline(always)]
pub fn more_than_one(x: u64) -> bool {
    (x & (x.wrapping_sub(1))) != 0
}
