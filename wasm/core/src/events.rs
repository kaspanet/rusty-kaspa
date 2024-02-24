use js_sys::{Array, Function};
use wasm_bindgen::prelude::*;

#[derive(Clone, Eq, PartialEq)]
pub struct Sink(pub Function);
unsafe impl Send for Sink {}
impl From<Sink> for Function {
    fn from(f: Sink) -> Self {
        f.0
    }
}

impl<F> From<F> for Sink
where
    F: AsRef<Function>,
{
    fn from(f: F) -> Self {
        Self(f.as_ref().clone())
    }
}

impl Sink {
    pub fn new<F: AsRef<Function>>(f: F) -> Self {
        Self(f.as_ref().clone())
    }
}

pub fn get_event_targets<T, R, E>(targets: T) -> std::result::Result<Vec<R>, E>
where
    T: Into<JsValue>,
    R: TryFrom<JsValue, Error = E>,
{
    let js_value = targets.into();
    if let Ok(array) = js_value.clone().dyn_into::<Array>() {
        array.iter().map(|item| R::try_from(item)).collect()
    } else {
        Ok(vec![R::try_from(js_value)?])
    }
}
