//! Sets of squares packed into 64-bit integers.

use std::ops::{BitAnd, BitOr, BitXorAssign};

use super::square::Square;

/// A set of squares, one bit per square, with bit 0 for a1 and bit 63 for h8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bitboard(u64);

impl Bitboard {
    /// The empty set.
    pub const EMPTY: Self = Self(0);

    /// Returns the set holding only `square`.
    #[must_use]
    pub const fn from_square(square: Square) -> Self {
        Self(1 << square.index())
    }

    /// Reports whether `square` is in the set.
    #[must_use]
    pub const fn contains(self, square: Square) -> bool {
        self.0 & (1 << square.index()) != 0
    }

    /// Reports whether the set has no squares.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Number of squares in the set.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

impl BitAnd for Bitboard {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl BitOr for Bitboard {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitXorAssign for Bitboard {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl IntoIterator for Bitboard {
    type Item = Square;
    type IntoIter = Squares;

    fn into_iter(self) -> Squares {
        Squares(self.0)
    }
}

/// Iterator over the squares of a [`Bitboard`], from a1 towards h8.
#[derive(Clone, Debug)]
pub struct Squares(u64);

impl Iterator for Squares {
    type Item = Square;

    fn next(&mut self) -> Option<Square> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(Square::ALL[index])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count_ones() as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for Squares {}
