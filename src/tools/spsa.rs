//! The `spsa` subcommand of the **Tune build**: the OpenBench SPSA workload input, printed
//! from the **Search parameter** table so that names, ranges and step sizes are kept in
//! one place.

use std::io::{self, Write};

use crate::search::params;

/// The final learning rate OpenBench is given for every **Search parameter**, the value
/// OpenBench's wiki advises and every reference engine uses.
const R_END: &str = "0.002";

/// Writes one OpenBench SPSA input line per **Search parameter**, in table order:
/// `name, int, value, min, max, C_end, R_end`.
///
/// # Errors
///
/// Returns any error from writing to `output`.
pub fn run(output: &mut impl Write) -> io::Result<()> {
    for param in params::ALL {
        writeln!(
            output,
            "{}, int, {}, {}, {}, {}, {R_END}",
            param.name, param.default, param.min, param.max, param.c_end
        )?;
    }
    Ok(())
}
