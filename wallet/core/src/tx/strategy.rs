use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct LimitCalcStrategy {
    #[wasm_bindgen(skip)]
    pub strategy: LimitStrategy,
}

#[wasm_bindgen]
impl LimitCalcStrategy {
    pub fn calculated() -> LimitCalcStrategy {
        LimitStrategy::Calculated.into()
    }
    pub fn inputs(inputs: u8) -> LimitCalcStrategy {
        LimitStrategy::Inputs(inputs).into()
    }
}

pub enum LimitStrategy {
    Calculated,
    Inputs(u8),
}

impl From<LimitStrategy> for LimitCalcStrategy {
    fn from(strategy: LimitStrategy) -> Self {
        Self { strategy }
    }
}
impl From<LimitCalcStrategy> for LimitStrategy {
    fn from(value: LimitCalcStrategy) -> Self {
        value.strategy
    }
}
