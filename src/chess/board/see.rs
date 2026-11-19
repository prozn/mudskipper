//! Static exchange evaluation: the material a move wins or loses once the captures on its
//! target square have been played out, each side always recapturing with its least
//! valuable piece and free to stop when recapturing would lose.

use super::Board;
use crate::chess::attacks;
use crate::chess::bitboard::Bitboard;
use crate::chess::moves::Move;
use crate::chess::piece::PieceType;

/// Piece values for static exchange evaluation, indexed by [`PieceType::index`]: the
/// classic 1, 3, 3, 5, 9 pawn scale. They decide which exchanges count as even, so raising
/// the minor pieces above three pawns, say, would make a knight for three pawns a loss. The
/// king is never captured, so its value is never counted.
const SEE_VALUES: [i32; 6] = [100, 300, 300, 500, 900, 0];

/// The static exchange value of a piece of type `piece_type`.
const fn value(piece_type: PieceType) -> i32 {
    SEE_VALUES[piece_type.index()]
}

impl Board {
    /// Reports whether the pseudo-legal move `mv` wins at least `threshold` in material
    /// by static exchange evaluation.
    ///
    /// Captures on the target square alternate, each side using its least valuable
    /// attacker, including sliders uncovered behind earlier capturers, and either side may
    /// stop when capturing on would lose. A promotion counts the new piece less the pawn,
    /// and castling is worth nothing. Pins are ignored, as is a pawn promoting as it
    /// recaptures on the last rank. A king's recapture uncovers no sliders behind it,
    /// which can matter only when `mv` leaves the mover's king in check.
    ///
    /// # Panics
    ///
    /// May panic if `mv` is not pseudo-legal in this position.
    #[must_use]
    pub fn see_ge(&self, mv: Move, threshold: i32) -> bool {
        if mv.is_castling() {
            return threshold <= 0;
        }
        let (from, to) = (mv.from(), mv.to());
        let mut gain = self.captured(mv).map_or(0, value);
        // The piece left standing on the target square, open to recapture.
        let mut exposed = self
            .piece_at(from)
            .unwrap_or_else(|| panic!("{mv} has no piece to move in {}", self.to_fen()))
            .piece_type();
        if let Some(promotion) = mv.promotion() {
            gain += value(promotion) - value(PieceType::Pawn);
            exposed = promotion;
        }

        // The balance is what the mover has won, less the threshold, if the exchange stops
        // here. The mover wins if it is non-negative even after losing the exposed piece.
        let mut balance = gain - threshold;
        if balance < 0 {
            return false;
        }
        balance -= value(exposed);
        if balance >= 0 {
            return true;
        }

        let mut occupied = self.occupied();
        occupied ^= Bitboard::from_square(from);
        if mv.is_en_passant() {
            occupied ^= Bitboard::from_square(mv.captured_square());
        }
        let mut attackers = self.attackers_to(to, occupied) & occupied;
        let diagonal = self.piece_type(PieceType::Bishop) | self.piece_type(PieceType::Queen);
        let straight = self.piece_type(PieceType::Rook) | self.piece_type(PieceType::Queen);
        // `side` is the side to capture next. Each capture turns the balance round to the
        // capturing side's point of view, as what it has won if its capturing piece is
        // then taken back. Negating less one keeps `>= 0` as the winning test, since an
        // integer `b` is non-negative exactly when `-b - 1` is negative. The exchange is
        // lost by `side` when it has nothing left to capture with, or when the other side's
        // balance shows it ahead even after losing its capturing piece.
        let mut side = !self.side_to_move;
        loop {
            let ours = attackers & self.colour(side);
            let Some(attacker) = PieceType::ALL
                .into_iter()
                .find(|&piece_type| !(ours & self.piece_type(piece_type)).is_empty())
            else {
                break;
            };
            let square = (ours & self.piece_type(attacker))
                .first()
                .expect("the attacker type was found among our attackers");
            occupied ^= Bitboard::from_square(square);
            // Moving the attacker off its square may uncover a slider behind it.
            if matches!(
                attacker,
                PieceType::Pawn | PieceType::Bishop | PieceType::Queen
            ) {
                attackers = attackers | (attacks::bishop(to, occupied) & diagonal);
            }
            if matches!(attacker, PieceType::Rook | PieceType::Queen) {
                attackers = attackers | (attacks::rook(to, occupied) & straight);
            }
            attackers = attackers & occupied;
            side = !side;
            balance = -balance - 1 - value(attacker);
            if balance >= 0 {
                // A king may not capture onto a square the other side still attacks, so
                // then the capture falls to the other side instead.
                if attacker == PieceType::King && !(attackers & self.colour(side)).is_empty() {
                    side = !side;
                }
                break;
            }
        }
        side != self.side_to_move
    }
}
