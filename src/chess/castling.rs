//! Castling rights, stored as the starting squares of the castling rooks (ADR-0001).

use super::piece::Colour;
use super::square::Square;

/// The side of the king a castling rook starts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CastlingSide {
    /// Towards the h-file.
    King,
    /// Towards the a-file.
    Queen,
}

impl CastlingSide {
    /// Both sides, king side first, as FEN lists them.
    pub const ALL: [Self; 2] = [Self::King, Self::Queen];

    const fn index(self) -> usize {
        self as usize
    }
}

/// The castling rights of both colours.
///
/// Each right is the starting square of the rook that may still castle, so standard chess
/// is one Chess960 start position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CastlingRights {
    rooks: [[Option<Square>; 2]; 2],
}

impl CastlingRights {
    /// No castling rights for either colour.
    pub const EMPTY: Self = Self {
        rooks: [[None; 2]; 2],
    };

    /// Starting square of the rook `colour` may castle with on `side`, if it still may.
    #[must_use]
    pub const fn rook(self, colour: Colour, side: CastlingSide) -> Option<Square> {
        self.rooks[colour.index()][side.index()]
    }

    /// Reports whether neither colour may castle.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.rooks.iter().flatten().all(Option::is_none)
    }

    /// Grants `colour` the right to castle on `side` with the rook starting on `rook`.
    pub(super) fn grant(&mut self, colour: Colour, side: CastlingSide, rook: Square) {
        self.rooks[colour.index()][side.index()] = Some(rook);
    }

    /// Starting squares of every rook that may still castle.
    pub(super) fn squares(self) -> impl Iterator<Item = Square> {
        self.rooks.into_iter().flatten().flatten()
    }
}
