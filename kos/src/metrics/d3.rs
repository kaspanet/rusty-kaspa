use js_sys::{Array, Function, Object};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {

    #[wasm_bindgen(extends = Object, js_name = d3)]
    pub type D3;

    /// (Internal) Get the filtered command line arguments when starting the app.
    /// In NW.js, some command line arguments are used by NW.js,
    /// which should not be interested of your app. App.argv will filter out
    /// those arguments and return the ones left. You can get filtered patterns
    /// from [app::filtered_argv](self::filtered_argv) and the full arguments from [app::full_argv](self::full_argv).
    ///
    /// ⧉ [NWJS Documentation](https://docs.nwjs.io/en/latest/References/App/#appargv)
    ///
    #[wasm_bindgen(static_method_of=D3, js_class=d3, js_name = scaleTime)]
    pub fn scale_time() -> ScaleTime;

    #[wasm_bindgen(static_method_of=D3, js_class=d3, js_name = scaleLinear)]
    pub fn scale_linear() -> ScaleLinear;

    #[wasm_bindgen(static_method_of=D3, js_class=d3, js_name = area)]
    pub fn area() -> Area;

    #[wasm_bindgen(static_method_of=D3, js_class=d3, js_name = extent)]
    pub fn extent(data: &Array, cb: Function) -> Array;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Function)]
    pub type ScaleTime;

    #[wasm_bindgen(method, js_name=range)]
    pub fn range_impl(this: &ScaleTime, range: Array) -> ScaleTime;

    #[wasm_bindgen(method, js_name=domain)]
    pub fn set_domain_array(this: &ScaleTime, domain: Array) -> ScaleTime;

    #[wasm_bindgen(method)]
    pub fn ticks(this: &ScaleTime, count: u32) -> Array;

    #[wasm_bindgen(method, js_name=tickFormat)]
    pub fn tick_format(this: &ScaleTime) -> Function;

    #[wasm_bindgen(method, js_name=clamp)]
    pub fn set_clamp(this: &ScaleTime, clamp: bool);

    // #[wasm_bindgen(method, js_name=tickFormat)]
    // pub fn call1(this: &ScaleTime, value: JsValue) -> f64;

}

impl ScaleTime {
    pub fn range(&self, range: [f32; 2]) -> &Self {
        let range_value = Array::new();
        range_value.push(&range[0].into());
        range_value.push(&range[1].into());
        self.range_impl(range_value);
        self
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Function)]
    pub type ScaleLinear;

    #[wasm_bindgen(method, js_name=range)]
    pub fn range_impl(this: &ScaleLinear, range: Array) -> ScaleLinear;

    #[wasm_bindgen(method, js_name=domain)]
    pub fn set_domain_array(this: &ScaleLinear, domain: Array) -> ScaleLinear;

    #[wasm_bindgen(method)]
    pub fn ticks(this: &ScaleLinear, count: u32) -> Array;

    #[wasm_bindgen(method, js_name=tickFormat)]
    pub fn tick_format(this: &ScaleLinear) -> Function;
}

impl ScaleLinear {
    pub fn range(&self, range: [f32; 2]) -> &Self {
        let range_value = Array::new();
        range_value.push(&range[0].into());
        range_value.push(&range[1].into());
        self.range_impl(range_value);
        self
    }

    pub fn set_domain(&self, min: u32, max: u32) -> &Self {
        let domain = Array::new();
        domain.push(&min.into());
        domain.push(&max.into());
        self.set_domain_array(domain);
        self
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Function)]
    pub type Area;

    #[wasm_bindgen(method)]
    pub fn x(this: &Area, cb: &Function) -> Area;

    #[wasm_bindgen(method)]
    pub fn y0(this: &Area, value: f32) -> Area;

    #[wasm_bindgen(method)]
    pub fn y1(this: &Area, cb: &Function) -> Area;

    #[wasm_bindgen(method)]
    pub fn context(this: &Area, ctx: &web_sys::CanvasRenderingContext2d) -> Area;
}
