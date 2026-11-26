//! The **Search parameters**: search constants that **SPSA** can tune, defined in one
//! table.
//!
//! In a normal build each parameter is a constant, and reading it with [`Param::get`]
//! inlines to its default, as a plain `const` would. In the **Tune build** (the `tune`
//! feature) each is a relaxed atomic that UCI sets through a spin option of the same name.
//! UCI sets options only between searches, and the search thread is started after the
//! last one, so relaxed ordering is enough.

#[cfg(feature = "tune")]
use std::sync::atomic::{AtomicI32, Ordering};

/// A search constant that **SPSA** can tune.
#[cfg(not(feature = "tune"))]
#[derive(Debug)]
pub(crate) struct Param {
    value: i32,
}

#[cfg(not(feature = "tune"))]
impl Param {
    /// Returns the parameter's value, which is fixed at its default in this build.
    pub(crate) const fn get(&self) -> i32 {
        self.value
    }
}

/// A search constant that **SPSA** can tune, set through the UCI option of the same name.
#[cfg(feature = "tune")]
#[derive(Debug)]
pub(crate) struct Param {
    /// The parameter's name, which is also its UCI option's name.
    pub(crate) name: &'static str,
    /// The value the parameter starts at, and has in every other build.
    pub(crate) default: i32,
    /// The smallest value the parameter may be set to.
    pub(crate) min: i32,
    /// The largest value the parameter may be set to.
    pub(crate) max: i32,
    /// The step SPSA perturbs the parameter by at the end of a tuning run.
    pub(crate) c_end: i32,
    value: AtomicI32,
}

#[cfg(feature = "tune")]
impl Param {
    /// Returns the parameter's current value.
    pub(crate) fn get(&self) -> i32 {
        self.value.load(Ordering::Relaxed)
    }

    /// Sets the parameter to `value`, which the caller has checked lies from
    /// [`min`](Self::min) to [`max`](Self::max). The next search uses it, along with the
    /// reduction tables, rebuilt here if they depend on it.
    pub(crate) fn set(&self, value: i32) {
        debug_assert!(
            (self.min..=self.max).contains(&value),
            "{} set to {value}, outside {}..={}",
            self.name,
            self.min,
            self.max
        );
        self.value.store(value, Ordering::Relaxed);
        super::reduction::param_changed(self);
    }
}

