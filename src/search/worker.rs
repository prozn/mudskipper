//! The state one search thread owns, and the search it runs.

use std::time::{Duration, Instant};

use super::MAX_PLY;
use super::history::{History, Killers};
use super::ordering::MovePicker;
use super::reduction::{self, LateMove};
use super::score::{self, DRAW, INFINITY, Score};
use super::stop::StopFlag;
use super::time::TimeLimit;
use super::tt::{Bound, Entry, TranspositionTable};
use super::window::Window;
use crate::chess::{Bitboard, Board, Game, KeyHistory, Move, MoveList};
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

/// Shallowest iteration that searches the root inside an aspiration window rather than
/// with the full window, the threshold common among open-source engines at a similar
/// stage. Shallower iterations' scores swing too much between depths to aim at; lowering
/// it risks more re-searches, raising it searches more iterations with the full window.
const ASPIRATION_MIN_DEPTH: u32 = 4;

/// Half the width of the first aspiration window, in centipawns, within the range of 10
/// to 25 common among open-source engines at a similar stage. The window grows by half
/// each time the score falls outside it. A narrower window cuts off more of the tree but
/// misses more often, each miss costing a re-search.
const ASPIRATION_DELTA: i32 = 20;

/// Shallowest depth at which null-move pruning is tried, the threshold common among
/// open-source engines at a similar stage. Lowering it prunes more shallow nodes, where
/// the reduced search is little more than quiescence.
const NMP_MIN_DEPTH: u32 = 3;

/// The plies a null move's search is reduced by at any depth, beyond the ply the null
/// move itself takes, as common among open-source engines at a similar stage. A larger
/// reduction makes the null-move search cheaper but more likely to miss a threat.
const NMP_BASE_REDUCTION: u32 = 3;

/// The null-move reduction grows by a ply for each this many plies of depth, so that deep
/// nodes, where a missed threat is more likely to be found again later, are reduced more.
const NMP_DEPTH_DIVISOR: u32 = 3;

/// Shallowest depth at which late moves are reduced, the threshold common among
/// open-source engines at a similar stage. Below it the reduced search would be little
/// more than quiescence.
const LMR_MIN_DEPTH: u32 = 3;

// A reduction capped at depth - 2 must not underflow.
const _: () = assert!(LMR_MIN_DEPTH >= 2, "LMR_MIN_DEPTH must be at least 2");

/// The number of legal moves searched at each node before later moves are reduced,
/// as common among open-source engines at a similar stage. Raising it searches more moves
/// at full depth.
const LMR_FULL_DEPTH_MOVES: u32 = 3;

/// Deepest depth at which a node may return its static evaluation without searching
/// (reverse futility pruning), within the range of 6 to 8 common among open-source engines
/// at a similar stage. Raising it prunes deeper nodes, where the static evaluation is a
/// poorer guide to the searched score.
const RFP_MAX_DEPTH: u32 = 7;

/// The margin per ply of depth by which the static evaluation must clear beta for reverse
/// futility pruning, in centipawns, as common among open-source engines at a similar
/// stage. A smaller margin prunes more nodes but misses more threats.
const RFP_MARGIN: i32 = 80;

/// Deepest depth at which quiet moves are skipped when the static evaluation is too far
/// below alpha for them to reach it (futility pruning), within the range common among
/// open-source engines at a similar stage.
const FP_MAX_DEPTH: u32 = 6;

/// The margin, in centipawns, a quiet move is allowed to gain over the static evaluation
/// at any depth for futility pruning. A smaller margin prunes more quiet moves.
const FP_BASE_MARGIN: i32 = 90;

/// The margin futility pruning allows per ply of depth beyond [`FP_BASE_MARGIN`], in
/// centipawns, since a deeper search has more moves in which to win material back.
const FP_DEPTH_MARGIN: i32 = 60;

/// Deepest depth at which quiet moves are skipped once enough moves have been searched
/// (late move pruning), within the range common among open-source engines at a similar
/// stage.
const LMP_MAX_DEPTH: u32 = 8;

/// The number of moves late move pruning searches at any depth before skipping quiet
/// moves; the count also grows by half the square of the depth. Raising it searches more
/// quiet moves.
const LMP_BASE_MOVES: u32 = 3;

