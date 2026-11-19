//! The late move reduction tables, built in integer arithmetic at compile time so that
//! they, and with them the **Bench signature**, are the same on every platform, and the
//! history adjustments made to them.

/// A late move to reduce, with its history in the table for its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LateMove {
    /// A quiet move after the killers, with its quiet history.
    Quiet { history: i32 },
    /// A noisy move other than the TT move, good or bad, with its noisy history.
    Noisy { history: i32 },
}

/// A late move is reduced a ply less for each this much history: about 2 plies at the
/// maximum history. Quiet moves are helped only by positive history; noisy moves are also
/// reduced more by negative history. Lowering it lets history move the reduction more.
const HISTORY_DIVISOR: i32 = 8_192;

/// The plies `late_move` is reduced by at `depth` plies deep when it is the
/// `move_number`th legal move searched, before the search's own adjustments: the base
/// reduction for its kind, as [`base_reduction`] describes, lowered by a ply for each
/// [`HISTORY_DIVISOR`] of its history, and never below zero. For a quiet move, negative
/// history is ignored; for a noisy move, it raises the reduction by as much.
pub(super) fn reduction(late_move: LateMove, depth: u32, move_number: u32) -> u32 {
    let (kind, history_bonus) = match late_move {
        LateMove::Quiet { history } => (Kind::Quiet, (history / HISTORY_DIVISOR).max(0)),
        LateMove::Noisy { history } => (Kind::Noisy, history / HISTORY_DIVISOR),
    };
    let base =
        i32::try_from(base_reduction(kind, depth, move_number)).expect("table entries fit in a u8");
    u32::try_from((base - history_bonus).max(0)).expect("the reduction is not negative")
}

/// The kind of late move, each with its own table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Quiet,
    Noisy,
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
/// but relative to mudskipper's own quiet table: the constants are [`QUIET`]'s, scaled by the ratios between Weiss's capture and quiet tables
/// (0.38 + ln·ln / 3.76 against 2.01 + ln·ln / 2.32): the base by 0.38 / 2.01 and the
/// divisor by 3.76 / 2.32, rounded to two decimal places.
const NOISY: Formula = Formula {
    base_numerator: 7,
    base_denominator: 50,
    divisor_numerator: 73,
    divisor_denominator: 20,
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

/// The [`NOISY`] reductions.
static NOISY_TABLE: Table = build_table(&NOISY);

/// The plies a late move of `kind` is reduced by, before history, at `depth` plies deep
/// when it is the `move_number`th legal move searched, by the [`QUIET`] or [`NOISY`]
/// formula.
fn base_reduction(kind: Kind, depth: u32, move_number: u32) -> u32 {
    let table = match kind {
        Kind::Quiet => &QUIET_TABLE,
        Kind::Noisy => &NOISY_TABLE,
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
        // floor(0.14 + ln(depth) * ln(move number) / 3.65) for noisy moves, worked by hand.
        for (kind, depth, move_number, expected) in [
            (Kind::Quiet, 1, 30, 0),
            (Kind::Quiet, 2, 2, 0),
            (Kind::Quiet, 3, 4, 1),
            (Kind::Quiet, 10, 20, 3),
            (Kind::Quiet, 20, 40, 5),
            (Kind::Quiet, 63, 63, 8),
            (Kind::Noisy, 1, 30, 0),
            (Kind::Noisy, 3, 4, 0),
            (Kind::Noisy, 4, 10, 1),
            (Kind::Noisy, 8, 20, 1),
            (Kind::Noisy, 16, 30, 2),
            (Kind::Noisy, 20, 40, 3),
            (Kind::Noisy, 63, 63, 4),
        ] {
            assert_eq!(
                base_reduction(kind, depth, move_number),
                expected,
                "{kind:?} at depth {depth}, move number {move_number}"
            );
        }
    }

    #[test]
    fn every_entry_matches_floating_point_away_from_rounding_boundaries() {
        for (kind, base, divisor) in [(Kind::Quiet, 0.75, 2.25), (Kind::Noisy, 0.14, 3.65)] {
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
                        base_reduction(kind, depth, move_number),
                        expected,
                        "{kind:?} at depth {depth}, move number {move_number}, exact {exact}"
                    );
                }
            }
        }
    }

    #[test]
    fn noisy_moves_are_reduced_less_than_quiet_moves() {
        for depth in 1..64 {
            for move_number in 1..64 {
                let noisy = base_reduction(Kind::Noisy, depth, move_number);
                let quiet = base_reduction(Kind::Quiet, depth, move_number);
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
        for kind in [Kind::Quiet, Kind::Noisy] {
            assert_eq!(
                base_reduction(kind, 200, 63),
                base_reduction(kind, 63, 63),
                "{kind:?}"
            );
            assert_eq!(
                base_reduction(kind, 63, 500),
                base_reduction(kind, 63, 63),
                "{kind:?}"
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
    fn noisy_history_lowers_or_raises_the_reduction_by_up_to_two_plies() {
        // Depth 20, move 40: a base of 3 noisy plies.
        for (history, expected) in [
            (-16_384, 5),
            (-8_192, 4),
            (-8_191, 3),
            (0, 3),
            (8_191, 3),
            (8_192, 2),
            (16_384, 1),
        ] {
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
