//! The late move reduction tables, built in integer arithmetic at compile time so that
//! they, and with them the **Bench signature**, are the same on every platform, and the
//! history adjustments made to them.

/// A **Late move** to reduce, with its history in the table for its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LateMove {
    /// A quiet late move, with its quiet history.
    Quiet { history: i32 },
    /// A noisy late move, good or bad, with its noisy history.
    Noisy { history: i32 },
}

/// A late move is reduced a ply less for each this much history, if its history is
/// positive: about 2 plies at the maximum history. Lowering it lets history protect more
/// moves from reduction.
const HISTORY_DIVISOR: i32 = 8_192;

/// The plies `late_move` is reduced by at `depth` plies deep when it is the
/// `move_number`th legal move searched, before the search's own adjustments: the entry in
/// its table, [`QUIET_TABLE`] or [`NOISY_TABLE`], lowered by a ply for each
/// [`HISTORY_DIVISOR`] of positive history, and never below zero. Negative history is
/// ignored.
pub(super) fn reduction(late_move: LateMove, depth: u32, move_number: u32) -> u32 {
    let (table, history) = match late_move {
        LateMove::Quiet { history } => (&QUIET_TABLE, history),
        LateMove::Noisy { history } => (&NOISY_TABLE, history),
    };
    let index = |n: u32| usize::try_from(n).map_or(TABLE_SIZE - 1, |n| n.min(TABLE_SIZE - 1));
    let base = u32::from(table[index(depth)][index(move_number)]);
    // Negative history fails the conversion and counts as none.
    let history_bonus = u32::try_from(history / HISTORY_DIVISOR).unwrap_or(0);
    base.saturating_sub(history_bonus)
}

/// A reduction of `base + ln(depth) * ln(move number) / divisor` plies, rounded down, with
/// the base and divisor as exact fractions. Raising the base reduces every move more;
/// lowering the divisor makes the reduction grow faster with depth and move number.
struct Formula {
    base_numerator: u128,
    base_denominator: u128,
    divisor_numerator: u128,
    divisor_denominator: u128,
}

/// The quiet move formula: 0.75 + ln(depth) * ln(move number) / 2.25, as common among
/// open-source engines at a similar stage.
const QUIET: Formula = Formula {
    base_numerator: 3,
    base_denominator: 4,
    divisor_numerator: 9,
    divisor_denominator: 4,
};

/// The noisy move formula: 0.14 + ln(depth) * ln(move number) / 3.65. Noisy moves are
/// forcing, so they are reduced less than quiet moves. Weiss's capture table is the model,
/// but relative to mudskipper's own quiet table: the constants are [`QUIET`]'s, scaled by
/// the ratios between Weiss's capture and quiet tables (0.38 + ln·ln / 3.76 against 2.01 + ln·ln / 2.32): the base by 0.38 / 2.01 and the
/// divisor by 3.76 / 2.32, rounded to two decimal places.
const NOISY: Formula = Formula {
    base_numerator: 7,
    base_denominator: 50,
    divisor_numerator: 73,
    divisor_denominator: 20,
};

/// The tables cover depths and move numbers below this.
const TABLE_SIZE: usize = 64;

/// Fractional bits of the fixed-point logarithms.
const FRACTION_BITS: u32 = 32;

/// Fractional bits of the mantissa while [`log2`] squares it, enough that squaring a
/// value below 2 fits in a `u128`.
const MANTISSA_BITS: u32 = 62;

/// The natural logarithm of 2 in fixed point with [`FRACTION_BITS`] fractional bits.
const LN_2: u128 = 2_977_044_472;

/// A reduction, before history, for each depth and move number below [`TABLE_SIZE`], with
/// 0 where either is 0. Larger depths and move numbers use the last entries.
type Table = [[u8; TABLE_SIZE]; TABLE_SIZE];

/// The [`QUIET`] reductions.
static QUIET_TABLE: Table = build_table(&QUIET);

/// The [`NOISY`] reductions.
static NOISY_TABLE: Table = build_table(&NOISY);

/// Builds a table of `formula` from fixed-point logarithms, so that no platform-dependent
/// floating point is involved.
const fn build_table(formula: &Formula) -> Table {
    let mut table = [[0; TABLE_SIZE]; TABLE_SIZE];
    let mut depth = 1;
    while depth < TABLE_SIZE {
        let mut move_number = 1;
        while move_number < TABLE_SIZE {
            // base + product / divisor, over the common denominator, with the product
            // carrying twice the fractional bits of each logarithm.
            let product = ln(depth as u32) * ln(move_number as u32);
            let reduction = (((formula.base_numerator * formula.divisor_numerator)
                << (2 * FRACTION_BITS))
                + formula.base_denominator * formula.divisor_denominator * product)
                / ((formula.base_denominator * formula.divisor_numerator) << (2 * FRACTION_BITS));
            assert!(
                reduction <= u8::MAX as u128,
                "reduction overflows the table"
            );
            table[depth][move_number] = reduction as u8;
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

    /// A quiet move at zero history, which takes its table entry unchanged.
    const QUIET_BASE: LateMove = LateMove::Quiet { history: 0 };
    /// A noisy move at zero history, which takes its table entry unchanged.
    const NOISY_BASE: LateMove = LateMove::Noisy { history: 0 };

    #[test]
    fn worked_examples_match_the_logarithmic_formulas() {
        // floor(0.75 + ln(depth) * ln(move number) / 2.25) for quiet moves and
        // floor(0.14 + ln(depth) * ln(move number) / 3.65) for noisy moves, worked by hand.
        for (late_move, depth, move_number, expected) in [
            (QUIET_BASE, 1, 30, 0),
            (QUIET_BASE, 2, 2, 0),
            (QUIET_BASE, 3, 4, 1),
            (QUIET_BASE, 10, 20, 3),
            (QUIET_BASE, 20, 40, 5),
            (QUIET_BASE, 63, 63, 8),
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
        for (late_move, base, divisor) in [(QUIET_BASE, 0.75, 2.25), (NOISY_BASE, 0.14, 3.65)] {
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
        // Depth 20, move 40: a base of 5 quiet plies.
        for (history, expected) in [(-16_384, 5), (0, 5), (8_191, 5), (8_192, 4), (16_384, 3)] {
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
        for (history, expected) in [(-16_384, 3), (0, 3), (8_191, 3), (8_192, 2), (16_384, 1)] {
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
