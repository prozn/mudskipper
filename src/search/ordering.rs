//! Move ordering by a staged move picker (ADR-0003), which generates each group of moves
//! only when the search reaches it: the transposition table's move, never generated; then
//! the good noisy moves, which static exchange evaluation finds do not lose material, by
//! most valuable victim, least valuable attacker; then the killer moves; then the other
//! quiet moves by history; then the bad noisy moves, which lose material, in the same order
//! as the good ones. A cutoff in an early stage skips generating the later ones.

use super::history::History;
use crate::chess::{Board, Move, MoveList, PieceType};

/// Rough piece values for ordering captures, indexed by [`PieceType::index`].
const ORDER_VALUES: [i32; 6] = [100, 300, 300, 500, 900, 0];

/// How much more the victim's value counts than the attacker's, so that any capture of a
/// more valuable piece comes before every capture of a less valuable one.
const VICTIM_WEIGHT: i32 = 10;

/// Which moves a picker hands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Coverage {
    /// Every pseudo-legal move.
    All,
    /// Only captures and promotions.
    Noisy,
    /// Only captures and promotions that do not lose material by static exchange
    /// evaluation.
    GoodNoisy,
}

/// Where a picker has got to. Each stage moves on to the next one declared, except that a
/// noisy-only picker goes straight from the good noisy moves to the bad ones, and a picker
/// of good noisy moves finishes after them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    TtMove,
    GenerateNoisy,
    GoodNoisy,
    FirstKiller,
    SecondKiller,
    GenerateQuiet,
    Quiet,
    BadNoisy,
    Done,
}

/// Pseudo-legal moves handed out best first, each once, generating and ordering each
/// stage's moves only when it is reached.
///
/// It is not an [`Iterator`] because ordering quiet moves reads the worker's [`History`],
/// which the search updates between moves: [`MovePicker::next_move`] borrows it afresh
/// for each move.
pub(super) struct MovePicker<'board> {
    board: &'board Board,
    tt_move: Option<Move>,
    /// The killer moves to hand out after the noisy moves, most recent first. Only quiet
    /// moves pseudo-legal in `board` that are not the TT move are kept.
    killers: [Option<Move>; 2],
    coverage: Coverage,
    stage: Stage,
    /// The moves of the current stage.
    moves: MoveList,
    /// The ordering score of each move of the current stage.
    scores: [i32; MoveList::CAPACITY],
    /// How many of the current stage's moves have been handed out or skipped.
    next: usize,
    /// The noisy moves found to lose material during the good noisy stage, best first.
    bad_noisy: MoveList,
    /// How many of `bad_noisy` have been handed out.
    next_bad: usize,
}

impl<'board> MovePicker<'board> {
    /// Returns a picker of every pseudo-legal move of `board`, with `tt_move` first and
    /// `killers` straight after the noisy moves.
    ///
    /// `tt_move` must be pseudo-legal in `board`, as checked by
    /// [`Board::is_pseudo_legal`]. A killer need not be: one that is not, or that is
    /// noisy or the TT move, is skipped.
    pub(super) fn new(
        board: &'board Board,
        tt_move: Option<Move>,
        killers: [Option<Move>; 2],
    ) -> Self {
        let mut picker = Self::with_coverage(board, tt_move, Coverage::All);
        let usable = |killer: Option<Move>| {
            killer.filter(|&mv| !mv.is_noisy() && Some(mv) != tt_move && board.is_pseudo_legal(mv))
        };
        let [first, second] = killers.map(usable);
        picker.killers = [first, second.filter(|&mv| Some(mv) != first)];
        picker
    }

    /// Returns a picker of the pseudo-legal captures and promotions of `board` that do not
    /// lose material by static exchange evaluation, with `tt_move` first if it is one of
    /// them.
    ///
    /// `tt_move` must be pseudo-legal in `board`, as checked by
    /// [`Board::is_pseudo_legal`].
    pub(super) fn good_noisy(board: &'board Board, tt_move: Option<Move>) -> Self {
        let tt_move = tt_move.filter(|&mv| mv.is_noisy() && board.see_ge(mv, 0));
        Self::with_coverage(board, tt_move, Coverage::GoodNoisy)
    }

