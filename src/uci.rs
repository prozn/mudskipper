//! The Universal Chess Interface protocol layer.

use std::fmt;
use std::io::{self, BufRead, Write};

use crate::chess::{
    Board, CastlingNotation, KeyHistory, ParseFenError, ParseMoveError, Piece, Square,
};
use crate::eval;
use crate::search::{Limits, Report, Score, Worker};

/// Engine name reported in reply to `uci`.
const NAME: &str = concat!("Mudskipper ", env!("CARGO_PKG_VERSION"));

/// Author reported in reply to `uci`.
const AUTHOR: &str = "Prozn";

/// Name of the check option that switches Chess960 mode on and off.
const CHESS960_OPTION: &str = "UCI_Chess960";

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
/// The engine starts in the standard start position with `UCI_Chess960` off. The loop
/// ends on `quit` or end of input. Lines it cannot act on are answered with an
/// `info string` and otherwise ignored.
///
/// # Errors
///
/// Returns any I/O error from reading `input` or writing `output`.
pub fn run(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut board = Board::startpos();
    let mut history = KeyHistory::new();
    let mut worker = Worker::new();
    let mut mode = Mode::Standard;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if input.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        let Ok(line) = str::from_utf8(&buf) else {
            writeln!(output, "info string ignoring line that is not valid UTF-8")?;
            output.flush()?;
            continue;
        };
        match line.split_whitespace().next() {
            None => {}
            Some("uci") => {
                writeln!(output, "id name {NAME}")?;
                writeln!(output, "id author {AUTHOR}")?;
                writeln!(
                    output,
                    "option name {CHESS960_OPTION} type check default false"
                )?;
                writeln!(output, "uciok")?;
            }
            Some("isready") => writeln!(output, "readyok")?,
            Some("setoption") => match parse_setoption(line) {
                Ok(new_mode) => mode = new_mode,
                Err(err) => writeln!(output, "info string invalid setoption: {err}")?,
            },
            Some("position") => match parse_position(line, mode.castling_notation()) {
                Ok((new_board, new_history)) => (board, history) = (new_board, new_history),
                Err(err) => writeln!(output, "info string invalid position: {err}")?,
            },
            Some("go") => match parse_go(line) {
                Ok(Go::Perft(depth)) => {
                    write_split_perft(&mut output, &board, mode.castling_notation(), depth)?;
                }
                Ok(Go::Search(limits)) => go(
                    &mut output,
                    &mut worker,
                    &board,
                    &history,
                    limits,
                    mode.castling_notation(),
                )?,
                Err(err) => writeln!(output, "info string invalid go: {err}")?,
            },
            Some("d") => write_board(&mut output, &board, mode)?,
            Some("eval") => writeln!(
                output,
                "Evaluation: {} (white side)",
                eval::evaluate_white(&board)
            )?,
            Some("quit") => break,
            Some(_) => writeln!(output, "info string unknown command: {}", line.trim())?,
        }
        output.flush()?;
    }
    Ok(())
}

/// Parses a `position` command into the board it sets up and the keys of the positions
/// played before it, reading castling moves as `notation` says.
fn parse_position(
    line: &str,
    notation: CastlingNotation,
) -> Result<(Board, KeyHistory), PositionError> {
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
    let mut history = KeyHistory::new();
    let board = moves.iter().try_fold(board, |board, text| {
        let mv = board
            .parse_move(text, notation)
            .map_err(PositionError::Move)?;
        history.push(board.key());
        Ok(board.make(mv))
    })?;
    Ok((board, history))
}

/// What a `go` command asks for.
#[derive(Debug)]
enum Go {
    /// `go perft <depth>`: count the leaf nodes below each legal move.
    Perft(u32),
    /// Search for the best move within these limits.
    Search(Limits),
}

/// Parses a `go` command.
///
/// Only `go perft <depth>` and searches limited by `depth` and `nodes` are supported so
/// far.
fn parse_go(line: &str) -> Result<Go, GoError> {
    let args: Vec<&str> = line.split_whitespace().skip(1).collect();
    if let ["perft", rest @ ..] = args.as_slice() {
        return match rest {
            [depth] => depth
                .parse()
                .map(Go::Perft)
                .map_err(|_| GoError::InvalidValue("perft depth", (*depth).to_owned())),
            _ => Err(GoError::MalformedPerft),
        };
    }
    let mut limits = Limits::default();
    let mut args = args.into_iter();
    while let Some(name) = args.next() {
        let value = args.next();
        let parse = |field| {
            value
                .and_then(|value| value.parse().ok())
                .ok_or_else(|| GoError::InvalidValue(field, value.unwrap_or_default().to_owned()))
        };
        match name {
            "depth" => limits.depth = Some(parse("depth")?),
            "nodes" => limits.nodes = Some(parse("nodes")?.into()),
            other => return Err(GoError::Unsupported(other.to_owned())),
        }
    }
    if limits == Limits::default() {
        return Err(GoError::MissingLimit);
    }
    Ok(Go::Search(limits))
}

