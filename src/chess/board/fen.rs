//! Forsyth-Edwards Notation: parsing positions from FEN and printing them back.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

use super::Board;
use crate::chess::castling::{CastlingRights, CastlingSide};
use crate::chess::piece::{
    Colour, DOUBLE_PUSH_RANK, DOUBLE_PUSH_SKIPPED_RANK, PAWN_START_RANK, Piece, PieceType,
};
use crate::chess::square::Square;

/// Error returned when text is not a valid FEN position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseFenError {
    /// Fewer than the four required fields: placement, side to move, castling, en passant.
    TooFewFields {
        /// Number of fields given.
        found: usize,
    },
    /// More than the six fields FEN defines.
    TooManyFields {
        /// Number of fields given.
        found: usize,
    },
    /// The placement field does not have eight ranks.
    WrongRankCount {
        /// Number of ranks given.
        found: usize,
    },
    /// A rank in the placement field does not describe exactly eight squares.
    WrongRankLength {
        /// The rank, from 1 to 8.
        rank: u8,
        /// Number of squares it describes.
        squares: usize,
    },
    /// A rank in the placement field has two empty-square counts in a row, such as `44`.
    SplitEmptyRun {
        /// The rank, from 1 to 8.
        rank: u8,
    },
    /// The placement field holds a character that is neither a piece letter nor 1 to 8.
    InvalidPiece {
        /// The offending character.
        found: char,
    },
    /// A colour does not have exactly one king.
    WrongKingCount {
        /// The colour with the wrong number of kings.
        colour: Colour,
        /// Number of kings it has.
        found: u32,
    },
    /// A pawn stands on the first or eighth rank.
    PawnOnBackRank {
        /// Where the pawn stands.
        square: Square,
    },
    /// The side to move is neither `w` nor `b`.
    InvalidSideToMove(String),
    /// The castling field is malformed or names a right with no rook to castle with.
    InvalidCastling(String),
    /// The en passant field is malformed or names a square no pawn just skipped.
    InvalidEnPassant(String),
    /// The halfmove clock is not a non-negative integer.
    InvalidHalfmoveClock(String),
    /// The fullmove number is not a non-negative integer.
    InvalidFullmoveNumber(String),
}

impl fmt::Display for ParseFenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewFields { found } => {
                write!(f, "too few fields: found {found}, need at least 4")
            }
            Self::TooManyFields { found } => {
                write!(f, "too many fields: found {found}, allowed at most 6")
            }
            Self::WrongRankCount { found } => write!(f, "found {found} ranks, need 8"),
            Self::WrongRankLength { rank, squares } => {
                write!(f, "rank {rank} describes {squares} squares, need 8")
            }
            Self::SplitEmptyRun { rank } => {
                write!(f, "rank {rank} has two empty-square counts in a row")
            }
            Self::InvalidPiece { found } => write!(f, "invalid piece character {found:?}"),
            Self::WrongKingCount { colour, found } => {
                write!(f, "{colour} has {found} kings, need exactly 1")
            }
            Self::PawnOnBackRank { square } => write!(f, "pawn on back rank square {square}"),
            Self::InvalidSideToMove(field) => write!(f, "invalid side to move {field:?}"),
            Self::InvalidCastling(field) => write!(f, "invalid castling rights {field:?}"),
            Self::InvalidEnPassant(field) => write!(f, "invalid en passant square {field:?}"),
            Self::InvalidHalfmoveClock(field) => write!(f, "invalid halfmove clock {field:?}"),
            Self::InvalidFullmoveNumber(field) => {
                write!(f, "invalid fullmove number {field:?}")
            }
        }
    }
}

impl Error for ParseFenError {}

