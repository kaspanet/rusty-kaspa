use kaspa_hashes::Hash;
use std::num::Wrapping;

pub struct XoShiRo256PlusPlus {
    s0: Wrapping<u64>,
    s1: Wrapping<u64>,
    s2: Wrapping<u64>,
    s3: Wrapping<u64>,
}

impl XoShiRo256PlusPlus {
    #[inline(always)]
    pub fn new(hash: Hash) -> Self {
        let hash = hash.to_le_u64();
        Self { s0: Wrapping(hash[0]), s1: Wrapping(hash[1]), s2: Wrapping(hash[2]), s3: Wrapping(hash[3]) }
    }

    #[inline(always)]
    pub fn u64(&mut self) -> u64 {
        let res = self.s0 + Wrapping((self.s0 + self.s3).0.rotate_left(23));
        let t = self.s1 << 17;
        self.s2 ^= self.s0;
        self.s3 ^= self.s1;
        self.s1 ^= self.s2;
        self.s0 ^= self.s3;

        self.s2 ^= t;
        self.s3 = Wrapping(self.s3.0.rotate_left(45));

        res.0
    }
}
