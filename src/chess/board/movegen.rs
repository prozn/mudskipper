//! Pseudo-legal move generation and the legality checks that filter it (ADR-0003).

use super::Board;
use crate::chess::attacks;
use crate::chess::bitboard::Bitboard;
use crate::chess::castling::CastlingSide;
use crate::chess::moves::{Kind, Move, MoveList};
use crate::chess::piece::{DOUBLE_PUSH_SKIPPED_RANK, PAWN_START_RANK, PROMOTION_RANK, PieceType};
use crate::chess::square::Square;

/// Which moves a generation pass produces.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    /// Every pseudo-legal move.
    All,
    /// Only captures and promotions.
    Noisy,
}

impl Board {
    /// Generates every pseudo-legal move: moves that follow the piece movement rules but
    /// may leave the mover's own king in check.
    ///
    /// Filter with [`Board::is_legal`], or use [`Board::legal_moves`].
    #[must_use]
    pub fn pseudo_legal_moves(&self) -> MoveList {
        self.generate(Selection::All)
    }

    /// Generates the pseudo-legal noisy moves: captures, including en passant, and
    /// promotions.
    #[must_use]
    pub fn noisy_moves(&self) -> MoveList {
        self.generate(Selection::Noisy)
    }

    /// Generates every legal move.
    #[must_use]
    pub fn legal_moves(&self) -> MoveList {
        let mut moves = self.pseudo_legal_moves();
        moves.retain(|mv| self.is_legal(mv));
        moves
    }

    /// Reports whether the pseudo-legal move `mv` leaves the mover's king out of check,
    /// and for castling whether the king also avoids passing through or starting in check.
    ///
    /// The result is meaningful only for moves that are pseudo-legal in this position, as
    /// generated or as checked by [`Board::is_pseudo_legal`].
    #[must_use]
    pub fn is_legal(&self, mv: Move) -> bool {
        let us = self.side_to_move;
        let them = !us;
        let (from, to) = (mv.from(), mv.to());
        let occupied = self.occupied();

        if mv.is_castling() {
            let king_target = mv.castling_king_target();
            if rank_span(from, king_target)
                .into_iter()
                .any(|square| self.is_attacked(square, them, occupied))
            {
                return false;
            }
            // The castling rook may have shielded the king's target along the back rank.
            let after = (occupied & !(Bitboard::from_square(from) | Bitboard::from_square(to)))
                | Bitboard::from_square(king_target)
                | Bitboard::from_square(mv.castling_rook_target());
            return !self.is_attacked(king_target, them, after);
        }

        if from == self.king(us) {
            let without_king = occupied & !Bitboard::from_square(from);
            return !self.is_attacked(to, them, without_king);
        }

        let captured = mv.captured_square();
        let after = (occupied & !Bitboard::from_square(from) & !Bitboard::from_square(captured))
            | Bitboard::from_square(to);
        let attackers = self.attackers_to(self.king(us), after)
            & self.colour(them)
            & !Bitboard::from_square(captured);
        attackers.is_empty()
    }

    /// Reports whether `mv` is pseudo-legal in this position, so that generation would
    /// produce it.
    ///
    /// This validates moves that were generated in some other position, such as moves
    /// remembered by the search.
    #[must_use]
    pub fn is_pseudo_legal(&self, mv: Move) -> bool {
        let us = self.side_to_move;
        let (from, to) = (mv.from(), mv.to());
        let Some(kind) = mv.kind() else {
            return false;
        };
        let Some(piece) = self.piece_at(from) else {
            return false;
        };
        if piece.colour() != us || mv.is_null() {
            return false;
        }

        if kind == Kind::Castling {
            return piece.piece_type() == PieceType::King
                && CastlingSide::ALL
                    .into_iter()
                    .any(|side| self.castling.rook(us, side) == Some(to))
                && self.castling_path_is_clear(mv);
        }

        let target = self.piece_at(to);
        if target.is_some_and(|target| target.colour() == us) {
            return false;
        }
        let captures = target.is_some();

        if piece.piece_type() != PieceType::Pawn {
            let kind_matches = match kind {
                Kind::Quiet => !captures,
                Kind::Capture => captures,
                _ => false,
            };
            return kind_matches
                && attacks::piece(piece.piece_type(), from, self.occupied()).contains(to);
        }

        let attacks = attacks::pawn(us, from);
        if kind == Kind::EnPassant {
            return self.en_passant == Some(to) && attacks.contains(to);
        }
        if kind == Kind::DoublePush {
            return from.relative_rank(us) == PAWN_START_RANK
                && to == from.forward(us, 2)
                && self.piece_at(from.forward(us, 1)).is_none()
                && !captures;
        }
        let promotes = to.relative_rank(us) == PROMOTION_RANK;
        if promotes != mv.promotion().is_some() {
            return false;
        }
        if mv.is_capture() {
            captures && attacks.contains(to)
        } else {
            !captures && to == from.forward(us, 1)
        }
    }

