//! Making moves by copying the board (ADR-0003).

use super::Board;
use crate::chess::moves::{Kind, Move};
use crate::chess::piece::{Colour, Piece, PieceType};
use crate::chess::zobrist;

impl Board {
    /// Returns the position after playing `mv`, leaving this one unchanged.
    ///
    /// The Zobrist key is updated incrementally. After a double pawn push the skipped
    /// square is recorded as the en passant square, as FEN records it.
    ///
    /// # Panics
    ///
    /// `mv` must be pseudo-legal in this position. Making any other move, including
    /// [`Move::NULL`], may panic or leave the board inconsistent.
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
        board.halfmove_clock = board.halfmove_clock.saturating_add(1);

        let piece = if mv.is_castling() {
            let king = board.remove_piece(from);
            let rook = board.remove_piece(to);
            board.put_piece(king, mv.castling_king_target());
            board.put_piece(rook, mv.castling_rook_target());
            king
        } else {
            if mv.is_capture() {
                board.remove_piece(mv.captured_square());
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
            board.fullmove_number = board.fullmove_number.saturating_add(1);
        }

        if mv.kind() == Some(Kind::DoublePush) {
            let target = from.forward(us, 1);
            board.en_passant = Some(target);
            if board.can_capture_en_passant(target) {
                board.key ^= zobrist::en_passant(target.file());
            }
        }
        board
    }
}
