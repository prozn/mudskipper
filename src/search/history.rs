//! Statistics a worker gathers while searching to order moves: killer moves, kept for one
//! search, and history, kept from one search to the next.

use super::MAX_PLY;
use super::params::{
    NOISY_BONUS_OFFSET, NOISY_BONUS_PER_PLY, NOISY_MALUS_OFFSET, NOISY_MALUS_PER_PLY,
    NOISY_MAX_BONUS, NOISY_MAX_MALUS, QUIET_BONUS_OFFSET, QUIET_BONUS_PER_PLY, QUIET_MALUS_OFFSET,
    QUIET_MALUS_PER_PLY, QUIET_MAX_BONUS, QUIET_MAX_MALUS,
};
use crate::chess::{Board, Colour, Move, PieceType};

/// The largest magnitude a history value reaches. The gravity formula in
/// [`apply_gravity`] keeps every value within it. Raising it relative to the bonuses
/// makes history slower to forget old cutoffs; lowering it lets recent ones take over
/// sooner. It must stay within `i16`, which stores the values.
pub(super) const MAX_HISTORY: i32 = 16_384;

/// The size of a history update for a beta cutoff at `depth`: `per_ply` per ply less
/// `offset`, capped at `max` and floored at zero. Each of the four updates, the bonus and
/// the malus in quiet and in noisy history, has its own three **Search parameters**. The
/// floor matters only if SPSA sets an offset beyond the shallowest updates: they then
/// change nothing, rather than turning a bonus into a malus or a malus into a bonus.
fn update_size(depth: u32, per_ply: i32, offset: i32, max: i32) -> i32 {
    let depth = i32::try_from(depth).unwrap_or(i32::MAX);
    per_ply
        .saturating_mul(depth)
        .saturating_sub(offset)
        .clamp(0, max)
}

/// The bonus a quiet move earns in quiet history by causing a beta cutoff at `depth`.
fn quiet_bonus(depth: u32) -> i32 {
    update_size(
        depth,
        QUIET_BONUS_PER_PLY.get(),
        QUIET_BONUS_OFFSET.get(),
        QUIET_MAX_BONUS.get(),
    )
}

/// The malus each quiet move tried before a quiet cut-off move at `depth` takes in quiet
/// history.
fn quiet_malus(depth: u32) -> i32 {
    update_size(
        depth,
        QUIET_MALUS_PER_PLY.get(),
        QUIET_MALUS_OFFSET.get(),
        QUIET_MAX_MALUS.get(),
    )
}

/// The bonus a noisy move earns in noisy history by causing a beta cutoff at `depth`.
fn noisy_bonus(depth: u32) -> i32 {
    update_size(
        depth,
        NOISY_BONUS_PER_PLY.get(),
        NOISY_BONUS_OFFSET.get(),
        NOISY_MAX_BONUS.get(),
    )
}

/// The malus each noisy move tried before a cut-off move at `depth` takes in noisy
/// history.
fn noisy_malus(depth: u32) -> i32 {
    update_size(
        depth,
        NOISY_MALUS_PER_PLY.get(),
        NOISY_MALUS_OFFSET.get(),
        NOISY_MAX_MALUS.get(),
    )
}

/// Adds `delta`, a bonus if positive and a malus if negative, to the history value in
/// `entry`. The gravity formula scales the change down as the value nears
/// [`MAX_HISTORY`], so it never leaves the range `-MAX_HISTORY..=MAX_HISTORY`.
fn apply_gravity(entry: &mut i16, delta: i32) {
    let delta = delta.clamp(-MAX_HISTORY, MAX_HISTORY);
    let value = i32::from(*entry);
    let updated = value + delta - value * delta.abs() / MAX_HISTORY;
    *entry = i16::try_from(updated).expect("gravity keeps history within MAX_HISTORY");
}

/// Every history table a worker keeps, and the rules for updating them when a move causes
/// a beta cutoff.
#[derive(Clone, Debug)]
pub(super) struct History {
    quiet: QuietHistory,
    noisy: NoisyHistory,
}

impl History {
    /// Returns tables with every value zero.
    pub(super) fn new() -> Self {
        Self {
            quiet: QuietHistory::new(),
            noisy: NoisyHistory::new(),
        }
    }

    /// The quiet history.
    pub(super) const fn quiet(&self) -> &QuietHistory {
        &self.quiet
    }

    /// The noisy history.
    pub(super) const fn noisy(&self) -> &NoisyHistory {
        &self.noisy
    }

