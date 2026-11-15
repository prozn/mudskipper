//! Draw rules through the `chess` API (Seam 2).

use mudskipper::chess::{Board, CastlingNotation, KeyHistory};

fn board(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?}: {err}"))
}

/// Plays `moves`, written in standard UCI notation, from `fen`.
fn play(fen: &str, moves: &[&str]) -> Board {
    moves.iter().fold(board(fen), |board, text| {
        let mv = board
            .parse_move(text, CastlingNotation::Standard)
            .unwrap_or_else(|err| panic!("{text} in {}: {err}", board.to_fen()));
        board.make(mv)
    })
}

/// Plays `moves` from `fen` as a game, returning the final position and the keys of the
/// positions before it.
fn play_game(fen: &str, moves: &[&str]) -> (Board, KeyHistory) {
    let mut history = KeyHistory::new();
    let board = moves.iter().fold(board(fen), |board, text| {
        history.push(board.key());
        play(&board.to_fen(), &[text])
    });
    (board, history)
}

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[test]
fn fifty_move_rule_applies_from_the_hundredth_halfmove() {
    assert!(!board("8/8/4k3/8/8/4K3/8/R7 w - - 99 80").is_fifty_move_draw());
    assert!(board("8/8/4k3/8/8/4K3/8/R7 w - - 100 80").is_fifty_move_draw());
    assert!(play("8/8/4k3/8/8/4K3/8/R7 w - - 99 80", &["a1a2"]).is_fifty_move_draw());
}

#[test]
fn captures_and_pawn_moves_reset_the_fifty_move_count() {
    let fen = "8/8/4k3/8/2p5/5K2/4P3/R7 w - - 99 80";
    let quiet = play(fen, &["a1a2"]);
    assert_eq!(quiet.halfmove_clock(), 100);
    assert!(quiet.is_fifty_move_draw());

    let pawn_move = play(fen, &["e2e4"]);
    assert_eq!(pawn_move.halfmove_clock(), 0);
    assert!(!pawn_move.is_fifty_move_draw());

    let capture = play(fen, &["a1a4", "e6d5", "a4c4"]);
    assert_eq!(capture.halfmove_clock(), 0);
    assert!(!capture.is_fifty_move_draw());
}

#[test]
fn checkmate_on_the_hundredth_halfmove_is_not_a_fifty_move_draw() {
    let mated = play("k7/8/1K6/8/8/8/8/7R w - - 99 80", &["h1h8"]);
    assert!(mated.in_check() && mated.legal_moves().is_empty());
    assert!(!mated.is_fifty_move_draw());

    let checked = play("k7/8/8/1K6/8/8/8/7R w - - 99 80", &["h1h8"]);
    assert!(checked.in_check());
    assert!(checked.is_fifty_move_draw());
}

#[test]
fn bare_kings_and_a_single_minor_piece_are_insufficient_material() {
    for fen in [
        "8/8/4k3/8/8/4K3/8/8 w - - 0 1",
        "8/8/4k3/8/8/4K3/8/1N6 w - - 0 1",
        "8/8/4k3/8/8/4K3/8/2b5 b - - 0 1",
    ] {
        assert!(board(fen).has_insufficient_material(), "{fen:?}");
    }
}

#[test]
fn bishops_all_on_one_square_colour_are_insufficient_material() {
    for fen in [
        "8/8/4k3/8/8/4K3/8/2B1b3 w - - 0 1",
        "8/8/4k1b1/8/8/4K3/8/1B1B4 w - - 0 1",
    ] {
        assert!(board(fen).has_insufficient_material(), "{fen:?}");
    }
}