impl Board {
    /// Parses a position from FEN.
    ///
    /// The halfmove clock and fullmove number may be left out, defaulting to 0 and 1.
    /// The castling field may be standard, X-FEN or Shredder-FEN: `KQkq` name the
    /// outermost rook on each side of the king, and a file letter, `A` to `H` for white
    /// and `a` to `h` for black, names the rook on that file.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseFenError`] naming the first problem found if `fen` is malformed or
    /// describes an impossible position: not one king per side, a pawn on a back rank,
    /// a castling right with no rook, or an en passant square no pawn just skipped.
    pub fn from_fen(fen: &str) -> Result<Self, ParseFenError> {
        let fields: Vec<&str> = fen.split_whitespace().collect();
        if fields.len() < 4 {
            return Err(ParseFenError::TooFewFields {
                found: fields.len(),
            });
        }
        if fields.len() > 6 {
            return Err(ParseFenError::TooManyFields {
                found: fields.len(),
            });
        }

        let mut board = Self::empty();
        parse_placement(&mut board, fields[0])?;
        board.side_to_move = parse_side_to_move(fields[1])?;
        board.castling = parse_castling(&board, fields[2])?;
        board.en_passant = parse_en_passant(&board, fields[3])?;
        if let Some(&field) = fields.get(4) {
            board.halfmove_clock = parse_counter(field)
                .ok_or_else(|| ParseFenError::InvalidHalfmoveClock(field.to_owned()))?;
        }
        if let Some(&field) = fields.get(5) {
            board.fullmove_number = parse_counter(field)
                .ok_or_else(|| ParseFenError::InvalidFullmoveNumber(field.to_owned()))?;
        }
        board.key = board.compute_key();
        Ok(board)
    }

    /// Prints the position as FEN, with all six fields.
    ///
    /// Castling rights are written in X-FEN: `KQkq` for the outermost rook on each side of
    /// the king, so standard positions print as standard FEN, and the rook's file for any
    /// other rook.
    #[must_use]
    pub fn to_fen(&self) -> String {
        self.format_fen(CastlingStyle::XFen)
    }

    /// Prints the position as Shredder-FEN, which writes each castling right as the file
    /// of its rook, such as `HAha` for the standard start position.
    #[must_use]
    pub fn to_shredder_fen(&self) -> String {
        self.format_fen(CastlingStyle::Shredder)
    }

    fn format_fen(&self, style: CastlingStyle) -> String {
        let mut fen = String::with_capacity(90);
        for (i, rank) in Square::ALL.chunks(8).rev().enumerate() {
            if i > 0 {
                fen.push('/');
            }
            let mut empty = 0u8;
            for &square in rank {
                if let Some(piece) = self.piece_at(square) {
                    if empty > 0 {
                        fen.push(char::from(b'0' + empty));
                        empty = 0;
                    }
                    fen.push(piece.to_char());
                } else {
                    empty += 1;
                }
            }
            if empty > 0 {
                fen.push(char::from(b'0' + empty));
            }
        }

        fen.push(' ');
        fen.push(match self.side_to_move {
            Colour::White => 'w',
            Colour::Black => 'b',
        });

        fen.push(' ');
        if self.castling.is_empty() {
            fen.push('-');
        }
        for colour in Colour::ALL {
            for side in CastlingSide::ALL {
                if let Some(rook) = self.castling.rook(colour, side) {
                    let letter = if style == CastlingStyle::XFen
                        && outermost_rook(self, colour, side) == Some(rook)
                    {
                        CastlingLetter::Outermost(side)
                    } else {
                        CastlingLetter::File(rook.file())
                    };
                    fen.push(letter.to_char(colour));
                }
            }
        }

        fen.push(' ');
        match self.en_passant {
            Some(square) => fen.push_str(&square.to_string()),
            None => fen.push('-'),
        }

        fen.push(' ');
        fen.push_str(&self.halfmove_clock.to_string());
        fen.push(' ');
        fen.push_str(&self.fullmove_number.to_string());
        fen
    }
}

impl FromStr for Board {
    type Err = ParseFenError;

