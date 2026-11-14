//! Mudskipper binary entry point.

use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    match mudskipper::uci::run(io::stdin().lock(), io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mudskipper: {err}");
            ExitCode::FAILURE
        }
    }
}
