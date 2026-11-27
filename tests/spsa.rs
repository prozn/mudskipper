//! The `spsa` subcommand of the engine binary, which prints the OpenBench SPSA workload
//! input (Seam 1).

#[cfg(feature = "tune")]
#[expect(dead_code, reason = "these tests use only part of the shared harness")]
mod common;

use std::process::{Command, Output};

/// Runs the engine binary with `args` and waits for it to exit.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mudskipper"))
        .args(args)
        .output()
        .expect("engine binary should start")
}

/// One OpenBench SPSA input line: `name, int, value, min, max, C_end, R_end`.
#[cfg(feature = "tune")]
#[derive(Debug)]
struct SpsaLine {
    name: String,
    value: i32,
    min: i32,
    max: i32,
    c_end: f64,
    r_end: f64,
}

/// Parses `line` as an OpenBench SPSA input line, panicking with the line if it is not one.
#[cfg(feature = "tune")]
fn parse_spsa_line(line: &str) -> SpsaLine {
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    let [name, "int", value, min, max, c_end, r_end] = fields[..] else {
        panic!("{line:?} is not `name, int, value, min, max, C_end, R_end`");
    };
    let int = |field: &str| {
        field
            .parse::<i32>()
            .unwrap_or_else(|err| panic!("{field:?} in {line:?}: {err}"))
    };
    let float = |field: &str| {
        field
            .parse::<f64>()
            .unwrap_or_else(|err| panic!("{field:?} in {line:?}: {err}"))
    };
    SpsaLine {
        name: name.to_owned(),
        value: int(value),
        min: int(min),
        max: int(max),
        c_end: float(c_end),
        r_end: float(r_end),
    }
}

/// The **Search parameters** the `uci` reply lists, in order, as (name, default, min, max):
/// every spin option after `Clear Hash`, which ends the options every build has.
#[cfg(feature = "tune")]
fn uci_search_parameters() -> Vec<(String, i32, i32, i32)> {
    let mut engine = common::Engine::start();
    engine.send("uci");
    let lines = engine.expect_line("uciok");
    let after_clear_hash = lines
        .iter()
        .skip_while(|line| *line != "option name Clear Hash type button")
        .skip(1);
    after_clear_hash
        .filter(|line| line.starts_with("option name "))
        .map(|line| {
            let spin = line
                .strip_prefix("option name ")
                .and_then(|rest| rest.split_once(" type spin default "));
            let Some((name, range)) = spin else {
                panic!("{line:?} is not a spin option");
            };
            let words: Vec<&str> = range.split(' ').collect();
            let [default, "min", min, "max", max] = words[..] else {
                panic!("{line:?} is not `default <n> min <n> max <n>`");
            };
            let int = |n: &str| {
                n.parse::<i32>()
                    .unwrap_or_else(|err| panic!("{n:?} in {line:?}: {err}"))
            };
            (name.to_owned(), int(default), int(min), int(max))
        })
        .collect()
}

/// Runs `spsa`, checks it succeeded, and parses each line it prints.
#[cfg(feature = "tune")]
fn spsa_lines() -> Vec<SpsaLine> {
    let output = run(&["spsa"]);
    let stdout = String::from_utf8(output.stdout).expect("spsa output is UTF-8");
    assert!(
        output.status.success(),
        "spsa failed: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout.lines().map(parse_spsa_line).collect()
}

#[cfg(feature = "tune")]
#[test]
fn spsa_prints_one_openbench_line_per_search_parameter_matching_uci() {
    let lines = spsa_lines();

    for line in &lines {
        assert!(
            (line.r_end - 0.002).abs() < 1e-12,
            "R_end should be 0.002: {line:?}"
        );
        assert!(line.c_end > 0.0, "C_end should be positive: {line:?}");
    }

    let printed: Vec<(String, i32, i32, i32)> = lines
        .into_iter()
        .map(|line| (line.name, line.value, line.min, line.max))
        .collect();
    let listed = uci_search_parameters();
    assert!(!listed.is_empty(), "uci lists no Search parameters");
    assert_eq!(
        printed, listed,
        "spsa should print the uci listing's names, defaults and ranges, in order"
    );
}

/// The step sizes of group A, group B, the history bonus and malus split by #98, as #94
/// lists them, and the null-move reduction and late move pruning terms #92 added, which
/// `uci` does not show.
#[cfg(feature = "tune")]
#[test]
fn spsa_prints_each_search_parameter_step_size() {
    const C_END: [(&str, f64); 29] = [
        ("RFP_MARGIN", 8.0),
        ("FP_BASE_MARGIN", 12.0),
        ("FP_DEPTH_MARGIN", 7.0),
        ("SEE_PRUNING_DEPTH_MARGIN", 8.0),
        ("SEE_PRUNING_HISTORY_DIVISOR", 10.0),
        ("ASPIRATION_DELTA", 2.0),
        ("HISTORY_DIVISOR", 600.0),
        ("QUIET_LMR_BASE", 7.0),
        ("QUIET_LMR_DIVISOR", 12.0),
        ("NOISY_LMR_BASE", 8.0),
        ("NOISY_LMR_DIVISOR", 18.0),
        ("VICTIM_WEIGHT", 2.0),
        ("PROMOTION_DIVISOR", 2.0),
        ("QUIET_BONUS_PER_PLY", 25.0),
        ("QUIET_BONUS_OFFSET", 30.0),
        ("QUIET_MAX_BONUS", 120.0),
        ("QUIET_MALUS_PER_PLY", 25.0),
        ("QUIET_MALUS_OFFSET", 30.0),
        ("QUIET_MAX_MALUS", 120.0),
        ("NOISY_BONUS_PER_PLY", 25.0),
        ("NOISY_BONUS_OFFSET", 30.0),
        ("NOISY_MAX_BONUS", 120.0),
        ("NOISY_MALUS_PER_PLY", 25.0),
        ("NOISY_MALUS_OFFSET", 30.0),
        ("NOISY_MAX_MALUS", 120.0),
        ("NMP_BASE_REDUCTION", 58.0),
        ("NMP_DEPTH_REDUCTION", 6.0),
        ("LMP_BASE_MOVES", 22.0),
        ("LMP_DEPTH_MOVES", 6.0),
    ];
    let lines = spsa_lines();
    for (name, c_end) in C_END {
        let line = lines
            .iter()
            .find(|line| line.name == name)
            .unwrap_or_else(|| panic!("spsa prints no line for {name}: {lines:?}"));
        assert!(
            (line.c_end - c_end).abs() < 1e-12,
            "{name} should have C_end {c_end}: {line:?}"
        );
    }
}

#[cfg(not(feature = "tune"))]
#[test]
fn release_build_has_no_spsa_subcommand() {
    let output = run(&["spsa"]);
    assert!(!output.status.success(), "spsa succeeded without `tune`");
    assert!(
        output.stdout.is_empty(),
        "spsa printed without `tune`: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage"),
        "stderr should show usage: {stderr}"
    );
}