    /// Parses a position from FEN, as [`Board::from_fen`] does.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_fen(s)
    }
}

/// Places the pieces of the placement field on `board` and checks they form a position.
fn parse_placement(board: &mut Board, field: &str) -> Result<(), ParseFenError> {
    let ranks: Vec<&str> = field.split('/').collect();
    if ranks.len() != 8 {
        return Err(ParseFenError::WrongRankCount { found: ranks.len() });
    }

    for (rank, text) in (0..8u8).rev().zip(ranks) {
        let mut squares = 0usize;
        let mut after_run = false;
        for c in text.chars() {
            if let Some(run @ 1..=8) = c.to_digit(10) {
                if after_run {
                    return Err(ParseFenError::SplitEmptyRun { rank: rank + 1 });
                }
                squares += run as usize;
                after_run = true;
            } else {
                after_run = false;
                let piece = Piece::from_char(c).ok_or(ParseFenError::InvalidPiece { found: c })?;
                // Squares past the h-file are counted but not placed; the length check
                // below rejects the rank.
                let square = u8::try_from(squares)
                    .ok()
                    .and_then(|file| Square::from_coords(file, rank));
                if let Some(square) = square {
                    board.put_piece(piece, square);
                }
                squares += 1;
            }
        }
        if squares != 8 {
            return Err(ParseFenError::WrongRankLength {
                rank: rank + 1,
                squares,
            });
        }
    }

    for colour in Colour::ALL {
        let found = board.pieces(colour, PieceType::King).count();
        if found != 1 {
            return Err(ParseFenError::WrongKingCount { colour, found });
        }
    }

    let pawns =
        board.pieces(Colour::White, PieceType::Pawn) | board.pieces(Colour::Black, PieceType::Pawn);
    if let Some(square) = pawns.into_iter().find(|sq| {
        Colour::ALL
            .iter()
            .any(|colour| sq.rank() == colour.back_rank())
    }) {
        return Err(ParseFenError::PawnOnBackRank { square });
    }
    Ok(())
}

/// Parses a move counter written as plain decimal digits, with no sign.
fn parse_counter(field: &str) -> Option<u16> {
    if field.bytes().all(|b| b.is_ascii_digit()) {
        field.parse().ok()
    } else {
        None
    }
}

fn parse_side_to_move(field: &str) -> Result<Colour, ParseFenError> {
    match field {
        "w" => Ok(Colour::White),
        "b" => Ok(Colour::Black),
        _ => Err(ParseFenError::InvalidSideToMove(field.to_owned())),
    }
}

fn parse_castling(board: &Board, field: &str) -> Result<CastlingRights, ParseFenError> {
    let mut rights = CastlingRights::EMPTY;
    if field == "-" {
        return Ok(rights);
    }
    let invalid = || ParseFenError::InvalidCastling(field.to_owned());
    for c in field.chars() {
        let (colour, letter) = CastlingLetter::parse(c).ok_or_else(invalid)?;
        let (side, rook) = letter.resolve(board, colour).ok_or_else(invalid)?;
        if rights.rook(colour, side).is_some() {
            return Err(invalid());
        }
        rights.grant(colour, side, rook);
    }
    Ok(rights)
}

/// How the castling field of a printed FEN names each right.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CastlingStyle {
    /// X-FEN: `K` or `Q` for the outermost rook on a side, the rook's file otherwise.
    XFen,
    /// Shredder-FEN: always the rook's file.
    Shredder,
}

/// One character of the castling field, apart from the colour its case gives: upper case
/// for white, lower case for black.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CastlingLetter {
    /// `K` or `Q`: the outermost rook on that side of the king.
    Outermost(CastlingSide),
    /// A file letter, `A` to `H`: the rook on that file.
    File(u8),
}

