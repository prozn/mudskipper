//! Attack tables: the squares each piece attacks from each square.
//!
//! Leaper and pawn attacks are plain lookups. Sliding attacks use magic bitboards: the
//! blockers on a slider's lines are multiplied by a fixed magic number to index a table of
//! precomputed attack sets. Every table is filled at compile time.

use super::bitboard::Bitboard;
use super::piece::{Colour, PieceType};
use super::square::Square;

/// Squares a pawn of `colour` on `square` attacks.
pub(super) const fn pawn(colour: Colour, square: Square) -> Bitboard {
    PAWN[colour.index()][square.index()]
}

/// Squares the pawns of `colour` in `pawns` attack, together.
pub(super) const fn pawns(colour: Colour, pawns: Bitboard) -> Bitboard {
    const NOT_FILE_A: u64 = !0x0101_0101_0101_0101;
    const NOT_FILE_H: u64 = !0x8080_8080_8080_8080;
    let towards_a = pawns.bits() & NOT_FILE_A;
    let towards_h = pawns.bits() & NOT_FILE_H;
    Bitboard::from_bits(match colour {
        Colour::White => (towards_a << 7) | (towards_h << 9),
        Colour::Black => (towards_a >> 9) | (towards_h >> 7),
    })
}

/// Squares a knight on `square` attacks.
pub(super) const fn knight(square: Square) -> Bitboard {
    KNIGHT[square.index()]
}

/// Squares a king on `square` attacks.
pub(super) const fn king(square: Square) -> Bitboard {
    KING[square.index()]
}

/// Squares a bishop on `square` attacks when `occupied` holds the pieces on the board.
pub(super) fn bishop(square: Square, occupied: Bitboard) -> Bitboard {
    BISHOP_TABLE[BISHOP[square.index()].index(occupied)]
}

/// Squares a rook on `square` attacks when `occupied` holds the pieces on the board.
pub(super) fn rook(square: Square, occupied: Bitboard) -> Bitboard {
    ROOK_TABLE[ROOK[square.index()].index(occupied)]
}

/// Squares a queen on `square` attacks when `occupied` holds the pieces on the board.
pub(super) fn queen(square: Square, occupied: Bitboard) -> Bitboard {
    bishop(square, occupied) | rook(square, occupied)
}

/// Squares a non-pawn piece of type `piece_type` on `square` attacks.
///
/// # Panics
///
/// Panics if `piece_type` is a pawn, whose attacks depend on its colour.
pub(super) fn piece(piece_type: PieceType, square: Square, occupied: Bitboard) -> Bitboard {
    match piece_type {
        PieceType::Knight => knight(square),
        PieceType::Bishop => bishop(square, occupied),
        PieceType::Rook => rook(square, occupied),
        PieceType::Queen => queen(square, occupied),
        PieceType::King => king(square),
        PieceType::Pawn => panic!("pawn attacks depend on the pawn's colour"),
    }
}

/// File and rank steps of each sliding direction.
const ROOK_DIRECTIONS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const BISHOP_DIRECTIONS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const KNIGHT_STEPS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];
const KING_STEPS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// The square `steps` away from `square`, as a set, or the empty set if it is off the board.
const fn offset(square: usize, (file_step, rank_step): (i8, i8)) -> u64 {
    let file = (square % 8) as i8 + file_step;
    let rank = (square / 8) as i8 + rank_step;
    if file >= 0 && file < 8 && rank >= 0 && rank < 8 {
        1 << (rank * 8 + file)
    } else {
        0
    }
}

const fn leaper_table(steps: &[(i8, i8)]) -> [Bitboard; 64] {
    let mut table = [Bitboard::EMPTY; 64];
    let mut square = 0;
    while square < 64 {
        let mut bits = 0;
        let mut i = 0;
        while i < steps.len() {
            bits |= offset(square, steps[i]);
            i += 1;
        }
        table[square] = Bitboard::from_bits(bits);
        square += 1;
    }
    table
}

static KNIGHT: [Bitboard; 64] = leaper_table(&KNIGHT_STEPS);
static KING: [Bitboard; 64] = leaper_table(&KING_STEPS);
static PAWN: [[Bitboard; 64]; 2] = [
    leaper_table(&[(-1, 1), (1, 1)]),
    leaper_table(&[(-1, -1), (1, -1)]),
];

/// Squares a slider on `square` attacks along `directions`, stopping at the first blocker
/// in `occupied` on each line.
const fn slide(square: usize, occupied: u64, directions: [(i8, i8); 4]) -> u64 {
    let mut attacks = 0;
    let mut d = 0;
    while d < 4 {
        let (file_step, rank_step) = directions[d];
        let mut file = (square % 8) as i8 + file_step;
        let mut rank = (square / 8) as i8 + rank_step;
        while file >= 0 && file < 8 && rank >= 0 && rank < 8 {
            let bit = 1 << (rank * 8 + file);
            attacks |= bit;
            if occupied & bit != 0 {
                break;
            }
            file += file_step;
            rank += rank_step;
        }
        d += 1;
    }
    attacks
}

