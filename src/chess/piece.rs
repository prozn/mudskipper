//! Colours, piece types and pieces.

use std::fmt;
use std::ops::Not;

/// Rank a pawn starts on, counted from its own side as [`Colour::relative_rank`] does.
pub(super) const PAWN_START_RANK: u8 = 1;
/// Rank a pawn's double push skips, which becomes the en passant square.
pub(super) const DOUBLE_PUSH_SKIPPED_RANK: u8 = 2;
/// Rank a pawn lands on after a double push.
pub(super) const DOUBLE_PUSH_RANK: u8 = 3;
/// Rank a pawn must stand on to capture en passant.
pub(super) const EN_PASSANT_CAPTURER_RANK: u8 = 4;
/// Rank a pawn promotes on.
pub(super) const PROMOTION_RANK: u8 = 7;

/// The side a piece belongs to, or the side to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Colour {
    /// The side that moves first.
    White,
    /// The side that moves second.
    Black,
}

impl Colour {
    /// Both colours, in index order.
    pub const ALL: [Self; 2] = [Self::White, Self::Black];

    /// Index of the colour: 0 for white, 1 for black.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Converts between absolute ranks and ranks counted from this colour's side.
    ///
    /// Rank 0 is this colour's back rank and rank 7 the opponent's. The conversion is its
    /// own inverse, so it maps a relative rank to an absolute one and back.
    #[must_use]
    pub const fn relative_rank(self, rank: u8) -> u8 {
        match self {
            Self::White => rank,
            Self::Black => 7 - rank,
        }
    }

    /// The absolute rank this colour's pieces start on: 0 for white, 7 for black.
    #[must_use]
    pub const fn back_rank(self) -> u8 {
        self.relative_rank(0)
    }
}

impl fmt::Display for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::White => "white",
            Self::Black => "black",
        })
    }
}

impl Not for Colour {
    type Output = Self;

    fn not(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

/// The kind of a piece, regardless of its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PieceType {
    /// A pawn.
    Pawn,
    /// A knight.
    Knight,
    /// A bishop.
    Bishop,
    /// A rook.
    Rook,
    /// A queen.
    Queen,
    /// A king.
    King,
}

impl PieceType {
    /// Every piece type, in index order.
    pub const ALL: [Self; 6] = [
        Self::Pawn,
        Self::Knight,
        Self::Bishop,
        Self::Rook,
        Self::Queen,
        Self::King,
    ];

    /// Index of the piece type, from 0 (pawn) to 5 (king).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A piece of a given type and colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Piece {
    colour: Colour,
    piece_type: PieceType,
}

impl Piece {
    /// Returns the piece of type `piece_type` belonging to `colour`.
    #[must_use]
    pub const fn new(colour: Colour, piece_type: PieceType) -> Self {
        Self { colour, piece_type }
    }

    /// Colour of the piece.
    #[must_use]
    pub const fn colour(self) -> Colour {
        self.colour
    }

    /// Type of the piece.
    #[must_use]
    pub const fn piece_type(self) -> PieceType {
        self.piece_type
    }

    /// Index of the piece among all twelve, from 0 (white pawn) to 11 (black king).
    #[must_use]
    pub const fn index(self) -> usize {
        self.colour.index() * 6 + self.piece_type.index()
    }

    /// Returns the piece for a FEN letter: upper case for white, lower case for black.
    #[must_use]
    pub const fn from_char(c: char) -> Option<Self> {
        let colour = if c.is_ascii_uppercase() {
            Colour::White
        } else {
            Colour::Black
        };
        let piece_type = match c.to_ascii_lowercase() {
            'p' => PieceType::Pawn,
            'n' => PieceType::Knight,
            'b' => PieceType::Bishop,
            'r' => PieceType::Rook,
            'q' => PieceType::Queen,
            'k' => PieceType::King,
            _ => return None,
        };
        Some(Self::new(colour, piece_type))
    }

    /// FEN letter of the piece: upper case for white, lower case for black.
    #[must_use]
    pub const fn to_char(self) -> char {
        let c = match self.piece_type {
            PieceType::Pawn => 'p',
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            PieceType::King => 'k',
        };
        match self.colour {
            Colour::White => c.to_ascii_uppercase(),
            Colour::Black => c,
        }
    }
}
