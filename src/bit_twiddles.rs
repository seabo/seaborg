/// Returns the positive difference between two unsigned u8s.
#[inline(always)]
pub fn diff(x: u8, y: u8) -> u8 {
    if x < y {
        y - x
    } else {
        x - y
    }
}
