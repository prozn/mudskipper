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
    #[expect(dead_code, reason = "the spsa subcommand prints it in a later ticket")]
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
    /// [`min`](Self::min) to [`max`](Self::max). The next search uses it.
    pub(crate) fn set(&self, value: i32) {
        debug_assert!(
            (self.min..=self.max).contains(&value),
            "{} set to {value}, outside {}..={}",
            self.name,
            self.min,
            self.max
        );
        self.value.store(value, Ordering::Relaxed);
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
    /// reverse futility pruning, in centipawns, as common among open-source engines at a
    /// similar stage. A smaller margin prunes more nodes but misses more threats.
    RFP_MARGIN: 80, 40..=160, c_end 8;

    /// The margin, in centipawns, a quiet move is allowed to gain over the static
    /// evaluation at any depth for futility pruning. A smaller margin prunes more quiet
    /// moves.
    FP_BASE_MARGIN: 90, 20..=250, c_end 12;

    /// The margin futility pruning allows per ply of depth beyond [`FP_BASE_MARGIN`], in
    /// centipawns, since a deeper search has more moves in which to win material back.
    FP_DEPTH_MARGIN: 60, 20..=160, c_end 7;

    /// The material, in centipawns, SEE pruning lets a noisy move lose per ply of depth
    /// before skipping it. Stormphrax, whose threshold has the same shape, uses 111. A
    /// larger margin prunes fewer moves.
    SEE_PRUNING_DEPTH_MARGIN: 90, 30..=180, c_end 8;

    /// The divisor of noisy history in the SEE pruning threshold, close to Stormphrax's
    /// 65: a noisy move with full history may lose about 2.5 pawns more. A larger divisor
    /// weighs history less.
    SEE_PRUNING_HISTORY_DIVISOR: 64, 16..=256, c_end 10;

    /// Half the width of the first aspiration window, in centipawns, within the range of
    /// 10 to 25 common among open-source engines at a similar stage. The window grows by
    /// half each time the score falls outside it. A narrower window cuts off more of the
    /// tree but misses more often, each miss costing a re-search.
    ASPIRATION_DELTA: 20, 5..=50, c_end 2;
}
