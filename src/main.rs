//! Mudskipper binary entry point.

use std::io::{self, BufReader};
use std::process::ExitCode;

fn main() -> ExitCode {
    match mudskipper::uci::run(BufReader::new(io::stdin()), io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mudskipper: {err}");
            ExitCode::FAILURE
        }
    }
}