/// Defines each **Search parameter** from one entry of the form
/// `NAME: default, min..=max, c_end C;`, and [`ALL`] listing them in order in the
/// **Tune build**. Every entry's range is checked at compile time in both builds.
macro_rules! search_params {
    ($(
        $(#[doc = $doc:literal])*
        $name:ident: $default:literal, $min:literal..=$max:literal, c_end $c_end:literal;
    )*) => {
        $(
            const _: () = assert!(
                $min < $max && $min <= $default && $default <= $max && $c_end > 0,
                concat!(stringify!($name), " needs min < max, a default within them and a positive c_end")
            );

            $(#[doc = $doc])*
            #[cfg(not(feature = "tune"))]
            pub(crate) const $name: Param = Param { value: $default };

            $(#[doc = $doc])*
            #[cfg(feature = "tune")]
            pub(crate) static $name: Param = Param {
                name: stringify!($name),
                default: $default,
                min: $min,
                max: $max,
                c_end: $c_end,
                value: AtomicI32::new($default),
            };
        )*

        /// Every **Search parameter**, in table order.
        #[cfg(feature = "tune")]
        pub(crate) static ALL: &[&Param] = &[$(&$name),*];
    };
}

search_params! {
    /// The margin per ply of depth by which the static evaluation must clear beta for
    /// reverse futility pruning, in centipawns, tuned by SPSA with the network (#84). A
    /// smaller margin prunes more nodes but misses more threats.
    RFP_MARGIN: 49, 40..=160, c_end 8;

    /// The margin, in centipawns, a quiet move is allowed to gain over the static
    /// evaluation at any depth for futility pruning, tuned by SPSA (#84). A smaller margin
    /// prunes more quiet moves.
    FP_BASE_MARGIN: 104, 20..=250, c_end 12;

    /// The margin futility pruning allows per ply of depth beyond [`FP_BASE_MARGIN`], in
    /// centipawns, since a deeper search has more moves in which to win material back.
    /// Tuned by SPSA (#84).
    FP_DEPTH_MARGIN: 59, 20..=160, c_end 7;

    /// The material, in centipawns, SEE pruning lets a noisy move lose per ply of depth
    /// before skipping it, tuned by SPSA (#84). Stormphrax, whose threshold has the same
    /// shape, uses 111. A larger margin prunes fewer moves.
    SEE_PRUNING_DEPTH_MARGIN: 88, 30..=180, c_end 8;

    /// The divisor of noisy history in the SEE pruning threshold, tuned by SPSA (#84): a
    /// noisy move with full history may lose about 2.2 pawns more. Stormphrax uses 65. A
    /// larger divisor weighs history less.
    SEE_PRUNING_HISTORY_DIVISOR: 76, 16..=256, c_end 10;

    /// Half the width of the first aspiration window, in centipawns, tuned by SPSA (#84)
    /// and within the range of 10 to 25 common among open-source engines. The window grows
    /// by half each time the score falls outside it. A narrower window cuts off more of the
    /// tree but misses more often, each miss costing a re-search.
    ASPIRATION_DELTA: 19, 5..=50, c_end 2;

    /// Bonus per ply of depth for a move that causes a beta cutoff, in quiet and noisy
    /// history, before the cap. Raising it makes deep cutoffs count for more than shallow
    /// ones and history adapt faster, until [`MAX_BONUS`] caps it.
    BONUS_PER_PLY: 300, 100..=600, c_end 25;

    /// What [`BONUS_PER_PLY`] times the depth is reduced by, so that shallow cutoffs count
    /// for less.
    BONUS_OFFSET: 250, 0..=600, c_end 30;

    /// The largest bonus a single cutoff earns, so that one deep cutoff does not swamp the
    /// history table.
    MAX_BONUS: 1_500, 500..=3_000, c_end 120;

    /// A late move is reduced a ply less for each this much history, if its history is
    /// positive: about 2 plies at the maximum history. Lowering it lets history protect
    /// more moves from reduction.
    HISTORY_DIVISOR: 8_192, 2_048..=16_384, c_end 600;

    /// The base of the quiet late move reduction table, ×100: 0.75 plies, as common among
    /// open-source engines at a similar stage. Raising it reduces every quiet move more.
    /// Setting it rebuilds the reduction tables.
    QUIET_LMR_BASE: 75, 25..=150, c_end 7;

    /// The divisor of `ln(depth) * ln(move number)` in the quiet late move reduction
    /// table, ×100: 2.25. Lowering it makes the reduction grow faster with depth and move
    /// number. Setting it rebuilds the reduction tables.
    QUIET_LMR_DIVISOR: 225, 150..=350, c_end 12;

    /// The base of the noisy late move reduction table, ×100: 0.14 plies. Noisy moves are
    /// forcing, so they are reduced less than quiet moves. Weiss's capture table is the
    /// model, relative to Mudskipper's own quiet table: [`QUIET_LMR_BASE`] scaled by the
    /// ratio of Weiss's capture and quiet bases, 0.38 / 2.01, rounded to two decimal
    /// places. It may go negative. Setting it rebuilds the reduction tables.
    NOISY_LMR_BASE: 14, -50..=100, c_end 8;

    /// The divisor of `ln(depth) * ln(move number)` in the noisy late move reduction
    /// table, ×100: 3.65, [`QUIET_LMR_DIVISOR`] scaled by the ratio of Weiss's capture and
    /// quiet divisors, 3.76 / 2.32, rounded to two decimal places. Setting it rebuilds the
    /// reduction tables.
    NOISY_LMR_DIVISOR: 365, 200..=550, c_end 18;

    /// What the victim's value is multiplied by in a noisy move's ordering score, setting
    /// how much noisy history it takes to put a capture above one of a more valuable
    /// victim. Raising it makes the victim count for more against history. With pawn = 100
    /// and history bounded by 16384 it matches Obsidian's scale, where a full history is
    /// worth about 10 pawns of victim value (`docs/research/capture-history.md`).
    VICTIM_WEIGHT: 16, 4..=48, c_end 2;

    /// What a promotion's weighted value is divided by in its ordering score. It keeps
    /// promotions in the proportion to captures they had under MVV-LVA, so that a queen
    /// promotion that captures nothing comes below a pawn capture at equal history.
    PROMOTION_DIVISOR: 10, 4..=30, c_end 2;
}
