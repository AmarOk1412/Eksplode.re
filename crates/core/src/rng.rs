//! Small seeded SplitMix64 generator so streams never depend on external crate versions.

/// Deterministic pseudo random generator.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Creates a generator from a seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns the next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..n`, or 0 when `n` is 0.
    pub fn next_below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % u64::from(n)) as u32
    }

    /// Returns true with probability `p` (clamped to `0.0..=1.0`).
    pub fn chance(&mut self, p: f32) -> bool {
        if p <= 0.0 {
            return false;
        }
        if p >= 1.0 {
            return true;
        }
        let unit = (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32;
        unit < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_differ() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn next_below_is_in_range() {
        let mut r = Rng::new(3);
        for n in 1..50 {
            assert!(r.next_below(n) < n);
        }
        assert_eq!(r.next_below(0), 0);
    }

    #[test]
    fn chance_extremes() {
        let mut r = Rng::new(4);
        for _ in 0..100 {
            assert!(!r.chance(0.0));
            assert!(r.chance(1.0));
        }
    }

    #[test]
    fn chance_is_roughly_calibrated() {
        let mut r = Rng::new(5);
        let hits = (0..10_000).filter(|_| r.chance(0.3)).count();
        assert!((2_500..3_500).contains(&hits), "hits = {hits}");
    }
}
