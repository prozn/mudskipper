//! The late move reduction tables, and the history adjustments made to them.
//!
//! The tables are built in integer arithmetic so that they, and with them the **Bench
//! signature**, are the same on every platform: at compile time from the **Search
//! parameters**' defaults in a normal build, and whenever one of their parameters is set
//! in the **Tune build**.

#[cfg(feature = "tune")]
use std::sync::{LazyLock, Mutex, MutexGuard};

#[cfg(feature = "tune")]
use super::params::Param;
use super::params::{
    HISTORY_DIVISOR, NOISY_LMR_BASE, NOISY_LMR_DIVISOR, QUIET_LMR_BASE, QUIET_LMR_DIVISOR,
};

/// A **Late move** to reduce, with its history in the table for its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LateMove {
    /// A quiet late move, with its quiet history.
    Quiet { history: i32 },
    /// A noisy late move, good or bad, with its noisy history.
    Noisy { history: i32 },
}

/// The reduction tables one worker searches with.
///
/// In a normal build it holds nothing and reads tables built at compile time from the
/// **Search parameters**' defaults. In the **Tune build** it holds its own copy of the
/// shared tables, taken by [`refresh`](Self::refresh) when a search starts, so that the
/// search never waits on a lock.
#[derive(Clone, Debug)]
pub(super) struct Reductions {
    #[cfg(feature = "tune")]
    tables: Tables,
}

impl Reductions {
    /// Returns the tables for the current **Search parameters**.
    pub(super) fn new() -> Self {
        Self {
            #[cfg(feature = "tune")]
            tables: shared_tables().clone(),
        }
    }

    /// Takes up the tables for the current **Search parameters**, which in the **Tune
    /// build** may have been rebuilt since the last search.
    #[cfg_attr(
        not(feature = "tune"),
        expect(
            clippy::unused_self,
            reason = "only the Tune build keeps tables per worker"
        )
    )]
    pub(super) fn refresh(&mut self) {
        #[cfg(feature = "tune")]
        self.tables.clone_from(&shared_tables());
    }

    /// The plies `late_move` is reduced by at `depth` plies deep when it is the
    /// `move_number`th legal move searched, before the search's own adjustments: the entry
    /// in its table, quiet or noisy, lowered by a ply for each [`HISTORY_DIVISOR`] of
    /// positive history, and never below zero. Negative history is ignored.
    #[cfg_attr(
        not(feature = "tune"),
        expect(
            clippy::unused_self,
            reason = "only the Tune build keeps tables per worker"
        )
    )]
    pub(super) fn reduction(&self, late_move: LateMove, depth: u32, move_number: u32) -> u32 {
        #[cfg(feature = "tune")]
        let tables = &self.tables;
        #[cfg(not(feature = "tune"))]
        let tables = &DEFAULT_TABLES;
        let (table, history) = match late_move {
            LateMove::Quiet { history } => (&tables.quiet, history),
            LateMove::Noisy { history } => (&tables.noisy, history),
        };
        let index = |n: u32| usize::try_from(n).map_or(TABLE_SIZE - 1, |n| n.min(TABLE_SIZE - 1));
        let base = u32::from(table[index(depth)][index(move_number)]);
        // Negative history fails the conversion and counts as none.
        let history_bonus = u32::try_from(history / HISTORY_DIVISOR.get()).unwrap_or(0);
        base.saturating_sub(history_bonus)
    }
}

/// A reduction of `base + ln(depth) * ln(move number) / divisor` plies, rounded down and
/// never below zero, with the base and divisor in units of 1 / [`FORMULA_SCALE`]. Raising
/// the base reduces every move more; lowering the divisor makes the reduction grow faster
/// with depth and move number.
#[derive(Clone, Copy, Debug)]
struct Formula {
    base: i32,
    divisor: i32,
}

/// The quiet and noisy reduction tables.
#[derive(Clone, Debug)]
struct Tables {
    quiet: Table,
    noisy: Table,
}

impl Tables {
    /// Builds the tables from the current values of their **Search parameters**.
    #[cfg(feature = "tune")]
    fn from_params() -> Self {
        Self::build(
            Formula {
                base: QUIET_LMR_BASE.get(),
                divisor: QUIET_LMR_DIVISOR.get(),
            },
            Formula {
                base: NOISY_LMR_BASE.get(),
                divisor: NOISY_LMR_DIVISOR.get(),
            },
        )
    }

    /// Builds the tables of the `quiet` and `noisy` formulas.
    const fn build(quiet: Formula, noisy: Formula) -> Self {
        Self {
            quiet: build_table(quiet),
            noisy: build_table(noisy),
        }
    }
}

/// The tables at the **Search parameters**' defaults, built at compile time.
#[cfg(not(feature = "tune"))]
static DEFAULT_TABLES: Tables = Tables::build(
    Formula {
        base: QUIET_LMR_BASE.get(),
        divisor: QUIET_LMR_DIVISOR.get(),
    },
    Formula {
        base: NOISY_LMR_BASE.get(),
        divisor: NOISY_LMR_DIVISOR.get(),
    },
);

