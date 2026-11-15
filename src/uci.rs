//! The Universal Chess Interface protocol layer.

use std::fmt;
use std::io::{self, BufRead, Write};

use crate::chess::{Board, CastlingNotation, ParseFenError, ParseMoveError, Piece, Square};

/// Engine name reported in reply to `uci`.
const NAME: &str = concat!("Mudskipper ", env!("CARGO_PKG_VERSION"));

/// Author reported in reply to `uci`.
const AUTHOR: &str = "Prozn";

/// Name of the check option that selects Chess960 castling notation.
const CHESS960_OPTION: &str = "UCI_Chess960";

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
    let mut notation = CastlingNotation::Standard;
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
                Ok(new_notation) => notation = new_notation,
                Err(err) => writeln!(output, "info string invalid setoption: {err}")?,
            },
            Some("position") => match parse_position(line, notation) {
                Ok(new_board) => board = new_board,
                Err(err) => writeln!(output, "info string invalid position: {err}")?,
            },
            Some("go") => go(&mut output, &board, notation, line)?,
            Some("d") => write_board(&mut output, &board, notation)?,
            Some("quit") => break,
            Some(_) => writeln!(output, "info string unknown command: {}", line.trim())?,
        }
        output.flush()?;
    }
    Ok(())
}

/// Parses a `position` command into the board it sets up, reading castling moves as
/// `notation` says.
fn parse_position(line: &str, notation: CastlingNotation) -> Result<Board, PositionError> {
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
    moves.iter().try_fold(board, |board, text| {
        let mv = board
            .parse_move(text, notation)
            .map_err(PositionError::Move)?;
        Ok(board.make(mv))
    })
}

/// Handles a `go` command. Only `go perft <depth>` is supported so far.
fn go(
    output: &mut impl Write,
    board: &Board,
    notation: CastlingNotation,
    line: &str,
) -> io::Result<()> {
    let args: Vec<&str> = line.split_whitespace().skip(1).collect();
    match args.as_slice() {
        ["perft", depth] => match depth.parse::<u32>() {
            Ok(depth) => write_split_perft(output, board, notation, depth),
            Err(_) => writeln!(output, "info string invalid perft depth {depth:?}"),
        },
        ["perft", ..] => writeln!(output, "info string expected go perft <depth>"),
        _ => writeln!(
            output,
            "info string unsupported go command: {}",
            line.trim()
        ),
    }
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

/// Parses a `setoption` command into the castling notation it selects.
///
/// `UCI_Chess960` is the only option so far. Option names are matched ignoring case, as
/// the UCI protocol asks.
fn parse_setoption(line: &str) -> Result<CastlingNotation, SetOptionError> {
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
        Some("true") => Ok(CastlingNotation::KingTakesRook),
        Some("false") => Ok(CastlingNotation::Standard),
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
fn write_board(
    output: &mut impl Write,
    board: &Board,
    notation: CastlingNotation,
) -> io::Result<()> {
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
    let fen = match notation {
        CastlingNotation::Standard => board.to_fen(),
        CastlingNotation::KingTakesRook => board.to_shredder_fen(),
    };
    writeln!(output, "Fen: {fen}")?;
    writeln!(output, "Key: {:016X}", board.key())
}
