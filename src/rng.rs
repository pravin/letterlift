// Small deterministic random number generator (SplitMix64).
// Written out here rather than taken from a crate so that a given seed picks
// the same puzzle on every platform and with every dependency version.

use std::time::{SystemTime, UNIX_EPOCH};

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// A seed for a fresh random puzzle: short enough to read out and share
    pub fn fresh_seed() -> u64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Rng::new(nanos ^ std::process::id() as u64).next_u64() % 1_000_000
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in 0..n (n must be non-zero)
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn test_known_output_is_stable() {
        // Pins the algorithm: changing it would change every daily puzzle
        assert_eq!(Rng::new(0).next_u64(), 0xE220_A839_7B1D_CDAF);
    }

    #[test]
    fn test_below_stays_in_range() {
        let mut rng = Rng::new(7);
        for _ in 0..1000 {
            assert!(rng.below(5) < 5);
        }
    }

    #[test]
    fn test_fresh_seed_is_short() {
        assert!(Rng::fresh_seed() < 1_000_000);
    }
}
