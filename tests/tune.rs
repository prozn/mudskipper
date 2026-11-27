//! The **Tune build**'s **Search parameters** as UCI options (Seam 1).

#![cfg(feature = "tune")]

#[expect(dead_code, reason = "these tests use only part of the shared harness")]
mod common;

use common::Engine;

/// Group A, as #94 lists it, with the defaults tuned in #84: name, default, min and max.
const GROUP_A: [(&str, i32, i32, i32); 6] = [
    ("RFP_MARGIN", 49, 40, 160),
    ("FP_BASE_MARGIN", 104, 20, 250),
    ("FP_DEPTH_MARGIN", 59, 20, 160),
    ("SEE_PRUNING_DEPTH_MARGIN", 88, 30, 180),
    ("SEE_PRUNING_HISTORY_DIVISOR", 76, 16, 256),
    ("ASPIRATION_DELTA", 19, 5, 50),
];

/// Group B, as #94 lists it, with the defaults tuned in #97: name, default, min and max.
/// The reduction tables' bases and divisors are ×100.
const GROUP_B: [(&str, i32, i32, i32); 10] = [
    ("BONUS_PER_PLY", 303, 100, 600),
    ("BONUS_OFFSET", 251, 0, 600),
    ("MAX_BONUS", 1503, 500, 3000),
    ("HISTORY_DIVISOR", 7835, 2048, 16384),
    ("QUIET_LMR_BASE", 79, 25, 150),
    ("QUIET_LMR_DIVISOR", 198, 150, 350),
    ("NOISY_LMR_BASE", 14, -50, 100),
    ("NOISY_LMR_DIVISOR", 363, 200, 550),
    ("VICTIM_WEIGHT", 16, 4, 48),
    ("PROMOTION_DIVISOR", 11, 4, 30),
];

/// Every **Search parameter**, group A then group B.
fn all_params() -> impl Iterator<Item = (&'static str, i32, i32, i32)> {
    GROUP_A.into_iter().chain(GROUP_B)
}

#[test]
fn uci_lists_each_search_parameter_as_a_spin_option() {
    let mut engine = Engine::start();
    engine.send("uci");
    let lines = engine.expect_line("uciok");
    for (name, default, min, max) in all_params() {
        let expected =
            format!("option name {name} type spin default {default} min {min} max {max}");
        assert!(
            lines.contains(&expected),
            "uci should list {expected:?}: {lines:?}"
        );
    }
}

/// A middlegame where every **Search parameter** but `PROMOTION_DIVISOR` matters within a
/// shallow search.
const POSITION: &str =
    "position fen r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N2N2/PP2BPPP/R2QKB1R w KQ - 0 8";

/// A pawn race where both sides can promote within a shallow search, so that
/// `PROMOTION_DIVISOR` matters.
const PROMOTION_POSITION: &str = "position fen 8/1P2k3/8/8/8/8/2p1K3/8 w - - 0 1";

/// The position in which setting the **Search parameter** `name` changes a shallow search.
fn position_for(name: &str) -> &'static str {
    if name == "PROMOTION_DIVISOR" {
        PROMOTION_POSITION
    } else {
        POSITION
    }
}

/// Searches [`POSITION`] to `depth` from a cleared search state, and returns the node
/// count the last `info` line reports.
fn nodes_at_depth(engine: &mut Engine, depth: u32) -> u64 {
    nodes_at_depth_in(engine, POSITION, depth)
}

/// Searches `position`, a `position` command, to `depth` from a cleared search state, and
/// returns the node count the last `info` line reports.
fn nodes_at_depth_in(engine: &mut Engine, position: &str, depth: u32) -> u64 {
    engine.send("ucinewgame");
    engine.send(position);
    engine.send(&format!("go depth {depth}"));
    let reply = engine.read_line_matching("a bestmove line", |line| line.starts_with("bestmove "));
    let last = reply
        .before
        .iter()
        .rev()
        .find(|line| line.starts_with("info depth"))
        .unwrap_or_else(|| panic!("no info depth line before bestmove: {:?}", reply.before));
    let mut words = last.split_whitespace();
    words.find(|&word| word == "nodes");
    words
        .next()
        .and_then(|nodes| nodes.parse().ok())
        .unwrap_or_else(|| panic!("info line has no node count: {last:?}"))
}

#[test]
fn setting_a_search_parameter_changes_the_search_until_it_is_restored() {
    const DEPTH: u32 = 8;
    let mut engine = Engine::start();
    for (name, default, min, _) in all_params() {
        let position = position_for(name);
        let baseline = nodes_at_depth_in(&mut engine, position, DEPTH);
        engine.send(&format!("setoption name {name} value {min}"));
        let changed = nodes_at_depth_in(&mut engine, position, DEPTH);
        assert_ne!(
            changed, baseline,
            "setting {name} to {min} should change the node count at depth {DEPTH}"
        );
        engine.send(&format!("setoption name {name} value {default}"));
        let restored = nodes_at_depth_in(&mut engine, position, DEPTH);
        assert_eq!(
            restored, baseline,
            "restoring {name} to {default} should restore the node count at depth {DEPTH}"
        );
    }
}

#[test]
fn setting_a_reduction_parameter_rebuilds_the_tables_until_it_is_restored() {
    const DEPTH: u32 = 8;
    // NOISY_LMR_BASE at its minimum checks that a negative base is accepted.
    const REDUCTION_PARAMS: [(&str, i32, i32); 4] = [
        ("QUIET_LMR_BASE", 79, 150),
        ("QUIET_LMR_DIVISOR", 198, 150),
        ("NOISY_LMR_BASE", 14, -50),
        ("NOISY_LMR_DIVISOR", 363, 200),
    ];
    let fresh = nodes_at_depth(&mut Engine::start(), DEPTH);
    let mut engine = Engine::start();
    for (name, default, value) in REDUCTION_PARAMS {
        engine.send(&format!("setoption name {name} value {value}"));
        let changed = nodes_at_depth(&mut engine, DEPTH);
        assert_ne!(
            changed, fresh,
            "setting {name} to {value} should change the node count at depth {DEPTH}"
        );
        engine.send(&format!("setoption name {name} value {default}"));
        let restored = nodes_at_depth(&mut engine, DEPTH);
        assert_eq!(
            restored, fresh,
            "restoring {name} to {default} should give a fresh engine's node count at depth {DEPTH}"
        );
    }
}

#[test]
fn out_of_range_search_parameter_is_refused_and_engine_keeps_running() {
    const DEPTH: u32 = 6;
    let mut engine = Engine::start();
    let baseline = nodes_at_depth(&mut engine, DEPTH);
    for (name, _, min, max) in all_params() {
        for value in [min - 1, max + 1] {
            engine.send(&format!("setoption name {name} value {value}"));
            let reply = engine.expect_info_string();
            assert!(
                reply.contains(&format!("from {min} to {max}")),
                "refusing {name} = {value} should give the range: {reply:?}"
            );
        }
    }
    assert!(
        engine.is_running(),
        "engine exited after a refused setoption"
    );
    assert_eq!(
        nodes_at_depth(&mut engine, DEPTH),
        baseline,
        "a refused setoption should leave every parameter at its default"
    );
}
