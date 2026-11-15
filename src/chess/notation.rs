//! Reading and writing moves in UCI notation.

use std::error::Error;
use std::fmt;

use super::board::Board;
use super::moves::{Move, promotion_letter};
use super::square::Square;

/// How UCI notation writes castling moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CastlingNotation {
    /// As the king's move to its castling square, such as `e1g1`.
    #[default]
    Standard,
    /// As the king capturing its own rook, such as `e1h1`, as the `UCI_Chess960` option
    /// asks. Only this form is unambiguous when the king already stands on its castling
    /// square.
    KingTakesRook,
}

impl CastlingNotation {
    /// The target square UCI notation writes for `mv`.
    const fn target(self, mv: Move) -> Square {
        match self {
            Self::Standard => mv.destination(),
            Self::KingTakesRook => mv.to(),
        }
    }
}

/// A move written in UCI notation with a chosen [`CastlingNotation`], as returned by
/// [`Move::display`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct UciMove {
    mv: Move,
    notation: CastlingNotation,
}

impl fmt::Display for UciMove {
    /// Writes the move in UCI notation, such as `e2e4` or `e7e8q`, and the null move as
    /// `0000`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mv.is_null() {
            return f.write_str("0000");
        }
        write!(f, "{}{}", self.mv.from(), self.notation.target(self.mv))?;
        if let Some(piece_type) = self.mv.promotion() {
            write!(f, "{}", promotion_letter(piece_type))?;
        }
        Ok(())
    }
}

impl Move {
    /// Writes the move in UCI notation, with castling written as `notation` says.
    ///
    /// The move's [`Display`](fmt::Display) impl uses [`CastlingNotation::Standard`].
    pub const fn display(self, notation: CastlingNotation) -> UciMove {
        UciMove { mv: self, notation }
    }
}

/// Error returned when text is not a legal move in UCI notation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseMoveError {
    /// The text is not a move in UCI notation, such as `e2e4` or `e7e8q`.
    Malformed(String),
    /// The text is a well-formed move, but not a legal move in the position.
    Illegal(String),
}

impl fmt::Display for ParseMoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(text) => write!(f, "invalid move {text:?}"),
            Self::Illegal(text) => write!(f, "illegal move {text}"),
        }
    }
}

impl Error for ParseMoveError {}

impl Board {
    /// Finds the legal move written as `text` in UCI notation, with castling written as
    /// `notation` says.
    ///
    /// In a Chess960 position, [`CastlingNotation::Standard`] can write castling the same
    /// way as a plain king move, such as `b1c1` with the king on b1 and its rook on a1.
    /// The plain king move is then found, so read Chess960 games with
    /// [`CastlingNotation::KingTakesRook`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseMoveError::Malformed`] if `text` is not UCI move notation, and
    /// [`ParseMoveError::Illegal`] if it names no legal move in this position.
    pub fn parse_move(
        &self,
        text: &str,
        notation: CastlingNotation,
    ) -> Result<Move, ParseMoveError> {
        let malformed = || ParseMoveError::Malformed(text.to_owned());
        let (squares, promotion) = match text.len() {
            4 => (text, None),
            5 => {
                let promotion = Move::PROMOTION_PIECES
                    .into_iter()
                    .find(|&piece_type| {
                        promotion_letter(piece_type) == char::from(text.as_bytes()[4])
                    })
                    .ok_or_else(malformed)?;
                (&text[..4], Some(promotion))
            }
            _ => return Err(malformed()),
        };
        let from: Square = squares
            .get(..2)
            .and_then(|s| s.parse().ok())
            .ok_or_else(malformed)?;
        let to: Square = squares
            .get(2..)
            .and_then(|s| s.parse().ok())
            .ok_or_else(malformed)?;

        self.legal_moves()
            .iter()
            .copied()
            .find(|&mv| {
                mv.from() == from && notation.target(mv) == to && mv.promotion() == promotion
            })
            .ok_or_else(|| ParseMoveError::Illegal(text.to_owned()))
    }
}