    /// Records that `mv` caused a beta cutoff in `board`, `depth` plies deep, after the
    /// quiet moves `quiets` and the noisy moves `noisies` were tried before it and failed
    /// to. The cut-off move earns its table's bonus, [`quiet_bonus`] or [`noisy_bonus`]. A
    /// quiet cut-off move also gives each move in `quiets` the [`quiet_malus`]. Either way,
    /// each move in `noisies` takes the [`noisy_malus`].
    pub(super) fn update_on_cutoff(
        &mut self,
        board: &Board,
        mv: Move,
        quiets: &[Move],
        noisies: &[Move],
        depth: u32,
    ) {
        if mv.is_noisy() {
            self.noisy.update(board, mv, noisy_bonus(depth));
        } else {
            let colour = board.side_to_move();
            self.quiet.update(colour, mv, quiet_bonus(depth));
            let malus = quiet_malus(depth);
            for &quiet in quiets {
                self.quiet.update(colour, quiet, -malus);
            }
        }
        let malus = noisy_malus(depth);
        for &noisy in noisies {
            self.noisy.update(board, noisy, -malus);
        }
    }
}

/// Quiet history: how often each quiet move, by colour, origin and destination, has
/// caused a beta cutoff, weighted by depth.
#[derive(Clone, Debug)]
pub(super) struct QuietHistory {
    table: Box<[[[i16; 64]; 64]; 2]>,
}

impl QuietHistory {
    /// Returns a table with every value zero.
    pub(super) fn new() -> Self {
        Self {
            table: Box::new([[[0; 64]; 64]; 2]),
        }
    }

    /// The quiet history value of `mv` played by `colour`.
    pub(super) fn get(&self, colour: Colour, mv: Move) -> i32 {
        i32::from(self.table[colour.index()][mv.from().index()][mv.to().index()])
    }

    /// Adds `delta`, a bonus if positive and a malus if negative, to the value of `mv`
    /// played by `colour`, as [`apply_gravity`] describes.
    pub(super) fn update(&mut self, colour: Colour, mv: Move, delta: i32) {
        apply_gravity(
            &mut self.table[colour.index()][mv.from().index()][mv.to().index()],
            delta,
        );
    }
}

/// Noisy history: how often each noisy move, by colour, moved piece type, destination
/// and victim type, has caused a beta cutoff, weighted by depth.
#[derive(Clone, Debug)]
pub(super) struct NoisyHistory {
    table: Box<[[[[i16; 6]; 64]; 6]; 2]>,
}

impl NoisyHistory {
    /// Returns a table with every value zero.
    pub(super) fn new() -> Self {
        Self {
            table: Box::new([[[[0; 6]; 64]; 6]; 2]),
        }
    }

    /// The noisy history value of the noisy move `mv` in `board`.
    pub(super) fn get(&self, board: &Board, mv: Move) -> i32 {
        let [colour, piece, to, victim] = Self::index(board, mv);
        i32::from(self.table[colour][piece][to][victim])
    }

    /// Adds `delta`, a bonus if positive and a malus if negative, to the value of the noisy
    /// move `mv` in `board`, as [`apply_gravity`] describes.
    pub(super) fn update(&mut self, board: &Board, mv: Move, delta: i32) {
        let [colour, piece, to, victim] = Self::index(board, mv);
        apply_gravity(&mut self.table[colour][piece][to][victim], delta);
    }

    /// The table indices of `mv` in `board`, in table order: colour, moved piece type,
    /// destination and victim slot.
    fn index(board: &Board, mv: Move) -> [usize; 4] {
        let piece = board
            .piece_at(mv.from())
            .expect("a move's origin holds a piece");
        [
            piece.colour().index(),
            piece.piece_type().index(),
            mv.to().index(),
            victim_slot(board, mv).index(),
        ]
    }
}

