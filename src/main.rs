//! Mudskipper binary entry point.
//!
//! With no arguments it runs the UCI loop. `bench [depth]` runs **Bench** and exits. With
//! the `datagen` feature, `datagen <threads> <games-or-0> <out-dir>` writes self-play
//! **Training data**.

use std::env;
#[cfg(feature = "datagen")]
use std::io::BufRead;
use std::io::{self, BufReader};
#[cfg(feature = "datagen")]
use std::path::Path;
use std::process::ExitCode;
#[cfg(feature = "datagen")]
use std::thread;

#[cfg(feature = "datagen")]
use mudskipper::search::StopFlag;
use mudskipper::tools::bench;
#[cfg(feature = "datagen")]
use mudskipper::tools::datagen;

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
        #[cfg(feature = "datagen")]
        ["datagen", threads, games, out_dir] => {
            let Some(threads) = threads.parse::<usize>().ok().filter(|&n| n > 0) else {
                return usage(&format!("invalid datagen thread count {threads:?}"));
            };
            let limit = match games.parse::<u64>() {
                Ok(0) => datagen::GameLimit::UntilStopped,
                Ok(games) => datagen::GameLimit::Games(games),
                Err(_) => return usage(&format!("invalid datagen game count {games:?}")),
            };
            run_datagen(threads, limit, out_dir)
        }
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
    bench::run(depth, &mut io::stdout().lock())
}

/// Runs datagen, stopping early when a line reading `stop` arrives on standard input.
#[cfg(feature = "datagen")]
fn run_datagen(threads: usize, limit: datagen::GameLimit, out_dir: &str) -> io::Result<()> {
    let stop = StopFlag::new();
    let listener = stop.clone();
    // Detached: it blocks on standard input, and the process exits when datagen returns.
    // Closed or unreadable input never stops datagen, so it can run in the background.
    thread::spawn(move || {
        let stop_requested = io::stdin()
            .lock()
            .lines()
            .map_while(Result::ok)
            .any(|line| line.trim() == "stop");
        if stop_requested {
            listener.raise();
        }
    });
    datagen::run(
        threads,
        limit,
        Path::new(out_dir),
        &stop,
        &mut io::stdout().lock(),
    )
}

/// Reports a command-line mistake and how to run the engine.
fn usage(problem: &str) -> ExitCode {
    eprintln!("mudskipper: {problem}");
    eprintln!("usage: mudskipper [bench [depth]]");
    #[cfg(feature = "datagen")]
    eprintln!("       mudskipper datagen <threads> <games-or-0> <out-dir>");
    ExitCode::FAILURE
}
