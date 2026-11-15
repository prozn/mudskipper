//! Reading moves written in UCI notation.

use std::error::Error;
use std::fmt;

use super::board::Board;
use super::moves::Move;
use super::piece::PieceType;
use super::square::Square;

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
    /// Finds the legal move written as `text` in UCI notation.
    ///
    /// Castling is written as the king's move, such as `e1g1`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseMoveError::Malformed`] if `text` is not UCI move notation, and
    /// [`ParseMoveError::Illegal`] if it names no legal move in this position.
    pub fn parse_move(&self, text: &str) -> Result<Move, ParseMoveError> {
        let malformed = || ParseMoveError::Malformed(text.to_owned());
        let (squares, promotion) = match text.len() {
            4 => (text, None),
            5 => {
                let promotion = match text.as_bytes()[4] {
                    b'n' => PieceType::Knight,
                    b'b' => PieceType::Bishop,
                    b'r' => PieceType::Rook,
                    b'q' => PieceType::Queen,
                    _ => return Err(malformed()),
                };
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
            .find(|mv| {
                let mv_to = if mv.is_castling() {
                    mv.castling_king_target()
                } else {
                    mv.to()
                };
                mv.from() == from && mv_to == to && mv.promotion() == promotion
            })
            .ok_or_else(|| ParseMoveError::Illegal(text.to_owned()))
    }
}
