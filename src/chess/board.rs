//! The board: piece placement plus the rest of the game state that FEN records.

mod draw;
mod fen;
mod make;
mod movegen;
mod see;

pub use fen::ParseFenError;

use super::attacks;
use super::bitboard::Bitboard;
use super::castling::CastlingRights;
use super::moves::Move;
use super::piece::{Colour, EN_PASSANT_CAPTURER_RANK, Piece, PieceType};
use super::square::Square;
use super::zobrist::{self, Key};

/// FEN of the standard start position.
const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A chess position.
///
/// Pieces are held twice, as bitboards for set operations and as a mailbox for lookup by
/// square, and the two always agree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    mailbox: [Option<Piece>; 64],
    pieces: [Bitboard; 6],
    colours: [Bitboard; 2],
    side_to_move: Colour,
    castling: CastlingRights,
    en_passant: Option<Square>,
    halfmove_clock: u16,
    fullmove_number: u16,
    key: u64,
}

impl Board {
    /// Returns the standard start position.
    #[must_use]
    #[expect(
        clippy::missing_panics_doc,
        reason = "parsing the constant start FEN cannot fail"
    )]
    pub fn startpos() -> Self {
        Self::from_fen(STARTPOS_FEN).expect("the start position FEN is valid")
    }

    /// Piece standing on `square`, if any.
    #[must_use]
    pub const fn piece_at(&self, square: Square) -> Option<Piece> {
        self.mailbox[square.index()]
    }

    /// The type of piece `mv` captures, a pawn for en passant, or `None` if it captures
    /// nothing.
    ///
    /// # Panics
    ///
    /// May panic if `mv` is not pseudo-legal in this position.
    #[must_use]
    pub fn captured(&self, mv: Move) -> Option<PieceType> {
        if mv.is_en_passant() {
            Some(PieceType::Pawn)
        } else if mv.is_capture() {
            let victim = self
                .piece_at(mv.to())
                .unwrap_or_else(|| panic!("{mv} captures nothing in {}", self.to_fen()));
            Some(victim.piece_type())
        } else {
            None
        }
    }

    /// Squares holding a piece of type `piece_type` belonging to `colour`.
    #[must_use]
    pub fn pieces(&self, colour: Colour, piece_type: PieceType) -> Bitboard {
        self.colours[colour.index()] & self.pieces[piece_type.index()]
    }

    /// The side whose turn it is.
    #[must_use]
    pub const fn side_to_move(&self) -> Colour {
        self.side_to_move
    }

    /// The castling rights still held by each colour.
    #[must_use]
    pub const fn castling_rights(&self) -> CastlingRights {
        self.castling
    }

    /// The square a pawn may capture onto en passant, as recorded after a double push.
    #[must_use]
    pub const fn en_passant(&self) -> Option<Square> {
        self.en_passant
    }

    /// Number of halfmoves since the last capture or pawn move.
    #[must_use]
    pub const fn halfmove_clock(&self) -> u16 {
        self.halfmove_clock
    }

    /// Number of the current full move, starting at 1 and increasing after black moves.
    #[must_use]
    pub const fn fullmove_number(&self) -> u16 {
        self.fullmove_number
    }

    /// Zobrist key of the position.
    ///
    /// The key covers piece placement, side to move, castling rights and the en passant
    /// file when a pawn stands ready to capture en passant, but not the move clocks.
    #[must_use]
    pub const fn key(&self) -> Key {
        Key::new(self.key)
    }

    /// Computes the Zobrist key from the rest of the position.
    fn compute_key(&self) -> u64 {
        let mut key = 0;
        for square in Square::ALL {
            if let Some(piece) = self.mailbox[square.index()] {
                key ^= zobrist::piece(piece, square);
            }
        }
        if self.side_to_move == Colour::Black {
            key ^= zobrist::black_to_move();
        }
        key ^= zobrist::castling_rights(self.castling);
        if let Some(square) = self.en_passant
            && self.can_capture_en_passant(square)
        {
            key ^= zobrist::en_passant(square.file());
        }
        key
    }

    /// Reports whether a pawn of the side to move stands beside the pawn that skipped
    /// `target`, ignoring pins.
    ///
    /// Only then does the en passant square change which moves are available, so only then
    /// is it part of the key.
    fn can_capture_en_passant(&self, target: Square) -> bool {
        let capturer_rank = self.side_to_move.relative_rank(EN_PASSANT_CAPTURER_RANK);
        let pawns = self.pieces(self.side_to_move, PieceType::Pawn);
        [target.file().checked_sub(1), target.file().checked_add(1)]
            .into_iter()
            .flatten()
            .filter_map(|file| Square::from_coords(file, capturer_rank))
            .any(|square| pawns.contains(square))
    }

    /// An empty board with white to move and no rights.
    const fn empty() -> Self {
        Self {
            mailbox: [None; 64],
            pieces: [Bitboard::EMPTY; 6],
            colours: [Bitboard::EMPTY; 2],
            side_to_move: Colour::White,
            castling: CastlingRights::EMPTY,
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            key: 0,
        }
    }

    /// Reports whether `colour` has a piece other than its king and pawns.
    #[must_use]
    pub fn has_non_pawn_material(&self, colour: Colour) -> bool {
        let kings_and_pawns = self.piece_type(PieceType::King) | self.piece_type(PieceType::Pawn);
        !(self.colour(colour) & !kings_and_pawns).is_empty()
    }

    /// Reports whether the side to move is in check.
    #[must_use]
    pub fn in_check(&self) -> bool {
        self.is_attacked(
            self.king(self.side_to_move),
            !self.side_to_move,
            self.occupied(),
        )
    }

    /// Squares holding a piece of either colour.
    const fn occupied(&self) -> Bitboard {
        Bitboard::from_bits(self.colours[0].bits() | self.colours[1].bits())
    }

    /// Squares holding a piece of `colour`.
    const fn colour(&self, colour: Colour) -> Bitboard {
        self.colours[colour.index()]
    }

    /// Squares holding a piece of type `piece_type`, of either colour.
    const fn piece_type(&self, piece_type: PieceType) -> Bitboard {
        self.pieces[piece_type.index()]
    }

    /// The square of `colour`'s king.
    fn king(&self, colour: Colour) -> Square {
        self.pieces(colour, PieceType::King)
            .first()
            .expect("each side has exactly one king")
    }

    /// Pieces of either colour attacking `square`, with `occupied` as the blockers for
    /// sliding pieces.
    fn attackers_to(&self, square: Square, occupied: Bitboard) -> Bitboard {
        let diagonal = self.piece_type(PieceType::Bishop) | self.piece_type(PieceType::Queen);
        let straight = self.piece_type(PieceType::Rook) | self.piece_type(PieceType::Queen);
        (attacks::pawn(Colour::White, square) & self.pieces(Colour::Black, PieceType::Pawn))
            | (attacks::pawn(Colour::Black, square) & self.pieces(Colour::White, PieceType::Pawn))
            | (attacks::knight(square) & self.piece_type(PieceType::Knight))
            | (attacks::king(square) & self.piece_type(PieceType::King))
            | (attacks::bishop(square, occupied) & diagonal)
            | (attacks::rook(square, occupied) & straight)
    }

    /// Reports whether a piece of colour `by` attacks `square`, with `occupied` as the
    /// blockers for sliding pieces.
    fn is_attacked(&self, square: Square, by: Colour, occupied: Bitboard) -> bool {
        !(self.attackers_to(square, occupied) & self.colour(by)).is_empty()
    }

    /// Places `piece` on the empty `square`, updating the key.
    fn put_piece(&mut self, piece: Piece, square: Square) {
        debug_assert!(
            self.mailbox[square.index()].is_none(),
            "{square} is already occupied"
        );
        let bb = Bitboard::from_square(square);
        self.mailbox[square.index()] = Some(piece);
        self.pieces[piece.piece_type().index()] ^= bb;
        self.colours[piece.colour().index()] ^= bb;
        self.key ^= zobrist::piece(piece, square);
    }

    /// Removes and returns the piece on the occupied `square`, updating the key.
    fn remove_piece(&mut self, square: Square) -> Piece {
        let piece = self.mailbox[square.index()]
            .take()
            .unwrap_or_else(|| panic!("{square} should be occupied"));
        let bb = Bitboard::from_square(square);
        self.pieces[piece.piece_type().index()] ^= bb;
        self.colours[piece.colour().index()] ^= bb;
        self.key ^= zobrist::piece(piece, square);
        piece
    }
}
