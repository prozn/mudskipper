//! The **Accumulator**: the network's hidden layer for one position, kept up to date
//! move by move.

use super::network::{INPUTS, NETWORK, QA, QB, Row, SCALE};
use crate::chess::{Board, Colour, Move, Piece, PieceType, Square};

/// The hidden layer of one position, from each side's perspective, before activation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accumulator {
    /// Indexed by [`Colour::index`] of the perspective.
    perspectives: [Row; 2],
}

impl Accumulator {
    /// Builds the accumulator of `board` from scratch.
    #[must_use]
    pub fn new(board: &Board) -> Self {
        let network = &*NETWORK;
        let mut accumulator = Self {
            perspectives: [*network.feature_bias; 2],
        };
        for colour in Colour::ALL {
            for piece_type in PieceType::ALL {
                let piece = Piece::new(colour, piece_type);
                for square in board.pieces(colour, piece_type) {
                    for perspective in Colour::ALL {
                        let row = &network.feature_weights[feature(perspective, piece, square)];
                        let values = &mut accumulator.perspectives[perspective.index()].0;
                        for (value, &weight) in values.iter_mut().zip(&row.0) {
                            *value += weight;
                        }
                    }
                }
            }
        }
        accumulator
    }

    /// Evaluates the position in centipawns for `side_to_move`, unclamped.
    #[must_use]
    pub fn evaluate(&self, side_to_move: Colour) -> i32 {
        let network = &*NETWORK;
        let us = &self.perspectives[side_to_move.index()];
        let them = &self.perspectives[(!side_to_move).index()];
        let sum = screlu_dot(us, &network.output_weights[0])
            + screlu_dot(them, &network.output_weights[1]);
        (sum / i32::from(QA) + i32::from(network.output_bias)) * SCALE / (i32::from(QA) * QB)
    }

    /// Writes into `self` the accumulator of the position after `mv` is played in
    /// `board`, whose accumulator is `parent`.
    fn make_from(&mut self, parent: &Self, board: &Board, mv: Move) {
        let network = &*NETWORK;
        let change = Change::of(board, mv);
        for perspective in Colour::ALL {
            let row = |(piece, square): (Piece, Square)| {
                &network.feature_weights[feature(perspective, piece, square)].0
            };
            let source = &parent.perspectives[perspective.index()].0;
            let target = &mut self.perspectives[perspective.index()].0;
            match change {
                Change::Move { sub, add } => {
                    let (sub, add) = (row(sub), row(add));
                    for (i, value) in target.iter_mut().enumerate() {
                        *value = source[i] - sub[i] + add[i];
                    }
                }
                Change::Capture { sub, victim, add } => {
                    let (sub, victim, add) = (row(sub), row(victim), row(add));
                    for (i, value) in target.iter_mut().enumerate() {
                        *value = source[i] - sub[i] - victim[i] + add[i];
                    }
                }
                Change::Castling { subs, adds } => {
                    let (sub0, sub1) = (row(subs[0]), row(subs[1]));
                    let (add0, add1) = (row(adds[0]), row(adds[1]));
                    for (i, value) in target.iter_mut().enumerate() {
                        *value = source[i] - sub0[i] - sub1[i] + add0[i] + add1[i];
                    }
                }
            }
        }
    }
}

/// The features a move takes away and puts back, as pieces on squares.
#[derive(Clone, Copy, Debug)]
enum Change {
    /// A quiet move or promotion: the piece leaves one square, and it or its promotion
    /// arrives on another.
    Move {
        sub: (Piece, Square),
        add: (Piece, Square),
    },
    /// A capture, including en passant and capturing promotions.
    Capture {
        sub: (Piece, Square),
        victim: (Piece, Square),
        add: (Piece, Square),
    },
    /// Castling: king and rook both move. In Chess960 a piece's target may be its own or
    /// the other piece's start square, which subtracting and adding handles.
    Castling {
        subs: [(Piece, Square); 2],
        adds: [(Piece, Square); 2],
    },
}

