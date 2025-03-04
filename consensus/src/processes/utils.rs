use rand::Rng;

pub(crate) struct CoinFlip {
    p: f64,
}

impl Default for CoinFlip {
    fn default() -> Self {
        Self { p: 1.0 / 200.0 }
    }
}

impl CoinFlip {
    pub(crate) fn new(p: f64) -> Self {
        Self { p }
    }

    pub fn flip(self) -> bool {
        rand::thread_rng().gen_bool(self.p)
    }
}
