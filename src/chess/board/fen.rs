//! Forsyth-Edwards Notation: parsing positions from FEN and printing them back.

use std::error::Error;
use std::fmt;
use std::str::FromStr;

use super::Board;
use crate::chess::castling::{CastlingRights, CastlingSide};
use crate::chess::piece::{Colour, Piece, PieceType};
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
            Self::InvalidPiece { found } => write!(f, "invalid piece character {found:?}"),
            Self::WrongKingCount { colour, found } => {
                write!(f, "{colour:?} has {found} kings, need exactly 1")
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
    /// Castling rights `KQkq` name the outermost rook on each side of the king.
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
            board.halfmove_clock = field
                .parse()
                .map_err(|_| ParseFenError::InvalidHalfmoveClock(field.to_owned()))?;
        }
        if let Some(&field) = fields.get(5) {
            board.fullmove_number = field
                .parse()
                .map_err(|_| ParseFenError::InvalidFullmoveNumber(field.to_owned()))?;
        }
        board.key = board.compute_key();
        Ok(board)
    }

    /// Prints the position as FEN, with all six fields.
    #[must_use]
    pub fn to_fen(&self) -> String {
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
                if self.castling.rook(colour, side).is_some() {
                    let letter = match side {
                        CastlingSide::King => 'k',
                        CastlingSide::Queen => 'q',
                    };
                    fen.push(match colour {
                        Colour::White => letter.to_ascii_uppercase(),
                        Colour::Black => letter,
                    });
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
        for c in text.chars() {
            if let Some(run @ 1..=8) = c.to_digit(10) {
                squares += run as usize;
            } else {
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
    if let Some(square) = pawns
        .into_iter()
        .find(|sq| sq.rank() == 0 || sq.rank() == 7)
    {
        return Err(ParseFenError::PawnOnBackRank { square });
    }
    Ok(())
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
        let (colour, side) = match c {
            'K' => (Colour::White, CastlingSide::King),
            'Q' => (Colour::White, CastlingSide::Queen),
            'k' => (Colour::Black, CastlingSide::King),
            'q' => (Colour::Black, CastlingSide::Queen),
            _ => return Err(invalid()),
        };
        if rights.rook(colour, side).is_some() {
            return Err(invalid());
        }
        let rook = outermost_rook(board, colour, side).ok_or_else(invalid)?;
        rights.grant(colour, side, rook);
    }
    Ok(rights)
}

/// The rook of `colour` furthest from its king on `side`, if the king is on its back rank.
fn outermost_rook(board: &Board, colour: Colour, side: CastlingSide) -> Option<Square> {
    let back_rank = match colour {
        Colour::White => 0,
        Colour::Black => 7,
    };
    let king = board.pieces(colour, PieceType::King).into_iter().next()?;
    if king.rank() != back_rank {
        return None;
    }
    let is_rook = |file| {
        let square = Square::from_coords(file, back_rank)?;
        (board.piece_at(square) == Some(Piece::new(colour, PieceType::Rook))).then_some(square)
    };
    match side {
        CastlingSide::King => (king.file() + 1..8).rev().find_map(is_rook),
        CastlingSide::Queen => (0..king.file()).find_map(is_rook),
    }
}

fn parse_en_passant(board: &Board, field: &str) -> Result<Option<Square>, ParseFenError> {
    if field == "-" {
        return Ok(None);
    }
    let invalid = || ParseFenError::InvalidEnPassant(field.to_owned());
    let square: Square = field.parse().map_err(|_| invalid())?;

    // The opponent's pawn has just moved two ranks, from `origin_rank` to `pawn_rank`,
    // skipping `square`.
    let (target_rank, pawn_rank, origin_rank) = match board.side_to_move {
        Colour::White => (5, 4, 6),
        Colour::Black => (2, 3, 1),
    };
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
