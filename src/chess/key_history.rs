//! The keys of earlier positions in a game, for detecting repetitions.

use super::board::Board;
use super::zobrist::Key;

/// The Zobrist keys of the positions played before the current one, oldest first.
///
/// Positions are compared by key, so two positions repeat when they have the same piece
/// placement, side to move and castling rights, and the same en passant capture is
/// available, whatever their move clocks say.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyHistory {
    keys: Vec<Key>,
}

impl KeyHistory {
    /// Returns an empty history.
    #[must_use]
    pub const fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Records `key` as the position played most recently.
    pub fn push(&mut self, key: Key) {
        self.keys.push(key);
    }

    /// Removes and returns the key of the position played most recently.
    pub fn pop(&mut self) -> Option<Key> {
        self.keys.pop()
    }

    /// Counts the earlier occurrences of `board`, the position reached after the last key
    /// pushed.
    ///
    /// Only positions since the last capture or pawn move, as `board`'s halfmove clock
    /// counts them, are compared. A count of 2 is a threefold repetition.
    #[must_use]
    pub fn repetitions(&self, board: &Board) -> usize {
        let key = board.key();
        self.keys
            .iter()
            .rev()
            .take(usize::from(board.halfmove_clock()))
            .skip(1)
            .step_by(2)
            .filter(|&&earlier| earlier == key)
            .count()
    }
}