/// Why a `go` command was rejected.
#[derive(Debug)]
enum GoError {
    /// `perft` was not followed by exactly one depth.
    MalformedPerft,
    /// A parameter's value was missing or not a valid count.
    InvalidValue(&'static str, String),
    /// The parameter is not supported yet.
    Unsupported(String),
    /// No limit was given, and the search could not be stopped.
    MissingLimit,
}

impl fmt::Display for GoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedPerft => f.write_str("expected go perft <depth>"),
            Self::InvalidValue(field, value) => write!(f, "invalid {field} {value:?}"),
            Self::Unsupported(name) => write!(f, "unsupported parameter {name:?}"),
            Self::MissingLimit => f.write_str("expected depth or nodes"),
        }
    }
}

/// Searches `board`, reached after the positions in `history`, writing an `info` line
/// for each report and then the `bestmove`.
fn go(
    output: &mut impl Write,
    worker: &mut Worker,
    board: &Board,
    history: &KeyHistory,
    limits: Limits,
    notation: CastlingNotation,
) -> io::Result<()> {
    let mut result = Ok(());
    let best = worker.search(board, history, limits, |report| {
        if result.is_ok() {
            result = write_info(output, report, notation);
        }
    });
    result?;
    match best {
        Some(mv) => writeln!(output, "bestmove {}", mv.display(notation)),
        None => writeln!(output, "bestmove 0000"),
    }
}

/// Writes a search report as an `info` line.
fn write_info(
    output: &mut impl Write,
    report: &Report<'_>,
    notation: CastlingNotation,
) -> io::Result<()> {
    let micros = report.elapsed.as_micros().max(1);
    let nps = u128::from(report.nodes) * 1_000_000 / micros;
    let score = match report.score {
        Score::Centipawns(cp) => format!("cp {cp}"),
        Score::Mate(moves) => format!("mate {moves}"),
    };
    write!(
        output,
        "info depth {} seldepth {} score {score} nodes {} nps {nps} time {} hashfull 0 pv",
        report.depth,
        report.seldepth,
        report.nodes,
        report.elapsed.as_millis(),
    )?;
    for mv in report.pv {
        write!(output, " {}", mv.display(notation))?;
    }
    writeln!(output)
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

/// Parses a `setoption` command into the mode it selects.
///
/// `UCI_Chess960` is the only option so far. Option names are matched ignoring case, as
/// the UCI protocol asks.
fn parse_setoption(line: &str) -> Result<Mode, SetOptionError> {
    let args: Vec<&str> = line.split_whitespace().skip(1).collect();
    let Some((&"name", rest)) = args.split_first() else {
        return Err(SetOptionError::MissingName);
    };
    let (name, value) = match rest.iter().position(|&arg| arg == "value") {
        Some(i) => (rest[..i].join(" "), Some(rest[i + 1..].join(" "))),
        None => (rest.join(" "), None),
    };
    if !name.eq_ignore_ascii_case(CHESS960_OPTION) {
        return Err(SetOptionError::UnknownOption(name));
    }
    match value.as_deref() {
        Some("true") => Ok(Mode::Chess960),
        Some("false") => Ok(Mode::Standard),
        Some(value) => Err(SetOptionError::InvalidValue(value.to_owned())),
        None => Err(SetOptionError::MissingValue),
    }
}

/// Why a `setoption` command was rejected.
#[derive(Debug)]
enum SetOptionError {
    /// No `name` followed `setoption`.
    MissingName,
    /// The engine has no option of this name.
    UnknownOption(String),
    /// The option was given no value.
    MissingValue,
    /// The value is not valid for the option.
    InvalidValue(String),
}

impl fmt::Display for SetOptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingName => f.write_str("expected setoption name <name> value <value>"),
            Self::UnknownOption(name) => write!(f, "unknown option {name:?}"),
            Self::MissingValue => f.write_str("expected value true or false"),
            Self::InvalidValue(value) => write!(f, "expected true or false, got {value:?}"),
        }
    }
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
