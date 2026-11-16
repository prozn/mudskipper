//! The state one search thread owns, and the search it runs.

use std::time::{Duration, Instant};

use super::MAX_PLY;
use super::history::{History, Killers, bonus};
use super::ordering::MovePicker;
use super::score::{self, DRAW, INFINITY, Score};
use super::stop::StopFlag;
use super::time::TimeLimit;
use super::tt::{Bound, Entry, TranspositionTable};
use crate::chess::{Board, Colour, Game, KeyHistory, Move, MoveList};
use crate::eval;

/// Deepest iteration the search runs, leaving room in the principal variation table for
/// the ply below it.
const MAX_DEPTH: u32 = MAX_PLY as u32 - 1;

/// Nodes searched between checks of the stop flag and the clock, so that a stop takes
/// effect within about a millisecond without the cost of checking at every node.
const CHECK_INTERVAL: u64 = 1024;

/// Shallowest depth at which a node with no transposition table move is searched one ply
/// shallower (internal iterative reduction), the threshold common among open-source
/// engines at a similar stage. Lowering it reduces more nodes, trading accuracy for depth.
const IIR_MIN_DEPTH: u32 = 4;

/// When a search stops. A search with no limits runs to the deepest supported depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    /// The deepest iteration to complete.
    pub depth: Option<u32>,
    /// The number of nodes after which to stop, even in the middle of an iteration.
    pub nodes: Option<u64>,
    /// How long the search may run.
    pub time: Option<TimeLimit>,
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
    /// How full the transposition table is, in permille.
    pub hashfull: u32,
    /// The principal variation, starting with the best move.
    pub pv: &'pv [Move],
}

/// The state one search thread owns: its principal variation table, node counter, move
/// ordering statistics and the keys of the positions leading to the one it is searching,
/// plus the flag that stops it and the transposition table it shares with other threads.
///
/// History is kept from one search to the next, so a fresh worker is needed to forget it.
#[derive(Debug)]
pub struct Worker {
    pv: PvTable,
    tt: TranspositionTable,
    /// Quiet moves that caused a beta cutoff at each ply, cleared for each search.
    killers: Killers,
    /// Butterfly history of quiet moves that caused a beta cutoff, kept between searches.
    history: History,
    /// Keys of every position before the current one, from the game and the search.
    keys: KeyHistory,
    /// Keys of the positions before the current one since the search's root.
    tree: KeyHistory,
    nodes: u64,
    seldepth: usize,
    limits: Limits,
    /// When the search started.
    start: Instant,
    stop: StopFlag,
    /// Whether the stop flag and the time limit may end the search, which they may once
    /// an iteration has completed and there is a searched move to return.
    interruptible: bool,
    stopped: bool,
}

impl Worker {
    /// Returns a worker ready to search, which stops when `stop` is raised and shares
    /// `tt` with every other worker holding a clone of it.
    #[must_use]
    pub fn new(stop: StopFlag, tt: TranspositionTable) -> Self {
        Self {
            pv: PvTable::new(),
            tt,
            killers: Killers::new(),
            history: History::new(),
            keys: KeyHistory::new(),
            tree: KeyHistory::new(),
            nodes: 0,
            seldepth: 0,
            limits: Limits::default(),
            start: Instant::now(),
            stop,
            interruptible: false,
            stopped: false,
        }
    }

