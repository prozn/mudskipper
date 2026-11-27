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

    /// The divisor of noisy history in the SEE pruning threshold, tuned by SPSA with group
    /// A (#84) and again with group C: a noisy move with full history may lose about 1.9
    /// pawns more. Stormphrax uses 65. A larger divisor weighs history less.
    SEE_PRUNING_HISTORY_DIVISOR: 88, 16..=256, c_end 10;

    /// Half the width of the first aspiration window, in centipawns, tuned by SPSA (#84)
    /// and within the range of 10 to 25 common among open-source engines. The window grows
    /// by half each time the score falls outside it. A narrower window cuts off more of the
    /// tree but misses more often, each miss costing a re-search.
    ASPIRATION_DELTA: 19, 5..=50, c_end 2;

    /// A late move is reduced a ply less for each this much history, if its history is
    /// positive: about 2 plies at the maximum history. Lowering it lets history protect
    /// more moves from reduction. Tuned by SPSA with group B (#97) and again with group C.
    HISTORY_DIVISOR: 7_944, 2_048..=16_384, c_end 600;

    /// The base of the quiet late move reduction table, ×100: 0.79 plies, tuned by SPSA
    /// (#97) from the 0.75 common among open-source engines at a similar stage. Raising it
    /// reduces every quiet move more. Setting it rebuilds the reduction tables.
    QUIET_LMR_BASE: 79, 25..=150, c_end 7;

    /// The divisor of `ln(depth) * ln(move number)` in the quiet late move reduction
    /// table, ×100: 1.98, tuned by SPSA (#97). Lowering it makes the reduction grow faster
    /// with depth and move number. Setting it rebuilds the reduction tables.
    QUIET_LMR_DIVISOR: 198, 150..=350, c_end 12;

    /// The base of the noisy late move reduction table, ×100: 0.14 plies. Noisy moves are
    /// forcing, so they are reduced less than quiet moves. SPSA (#97) kept the value
    /// modelled on Weiss's capture table: the old quiet base of 0.75 scaled by the ratio of
    /// Weiss's capture and quiet bases, 0.38 / 2.01, rounded to two decimal places. It may
    /// go negative. Setting it rebuilds the reduction tables.
    NOISY_LMR_BASE: 14, -50..=100, c_end 8;

    /// The divisor of `ln(depth) * ln(move number)` in the noisy late move reduction
    /// table, ×100: 3.63, tuned by SPSA (#97) from 3.65, the old quiet divisor of 2.25
    /// scaled by the ratio of Weiss's capture and quiet divisors, 3.76 / 2.32, rounded to
    /// two decimal places. Setting it rebuilds the reduction tables.
    NOISY_LMR_DIVISOR: 363, 200..=550, c_end 18;

    /// What the victim's value is multiplied by in a noisy move's ordering score, setting
    /// how much noisy history it takes to put a capture above one of a more valuable
    /// victim. Raising it makes the victim count for more against history. With pawn = 100
    /// and history bounded by 16384 it matches Obsidian's scale, where a full history is
    /// worth about 10 pawns of victim value (`docs/research/capture-history.md`). SPSA
    /// (#97) kept it.
    VICTIM_WEIGHT: 16, 4..=48, c_end 2;

    /// What a promotion's weighted value is divided by in its ordering score. It keeps
    /// promotions in the proportion to captures they had under MVV-LVA, so that a queen
    /// promotion that captures nothing comes below a pawn capture at equal history. Tuned
    /// by SPSA (#97).
    PROMOTION_DIVISOR: 11, 4..=30, c_end 2;

    /// Bonus per ply of depth in quiet history for a quiet move that causes a beta cutoff,
    /// before the cap. Raising it makes deep cutoffs count for more than shallow ones and
    /// history adapt faster, until [`QUIET_MAX_BONUS`] caps it. Tuned by SPSA with group
    /// C, from the value tuned for the bonus and malus of both tables together (#97).
    QUIET_BONUS_PER_PLY: 298, 100..=600, c_end 25;

    /// What [`QUIET_BONUS_PER_PLY`] times the depth is reduced by, so that shallow cutoffs
    /// count for less. Tuned by SPSA with group C.
    QUIET_BONUS_OFFSET: 263, 0..=600, c_end 30;

    /// The largest bonus a single cutoff earns in quiet history, so that one deep cutoff
    /// does not swamp the table. Tuned by SPSA with group C.
    QUIET_MAX_BONUS: 1_536, 500..=3_000, c_end 120;

    /// Malus per ply of depth in quiet history for each quiet move tried before a quiet
    /// move that causes a beta cutoff, before the cap. Raising it makes history forget
    /// moves that failed to cut off faster. Tuned by SPSA with group C.
    QUIET_MALUS_PER_PLY: 296, 100..=600, c_end 25;

    /// What [`QUIET_MALUS_PER_PLY`] times the depth is reduced by. Tuned by SPSA with group
    /// C.
    QUIET_MALUS_OFFSET: 278, 0..=600, c_end 30;

    /// The largest malus a single cutoff gives a quiet move. Tuned by SPSA with group C.
    QUIET_MAX_MALUS: 1_395, 500..=3_000, c_end 120;

    /// Bonus per ply of depth in noisy history for a noisy move that causes a beta cutoff,
    /// before the cap at [`NOISY_MAX_BONUS`]. Tuned by SPSA with group C.
    NOISY_BONUS_PER_PLY: 275, 100..=600, c_end 25;

    /// What [`NOISY_BONUS_PER_PLY`] times the depth is reduced by. Tuned by SPSA with group
    /// C.
    NOISY_BONUS_OFFSET: 241, 0..=600, c_end 30;

    /// The largest bonus a single cutoff earns in noisy history. Tuned by SPSA with group
    /// C.
    NOISY_MAX_BONUS: 1_405, 500..=3_000, c_end 120;

    /// Malus per ply of depth in noisy history for each noisy move tried before a move,
    /// quiet or noisy, that causes a beta cutoff, before the cap at [`NOISY_MAX_MALUS`].
    /// Tuned by SPSA with group C.
    NOISY_MALUS_PER_PLY: 313, 100..=600, c_end 25;

    /// What [`NOISY_MALUS_PER_PLY`] times the depth is reduced by. Tuned by SPSA with group
    /// C.
    NOISY_MALUS_OFFSET: 246, 0..=600, c_end 30;

    /// The largest malus a single cutoff gives a noisy move. Tuned by SPSA with group C.
    NOISY_MAX_MALUS: 1_484, 500..=3_000, c_end 120;

    /// The plies a null move's search is reduced by at any depth, beyond the ply the null
    /// move itself takes, ×256: about 2.95 plies, tuned by SPSA with group C from the 3
    /// common among open-source engines at a similar stage. A larger reduction makes the
    /// null-move search cheaper but more likely to miss a threat.
    NMP_BASE_REDUCTION: 756, 384..=1_536, c_end 58;

    /// What the null-move reduction grows by per ply of depth, ×256: about 0.36 of a ply,
    /// tuned by SPSA with group C from a third, so that deep nodes, where a missed threat
    /// is more likely to be found again later, are reduced more.
    NMP_DEPTH_REDUCTION: 91, 43..=172, c_end 6;

    /// The number of moves late move pruning searches at any depth before skipping quiet
    /// moves, ×100: 3.3 moves, tuned by SPSA with group C from 3. Raising it searches more
    /// quiet moves.
    LMP_BASE_MOVES: 330, 150..=600, c_end 22;

    /// What the late move pruning count grows by per ply of depth, squared, ×100: 0.61 of a
    /// move, tuned by SPSA with group C from half. Raising it searches more quiet moves at
    /// deeper nodes. The range reaches past the whole move Stormphrax uses.
    LMP_DEPTH_MOVES: 61, 25..=150, c_end 6;
}
