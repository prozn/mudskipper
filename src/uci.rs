//! The Universal Chess Interface protocol layer.

mod go;
mod options;

use std::collections::VecDeque;
use std::fmt::{self, Write as _};
use std::io::{self, BufRead, Write};
use std::ops::ControlFlow;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::chess::{Board, CastlingNotation, Game, ParseFenError, ParseMoveError, Piece, Square};
use crate::eval;
use crate::search::{Report, Score, StopFlag, Worker};

use go::{Go, SearchPlan, parse_go};
use options::Options;

/// Engine name reported in reply to `uci`.
const NAME: &str = concat!("Mudskipper ", env!("CARGO_PKG_VERSION"));

/// Author reported in reply to `uci`.
const AUTHOR: &str = "Prozn";

/// Whether the `UCI_Chess960` option is on, which decides how castling moves and FENs are
/// written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// `UCI_Chess960` off: castling is the king's move, such as `e1g1`, and FENs are X-FEN.
    Standard,
    /// `UCI_Chess960` on: castling is king takes rook, such as `e1h1`, and FENs are
    /// Shredder-FEN.
    Chess960,
}

impl Mode {
    /// How castling moves are read and written in this mode.
    const fn castling_notation(self) -> CastlingNotation {
        match self {
            Self::Standard => CastlingNotation::Standard,
            Self::Chess960 => CastlingNotation::KingTakesRook,
        }
    }

    /// The FEN of `board` as this mode writes it.
    fn fen(self, board: &Board) -> String {
        match self {
            Self::Standard => board.to_fen(),
            Self::Chess960 => board.to_shredder_fen(),
        }
    }
}

/// Runs the UCI loop, reading commands from `input` and writing replies to `output`.
///
/// The engine starts in the standard start position with `UCI_Chess960` off. `input` is
/// read on its own thread and searches run on another, so that `stop` and `isready` are
/// answered during a search. Other commands that arrive during a search wait until it
/// ends. The loop ends on `quit`, which stops any search first, or at the end of input,
/// once the current search has finished; an infinite search is stopped. Lines the loop
/// cannot act on are answered with an `info string` and otherwise ignored.
///
/// # Errors
///
/// Returns any I/O error from reading `input`, writing `output` or starting a thread.
pub fn run(input: impl BufRead + Send + 'static, mut output: impl Write) -> io::Result<()> {
    let (events, received) = mpsc::channel();
    spawn_input_thread(input, events.clone())?;
    let mut engine = Engine::new(events);
    for event in received {
        if engine.handle(event, &mut output)?.is_break() {
            break;
        }
        output.flush()?;
    }
    output.flush()
}

/// Something the UCI loop reacts to.
#[derive(Debug)]
enum Event {
    /// A line read from input, including its line ending and not yet decoded.
    Line(Vec<u8>),
    /// Input ended, normally or with an error.
    InputEnd(io::Result<()>),
    /// A line of search output, such as an `info` line.
    SearchOutput(String),
    /// The search finished with this `bestmove` line, to write once the search may end.
    SearchDone(String),
}

/// Reads `input` line by line on a new thread, sending each line to `events`.
fn spawn_input_thread(
    mut input: impl BufRead + Send + 'static,
    events: Sender<Event>,
) -> io::Result<()> {
    thread::Builder::new()
        .name("input".to_owned())
        .spawn(move || {
            loop {
                let mut line = Vec::new();
                let event = match input.read_until(b'\n', &mut line) {
                    Ok(0) => Event::InputEnd(Ok(())),
                    Ok(_) => Event::Line(line),
                    Err(err) => Event::InputEnd(Err(err)),
                };
                let last = matches!(event, Event::InputEnd(_));
                // A send fails only once the loop has ended, and then nobody is listening.
                if events.send(event).is_err() || last {
                    break;
                }
            }
        })?;
    Ok(())
}

/// A search running on its own thread.
#[derive(Debug)]
struct Search {
    thread: JoinHandle<()>,
    /// Whether the `bestmove` must wait for `stop`: set by `go infinite`, cleared by
    /// `stop`, `quit` or the end of input.
    wait_for_stop: bool,
    /// The search's `bestmove` line, once it finished, while it waits for `stop`.
    bestmove: Option<String>,
}

