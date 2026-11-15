//! Moves packed into 16 bits, and fixed-capacity lists of them.

use std::fmt;
use std::ops::Deref;

use super::piece::PieceType;
use super::square::Square;

/// A move, packed into 16 bits: 6 for the origin square, 6 for the target and 4 for its
/// kind.
///
/// Moves come only from move generation or from parsing against a position, so a `Move`
/// always describes a move that was available in some position. Castling is stored as the
/// king capturing its own rook (ADR-0001). [`Move::NULL`] stands for passing the turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Move(u16);

/// What a move does beyond moving a piece, stored in the top four bits of a [`Move`].
///
/// Bit 2 marks captures and bit 3 promotions, whose low two bits give the promotion piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Kind {
    Quiet = 0,
    DoublePush = 1,
    Castling = 2,
    Capture = 4,
    EnPassant = 5,
    KnightPromotion = 8,
    BishopPromotion = 9,
    RookPromotion = 10,
    QueenPromotion = 11,
    KnightPromotionCapture = 12,
    BishopPromotionCapture = 13,
    RookPromotionCapture = 14,
    QueenPromotionCapture = 15,
}

impl Kind {
    const CAPTURE_BIT: u8 = 0b0100;
    const PROMOTION_BIT: u8 = 0b1000;

    /// Every kind that promotes, by promotion piece from knight to queen, with or without a
    /// capture.
    pub(super) const fn promotion(piece_type: PieceType, capture: bool) -> Self {
        match (piece_type, capture) {
            (PieceType::Knight, false) => Self::KnightPromotion,
            (PieceType::Bishop, false) => Self::BishopPromotion,
            (PieceType::Rook, false) => Self::RookPromotion,
            (PieceType::Knight, true) => Self::KnightPromotionCapture,
            (PieceType::Bishop, true) => Self::BishopPromotionCapture,
            (PieceType::Rook, true) => Self::RookPromotionCapture,
            (_, false) => Self::QueenPromotion,
            (_, true) => Self::QueenPromotionCapture,
        }
    }

    const fn from_bits(bits: u8) -> Option<Self> {
        Some(match bits {
            0 => Self::Quiet,
            1 => Self::DoublePush,
            2 => Self::Castling,
            4 => Self::Capture,
            5 => Self::EnPassant,
            8 => Self::KnightPromotion,
            9 => Self::BishopPromotion,
            10 => Self::RookPromotion,
            11 => Self::QueenPromotion,
            12 => Self::KnightPromotionCapture,
            13 => Self::BishopPromotionCapture,
            14 => Self::RookPromotionCapture,
            15 => Self::QueenPromotionCapture,
            _ => return None,
        })
    }
}

impl Move {
    /// The null move, which passes the turn. It is written `0000` in UCI.
    pub const NULL: Self = Self(0);

    /// The promotion pieces, in the order moves are generated.
    pub(super) const PROMOTIONS: [PieceType; 4] = [
        PieceType::Queen,
        PieceType::Knight,
        PieceType::Rook,
        PieceType::Bishop,
    ];

    /// Packs a move. Castling moves go from the king to its own rook.
    pub(super) const fn new(from: Square, to: Square, kind: Kind) -> Self {
        Self(from.index() as u16 | (to.index() as u16) << 6 | (kind as u16) << 12)
    }

    /// Unpacks raw bits, for tests that try every possible encoding.
    #[cfg(test)]
    pub(super) const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// The square the moving piece starts on.
    #[must_use]
    pub const fn from(self) -> Square {
        Square::ALL[(self.0 & 0x3F) as usize]
    }

    /// The square the moving piece lands on, or for castling the castling rook's square.
    #[must_use]
    pub const fn to(self) -> Square {
        Square::ALL[(self.0 >> 6 & 0x3F) as usize]
    }

    /// The kind of the move, or `None` for the four unused kind codes.
    pub(super) const fn kind(self) -> Option<Kind> {
        Kind::from_bits(self.kind_bits())
    }

    const fn kind_bits(self) -> u8 {
        (self.0 >> 12) as u8
    }

