//! The Universal Chess Interface protocol layer.

use std::io::{self, BufRead, Write};

/// Engine name reported in reply to `uci`.
const NAME: &str = concat!("Mudskipper ", env!("CARGO_PKG_VERSION"));

/// Author reported in reply to `uci`.
const AUTHOR: &str = "Prozn";

/// Runs the UCI loop, reading commands from `input` and writing replies to `output`.
///
/// The loop ends on `quit` or end of input. Lines it cannot act on are answered with an
/// `info string` and otherwise ignored.
///
/// # Errors
///
/// Returns any I/O error from reading `input` or writing `output`.
pub fn run(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if input.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        let Ok(line) = str::from_utf8(&buf) else {
            writeln!(output, "info string ignoring line that is not valid UTF-8")?;
            output.flush()?;
            continue;
        };
        match line.split_whitespace().next() {
            None => {}
            Some("uci") => {
                writeln!(output, "id name {NAME}")?;
                writeln!(output, "id author {AUTHOR}")?;
                writeln!(output, "uciok")?;
            }
            Some("isready") => writeln!(output, "readyok")?,
            Some("quit") => break,
            Some(_) => writeln!(output, "info string unknown command: {}", line.trim())?,
        }
        output.flush()?;
    }
    Ok(())
}