    fn with_coverage(board: &'board Board, tt_move: Option<Move>, coverage: Coverage) -> Self {
        debug_assert!(
            tt_move.is_none_or(|mv| board.is_pseudo_legal(mv)),
            "TT move {tt_move:?} is not pseudo-legal in {}",
            board.to_fen()
        );
        Self {
            board,
            tt_move,
            killers: [None; 2],
            coverage,
            stage: Stage::TtMove,
            moves: MoveList::new(),
            scores: [0; MoveList::CAPACITY],
            next: 0,
            bad_noisy: MoveList::new(),
            next_bad: 0,
        }
    }

    /// Hands out the best-scored move of the current stage not yet handed out, or `None`
    /// once they are all gone. The first of equally scored moves wins, keeping the order
    /// deterministic.
    fn pick_best(&mut self) -> Option<Move> {
        if self.next == self.moves.len() {
            return None;
        }
        let mut best = self.next;
        for i in self.next + 1..self.moves.len() {
            if self.scores[i] > self.scores[best] {
                best = i;
            }
        }
        self.moves.swap(self.next, best);
        self.scores.swap(self.next, best);
        self.next += 1;
        Some(self.moves[self.next - 1])
    }

    /// Reports whether `mv` was handed out by an earlier stage than the quiet one.
    fn handed_out_early(&self, mv: Move) -> bool {
        Some(mv) == self.tt_move || self.killers.contains(&Some(mv))
    }

    /// Reports whether the picker has reached the quiet moves after the killers, so that
    /// the move it last handed out is quiet and neither the TT move nor a killer.
    pub(super) fn is_handing_out_late_quiets(&self) -> bool {
        self.stage == Stage::Quiet
    }

    /// Stops handing out quiet moves, killers included, so that only the noisy moves not
    /// yet handed out remain. Quiet moves not yet generated never are.
    pub(super) fn skip_quiets(&mut self) {
        if self.coverage == Coverage::All {
            self.coverage = Coverage::Noisy;
        }
        if matches!(
            self.stage,
            Stage::FirstKiller | Stage::SecondKiller | Stage::GenerateQuiet | Stage::Quiet
        ) {
            self.stage = Stage::BadNoisy;
        }
    }

    /// Hands out the next move, best first, or `None` once every move has been handed
    /// out. Quiet moves are ordered by `history` as it stands when they are generated.
    pub(super) fn next_move(&mut self, history: &History) -> Option<Move> {
        loop {
            match self.stage {
                Stage::TtMove => {
                    self.stage = Stage::GenerateNoisy;
                    if self.tt_move.is_some() {
                        return self.tt_move;
                    }
                }
                Stage::GenerateNoisy => {
                    self.moves = self.board.noisy_moves();
                    for (i, &mv) in self.moves.iter().enumerate() {
                        self.scores[i] = score(self.board, mv);
                    }
                    self.next = 0;
                    self.stage = Stage::GoodNoisy;
                }
                Stage::GoodNoisy => match self.pick_best() {
                    Some(mv) if Some(mv) == self.tt_move => {}
                    Some(mv) if !self.board.see_ge(mv, 0) => self.bad_noisy.push(mv),
                    Some(mv) => return Some(mv),
                    None => {
                        self.stage = match self.coverage {
                            Coverage::All => Stage::FirstKiller,
                            Coverage::Noisy => Stage::BadNoisy,
                            Coverage::GoodNoisy => Stage::Done,
                        };
                    }
                },
                Stage::FirstKiller => {
                    self.stage = Stage::SecondKiller;
                    if let Some(mv) = self.killers[0] {
                        return Some(mv);
                    }
                }
                Stage::SecondKiller => {
                    self.stage = Stage::GenerateQuiet;
                    if let Some(mv) = self.killers[1] {
                        return Some(mv);
                    }
                }
                Stage::GenerateQuiet => {
                    self.moves = self.board.quiet_moves();
                    let colour = self.board.side_to_move();
                    for (i, &mv) in self.moves.iter().enumerate() {
                        self.scores[i] = history.get(colour, mv);
                    }
                    self.next = 0;
                    self.stage = Stage::Quiet;
                }
                Stage::Quiet => match self.pick_best() {
                    Some(mv) if self.handed_out_early(mv) => {}
                    Some(mv) => return Some(mv),
                    None => self.stage = Stage::BadNoisy,
                },
                Stage::BadNoisy => {
                    if let Some(&mv) = self.bad_noisy.get(self.next_bad) {
                        self.next_bad += 1;
                        return Some(mv);
                    }
                    self.stage = Stage::Done;
                }
                Stage::Done => return None,
            }
        }
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::sample::Index;

    use super::*;
    use crate::chess::Colour;

    /// Where random games start: the start position, a busy middlegame, an endgame, a
    /// position with promotions both ways and a Chess960 position.
    const STARTS: [&str; 5] = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
    ];