    /// Searches the current position of `game` until `limits` say to stop, and returns the
    /// best move, or `None` if there are no legal moves.
    ///
    /// The search deepens one ply at a time and calls `report` after each completed
    /// iteration, and once more if a node limit, the time limit or the stop flag cuts an
    /// iteration short. A depth limit of 0 is treated as 1, so that there is always a
    /// searched move to report. For the same reason the time limit and the stop flag are
    /// ignored until the first iteration completes.
    ///
    /// Each search starts a new generation of the transposition table, so that what
    /// earlier searches stored is kept but replaced first. Killer moves are cleared for
    /// each search; history carries over.
    pub fn search(
        &mut self,
        game: &Game,
        limits: Limits,
        mut report: impl FnMut(&Report<'_>),
    ) -> Option<Move> {
        self.start = Instant::now();
        self.tt.new_search();
        let board = game.board();
        let mut best_move = *board.legal_moves().first()?;
        self.keys.clone_from(game.history());
        self.tree = KeyHistory::new();
        self.killers = Killers::new();
        self.nodes = 0;
        self.seldepth = 0;
        self.limits = limits;
        self.interruptible = false;
        self.stopped = false;

        let max_depth = limits
            .depth
            .map_or(MAX_DEPTH, |depth| depth.clamp(1, MAX_DEPTH));
        let mut completed: Option<Iteration> = None;
        for depth in 1..=max_depth {
            if self.interruptible && (self.stop.is_raised() || self.soft_time_is_up()) {
                break;
            }
            let score = self.negamax(board, depth, 0, -INFINITY, INFINITY, Node::Pv);
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
            report(&iteration.report(self.nodes, self.start.elapsed(), self.tt.hashfull()));
            completed = Some(iteration);
            self.interruptible = true;
        }
        if self.stopped
            && let Some(iteration) = &completed
        {
            report(&iteration.report(self.nodes, self.start.elapsed(), self.tt.hashfull()));
        }
        Some(best_move)
    }

    /// Searches `board`, `ply` plies from the root, to `depth` plies and returns its score
    /// within the window `alpha` to `beta`.
    ///
    /// Off the principal variation, a transposition table entry at least `depth` deep
    /// whose bound settles the window is returned without searching.
    ///
    /// A node at least [`IIR_MIN_DEPTH`] deep with no transposition table move is searched
    /// one ply shallower: with no good move to try first, a full-depth search would mostly
    /// be wasted effort.
    ///
    /// A quiet move causing a beta cutoff becomes a killer move at this ply and earns a
    /// history bonus, and the quiet moves tried before it take the matching malus.
    fn negamax(
        &mut self,
        board: &Board,
        mut depth: u32,
        ply: usize,
        mut alpha: i32,
        beta: i32,
        node: Node,
    ) -> i32 {
        self.pv.clear(ply);
        if depth == 0 {
            return self.quiescence(board, ply, alpha, beta);
        }
        if self.should_stop() {
            return DRAW;
        }
        self.count_node(ply);
        if ply > 0 && self.is_draw(board) {
            return DRAW;
        }
        if ply >= MAX_PLY - 1 {
            return eval::evaluate(board);
        }

        let tt_entry = self.tt.probe(board.key(), ply);
        if let Some(entry) = tt_entry
            && node == Node::NonPv
            && entry.depth >= depth
            && entry.cuts_off(alpha, beta)
        {
            return entry.score;
        }
        let tt_move = tt_move(board, tt_entry);
        if tt_move.is_none() && depth >= IIR_MIN_DEPTH {
            depth -= 1;
        }

        let original_alpha = alpha;
        let mut best = -INFINITY;
        let mut best_move = None;
        let mut legal_moves = 0;
        let mut quiets_tried = MoveList::new();
        let mut picker = MovePicker::new(board, tt_move, self.killers.get(ply));
        while let Some(mv) = picker.next_move(&self.history) {
            if !board.is_legal(mv) {
                continue;
            }
            legal_moves += 1;
            let child_node = if node == Node::Pv && legal_moves == 1 {
                Node::Pv
            } else {
                Node::NonPv
            };
            let child = board.make(mv);
            self.enter(board);
            let score = -self.negamax(&child, depth - 1, ply + 1, -beta, -alpha, child_node);
            self.leave();
            if self.stopped {
                return DRAW;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    best_move = Some(mv);
                    self.pv.update(ply, mv);
                    if score >= beta {
                        if !mv.is_noisy() {
                            self.reward_quiet_cutoff(
                                board.side_to_move(),
                                ply,
                                depth,
                                mv,
                                &quiets_tried,
                            );
                        }
                        break;
                    }
                }
            }
            if !mv.is_noisy() {
                quiets_tried.push(mv);
            }
        }
        if legal_moves == 0 {
            return if board.in_check() {
                score::mated_in(ply)
            } else {
                DRAW
            };
        }
        self.tt.store(
            board.key(),
            ply,
            Entry {
                mv: best_move,
                score: best,
                eval: tt_entry.and_then(|entry| entry.eval),
                depth,
                bound: bound(best, original_alpha, beta),
            },
        );
        best
    }

