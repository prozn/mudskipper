//! The **Tune build**'s **Search parameters** as UCI options (Seam 1).

#![cfg(feature = "tune")]

#[expect(dead_code, reason = "these tests use only part of the shared harness")]
mod common;

use common::Engine;

/// Group A, as #94 lists it: name, default, min and max.
const GROUP_A: [(&str, i32, i32, i32); 6] = [
    ("RFP_MARGIN", 80, 40, 160),
    ("FP_BASE_MARGIN", 90, 20, 250),
    ("FP_DEPTH_MARGIN", 60, 20, 160),
    ("SEE_PRUNING_DEPTH_MARGIN", 90, 30, 180),
    ("SEE_PRUNING_HISTORY_DIVISOR", 64, 16, 256),
    ("ASPIRATION_DELTA", 20, 5, 50),
];

#[test]
fn uci_lists_each_search_parameter_as_a_spin_option() {
    let mut engine = Engine::start();
    engine.send("uci");
    let lines = engine.expect_line("uciok");
    for (name, default, min, max) in GROUP_A {
        let expected =
            format!("option name {name} type spin default {default} min {min} max {max}");
        assert!(
            lines.contains(&expected),
            "uci should list {expected:?}: {lines:?}"
        );
    }
}

/// A middlegame where every group A parameter matters within a shallow search.
const POSITION: &str =
    "position fen r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N2N2/PP2BPPP/R2QKB1R w KQ - 0 8";

/// Searches [`POSITION`] to `depth` from a cleared search state, and returns the node
/// count the last `info` line reports.
fn nodes_at_depth(engine: &mut Engine, depth: u32) -> u64 {
    engine.send("ucinewgame");
    engine.send(POSITION);
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
    let baseline = nodes_at_depth(&mut engine, DEPTH);
    for (name, default, min, _) in GROUP_A {
        engine.send(&format!("setoption name {name} value {min}"));
        let changed = nodes_at_depth(&mut engine, DEPTH);
        assert_ne!(
            changed, baseline,
            "setting {name} to {min} should change the node count at depth {DEPTH}"
        );
        engine.send(&format!("setoption name {name} value {default}"));
        let restored = nodes_at_depth(&mut engine, DEPTH);
        assert_eq!(
            restored, baseline,
            "restoring {name} to {default} should restore the node count at depth {DEPTH}"
        );
    }
}

#[test]
fn out_of_range_search_parameter_is_refused_and_engine_keeps_running() {
    const DEPTH: u32 = 6;
    let mut engine = Engine::start();
    let baseline = nodes_at_depth(&mut engine, DEPTH);
    for (name, _, min, max) in GROUP_A {
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
