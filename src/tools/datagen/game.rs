//! One self-play game: a random opening, then searched moves until the game ends or is
//! adjudicated.

use super::record::{GameRecord, Outcome};
use super::rng::Rng;
use crate::chess::{Board, Game, Move};
use crate::search::{Limits, Score, StopFlag, TranspositionTable, Worker};

/// The fewest random moves an opening is made of; it has one more half the time.
const MIN_RANDOM_MOVES: usize = 8;

/// An opening whose searched score is further than this from 0, in centipawns, is too
/// lopsided to teach much, so it is discarded.
const MAX_OPENING_SCORE: i32 = 1_000;

/// A game is adjudicated a win once the score has favoured one side by at least this many
/// centipawns for [`WIN_PLIES`] plies in a row.
const WIN_SCORE: i32 = 2_500;
/// See [`WIN_SCORE`].
const WIN_PLIES: u32 = 4;

/// A game is adjudicated a draw once the score has stayed within this many centipawns of
/// 0 for [`DRAW_PLIES`] plies in a row.
const DRAW_SCORE: i32 = 4;
/// See [`DRAW_SCORE`].
const DRAW_PLIES: u32 = 12;

/// The score recorded for a position in which one side mates by force.
const MATE_SCORE: i16 = i16::MAX;

/// Node limits for the searches of a self-play game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The soft node limit of each move's search: no new iteration starts after it.
    pub soft_nodes: u64,
    /// The hard node limit of each move's search.
    pub hard_nodes: u64,
    /// The node limit of the search that checks an opening is balanced.
    pub opening_nodes: u64,
}

impl Default for Settings {
    /// Soft 10,000 and hard 80,000 nodes a move, and 50,000 for the opening check.
    fn default() -> Self {
        Self {
            soft_nodes: 10_000,
            hard_nodes: 80_000,
            opening_nodes: 50_000,
        }
    }
}

/// Plays one self-play game and returns it with its result, or `None` if its random
/// opening ends the game or is too lopsided.
///
/// The opening is the standard position plus a few random legal moves, picked by `rng`.
/// Every search uses `tt`, cleared first, and a fresh [`Worker`], so nothing carries over
/// from the previous game.
pub fn play_game(
    rng: &mut Rng,
    settings: &Settings,
    tt: &TranspositionTable,
) -> Option<(GameRecord, Outcome)> {
    let mut game = random_opening(rng)?;
    tt.clear();
    let mut worker = Worker::new(StopFlag::new(), tt.clone());
    let opening = Limits {
        nodes: Some(settings.opening_nodes),
        ..Limits::default()
    };
    let (_, score) = search(&mut worker, &game, opening);
    match score {
        Score::Centipawns(cp) if cp.abs() <= MAX_OPENING_SCORE => {}
        _ => return None,
    }

    let limits = Limits {
        nodes: Some(settings.hard_nodes),
        soft_nodes: Some(settings.soft_nodes),
        ..Limits::default()
    };
    let mut record = GameRecord::new(game.board().clone());
    let mut adjudicator = Adjudicator::default();
    loop {
        if let Some(outcome) = rules_outcome(&game) {
            return Some((record, outcome));
        }
        let (mv, score) = search(&mut worker, &game, limits);
        let sign = game.board().side_to_move().sign();
        let white_score = match score {
            Score::Centipawns(cp) => Score::Centipawns(sign * cp),
            Score::Mate(moves) => Score::Mate(sign * moves),
        };
        record.push(mv, to_record(white_score));
        if let Some(outcome) = adjudicator.update(white_score) {
            return Some((record, outcome));
        }
        game.play(mv);
    }
}

/// Plays [`MIN_RANDOM_MOVES`] random legal moves, or one more, from the standard
/// position, or returns `None` if they end the game.
fn random_opening(rng: &mut Rng) -> Option<Game> {
    let mut game = Game::startpos();
    let plies = MIN_RANDOM_MOVES + rng.below(2);
    for _ in 0..plies {
        let moves = game.board().legal_moves();
        if moves.is_empty() {
            return None;
        }
        game.play(moves[rng.below(moves.len())]);
    }
    rules_outcome(&game).is_none().then_some(game)
}

/// Searches the current position of `game` and returns the best move with its score,
/// from the side to move's point of view.
///
/// # Panics
///
/// Panics if the position has no legal moves, which callers rule out first.
fn search(worker: &mut Worker, game: &Game, limits: Limits) -> (Move, Score) {
    let mut score = Score::Centipawns(0);
    let mv = worker
        .search(game, limits, |report| score = report.score)
        .unwrap_or_else(|| panic!("{} has no legal moves", game.board().to_fen()));
    (mv, score)
}

/// The result the rules give the current position of `game`, if it is over: checkmate,
/// stalemate, the fifty-move rule, threefold **Repetition** or insufficient material.
fn rules_outcome(game: &Game) -> Option<Outcome> {
    let board: &Board = game.board();
    if board.legal_moves().is_empty() {
        return Some(if board.in_check() {
            Outcome::win_for(!board.side_to_move())
        } else {
            Outcome::Draw
        });
    }
    let drawn = board.is_fifty_move_draw()
        || board.has_insufficient_material()
        || game.history().repetitions(board) >= 2;
    drawn.then_some(Outcome::Draw)
}

/// Converts a white-relative score to the `i16` viriformat records, with a forced mate as
/// [`MATE_SCORE`].
fn to_record(white_score: Score) -> i16 {
    match white_score {
        Score::Centipawns(cp) => {
            cp.clamp(-i32::from(MATE_SCORE) + 1, i32::from(MATE_SCORE) - 1) as i16
        }
        Score::Mate(moves) if moves > 0 => MATE_SCORE,
        Score::Mate(_) => -MATE_SCORE,
    }
}

/// Ends a game early once the searched scores make its result clear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Adjudicator {
    /// Plies in a row the score has favoured White by at least [`WIN_SCORE`].
    white_winning: u32,
    /// Plies in a row the score has favoured Black by at least [`WIN_SCORE`].
    black_winning: u32,
    /// Plies in a row the score has been within [`DRAW_SCORE`] of 0.
    drawish: u32,
}

impl Adjudicator {
    /// Takes the white-relative score of the latest position and returns the result if it
    /// settles the game: a mate score at once, or a run of decisive or drawish scores.
    fn update(&mut self, white_score: Score) -> Option<Outcome> {
        let cp = match white_score {
            Score::Mate(moves) if moves > 0 => return Some(Outcome::WhiteWin),
            Score::Mate(_) => return Some(Outcome::BlackWin),
            Score::Centipawns(cp) => cp,
        };
        let extend = |run: u32, holds: bool| if holds { run + 1 } else { 0 };
        self.white_winning = extend(self.white_winning, cp >= WIN_SCORE);
        self.black_winning = extend(self.black_winning, cp <= -WIN_SCORE);
        self.drawish = extend(self.drawish, cp.abs() <= DRAW_SCORE);
        if self.white_winning >= WIN_PLIES {
            Some(Outcome::WhiteWin)
        } else if self.black_winning >= WIN_PLIES {
            Some(Outcome::BlackWin)
        } else if self.drawish >= DRAW_PLIES {
            Some(Outcome::Draw)
        } else {
            None
        }
    }
}
