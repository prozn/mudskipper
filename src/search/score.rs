//! Search scores, including distance-to-mate scores.

use super::MAX_PLY;

/// A bound beyond any score the search returns.
pub(super) const INFINITY: i32 = 32_001;

/// The score for delivering checkmate on the current move; a mate `n` plies away scores
/// `MATE - n`.
const MATE: i32 = 32_000;

/// The lowest score that still means the side to move mates.
const MATE_BOUND: i32 = MATE - MAX_PLY as i32;

/// The score of a draw.
pub(super) const DRAW: i32 = 0;

/// The score for being checkmated `ply` plies from the root.
pub(super) const fn mated_in(ply: usize) -> i32 {
    -MATE + ply as i32
}

/// Converts `score`, found `ply` plies from the root, to the form the transposition table
/// stores: a mate score counts plies from the position itself rather than from the root,
/// so that it stays right when the position is reached at another ply.
pub(super) const fn to_table(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score + ply as i32
    } else if score <= -MATE_BOUND {
        score - ply as i32
    } else {
        score
    }
}

/// Converts a score read from the transposition table back to one relative to the root,
/// for the position found `ply` plies from it. The inverse of [`to_table`].
pub(super) const fn from_table(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score - ply as i32
    } else if score <= -MATE_BOUND {
        score + ply as i32
    } else {
        score
    }
}

/// A search score as UCI reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    /// An evaluation in centipawns from the side to move's point of view.
    Centipawns(i32),
    /// A forced mate in this many moves: positive when the side to move mates, negative
    /// when it is mated.
    Mate(i32),
}

impl Score {
    /// Converts an internal score, from the side to move's point of view, to the form
    /// UCI reports.
    pub(super) const fn from_internal(score: i32) -> Self {
        if score >= MATE_BOUND {
            Self::Mate((MATE - score + 1) / 2)
        } else if score <= -MATE_BOUND {
            Self::Mate(-(MATE + score) / 2)
        } else {
            Self::Centipawns(score)
        }
    }
}