/// The victim slot of noisy history that `mv` uses in `board`: what it captures, or the
/// pawn slot for a promotion that captures nothing, which no real capture uses.
fn victim_slot(board: &Board, mv: Move) -> PieceType {
    board.captured(mv).unwrap_or(PieceType::Pawn)
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

    /// A position where White can capture exd5 or make a few quiet moves.
    fn capture_position() -> Board {
        "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1"
            .parse()
            .expect("valid FEN")
    }

    proptest! {
        #[test]
        fn history_values_stay_within_their_bound_after_many_updates(
            depths in prop::collection::vec((1u32..=200, any::<bool>()), 1..2_000),
        ) {
            let mut history = QuietHistory::new();
            let mv = quiet_move();
            for (depth, is_bonus) in depths {
                let delta = if is_bonus { quiet_bonus(depth) } else { -quiet_malus(depth) };
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
    fn an_offset_beyond_the_shallow_update_gives_no_update_rather_than_the_opposite_one() {
        // 100 per ply less 250 would be -150 at depth 1 and -50 at depth 2.
        assert_eq!(update_size(1, 100, 250, 1_500), 0);
        assert_eq!(update_size(2, 100, 250, 1_500), 0);
        assert_eq!(update_size(3, 100, 250, 1_500), 50);
        assert_eq!(update_size(20, 100, 250, 1_500), 1_500, "capped at the max");
    }

    #[test]
    fn repeated_bonuses_approach_but_never_pass_the_bound() {
        let mut history = QuietHistory::new();
        let mv = quiet_move();
        for _ in 0..10_000 {
            history.update(Colour::Black, mv, MAX_HISTORY);
        }
        assert_eq!(history.get(Colour::Black, mv), MAX_HISTORY);
        assert_eq!(history.get(Colour::White, mv), 0, "colours are kept apart");
    }

    #[test]
    fn a_quiet_cutoff_gives_the_cut_off_move_the_bonus() {
        let board = Board::startpos();
        let quiets = board.quiet_moves();
        let mut history = History::new();
        history.update_on_cutoff(&board, quiets[2], &[], &[], 4);
        assert_eq!(history.quiet().get(Colour::White, quiets[2]), 961);
        assert_eq!(history.quiet().get(Colour::Black, quiets[2]), 0);
    }

    #[test]
    fn quiet_moves_tried_before_a_quiet_cutoff_get_the_malus() {
        let board = capture_position();
        let quiets = board.quiet_moves();
        let noisy = *board.noisy_moves().first().expect("exd5 is a capture");
        let mut history = History::new();
        history.update_on_cutoff(&board, quiets[2], &quiets[..2], &[noisy], 4);
        assert_eq!(history.quiet().get(Colour::White, quiets[0]), -961);
        assert_eq!(history.quiet().get(Colour::White, quiets[1]), -961);
        assert_eq!(
            history.quiet().get(Colour::White, quiets[3]),
            0,
            "not tried"
        );
    }

    #[test]
    fn a_noisy_cutoff_leaves_quiet_history_unchanged() {
        let board = capture_position();
        let capture = *board.noisy_moves().first().expect("exd5 is a capture");
        let quiets = board.quiet_moves();
        let mut history = History::new();
        history.update_on_cutoff(&board, capture, &quiets[..2], &[], 4);
        for &quiet in &quiets[..2] {
            assert_eq!(history.quiet().get(Colour::White, quiet), 0, "{quiet}");
        }
        assert_eq!(history.quiet().get(Colour::White, capture), 0);
    }

    #[test]
    fn noisy_moves_tried_before_a_quiet_cutoff_get_the_malus() {
        let board = capture_position();
        let quiets = board.quiet_moves();
        let noisy = *board.noisy_moves().first().expect("exd5 is a capture");
        let mut history = History::new();
        history.update_on_cutoff(&board, quiets[2], &quiets[..2], &[noisy], 4);
        assert_eq!(history.noisy().get(&board, noisy), -961);
    }

    #[test]
    fn a_noisy_cutoff_gives_the_bonus_to_it_and_the_malus_to_earlier_noisy_moves() {
        // White can take the rook with the queen or the pawn, or the knight with the
        // pawn.
        let board: Board = "4k3/8/8/2r1n3/3P4/8/8/2Q1K3 w - - 0 1"
            .parse()
            .expect("valid FEN");
        let noisies = board.noisy_moves();
        assert_eq!(noisies.len(), 3, "{noisies:?}");
        let mut history = History::new();
        history.update_on_cutoff(&board, noisies[2], &[], &noisies[..1], 4);
        assert_eq!(history.noisy().get(&board, noisies[2]), 961);
        assert_eq!(history.noisy().get(&board, noisies[0]), -961);
        assert_eq!(history.noisy().get(&board, noisies[1]), 0, "not tried");
    }

    #[test]
    fn en_passant_uses_the_pawn_victim_slot() {
        let board: Board = "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1"
            .parse()
            .expect("valid FEN");
        let en_passant = *board
            .noisy_moves()
            .iter()
            .find(|mv| mv.is_en_passant())
            .expect("exd6 en passant is legal");
        assert_eq!(victim_slot(&board, en_passant), PieceType::Pawn);
    }

    #[test]
    fn a_promotion_that_captures_nothing_uses_the_pawn_victim_slot() {
        let board: Board = "1n2k3/P7/8/8/8/8/8/4K3 w - - 0 1"
            .parse()
            .expect("valid FEN");
        let noisies = board.noisy_moves();
        let push = noisies
            .iter()
            .find(|mv| !mv.is_capture())
            .expect("a8 promotions capture nothing");
        let capture = noisies
            .iter()
            .find(|mv| mv.is_capture())
            .expect("axb8 promotions capture the knight");
        assert_eq!(victim_slot(&board, *push), PieceType::Pawn);
        assert_eq!(victim_slot(&board, *capture), PieceType::Knight);
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
