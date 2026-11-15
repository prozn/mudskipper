//! Chess rules and position representation.
//!
//! This module never depends on the rest of the engine.

mod bitboard;
mod board;
mod castling;
mod piece;
mod square;
mod zobrist;

pub use bitboard::{Bitboard, Squares};
pub use board::{Board, ParseFenError};
pub use castling::{CastlingRights, CastlingSide};
pub use piece::{Colour, Piece, PieceType};
pub use square::{ParseSquareError, Square};
