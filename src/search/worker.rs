//! The state one search thread owns, and the search it runs.

use std::time::{Duration, Instant};

use super::MAX_PLY;
use super::ordering::OrderedMoves;
use super::score::{self, DRAW, INFINITY, Score};
use crate::chess::{Board, KeyHistory, Move};
use crate::eval;

/// Deepest iteration the search runs, leaving room in the principal variation table for
/// the ply below it.
const MAX_DEPTH: u32 = MAX_PLY as u32 - 1;

/// When a search stops. A search with no limits runs to the deepest supported depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    /// The deepest iteration to complete.
    pub depth: Option<u32>,
    /// The number of nodes after which to stop, even in the middle of an iteration.
    pub nodes: Option<u64>,
}

/// Progress of a search, reported after each completed iteration and when it stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report<'pv> {
    /// The depth of the last completed iteration.
    pub depth: u32,
    /// The deepest ply reached, counting quiescence search.
    pub seldepth: usize,
    /// The score of the principal variation.
    pub score: Score,
    /// The nodes searched so far.
    pub nodes: u64,
    /// The time since the search started.
    pub elapsed: Duration,
    /// The principal variation, starting with the best move.
    pub pv: &'pv [Move],
}

/// The state one search thread owns: its principal variation table, node counter and the
/// keys of the positions leading to the one it is searching.
#[derive(Debug)]
pub struct Worker {
    pv: PvTable,
    /// Keys of every position before the current one, from the game and the search.
    history: KeyHistory,
    /// Keys of the positions before the current one since the search's root.
    tree: KeyHistory,
    nodes: u64,
    seldepth: usize,
    limits: Limits,
    stopped: bool,
}

impl Worker {
    /// Returns a worker ready to search.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pv: PvTable::new(),
            history: KeyHistory::new(),
            tree: KeyHistory::new(),
            nodes: 0,
            seldepth: 0,
            limits: Limits::default(),
            stopped: false,
        }
    }

    /// Searches `board`, reached after the positions in `history`, until `limits` say to
    /// stop, and returns the best move, or `None` if there are no legal moves.
    ///
    /// The search deepens one ply at a time and calls `report` after each completed
    /// iteration, and once more if a node limit cuts an iteration short.
    pub fn search(
        &mut self,
        board: &Board,
        history: &KeyHistory,
        limits: Limits,
        mut report: impl FnMut(&Report<'_>),
    ) -> Option<Move> {
        let start = Instant::now();
        let mut best_move = *board.legal_moves().first()?;
        self.history.clone_from(history);
        self.tree = KeyHistory::new();
        self.nodes = 0;
        self.seldepth = 0;
        self.limits = limits;
        self.stopped = false;

        let max_depth = limits
            .depth
            .map_or(MAX_DEPTH, |depth| depth.clamp(1, MAX_DEPTH));
        let mut completed: Option<Iteration> = None;
        for depth in 1..=max_depth {
            let score = self.negamax(board, depth, 0, -INFINITY, INFINITY);
            if self.stopped {
                break;
            }
            let iteration = Iteration {
                depth,
                seldepth: self.seldepth,
                score: Score::from_internal(score),
                pv: self.pv.line(0).to_vec(),
            };
            best_move = iteration.pv[0];
            report(&iteration.report(self.nodes, start.elapsed()));
            completed = Some(iteration);
        }
        if self.stopped
            && let Some(iteration) = &completed
        {
            report(&iteration.report(self.nodes, start.elapsed()));
        }
        Some(best_move)
    }

    /// Searches `board`, `ply` plies from the root, to `depth` plies and returns its score
    /// within the window `alpha` to `beta`.
    fn negamax(&mut self, board: &Board, depth: u32, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        self.pv.clear(ply);
        if depth == 0 {
            return self.quiescence(board, ply, alpha, beta);
        }
        if self.visit(ply) || (ply > 0 && self.is_draw(board)) {
            return DRAW;
        }
        if ply >= MAX_PLY - 1 {
            return eval::evaluate(board);
        }

        let mut best = -INFINITY;
        let mut legal_moves = 0;
        for mv in OrderedMoves::new(board, &board.pseudo_legal_moves()) {
            if !board.is_legal(mv) {
                continue;
            }
            legal_moves += 1;
            let child = board.make(mv);
            self.enter(board);
            let score = -self.negamax(&child, depth - 1, ply + 1, -beta, -alpha);
            self.leave();
            if self.stopped {
                return DRAW;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    self.pv.update(ply, mv);
                    if score >= beta {
                        break;
                    }
                }
            }
        }
        if legal_moves == 0 {
            return if board.in_check() {
                score::mated_in(ply)
            } else {
                DRAW
            };
        }
        best
    }

    /// Searches only noisy moves from `board`, `ply` plies from the root, until the
    /// position is quiet, and returns its score within the window `alpha` to `beta`.
    ///
    /// The side to move may stand pat on the static evaluation instead of capturing.
    /// In check it may not, and every evasion is searched so that mates are seen.
    fn quiescence(&mut self, board: &Board, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        if self.visit(ply) || self.is_draw(board) {
            return DRAW;
        }
        if ply >= MAX_PLY {
            return eval::evaluate(board);
        }

        let in_check = board.in_check();
        let mut best = -INFINITY;
        let moves = if in_check {
            board.pseudo_legal_moves()
        } else {
            best = eval::evaluate(board);
            if best >= beta {
                return best;
            }
            alpha = alpha.max(best);
            board.noisy_moves()
        };

        let mut legal_moves = 0;
        for mv in OrderedMoves::new(board, &moves) {
            if !board.is_legal(mv) {
                continue;
            }
            legal_moves += 1;
            let child = board.make(mv);
            self.enter(board);
            let score = -self.quiescence(&child, ply + 1, -beta, -alpha);
            self.leave();
            if self.stopped {
                return DRAW;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    if score >= beta {
                        break;
                    }
                }
            }
        }
        if in_check && legal_moves == 0 {
            return score::mated_in(ply);
        }
        best
    }

    /// Counts a node at `ply` and reports whether the search must stop.
    fn visit(&mut self, ply: usize) -> bool {
        if self.limits.nodes.is_some_and(|limit| self.nodes >= limit) {
            self.stopped = true;
            return true;
        }
        self.nodes += 1;
        self.seldepth = self.seldepth.max(ply);
        false
    }

    /// Records that the search is moving on from `board` to one of its children.
    fn enter(&mut self, board: &Board) {
        self.history.push(board.key());
        self.tree.push(board.key());
    }

    /// Undoes the matching [`Worker::enter`] on returning from a child.
    fn leave(&mut self) {
        self.history.pop();
        self.tree.pop();
    }

    /// Reports whether `board` is drawn by repetition, the fifty-move rule or
    /// insufficient material.
    ///
    /// A position that already occurred since the root counts as a repetition: if
    /// repeating is best now, it stays best when the position comes round again. Earlier
    /// positions in the game count only once they make a threefold repetition.
    fn is_draw(&self, board: &Board) -> bool {
        self.tree.repetitions(board) > 0
            || self.history.repetitions(board) >= 2
            || board.is_fifty_move_draw()
            || board.has_insufficient_material()
    }
}