    /// Reports whether this is the null move.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == Self::NULL.0
    }

    /// Reports whether the move captures a piece, including en passant.
    ///
    /// Castling is never a capture, although it is stored as the king taking its rook.
    #[must_use]
    pub const fn is_capture(self) -> bool {
        self.kind_bits() & Kind::CAPTURE_BIT != 0
    }

    /// Reports whether the move is an en passant capture.
    #[must_use]
    pub const fn is_en_passant(self) -> bool {
        self.kind_bits() == Kind::EnPassant as u8
    }

    /// Reports whether the move is castling.
    #[must_use]
    pub const fn is_castling(self) -> bool {
        self.kind_bits() == Kind::Castling as u8
    }

    /// The piece a pawn promotes to, if the move is a promotion.
    #[must_use]
    pub const fn promotion(self) -> Option<PieceType> {
        if self.kind_bits() & Kind::PROMOTION_BIT == 0 {
            return None;
        }
        Some(match self.kind_bits() & 0b11 {
            0 => PieceType::Knight,
            1 => PieceType::Bishop,
            2 => PieceType::Rook,
            _ => PieceType::Queen,
        })
    }

    /// Reports whether the move is noisy: a capture or a promotion.
    #[must_use]
    pub const fn is_noisy(self) -> bool {
        self.kind_bits() & (Kind::CAPTURE_BIT | Kind::PROMOTION_BIT) != 0
    }

    /// The square the king lands on when castling, on the c- or g-file of its rank.
    pub(super) const fn castling_king_target(self) -> Square {
        let file = if self.to().file() > self.from().file() {
            6
        } else {
            2
        };
        Square::ALL[self.from().rank() as usize * 8 + file]
    }

    /// The square the rook lands on when castling, on the d- or f-file of its rank.
    pub(super) const fn castling_rook_target(self) -> Square {
        let file = if self.to().file() > self.from().file() {
            5
        } else {
            3
        };
        Square::ALL[self.from().rank() as usize * 8 + file]
    }
}

impl fmt::Display for Move {
    /// Writes the move in UCI notation, such as `e2e4` or `e7e8q`.
    ///
    /// Castling is written as the king's move, such as `e1g1`, and the null move as `0000`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            return f.write_str("0000");
        }
        let to = if self.is_castling() {
            self.castling_king_target()
        } else {
            self.to()
        };
        write!(f, "{}{to}", self.from())?;
        if let Some(piece_type) = self.promotion() {
            let letter = match piece_type {
                PieceType::Knight => 'n',
                PieceType::Bishop => 'b',
                PieceType::Rook => 'r',
                _ => 'q',
            };
            write!(f, "{letter}")?;
        }
        Ok(())
    }
}

/// Upper bound on the number of moves in any position; the known maximum is 218.
const MAX_MOVES: usize = 256;

/// A list of moves with a fixed capacity, stored inline without allocating.
///
/// It dereferences to a slice of the moves it holds.
#[derive(Clone, Debug)]
pub struct MoveList {
    moves: [Move; MAX_MOVES],
    len: usize,
}

impl MoveList {
    /// Returns an empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            moves: [Move::NULL; MAX_MOVES],
            len: 0,
        }
    }

    /// Appends a move.
    pub(super) fn push(&mut self, mv: Move) {
        debug_assert!(self.len < MAX_MOVES, "move list overflow");
        self.moves[self.len] = mv;
        self.len += 1;
    }

    /// Keeps only the moves for which `keep` returns true, preserving their order.
    pub fn retain(&mut self, mut keep: impl FnMut(Move) -> bool) {
        let mut kept = 0;
        for i in 0..self.len {
            let mv = self.moves[i];
            if keep(mv) {
                self.moves[kept] = mv;
                kept += 1;
            }
        }
        self.len = kept;
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for MoveList {
    type Target = [Move];

    fn deref(&self) -> &[Move] {
        &self.moves[..self.len]
    }
}

impl<'list> IntoIterator for &'list MoveList {
    type Item = &'list Move;
    type IntoIter = std::slice::Iter<'list, Move>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
