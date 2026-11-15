//! Chess rules and position representation.
//!
//! This module never depends on the rest of the engine.

mod attacks;
mod bitboard;
mod board;
mod castling;
mod moves;
mod notation;
mod perft;
mod piece;
mod square;
mod zobrist;

pub use bitboard::{Bitboard, Squares};
pub use board::{Board, ParseFenError};
pub use castling::{CastlingRights, CastlingSide};
pub use moves::{Move, MoveList};
pub use notation::{CastlingNotation, ParseMoveError, UciMove};
pub use piece::{Colour, Piece, PieceType};
pub use square::{ParseSquareError, Square};