impl Change {
    /// The change `mv` makes to `board`, following [`Board::make`].
    fn of(board: &Board, mv: Move) -> Self {
        let (from, to) = (mv.from(), mv.to());
        let piece = board
            .piece_at(from)
            .expect("a pseudo-legal move starts on a piece");
        if mv.is_castling() {
            let rook = board
                .piece_at(to)
                .expect("castling is stored as the king taking its rook");
            return Self::Castling {
                subs: [(piece, from), (rook, to)],
                adds: [
                    (piece, mv.castling_king_target()),
                    (rook, mv.castling_rook_target()),
                ],
            };
        }
        let placed = mv
            .promotion()
            .map_or(piece, |piece_type| Piece::new(piece.colour(), piece_type));
        let (sub, add) = ((piece, from), (placed, to));
        if mv.is_capture() {
            let square = mv.captured_square();
            let victim = board
                .piece_at(square)
                .expect("a capture has a piece to capture");
            Self::Capture {
                sub,
                victim: (victim, square),
                add,
            }
        } else {
            Self::Move { sub, add }
        }
    }
}

/// Input features for one side's pieces: 6 piece types × 64 squares.
const INPUTS_PER_SIDE: usize = INPUTS / 2;

/// The index of `piece` on `square` among the input features of `perspective`.
///
/// This is bullet's `Chess768`: the perspective's own pieces come first, and Black sees
/// the board flipped vertically, so both sides see their pieces from the bottom.
fn feature(perspective: Colour, piece: Piece, square: Square) -> usize {
    let (side, square) = match perspective {
        Colour::White => (piece.colour().index(), square.index()),
        Colour::Black => ((!piece.colour()).index(), square.index() ^ 0b11_1000),
    };
    side * INPUTS_PER_SIDE + piece.piece_type().index() * Square::ALL.len() + square
}

/// The dot product of `weights` with the squared clamped activation of `values`, scaled
/// by `QA² · QB`.
fn screlu_dot(values: &Row, weights: &Row) -> i32 {
    values
        .0
        .iter()
        .zip(&weights.0)
        .map(|(&value, &weight)| {
            let value = value.clamp(0, QA);
            // value · weight fits in an i16 because training clips the output weights
            // to ±1.98, so |weight| ≤ 127 and |value · weight| ≤ 32,385.
            i32::from(value * weight) * i32::from(value)
        })
        .sum()
}

/// One [`Accumulator`] per ply of the current line, heap-allocated, which a search
/// thread owns.
#[derive(Clone, Debug)]
pub struct AccumulatorStack {
    entries: Box<[Accumulator]>,
}

impl AccumulatorStack {
    /// Returns a stack of `plies` entries, each that of the start position until set.
    ///
    /// # Panics
    ///
    /// If `plies` is 0.
    #[must_use]
    pub fn new(plies: usize) -> Self {
        assert!(plies > 0, "an accumulator stack needs at least the root");
        Self {
            entries: vec![Accumulator::new(&Board::startpos()); plies].into_boxed_slice(),
        }
    }

    /// Sets the root entry, at ply 0, to the accumulator of `board`, built from scratch.
    pub fn refresh(&mut self, board: &Board) {
        self.entries[0] = Accumulator::new(board);
    }

    /// Sets the entry at `ply + 1` to the position after `mv` is played in `board`, the
    /// position of the entry at `ply`.
    ///
    /// # Panics
    ///
    /// If `ply + 1` is past the stack's end. `mv` must be pseudo-legal in `board`.
    pub fn make(&mut self, ply: usize, board: &Board, mv: Move) {
        let (parents, children) = self.entries.split_at_mut(ply + 1);
        children[0].make_from(&parents[ply], board, mv);
    }

    /// Sets the entry at `ply + 1` to the position after a null move from that at `ply`,
    /// which places the same pieces.
    ///
    /// # Panics
    ///
    /// If `ply + 1` is past the stack's end.
    pub fn pass(&mut self, ply: usize) {
        let (parents, children) = self.entries.split_at_mut(ply + 1);
        children[0].clone_from(&parents[ply]);
    }

    /// The entry at `ply`.
    ///
    /// # Panics
    ///
    /// If `ply` is past the stack's end.
    #[must_use]
    pub fn get(&self, ply: usize) -> &Accumulator {
        &self.entries[ply]
    }
}
