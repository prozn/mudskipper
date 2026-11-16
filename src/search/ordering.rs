//! Move ordering by a staged move picker (ADR-0003), which generates each group of moves
//! only when the search reaches it: the transposition table's move, never generated; then
//! captures and promotions by most valuable victim, least valuable attacker; then quiet
//! moves in generation order. A cutoff in an early stage skips generating the later ones.

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
}

/// Where a picker has got to. Each stage moves on to the next one declared, except that a
/// noisy-only picker finishes after the noisy stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    TtMove,
    GenerateNoisy,
    Noisy,
    GenerateQuiet,
    Quiet,
    Done,
}

/// Pseudo-legal moves handed out best first, each once, generating and ordering each
/// stage's moves only when it is reached.
pub(super) struct MovePicker<'board> {
    board: &'board Board,
    tt_move: Option<Move>,
    coverage: Coverage,
    stage: Stage,
    /// The moves of the current stage.
    moves: MoveList,
    /// The ordering score of each move of the noisy stage.
    scores: [i32; MoveList::CAPACITY],
    /// How many of the current stage's moves have been handed out or skipped.
    next: usize,
}

impl<'board> MovePicker<'board> {
    /// Returns a picker of every pseudo-legal move of `board`, with `tt_move` first.
    ///
    /// `tt_move` must be pseudo-legal in `board`, as checked by
    /// [`Board::is_pseudo_legal`].
    pub(super) fn new(board: &'board Board, tt_move: Option<Move>) -> Self {
        Self::with_coverage(board, tt_move, Coverage::All)
    }

    /// Returns a picker of the pseudo-legal captures and promotions of `board`, with
    /// `tt_move` first if it is one of them.
    ///
    /// `tt_move` must be pseudo-legal in `board`, as checked by
    /// [`Board::is_pseudo_legal`].
    pub(super) fn noisy(board: &'board Board, tt_move: Option<Move>) -> Self {
        Self::with_coverage(board, tt_move.filter(|mv| mv.is_noisy()), Coverage::Noisy)
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
            coverage,
            stage: Stage::TtMove,
            moves: MoveList::new(),
            scores: [0; MoveList::CAPACITY],
            next: 0,
        }
    }

    /// Hands out the best-scored noisy move not yet handed out, or `None` once they are
    /// all gone. The first of equally scored moves wins, keeping the order deterministic.
    fn pick_best_noisy(&mut self) -> Option<Move> {
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

    /// Hands out the next quiet move in generation order, or `None` once they are all
    /// gone.
    fn pick_next_quiet(&mut self) -> Option<Move> {
        let mv = self.moves.get(self.next).copied()?;
        self.next += 1;
        Some(mv)
    }
}

impl Iterator for MovePicker<'_> {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
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
                    self.stage = Stage::Noisy;
                }
                Stage::Noisy => match self.pick_best_noisy() {
                    Some(mv) if Some(mv) == self.tt_move => {}
                    Some(mv) => return Some(mv),
                    None if self.coverage == Coverage::All => self.stage = Stage::GenerateQuiet,
                    None => self.stage = Stage::Done,
                },
                Stage::GenerateQuiet => {
                    self.moves = self.board.quiet_moves();
                    self.next = 0;
                    self.stage = Stage::Quiet;
                }
                Stage::Quiet => match self.pick_next_quiet() {
                    Some(mv) if Some(mv) == self.tt_move => {}
                    Some(mv) => return Some(mv),
                    None => self.stage = Stage::Done,
                },
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

    /// Checks that the pickers of `board` produce exactly the moves of the full
    /// generators, each once, with `tt_move` first when they may return it.
    fn check_pickers(board: &Board, tt_move: Option<Move>) -> Result<(), TestCaseError> {
        let fen = board.to_fen();
        let all: Vec<Move> = MovePicker::new(board, tt_move).collect();
        prop_assert_eq!(
            sorted(all.iter().copied()),
            sorted(board.pseudo_legal_moves().iter().copied()),
            "all moves of {:?} with TT move {:?}",
            fen,
            tt_move
        );
        if let Some(tt_move) = tt_move {
            prop_assert_eq!(all.first(), Some(&tt_move), "in {:?}", fen);
        }

        let noisy: Vec<Move> = MovePicker::noisy(board, tt_move).collect();
        prop_assert_eq!(
            sorted(noisy.iter().copied()),
            sorted(board.noisy_moves().iter().copied()),
            "noisy moves of {:?} with TT move {:?}",
            fen,
            tt_move
        );
        if let Some(tt_move) = tt_move.filter(|mv| mv.is_noisy()) {
            prop_assert_eq!(noisy.first(), Some(&tt_move), "in {:?}", fen);
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn pickers_produce_each_generated_move_once_with_or_without_a_tt_move(
            start in 0..STARTS.len(),
            choices in prop::collection::vec((any::<Index>(), any::<Option<Index>>()), 1..MAX_PLIES),
        ) {
            let mut board = Board::from_fen(STARTS[start]).expect("start FEN is valid");
            for (next, tt_choice) in choices {
                let pseudo_legal = board.pseudo_legal_moves();
                let tt_move = tt_choice.map(|index| *index.get(&pseudo_legal));
                check_pickers(&board, tt_move)?;
                check_pickers(&board, None)?;
                let legal = board.legal_moves();
                if legal.is_empty() {
                    break;
                }
                board = board.make(*next.get(&legal));
            }
        }
    }

    #[test]
    fn noisy_moves_come_before_quiet_ones_by_most_valuable_victim_first() {
        // White can take the queen with the pawn or the knight, and the rook with the pawn
        // or the bishop.
        let board =
            Board::from_fen("4k3/8/8/2r1q3/3P4/B4N2/P7/3Q3K w - - 0 1").expect("test FEN is valid");
        let order: Vec<String> = MovePicker::new(&board, None)
            .map(|mv| mv.to_string())
            .collect();
        let first_quiet = order
            .iter()
            .position(|mv| !board_move(&board, mv).is_noisy())
            .expect("there are quiet moves");
        assert_eq!(&order[..first_quiet], ["d4e5", "f3e5", "d4c5", "a3c5"]);
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
