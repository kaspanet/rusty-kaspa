use rand::Rng;

pub(crate) struct _CoinFlip {
    p: f64,
}

impl Default for _CoinFlip {
    fn default() -> Self {
        Self { p: 1.0 / 200.0 }
    }
}

impl _CoinFlip {
    pub(crate) fn _new(p: f64) -> Self {
        Self { p }
    }

    pub fn _flip(self) -> bool {
        rand::thread_rng().gen_bool(self.p)
    }
}
