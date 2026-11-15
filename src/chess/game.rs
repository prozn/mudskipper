//! A game in progress: the current position and the positions played before it.

use super::board::Board;
use super::key_history::KeyHistory;
use super::moves::Move;

/// The current position of a game together with the keys of the positions played before
/// it, which repetition detection needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    board: Board,
    history: KeyHistory,
}

impl Game {
    /// Returns a game starting from `board`, with no earlier positions.
    #[must_use]
    pub const fn new(board: Board) -> Self {
        Self {
            board,
            history: KeyHistory::new(),
        }
    }

    /// Returns a game in the standard start position.
    #[must_use]
    pub fn startpos() -> Self {
        Self::new(Board::startpos())
    }

    /// The current position.
    #[must_use]
    pub const fn board(&self) -> &Board {
        &self.board
    }

    /// The keys of the positions played before the current one.
    #[must_use]
    pub const fn history(&self) -> &KeyHistory {
        &self.history
    }

    /// Plays `mv`, recording the current position in the history.
    ///
    /// `mv` must be legal in the current position, as [`Board::make`] requires.
    pub fn play(&mut self, mv: Move) {
        self.history.push(self.board.key());
        self.board = self.board.make(mv);
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::startpos()
    }
}