#[test]
fn material_that_can_still_mate_is_sufficient() {
    for (fen, why) in [
        ("8/8/4k3/8/8/4K3/4P3/8 w - - 0 1", "a pawn"),
        ("8/8/4k3/8/8/4K3/8/R7 w - - 0 1", "a rook"),
        ("8/8/4k3/8/8/4K3/8/3q4 w - - 0 1", "a queen"),
        ("8/8/4k3/8/8/4K3/8/1N4N1 w - - 0 1", "two knights"),
        (
            "8/8/4k3/8/8/4K3/8/1Nb5 w - - 0 1",
            "a knight against a bishop",
        ),
        (
            "8/8/4k3/8/8/4K3/8/2B2b2 w - - 0 1",
            "bishops on both colours",
        ),
        ("8/8/4k3/8/8/4K3/8/2BB4 w - - 0 1", "a bishop pair"),
    ] {
        assert!(!board(fen).has_insufficient_material(), "{why}: {fen:?}");
    }
}

#[test]
fn shuffling_knights_back_repeats_the_position() {
    let (board, history) = play_game(STARTPOS, &[]);
    assert_eq!(history.repetitions(&board), 0);

    let (board, history) = play_game(STARTPOS, &["g1f3", "g8f6", "f3g1"]);
    assert_eq!(history.repetitions(&board), 0);

    let (board, history) = play_game(STARTPOS, &["g1f3", "g8f6", "f3g1", "f6g8"]);
    assert_eq!(history.repetitions(&board), 1);

    let (board, history) = play_game(
        STARTPOS,
        &[
            "g1f3", "g8f6", "f3g1", "f6g8", "b1c3", "b8c6", "c3b1", "c6b8",
        ],
    );
    assert_eq!(history.repetitions(&board), 2);
}

#[test]
fn the_same_placement_with_the_other_side_to_move_is_not_a_repetition() {
    // White's king walks a triangle while Black's goes there and back, so the pieces
    // return to their squares with Black to move.
    let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
    let (board, history) = play_game(fen, &["e1d1", "e8d8", "d1d2", "d8e8", "d2e1"]);
    assert_eq!(board.to_fen().split(' ').next(), fen.split(' ').next());
    assert_eq!(history.repetitions(&board), 0);
}

#[test]
fn lost_castling_rights_make_a_new_position() {
    let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
    let (board, history) = play_game(fen, &["e1f1", "e8f8", "f1e1", "f8e8"]);
    assert_eq!(history.repetitions(&board), 0);
    let (board, history) = play_game(
        fen,
        &[
            "e1f1", "e8f8", "f1e1", "f8e8", "e1f1", "e8f8", "f1e1", "f8e8",
        ],
    );
    assert_eq!(history.repetitions(&board), 1);
}

#[test]
fn positions_before_an_irreversible_move_are_not_compared() {
    // The history holds the key of this very position, but the halfmove clock says a
    // capture or pawn move came after it, so it cannot be the same game position.
    let fen = "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1";
    let board = board(fen);
    let mut history = KeyHistory::new();
    history.push(board.key());
    history.push(Board::startpos().key());
    assert_eq!(history.repetitions(&board), 0);

    let reversible = Board::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 2 1").expect("valid FEN");
    assert_eq!(history.repetitions(&reversible), 1);
}

#[test]
fn a_pawn_move_between_shuffles_ends_the_repetition_count() {
    let (board, history) = play_game(
        STARTPOS,
        &[
            "g1f3", "g8f6", "f3g1", "f6g8", "e2e4", "e7e5", "g1f3", "g8f6", "f3g1", "f6g8",
        ],
    );
    assert_eq!(history.repetitions(&board), 1);
}

#[test]
fn popping_a_key_forgets_that_position() {
    let (board, mut history) = play_game(STARTPOS, &["g1f3", "g8f6", "f3g1", "f6g8"]);
    assert_eq!(
        history.pop(),
        Some(play(STARTPOS, &["g1f3", "g8f6", "f3g1"]).key())
    );
    history.pop();
    history.pop();
    history.pop();
    assert_eq!(history.repetitions(&board), 0);
    assert_eq!(history.pop(), None);
}