/// The tables for the current **Search parameters**, which workers copy when a search
/// starts.
#[cfg(feature = "tune")]
static SHARED: LazyLock<Mutex<Tables>> = LazyLock::new(|| Mutex::new(Tables::from_params()));

/// Locks [`SHARED`]. Callers hold the lock only to copy or replace the tables.
#[cfg(feature = "tune")]
fn shared_tables() -> MutexGuard<'static, Tables> {
    SHARED.lock().expect("reduction tables lock poisoned")
}

/// The **Search parameters** the tables are built from.
#[cfg(feature = "tune")]
const TABLE_PARAMS: [&Param; 4] = [
    &QUIET_LMR_BASE,
    &QUIET_LMR_DIVISOR,
    &NOISY_LMR_BASE,
    &NOISY_LMR_DIVISOR,
];

/// Rebuilds the shared tables if `param` is one they are built from, which the caller has
/// just set. Workers take up the new tables at the start of their next search.
#[cfg(feature = "tune")]
pub(super) fn param_changed(param: &Param) {
    if TABLE_PARAMS
        .iter()
        .any(|table_param| std::ptr::eq(*table_param, param))
    {
        let tables = Tables::from_params();
        *shared_tables() = tables;
    }
}

/// What a [`Formula`]'s base and divisor are multiplied by: their **Search parameters**
/// are in hundredths, so that a UCI spin option can carry them and a step of 1 is a small
/// change.
const FORMULA_SCALE: i128 = 100;

/// The tables cover depths and move numbers below this.
const TABLE_SIZE: usize = 64;

/// Fractional bits of the fixed-point logarithms.
const FRACTION_BITS: u32 = 32;

/// Fractional bits of the mantissa while [`log2`] squares it, enough that squaring a
/// value below 2 fits in a `u128`.
const MANTISSA_BITS: u32 = 62;

/// One in the fixed point of a product of two logarithms, which carries twice their
/// fractional bits.
const PRODUCT_ONE: i128 = 1 << (2 * FRACTION_BITS);

/// The natural logarithm of 2 in fixed point with [`FRACTION_BITS`] fractional bits.
const LN_2: u128 = 2_977_044_472;

/// A reduction, before history, for each depth and move number below [`TABLE_SIZE`], with
/// 0 where either is 0. Larger depths and move numbers use the last entries.
type Table = [[u8; TABLE_SIZE]; TABLE_SIZE];

/// Builds a table of `formula` from fixed-point logarithms, so that no platform-dependent
/// floating point is involved.
const fn build_table(formula: Formula) -> Table {
    assert!(formula.divisor > 0, "a reduction divisor must be positive");
    let base = formula.base as i128;
    let divisor = formula.divisor as i128;
    let mut table = [[0; TABLE_SIZE]; TABLE_SIZE];
    let mut depth = 1;
    while depth < TABLE_SIZE {
        let mut move_number = 1;
        while move_number < TABLE_SIZE {
            // base / scale + product / (divisor / scale), over the common denominator
            // scale * divisor, rounded down.
            let product = (ln(depth as u32) * ln(move_number as u32)) as i128;
            let reduction = (base * divisor * PRODUCT_ONE
                + FORMULA_SCALE * FORMULA_SCALE * product)
                .div_euclid(FORMULA_SCALE * divisor * PRODUCT_ONE);
            assert!(
                reduction <= u8::MAX as i128,
                "reduction overflows the table"
            );
            table[depth][move_number] = if reduction < 0 { 0 } else { reduction as u8 };
            move_number += 1;
        }
        depth += 1;
    }
    table
}

/// The natural logarithm of `n`, at least 1, in fixed point with [`FRACTION_BITS`]
/// fractional bits, rounded down.
const fn ln(n: u32) -> u128 {
    (log2(n) * LN_2) >> FRACTION_BITS
}