    /// Longest random game, in plies.
    const MAX_PLIES: usize = 60;

    /// The moves of `moves`, sorted so that two lists compare as multisets.
    fn sorted(moves: impl IntoIterator<Item = Move>) -> Vec<u16> {
        let mut bits: Vec<u16> = moves.into_iter().map(Move::to_bits).collect();
        bits.sort_unstable();
        bits
    }

    /// Every move `picker` hands out, in order.
    fn drain(mut picker: MovePicker<'_>, history: &History) -> Vec<Move> {
        std::iter::from_fn(|| picker.next_move(history)).collect()
    }

    /// Checks that the pickers of `board` produce exactly the moves of the full
    /// generators, each once, with `tt_move` first when they may return it, and the
    /// usable `killers` straight after the noisy moves.
    fn check_pickers(
        board: &Board,
        tt_move: Option<Move>,
        killers: [Option<Move>; 2],
    ) -> Result<(), TestCaseError> {
        let fen = board.to_fen();
        let history = History::new();
        let all = drain(MovePicker::new(board, tt_move, killers), &history);
        prop_assert_eq!(
            sorted(all.iter().copied()),
            sorted(board.pseudo_legal_moves().iter().copied()),
            "all moves of {:?} with TT move {:?} and killers {:?}",
            fen,
            tt_move,
            killers
        );
        if let Some(tt_move) = tt_move {
            prop_assert_eq!(all.first(), Some(&tt_move), "in {:?}", fen);
        }
        let first_quiet = all
            .iter()
            .position(|mv| !mv.is_noisy() && Some(*mv) != tt_move)
            .unwrap_or(all.len());
        let mut usable: Vec<Move> = Vec::new();
        for killer in killers.into_iter().flatten() {
            if !killer.is_noisy()
                && Some(killer) != tt_move
                && board.is_pseudo_legal(killer)
                && !usable.contains(&killer)
            {
                usable.push(killer);
            }
        }
        prop_assert_eq!(
            &all[first_quiet..(first_quiet + usable.len()).min(all.len())],
            &usable[..],
            "killers {:?} in {:?} with TT move {:?}",
            killers,
            fen,
            tt_move
        );

        let good_noisy = drain(MovePicker::good_noisy(board, tt_move), &history);
        let mut expected = board.noisy_moves();
        expected.retain(|mv| board.see_ge(mv, 0));
        prop_assert_eq!(
            sorted(good_noisy.iter().copied()),
            sorted(expected.iter().copied()),
            "good noisy moves of {:?} with TT move {:?}",
            fen,
            tt_move
        );
        if let Some(tt_move) = tt_move.filter(|&mv| expected.contains(&mv)) {
            prop_assert_eq!(good_noisy.first(), Some(&tt_move), "in {:?}", fen);
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Killers are drawn from the moves of the position before, as they would be from a
        /// sibling node, so they are often noisy, not pseudo-legal or the TT move here.
        #[test]
        fn pickers_produce_each_generated_move_once_with_or_without_a_tt_move_and_killers(
            start in 0..STARTS.len(),
            choices in prop::collection::vec(
                (any::<Index>(), any::<Option<Index>>(), any::<[Option<Index>; 2]>()),
                1..MAX_PLIES,
            ),
        ) {
            let mut board = Board::from_fen(STARTS[start]).expect("start FEN is valid");
            let mut previous = board.pseudo_legal_moves();
            for (next, tt_choice, killer_choices) in choices {
                let pseudo_legal = board.pseudo_legal_moves();
                let tt_move = tt_choice.map(|index| *index.get(&pseudo_legal));
                let killers = killer_choices.map(|choice| choice.map(|index| *index.get(&previous)));
                check_pickers(&board, tt_move, killers)?;
                check_pickers(&board, None, [None; 2])?;
                let legal = board.legal_moves();
                if legal.is_empty() {
                    break;
                }
                previous = pseudo_legal;
                board = board.make(*next.get(&legal));
            }
        }
    }

    #[test]
    fn killers_come_after_noisy_moves_and_other_quiets_follow_by_history() {
        let board =
            Board::from_fen("4k3/8/8/2r1q3/3P4/B4N2/P7/3Q3K w - - 0 1").expect("test FEN is valid");
        let killer = board_move(&board, "h1g2");
        let liked = board_move(&board, "d1d3");
        let disliked = board_move(&board, "f3g1");
        let mut history = History::new();
        history.update(Colour::White, liked, 1_000);
        history.update(Colour::White, disliked, -1_000);
        let order: Vec<String> = drain(
            MovePicker::new(&board, None, [Some(killer), None]),
            &history,
        )
        .iter()
        .map(Move::to_string)
        .collect();
        assert_eq!(order[4..6], ["h1g2", "d1d3"], "full order {order:?}");
        assert_eq!(
            order.last().map(String::as_str),
            Some("f3g1"),
            "full order {order:?}"
        );
    }

    #[test]
    fn noisy_moves_come_before_quiet_ones_by_most_valuable_victim_first() {
        // White can take the queen with the pawn or the knight, and the rook with the pawn
        // or the bishop.
        let board =
            Board::from_fen("4k3/8/8/2r1q3/3P4/B4N2/P7/3Q3K w - - 0 1").expect("test FEN is valid");
        let order: Vec<String> = drain(MovePicker::new(&board, None, [None; 2]), &History::new())
            .iter()
            .map(Move::to_string)
            .collect();
        let first_quiet = order
            .iter()
            .position(|mv| !board_move(&board, mv).is_noisy())
            .expect("there are quiet moves");
        assert_eq!(&order[..first_quiet], ["d4e5", "f3e5", "d4c5", "a3c5"]);
    }

    #[test]
    fn captures_that_lose_material_come_after_the_quiet_moves() {
        // Nxd5 cxd5 breaks even, but Qxd5 cxd5 loses the queen for a knight.
        let board =
            Board::from_fen("4k3/8/2p5/3n4/8/4N3/8/3QK3 w - - 0 1").expect("test FEN is valid");
        let killer = board_move(&board, "e1f1");
        let order: Vec<String> = drain(
            MovePicker::new(&board, None, [Some(killer), None]),
            &History::new(),
        )
        .iter()
        .map(Move::to_string)
        .collect();
        assert_eq!(order[..2], ["e3d5", "e1f1"], "full order {order:?}");
        assert_eq!(
            order.last().map(String::as_str),
            Some("d1d5"),
            "full order {order:?}"
        );
    }

    #[test]
    fn a_good_noisy_picker_leaves_out_captures_that_lose_material_even_as_the_tt_move() {
        // Nxd5 cxd5 breaks even, but Qxd5 cxd5 loses the queen for a knight.
        let board =
            Board::from_fen("4k3/8/2p5/3n4/8/4N3/8/3QK3 w - - 0 1").expect("test FEN is valid");
        let history = History::new();
        for tt_move in [None, Some(board_move(&board, "d1d5"))] {
            let handed_out: Vec<String> = drain(MovePicker::good_noisy(&board, tt_move), &history)
                .iter()
                .map(Move::to_string)
                .collect();
            assert_eq!(handed_out, ["e3d5"], "with TT move {tt_move:?}");
        }
    }

    #[test]
    fn skipping_quiets_still_hands_out_the_remaining_noisy_moves_and_nothing_after() {
        let board =
            Board::from_fen("4k3/8/8/2r1q3/3P4/B4N2/P7/3Q3K w - - 0 1").expect("test FEN is valid");
        let killer = board_move(&board, "h1g2");
        let history = History::new();

        let mut picker = MovePicker::new(&board, None, [Some(killer), None]);
        let first = picker.next_move(&history).map(|mv| mv.to_string());
        assert_eq!(first.as_deref(), Some("d4e5"));
        picker.skip_quiets();
        let rest: Vec<String> = drain(picker, &history)
            .iter()
            .map(Move::to_string)
            .collect();
        assert_eq!(rest, ["f3e5", "d4c5", "a3c5"]);

        let mut picker = MovePicker::new(&board, None, [Some(killer), None]);
        let handed_out: Vec<String> = std::iter::from_fn(|| picker.next_move(&history))
            .take(5)
            .map(|mv| mv.to_string())
            .collect();
        assert_eq!(handed_out.last().map(String::as_str), Some("h1g2"));
        picker.skip_quiets();
        assert_eq!(picker.next_move(&history), None, "after {handed_out:?}");
    }

    /// The pseudo-legal move of `board` written as `text`.
    fn board_move(board: &Board, text: &str) -> Move {
        *board
            .pseudo_legal_moves()
            .iter()
            .find(|mv| mv.to_string() == text)
            .unwrap_or_else(|| panic!("{text} is not a move of {}", board.to_fen()))
    }
}