/// The squares whose occupancy can change a slider's attacks from `square`: its lines,
/// without the last square of each, which is attacked whether occupied or not.
const fn relevant_mask(square: usize, directions: [(i8, i8); 4]) -> u64 {
    let mut mask = 0;
    let mut d = 0;
    while d < 4 {
        let step = directions[d];
        let mut bit = offset(square, step);
        // Stop before the last square of the line: it has no square beyond it.
        while bit != 0 && offset(bit.trailing_zeros() as usize, step) != 0 {
            mask |= bit;
            bit = offset(bit.trailing_zeros() as usize, step);
        }
        d += 1;
    }
    mask
}

/// How to find one square's slider attacks in the shared table.
#[derive(Clone, Copy)]
struct Magic {
    mask: u64,
    number: u64,
    shift: u32,
    offset: usize,
}

impl Magic {
    fn index(self, occupied: Bitboard) -> usize {
        self.index_of_blockers(occupied.bits() & self.mask)
    }

    /// Table index for `blockers`, which must lie within the mask.
    const fn index_of_blockers(self, blockers: u64) -> usize {
        self.offset + (blockers.wrapping_mul(self.number) >> self.shift) as usize
    }
}

/// Builds the per-square magic entries, each square's slice of the table following the
/// previous square's.
const fn magics(numbers: &[u64; 64], directions: [(i8, i8); 4]) -> [Magic; 64] {
    let mut magics = [Magic {
        mask: 0,
        number: 0,
        shift: 0,
        offset: 0,
    }; 64];
    let mut offset = 0;
    let mut square = 0;
    while square < 64 {
        let mask = relevant_mask(square, directions);
        magics[square] = Magic {
            mask,
            number: numbers[square],
            shift: 64 - mask.count_ones(),
            offset,
        };
        offset += 1 << mask.count_ones();
        square += 1;
    }
    magics
}

/// Fills a slider table by enumerating every blocker subset of each square's mask.
///
/// # Panics
///
/// Panics at compile time if a magic number maps two subsets with different attacks to
/// the same entry.
const fn slider_table<const N: usize>(
    magics: &[Magic; 64],
    directions: [(i8, i8); 4],
) -> [Bitboard; N] {
    let mut table = [Bitboard::EMPTY; N];
    let mut filled = [false; N];
    let mut square = 0;
    while square < 64 {
        let magic = magics[square];
        let mut subset: u64 = 0;
        loop {
            let index = magic.index_of_blockers(subset);
            let attacks = slide(square, subset, directions);
            assert!(
                !filled[index] || table[index].bits() == attacks,
                "magic number collision"
            );
            table[index] = Bitboard::from_bits(attacks);
            filled[index] = true;
            subset = subset.wrapping_sub(magic.mask) & magic.mask;
            if subset == 0 {
                break;
            }
        }
        square += 1;
    }
    table
}

/// Size of the rook table: the sum over squares of 2 to the number of relevant squares.
const ROOK_TABLE_SIZE: usize = 102_400;
/// Size of the bishop table, counted the same way.
const BISHOP_TABLE_SIZE: usize = 5_248;

static ROOK: [Magic; 64] = magics(&ROOK_MAGICS, ROOK_DIRECTIONS);
static BISHOP: [Magic; 64] = magics(&BISHOP_MAGICS, BISHOP_DIRECTIONS);
static ROOK_TABLE: [Bitboard; ROOK_TABLE_SIZE] = slider_table(&ROOK, ROOK_DIRECTIONS);
static BISHOP_TABLE: [Bitboard; BISHOP_TABLE_SIZE] = slider_table(&BISHOP, BISHOP_DIRECTIONS);

// Magic numbers found offline by a fixed-seed search over sparse random numbers.
// Any number that maps each square's blocker subsets without a harmful collision works;
// changing one changes no behaviour.