/// The base-2 logarithm of `n`, at least 1, in fixed point with [`FRACTION_BITS`]
/// fractional bits, found one bit at a time by repeatedly squaring the mantissa.
const fn log2(n: u32) -> u128 {
    assert!(n > 0, "the logarithm of 0 is undefined");
    let whole = n.ilog2();
    let two: u128 = 2 << MANTISSA_BITS;
    // The mantissa n / 2^whole, in [1, 2).
    let mut mantissa = ((n as u128) << MANTISSA_BITS) >> whole;
    let mut result = (whole as u128) << FRACTION_BITS;
    let mut bit = FRACTION_BITS;
    while bit > 0 {
        bit -= 1;
        mantissa = (mantissa * mantissa) >> MANTISSA_BITS;
        if mantissa >= two {
            mantissa >>= 1;
            result |= 1 << bit;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plies `late_move` is reduced by with the default tables.
    fn reduction(late_move: LateMove, depth: u32, move_number: u32) -> u32 {
        Reductions::new().reduction(late_move, depth, move_number)
    }

    /// A quiet move at zero history, which takes its table entry unchanged.
    const QUIET_BASE: LateMove = LateMove::Quiet { history: 0 };
    /// A noisy move at zero history, which takes its table entry unchanged.
    const NOISY_BASE: LateMove = LateMove::Noisy { history: 0 };

    #[test]
    fn worked_examples_match_the_logarithmic_formulas() {
        // floor(0.79 + ln(depth) * ln(move number) / 1.98) for quiet moves and
        // floor(0.14 + ln(depth) * ln(move number) / 3.63) for noisy moves, worked by hand.
        for (late_move, depth, move_number, expected) in [
            (QUIET_BASE, 1, 30, 0),
            (QUIET_BASE, 2, 2, 1),
            (QUIET_BASE, 3, 4, 1),
            (QUIET_BASE, 10, 20, 4),
            (QUIET_BASE, 20, 40, 6),
            (QUIET_BASE, 63, 63, 9),
            (NOISY_BASE, 1, 30, 0),
            (NOISY_BASE, 3, 4, 0),
            (NOISY_BASE, 4, 10, 1),
            (NOISY_BASE, 8, 20, 1),
            (NOISY_BASE, 16, 30, 2),
            (NOISY_BASE, 20, 40, 3),
            (NOISY_BASE, 63, 63, 4),
        ] {
            assert_eq!(
                reduction(late_move, depth, move_number),
                expected,
                "{late_move:?} at depth {depth}, move number {move_number}"
            );
        }
    }

    #[test]
    fn every_entry_matches_floating_point_away_from_rounding_boundaries() {
        for (late_move, base, divisor) in [(QUIET_BASE, 0.79, 1.98), (NOISY_BASE, 0.14, 3.63)] {
            for depth in 1..64 {
                for move_number in 1..64 {
                    let exact =
                        base + f64::from(depth).ln() * f64::from(move_number).ln() / divisor;
                    // The fixed-point logarithms are off by about 1e-9, far inside this
                    // margin; entries this close to an integer are skipped.
                    if (exact - exact.round()).abs() < 1e-6 {
                        continue;
                    }
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "exact is positive and below 64"
                    )]
                    let expected = exact.floor() as u32;
                    assert_eq!(
                        reduction(late_move, depth, move_number),
                        expected,
                        "{late_move:?} at depth {depth}, move number {move_number}, exact {exact}"
                    );
                }
            }
        }
    }

    #[test]
    fn noisy_moves_are_reduced_less_than_quiet_moves() {
        for depth in 1..64 {
            for move_number in 1..64 {
                let noisy = reduction(NOISY_BASE, depth, move_number);
                let quiet = reduction(QUIET_BASE, depth, move_number);
                // Strictly less wherever moves are reduced at all: from depth 3 on and past
                // the first move. Elsewhere both may round down to the same value.
                if depth >= 3 && move_number >= 2 {
                    assert!(
                        noisy < quiet,
                        "depth {depth}, move number {move_number}: noisy {noisy}, quiet {quiet}"
                    );
                } else {
                    assert!(
                        noisy <= quiet,
                        "depth {depth}, move number {move_number}: noisy {noisy}, quiet {quiet}"
                    );
                }
            }
        }
    }

    #[test]
    fn depths_and_move_numbers_past_the_table_use_its_last_entries() {
        for late_move in [QUIET_BASE, NOISY_BASE] {
            assert_eq!(
                reduction(late_move, 200, 63),
                reduction(late_move, 63, 63),
                "{late_move:?}"
            );
            assert_eq!(
                reduction(late_move, 63, 500),
                reduction(late_move, 63, 63),
                "{late_move:?}"
            );
        }
    }

    #[test]
    fn good_quiet_history_lowers_the_reduction_and_bad_quiet_history_does_not_raise_it() {
        // Depth 20, move 40: a base of 6 quiet plies.
        for (history, expected) in [(-16_384, 6), (0, 6), (7_943, 6), (7_944, 5), (16_384, 4)] {
            assert_eq!(
                reduction(LateMove::Quiet { history }, 20, 40),
                expected,
                "history {history}"
            );
        }
    }

    #[test]
    fn good_noisy_history_lowers_the_reduction_and_bad_noisy_history_does_not_raise_it() {
        // Depth 20, move 40: a base of 3 noisy plies.
        for (history, expected) in [(-16_384, 3), (0, 3), (7_943, 3), (7_944, 2), (16_384, 1)] {
            assert_eq!(
                reduction(LateMove::Noisy { history }, 20, 40),
                expected,
                "history {history}"
            );
        }
    }

    #[test]
    fn noisy_history_never_takes_the_reduction_below_zero() {
        // Depth 8, move 20: a base of 1 noisy ply.
        assert_eq!(reduction(LateMove::Noisy { history: 16_384 }, 8, 20), 0);
        assert_eq!(reduction(LateMove::Noisy { history: 16_384 }, 3, 4), 0);
    }
}
