//! The late move reduction tables, built in integer arithmetic at compile time so that
//! they, and with them the **Bench signature**, are the same on every platform.

/// The kind of late move being reduced, each with its own table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LateMove {
    /// A quiet move after the killers.
    Quiet,
    /// A noisy move that loses material by static exchange evaluation. These are forcing
    /// and come last at a node, so they would take the largest quiet reductions; they are
    /// reduced less, by a table like the capture table of Weiss.
    BadNoisy,
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

/// The bad noisy move formula: 0.375 + ln(depth) * ln(move number) / 3.75, close to the
/// 0.38 and 3.76 of Weiss's capture table, so that the gap to the quiet reduction grows
/// with depth.
const BAD_NOISY: Formula = Formula {
    base_numerator: 3,
    base_denominator: 8,
    divisor_numerator: 15,
    divisor_denominator: 4,
};

/// The tables cover depths and move numbers below this; larger ones use their last
/// entries.
const TABLE_SIZE: usize = 64;

/// Fractional bits of the fixed-point logarithms.
const FRACTION_BITS: u32 = 32;

/// Fractional bits of the mantissa while [`log2`] squares it, enough that squaring a
/// value below 2 fits in a `u128`.
const MANTISSA_BITS: u32 = 62;

/// The natural logarithm of 2 in fixed point with [`FRACTION_BITS`] fractional bits.
const LN_2: u128 = 2_977_044_472;

/// A reduction for each depth and move number below [`TABLE_SIZE`], with 0 where either
/// is 0.
type Table = [[u8; TABLE_SIZE]; TABLE_SIZE];

/// The [`QUIET`] reductions.
static QUIET_TABLE: Table = build_table(&QUIET);

/// The [`BAD_NOISY`] reductions.
static BAD_NOISY_TABLE: Table = build_table(&BAD_NOISY);

/// The plies a late move of kind `late_move` is reduced by, before adjustments, at
/// `depth` plies deep when it is the `move_number`th legal move searched, by the
/// [`QUIET`] or [`BAD_NOISY`] formula.
pub(super) fn base_reduction(late_move: LateMove, depth: u32, move_number: u32) -> u32 {
    let table = match late_move {
        LateMove::Quiet => &QUIET_TABLE,
        LateMove::BadNoisy => &BAD_NOISY_TABLE,
    };
    let index = |n: u32| usize::try_from(n).map_or(TABLE_SIZE - 1, |n| n.min(TABLE_SIZE - 1));
    u32::from(table[index(depth)][index(move_number)])
}

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

    #[test]
    fn worked_examples_match_the_logarithmic_formulas() {
        // floor(0.75 + ln(depth) * ln(move number) / 2.25) for quiet moves and
        // floor(0.375 + ln(depth) * ln(move number) / 3.75) for bad noisy moves, worked by
        // hand.
        for (late_move, depth, move_number, expected) in [
            (LateMove::Quiet, 1, 30, 0),
            (LateMove::Quiet, 2, 2, 0),
            (LateMove::Quiet, 3, 4, 1),
            (LateMove::Quiet, 10, 20, 3),
            (LateMove::Quiet, 20, 40, 5),
            (LateMove::Quiet, 63, 63, 8),
            (LateMove::BadNoisy, 1, 30, 0),
            (LateMove::BadNoisy, 3, 4, 0),
            (LateMove::BadNoisy, 4, 10, 1),
            (LateMove::BadNoisy, 8, 20, 2),
            (LateMove::BadNoisy, 16, 30, 2),
            (LateMove::BadNoisy, 63, 63, 4),
        ] {
            assert_eq!(
                base_reduction(late_move, depth, move_number),
                expected,
                "{late_move:?} at depth {depth}, move number {move_number}"
            );
        }
    }

    #[test]
    fn every_entry_matches_floating_point_away_from_rounding_boundaries() {
        for (late_move, base, divisor) in [
            (LateMove::Quiet, 0.75, 2.25),
            (LateMove::BadNoisy, 0.375, 3.75),
        ] {
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
                        base_reduction(late_move, depth, move_number),
                        expected,
                        "{late_move:?} at depth {depth}, move number {move_number}, exact {exact}"
                    );
                }
            }
        }
    }

    #[test]
    fn bad_noisy_moves_are_never_reduced_more_than_quiet_moves() {
        for depth in 1..64 {
            for move_number in 1..64 {
                assert!(
                    base_reduction(LateMove::BadNoisy, depth, move_number)
                        <= base_reduction(LateMove::Quiet, depth, move_number),
                    "depth {depth}, move number {move_number}"
                );
            }
        }
    }

    #[test]
    fn depths_and_move_numbers_past_the_table_use_its_last_entries() {
        for late_move in [LateMove::Quiet, LateMove::BadNoisy] {
            assert_eq!(
                base_reduction(late_move, 200, 63),
                base_reduction(late_move, 63, 63),
                "{late_move:?}"
            );
            assert_eq!(
                base_reduction(late_move, 63, 500),
                base_reduction(late_move, 63, 63),
                "{late_move:?}"
            );
        }
    }
}
