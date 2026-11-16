//! Statistics a worker gathers while searching to order quiet moves: killer moves, kept
//! for one search, and butterfly history, kept from one search to the next.

use super::MAX_PLY;
use crate::chess::{Colour, Move};

/// The largest magnitude a history value reaches. The gravity formula in
/// [`History::update`] keeps every value within it. Raising it relative to the bonuses
/// makes history slower to forget old cutoffs; lowering it lets recent ones take over
/// sooner. It must stay within `i16`, which stores the values.
pub(super) const MAX_HISTORY: i32 = 16_384;

/// Bonus per ply of depth for a quiet move that causes a beta cutoff, before the cap.
/// Raising it makes deep cutoffs count for more than shallow ones and history adapt
/// faster, until [`MAX_BONUS`] caps it.
const BONUS_PER_PLY: i32 = 300;

/// What [`BONUS_PER_PLY`] times the depth is reduced by, so that shallow cutoffs count
/// for less.
const BONUS_OFFSET: i32 = 250;

/// The largest bonus a single cutoff earns, so that one deep cutoff does not swamp the
/// table.
const MAX_BONUS: i32 = 1_500;

/// The bonus earned by a quiet move causing a beta cutoff at `depth`, and the malus taken
/// by each quiet move tried before it. Common values among open-source engines at a
/// similar stage.
pub(super) fn bonus(depth: u32) -> i32 {
    let depth = i32::try_from(depth).unwrap_or(i32::MAX);
    BONUS_PER_PLY
        .saturating_mul(depth)
        .saturating_sub(BONUS_OFFSET)
        .min(MAX_BONUS)
}

/// Butterfly history: how often each quiet move, by colour, origin and destination, has
/// caused a beta cutoff, weighted by depth.
#[derive(Clone, Debug)]
pub(super) struct History {
    table: Box<[[[i16; 64]; 64]; 2]>,
}

impl History {
    /// Returns a table with every value zero.
    pub(super) fn new() -> Self {
        Self {
            table: Box::new([[[0; 64]; 64]; 2]),
        }
    }

    /// The history value of `mv` played by `colour`.
    pub(super) fn get(&self, colour: Colour, mv: Move) -> i32 {
        i32::from(self.table[colour.index()][mv.from().index()][mv.to().index()])
    }

    /// Adds `delta`, a bonus if positive and a malus if negative, to the value of `mv`
    /// played by `colour`. The gravity formula scales the change down as the value nears
    /// [`MAX_HISTORY`], so it never leaves the range `-MAX_HISTORY..=MAX_HISTORY`.
    pub(super) fn update(&mut self, colour: Colour, mv: Move, delta: i32) {
        let delta = delta.clamp(-MAX_HISTORY, MAX_HISTORY);
        let entry = &mut self.table[colour.index()][mv.from().index()][mv.to().index()];
        let value = i32::from(*entry);
        let updated = value + delta - value * delta.abs() / MAX_HISTORY;
        *entry = i16::try_from(updated).expect("gravity keeps history within MAX_HISTORY");
    }
}

/// Two quiet moves per ply that recently caused a beta cutoff there, most recent first.
#[derive(Clone, Debug)]
pub(super) struct Killers {
    slots: [[Option<Move>; 2]; MAX_PLY],
}

impl Killers {
    /// Returns a table with every slot empty.
    pub(super) fn new() -> Self {
        Self {
            slots: [[None; 2]; MAX_PLY],
        }
    }

    /// The killer moves at `ply`, most recent first.
    pub(super) fn get(&self, ply: usize) -> [Option<Move>; 2] {
        self.slots[ply]
    }

    /// Records `mv` as the most recent killer at `ply`, keeping the previous most recent
    /// one as the second unless it is `mv` itself.
    pub(super) fn update(&mut self, ply: usize, mv: Move) {
        let slots = &mut self.slots[ply];
        if slots[0] != Some(mv) {
            slots[1] = slots[0];
            slots[0] = Some(mv);
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::chess::Board;

    /// A quiet move of the start position.
    fn quiet_move() -> Move {
        let board = Board::startpos();
        *board
            .quiet_moves()
            .first()
            .expect("the start position has quiet moves")
    }

    proptest! {
        #[test]
        fn history_values_stay_within_their_bound_after_many_updates(
            depths in prop::collection::vec((1u32..=200, any::<bool>()), 1..2_000),
        ) {
            let mut history = History::new();
            let mv = quiet_move();
            for (depth, is_bonus) in depths {
                let delta = if is_bonus { bonus(depth) } else { -bonus(depth) };
                history.update(Colour::White, mv, delta);
                let value = history.get(Colour::White, mv);
                prop_assert!(
                    (-MAX_HISTORY..=MAX_HISTORY).contains(&value),
                    "value {} out of bounds after delta {}", value, delta
                );
            }
        }
    }

    #[test]
    fn repeated_bonuses_approach_but_never_pass_the_bound() {
        let mut history = History::new();
        let mv = quiet_move();
        for _ in 0..10_000 {
            history.update(Colour::Black, mv, MAX_HISTORY);
        }
        assert_eq!(history.get(Colour::Black, mv), MAX_HISTORY);
        assert_eq!(history.get(Colour::White, mv), 0, "colours are kept apart");
    }

    #[test]
    fn a_new_killer_displaces_the_older_one_and_a_repeat_changes_nothing() {
        let board = Board::startpos();
        let quiets = board.quiet_moves();
        let (a, b, c) = (quiets[0], quiets[1], quiets[2]);
        let mut killers = Killers::new();
        killers.update(3, a);
        killers.update(3, b);
        killers.update(3, b);
        assert_eq!(killers.get(3), [Some(b), Some(a)]);
        killers.update(3, c);
        assert_eq!(killers.get(3), [Some(c), Some(b)]);
        assert_eq!(killers.get(2), [None, None]);
    }
}
