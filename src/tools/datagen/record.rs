//! A self-play game in viriformat: a packed start position, then each move with its
//! score.
//!
//! The format is specified by the viriformat README
//! (<https://github.com/cosmobobak/viriformat>). Read the copy on GitHub: the one bundled
//! with the 2.0.1 crate has the piece nibbles of its example the wrong way round.

use std::io::{self, Write};

use crate::chess::{Board, CastlingSide, Colour, Move, PieceType, Square};

/// Piece code of a rook that may still castle, one past the king's.
const UNMOVED_ROOK: u8 = 6;

/// Piece-code bit marking a black piece.
const BLACK_BIT: u8 = 0b1000;

/// The en passant field's value when there is no en passant square.
const NO_EN_PASSANT: u8 = 64;

/// Move-type bits of an en passant capture, a castling move and a promotion.
const EN_PASSANT_TYPE: u16 = 1 << 14;
const CASTLING_TYPE: u16 = 2 << 14;
const PROMOTION_TYPE: u16 = 3 << 14;

/// Shift of the promotion piece in a move.
const PROMOTION_SHIFT: u16 = 12;

/// Shift of the target square in a move.
const TO_SHIFT: u16 = 6;

/// How a game ended, from White's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// Black won.
    BlackWin,
    /// The game was drawn.
    Draw,
    /// White won.
    WhiteWin,
}

impl Outcome {
    /// Returns the win for `colour`.
    #[must_use]
    pub const fn win_for(colour: Colour) -> Self {
        match colour {
            Colour::White => Self::WhiteWin,
            Colour::Black => Self::BlackWin,
        }
    }

    /// The game-result byte: 0 for a black win, 1 for a draw, 2 for a white win.
    const fn to_byte(self) -> u8 {
        match self {
            Self::BlackWin => 0,
            Self::Draw => 1,
            Self::WhiteWin => 2,
        }
    }
}

/// A game being recorded: the position it started from and every move played, each with
/// the white-relative score of the position it was played in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRecord {
    start: Board,
    moves: Vec<(Move, i16)>,
}

impl GameRecord {
    /// Returns an empty record of a game starting from `start`.
    #[must_use]
    pub const fn new(start: Board) -> Self {
        Self {
            start,
            moves: Vec::new(),
        }
    }

    /// Records `mv`, played in the position after the moves recorded so far, with the
    /// white-relative score of that position.
    pub fn push(&mut self, mv: Move, white_score: i16) {
        self.moves.push((mv, white_score));
    }

    /// Each move recorded, in the order played, with its white-relative score.
    #[must_use]
    pub fn moves(&self) -> &[(Move, i16)] {
        &self.moves
    }

    /// The number of moves recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// Reports whether no move has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Writes the game in viriformat, with `outcome` as its result.
    ///
    /// # Errors
    ///
    /// Returns any I/O error from writing `out`.
    pub fn write(&self, outcome: Outcome, out: &mut impl Write) -> io::Result<()> {
        out.write_all(&pack(&self.start, outcome))?;
        for &(mv, score) in &self.moves {
            out.write_all(&encode(mv).to_le_bytes())?;
            out.write_all(&score.to_le_bytes())?;
        }
        out.write_all(&[0; 4])
    }
}

/// Packs `board` into viriformat's 32-byte header, with a position score of 0.
fn pack(board: &Board, outcome: Outcome) -> [u8; 32] {
    let mut occupancy = 0_u64;
    let mut pieces = [0_u8; 16];
    let occupied = Square::ALL
        .into_iter()
        .filter_map(|square| board.piece_at(square).map(|piece| (square, piece)));
    for (i, (square, piece)) in occupied.enumerate() {
        occupancy |= 1 << square.index();
        let colour = piece.colour();
        let is_castling_rook = CastlingSide::ALL
            .into_iter()
            .any(|side| board.castling_rights().rook(colour, side) == Some(square));
        let mut code = if is_castling_rook {
            UNMOVED_ROOK
        } else {
            piece.piece_type().index() as u8
        };
        if colour == Colour::Black {
            code |= BLACK_BIT;
        }
        pieces[i / 2] |= code << (i % 2 * 4);
    }

    let en_passant = board
        .en_passant()
        .map_or(NO_EN_PASSANT, |square| square.index() as u8);
    let side = match board.side_to_move() {
        Colour::White => 0,
        Colour::Black => 1 << 7,
    };
    let halfmove_clock = u8::try_from(board.halfmove_clock()).unwrap_or(u8::MAX);
    let fullmove_number = board.fullmove_number().max(1);

    let mut header = [0; 32];
    header[0..8].copy_from_slice(&occupancy.to_le_bytes());
    header[8..24].copy_from_slice(&pieces);
    header[24] = side | en_passant;
    header[25] = halfmove_clock;
    header[26..28].copy_from_slice(&fullmove_number.to_le_bytes());
    // Bytes 28 and 29 hold the position score, left at 0.
    header[30] = outcome.to_byte();
    header
}

/// Encodes `mv` as a viriformat move, with castling as king-takes-rook.
fn encode(mv: Move) -> u16 {
    let squares = mv.from().index() as u16 | (mv.to().index() as u16) << TO_SHIFT;
    let flags = if let Some(piece) = mv.promotion() {
        let promotion = match piece {
            PieceType::Knight => 0,
            PieceType::Bishop => 1,
            PieceType::Rook => 2,
            PieceType::Queen => 3,
            PieceType::Pawn | PieceType::King => unreachable!("pawns promote to N, B, R or Q"),
        };
        PROMOTION_TYPE | promotion << PROMOTION_SHIFT
    } else if mv.is_en_passant() {
        EN_PASSANT_TYPE
    } else if mv.is_castling() {
        CASTLING_TYPE
    } else {
        0
    };
    squares | flags
}
