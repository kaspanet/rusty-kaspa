use crate::imports::*;
use crate::metrics::container::*;
use crate::metrics::d3::{self, D3};
use js_sys::Array;
#[allow(unused_imports)]
use kaspa_cli::metrics::{Metric, MetricsData};
use web_sys::{Element, HtmlCanvasElement};
use workflow_core::sendable::Sendable;
use workflow_dom::inject::*;
use workflow_wasm::callback::AsCallback;
use workflow_wasm::object::ObjectTrait;
use workflow_wasm::prelude::CallbackMap;

static mut DOM_INIT: bool = false;

#[derive(Clone)]
pub struct Graph {
    #[allow(dead_code)]
    element: Element,
    canvas: HtmlCanvasElement,
    context: web_sys::CanvasRenderingContext2d,
    width: f32,
    height: f32,
    full_width: f32,
    full_height: f32,
    margin_left: f32,
    margin_right: f32,
    margin_top: f32,
    margin_bottom: f32,

    x: Arc<d3::ScaleTime>,
    y: Arc<d3::ScaleLinear>,
    area: Option<Arc<d3::Area>>,
    data: Array,

    /// holds references to [Callback](workflow_wasm::callback::Callback)
    pub callbacks: CallbackMap,
    // #[allow(dead_code)]
    // metric: Metric,
}

unsafe impl Sync for Graph {}
unsafe impl Send for Graph {}

impl Graph {
    pub async fn try_init() -> Result<()> {
        if !unsafe { DOM_INIT } {
            let graph_css = include_str!("graph.css");
            inject_css(graph_css)?;

            // TODO: this should be used for development only, then embedded directly into this file
            // alternatively use Function() to bootstrap the js graph code
            let graph_js = include_bytes!("graph.js");
            inject_blob(Content::Script(None, graph_js)).await?;
            unsafe {
                DOM_INIT = true;
            }
        }

        Ok(())
    }

    pub async fn try_new(
        window: &web_sys::Window,
        container: &Arc<Container>,
        margin_left: f32,
        margin_right: f32,
        margin_top: f32,
        margin_bottom: f32,
    ) -> Result<Graph> {
        let document = window.document().unwrap();
        let element = document.create_element("div").unwrap();
        container.element().append_child(&element).unwrap();

        element.set_class_name("graph");
        let canvas: Element = document.create_element("canvas").unwrap();
        element.append_child(&canvas).unwrap();
        let canvas = canvas.dyn_into::<web_sys::HtmlCanvasElement>().unwrap();
        let context: web_sys::CanvasRenderingContext2d =
            canvas.get_context("2d").unwrap().unwrap().dyn_into::<web_sys::CanvasRenderingContext2d>().unwrap();
        let mut graph: Graph = Graph {
            element,
            width: 0.0,
            height: 0.0,
            full_width: 0.0,
            full_height: 0.0,
            margin_left,
            margin_right,
            margin_top,
            margin_bottom,
            x: Arc::new(D3::scale_time()),
            y: Arc::new(D3::scale_linear()),
            area: None,
            data: Array::new(),
            canvas,
            context,
            callbacks: CallbackMap::new(),
        };
        graph.init(window).await?;
        Ok(graph)
    }

    pub async fn init(&mut self, window: &web_sys::Window) -> Result<()> {
        let rect = self.canvas.get_bounding_client_rect();
        let pixel_ratio = window.device_pixel_ratio() as f32;
        //workflow_log::log_info!("rectrectrect: {:?}, pixel_ratio:{pixel_ratio}", rect);
        let width = (pixel_ratio * rect.right() as f32).round() - (pixel_ratio * rect.left() as f32).round();
        let height = (pixel_ratio * rect.bottom() as f32).round() - (pixel_ratio * rect.top() as f32).round();
        self.canvas.set_width(width as u32);
        self.canvas.set_height(height as u32);

        self.width = width - self.margin_left - self.margin_right;
        self.height = height - self.margin_top - self.margin_bottom;
        self.full_width = width;
        self.full_height = height;

        self.x.range([0.0, self.width]);
        self.y.range([self.height, 0.0]);

        self.x_axis()?;
        self.y_axis()?;

        // line = d3.line()
        //     .x(function(d) { return x(d.date); })
        //     .y(function(d) { return y(d.value); })
        //     .curve(d3.curveStep)
        //     .context(context);

        let context = &self.context;

        //let x_cb = js_sys::Function::new_with_args("d", "return d.date");
        //let y_cb = js_sys::Function::new_with_args("d", "return d.value");
        let that = self.clone();
        let x_cb = callback!(move |d: js_sys::Object| { that.x.call1(&JsValue::NULL, &d.get("date").unwrap()) });
        let that = self.clone();
        let y_cb = callback!(move |d: js_sys::Object| { that.y.call1(&JsValue::NULL, &d.get("value").unwrap()) });
        self.area = Some(Arc::new(D3::area().x(x_cb.get_fn()).y0(self.height).y1(y_cb.get_fn()).context(context)));

        self.callbacks.retain(x_cb)?;
        self.callbacks.retain(y_cb)?;
        context.translate(self.margin_left as f64, self.margin_top as f64)?;

        Ok(())
    }

