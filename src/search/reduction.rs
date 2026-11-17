//! The late move reduction table, built in integer arithmetic at compile time so that it,
//! and with it the **Bench signature**, is the same on every platform.

/// The reduction, in plies, before the logarithmic term is added: 0.75, as common among
/// open-source engines at a similar stage, written as the fraction [`BASE_NUMERATOR`]
/// over [`BASE_DENOMINATOR`]. Raising it reduces every late move more.
const BASE_NUMERATOR: u128 = 3;
/// See [`BASE_NUMERATOR`].
const BASE_DENOMINATOR: u128 = 4;

/// What the product of the natural logarithms of depth and move number is divided by: 2.25,
/// as common among open-source engines at a similar stage, written as the fraction
/// [`DIVISOR_NUMERATOR`] over [`DIVISOR_DENOMINATOR`]. Lowering it makes the reduction
/// grow faster with depth and move number.
const DIVISOR_NUMERATOR: u128 = 9;
/// See [`DIVISOR_NUMERATOR`].
const DIVISOR_DENOMINATOR: u128 = 4;

/// The table covers depths and move numbers below this; larger ones use its last entries.
const TABLE_SIZE: usize = 64;

/// Fractional bits of the fixed-point logarithms.
const FRACTION_BITS: u32 = 32;

/// Fractional bits of the mantissa while [`log2`] squares it, enough that squaring a
/// value below 2 fits in a `u128`.
const MANTISSA_BITS: u32 = 62;

/// The natural logarithm of 2 in fixed point with [`FRACTION_BITS`] fractional bits.
const LN_2: u128 = 2_977_044_472;

/// The reduction for each depth and move number below [`TABLE_SIZE`], with 0 where either
/// is 0.
static TABLE: [[u8; TABLE_SIZE]; TABLE_SIZE] = build_table();

/// The plies a late move is reduced by, before adjustments, at `depth` plies deep
/// when it is the `move_number`th legal move searched: rounded down, 0.75 plus the product
/// of the natural logarithms of the two over 2.25.
pub(super) fn base_reduction(depth: u32, move_number: u32) -> u32 {
    let index = |n: u32| usize::try_from(n).map_or(TABLE_SIZE - 1, |n| n.min(TABLE_SIZE - 1));
    u32::from(TABLE[index(depth)][index(move_number)])
}

/// Builds [`TABLE`] from fixed-point logarithms, so that no platform-dependent floating
/// point is involved.
const fn build_table() -> [[u8; TABLE_SIZE]; TABLE_SIZE] {
    let mut table = [[0; TABLE_SIZE]; TABLE_SIZE];
    let mut depth = 1;
    while depth < TABLE_SIZE {
        let mut move_number = 1;
        while move_number < TABLE_SIZE {
            // base + product / divisor, over the common denominator, with the product
            // carrying twice the fractional bits of each logarithm.
            let product = ln(depth as u32) * ln(move_number as u32);
            let reduction = (((BASE_NUMERATOR * DIVISOR_NUMERATOR) << (2 * FRACTION_BITS))
                + BASE_DENOMINATOR * DIVISOR_DENOMINATOR * product)
                / ((BASE_DENOMINATOR * DIVISOR_NUMERATOR) << (2 * FRACTION_BITS));
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
    fn worked_examples_match_the_logarithmic_formula() {
        // floor(0.75 + ln(depth) * ln(move number) / 2.25), worked by hand.
        for (depth, move_number, expected) in [
            (1, 30, 0),
            (2, 2, 0),
            (3, 4, 1),
            (10, 20, 3),
            (20, 40, 5),
            (63, 63, 8),
        ] {
            assert_eq!(
                base_reduction(depth, move_number),
                expected,
                "depth {depth}, move number {move_number}"
            );
        }
    }

    #[test]
    fn every_entry_matches_floating_point_away_from_rounding_boundaries() {
        for depth in 1..64 {
            for move_number in 1..64 {
                let exact = 0.75 + f64::from(depth).ln() * f64::from(move_number).ln() / 2.25;
                // The fixed-point logarithms are off by about 1e-9, far inside this margin;
                // no entry lies this close to an integer, so every entry is checked.
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
                    base_reduction(depth, move_number),
                    expected,
                    "depth {depth}, move number {move_number}, exact {exact}"
                );
            }
        }
    }

    #[test]
    fn depths_and_move_numbers_past_the_table_use_its_last_entries() {
        assert_eq!(base_reduction(200, 63), base_reduction(63, 63));
        assert_eq!(base_reduction(63, 500), base_reduction(63, 63));
    }
}
