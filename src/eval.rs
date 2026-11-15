//! Static evaluation of a position.

mod pesto;

use crate::chess::{Board, Colour, PieceType};

/// Game phase of the full starting material; the phase falls towards 0 as pieces leave.
const MAX_PHASE: i32 = 24;

/// How much each piece type contributes to the game phase, indexed by [`PieceType::index`].
const PHASE_WEIGHTS: [i32; 6] = [0, 1, 1, 2, 4, 0];

/// Evaluates `board` in centipawns from the side to move's point of view.
///
/// The score is a tapered blend of middlegame and endgame piece-square tables, weighted
/// by how much material is left. It is computed in integers, so it is the same on every
/// platform.
#[must_use]
pub fn evaluate(board: &Board) -> i32 {
    let white = evaluate_white(board);
    match board.side_to_move() {
        Colour::White => white,
        Colour::Black => -white,
    }
}

/// Evaluates `board` in centipawns from White's point of view, whoever is to move.
#[must_use]
pub fn evaluate_white(board: &Board) -> i32 {
    let mut middlegame = 0;
    let mut endgame = 0;
    let mut phase = 0;
    for colour in Colour::ALL {
        let sign = match colour {
            Colour::White => 1,
            Colour::Black => -1,
        };
        for piece_type in PieceType::ALL {
            for square in board.pieces(colour, piece_type) {
                let (mg, eg) = pesto::value(colour, piece_type, square);
                middlegame += sign * mg;
                endgame += sign * eg;
                phase += PHASE_WEIGHTS[piece_type.index()];
            }
        }
    }
    // Early promotions can push the phase past the starting material.
    let phase = phase.min(MAX_PHASE);
    (middlegame * phase + endgame * (MAX_PHASE - phase)) / MAX_PHASE
}
