use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Sink {
    context: Option<Object>,
    callback: Function,
}

impl Sink {
    pub fn new<F>(callback: F) -> Self
    where
        F: AsRef<Function>,
    {
        Self { context: None, callback: callback.as_ref().clone() }
    }

    pub fn with_context(mut self, context: Option<Object>) -> Self {
        self.context = context;
        self
    }

    pub fn call(&self, args: &JsValue) -> std::result::Result<JsValue, JsValue> {
        if let Some(context) = &self.context {
            self.callback.call1(context, args)
        } else {
            self.callback.call1(&JsValue::UNDEFINED, args)
        }
    }
}

unsafe impl Send for Sink {}

impl Sink {
    pub fn try_from<T>(value: T) -> std::result::Result<Self, JsValue>
    where
        T: AsRef<JsValue>,
    {
        let value = value.as_ref();
        if let Some(callback) = value.dyn_ref::<Function>() {
            Ok(Sink::new(callback))
        } else if let Some(context) = value.dyn_ref::<Object>() {
            let callback = Reflect::get(context, &JsValue::from("handleEvent"))
                .map_err(|_| JsValue::from("Object does not have 'handleEvent()' method"))?
                .dyn_into::<Function>()
                .map_err(|_| JsValue::from("'handleEvent()' is not a function"))?;
            Ok(Sink::new(callback).with_context(Some(context.clone())))
        } else {
            Err(JsValue::from(format!("Invalid event listener callback: {:?}", value)))
        }
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
