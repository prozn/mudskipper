//! A small, fast random number generator for picking opening moves.

/// A xorshift64 generator. Not cryptographic, but quick and good enough to pick random
/// opening moves, and reproducible from its seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng(u64);

impl Rng {
    /// Returns a generator seeded with `seed`. A seed of 0, which xorshift cannot leave,
    /// is replaced with a fixed nonzero one.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// Returns the generator for thread `index` of a run seeded with `seed`, scrambled so
    /// that neighbouring threads get unrelated sequences.
    #[must_use]
    pub const fn for_thread(seed: u64, index: usize) -> Self {
        Self::new(splitmix(seed ^ index as u64))
    }

    /// Returns the next 64 random bits.
    const fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Returns a number in `0..bound`, nearly uniformly.
    ///
    /// # Panics
    ///
    /// Panics if `bound` is 0.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "the bound must be positive");
        ((u128::from(self.next_u64()) * bound as u128) >> 64) as usize
    }
}

/// Scrambles `seed` with the `SplitMix64` finaliser, so that nearby seeds give unrelated
/// values.
const fn splitmix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_stays_within_its_bound_and_reaches_every_value() {
        let mut rng = Rng::new(1);
        let mut seen = [false; 9];
        for _ in 0..1_000 {
            seen[rng.below(9)] = true;
        }
        assert!(seen.iter().all(|&hit| hit), "{seen:?}");
    }
}
