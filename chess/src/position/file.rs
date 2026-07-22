use std::fmt;

/// One of the eight files of a chess board, from the a-file to the h-file.
///
/// The discriminants match the low three bits of a square index, so the file of
/// a square is a mask rather than a conversion, and `index()` is the correct
/// subscript into any file-indexed table such as `masks::FILE_BB`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum File {
    A = 0,
    B = 1,
    C = 2,
    D = 3,
    E = 4,
    F = 5,
    G = 6,
    H = 7,
}

/// Every file in index order. Indexing this with a value already masked to
/// `0..8` is how the hot paths get a `File` without a bounds check surviving
/// optimisation.
const FILES: [File; 8] = [
    File::A,
    File::B,
    File::C,
    File::D,
    File::E,
    File::F,
    File::G,
    File::H,
];

impl File {
    /// Returns the zero-based index of this file, counting from the a-file.
    #[inline(always)]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Returns the file occupying the low three bits of `bits`, ignoring the
    /// rest. This is total by construction: masking leaves eight possible
    /// values and every one of them names a file.
    #[inline(always)]
    pub(crate) const fn from_low_bits(bits: u8) -> Self {
        FILES[(bits & 0b0000_0111) as usize]
    }

    /// Returns the lower-case letter naming this file in algebraic notation.
    #[inline]
    pub const fn to_char(self) -> char {
        (b'a' + self as u8) as char
    }
}

impl TryFrom<u8> for File {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < 8 {
            Ok(FILES[value as usize])
        } else {
            Err(())
        }
    }
}

impl From<File> for u8 {
    fn from(file: File) -> Self {
        file as u8
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_indices_round_trip() {
        for (idx, file) in FILES.iter().enumerate() {
            assert_eq!(File::try_from(idx as u8), Ok(*file));
            assert_eq!(file.index() as usize, idx);
        }
    }

    #[test]
    fn raw_file_indices_are_checked() {
        assert_eq!(File::try_from(8), Err(()));
        assert_eq!(File::try_from(u8::MAX), Err(()));
    }

    #[test]
    fn files_are_named_by_their_algebraic_letter() {
        assert_eq!(File::A.to_char(), 'a');
        assert_eq!(File::H.to_char(), 'h');
        assert_eq!(File::D.to_string(), "d");
    }

    #[test]
    fn high_bits_do_not_affect_the_file() {
        // The whole point of the discriminants is that a square index needs no
        // conversion beyond dropping its rank bits.
        assert_eq!(File::from_low_bits(0b111_101), File::F);
        assert_eq!(File::from_low_bits(0b000_101), File::F);
    }
}
