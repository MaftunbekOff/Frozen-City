//! Tiny deterministic SplitMix64 RNG. The state is a single u64 stored inside
//! `GameState`, which keeps the whole simulation reproducible from a seed.

#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u64);

impl Default for Rng {
    fn default() -> Self {
        Rng::new(0xF0_2E_57)
    }
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..n` (returns 0 when n == 0).
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as u32
        }
    }

    /// Uniform value in `lo..=hi`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + self.below((hi - lo) as u32 + 1) as i32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64) < p as f64
    }
}