    /// Searches only noisy moves from `board`, `ply` plies from the root, until the
    /// position is quiet, and returns its score within the window `alpha` to `beta`.
    ///
    /// The side to move may stand pat on the static evaluation instead of capturing.
    /// In check it may not, and every evasion is searched so that mates are seen.
    ///
    /// Stalemate is not detected when the side to move is not in check: it stands pat on
    /// the static evaluation even with no legal move. Detecting it would need a full
    /// legal-move generation at every quiet node, which costs more than it gains, so this
    /// is deliberate. The main search scores stalemate as a draw, so from depth 2 upwards
    /// a stalemating move is seen as one; at depth 1 the engine may play into stalemate.
    ///
    /// Every node probes the transposition table and returns an entry whose bound settles
    /// the window, and stores its result at depth 0, so that main search entries win
    /// replacement.
    fn quiescence(&mut self, board: &Board, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        if self.should_stop() {
            return DRAW;
        }
        self.count_node(ply);
        if self.is_draw(board) {
            return DRAW;
        }
        if ply >= MAX_PLY {
            return eval::evaluate(board);
        }

        let tt_entry = self.tt.probe(board.key(), ply);
        if let Some(entry) = tt_entry
            && entry.cuts_off(alpha, beta)
        {
            return entry.score;
        }

        let original_alpha = alpha;
        let in_check = board.in_check();
        let mut best = -INFINITY;
        let mut eval = None;
        let tt_move = tt_move(board, tt_entry);
        let mut moves = if in_check {
            MovePicker::new(board, tt_move, [None; 2])
        } else {
            let static_eval = tt_entry
                .and_then(|entry| entry.eval)
                .unwrap_or_else(|| eval::evaluate(board));
            eval = Some(static_eval);
            best = static_eval;
            if best >= beta {
                self.tt.store(
                    board.key(),
                    ply,
                    Entry {
                        mv: None,
                        score: best,
                        eval,
                        depth: 0,
                        bound: bound(best, original_alpha, beta),
                    },
                );
                return best;
            }
            alpha = alpha.max(best);
            MovePicker::noisy(board, tt_move)
        };

        let mut best_move = None;
        let mut legal_moves = 0;
        while let Some(mv) = moves.next_move(&self.history) {
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
                    best_move = Some(mv);
                    if score >= beta {
                        break;
                    }
                }
            }
        }
        if in_check && legal_moves == 0 {
            return score::mated_in(ply);
        }
        self.tt.store(
            board.key(),
            ply,
            Entry {
                mv: best_move,
                score: best,
                eval,
                depth: 0,
                bound: bound(best, original_alpha, beta),
            },
        );
        best
    }

    /// Records that the quiet move `mv`, played by `colour`, caused a beta cutoff `ply`
    /// plies from the root and `depth` plies deep, after the quiet moves `tried` failed to.
    fn reward_quiet_cutoff(
        &mut self,
        colour: Colour,
        ply: usize,
        depth: u32,
        mv: Move,
        tried: &[Move],
    ) {
        self.killers.update(ply, mv);
        let bonus = bonus(depth);
        self.history.update(colour, mv, bonus);
        for &quiet in tried {
            self.history.update(colour, quiet, -bonus);
        }
    }

    /// Reports whether the limits or the stop flag say the search must stop, and if so
    /// marks it stopped.
    fn should_stop(&mut self) -> bool {
        if self.limits.nodes.is_some_and(|limit| self.nodes >= limit)
            || (self.interruptible
                && self.nodes.is_multiple_of(CHECK_INTERVAL)
                && (self.stop.is_raised() || self.hard_time_is_up()))
        {
            self.stopped = true;
        }
        self.stopped
    }

    /// Reports whether the time limit says to start no new iteration.
    fn soft_time_is_up(&self) -> bool {
        self.limits
            .time
            .is_some_and(|time| self.start.elapsed() >= time.soft)
    }

    /// Reports whether the time limit says to stop at once.
    fn hard_time_is_up(&self) -> bool {
        self.limits
            .time
            .is_some_and(|time| self.start.elapsed() >= time.hard)
    }

    /// Counts a node searched at `ply`.
    fn count_node(&mut self, ply: usize) {
        self.nodes += 1;
        self.seldepth = self.seldepth.max(ply);
    }

    /// Records that the search is moving on from `board` to one of its children.
    fn enter(&mut self, board: &Board) {
        self.keys.push(board.key());
        self.tree.push(board.key());
    }

    /// Undoes the matching [`Worker::enter`] on returning from a child.
    fn leave(&mut self) {
        self.keys.pop();
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
            || self.keys.repetitions(board) >= 2
            || board.is_fifty_move_draw()
            || board.has_insufficient_material()
    }
}

/// Whether a node lies on the principal variation: the root, and the first move searched
/// from each node that does. Only nodes off it take transposition table cutoffs, so that
/// the principal variation is searched rather than read back from the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Pv,
    NonPv,
}

/// The move from `entry` to search first in `board`, if it has one that is pseudo-legal
/// there. A move stored under the same key may still belong to another position.
fn tt_move(board: &Board, entry: Option<Entry>) -> Option<Move> {
    entry
        .and_then(|entry| entry.mv)
        .filter(|&mv| board.is_pseudo_legal(mv))
}

/// The bound of a fail-soft search of the window `alpha` to `beta` that returned `best`.
fn bound(best: i32, alpha: i32, beta: i32) -> Bound {
    if best >= beta {
        Bound::Lower
    } else if best > alpha {
        Bound::Exact
    } else {
        Bound::Upper
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
    /// Reports this iteration with the search's current node count, elapsed time and
    /// transposition table fill.
    fn report(&self, nodes: u64, elapsed: Duration, hashfull: u32) -> Report<'_> {
        Report {
            depth: self.depth,
            seldepth: self.seldepth,
            score: self.score,
            nodes,
            elapsed,
            hashfull,
            pv: &self.pv,
        }
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
