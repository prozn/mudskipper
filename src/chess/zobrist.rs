//! Random keys for Zobrist hashing of positions.
//!
//! The keys come from a fixed-seed integer generator evaluated at compile time, so every
//! build on every platform hashes positions identically.

use super::castling::CastlingRights;
use super::piece::Piece;
use super::square::Square;

/// Key for `piece` standing on `square`.
pub(super) const fn piece(piece: Piece, square: Square) -> u64 {
    KEYS.pieces[piece.index()][square.index()]
}

/// Key for a castling right held with the rook starting on `rook`.
///
/// The square alone identifies the right, because each colour's rooks start on its own
/// back rank.
const fn castling(rook: Square) -> u64 {
    KEYS.castling[rook.index()]
}

/// Key for the combined castling rights of both colours.
pub(super) fn castling_rights(rights: CastlingRights) -> u64 {
    rights.squares().fold(0, |key, rook| key ^ castling(rook))
}

/// Key for an en passant capture being possible on `file`.
pub(super) const fn en_passant(file: u8) -> u64 {
    KEYS.en_passant[file as usize]
}

/// Key included when black is to move.
pub(super) const fn black_to_move() -> u64 {
    KEYS.black_to_move
}

struct Keys {
    pieces: [[u64; 64]; 12],
    castling: [u64; 64],
    en_passant: [u64; 8],
    black_to_move: u64,
}

/// Seed for the key generator. Changing it changes every key and so the Bench signature.
const SEED: u64 = 0x6D75_6473_6B69_7070;

const KEYS: Keys = {
    let mut rng = SplitMix64(SEED);
    let mut keys = Keys {
        pieces: [[0; 64]; 12],
        castling: [0; 64],
        en_passant: [0; 8],
        black_to_move: 0,
    };
    let mut p = 0;
    while p < 12 {
        let mut sq = 0;
        while sq < 64 {
            keys.pieces[p][sq] = rng.next();
            sq += 1;
        }
        p += 1;
    }
    let mut sq = 0;
    while sq < 64 {
        keys.castling[sq] = rng.next();
        sq += 1;
    }
    let mut file = 0;
    while file < 8 {
        keys.en_passant[file] = rng.next();
        file += 1;
    }
    keys.black_to_move = rng.next();
    keys
};

/// The `SplitMix64` generator: simple, fast and well distributed for seeding tables.
struct SplitMix64(u64);

impl SplitMix64 {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}
