//! The Universal Chess Interface protocol layer.

use std::fmt;
use std::io::{self, BufRead, Write};

use crate::chess::{Board, ParseFenError, ParseMoveError, Piece, Square};

/// Engine name reported in reply to `uci`.
const NAME: &str = concat!("Mudskipper ", env!("CARGO_PKG_VERSION"));

/// Author reported in reply to `uci`.
const AUTHOR: &str = "Prozn";

/// Runs the UCI loop, reading commands from `input` and writing replies to `output`.
///
/// The engine starts in the standard start position. The loop ends on `quit` or end of
/// input. Lines it cannot act on are answered with an `info string` and otherwise ignored.
///
/// # Errors
///
/// Returns any I/O error from reading `input` or writing `output`.
pub fn run(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut board = Board::startpos();
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
                writeln!(output, "uciok")?;
            }
            Some("isready") => writeln!(output, "readyok")?,
            Some("position") => match parse_position(line) {
                Ok(new_board) => board = new_board,
                Err(err) => writeln!(output, "info string invalid position: {err}")?,
            },
            Some("go") => go(&mut output, &board, line)?,
            Some("d") => write_board(&mut output, &board)?,
            Some("quit") => break,
            Some(_) => writeln!(output, "info string unknown command: {}", line.trim())?,
        }
        output.flush()?;
    }
    Ok(())
}

/// Parses a `position` command into the board it sets up.
fn parse_position(line: &str) -> Result<Board, PositionError> {
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
        let mv = board.parse_move(text).map_err(PositionError::Move)?;
        Ok(board.make(mv))
    })
}

/// Handles a `go` command. Only `go perft <depth>` is supported so far.
fn go(output: &mut impl Write, board: &Board, line: &str) -> io::Result<()> {
    let args: Vec<&str> = line.split_whitespace().skip(1).collect();
    match args.as_slice() {
        ["perft", depth] => match depth.parse::<u32>() {
            Ok(depth) => write_split_perft(output, board, depth),
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
fn write_split_perft(output: &mut impl Write, board: &Board, depth: u32) -> io::Result<()> {
    let split = board.split_perft(depth);
    for (mv, nodes) in &split {
        writeln!(output, "{mv}: {nodes}")?;
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
fn write_board(output: &mut impl Write, board: &Board) -> io::Result<()> {
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
    writeln!(output, "Fen: {}", board.to_fen())?;
    writeln!(output, "Key: {:016X}", board.key())
}