const ROOK_MAGICS: [u64; 64] = [
    0x1280_0040_0310_8860,
    0x0040_2000_1000_4000,
    0x0C80_1000_0A80_2002,
    0x0200_040A_0040_2010,
    0x0200_0200_0904_A010,
    0x0600_1002_0008_0401,
    0x0880_0100_3880_0200,
    0x0200_0224_4104_0092,
    0x1008_8001_8840_0620,
    0x2000_4010_0020_0040,
    0x8100_8010_0020_0080,
    0x1824_8010_0281_0801,
    0x1241_0010_0501_0800,
    0x1685_0008_0204_0100,
    0x0084_8001_0080_0200,
    0x0082_0000_4104_00A2,
    0x0040_2280_0040_0090,
    0x0000_4040_1000_2000,
    0x000A_8200_1022_0043,
    0x0400_0900_2010_0102,
    0x2001_0300_0800_1004,
    0x0004_8080_0401_0200,
    0x1000_8080_0200_0100,
    0x0020_0200_00A4_0051,
    0x2814_4101_0028_8000,
    0x4080_5000_4000_2002,
    0x8000_4101_0020_0018,
    0xB420_2042_0008_1200,
    0x8003_0005_0008_0090,
    0x0002_0002_0004_1009,
    0x0011_0021_0004_8200,
    0x0100_0102_0000_8044,
    0x8480_0020_0040_4000,
    0x0140_4000_8880_2000,
    0x0040_8010_0080_2000,
    0x2130_0008_0080_8010,
    0x8152_0010_2200_0804,
    0x4006_0200_8080_0400,
    0x0200_0102_0400_0810,
    0x0902_0280_DA00_0104,
    0x0040_0080_04C3_8022,
    0x0200_5000_2001_4004,
    0x8252_0014_8022_0041,
    0x4110_1001_0021_000C,
    0x0203_0008_0011_0004,
    0x0A8A_0008_1002_0004,
    0x0008_4190_0804_0042,
    0x0080_0080_5502_0024,
    0x0000_8001_C004_AB80,
    0x2030_2004_8240_0180,
    0x01A0_1700_2002_4100,
    0x0001_0800_1000_8280,
    0x0008_0500_0800_1100,
    0x0004_0010_2008_0401,
    0x0020_4830_0291_0400,
    0x0600_8001_0000_4080,
    0x0044_2100_4080_0055,
    0x028B_3102_4002_8021,
    0x2001_4030_4B00_2001,
    0x0020_1000_2004_0901,
    0x0101_0008_0050_2225,
    0xC001_0048_2604_0009,
    0x0403_A820_9001_0234,
    0x0008_0400_6441_0092,
];
const BISHOP_MAGICS: [u64; 64] = [
    0x0204_0800_EC00_8200,
    0x0082_8208_0200_8080,
    0x0B09_1A04_010C_0002,
    0x1008_0A00_2202_8002,
    0x901A_1210_0024_0A04,
    0x4200_8210_4200_0400,
    0x0000_4554_2020_0000,
    0x8612_4200_4120_1010,
    0x0080_4010_0401_5040,
    0x0050_0812_1C14_0020,
    0x0000_5108_0610_4180,
    0x01A4_0407_0202_2042,
    0x0000_0424_2000_0100,
    0x4400_9090_1008_4011,
    0x0001_0400_C208_2000,
    0x0882_0041_0090_1160,
    0x1021_0040_0911_2100,
    0x0085_1420_0822_0040,
    0x0190_0818_0041_4808,
    0x1004_004A_0212_03A8,
    0x4461_0008_2008_0000,
    0x0401_0002_0101_0108,
    0x4012_0000_5842_0804,
    0x0408_20A0_8888_0840,
    0x0404_0600_C008_0800,
    0x4310_3000_0802_4080,
    0x00CE_0800_8480_2400,
    0xCC04_0800_18E0_2040,
    0x0080_8480_0400_2000,
    0x0810_0100_0220_8840,
    0x0103_1300_8408_8820,
    0x0504_0620_0F01_0150,
    0x0850_5090_0008_0208,
    0x4041_1010_1002_A440,
    0x4002_080C_02C2_0120,
    0x0880_8404_0008_0210,
    0x0000_5404_00A0_4100,
    0x8010_1602_8012_3000,
    0x4021_B102_0024_0202,
    0x110C_0242_0411_4108,
    0x0C12_08A0_6400_1823,
    0x0000_6118_1800_2040,
    0x1023_0044_3000_1202,
    0x8400_0440_1048_0200,
    0x4040_4010_0901_1084,
    0x0122_0881_0A00_1300,
    0x0410_0208_0110_0041,
    0x0008_8104_0020_1081,
    0x0010_5210_0420_000A,
    0x0AC9_0082_0120_0000,
    0x0200_0104_0092_0002,
    0xC044_0880_2088_0800,
    0x8000_2106_2082_0024,
    0x0306_1020_4129_1000,
    0x0004_0410_0212_0001,
    0x0208_4128_0091_0000,
    0x0048_1088_0842_1000,
    0x08A0_2080_5108_2031,
    0x0008_0410_2108_0800,
    0x0008_00C4_0284_1400,
    0x4200_84A0_1142_0200,
    0x4504_04C1_0204_0100,
    0x0008_1012_0808_2288,
    0x0512_2038_4200_8020,
];