impl CastlingLetter {
    /// Reads a castling field character and the colour it is for.
    fn parse(c: char) -> Option<(Colour, Self)> {
        let colour = if c.is_ascii_uppercase() {
            Colour::White
        } else {
            Colour::Black
        };
        let letter = match c.to_ascii_lowercase() {
            'k' => Self::Outermost(CastlingSide::King),
            'q' => Self::Outermost(CastlingSide::Queen),
            file @ 'a'..='h' => Self::File(file as u8 - b'a'),
            _ => return None,
        };
        Some((colour, letter))
    }

    /// The castling field character for `colour`.
    fn to_char(self, colour: Colour) -> char {
        let c = match self {
            Self::Outermost(CastlingSide::King) => 'k',
            Self::Outermost(CastlingSide::Queen) => 'q',
            Self::File(file) => char::from(b'a' + file),
        };
        match colour {
            Colour::White => c.to_ascii_uppercase(),
            Colour::Black => c,
        }
    }

    /// The side and starting square of the rook this letter names for `colour`, if `colour`
    /// has a rook there and its king stands on its back rank.
    fn resolve(self, board: &Board, colour: Colour) -> Option<(CastlingSide, Square)> {
        match self {
            Self::Outermost(side) => Some((side, outermost_rook(board, colour, side)?)),
            Self::File(file) => {
                let king = back_rank_king(board, colour)?;
                let rook = Square::from_coords(file, colour.back_rank())?;
                let side = match file.cmp(&king.file()) {
                    Ordering::Greater => CastlingSide::King,
                    Ordering::Less => CastlingSide::Queen,
                    Ordering::Equal => return None,
                };
                (board.piece_at(rook) == Some(Piece::new(colour, PieceType::Rook)))
                    .then_some((side, rook))
            }
        }
    }
}

/// The rook of `colour` furthest from its king on `side`, if the king is on its back rank.
fn outermost_rook(board: &Board, colour: Colour, side: CastlingSide) -> Option<Square> {
    let back_rank = colour.back_rank();
    let king = back_rank_king(board, colour)?;
    let is_rook = |file| {
        let square = Square::from_coords(file, back_rank)?;
        (board.piece_at(square) == Some(Piece::new(colour, PieceType::Rook))).then_some(square)
    };
    match side {
        CastlingSide::King => (king.file() + 1..8).rev().find_map(is_rook),
        CastlingSide::Queen => (0..king.file()).find_map(is_rook),
    }
}

/// The square of `colour`'s king, if it stands on its back rank, as castling needs.
fn back_rank_king(board: &Board, colour: Colour) -> Option<Square> {
    let king = board.king(colour);
    (king.rank() == colour.back_rank()).then_some(king)
}

fn parse_en_passant(board: &Board, field: &str) -> Result<Option<Square>, ParseFenError> {
    if field == "-" {
        return Ok(None);
    }
    let invalid = || ParseFenError::InvalidEnPassant(field.to_owned());
    let square: Square = field.parse().map_err(|_| invalid())?;

    // The opponent's pawn has just moved two ranks, from `origin_rank` to `pawn_rank`,
    // skipping `square`.
    let pusher = !board.side_to_move;
    let (target_rank, pawn_rank, origin_rank) = (
        pusher.relative_rank(DOUBLE_PUSH_SKIPPED_RANK),
        pusher.relative_rank(DOUBLE_PUSH_RANK),
        pusher.relative_rank(PAWN_START_RANK),
    );
    if square.rank() != target_rank {
        return Err(invalid());
    }
    let pawn = Square::from_coords(square.file(), pawn_rank).ok_or_else(invalid)?;
    let origin = Square::from_coords(square.file(), origin_rank).ok_or_else(invalid)?;
    let pushed_pawn = Piece::new(!board.side_to_move, PieceType::Pawn);
    if board.piece_at(square).is_some()
        || board.piece_at(origin).is_some()
        || board.piece_at(pawn) != Some(pushed_pawn)
    {
        return Err(invalid());
    }
    Ok(Some(square))
}
