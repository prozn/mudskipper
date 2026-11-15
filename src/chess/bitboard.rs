//! Sets of squares packed into 64-bit integers.

use std::ops::{BitAnd, BitOr, BitXorAssign, Not};

use super::piece::Colour;
use super::square::Square;

/// A set of squares, one bit per square, with bit 0 for a1 and bit 63 for h8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bitboard(u64);

impl Bitboard {
    /// The empty set.
    pub const EMPTY: Self = Self(0);

    /// Returns the set whose bit `i` is set for each square with index `i` in `bits`.
    pub(crate) const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// The raw bits of the set, bit `i` standing for the square with index `i`.
    pub(crate) const fn bits(self) -> u64 {
        self.0
    }

    /// Returns the set holding only `square`.
    #[must_use]
    pub const fn from_square(square: Square) -> Self {
        Self(1 << square.index())
    }

    /// Returns the set of the eight squares on `rank`, from 0 (first rank) to 7.
    #[must_use]
    pub const fn rank(rank: u8) -> Self {
        Self(0xFF << (8 * rank))
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

    /// The square of the set closest to a1, if the set is not empty.
    #[must_use]
    pub const fn first(self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            Some(Square::ALL[self.0.trailing_zeros() as usize])
        }
    }

    /// The set moved one rank towards the eighth rank; squares on it fall off.
    #[must_use]
    pub const fn north(self) -> Self {
        Self(self.0 << 8)
    }

    /// The set moved one rank towards the first rank; squares on it fall off.
    #[must_use]
    pub const fn south(self) -> Self {
        Self(self.0 >> 8)
    }

    /// The set moved one rank forward from `colour`'s point of view.
    #[must_use]
    pub const fn forward(self, colour: Colour) -> Self {
        match colour {
            Colour::White => self.north(),
            Colour::Black => self.south(),
        }
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

impl Not for Bitboard {
    type Output = Self;

    fn not(self) -> Self {
        Self(!self.0)
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