    fn x_axis(&self) -> Result<()> {
        let tick_count = 10;
        let tick_size = 6.0;
        let ticks = self.x.ticks(tick_count);
        let tick_format = self.x.tick_format();
        let context = &self.context;
        //workflow_log::log_info!("tick_format:::: {:?}", tick_format);
        context.begin_path();
        for tick in ticks.clone() {
            //workflow_log::log_info!("tick:::: {:?}", tick);
            let x = self.x.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            //workflow_log::log_info!("tick::::x: {:?}", x);
            context.move_to(x, self.height as f64);
            context.line_to(x, self.height as f64 + tick_size);
        }
        context.set_stroke_style(&JsValue::from("black"));
        context.stroke();

        context.set_text_align("center");
        context.set_text_baseline("top");
        for tick in ticks {
            let x = self.x.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            let text = tick_format.call1(&JsValue::NULL, &tick).unwrap().as_string().unwrap();
            context.fill_text(&text, x, self.height as f64 + tick_size)?;
        }

        Ok(())
    }

    fn y_axis(&self) -> Result<()> {
        let tick_count = 10;
        let tick_size = 6.0;
        let tick_padding = 3.0;
        let ticks = self.y.ticks(tick_count);
        let tick_format = self.y.tick_format();
        let context = &self.context;
        context.begin_path();
        for tick in ticks.clone() {
            let y = self.y.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            context.move_to(0.0, y);
            context.line_to(-6.0, y);
        }
        context.set_stroke_style(&JsValue::from("black"));
        context.stroke();

        context.begin_path();
        context.move_to(-tick_size, 0.0);
        context.line_to(0.5, 0.0);
        context.line_to(0.5, self.height as f64);
        context.line_to(-tick_size, self.height as f64);
        context.set_stroke_style(&JsValue::from("black"));
        context.stroke();

        context.set_text_align("right");
        context.set_text_baseline("middle");
        for tick in ticks {
            let y = self.y.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            let text = tick_format.call1(&JsValue::NULL, &tick).unwrap().as_string().unwrap();
            context.fill_text(&text, -tick_size - tick_padding, y)?;
        }

        context.save();
        context.rotate(-std::f64::consts::PI / 2.0)?;
        context.set_text_align("right");
        context.set_text_baseline("top");
        context.set_font("bold 10px sans-serif");
        context.fill_text("Price", -10.0, 10.0)?;
        context.restore();

        Ok(())
    }

    pub fn _element(&self) -> &Element {
        &self.element
    }

    pub fn clear(&self) -> Result<()> {
        let context = &self.context;
        context.clear_rect(-self.margin_left as f64, -self.margin_top as f64, self.full_width as f64, self.full_height as f64);
        Ok(())
    }

    fn update_domains(&self) -> Result<()> {
        let cb = js_sys::Function::new_with_args("d", "return d.date");
        self.x.domain(D3::extent(&self.data, cb));
        let cb = js_sys::Function::new_with_args("d", "return d.value");
        self.y.domain(D3::extent(&self.data, cb));
        self.clear()?;
        self.x_axis()?;
        self.y_axis()?;
        Ok(())
    }

    pub async fn ingest(&self, time: f64, _value: Sendable<JsValue>) -> Result<()> {
        // TODO - ingest into graph
        //self.element().set_inner_html(format!("{} -> {:?}", time, value).as_str());
        //workflow_log::log_info!("{} -> {:?}", time, value);
        let item = js_sys::Object::new();
        let date = js_sys::Date::new(&JsValue::from(time));
        //date.set_date((js_sys::Math::random() * 10.0) as u32);
        let _ = item.set("date", &date);
        let _ = item.set("value", &(js_sys::Math::random() * 100.0).into());
        workflow_log::log_info!("item: {item:?}");
        self.data.push(&item.into());
        self.update_domains()?;

        let context = &self.context;
        context.begin_path();
        self.area.as_ref().unwrap().call1(&JsValue::NULL, &self.data)?;
        context.set_fill_style(&JsValue::from("red"));
        context.set_stroke_style(&JsValue::from("red"));
        context.fill();
        Ok(())
    }
}
