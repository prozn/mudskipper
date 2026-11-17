//! The search window: the bounds alpha and beta between which a search wants an exact score.

use super::score::INFINITY;

/// The scores a search of one node cares about: below `alpha` the side to move already has
/// something better, and at or above `beta` the opponent has something better. Scores in
/// between are exact; outside it a search returns only a bound.
///
/// Alpha is always below beta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Window {
    alpha: i32,
    beta: i32,
}

impl Window {
    /// The window that admits every score, for a search with nothing to aim at.
    pub(super) const FULL: Self = Self::new(-INFINITY, INFINITY);

    /// Returns the window from `alpha` to `beta`, which must be below it.
    pub(super) const fn new(alpha: i32, beta: i32) -> Self {
        debug_assert!(alpha < beta, "alpha must be below beta");
        Self { alpha, beta }
    }

    /// Returns the aspiration window `delta` either side of `previous`, within the full
    /// window.
    pub(super) fn aspiration(previous: i32, delta: i32) -> Self {
        Self::new(
            (previous - delta).max(-INFINITY),
            (previous + delta).min(INFINITY),
        )
    }

    /// The lower bound: the score the side to move can already reach.
    pub(super) const fn alpha(self) -> i32 {
        self.alpha
    }

    /// The upper bound: the score at which the opponent avoids this node.
    pub(super) const fn beta(self) -> i32 {
        self.beta
    }

    /// Reports whether `score` lies strictly inside the window, and so is exact.
    pub(super) const fn contains(self, score: i32) -> bool {
        self.alpha < score && score < self.beta
    }

    /// The same window from the opponent's point of view, for searching a child.
    pub(super) const fn negated(self) -> Self {
        Self::new(-self.beta, -self.alpha)
    }

    /// The null window just above alpha, which only proves whether a score beats alpha.
    pub(super) const fn null_above_alpha(self) -> Self {
        Self::new(self.alpha, self.alpha + 1)
    }

    /// The null window just below beta, which only proves whether a score reaches beta.
    pub(super) const fn null_below_beta(self) -> Self {
        Self::new(self.beta - 1, self.beta)
    }

    /// Raises alpha to `score` if that is higher. A score at or above beta is a cutoff,
    /// which the caller returns instead of raising alpha.
    pub(super) fn raise_alpha(&mut self, score: i32) {
        debug_assert!(score < self.beta, "score {score} is a beta cutoff");
        self.alpha = self.alpha.max(score);
    }

    /// The aspiration window to search again with after failing low with `score`: beta
    /// comes down halfway to alpha, and alpha drops to `delta` below `score`, within the
    /// full window.
    pub(super) fn widened_down(self, score: i32, delta: i32) -> Self {
        Self::new(
            (score - delta).max(-INFINITY),
            self.alpha.midpoint(self.beta),
        )
    }

    /// The aspiration window to search again with after failing high with `score`: beta
    /// rises to `delta` above `score`, within the full window.
    pub(super) fn widened_up(self, score: i32, delta: i32) -> Self {
        Self::new(self.alpha, (score + delta).min(INFINITY))
    }
}
