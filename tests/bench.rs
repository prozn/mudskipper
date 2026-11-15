//! The `bench` subcommand of the engine binary (Seam 1).

use std::process::{Command, Output};

/// Runs the engine binary with `args` and waits for it to exit.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mudskipper"))
        .args(args)
        .output()
        .expect("engine binary should start")
}

/// Runs `bench` with `args`, checks it succeeded and that its final line is
/// `<nodes> nodes <nps> nps`, and returns the node count.
fn bench(args: &[&str]) -> u64 {
    let output = run(args);
    let stdout = String::from_utf8(output.stdout).expect("bench output is UTF-8");
    assert!(output.status.success(), "bench failed: {stdout}");
    let last = stdout.lines().last().expect("bench prints something");
    let fields: Vec<&str> = last.split(' ').collect();
    let [nodes, "nodes", nps, "nps"] = fields[..] else {
        panic!("final line {last:?} is not `<nodes> nodes <nps> nps`");
    };
    let parse = |n: &str| {
        n.parse::<u64>()
            .unwrap_or_else(|err| panic!("{n:?} in {last:?}: {err}"))
    };
    parse(nps);
    parse(nodes)
}

#[test]
fn bench_final_line_reports_nodes_and_nps() {
    let nodes = bench(&["bench", "1"]);
    assert!(nodes > 0, "bench searched no nodes");
}

#[test]
fn bench_depth_argument_overrides_the_depth() {
    let shallow = bench(&["bench", "1"]);
    let deeper = bench(&["bench", "2"]);
    assert!(
        deeper > shallow,
        "depth 2 searched {deeper} nodes, depth 1 searched {shallow}"
    );
}

#[test]
fn bench_node_count_is_identical_across_runs() {
    let first = bench(&["bench", "2"]);
    let second = bench(&["bench", "2"]);
    assert_eq!(first, second, "two bench runs disagree");
}

#[test]
fn bench_rejects_a_bad_depth() {
    for depth in ["0", "-1", "deep"] {
        let output = run(&["bench", depth]);
        assert!(!output.status.success(), "bench {depth} succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(depth),
            "stderr should name {depth:?}: {stderr}"
        );
    }
}

#[test]
fn unknown_arguments_are_rejected() {
    let output = run(&["frobnicate"]);
    assert!(!output.status.success(), "unknown argument was accepted");
}
