//! Perft counts through the `chess` API (Seam 2), against published reference counts.

use mudskipper::chess::Board;

fn board(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?} should parse: {err}"))
}

/// Checks the perft count of `fen` at every depth from 1 up to `counts.len()`.
fn assert_perft(fen: &str, counts: &[u64]) {
    let board = board(fen);
    for (depth, &want) in (1..).zip(counts) {
        assert_eq!(board.perft(depth), want, "perft {depth} of {fen:?}");
    }
}

#[test]
fn start_position() {
    assert_perft(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        &[20, 400, 8_902, 197_281],
    );
}

#[test]
fn kiwipete() {
    assert_perft(
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        &[48, 2_039, 97_862],
    );
}

#[test]
fn chess_programming_wiki_position_3() {
    assert_perft(
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        &[14, 191, 2_812, 43_238, 674_624],
    );
}

#[test]
fn chess_programming_wiki_position_4() {
    assert_perft(
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        &[6, 264, 9_467, 422_333],
    );
}

#[test]
fn chess_programming_wiki_position_4_mirrored() {
    assert_perft(
        "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
        &[6, 264, 9_467, 422_333],
    );
}

#[test]
fn chess_programming_wiki_position_5() {
    assert_perft(
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        &[44, 1_486, 62_379],
    );
}

#[test]
fn chess_programming_wiki_position_6() {
    assert_perft(
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        &[46, 2_079, 89_890],
    );
}

/// Martin Sedlak's positions, each aimed at one rule, with the count at one depth.
const EDGE_CASES: [(&str, u32, u64); 15] = [
    // En passant that would expose the king along a diagonal or rank.
    ("8/8/1k6/2b5/2pP4/8/5K2/8 b - d3 0 1", 6, 1_440_467),
    ("8/5bk1/8/2Pp4/8/1K6/8/8 w - d6 0 1", 6, 824_064),
    ("3k4/3p4/8/K1P4r/8/8/8/8 b - - 0 1", 6, 1_134_888),
    ("8/8/4k3/8/2p5/8/B2P2K1/8 w - - 0 1", 6, 1_015_133),
    // Castling: losing rights, castling out of and through check.
    ("5k2/8/8/8/8/8/8/4K2R w K - 0 1", 6, 661_072),
    ("3k4/8/8/8/8/8/8/R3K3 w Q - 0 1", 6, 803_711),
    ("r3k2r/1b4bq/8/8/8/8/7B/R3K2R w KQkq - 0 1", 4, 1_274_206),
    ("r3k2r/8/3Q4/8/8/5q2/8/R3K2R b KQkq - 0 1", 4, 1_720_476),
    // Promotions, with and without capture, and stalemate traps.
    ("2K2r2/4P3/8/8/8/8/8/3k4 w - - 0 1", 6, 3_821_001),
    ("4k3/1P6/8/8/8/8/K7/8 w - - 0 1", 6, 217_342),
    ("8/P1k5/K7/8/8/8/8/8 w - - 0 1", 6, 92_683),
    ("K1k5/8/P7/8/8/8/8/8 w - - 0 1", 6, 2_217),
    ("8/k1P5/8/1K6/8/8/8/8 w - - 0 1", 7, 567_584),
    // Knight and queen checks against a lone king.
    ("8/8/2k5/5q2/5n2/8/5K2/8 b - - 0 1", 4, 23_527),
    ("8/8/1P2K3/8/2n5/1q6/8/5k2 b - - 0 1", 5, 1_004_658),
];

#[test]
fn edge_cases() {
    for (fen, depth, want) in EDGE_CASES {
        assert_eq!(board(fen).perft(depth), want, "perft {depth} of {fen:?}");
    }
}

