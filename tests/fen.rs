//! FEN parsing, printing and Zobrist keys through the `chess` API (Seam 2).

use mudskipper::chess::{Board, Colour, ParseFenError};

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[test]
fn start_position_prints_the_standard_start_fen() {
    assert_eq!(Board::startpos().to_fen(), STARTPOS);
}

/// Standard reference positions, each already in canonical six-field FEN.
const POSITIONS: [&str; 8] = [
    STARTPOS,
    // After 1. e4, with the en passant square recorded.
    "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
    // After 1. e4 c5 2. Nf3.
    "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
    // Kiwipete.
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    // Chess Programming Wiki perft position 3.
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    // Chess Programming Wiki perft position 4.
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    // Chess Programming Wiki perft position 5.
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    // Chess Programming Wiki perft position 6.
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
];

#[test]
fn fen_round_trips_for_standard_positions() {
    for fen in POSITIONS {
        let board =
            Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?} should parse: {err}"));
        assert_eq!(board.to_fen(), fen);
    }
}

#[test]
fn en_passant_square_on_the_sixth_rank_round_trips() {
    // After 1. e4 d5 2. e5 f5: white may capture on f6.
    let fen = "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3";
    assert_eq!(Board::from_fen(fen).map(|b| b.to_fen()), Ok(fen.to_owned()));
}

#[test]
fn missing_clocks_default_to_zero_and_one() {
    let board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -")
        .expect("four-field FEN should parse");
    assert_eq!(board.to_fen(), STARTPOS);
}

#[test]
fn parsing_the_start_fen_gives_the_start_position() {
    assert_eq!(Board::from_fen(STARTPOS), Ok(Board::startpos()));
}

/// Asserts that `fen` is rejected with an error satisfying `is_expected`.
fn assert_rejected(fen: &str, what: &str, is_expected: impl Fn(&ParseFenError) -> bool) {
    match Board::from_fen(fen) {
        Ok(board) => panic!("{fen:?} should be rejected ({what}) but parsed as {board:?}"),
        Err(err) => assert!(
            is_expected(&err),
            "{fen:?} should fail with {what}, got {err:?}"
        ),
    }
}

#[test]
fn fen_with_too_few_or_too_many_fields_is_rejected() {
    assert_rejected("", "no fields", |e| {
        *e == ParseFenError::TooFewFields { found: 0 }
    });
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq",
        "three fields",
        |e| *e == ParseFenError::TooFewFields { found: 3 },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 extra",
        "seven fields",
        |e| *e == ParseFenError::TooManyFields { found: 7 },
    );
}

#[test]
fn fen_with_bad_ranks_is_rejected() {
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "seven ranks",
        |e| *e == ParseFenError::WrongRankCount { found: 7 },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "nine ranks",
        |e| *e == ParseFenError::WrongRankCount { found: 9 },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/7/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "a short sixth rank",
        |e| {
            *e == ParseFenError::WrongRankLength {
                rank: 6,
                squares: 7,
            }
        },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "a long second rank",
        |e| {
            *e == ParseFenError::WrongRankLength {
                rank: 2,
                squares: 9,
            }
        },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR8 w KQkq - 0 1",
        "a far too long first rank",
        |e| {
            *e == ParseFenError::WrongRankLength {
                rank: 1,
                squares: 16,
            }
        },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/44/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "two empty runs in a row",
        |e| *e == ParseFenError::SplitEmptyRun { rank: 4 },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/3x4/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "an unknown piece letter",
        |e| *e == ParseFenError::InvalidPiece { found: 'x' },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/08/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "a zero run",
        |e| *e == ParseFenError::InvalidPiece { found: '0' },
    );
}

#[test]
fn fen_describing_an_impossible_position_is_rejected() {
    let err = Board::from_fen("8/8/8/8/8/8/8/4K3 w - - 0 1").expect_err("no black king");
    assert_eq!(err.to_string(), "black has 0 kings, need exactly 1");
    assert_rejected(
        "rnbq1bnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1",
        "a missing black king",
        |e| {
            *e == ParseFenError::WrongKingCount {
                colour: Colour::Black,
                found: 0,
            }
        },
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKKBNR w kq - 0 1",
        "two white kings",
        |e| {
            *e == ParseFenError::WrongKingCount {
                colour: Colour::White,
                found: 2,
            }
        },
    );
    assert_rejected(
        "4k2P/8/8/8/8/8/8/4K3 w - - 0 1",
        "a pawn on the eighth rank",
        |e| matches!(e, ParseFenError::PawnOnBackRank { square } if square.to_string() == "h8"),
    );
}