    fn generate(&self, selection: Selection) -> MoveList {
        let mut list = MoveList::new();
        let us = self.side_to_move;
        let them = self.colour(!us);
        let targets = match selection {
            Selection::All => !self.colour(us),
            Selection::Noisy => them,
        };

        self.generate_pawn_moves(&mut list, selection);
        let occupied = self.occupied();
        for piece_type in [
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
            PieceType::King,
        ] {
            for from in self.pieces(us, piece_type) {
                for to in attacks::piece(piece_type, from, occupied) & targets {
                    let kind = if them.contains(to) {
                        Kind::Capture
                    } else {
                        Kind::Quiet
                    };
                    list.push(Move::new(from, to, kind));
                }
            }
        }

        if selection == Selection::All {
            for side in CastlingSide::ALL {
                if let Some(rook) = self.castling.rook(us, side) {
                    let mv = Move::new(self.king(us), rook, Kind::Castling);
                    if self.castling_path_is_clear(mv) {
                        list.push(mv);
                    }
                }
            }
        }
        list
    }

    fn generate_pawn_moves(&self, list: &mut MoveList, selection: Selection) {
        let us = self.side_to_move;
        let them = self.colour(!us);
        let empty = !self.occupied();
        let pawns = self.pieces(us, PieceType::Pawn);
        let last_rank = Bitboard::rank(us.relative_rank(PROMOTION_RANK));
        let skipped_rank = Bitboard::rank(us.relative_rank(DOUBLE_PUSH_SKIPPED_RANK));

        let single = pawns.forward(us) & empty;
        for to in single & last_rank {
            push_promotions(list, to.forward(!us, 1), to, Kind::PROMOTIONS);
        }
        if selection == Selection::All {
            for to in single & !last_rank {
                list.push(Move::new(to.forward(!us, 1), to, Kind::Quiet));
            }
            for to in (single & skipped_rank).forward(us) & empty {
                list.push(Move::new(to.forward(!us, 2), to, Kind::DoublePush));
            }
        }

        for from in pawns {
            for to in attacks::pawn(us, from) & them {
                if last_rank.contains(to) {
                    push_promotions(list, from, to, Kind::PROMOTION_CAPTURES);
                } else {
                    list.push(Move::new(from, to, Kind::Capture));
                }
            }
        }

        if let Some(target) = self.en_passant {
            for from in attacks::pawn(!us, target) & pawns {
                list.push(Move::new(from, target, Kind::EnPassant));
            }
        }
    }

    /// Reports whether every square the king and rook of the castling move `mv` cross or
    /// land on is empty, apart from the two of them.
    fn castling_path_is_clear(&self, mv: Move) -> bool {
        let (king, rook) = (mv.from(), mv.to());
        let movers = Bitboard::from_square(king) | Bitboard::from_square(rook);
        let path =
            rank_span(king, mv.castling_king_target()) | rank_span(rook, mv.castling_rook_target());
        (path & self.occupied() & !movers).is_empty()
    }
}

/// Adds the four promotions of a pawn moving from `from` to `to`.
fn push_promotions(list: &mut MoveList, from: Square, to: Square, kinds: [Kind; 4]) {
    for kind in kinds {
        list.push(Move::new(from, to, kind));
    }
}

/// The squares from `a` to `b` inclusive, which must be on the same rank.
fn rank_span(a: Square, b: Square) -> Bitboard {
    debug_assert_eq!(a.rank(), b.rank(), "{a} and {b} are on different ranks");
    let (low, high) = if a <= b { (a, b) } else { (b, a) };
    Bitboard::from_bits((2u64 << high.index()).wrapping_sub(1 << low.index()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Positions covering castling, en passant, promotions and pins.
    const POSITIONS: [&str; 6] = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
    ];

    #[test]
    fn is_pseudo_legal_accepts_exactly_the_generated_moves_among_all_encodings() {
        for fen in POSITIONS {
            let board = Board::from_fen(fen).expect("test FEN is valid");
            let generated = board.pseudo_legal_moves();
            for bits in 0..=u16::MAX {
                let mv = Move::from_bits(bits);
                assert_eq!(
                    board.is_pseudo_legal(mv),
                    generated.contains(&mv),
                    "{mv} ({bits:#06x}, {:?}) in {fen:?}",
                    mv.kind()
                );
            }
        }
    }
}
