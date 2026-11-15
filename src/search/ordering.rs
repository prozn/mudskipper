//! Move ordering: captures by most valuable victim, least valuable attacker, then
//! quiet moves in generation order.

use crate::chess::{Board, Move, MoveList, PieceType};

/// Rough piece values for ordering captures, indexed by [`PieceType::index`].
const ORDER_VALUES: [i32; 6] = [100, 300, 300, 500, 900, 0];

/// Moves handed out best first, choosing the next one only when it is asked for, so a
/// cutoff skips sorting the rest.
pub(super) struct OrderedMoves {
    moves: [Move; MoveList::CAPACITY],
    scores: [i32; MoveList::CAPACITY],
    len: usize,
    next: usize,
}

impl OrderedMoves {
    /// Scores `list` for ordering in `board`.
    pub(super) fn new(board: &Board, list: &MoveList) -> Self {
        let mut moves = [Move::NULL; MoveList::CAPACITY];
        let mut scores = [0; MoveList::CAPACITY];
        for (i, &mv) in list.iter().enumerate() {
            moves[i] = mv;
            scores[i] = score(board, mv);
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
        score += 10 * ORDER_VALUES[victim.index()] - ORDER_VALUES[attacker.index()];
    }
    if let Some(promotion) = mv.promotion() {
        score += ORDER_VALUES[promotion.index()];
    }
    score
}
