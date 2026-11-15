//! Mudskipper binary entry point.
//!
//! With no arguments it runs the UCI loop. `bench [depth]` runs **Bench** and exits.

use std::env;
use std::io::{self, BufReader};
use std::process::ExitCode;

use mudskipper::tools::bench;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args[..] {
        [] => mudskipper::uci::run(BufReader::new(io::stdin()), io::stdout().lock()),
        ["bench"] => run_bench(bench::DEFAULT_DEPTH),
        ["bench", depth] => match depth.parse::<u32>() {
            Ok(depth) if depth > 0 => run_bench(depth),
            _ => return usage(&format!("invalid bench depth {depth:?}")),
        },
        _ => return usage(&format!("unexpected arguments {:?}", args.join(" "))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mudskipper: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Runs Bench to `depth`, writing to standard output.
fn run_bench(depth: u32) -> io::Result<()> {
    bench::run(depth, &mut io::stdout().lock()).map(drop)
}

/// Reports a command-line mistake and how to run the engine.
fn usage(problem: &str) -> ExitCode {
    eprintln!("mudskipper: {problem}");
    eprintln!("usage: mudskipper [bench [depth]]");
    ExitCode::FAILURE
}