/// Deepest depth at which a noisy move is skipped when it loses too much material by
/// static exchange evaluation (SEE pruning), the same as [`FP_MAX_DEPTH`].
const SEE_PRUNING_MAX_DEPTH: u32 = FP_MAX_DEPTH;

/// The material, in centipawns, SEE pruning lets a noisy move lose per ply of depth before
/// skipping it. Stormphrax, whose threshold has the same shape, uses 111. A larger margin
/// prunes fewer moves.
const SEE_PRUNING_DEPTH_MARGIN: i32 = 90;

/// The divisor of noisy history in the SEE pruning threshold, close to Stormphrax's 65: a
/// noisy move with full history may lose about 2.5 pawns more. A larger divisor weighs
/// history less.
const SEE_PRUNING_HISTORY_DIVISOR: i32 = 64;

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
    /// History, learned from beta cutoffs and kept between searches.
    history: History,
    /// Keys of every position before the current one, from the game and the search.
    keys: KeyHistory,
    /// Keys of the positions before the current one since the search's root.
    tree: KeyHistory,
    /// What the search knows about each node of the current line, by ply. One frame more
    /// than [`MAX_PLY`], since quiescence enters a child at the last ply before stopping.
    stack: [Frame; MAX_PLY + 1],
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
            stack: [Frame::ROOT; MAX_PLY + 1],
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
        self.stack[0] = Frame::ROOT;
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
        let mut previous_score = None;
        for depth in 1..=max_depth {
            if self.interruptible && (self.stop.is_raised() || self.soft_time_is_up()) {
                break;
            }
            let score = self.search_root(board, depth, previous_score);
            if self.stopped {
                break;
            }
            previous_score = Some(score);
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

    /// Searches the root `board` to `depth` plies and returns its exact score, unless the
    /// search stops first.
    ///
    /// From [`ASPIRATION_MIN_DEPTH`] on, the search starts with a narrow window around
    /// `previous`, the score of the iteration before. Each time the score falls outside
    /// the window, the window is widened on that side and the root searched again, until
    /// the score lands inside it.
    fn search_root(&mut self, board: &Board, depth: u32, previous: Option<i32>) -> i32 {
        let mut delta = ASPIRATION_DELTA;
        let mut window = match previous {
            Some(previous) if depth >= ASPIRATION_MIN_DEPTH => Window::aspiration(previous, delta),
            _ => Window::FULL,
        };
        loop {
            let score = self.negamax(board, depth, 0, window, Node::Pv);
            if self.stopped || window.contains(score) {
                return score;
            }
            window = if score <= window.alpha() {
                window.widened_down(score, delta)
            } else {
                window.widened_up(score, delta)
            };
            delta += delta / 2;
        }
    }

    /// Searches `board`, `ply` plies from the root, to `depth` plies and returns its score
    /// within `window`.
    ///
    /// After the first move, each move is searched with a null window just above alpha
    /// (principal variation search). Only at a node on the principal variation, and only
    /// when the score lands inside the window, is the move searched again with the full
    /// window, to find its exact score.
    ///
    /// Off the principal variation, a transposition table entry at least `depth` deep
    /// whose bound settles the window is returned without searching.
    ///
    /// A node whose side to move is in check is searched one ply deeper (check extension),
    /// so that a checking line is not cut off before its point is seen. That includes a
    /// node in check at depth 0, which searches every evasion rather than dropping into
    /// quiescence. Extensions deepen the search but never lengthen a line past the ply
    /// limit just below [`MAX_PLY`], where a node returns its static evaluation.
    ///
    /// Off the principal variation and out of check, a node whose static evaluation is well
    /// above beta may return a cutoff without searching its moves, as
    /// [`Worker::eval_cutoff`] describes.
    ///
    /// A node at least [`IIR_MIN_DEPTH`] deep with no transposition table move is searched
    /// one ply shallower: with no good move to try first, a full-depth search would mostly
    /// be wasted effort.
    ///
    /// **Late moves** after the first few are searched to a reduced depth first, as
    /// [`Worker::late_move_reduction`] describes.
    ///
    /// At shallow nodes off the principal variation and out of check, the remaining quiet
    /// moves are skipped once they look unable to raise alpha or enough moves have been
    /// searched, as [`prunes_quiet`] describes, and a noisy move is skipped when it loses
    /// too much material, as [`prunes_noisy`] describes.
    ///
    /// A quiet move causing a beta cutoff becomes a killer move at this ply, and the
    /// history are updated as [`History::update_on_cutoff`] describes.
    fn negamax(
        &mut self,
        board: &Board,
        mut depth: u32,
        ply: usize,
        mut window: Window,
        node: Node,
    ) -> i32 {
        debug_assert!(ply < MAX_PLY, "ply {ply} is out of bounds");
        self.pv.clear(ply);
        let in_check = board.in_check();
        if in_check {
            depth += 1;
        }
        if depth == 0 {
            return self.quiescence(board, ply, window);
        }
        if self.should_stop() {
            return DRAW;
        }
        self.count_node(ply);
        if ply > 0 && self.is_draw(board, ply) {
            return DRAW;
        }
        if ply >= MAX_PLY - 1 {
            return eval::evaluate(board);
        }

        let tt_entry = self.tt.probe(board.key(), ply);
        if let Some(score) = tt_cutoff(tt_entry, node, depth, window) {
            return score;
        }
        let static_eval = self.record_static_eval(board, ply, in_check, tt_entry);
        if let Some(score) = self.eval_cutoff(board, node, depth, ply, window) {
            return score;
        }
        let tt_move = tt_move(board, tt_entry);
        if tt_move.is_none() && depth >= IIR_MIN_DEPTH {
            depth -= 1;
        }

        let original_window = window;
        let mut best = -INFINITY;
        let mut best_move = None;
        let mut legal_moves = 0;
        let mut tried = Tried::new();
        let mut picker = MovePicker::new(board, tt_move, self.killers.get(ply));
        while let Some(mv) = picker.next_move(&self.history) {
            if prunes_quiet(mv, node, depth, static_eval, best, window, legal_moves) {
                picker.skip_quiets();
                continue;
            }
            let noisy_history = mv
                .is_noisy()
                .then(|| self.history.noisy().get(board, picker.threats(), mv));
            if prunes_noisy(board, mv, node, depth, static_eval, best, noisy_history)
                || !board.is_legal(mv)
            {
                continue;
            }
            legal_moves += 1;
            let child = board.make(mv);
            let order = if legal_moves == 1 {
                Order::First
            } else if !in_check
                && picker.is_handing_out_late_moves()
                && is_reducible(&child, depth, legal_moves)
            {
                let reduction =
                    self.late_move_reduction(board, mv, noisy_history, node, depth, legal_moves);
                Order::Later { reduction }
            } else {
                Order::Later { reduction: 0 }
            };
            self.enter(board, ply, Reached::Move);
            let score = self.search_child(&child, depth - 1, ply + 1, window, node, order);
            self.leave();
            if self.stopped {
                return DRAW;
            }
            if score > best {
                best = score;
                if score > window.alpha() {
                    best_move = Some(mv);
                    self.pv.update(ply, mv);
                    if score >= window.beta() {
                        self.record_cutoff(board, picker.threats(), ply, mv, &tried, depth);
                        break;
                    }
                    window.raise_alpha(score);
                }
            }
            tried.push(mv);
        }
        if legal_moves == 0 {
            return if in_check { score::mated_in(ply) } else { DRAW };
        }
        self.tt.store(
            board.key(),
            ply,
            Entry {
                mv: best_move,
                score: best,
                eval: static_eval,
                depth,
                bound: bound(best, original_window),
            },
        );
        best
    }

    /// Searches `child`, reached by the move at `order` from a node of kind `node` searched
    /// with `window`, to `depth` plies at `ply`, and returns its score from the parent's
    /// point of view.
    ///
    /// The first move is searched with the full window. Later moves get a null window just
    /// above alpha, and only at a node on the principal variation, when that score lands
    /// inside the window, a full-window re-search (principal variation search).
    ///
    /// A move with a reduction is first searched that many plies shallower with the null
    /// window, and searched again at full depth only if it beats alpha there.
    fn search_child(
        &mut self,
        child: &Board,
        depth: u32,
        ply: usize,
        window: Window,
        node: Node,
        order: Order,
    ) -> i32 {
        let reduction = match order {
            Order::First => return -self.negamax(child, depth, ply, window.negated(), node),
            Order::Later { reduction } => reduction,
        };
        debug_assert!(
            reduction < depth.max(1),
            "reduction {reduction} leaves no depth of {depth}"
        );
        let scout = window.null_above_alpha().negated();
        let mut score = -self.negamax(child, depth - reduction, ply, scout, Node::NonPv);
        if reduction > 0 && score > window.alpha() && !self.stopped {
            score = -self.negamax(child, depth, ply, scout, Node::NonPv);
        }
        if node == Node::Pv && window.contains(score) && !self.stopped {
            -self.negamax(child, depth, ply, window.negated(), Node::Pv)
        } else {
            score
        }
    }

    /// The plies by which to reduce the late move `mv` from `board` (late move reduction),
    /// the `move_number`th legal move searched from a node of kind `node` searched to
    /// `depth` plies. `noisy_history` is the move's noisy history, `None` for a quiet
    /// move.
    ///
    /// The amount comes from a table that grows with the logarithms of the depth and move
    /// number, a milder one for noisy moves than for quiet ones, adjusted by the move's
    /// history, as [`reduction::reduction`] describes. The reduction is a ply smaller on
    /// the principal variation and always leaves at least one ply to search. The caller
    /// reduces only late moves that [`is_reducible`] allows, from a node not in check.
    fn late_move_reduction(
        &self,
        board: &Board,
        mv: Move,
        noisy_history: Option<i32>,
        node: Node,
        depth: u32,
        move_number: u32,
    ) -> u32 {
        let late_move = match noisy_history {
            Some(history) => LateMove::Noisy { history },
            None => LateMove::Quiet {
                history: self.history.quiet().get(board.side_to_move(), mv),
            },
        };
        let pv_bonus = u32::from(node == Node::Pv);
        reduction::reduction(late_move, depth, move_number)
            .saturating_sub(pv_bonus)
            .min(depth - 2)
    }

    /// Returns the score to cut off with at `board`, `ply` plies from the root and `depth`
    /// plies deep and searched with `window`, without searching its moves, if its static
    /// evaluation stands far enough above beta: either the static evaluation itself, as
    /// [`reverse_futility_prunes`] describes, or the score of a null-move search, as
    /// [`Worker::null_move_cutoff`] describes. A node in check has no static evaluation
    /// and is never cut off.
    fn eval_cutoff(
        &mut self,
        board: &Board,
        node: Node,
        depth: u32,
        ply: usize,
        window: Window,
    ) -> Option<i32> {
        let eval = self.stack[ply].static_eval?;
        if reverse_futility_prunes(node, depth, eval, window.beta()) {
            return Some(eval);
        }
        self.null_move_cutoff(board, node, depth, ply, window)
    }

    /// Gives the opponent a free move from `board`, `ply` plies from the root, `depth`
    /// plies deep and searched with `window`, and returns the score to cut off with if a
    /// reduced search still fails high (null-move pruning).
    ///
    /// The null move is tried only off the principal variation, out of check, when the
    /// static evaluation is at least beta, beta is not a mate score, and from
    /// [`NMP_MIN_DEPTH`] on. It is not tried
    /// straight after another null move, or when the side to move has only its king and
    /// pawns, where being obliged to move can be a disadvantage (zugzwang) that passing
    /// would hide. An unproven mate score is returned as beta.
    fn null_move_cutoff(
        &mut self,
        board: &Board,
        node: Node,
        depth: u32,
        ply: usize,
        window: Window,
    ) -> Option<i32> {
        let beta = window.beta();
        let frame = self.stack[ply];
        if node == Node::Pv
            || frame.static_eval.is_none_or(|eval| eval < beta)
            || score::is_mate(beta)
            || depth < NMP_MIN_DEPTH
            || frame.null_ply == Some(ply)
            || !board.has_non_pawn_material(board.side_to_move())
        {
            return None;
        }
        let reduction = NMP_BASE_REDUCTION + depth / NMP_DEPTH_DIVISOR;
        let child = board.pass();
        self.enter(board, ply, Reached::NullMove);
        let score = -self.negamax(
            &child,
            depth.saturating_sub(reduction + 1),
            ply + 1,
            window.null_below_beta().negated(),
            Node::NonPv,
        );
        self.leave();
        if self.stopped || score < beta {
            return None;
        }
        Some(if score::is_mate(score) { beta } else { score })
    }

    /// Searches only noisy moves from `board`, `ply` plies from the root, until the
    /// position is quiet, and returns its score within `window`.
    ///
    /// The side to move may stand pat on the static evaluation instead of capturing, and
    /// noisy moves that lose material by static exchange evaluation are skipped, the TT
    /// move included. In check it may not stand pat, and every evasion is searched so that
    /// mates are seen.
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
    fn quiescence(&mut self, board: &Board, ply: usize, mut window: Window) -> i32 {
        if self.should_stop() {
            return DRAW;
        }
        self.count_node(ply);
        if self.is_draw(board, ply) {
            return DRAW;
        }
        if ply >= MAX_PLY {
            return eval::evaluate(board);
        }

        let tt_entry = self.tt.probe(board.key(), ply);
        if let Some(entry) = tt_entry
            && entry.cuts_off(window)
        {
            return entry.score;
        }

        let original_window = window;
        let in_check = board.in_check();
        let mut best = -INFINITY;
        let mut eval = None;
        let tt_move = tt_move(board, tt_entry);
        let mut moves = if in_check {
            MovePicker::new(board, tt_move, [None; 2])
        } else {
            let static_eval = static_eval(board, tt_entry);
            eval = Some(static_eval);
            best = static_eval;
            if best >= window.beta() {
                self.tt.store(
                    board.key(),
                    ply,
                    Entry {
                        mv: None,
                        score: best,
                        eval,
                        depth: 0,
                        bound: bound(best, original_window),
                    },
                );
                return best;
            }
            window.raise_alpha(best);
            MovePicker::good_noisy(board, tt_move)
        };

        let mut best_move = None;
        let mut legal_moves = 0;
        while let Some(mv) = moves.next_move(&self.history) {
            if !board.is_legal(mv) {
                continue;
            }
            legal_moves += 1;
            let child = board.make(mv);
            self.enter(board, ply, Reached::Move);
            let score = -self.quiescence(&child, ply + 1, window.negated());
            self.leave();
            if self.stopped {
                return DRAW;
            }
            if score > best {
                best = score;
                if score > window.alpha() {
                    best_move = Some(mv);
                    if score >= window.beta() {
                        break;
                    }
                    window.raise_alpha(score);
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
                bound: bound(best, original_window),
            },
        );
        best
    }

    /// Records that `mv` caused a beta cutoff in `board`, whose opponent attacks `threats`,
    /// `ply` plies from the root, after the moves in `tried` failed to, `depth` plies deep.
    /// A quiet move that cuts off also becomes a killer move at `ply`.
    fn record_cutoff(
        &mut self,
        board: &Board,
        threats: Bitboard,
        ply: usize,
        mv: Move,
        tried: &Tried,
        depth: u32,
    ) {
        if !mv.is_noisy() {
            self.killers.update(ply, mv);
        }
        self.history
            .update_on_cutoff(board, threats, mv, &tried.quiets, &tried.noisies, depth);
    }

    /// Records the static evaluation of `board`, `ply` plies from the root, on the stack and
    /// returns it, reading it from its transposition table `entry` when that holds one. A
    /// node `in_check` is not evaluated and records `None`.
    fn record_static_eval(
        &mut self,
        board: &Board,
        ply: usize,
        in_check: bool,
        entry: Option<Entry>,
    ) -> Option<i32> {
        let eval = (!in_check).then(|| static_eval(board, entry));
        self.stack[ply].static_eval = eval;
        eval
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

    /// Records that the search is moving on from `board`, `ply` plies from the root, to
    /// the child it `reached` by a move or a null move.
    fn enter(&mut self, board: &Board, ply: usize, reached: Reached) {
        self.keys.push(board.key());
        self.tree.push(board.key());
        self.stack[ply + 1].null_ply = match reached {
            Reached::Move => self.stack[ply].null_ply,
            Reached::NullMove => Some(ply + 1),
        };
    }

    /// Undoes the matching [`Worker::enter`] on returning from a child.
    fn leave(&mut self) {
        self.keys.pop();
        self.tree.pop();
    }

    /// Reports whether `board`, `ply` plies from the root, is drawn by repetition, the
    /// fifty-move rule or insufficient material.
    ///
    /// A position that already occurred since the root counts as a repetition: if
    /// repeating is best now, it stays best when the position comes round again. Earlier
    /// positions in the game count only once they make a threefold repetition. Positions
    /// from before the line's last null move are never compared: the null move is not a
    /// real move, so a position before it cannot recur after it.
    fn is_draw(&self, board: &Board, ply: usize) -> bool {
        // With no null move on the line, only the halfmove clock limits the comparison.
        let lookback = self.stack[ply]
            .null_ply
            .map_or(usize::MAX, |null| ply - null);
        self.tree.repetitions_within(board, lookback) > 0
            || self.keys.repetitions_within(board, lookback) >= 2
            || board.is_fifty_move_draw()
            || board.has_insufficient_material()
    }
}

/// The moves searched so far at a node without causing a beta cutoff, quiet and noisy
/// apart.
struct Tried {
    quiets: MoveList,
    noisies: MoveList,
}

impl Tried {
    fn new() -> Self {
        Self {
            quiets: MoveList::new(),
            noisies: MoveList::new(),
        }
    }

    /// Adds `mv` to the quiet or noisy moves, whichever it is.
    fn push(&mut self, mv: Move) {
        if mv.is_noisy() {
            self.noisies.push(mv);
        } else {
            self.quiets.push(mv);
        }
    }
}

/// Reports whether a node of kind `node`, `depth` plies deep with static evaluation `eval`,
/// stands so far above `beta` that it may return `eval` without searching (reverse
/// futility pruning).
///
/// Only nodes off the principal variation, out of check (which the caller rules out by
/// having a static evaluation) and at most [`RFP_MAX_DEPTH`] deep are pruned, and never
/// when `beta` is a mate score, which a static evaluation cannot prove.
fn reverse_futility_prunes(node: Node, depth: u32, eval: i32, beta: i32) -> bool {
    node == Node::NonPv
        && depth <= RFP_MAX_DEPTH
        && !score::is_mate(beta)
        && eval - RFP_MARGIN * depth as i32 >= beta
}

/// Reports whether a late move that reached `child` as the `move_number`th legal move
/// searched, from a node `depth` plies deep not in check, may be reduced (late move
/// reduction): only from [`LMR_MIN_DEPTH`] on, after the first [`LMR_FULL_DEPTH_MOVES`],
/// and when it does not give check. The attack test for check comes last, as the costliest.
fn is_reducible(child: &Board, depth: u32, move_number: u32) -> bool {
    depth >= LMR_MIN_DEPTH && move_number > LMR_FULL_DEPTH_MOVES && !child.in_check()
}

/// Returns the static evaluation `static_eval` of a node of kind `node` if moves may be
/// pruned there at all, given the best score `best` so far.
///
/// Moves are pruned only off the principal variation and out of check (where `static_eval`
/// is `None`), and only once a move has been searched with a score that is not a mate, so
/// that a node never skips every move and is mistaken for checkmate or stalemate, and a
/// mate found below it is never hidden. Before any move is searched, `best` is
/// `-INFINITY`, which counts as a mate score.
fn pruning_eval(node: Node, static_eval: Option<i32>, best: i32) -> Option<i32> {
    static_eval.filter(|_| node == Node::NonPv && !score::is_mate(best))
}

/// Reports whether `mv` is a quiet move that, with the quiet moves after it, may be
/// skipped at a node of kind `node`, `depth` plies deep with static evaluation
/// `static_eval`, given the best score `best` so far, the node's `window` and the
/// `legal_moves` already searched.
///
/// Quiet moves are skipped only where [`pruning_eval`] allows pruning. There, they are
/// skipped when the static evaluation plus a margin growing with depth cannot reach alpha
/// (futility pruning), or once enough moves for the depth have been searched (late move
/// pruning).
fn prunes_quiet(
    mv: Move,
    node: Node,
    depth: u32,
    static_eval: Option<i32>,
    best: i32,
    window: Window,
    legal_moves: u32,
) -> bool {
    if mv.is_noisy() {
        return false;
    }
    let Some(eval) = pruning_eval(node, static_eval, best) else {
        return false;
    };
    let futile = depth <= FP_MAX_DEPTH
        && eval + FP_BASE_MARGIN + FP_DEPTH_MARGIN * depth as i32 <= window.alpha();
    let late = depth <= LMP_MAX_DEPTH && legal_moves >= LMP_BASE_MOVES + depth * depth / 2;
    futile || late
}

/// Reports whether `mv`, a noisy move on `board`, may be skipped at a node of kind `node`,
/// `depth` plies deep with static evaluation `static_eval`, given the best score `best` so
/// far (SEE pruning).
///
/// Noisy moves are skipped only where [`pruning_eval`] allows pruning, at most
/// [`SEE_PRUNING_MAX_DEPTH`] deep, when static exchange evaluation falls below the
/// [`see_pruning_threshold`] for the depth and the move's noisy history `noisy_history`,
/// which is `None` for a quiet move, never skipped here.
fn prunes_noisy(
    board: &Board,
    mv: Move,
    node: Node,
    depth: u32,
    static_eval: Option<i32>,
    best: i32,
    noisy_history: Option<i32>,
) -> bool {
    noisy_history.is_some_and(|history| {
        depth <= SEE_PRUNING_MAX_DEPTH
            && pruning_eval(node, static_eval, best).is_some()
            && !board.see_ge(mv, see_pruning_threshold(depth, history))
    })
}

/// The material, in centipawns, below which static exchange evaluation lets SEE pruning
/// skip a noisy move `depth` plies deep with noisy history `noisy_history`.
///
/// The threshold falls with depth and with good history, and never rises above zero, so a
/// noisy move that does not lose material is never skipped however bad its history.
fn see_pruning_threshold(depth: u32, noisy_history: i32) -> i32 {
    (-SEE_PRUNING_DEPTH_MARGIN * depth as i32 - noisy_history / SEE_PRUNING_HISTORY_DIVISOR).min(0)
}

/// Whether a node lies on the principal variation: the root, the first move searched from
/// each node that does, and any later move re-searched there with the full window. Only
/// nodes off it take transposition table cutoffs, so that the principal variation is
/// searched rather than read back from the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Pv,
    NonPv,
}

/// The score of the transposition table `entry` if it settles `window` for a node of kind
/// `node` searched to `depth` plies: only off the principal variation, and only from an
/// entry searched at least as deep.
fn tt_cutoff(entry: Option<Entry>, node: Node, depth: u32, window: Window) -> Option<i32> {
    entry
        .filter(|entry| node == Node::NonPv && entry.depth >= depth && entry.cuts_off(window))
        .map(|entry| entry.score)
}

/// The static evaluation of `board`, read from its transposition table `entry` when that
/// holds one.
fn static_eval(board: &Board, entry: Option<Entry>) -> i32 {
    entry
        .and_then(|entry| entry.eval)
        .unwrap_or_else(|| eval::evaluate(board))
}

/// What the search knows about one node of the current line.
#[derive(Clone, Copy, Debug)]
struct Frame {
    /// The node's static evaluation, set by the main search: `None` in check, where the
    /// search does not evaluate.
    static_eval: Option<i32>,
    /// The ply of the node reached by the line's last null move, if there is one.
    null_ply: Option<usize>,
}

impl Frame {
    /// The frame of the search's root, which no null move precedes.
    const ROOT: Self = Self {
        static_eval: None,
        null_ply: None,
    };
}

/// Where a move falls in its node's move order, as the child search needs to know it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    /// The first legal move, searched with the full window and never reduced.
    First,
    /// A later legal move, searched with a null window after a `reduction` of that many
    /// plies (late move reduction), which may be none.
    Later { reduction: u32 },
}

/// How the search reached a node from its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reached {
    Move,
    NullMove,
}

/// The move from `entry` to search first in `board`, if it has one that is pseudo-legal
/// there. A move stored under the same key may still belong to another position.
fn tt_move(board: &Board, entry: Option<Entry>) -> Option<Move> {
    entry
        .and_then(|entry| entry.mv)
        .filter(|&mv| board.is_pseudo_legal(mv))
}

/// The bound of a fail-soft search of `window` that returned `best`.
fn bound(best: i32, window: Window) -> Bound {
    if best >= window.beta() {
        Bound::Lower
    } else if best > window.alpha() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn see_pruning_threshold_falls_with_depth_and_good_noisy_history() {
        assert_eq!(see_pruning_threshold(1, 0), -90);
        assert_eq!(see_pruning_threshold(4, 0), -360);
        assert_eq!(see_pruning_threshold(2, 6400), -280);
        assert_eq!(see_pruning_threshold(3, -16384), -14);
    }

    #[test]
    fn see_pruning_threshold_never_rises_above_zero() {
        assert_eq!(see_pruning_threshold(1, -16384), 0);
        assert_eq!(see_pruning_threshold(2, -16384), 0);
    }
}
