//! Static exchange evaluation through the `chess` API.

use mudskipper::chess::{Board, CastlingNotation};

/// Checks that `mv`, written in standard UCI notation, wins exactly `expected` in the
/// position `fen`: at least `expected` and not at least one more.
fn assert_see(fen: &str, mv: &str, expected: i32) {
    let board = Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?}: {err}"));
    let mv = board
        .parse_move(mv, CastlingNotation::Standard)
        .unwrap_or_else(|err| panic!("{mv} in {fen}: {err}"));
    assert!(
        board.see_ge(mv, expected),
        "{mv} in {fen} should win at least {expected}"
    );
    assert!(
        !board.see_ge(mv, expected + 1),
        "{mv} in {fen} should win no more than {expected}"
    );
}

#[test]
fn capturing_an_undefended_piece_wins_it() {
    assert_see("4k3/8/8/3p4/8/8/8/3QK3 w - - 0 1", "d1d5", 100);
}

#[test]
fn capturing_a_defended_piece_with_a_more_valuable_one_loses_the_difference() {
    assert_see("4k3/8/2p5/3p4/8/8/8/3QK3 w - - 0 1", "d1d5", -800);
}

#[test]
fn trading_equal_pieces_breaks_even() {
    assert_see("4k3/8/2p5/3n4/8/4N3/8/4K3 w - - 0 1", "e3d5", 0);
}

#[test]
fn the_side_to_recapture_stops_when_recapturing_would_lose() {
    // Black's queen could take the rook back, but the knight would then win the queen,
    // so black leaves the rook and white is a pawn up.
    assert_see("3qk3/8/8/3p4/8/4N3/8/3RK3 w - - 0 1", "d1d5", 100);
}

#[test]
fn a_rook_behind_the_capturer_recaptures_through_it() {
    // Rxd5 Rxd5 Rxd5: the rook on d1 x-rays through the one on d2.
    assert_see("3rk3/8/8/3p4/8/8/3R4/3RK3 w - - 0 1", "d2d5", 100);
}

#[test]
fn a_queen_behind_the_capturer_recaptures_along_the_diagonal() {
    // Bxe5 Nxe5 Qxe5: the queen on a1 x-rays through the bishop on b2.
    assert_see("4k3/5n2/8/4p3/8/8/1B6/Q3K3 w - - 0 1", "b2e5", 100);
}

#[test]
fn a_pawn_recapture_uncovers_a_bishop_behind_it() {
    // Nxd4 cxd4 and white stops: Rxd4 would lose the rook to the bishop on b6.
    assert_see("4k3/8/1b6/2p5/3n4/8/4N3/3RK3 w - - 0 1", "e2d4", 0);
}

#[test]
fn an_x_ray_attacker_behind_a_defender_joins_in_too() {
    // Rxd5 Rxd5 Rxd5 Rxd5: black's second rook backs up its first, and white loses two
    // rooks for a rook and a pawn.
    assert_see("3rk3/3r4/8/3p4/8/8/3R4/3RK3 w - - 0 1", "d2d5", -400);
}

#[test]
fn a_king_does_not_recapture_onto_a_defended_square() {
    assert_see("8/8/4k3/3p4/8/8/3Q4/3RK3 w - - 0 1", "d2d5", 100);
}

#[test]
fn a_king_recaptures_onto_an_undefended_square() {
    assert_see("8/8/4k3/3p4/8/8/3Q4/4K3 w - - 0 1", "d2d5", -800);
}

#[test]
fn en_passant_wins_the_pawn_it_takes() {
    assert_see("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6", 100);
    assert_see("4k3/2p5/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6", 0);
}

#[test]
fn en_passant_uncovers_an_attacker_behind_the_pawn_it_takes() {
    // Taking the d5 pawn opens the d-file for white's rook, so black's rook cannot take
    // the pawn on d6 without losing itself.
    assert_see("3rk3/8/8/3pP3/8/8/8/3RK3 w - d6 0 1", "e5d6", 100);
}

#[test]
fn promoting_wins_the_new_piece_less_the_pawn() {
    assert_see("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7a8q", 800);
    assert_see("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1", "b7a8q", 1300);
}

#[test]
fn promoting_onto_a_defended_square_loses_the_new_piece() {
    assert_see("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1", "b7b8q", -100);
    assert_see("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1", "b7b8n", -100);
}

#[test]
fn castling_neither_wins_nor_loses() {
    assert_see("4k3/8/8/8/8/8/8/4K2R w K - 0 1", "e1g1", 0);
}
