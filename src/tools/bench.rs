//! **Bench**: a fixed, deterministic search whose node count is the **Bench signature**.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::chess::{Board, Game};
use crate::search::{Limits, StopFlag, Worker};

/// Depth each position is searched to when `bench` is given no depth. Chosen so that a
/// release build takes a few seconds.
pub const DEFAULT_DEPTH: u32 = 5;

/// The positions Bench searches: openings, middlegames, endgames and some **Chess960**
/// positions. Changing them changes the **Bench signature**.
const POSITIONS: [&str; 50] = [
    // Openings and early middlegames.
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "rnbqkb1r/pppp1ppp/5n2/4p3/2B1P3/8/PPPP1PPP/RNBQK1NR w KQkq - 2 3",
    "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
    "rnbqkb1r/pp2pppp/3p1n2/8/3NP3/8/PPP2PPP/RNBQKB1R w KQkq - 1 5",
    "rnbqk2r/ppp1bppp/4pn2/3p2B1/2PP4/2N5/PP2PPPP/R2QKBNR w KQkq - 4 5",
    "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQK2R b KQkq - 0 5",
    "rnbq1rk1/ppp1ppbp/3p1np1/8/2PPP3/2N2N2/PP3PPP/R1BQKB1R w KQ - 1 6",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 10",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    // Middlegames.
    "4rrk1/pp1n3p/3q2pQ/2p1pb2/2PP4/2P3N1/P2B2PP/4RRK1 b - - 7 19",
    "rq3rk1/ppp2ppp/1bnpb3/3N2B1/3NP3/7P/PPPQ1PP1/2KR3R w - - 7 14",
    "r1bq1r1k/1pp1n1pp/1p1p4/4p2Q/4Pp2/1BNP4/PPP2PPP/3R1RK1 w - - 2 14",
    "r3r1k1/2p2ppp/p1p1bn2/8/1q2P3/2NPQN2/PPP3PP/R4RK1 b - - 2 15",
    "r1bbk1nr/pp3p1p/2n5/1N4p1/2Np1B2/8/PPP2PPP/2KR1B1R w kq - 0 13",
    "r1bq1rk1/ppp1nppp/4n3/3p3Q/3P4/1BP1B3/PP1N2PP/R4RK1 w - - 1 16",
    "4r1k1/r1q2ppp/ppp2n2/4P3/5Rb1/1N1BQ3/PPP3PP/R5K1 w - - 1 17",
    "2rqkb1r/ppp2p2/2npb1p1/1N1Nn2p/2P1PP2/8/PP2B1PP/R1BQK2R b KQ - 0 11",
    "r1bq1r1k/b1p1npp1/p2p3p/1p6/3PP3/1B2NN2/PP3PPP/R2Q1RK1 w - - 1 16",
    "3r1rk1/p5pp/bpp1pp2/8/q1PP1P2/b3P3/P2NQRPP/1R2B1K1 b - - 6 22",
    "r1q2rk1/2p1bppp/2Pp4/p6b/Q1PNp3/4B3/PP1R1PPP/2K4R w - - 2 18",
    "4k2r/1pb2ppp/1p2p3/1R1p4/3P4/2r1PN2/P4PPP/1R4K1 b - - 3 22",
    "3q2k1/pb3p1p/4pbp1/2r5/PpN2N2/1P2P2P/5PP1/Q2R2K1 b - - 4 26",
    "6k1/3b3r/1p1p4/p1n2p2/1PPNpP1q/P3Q1p1/1R1RB1P1/5K2 b - - 0 1",
    "r2r1n2/pp2bk2/2p1p2p/3q4/3PN1QP/2P3R1/P4PP1/5RK1 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
    "2r2rk1/1bqnbppp/pp1ppn2/8/2PNP3/1PN1B3/P1Q1BPPP/2RR2K1 w - - 4 14",
    "r4rk1/pp1b1ppp/2n1pn2/q1bp4/2P5/P1NBPN2/1PQ2PPP/R1B2RK1 w - - 3 11",
    "2kr3r/ppp2ppp/2n1b3/2b1P3/8/2N2N2/PPP2PPP/R1B1KB1R w KQ - 4 10",
    // Endgames.
    "6k1/6p1/6Pp/ppp5/3pn2P/1P3K2/1PP2P2/8 b - - 3 54",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 11",
    "8/8/8/8/5kp1/P7/8/1K1N4 w - - 0 1",
    "8/8/8/5N2/8/p7/8/2NK3k w - - 0 1",
    "8/3k4/8/8/8/4B3/4KB2/2B5 w - - 0 1",
    "8/8/1P6/5pr1/8/4R3/7k/2K5 w - - 0 1",
    "8/2p4P/8/kr6/6R1/8/8/1K6 w - - 0 1",
    "8/8/3P3k/8/1p6/8/1P6/1K3n2 b - - 0 1",
    "8/R7/2q5/8/6k1/8/1P5p/K6R w - - 0 124",
    "8/8/1k6/2b5/2pP4/8/5K2/8 b - d3 0 1",
    "3k4/3p4/8/K1P4r/8/8/8/8 b - - 0 1",
    "8/8/4k3/8/2p5/8/B2P2K1/8 w - - 0 1",
    "8/5k2/8/3K4/8/8/2P5/8 w - - 0 1",
    "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
    // Chess960.
    "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
    "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
    "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
    "qnbnr1kr/ppp1b1pp/4p3/3p1p2/8/2NPP3/PPP1BPPP/QNB1R1KR w HEhe - 1 9",
    "1qbnrbkr/1p1ppppp/8/p1p5/5P2/P2P4/1PP1P1PP/QNBNRBKR w HEhe - 0 9",
    "bnrqkbrn/pppppppp/8/8/8/8/PPPPPPPP/BNRQKBRN w GCgc - 0 1",
];

