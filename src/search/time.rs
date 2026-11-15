//! Time management: how long a search may take.

use std::time::Duration;

/// Moves assumed left in the game when the time control does not say, so that the
/// remaining time is spread over them.
const DEFAULT_MOVES_TO_GO: u32 = 30;

/// The side to move's clock in a game played with a time control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// The time left.
    pub time: Duration,
    /// The time added after each move.
    pub increment: Duration,
    /// The moves left until the next time control, if the time control has one.
    pub moves_to_go: Option<u32>,
}

/// How long a search may run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeLimit {
    /// Once this much time has passed, the search starts no new iteration.
    pub(super) soft: Duration,
    /// Once this much time has passed, the search stops, even in the middle of an
    /// iteration.
    pub(super) hard: Duration,
}

impl TimeLimit {
    /// Returns the limit for a search given `movetime` to move, keeping `overhead` back
    /// for communication delays.
    #[must_use]
    pub fn movetime(movetime: Duration, overhead: Duration) -> Self {
        let time = movetime.saturating_sub(overhead);
        Self {
            soft: time,
            hard: time,
        }
    }

    /// Returns the limit for a search with `clock` showing the time left, keeping
    /// `overhead` back for communication delays.
    ///
    /// The search aims to spend an equal share of the time left on each move until the
    /// next time control, plus most of the increment. It may run up to three times that
    /// to finish an iteration, but never past three quarters of the time left.
    #[must_use]
    pub fn clock(clock: Clock, overhead: Duration) -> Self {
        let available = clock.time.saturating_sub(overhead);
        let ceiling = available * 3 / 4;
        let moves = clock.moves_to_go.unwrap_or(DEFAULT_MOVES_TO_GO).max(1);
        let soft = (available / moves + clock.increment * 3 / 4).min(ceiling);
        Self {
            soft,
            hard: (soft * 3).min(ceiling),
        }
    }
}
