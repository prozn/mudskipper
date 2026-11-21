//! Static evaluation of a position by the **Network**.
//!
//! The network is embedded at build time from the file `EVALFILE` names (ADR-0004). The
//! search keeps an [`AccumulatorStack`] updated as it makes moves; [`evaluate`] builds
//! the **Accumulator** from scratch.

mod accumulator;
mod network;

pub use accumulator::{Accumulator, AccumulatorStack};

use crate::chess::Board;

/// Evaluates `board` in centipawns from the side to move's point of view, building its
/// accumulator from scratch. The score is not clamped, so it may exceed a mate score.
///
/// The network's arithmetic is all integer, so the score is the same on every platform.
#[must_use]
pub fn evaluate(board: &Board) -> i32 {
    Accumulator::new(board).evaluate(board.side_to_move())
}

/// Evaluates `board` in centipawns from White's point of view, whoever is to move.
#[must_use]
pub fn evaluate_white(board: &Board) -> i32 {
    board.side_to_move().sign() * evaluate(board)
}