/// The result of a completed iteration of iterative deepening.
#[derive(Debug)]
struct Iteration {
    depth: u32,
    seldepth: usize,
    score: Score,
    pv: Vec<Move>,
}

impl Iteration {
    /// Reports this iteration with the search's current node count and elapsed time.
    fn report(&self, nodes: u64, elapsed: Duration) -> Report<'_> {
        Report {
            depth: self.depth,
            seldepth: self.seldepth,
            score: self.score,
            nodes,
            elapsed,
            pv: &self.pv,
        }
    }
}

impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}

/// The principal variation found below each ply of the current search line, stored as a
/// triangular table.
#[derive(Debug)]
struct PvTable {
    moves: Box<[[Move; MAX_PLY]]>,
    lengths: [usize; MAX_PLY],
}

impl PvTable {
    /// Returns a table with every line empty.
    fn new() -> Self {
        Self {
            moves: vec![[Move::NULL; MAX_PLY]; MAX_PLY].into_boxed_slice(),
            lengths: [0; MAX_PLY],
        }
    }

    /// Empties the line at `ply`.
    fn clear(&mut self, ply: usize) {
        self.lengths[ply] = 0;
    }

    /// Makes `mv` followed by the line at `ply + 1` the line at `ply`.
    fn update(&mut self, ply: usize, mv: Move) {
        let child_len = self.lengths[ply + 1];
        let (parent, child) = self.moves.split_at_mut(ply + 1);
        let line = &mut parent[ply];
        line[0] = mv;
        line[1..=child_len].copy_from_slice(&child[0][..child_len]);
        self.lengths[ply] = child_len + 1;
    }

    /// The principal variation at `ply`.
    fn line(&self, ply: usize) -> &[Move] {
        &self.moves[ply][..self.lengths[ply]]
    }
}
