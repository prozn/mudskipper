//! Making moves by copying the board (ADR-0003).

use super::Board;
use crate::chess::moves::Move;
use crate::chess::piece::{Colour, Piece, PieceType};
use crate::chess::zobrist;

impl Board {
    /// Returns the position after playing `mv`, leaving this one unchanged.
    ///
    /// The Zobrist key is updated incrementally. `mv` must be pseudo-legal in this
    /// position; making any other move, including [`Move::NULL`], leaves the board
    /// inconsistent or panics.
    #[must_use]
    pub fn make(&self, mv: Move) -> Self {
        debug_assert!(self.is_pseudo_legal(mv), "{mv} is not pseudo-legal");
        let mut board = self.clone();
        let us = self.side_to_move;
        let (from, to) = (mv.from(), mv.to());

        if let Some(target) = board.en_passant.take()
            && self.can_capture_en_passant(target)
        {
            board.key ^= zobrist::en_passant(target.file());
        }
        board.key ^= zobrist::castling_rights(board.castling);
        board.halfmove_clock += 1;

        let piece = if mv.is_castling() {
            let king = board.remove_piece(from);
            let rook = board.remove_piece(to);
            board.put_piece(king, mv.castling_king_target());
            board.put_piece(rook, mv.castling_rook_target());
            king
        } else {
            if mv.is_capture() {
                let captured = if mv.is_en_passant() {
                    to.forward(!us, 1)
                } else {
                    to
                };
                board.remove_piece(captured);
                board.halfmove_clock = 0;
            }
            let piece = board.remove_piece(from);
            let placed = mv
                .promotion()
                .map_or(piece, |piece_type| Piece::new(us, piece_type));
            board.put_piece(placed, to);
            piece
        };

        match piece.piece_type() {
            PieceType::Pawn => board.halfmove_clock = 0,
            PieceType::King => board.castling.revoke_all(us),
            _ => {}
        }
        board.castling.revoke(from);
        board.castling.revoke(to);
        board.key ^= zobrist::castling_rights(board.castling);

        board.side_to_move = !us;
        board.key ^= zobrist::black_to_move();
        if us == Colour::Black {
            board.fullmove_number += 1;
        }

        let double_push =
            piece.piece_type() == PieceType::Pawn && from.rank().abs_diff(to.rank()) == 2;
        if double_push {
            let target = from.forward(us, 1);
            if board.can_capture_en_passant(target) {
                board.en_passant = Some(target);
                board.key ^= zobrist::en_passant(target.file());
            }
        }
        board
    }
}
