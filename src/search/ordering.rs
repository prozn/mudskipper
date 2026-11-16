//! Move ordering: the transposition table's move, then captures by most valuable victim,
//! least valuable attacker, then quiet moves in generation order.

use crate::chess::{Board, Move, MoveList, PieceType};

/// Rough piece values for ordering captures, indexed by [`PieceType::index`].
const ORDER_VALUES: [i32; 6] = [100, 300, 300, 500, 900, 0];

/// How much more the victim's value counts than the attacker's, so that any capture of a
/// more valuable piece comes before every capture of a less valuable one.
const VICTIM_WEIGHT: i32 = 10;

/// The ordering score of the transposition table's move, above every other move's.
const TT_MOVE_SCORE: i32 = i32::MAX;

/// Moves handed out best first, choosing the next one only when it is asked for, so a
/// cutoff skips sorting the rest.
pub(super) struct OrderedMoves {
    moves: [Move; MoveList::CAPACITY],
    scores: [i32; MoveList::CAPACITY],
    len: usize,
    next: usize,
}

impl OrderedMoves {
    /// Scores `list` for ordering in `board`, putting `tt_move` first if it is in `list`.
    pub(super) fn new(board: &Board, list: &MoveList, tt_move: Option<Move>) -> Self {
        let mut moves = [Move::NULL; MoveList::CAPACITY];
        let mut scores = [0; MoveList::CAPACITY];
        for (i, &mv) in list.iter().enumerate() {
            moves[i] = mv;
            scores[i] = if Some(mv) == tt_move {
                TT_MOVE_SCORE
            } else {
                score(board, mv)
            };
        }
        Self {
            moves,
            scores,
            len: list.len(),
            next: 0,
        }
    }
}

impl Iterator for OrderedMoves {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        if self.next == self.len {
            return None;
        }
        // The first of equally scored moves wins, keeping the order deterministic.
        let mut best = self.next;
        for i in self.next + 1..self.len {
            if self.scores[i] > self.scores[best] {
                best = i;
            }
        }
        self.moves.swap(self.next, best);
        self.scores.swap(self.next, best);
        self.next += 1;
        Some(self.moves[self.next - 1])
    }
}

/// The ordering score of `mv` in `board`: higher is searched first.
fn score(board: &Board, mv: Move) -> i32 {
    let mut score = 0;
    if mv.is_capture() {
        let victim = if mv.is_en_passant() {
            PieceType::Pawn
        } else {
            board
                .piece_at(mv.to())
                .expect("a capture's target holds a piece")
                .piece_type()
        };
        let attacker = board
            .piece_at(mv.from())
            .expect("a move's origin holds a piece")
            .piece_type();
        score += VICTIM_WEIGHT * ORDER_VALUES[victim.index()] - ORDER_VALUES[attacker.index()];
    }
    if let Some(promotion) = mv.promotion() {
        score += ORDER_VALUES[promotion.index()];
    }
    score
}
