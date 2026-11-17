//! Property tests over random legal games through the `chess` API (Seam 2).

use mudskipper::chess::{Board, CastlingNotation};
use proptest::prelude::*;
use proptest::sample::Index;

/// Longest random game, in plies.
const MAX_PLIES: usize = 200;

/// Where random games start, with the castling notations that read every legal move
/// unambiguously there. Standard notation can be ambiguous in Chess960 positions.
const STARTS: [(&str, &[CastlingNotation]); 5] = [
    (
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        &[CastlingNotation::Standard, CastlingNotation::KingTakesRook],
    ),
    (
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        &[CastlingNotation::Standard, CastlingNotation::KingTakesRook],
    ),
    (
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        &[CastlingNotation::Standard, CastlingNotation::KingTakesRook],
    ),
    (
        "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        &[CastlingNotation::KingTakesRook],
    ),
    (
        "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
        &[CastlingNotation::KingTakesRook],
    ),
];

/// Checks every invariant that must hold in any position reached by legal moves.
fn check_position(board: &Board, notations: &[CastlingNotation]) -> Result<(), TestCaseError> {
    let fen = board.to_fen();
    let reparsed =
        Board::from_fen(&fen).map_err(|err| TestCaseError::fail(format!("{fen:?}: {err}")))?;
    // Checked before the boards, whose equality includes the key, for a clearer failure.
    prop_assert_eq!(
        board.key(),
        reparsed.key(),
        "incremental key differs from the key computed from {:?}",
        fen
    );
    prop_assert_eq!(&reparsed, board, "FEN {:?} does not round-trip", fen);

    let shredder = board.to_shredder_fen();
    let reparsed = Board::from_fen(&shredder)
        .map_err(|err| TestCaseError::fail(format!("{shredder:?}: {err}")))?;
    prop_assert_eq!(
        &reparsed,
        board,
        "Shredder-FEN {:?} does not round-trip",
        shredder
    );

    for &mv in &board.legal_moves() {
        for &notation in notations {
            let text = mv.display(notation).to_string();
            prop_assert_eq!(
                board.parse_move(&text, notation),
                Ok(mv),
                "{} in {:?} with {:?}",
                text,
                fen,
                notation
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_legal_games_keep_keys_fen_and_uci_notation_consistent(
        start in 0..STARTS.len(),
        choices in prop::collection::vec(any::<Index>(), 0..=MAX_PLIES),
    ) {
        let (fen, notations) = STARTS[start];
        let mut board = Board::from_fen(fen).expect("start FENs are valid");
        check_position(&board, notations)?;
        for choice in choices {
            let moves = board.legal_moves();
            if moves.is_empty() {
                break;
            }
            board = board.make(*choice.get(&moves));
            check_position(&board, notations)?;
        }
    }

    #[test]
    fn passing_keeps_the_incremental_key_equal_to_the_computed_key(
        start in 0..STARTS.len(),
        choices in prop::collection::vec(any::<Index>(), 0..=MAX_PLIES),
    ) {
        let (fen, _) = STARTS[start];
        let mut board = Board::from_fen(fen).expect("start FENs are valid");
        for choice in choices {
            if !board.in_check() {
                check_pass(&board)?;
            }
            let moves = board.legal_moves();
            if moves.is_empty() {
                break;
            }
            board = board.make(*choice.get(&moves));
        }
    }
}

/// Checks that passing in `board` gives the other side the move with no en passant
/// capture, and keeps the incremental key equal to the key computed from scratch.
fn check_pass(board: &Board) -> Result<(), TestCaseError> {
    let passed = board.pass();
    let fen = passed.to_fen();
    let reparsed =
        Board::from_fen(&fen).map_err(|err| TestCaseError::fail(format!("{fen:?}: {err}")))?;
    prop_assert_eq!(
        passed.key(),
        reparsed.key(),
        "incremental key after passing in {:?} differs from the key computed from {:?}",
        board.to_fen(),
        fen
    );
    prop_assert_eq!(passed.side_to_move(), !board.side_to_move());
    prop_assert_eq!(passed.en_passant(), None);
    Ok(())
}