/// The state of the UCI loop.
#[derive(Debug)]
struct Engine {
    game: Game,
    options: Options,
    /// The worker that runs every search. The search thread holds its lock for the whole
    /// search, reporting under it, and the loop touches it only when no search is running,
    /// so the lock is never contended. It lets the worker stay put rather than travel to
    /// the search thread and back.
    worker: Arc<Mutex<Worker>>,
    /// The running search, if any.
    search: Option<Search>,
    stop: StopFlag,
    /// Where the search thread sends its output.
    events: Sender<Event>,
    /// Commands received during the search, to run once it ends.
    deferred: VecDeque<Vec<u8>>,
    shutdown: Shutdown,
}

/// Whether and how the UCI loop is winding down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shutdown {
    /// Running normally.
    No,
    /// Input ended: exit once the search and any commands still waiting are done.
    EndOfInput,
    /// `quit` was received: exit once the search stops, dropping commands still waiting.
    Quit,
}

impl Engine {
    /// Returns an engine in the start position, idle, sending search output to `events`.
    fn new(events: Sender<Event>) -> Self {
        let stop = StopFlag::new();
        Self {
            game: Game::startpos(),
            options: Options::new(),
            worker: Arc::new(Mutex::new(Worker::new(stop.clone()))),
            search: None,
            stop,
            events,
            deferred: VecDeque::new(),
            shutdown: Shutdown::No,
        }
    }