/// Searches every Bench position to `depth`, writing one line per position to `output`
/// and then the final line `<nodes> nodes <nps> nps`.
///
/// Bench runs on one thread with a 16 MB hash, and clears the hash and the histories
/// between positions: each position is searched by a fresh [`Worker`] with no time or
/// node limit, so nothing learned from one position carries over to the next and the
/// node count depends only on the code, never on timing or the platform. There is no
/// hash table yet; when one arrives, each position must get a fresh 16 MB table.
///
/// # Errors
///
/// Returns any I/O error from writing `output`.
///
/// # Panics
///
/// Panics if one of the fixed Bench positions is not a valid FEN, which is a bug.
pub fn run(depth: u32, output: &mut impl Write) -> io::Result<()> {
    let limits = Limits {
        depth: Some(depth),
        ..Limits::default()
    };
    let mut total_nodes = 0;
    let mut elapsed = Duration::ZERO;
    for (index, fen) in POSITIONS.iter().enumerate() {
        let board = Board::from_fen(fen)
            .unwrap_or_else(|err| panic!("bench position {fen:?} is invalid: {err}"));
        let game = Game::new(board);
        let mut worker = Worker::new(StopFlag::new());
        let mut nodes = 0;
        let start = Instant::now();
        worker.search(&game, limits, |report| nodes = report.nodes);
        elapsed += start.elapsed();
        total_nodes += nodes;
        writeln!(
            output,
            "position {:>2}/{} nodes {nodes:>9} fen {fen}",
            index + 1,
            POSITIONS.len(),
        )?;
    }
    let micros = elapsed.as_micros().max(1);
    let nps = u64::try_from(u128::from(total_nodes) * 1_000_000 / micros).unwrap_or(u64::MAX);
    writeln!(output)?;
    writeln!(output, "{total_nodes} nodes {nps} nps")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_position_is_valid_and_has_legal_moves() {
        for fen in POSITIONS {
            let board = Board::from_fen(fen).unwrap_or_else(|err| panic!("{fen:?}: {err}"));
            assert!(
                !board.legal_moves().is_empty(),
                "{fen:?} has no legal moves"
            );
        }
    }
}
