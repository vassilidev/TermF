//! A seeded generator for the property tests of every crate: the same seed gives the same cases, so a failure
//! replays without a new dependency.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// In `0..n`; `n` must not be 0.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64() as u8).collect()
    }

    /// Up to `max` characters drawn from `alphabet`.
    pub fn text(&mut self, alphabet: &[char], max: usize) -> String {
        let len = self.below(max + 1);
        (0..len).map(|_| *self.pick(alphabet)).collect()
    }
}

/// Characters that break naive code: quotes, separators, line ends, accents, wide and joined characters.
pub const AWKWARD: &[char] = &[
    'a', 'Z', '0', '9', ' ', '\t', '\n', '\r', '"', '\'', ',', ';', '=', ':', '.', '/', '\\', '$',
    '`', '{', '}', '[', ']', '#', '-', '_', '+', '@', '%', '&', 'é', 'ß', 'ü', '中', '😀',
    '\u{200d}', '\u{0}', '•',
];