#[test]
fn fen_with_bad_side_to_move_is_rejected() {
    for side in ["x", "W", "white"] {
        let fen = format!("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR {side} KQkq - 0 1");
        assert_rejected(&fen, "a bad side to move", |e| {
            *e == ParseFenError::InvalidSideToMove(side.to_owned())
        });
    }
}

#[test]
fn fen_with_bad_castling_is_rejected() {
    let cases = [
        // Not a castling letter.
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQxq - 0 1",
            "KQxq",
        ),
        // A right named twice.
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KKq - 0 1",
            "KKq",
        ),
        // No rook on the king side.
        (
            "rnbqkbn1/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "KQkq",
        ),
        // The king is not on its back rank.
        ("4k3/8/8/8/8/8/4K3/R6R w K - 0 1", "K"),
        // Empty marker mixed with rights.
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w -K - 0 1",
            "-K",
        ),
    ];
    for (fen, field) in cases {
        assert_rejected(fen, "bad castling", |e| {
            *e == ParseFenError::InvalidCastling(field.to_owned())
        });
    }
}

#[test]
fn fen_with_bad_en_passant_is_rejected() {
    let cases = [
        // Not a square.
        (
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e9 0 1",
            "e9",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq z3 0 1",
            "z3",
        ),
        // Wrong rank for the side to move.
        (
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e6 0 1",
            "e6",
        ),
        // No pawn has just skipped the square.
        (
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq d3 0 1",
            "d3",
        ),
        // The pawn's starting square is still occupied.
        (
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPPPPPP/RNBQKBN1 b Qkq e3 0 1",
            "e3",
        ),
    ];
    for (fen, field) in cases {
        assert_rejected(fen, "bad en passant", |e| {
            *e == ParseFenError::InvalidEnPassant(field.to_owned())
        });
    }
}

#[test]
fn fen_with_bad_clocks_is_rejected() {
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - -1 1",
        "a negative halfmove clock",
        |e| *e == ParseFenError::InvalidHalfmoveClock("-1".to_owned()),
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 one",
        "a non-numeric fullmove number",
        |e| *e == ParseFenError::InvalidFullmoveNumber("one".to_owned()),
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - +5 1",
        "a signed halfmove clock",
        |e| *e == ParseFenError::InvalidHalfmoveClock("+5".to_owned()),
    );
}

#[test]
fn non_ascii_fen_is_rejected_without_panicking() {
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq é3 0 1",
        "a non-ASCII en passant field",
        |e| matches!(e, ParseFenError::InvalidEnPassant(_)),
    );
    assert_rejected(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - ٣ 1",
        "a non-ASCII digit clock",
        |e| matches!(e, ParseFenError::InvalidHalfmoveClock(_)),
    );
}

fn key(fen: &str) -> u64 {
    Board::from_fen(fen)
        .unwrap_or_else(|err| panic!("{fen:?} should parse: {err}"))
        .key()
}

#[test]
fn identical_positions_have_identical_keys() {
    for fen in POSITIONS {
        assert_eq!(key(fen), key(fen), "{fen:?}");
    }
    assert_eq!(Board::startpos().key(), key(STARTPOS));
}

#[test]
fn move_clocks_do_not_affect_the_key() {
    assert_eq!(
        key(STARTPOS),
        key("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 37 52")
    );
}

#[test]
fn different_positions_have_different_keys() {
    let mut keys: Vec<(u64, &str)> = POSITIONS.iter().map(|&fen| (key(fen), fen)).collect();
    keys.extend(
        [
            // Startpos with each component changed in turn.
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w Qkq - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w Kkq - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQq - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQk - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKQBNR w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBRN w Qkq - 0 1",
            // White may capture en passant on f6, and the same position without the right.
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq - 0 3",
            // As above, but the capture is on d6.
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
        ]
        .map(|fen| (key(fen), fen)),
    );
    for (i, (a, fen_a)) in keys.iter().enumerate() {
        for (b, fen_b) in &keys[i + 1..] {
            assert_ne!(a, b, "{fen_a:?} and {fen_b:?} share a key");
        }
    }
}

#[test]
fn en_passant_square_without_a_capturing_pawn_does_not_affect_the_key() {
    // After 1. e4 no black pawn can capture on e3, so the position is the same as without it.
    assert_eq!(
        key("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"),
        key("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"),
    );
}
