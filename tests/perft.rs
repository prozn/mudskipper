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
