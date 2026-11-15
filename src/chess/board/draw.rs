//! Draw rules that depend only on the position: the fifty-move rule and insufficient
//! material.

use super::Board;
use crate::chess::bitboard::Bitboard;
use crate::chess::piece::PieceType;

/// Halfmoves without a capture or pawn move after which the game is drawn.
const FIFTY_MOVE_LIMIT: u16 = 100;

impl Board {
    /// Reports whether the game is drawn by the fifty-move rule.
    ///
    /// The rule applies once a hundred halfmoves have passed without a capture or pawn
    /// move, unless the last of them delivered checkmate.
    #[must_use]
    pub fn is_fifty_move_draw(&self) -> bool {
        self.halfmove_clock >= FIFTY_MOVE_LIMIT
            && !(self.in_check() && self.legal_moves().is_empty())
    }

    /// Reports whether neither side has enough material left to checkmate.
    ///
    /// That is the case with bare kings, with a single knight or bishop, or with any
    /// number of bishops that all stand on squares of one colour.
    #[must_use]
    pub fn has_insufficient_material(&self) -> bool {
        let heavy_or_pawn = self.piece_type(PieceType::Pawn)
            | self.piece_type(PieceType::Rook)
            | self.piece_type(PieceType::Queen);
        if !heavy_or_pawn.is_empty() {
            return false;
        }
        let knights = self.piece_type(PieceType::Knight);
        let bishops = self.piece_type(PieceType::Bishop);
        (knights | bishops).count() <= 1
            || (knights.is_empty()
                && ((bishops & Bitboard::LIGHT_SQUARES).is_empty()
                    || (bishops & !Bitboard::LIGHT_SQUARES).is_empty()))
    }
}