/// Deeper counts for the reference positions. Run by hand with `cargo test --release --
/// --ignored`.
#[test]
#[ignore = "slow: run by hand in release mode"]
fn deep_suite() {
    for (fen, depth, want) in [
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            6,
            119_060_324,
        ),
        (
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            5,
            193_690_690,
        ),
        ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 7, 178_633_661),
        (
            "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
            5,
            15_833_292,
        ),
        (
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
            5,
            89_941_194,
        ),
        (
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
            5,
            164_075_551,
        ),
    ] {
        assert_eq!(board(fen).perft(depth), want, "perft {depth} of {fen:?}");
    }
}

/// Chess960 positions in Shredder-FEN, with counts from the published Chess960 Perft suite
/// (the first three) and python-chess.
const CHESS960: [(&str, &[u64]); 13] = [
    (
        "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        &[21, 528, 12_189, 326_672],
    ),
    (
        "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
        &[21, 807, 18_002, 667_366],
    ),
    (
        "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
        &[20, 479, 10_471, 273_318],
    ),
    (
        "qbbnnrkr/2pp2pp/p7/1p2pp2/8/P3PP2/1PPP1KPP/QBBNNR1R w hf - 0 9",
        &[22, 593, 13_440, 382_958],
    ),
    (
        "1nbbnrkr/p1p1ppp1/3p4/1p3P1p/3Pq2P/8/PPP1P1P1/QNBBNRKR w HFhf - 0 9",
        &[28, 1_120, 31_058, 1_171_749],
    ),
    (
        "qnbnr1kr/ppp1b1pp/4p3/3p1p2/8/2NPP3/PPP1BPPP/QNB1R1KR w HEhe - 1 9",
        &[29, 899, 26_578, 824_055],
    ),
    (
        "1qbnrbkr/1p1ppppp/8/p1p5/5P2/P2P4/1PP1P1PP/QNBNRBKR w HEhe - 0 9",
        &[22, 484, 11_480, 277_517],
    ),
    (
        "bnrqkbrn/pppppppp/8/8/8/8/PPPPPPPP/BNRQKBRN w GCgc - 0 1",
        &[19, 361, 7_697, 163_313],
    ),
    // Rooks on both sides of each king, with the king on its castling square or not.
    (
        "2r1kr2/8/8/8/8/8/8/1R2K1R1 w GBfc - 0 1",
        &[22, 501, 11_459, 264_663],
    ),
    // Kings on b1 and b8 castle queen side with the rook already beside them.
    (
        "rk2r3/8/8/8/8/8/8/RK2R3 w Aa - 0 1",
        &[23, 441, 9_870, 210_332],
    ),
    // Kings on c1 and c8 with a castling rook on the g- and h-file.
    (
        "1rk4r/8/8/8/8/8/8/1RK3R1 w GBhb - 0 1",
        &[24, 487, 11_415, 250_698],
    ),
    // Kings already on g1 and g8, so king side castling moves only the rook.
    (
        "1r4kr/8/8/8/8/8/8/1R4KR w HBhb - 0 1",
        &[24, 479, 10_954, 237_166],
    ),
    // Kings on b1 and b8 between their rooks: the king side rook lands on the f-file past
    // the king, and the queen side rook lands on the d-file.
    (
        "rkr5/8/8/8/8/8/8/RKR5 w CAca - 0 1",
        &[20, 328, 6_855, 138_584],
    ),
];

#[test]
fn chess960_positions() {
    for (fen, counts) in CHESS960 {
        assert_perft(fen, counts);
    }
}

/// Deeper counts from the published Chess960 Perft suite. Run by hand with `cargo test
/// --release -- --ignored`.
#[test]
#[ignore = "slow: run by hand in release mode"]
fn chess960_deep_suite() {
    for (fen, depth, want) in [
        (
            "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
            6,
            227_689_589,
        ),
        (
            "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
            6,
            590_751_109,
        ),
        (
            "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
            6,
            177_654_692,
        ),
    ] {
        assert_eq!(board(fen).perft(depth), want, "perft {depth} of {fen:?}");
    }
}
