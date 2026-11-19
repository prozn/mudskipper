//! Statistics a worker gathers while searching to order moves: killer moves, kept for one
//! search, and history, kept from one search to the next.

use super::MAX_PLY;
use crate::chess::{Board, Colour, Move, PieceType};

/// The largest magnitude a history value reaches. The gravity formula in
/// [`apply_gravity`] keeps every value within it. Raising it relative to the bonuses
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

/// The bonus earned by a move causing a beta cutoff at `depth`, and the malus taken by
/// each move tried before it, in both quiet and noisy history. Common values among
/// open-source engines at a similar stage.
fn bonus(depth: u32) -> i32 {
    let depth = i32::try_from(depth).unwrap_or(i32::MAX);
    BONUS_PER_PLY
        .saturating_mul(depth)
        .saturating_sub(BONUS_OFFSET)
        .min(MAX_BONUS)
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
    /// to. The cut-off move earns a [`bonus`] in its own table. A quiet cut-off move also
    /// gives each move in `quiets` the matching malus in quiet history. Either way, each
    /// move in `noisies` takes the malus in noisy history.
    pub(super) fn update_on_cutoff(
        &mut self,
        board: &Board,
        mv: Move,
        quiets: &[Move],
        noisies: &[Move],
        depth: u32,
    ) {
        let bonus = bonus(depth);
        if mv.is_noisy() {
            self.noisy.update(board, mv, bonus);
        } else {
            let colour = board.side_to_move();
            self.quiet.update(colour, mv, bonus);
            for &quiet in quiets {
                self.quiet.update(colour, quiet, -bonus);
            }
        }
        for &noisy in noisies {
            self.noisy.update(board, noisy, -bonus);
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
        assert_eq!(history.quiet().get(Colour::White, quiets[2]), 950);
        assert_eq!(history.quiet().get(Colour::Black, quiets[2]), 0);
    }

    #[test]
    fn quiet_moves_tried_before_a_quiet_cutoff_get_the_malus() {
        let board = capture_position();
        let quiets = board.quiet_moves();
        let noisy = *board.noisy_moves().first().expect("exd5 is a capture");
        let mut history = History::new();
        history.update_on_cutoff(&board, quiets[2], &quiets[..2], &[noisy], 4);
        assert_eq!(history.quiet().get(Colour::White, quiets[0]), -950);
        assert_eq!(history.quiet().get(Colour::White, quiets[1]), -950);
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
        assert_eq!(history.noisy().get(&board, noisy), -950);
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
        assert_eq!(history.noisy().get(&board, noisies[2]), 950);
        assert_eq!(history.noisy().get(&board, noisies[0]), -950);
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
