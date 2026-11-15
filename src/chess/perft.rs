//! Perft: counting the leaves of the legal move tree to verify move generation.

use super::board::Board;
use super::moves::Move;

impl Board {
    /// Counts the leaf nodes of the legal move tree `depth` plies deep.
    ///
    /// Depth 0 counts the position itself, so it returns 1.
    #[must_use]
    pub fn perft(&self, depth: u32) -> u64 {
        match depth {
            0 => 1,
            1 => self.legal_moves().len() as u64,
            _ => self
                .legal_moves()
                .iter()
                .map(|&mv| self.make(mv).perft(depth - 1))
                .sum(),
        }
    }

    /// Counts the leaf nodes `depth` plies deep below each legal move, in generation order.
    ///
    /// The counts add up to [`Board::perft`] at the same depth. At depth 0 there are no
    /// moves to split by, so the list is empty.
    #[must_use]
    pub fn split_perft(&self, depth: u32) -> Vec<(Move, u64)> {
        if depth == 0 {
            return Vec::new();
        }
        self.legal_moves()
            .iter()
            .map(|&mv| (mv, self.make(mv).perft(depth - 1)))
            .collect()
    }
}
