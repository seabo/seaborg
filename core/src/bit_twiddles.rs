/// Returns if there are more than one bits in a u64.
#[inline(always)]
pub fn more_than_one(x: u64) -> bool {
    (x & (x.wrapping_sub(1))) != 0
}

/// Isolates the least significant bit of a u64.
#[inline(always)]
pub fn lsb(x: u64) -> u64 {
    x & x.overflowing_neg().0 as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsb_works() {
        assert_eq!(lsb(3523476), 4);
        assert_eq!(lsb(2346467342467), 1);
        assert_eq!(lsb(239889852416), 2_048);
        assert_eq!(lsb(0), 0);
    }
}
