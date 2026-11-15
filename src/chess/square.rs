//! Squares of the board.

use std::error::Error;
use std::fmt;
use std::str::FromStr;

use super::piece::Colour;

/// One of the 64 squares, numbered from a1 = 0 across each rank to h8 = 63.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Square(u8);

impl Square {
    /// Every square in index order, from a1 to h8.
    pub const ALL: [Self; 64] = {
        let mut all = [Self(0); 64];
        let mut i = 0;
        while i < 64 {
            all[i] = Self(i as u8);
            i += 1;
        }
        all
    };

    /// Returns the square on `file` (0 is the a-file) and `rank` (0 is the first rank).
    ///
    /// Returns `None` if either coordinate is off the board.
    #[must_use]
    pub const fn from_coords(file: u8, rank: u8) -> Option<Self> {
        if file < 8 && rank < 8 {
            Some(Self(rank * 8 + file))
        } else {
            None
        }
    }

    /// Index of the square, from 0 (a1) to 63 (h8).
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// File of the square, from 0 (a-file) to 7 (h-file).
    #[must_use]
    pub const fn file(self) -> u8 {
        self.0 % 8
    }

    /// Rank of the square, from 0 (first rank) to 7 (eighth rank).
    #[must_use]
    pub const fn rank(self) -> u8 {
        self.0 / 8
    }

    /// Rank of the square counted from `colour`'s side, so 0 is its back rank.
    #[must_use]
    pub const fn relative_rank(self, colour: Colour) -> u8 {
        colour.relative_rank(self.rank())
    }

    /// The square `ranks` ranks forward from `colour`'s point of view, which must be on the
    /// board.
    pub(super) const fn forward(self, colour: Colour, ranks: u8) -> Self {
        let index = match colour {
            Colour::White => self.0 + 8 * ranks,
            Colour::Black => self.0 - 8 * ranks,
        };
        Self(index)
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = char::from(b'a' + self.file());
        let rank = char::from(b'1' + self.rank());
        write!(f, "{file}{rank}")
    }
}

impl FromStr for Square {
    type Err = ParseSquareError;

    /// Parses a square in algebraic notation, such as `e4`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.as_bytes() {
            &[file @ b'a'..=b'h', rank @ b'1'..=b'8'] => {
                Self::from_coords(file - b'a', rank - b'1').ok_or(ParseSquareError)
            }
            _ => Err(ParseSquareError),
        }
    }
}

/// Error returned when text is not a square in algebraic notation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseSquareError;

impl fmt::Display for ParseSquareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid square")
    }
}

impl Error for ParseSquareError {}