    /// Reacts to `event`, and says whether the loop should end.
    fn handle(&mut self, event: Event, output: &mut impl Write) -> io::Result<ControlFlow<()>> {
        match event {
            Event::Line(line) => match &mut self.search {
                None => self.execute(&line, output)?,
                Some(_) => match first_word(&line) {
                    Some("stop") => self.stop_search(),
                    Some("isready") => writeln!(output, "readyok")?,
                    Some("quit") => {
                        self.stop_search();
                        self.shutdown = Shutdown::Quit;
                    }
                    _ => self.deferred.push_back(line),
                },
            },
            Event::InputEnd(result) => {
                result?;
                if self.shutdown == Shutdown::No {
                    self.shutdown = Shutdown::EndOfInput;
                }
                if self
                    .search
                    .as_ref()
                    .is_some_and(|search| search.wait_for_stop)
                {
                    self.stop_search();
                }
            }
            Event::SearchOutput(line) => writeln!(output, "{line}")?,
            Event::SearchDone(bestmove) => {
                if let Some(search) = &mut self.search {
                    search.bestmove = Some(bestmove);
                }
            }
        }
        self.finish_search(output)?;
        while self.search.is_none()
            && self.shutdown != Shutdown::Quit
            && let Some(line) = self.deferred.pop_front()
        {
            self.execute(&line, output)?;
            self.finish_search(output)?;
        }
        Ok(if self.shutdown != Shutdown::No && self.search.is_none() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    }

    /// Tells the running search, if any, to stop and to write its `bestmove`.
    fn stop_search(&mut self) {
        if let Some(search) = &mut self.search {
            search.wait_for_stop = false;
            self.stop.raise();
        }
    }

    /// Ends the running search if it has finished and may end, writing its `bestmove`.
    fn finish_search(&mut self, output: &mut impl Write) -> io::Result<()> {
        let Some(search) = &mut self.search else {
            return Ok(());
        };
        if search.wait_for_stop {
            return Ok(());
        }
        let Some(bestmove) = search.bestmove.take() else {
            return Ok(());
        };
        if let Some(search) = self.search.take() {
            // The thread has sent its last event, so it ends at once and cannot have
            // panicked.
            let _ = search.thread.join();
        }
        writeln!(output, "{bestmove}")
    }

    /// Carries out the command on `line` while no search is running.
    fn execute(&mut self, line: &[u8], output: &mut impl Write) -> io::Result<()> {
        let Ok(line) = str::from_utf8(line) else {
            return writeln!(output, "info string ignoring line that is not valid UTF-8");
        };
        let notation = self.options.mode.castling_notation();
        match first_word(line.as_bytes()) {
            // A `stop` with no search to stop needs no reply.
            None | Some("stop") => {}
            Some("uci") => {
                writeln!(output, "id name {NAME}")?;
                writeln!(output, "id author {AUTHOR}")?;
                Options::write_uci(output)?;
                writeln!(output, "uciok")?;
            }
            Some("isready") => writeln!(output, "readyok")?,
            Some("ucinewgame") => {
                // A fresh worker forgets whatever earlier searches taught it.
                self.game = Game::startpos();
                *self.worker.lock().expect("worker lock poisoned") = Worker::new(self.stop.clone());
            }
            Some("setoption") => {
                if let Err(err) = self.options.set(line) {
                    writeln!(output, "info string invalid setoption: {err}")?;
                }
            }
            Some("position") => match parse_position(line, notation) {
                Ok(game) => self.game = game,
                Err(err) => writeln!(output, "info string invalid position: {err}")?,
            },
            Some("go") => match parse_go(line) {
                Ok(Go::Perft(depth)) => {
                    write_split_perft(output, self.game.board(), notation, depth)?;
                }
                Ok(Go::Search { request, ignored }) => {
                    for err in ignored {
                        writeln!(output, "info string ignoring go parameter: {err}")?;
                    }
                    let side = self.game.board().side_to_move();
                    self.start_search(request.plan(side, self.options.move_overhead))?;
                }
                Err(err) => writeln!(output, "info string invalid go: {err}")?,
            },
            Some("d") => write_board(output, self.game.board(), self.options.mode)?,
            Some("eval") => writeln!(
                output,
                "Evaluation: {} (white side)",
                eval::evaluate_white(self.game.board())
            )?,
            Some("quit") => self.shutdown = Shutdown::Quit,
            Some(_) => writeln!(output, "info string unknown command: {}", line.trim())?,
        }
        Ok(())
    }

    /// Starts searching the current position on a new thread as `plan` says. An infinite
    /// search holds its `bestmove` until `stop`, unless input has already ended and no
    /// `stop` can come: then it is stopped at once, and reports after its first completed
    /// iteration.
    fn start_search(&mut self, plan: SearchPlan) -> io::Result<()> {
        let SearchPlan { limits, infinite } = plan;
        // Once input has ended, no `stop` can come to end an infinite search.
        let input_ended = self.shutdown != Shutdown::No;
        if infinite && input_ended {
            self.stop.raise();
        } else {
            self.stop.lower();
        }
        let wait_for_stop = infinite && !input_ended;
        let worker = Arc::clone(&self.worker);
        let game = self.game.clone();
        let notation = self.options.mode.castling_notation();
        let events = self.events.clone();
        let thread = thread::Builder::new()
            .name("search".to_owned())
            .spawn(move || {
                let mut worker = worker.lock().expect("worker lock poisoned");
                // A send fails only once the loop has ended, and then nobody is listening.
                let best = worker.search(&game, limits, |report| {
                    let _ = events.send(Event::SearchOutput(info_line(report, notation)));
                });
                drop(worker);
                let bestmove = match best {
                    Some(mv) => format!("bestmove {}", mv.display(notation)),
                    None => "bestmove 0000".to_owned(),
                };
                let _ = events.send(Event::SearchDone(bestmove));
            })?;
        self.search = Some(Search {
            thread,
            wait_for_stop,
            bestmove: None,
        });
        Ok(())
    }
}

/// The first word of `line`, if it is valid UTF-8 and not blank.
fn first_word(line: &[u8]) -> Option<&str> {
    str::from_utf8(line).ok()?.split_whitespace().next()
}

/// Parses a `position` command into the game it sets up, reading castling moves as
/// `notation` says.
fn parse_position(line: &str, notation: CastlingNotation) -> Result<Game, PositionError> {
    let args: Vec<&str> = line.split_whitespace().skip(1).collect();
    let (setup, moves) = match args.iter().position(|&arg| arg == "moves") {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (&args[..], &[][..]),
    };
    let board = match setup {
        [] => return Err(PositionError::MissingSetup),
        ["startpos"] => Board::startpos(),
        ["startpos", extra @ ..] => return Err(PositionError::Trailing(extra.join(" "))),
        ["fen", fen @ ..] => Board::from_fen(&fen.join(" ")).map_err(PositionError::Fen)?,
        [other, ..] => return Err(PositionError::UnknownSetup((*other).to_owned())),
    };
    let mut game = Game::new(board);
    for text in moves {
        let mv = game
            .board()
            .parse_move(text, notation)
            .map_err(PositionError::Move)?;
        game.play(mv);
    }
    Ok(game)
}

/// Formats a search report as an `info` line.
fn info_line(report: &Report<'_>, notation: CastlingNotation) -> String {
    let micros = report.elapsed.as_micros().max(1);
    let nps = u128::from(report.nodes) * 1_000_000 / micros;
    let score = match report.score {
        Score::Centipawns(cp) => format!("cp {cp}"),
        Score::Mate(moves) => format!("mate {moves}"),
    };
    let mut line = format!(
        "info depth {} seldepth {} score {score} nodes {} nps {nps} time {} hashfull 0 pv",
        report.depth,
        report.seldepth,
        report.nodes,
        report.elapsed.as_millis(),
    );
    for mv in report.pv {
        write!(line, " {}", mv.display(notation)).expect("writing to a String cannot fail");
    }
    line
}

/// Writes the reply to `go perft`: the count below each legal move, then the total.
fn write_split_perft(
    output: &mut impl Write,
    board: &Board,
    notation: CastlingNotation,
    depth: u32,
) -> io::Result<()> {
    let split = board.split_perft(depth);
    for (mv, nodes) in &split {
        writeln!(output, "{}: {nodes}", mv.display(notation))?;
    }
    // With no moves to sum over (depth 0, or no legal moves), perft is cheap.
    let total = if split.is_empty() {
        board.perft(depth)
    } else {
        split.iter().map(|&(_, nodes)| nodes).sum()
    };
    writeln!(output)?;
    writeln!(output, "Nodes searched: {total}")
}

/// Why a `position` command was rejected.
#[derive(Debug)]
enum PositionError {
    /// Neither `startpos` nor `fen` was given.
    MissingSetup,
    /// The setup was something other than `startpos` or `fen`.
    UnknownSetup(String),
    /// Unexpected arguments followed `startpos`.
    Trailing(String),
    /// The FEN was malformed.
    Fen(ParseFenError),
    /// A move in the move list was malformed or illegal.
    Move(ParseMoveError),
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSetup => f.write_str("expected startpos or fen"),
            Self::UnknownSetup(setup) => write!(f, "expected startpos or fen, got {setup:?}"),
            Self::Trailing(extra) => write!(f, "unexpected {extra:?} after startpos"),
            Self::Fen(err) => write!(f, "{err}"),
            Self::Move(err) => write!(f, "{err}"),
        }
    }
}

/// Writes the reply to `d`: a diagram of the board, its FEN and its Zobrist key.
///
/// With `UCI_Chess960` on, the FEN is Shredder-FEN.
fn write_board(output: &mut impl Write, board: &Board, mode: Mode) -> io::Result<()> {
    const BORDER: &str = " +---+---+---+---+---+---+---+---+";
    writeln!(output, "{BORDER}")?;
    for (rank, squares) in Square::ALL.chunks(8).enumerate().rev() {
        let mut row = String::with_capacity(BORDER.len());
        for &square in squares {
            row.push_str(" | ");
            row.push(board.piece_at(square).map_or(' ', Piece::to_char));
        }
        writeln!(output, "{row} | {}", rank + 1)?;
        writeln!(output, "{BORDER}")?;
    }
    writeln!(output, "   a   b   c   d   e   f   g   h")?;
    writeln!(output)?;
    writeln!(output, "Fen: {}", mode.fen(board))?;
    writeln!(output, "Key: {:016X}", board.key())
}
