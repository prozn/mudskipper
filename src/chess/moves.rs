//! Moves packed into 16 bits, and fixed-capacity lists of them.

use std::fmt;
use std::ops::Deref;

use super::notation::CastlingNotation;
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

    /// The promotions without a capture, in generation order: queen first.
    pub(super) const PROMOTIONS: [Self; 4] = [
        Self::QueenPromotion,
        Self::KnightPromotion,
        Self::RookPromotion,
        Self::BishopPromotion,
    ];

    /// The promotions with a capture, in the same order as [`Kind::PROMOTIONS`].
    pub(super) const PROMOTION_CAPTURES: [Self; 4] = [
        Self::QueenPromotionCapture,
        Self::KnightPromotionCapture,
        Self::RookPromotionCapture,
        Self::BishopPromotionCapture,
    ];

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

/// Bits of a [`Move`] holding one square.
const SQUARE_MASK: u16 = 0x3F;
/// Position of the target square in a [`Move`].
const TO_SHIFT: u16 = 6;
/// Position of the [`Kind`] in a [`Move`].
const KIND_SHIFT: u16 = 12;

/// Files the king and rook land on when castling: (king, rook).
const KING_SIDE_FILES: (u8, u8) = (6, 5);
const QUEEN_SIDE_FILES: (u8, u8) = (2, 3);

impl Move {
    /// The null move, which passes the turn. It is written `0000` in UCI.
    pub const NULL: Self = Self(0);

    /// The pieces a pawn may promote to.
    pub(super) const PROMOTION_PIECES: [PieceType; 4] = [
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
    ];

    /// Packs a move. Castling moves go from the king to its own rook.
    pub(super) const fn new(from: Square, to: Square, kind: Kind) -> Self {
        Self(from.index() as u16 | (to.index() as u16) << TO_SHIFT | (kind as u16) << KIND_SHIFT)
    }

    /// Unpacks the raw bits of a move, as returned by [`Move::to_bits`].
    ///
    /// The result need not be a move in any position, nor even a valid encoding, so check
    /// it with [`Board::is_pseudo_legal`](super::Board::is_pseudo_legal) before use.
    pub(crate) const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// The raw bits of the move. The null move is 0.
    pub(crate) const fn to_bits(self) -> u16 {
        self.0
    }

    /// The square the moving piece starts on.
    #[must_use]
    pub const fn from(self) -> Square {
        Square::ALL[(self.0 & SQUARE_MASK) as usize]
    }

    /// The square the moving piece lands on, or for castling the castling rook's square.
    #[must_use]
    pub const fn to(self) -> Square {
        Square::ALL[(self.0 >> TO_SHIFT & SQUARE_MASK) as usize]
    }

    /// The kind of the move, or `None` for the four unused kind codes.
    pub(super) const fn kind(self) -> Option<Kind> {
        Kind::from_bits(self.kind_bits())
    }

    const fn kind_bits(self) -> u8 {
        (self.0 >> KIND_SHIFT) as u8
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

    /// The square the moving piece lands on; for castling, the king's landing square.
    ///
    /// This is the target square UCI notation writes.
    pub(super) const fn destination(self) -> Square {
        if self.is_castling() {
            self.castling_king_target()
        } else {
            self.to()
        }
    }

    /// The square of the piece the move captures: the target square, or for en passant
    /// the square of the pawn that just moved, beside the capturing pawn.
    pub(super) const fn captured_square(self) -> Square {
        if self.is_en_passant() {
            Square::ALL[self.from().rank() as usize * 8 + self.to().file() as usize]
        } else {
            self.to()
        }
    }

    /// The square the king lands on when castling, on the c- or g-file of its rank.
    pub(super) const fn castling_king_target(self) -> Square {
        self.castling_square(self.castling_files().0)
    }

    /// The square the rook lands on when castling, on the d- or f-file of its rank.
    pub(super) const fn castling_rook_target(self) -> Square {
        self.castling_square(self.castling_files().1)
    }

    const fn castling_files(self) -> (u8, u8) {
        if self.to().file() > self.from().file() {
            KING_SIDE_FILES
        } else {
            QUEEN_SIDE_FILES
        }
    }

    const fn castling_square(self, file: u8) -> Square {
        Square::ALL[self.from().rank() as usize * 8 + file as usize]
    }
}

/// The letter UCI notation uses for promoting to `piece_type`.
pub(super) const fn promotion_letter(piece_type: PieceType) -> char {
    match piece_type {
        PieceType::Knight => 'n',
        PieceType::Bishop => 'b',
        PieceType::Rook => 'r',
        PieceType::Queen => 'q',
        PieceType::Pawn | PieceType::King => panic!("pawns promote to N, B, R or Q only"),
    }
}

impl fmt::Display for Move {
    /// Writes the move in UCI notation, such as `e2e4` or `e7e8q`.
    ///
    /// Castling is written as the king's move, such as `e1g1`, and the null move as `0000`.
    /// Use [`Move::display`] to write castling as the king taking its rook.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.display(CastlingNotation::Standard).fmt(f)
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
    /// The most moves a list can hold, which is more than any position has.
    pub const CAPACITY: usize = MAX_MOVES;

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
