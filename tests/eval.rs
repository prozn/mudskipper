//! Tests of the evaluation through the `eval` API. The UCI tests check the scores
//! themselves through the `eval` command.

use mudskipper::chess::Board;
use mudskipper::eval::{Accumulator, AccumulatorStack};
use proptest::prelude::*;
use proptest::sample::Index;

/// Longest random line, in plies.
const MAX_PLIES: usize = 120;

/// Where random lines start: positions with castling both ways, en passant, promotions
/// close at hand, and Chess960 castling where king and rook targets overlap their
/// starting squares.
const STARTS: [&str; 6] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1",
    "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
    "1r2k1r1/pppppppp/8/8/8/8/PPPPPPPP/1R2K1R1 w GBgb - 0 1",
];

fn board(fen: &str) -> Board {
    Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?}: {err}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn incremental_accumulators_match_ones_built_from_scratch(
        start in 0..STARTS.len(),
        // Each step picks a move, or passes instead when the flag is set and the side
        // to move is not in check.
        steps in prop::collection::vec((any::<Index>(), prop::bool::weighted(0.1)), 1..MAX_PLIES),
    ) {
        let mut board = board(STARTS[start]);
        let mut stack = AccumulatorStack::new(MAX_PLIES + 1);
        stack.refresh(&board);
        for (ply, (pick, pass)) in steps.into_iter().enumerate() {
            let moves = board.legal_moves();
            if pass && !board.in_check() {
                stack.pass(ply);
                board = board.pass();
            } else if moves.is_empty() {
                break;
            } else {
                let mv = moves[pick.index(moves.len())];
                stack.make(ply, &board, mv);
                board = board.make(mv);
            }
            prop_assert_eq!(
                stack.get(ply + 1),
                &Accumulator::new(&board),
                "after ply {} in {:?}",
                ply,
                board.to_fen()
            );
        }
    }
}
