//! Colours, piece types and pieces.

use std::ops::Not;

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
