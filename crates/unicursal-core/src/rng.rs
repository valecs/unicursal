//! The core's one random number generator: PCG32 (XSH RR 64/32), as specified
//! by O'Neill, "PCG: A Family of Simple Fast Space-Efficient Statistically Good
//! Algorithms for Random Number Generation" (2014), `pcg32_srandom_r` and
//! `pcg32_random_r` of the reference implementation.
//!
//! Puzzle links regenerate a puzzle from its seed (#5), so every derived
//! operation (bounded integers, shuffles, floats) is specified here too and
//! must never change without bumping [`crate::GENERATOR_VERSION`].

const MULTIPLIER: u64 = 6364136223846793005;

/// Stream used for the main generator of a puzzle (any odd-able constant; this
/// is the reference implementation's demo stream).
pub const MAIN_STREAM: u64 = 54;

#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    /// `pcg32_srandom_r(seed, stream)`.
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Pcg32 {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// The generator for a puzzle seed.
    pub fn from_seed(seed: u64) -> Self {
        Self::new(seed, MAIN_STREAM)
    }

    /// `pcg32_random_r`.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Two draws, high word first.
    pub fn next_u64(&mut self) -> u64 {
        let hi = self.next_u32() as u64;
        let lo = self.next_u32() as u64;
        (hi << 32) | lo
    }

    /// Uniform in `0..bound` by rejection (`pcg32_boundedrand_r`): unbiased,
    /// and the number of draws depends only on the drawn values.
    ///
    /// # Panics
    /// If `bound` is 0 or does not fit in 32 bits.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "below(0)");
        let bound = u32::try_from(bound).expect("bound fits in u32");
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return (r % bound) as usize;
            }
        }
    }

    /// Uniform in `[0, 1)` with 53 random bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Fisher-Yates, from the back: for `i` from `len-1` down to 1, swap `i`
    /// with `below(i + 1)`.
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }

    /// A uniformly chosen element.
    ///
    /// # Panics
    /// If `v` is empty.
    pub fn choose<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len())]
    }

    /// A seed for a sub-generator (e.g. flow noise), drawn from this one.
    pub fn derive_seed(&mut self) -> u64 {
        self.next_u64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_implementation() {
        // pcg32-demo from the PCG reference implementation: srandom(42, 54).
        let mut r = Pcg32::new(42, 54);
        let got: Vec<u32> = (0..6).map(|_| r.next_u32()).collect();
        assert_eq!(
            got,
            [
                0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e
            ]
        );
    }

    #[test]
    fn below_stays_in_range_and_hits_every_value() {
        let mut r = Pcg32::from_seed(1);
        let mut seen = [false; 7];
        for _ in 0..1000 {
            seen[r.below(7)] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut r = Pcg32::from_seed(9);
        let mut v: Vec<u32> = (0..50).collect();
        r.shuffle(&mut v);
        let mut s = v.clone();
        s.sort();
        assert_eq!(s, (0..50).collect::<Vec<_>>());
        assert_ne!(v, s);
    }

    #[test]
    fn floats_are_in_unit_interval() {
        let mut r = Pcg32::from_seed(3);
        for _ in 0..1000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
