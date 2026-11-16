//! Move generation, making moves and UCI move notation through the `chess` API (Seam 2).

use mudskipper::chess::{Board, CastlingNotation, Key, MoveList, ParseMoveError};

/// The quick Perft suite positions, plus one with an en passant capture available.
const POSITIONS: [&str; 8] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
];

fn boards() -> impl Iterator<Item = Board> {
    POSITIONS
        .into_iter()
        .map(|fen| Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?}: {err}")))
}

/// Parses the board's own FEN, which computes its key from scratch.
fn key_from_scratch(board: &Board) -> Key {
    Board::from_fen(&board.to_fen())
        .expect("printed FEN parses")
        .key()
}

#[test]
fn incremental_key_matches_key_from_scratch_two_plies_deep() {
    for board in boards() {
        for &first in &board.legal_moves() {
            let child = board.make(first);
            assert_eq!(
                child.key(),
                key_from_scratch(&child),
                "after {first} from {}",
                board.to_fen()
            );
            for &second in &child.legal_moves() {
                let grandchild = child.make(second);
                assert_eq!(
                    grandchild.key(),
                    key_from_scratch(&grandchild),
                    "after {first} {second} from {}",
                    board.to_fen()
                );
            }
        }
    }
}

#[test]
fn is_pseudo_legal_agrees_with_generation_for_moves_from_every_position() {
    let boards: Vec<Board> = boards().collect();
    let mut rejected = 0;
    for board in &boards {
        let generated = board.pseudo_legal_moves();
        for other in &boards {
            for &mv in &other.pseudo_legal_moves() {
                let expected = generated.contains(&mv);
                assert_eq!(
                    board.is_pseudo_legal(mv),
                    expected,
                    "{mv} from {} in {}",
                    other.to_fen(),
                    board.to_fen()
                );
                if !expected {
                    rejected += 1;
                }
            }
        }
    }
    assert!(rejected > 0, "no foreign move was rejected");
}

#[test]
fn noisy_moves_are_the_captures_and_promotions_among_all_moves() {
    for board in boards() {
        let noisy: Vec<String> = board
            .noisy_moves()
            .iter()
            .map(ToString::to_string)
            .collect();
        let expected: Vec<String> = board
            .pseudo_legal_moves()
            .iter()
            .filter(|mv| mv.is_noisy())
            .map(ToString::to_string)
            .collect();
        let mut sorted_noisy = noisy.clone();
        sorted_noisy.sort();
        let mut sorted_expected = expected;
        sorted_expected.sort();
        assert_eq!(sorted_noisy, sorted_expected, "in {}", board.to_fen());
    }
}

#[test]
fn quiet_moves_are_the_moves_that_neither_capture_nor_promote() {
    let mut castling_seen = false;
    for board in boards() {
        let quiet = board.quiet_moves();
        castling_seen |= quiet.iter().any(|mv| mv.is_castling());
        let mut got: Vec<String> = quiet.iter().map(ToString::to_string).collect();
        let mut expected: Vec<String> = board
            .pseudo_legal_moves()
            .iter()
            .filter(|mv| !mv.is_noisy())
            .map(ToString::to_string)
            .collect();
        got.sort();
        expected.sort();
        assert_eq!(got, expected, "in {}", board.to_fen());
    }
    assert!(castling_seen, "no position offered a quiet castling move");
}

#[test]
fn kiwipete_has_eight_captures_and_both_castling_moves() {
    let board = Board::from_fen(POSITIONS[1]).expect("Kiwipete parses");
    let captures = board
        .legal_moves()
        .iter()
        .filter(|mv| mv.is_capture())
        .count();
    assert_eq!(captures, 8);
    let castling: Vec<String> = board
        .legal_moves()
        .iter()
        .filter(|mv| mv.is_castling())
        .map(ToString::to_string)
        .collect();
    assert_eq!(castling, ["e1g1", "e1c1"]);
}

#[test]
fn every_legal_move_round_trips_through_uci_notation() {
    for board in boards() {
        for &mv in &board.legal_moves() {
            assert_eq!(
                board.parse_move(&mv.to_string(), CastlingNotation::Standard),
                Ok(mv),
                "in {}",
                board.to_fen()
            );
        }
    }
}

#[test]
fn parse_move_rejects_malformed_and_illegal_moves() {
    let board = Board::startpos();
    for text in ["", "e2", "e2e4x", "e2e9", "i2i4", "e2e4 ", "e2e\u{e9}"] {
        assert_eq!(
            board.parse_move(text, CastlingNotation::Standard),
            Err(ParseMoveError::Malformed(text.to_owned()))
        );
    }
    for text in ["e2e5", "e1g1", "e7e5", "e2e4q"] {
        assert_eq!(
            board.parse_move(text, CastlingNotation::Standard),
            Err(ParseMoveError::Illegal(text.to_owned()))
        );
    }
}

#[test]
fn a_pinned_knight_has_pseudo_legal_moves_but_no_legal_ones() {
    // The e8 rook pins the e2 knight to the white king.
    let board = Board::from_fen("4r1k1/8/8/8/8/8/4N3/4K3 w - - 0 1").expect("valid FEN");
    let from_e2 = |moves: &MoveList| {
        moves
            .iter()
            .filter(|mv| mv.from().to_string() == "e2")
            .count()
    };
    assert_eq!(from_e2(&board.pseudo_legal_moves()), 6);
    assert_eq!(from_e2(&board.legal_moves()), 0);
}

#[test]
fn a_double_push_records_the_same_position_as_its_fen() {
    let after_e4 = Board::startpos().make(
        Board::startpos()
            .parse_move("e2e4", CastlingNotation::Standard)
            .expect("legal"),
    );
    let parsed = Board::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1")
        .expect("valid FEN");
    assert_eq!(after_e4, parsed);
}

#[test]
fn in_check_reports_whether_the_side_to_move_is_attacked() {
    assert!(!Board::startpos().in_check());
    let checked = Board::from_fen("4k3/8/8/8/8/8/8/4K2r w - - 0 1").expect("valid FEN");
    assert!(checked.in_check());
}

#[test]
fn every_legal_move_round_trips_through_king_takes_rook_notation() {
    let chess960 = [
        "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        "1r4kr/8/8/8/8/8/8/1R4KR w HBhb - 0 1",
        "rkr5/8/8/8/8/8/8/RKR5 w CAca - 0 1",
    ]
    .map(|fen| Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?}: {err}")));
    for board in boards().chain(chess960) {
        for &mv in &board.legal_moves() {
            let text = mv.display(CastlingNotation::KingTakesRook).to_string();
            assert_eq!(
                board.parse_move(&text, CastlingNotation::KingTakesRook),
                Ok(mv),
                "{text} in {}",
                board.to_fen()
            );
        }
    }
}

#[test]
fn castling_with_the_king_already_on_its_castling_square_is_written_king_takes_rook() {
    let board = Board::from_fen("1r4kr/8/8/8/8/8/8/1R4KR w HBhb - 0 1").expect("valid FEN");
    let castling: Vec<String> = board
        .legal_moves()
        .iter()
        .filter(|mv| mv.is_castling())
        .map(|mv| mv.display(CastlingNotation::KingTakesRook).to_string())
        .collect();
    assert_eq!(castling, ["g1h1", "g1b1"]);
    let after = board.make(
        board
            .parse_move("g1h1", CastlingNotation::KingTakesRook)
            .expect("castling is legal"),
    );
    assert_eq!(
        after.to_shredder_fen(),
        "1r4kr/8/8/8/8/8/8/1R3RK1 b hb - 1 1"
    );
}
