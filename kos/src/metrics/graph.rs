use crate::imports::*;
use crate::metrics::container::*;
use crate::metrics::d3::{self, D3};
use js_sys::Array;
#[allow(unused_imports)]
use kaspa_cli::metrics::{Metric, MetricsData};
use std::sync::MutexGuard;
use web_sys::{Element, HtmlCanvasElement};
use workflow_core::sendable::Sendable;
use workflow_dom::inject::*;
use workflow_wasm::callback::AsCallback;
use workflow_wasm::object::ObjectTrait;
use workflow_wasm::prelude::CallbackMap;

static mut DOM_INIT: bool = false;

#[derive(Clone)]
pub enum GraphTimeline {
    Seconds(u32),
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

#[derive(Clone)]
pub struct GraphThemeOptions {
    pub area_color: String,
    pub x_axis_color: String,
    pub y_axis_color: String,
    pub title_color: String,
    pub x_axis_font: String,
    pub y_axis_font: String,
    pub title_font: String,
}

#[derive(Clone)]
pub enum GraphTheme {
    Light,
    Dark,
    Custom(GraphThemeOptions),
}

impl GraphTheme {
    pub fn get_options(self) -> GraphThemeOptions {
        match self {
            Self::Light => Self::light_theme_options(),
            Self::Dark => Self::dark_theme_options(),
            Self::Custom(theme) => theme,
        }
    }
    pub fn light_theme_options() -> GraphThemeOptions {
        GraphThemeOptions {
            title_font: String::from("bold 15px sans-serif"),
            x_axis_font: String::from("20px serif"),
            y_axis_font: String::from("20px serif"),
            area_color: String::from("blue"),
            x_axis_color: String::from("black"),
            y_axis_color: String::from("black"),
            title_color: String::from("black"),
        }
    }
    pub fn dark_theme_options() -> GraphThemeOptions {
        GraphThemeOptions {
            title_font: String::from("bold 15px sans-serif"),
            x_axis_font: String::from("20px serif"),
            y_axis_font: String::from("20px serif"),
            area_color: String::from("grey"),
            x_axis_color: String::from("white"),
            y_axis_color: String::from("white"),
            title_color: String::from("white"),
        }
    }
}

struct Inner {
    width: f32,
    height: f32,
    full_width: f32,
    full_height: f32,
    margin_left: f32,
    margin_right: f32,
    margin_top: f32,
    margin_bottom: f32,
    min_date: js_sys::Date,
}

#[derive(Clone)]
pub struct Graph {
    #[allow(dead_code)]
    element: Element,
    canvas: HtmlCanvasElement,
    context: web_sys::CanvasRenderingContext2d,

    inner: Arc<Mutex<Inner>>,
    x: Arc<d3::ScaleTime>,
    y: Arc<d3::ScaleLinear>,
    area: Option<Arc<d3::Area>>,
    data: Array,
    timeline: GraphTimeline,
    x_tick_size: f64,
    y_tick_size: f64,
    x_tick_count: u32,
    y_tick_count: u32,
    y_tick_padding: f64,
    title: String,
    options: Arc<Mutex<GraphThemeOptions>>,

    /// holds references to [Callback](workflow_wasm::callback::Callback)
    pub callbacks: CallbackMap,
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
            // let graph_js = include_bytes!("graph.js");
            // inject_blob(Content::Script(None, graph_js)).await?;
            unsafe {
                DOM_INIT = true;
            }
        }

        Ok(())
    }

    pub async fn try_new<T: Into<String>>(
        window: &web_sys::Window,
        container: &Arc<Container>,
        title: T,
        timeline: GraphTimeline,
        theme: GraphTheme,
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

        let options = Arc::new(Mutex::new(theme.get_options()));

        let mut graph: Graph = Graph {
            element,
            inner: Arc::new(Mutex::new(Inner {
                width: 0.0,
                height: 0.0,
                full_width: 0.0,
                full_height: 0.0,
                margin_left,
                margin_right,
                margin_top,
                margin_bottom,
                min_date: js_sys::Date::new_0(),
            })),
            x: Arc::new(D3::scale_time()),
            y: Arc::new(D3::scale_linear()),
            area: None,
            data: Array::new(),
            timeline,
            canvas,
            context,
            x_tick_size: 6.0,
            y_tick_size: 6.0,
            x_tick_count: 10,
            y_tick_count: 10,
            y_tick_padding: 3.0,
            title: title.into(),
            options,
            callbacks: CallbackMap::new(),
        };
        graph.init().await?;
        Ok(graph)
    }

    pub fn set_title<T: Into<String>>(mut self, title: T) -> Self {
        self.title = title.into();
        self
    }

    pub fn set_x_tick_size(mut self, tick_size: f64) -> Self {
        self.x_tick_size = tick_size;
        self
    }

    pub fn set_y_tick_size(mut self, tick_size: f64) -> Self {
        self.y_tick_size = tick_size;
        self
    }

    pub fn set_x_tick_count(mut self, tick_count: u32) -> Self {
        self.x_tick_count = tick_count;
        self
    }

    pub fn set_y_tick_count(mut self, tick_count: u32) -> Self {
        self.y_tick_count = tick_count;
        self
    }

    pub fn set_y_tick_padding(mut self, tick_padding: f64) -> Self {
        self.y_tick_padding = tick_padding;
        self
    }

    pub fn options(&self) -> MutexGuard<GraphThemeOptions> {
        self.options.lock().unwrap()
    }
    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub fn set_title_font<T: Into<String>>(&self, font: T) -> &Self {
        self.options().title_font = font.into();
        self
    }

    pub fn set_x_axis_font<T: Into<String>>(&self, font: T) -> &Self {
        self.options().x_axis_font = font.into();
        self
    }

    pub fn set_y_axis_font<T: Into<String>>(&self, font: T) -> &Self {
        self.options().y_axis_font = font.into();
        self
    }

    pub fn set_area_color<T: Into<String>>(&self, color: T) -> &Self {
        self.options().area_color = color.into();
        self
    }

    pub fn set_x_axis_color<T: Into<String>>(&self, color: T) -> &Self {
        self.options().x_axis_color = color.into();
        self
    }

    pub fn set_y_axis_color<T: Into<String>>(&self, color: T) -> &Self {
        self.options().y_axis_color = color.into();
        self
    }

    pub fn set_title_color<T: Into<String>>(&self, color: T) -> &Self {
        self.options().title_color = color.into();
        self
    }

    pub fn set_theme(&self, theme: GraphTheme) {
        *self.options() = theme.get_options();
    }

    pub async fn init(&mut self) -> Result<()> {
        self.update_size()?;
        self.update_x_domain()?;
        self.x.set_clamp(true);
        // line = d3.line()
        //     .x(function(d) { return x(d.date); })
        //     .y(function(d) { return y(d.value); })
        //     .curve(d3.curveStep)
        //     .context(context);

        //let x_cb = js_sys::Function::new_with_args("d", "return d.date");
        //let y_cb = js_sys::Function::new_with_args("d", "return d.value");
        let height = self.height();
        let that = self.clone();
        let x_cb = callback!(move |d: js_sys::Object| { that.x.call1(&JsValue::NULL, &d.get("date").unwrap()) });
        let that = self.clone();
        let y_cb = callback!(move |d: js_sys::Object| { that.y.call1(&JsValue::NULL, &d.get("value").unwrap()) });
        self.area = Some(Arc::new(D3::area().x(x_cb.get_fn()).y0(height).y1(y_cb.get_fn()).context(&self.context)));

        let that = self.clone();
        let on_resize = callback!(move || { that.update_size() });

        window().add_event_listener_with_callback("resize", on_resize.get_fn())?;

        self.callbacks.retain(x_cb)?;
        self.callbacks.retain(y_cb)?;
        self.callbacks.retain(on_resize)?;

        Ok(())
    }

    fn update_size(&self) -> Result<()> {
        let rect = self.canvas.get_bounding_client_rect();
        let pixel_ratio = workflow_dom::utils::window().device_pixel_ratio() as f32;
        //workflow_log::log_info!("rectrectrect: {:?}, pixel_ratio:{pixel_ratio}", rect);
        let width = (pixel_ratio * rect.right() as f32).round() - (pixel_ratio * rect.left() as f32).round();
        let height = (pixel_ratio * rect.bottom() as f32).round() - (pixel_ratio * rect.top() as f32).round();
        self.canvas.set_width(width as u32);
        self.canvas.set_height(height as u32);
        let (margin_left, margin_top) = {
            let mut inner = self.inner();
            inner.width = width - inner.margin_left - inner.margin_right;
            inner.height = height - inner.margin_top - inner.margin_bottom;
            inner.full_width = width;
            inner.full_height = height;

            self.x.range([0.0, inner.width]);
            self.y.range([inner.height, 0.0]);
            (inner.margin_left, inner.margin_top)
        };
        let context = &self.context;
        context.translate(margin_left as f64, margin_top as f64)?;
        self.x_axis()?;
        self.y_axis()?;
        Ok(())
    }

    pub fn height(&self) -> f32 {
        self.inner().height
    }
    pub fn width(&self) -> f32 {
        self.inner().width
    }
    pub fn min_date(&self) -> js_sys::Date {
        self.inner().min_date.clone()
    }
    pub fn area_color(&self) -> String {
        self.options().area_color.clone()
    }
    pub fn title_font(&self) -> String {
        self.options().title_font.clone()
    }
    pub fn title_color(&self) -> String {
        self.options().title_color.clone()
    }

    fn x_axis(&self) -> Result<()> {
        let tick_count = self.x_tick_count;
        let tick_size = self.x_tick_size;
        let ticks = self.x.ticks(tick_count);
        let tick_format = self.x.tick_format();
        let context = &self.context;
        //workflow_log::log_info!("tick_format:::: {:?}", tick_format);
        let options = self.options();
        let height = self.height();
        let width = self.width();

        context.begin_path();
        context.move_to(0.0, height as f64);
        context.line_to(width as f64, height as f64);
        context.set_stroke_style(&JsValue::from(&options.x_axis_color));
        context.stroke();

        context.begin_path();
        for tick in ticks.clone() {
            //workflow_log::log_info!("tick:::: {:?}", tick);
            let x = self.x.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            //workflow_log::log_info!("tick::::x: {:?}", x);
            context.move_to(x, height as f64);
            context.line_to(x, height as f64 + tick_size);
        }
        context.set_stroke_style(&JsValue::from(&options.x_axis_color));
        context.stroke();

        context.set_text_align("center");
        context.set_text_baseline("top");
        context.set_fill_style(&JsValue::from(&options.x_axis_color));
        context.set_font(&options.x_axis_font);
        for tick in ticks {
            let x = self.x.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            let text = tick_format.call1(&JsValue::NULL, &tick).unwrap().as_string().unwrap();
            context.fill_text(&text, x, height as f64 + tick_size)?;
        }

        Ok(())
    }

    fn y_axis(&self) -> Result<()> {
        let tick_count = self.y_tick_count;
        let tick_size = self.y_tick_size;
        let tick_padding = self.y_tick_padding;
        let ticks = self.y.ticks(tick_count);
        let tick_format = self.y.tick_format();
        let context = &self.context;
        context.begin_path();
        let options = self.options();
        for tick in ticks.clone() {
            let y = self.y.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            context.move_to(0.0, y);
            context.line_to(-tick_size, y);
        }
        context.set_stroke_style(&JsValue::from(&options.y_axis_color));
        context.stroke();
        let height = self.height();
        context.begin_path();
        context.move_to(-tick_size, 0.0);
        context.line_to(0.0, 0.0);
        context.line_to(0.0, height as f64);
        context.line_to(-tick_size, height as f64);
        context.set_stroke_style(&JsValue::from(&options.y_axis_color));
        context.stroke();

        context.set_text_align("right");
        context.set_text_baseline("middle");
        context.set_fill_style(&JsValue::from(&options.y_axis_color));
        context.set_font(&options.y_axis_font);
        for tick in ticks {
            let y = self.y.call1(&JsValue::NULL, &tick).unwrap().as_f64().unwrap();
            let text = tick_format.call1(&JsValue::NULL, &tick).unwrap().as_string().unwrap();
            context.fill_text(&text, -tick_size - tick_padding, y)?;
        }
        Ok(())
    }

    fn build_title(&self) -> Result<()> {
        let context = &self.context;
        let title_font = self.title_font();
        let title_color = self.title_color();
        context.save();
        context.rotate(-std::f64::consts::PI / 2.0)?;
        context.set_text_align("right");
        context.set_text_baseline("top");
        context.set_font(&title_font);
        context.set_fill_style(&JsValue::from(&title_color));
        context.fill_text(&self.title, -10.0, 10.0)?;
        context.restore();

        Ok(())
    }

    pub fn _element(&self) -> &Element {
        &self.element
    }

    pub fn clear(&self) -> Result<()> {
        let inner = self.inner();
        let context = &self.context;
        context.clear_rect(-inner.margin_left as f64, -inner.margin_top as f64, inner.full_width as f64, inner.full_height as f64);
        Ok(())
    }

    fn update_x_domain(&self) -> Result<()> {
        // let cb = js_sys::Function::new_with_args("d", "return d.date");
        // self.x.domain(D3::extent(&self.data, cb));
        let date1 = js_sys::Date::new_0();
        let time = date1.get_time();
        let date2 = js_sys::Date::new(&time.into());

        match self.timeline {
            GraphTimeline::Seconds(seconds) => {
                date2.set_time(time - seconds as f64 * 1000.0);
            }
            GraphTimeline::Minutes(minutes) => {
                date2.set_time(time - minutes as f64 * 60000.0);
            }
            GraphTimeline::Hours(hours) => {
                date2.set_time(time - hours as f64 * 3600000.0);
            }
            GraphTimeline::Days(days) => {
                date2.set_time(time - days as f64 * 86400000.0);
            }
        }

        let x_domain = js_sys::Array::new();
        x_domain.push(&date2);
        x_domain.push(&date1);
        self.inner().min_date = date2;

        self.x.set_domain_array(x_domain);
        Ok(())
    }

    fn update_axis_and_title(&self) -> Result<()> {
        self.update_x_domain()?;
        let cb = js_sys::Function::new_with_args("d", "return d.value");
        self.y.set_domain_array(D3::extent(&self.data, cb));
        self.clear()?;
        self.x_axis()?;
        self.y_axis()?;
        self.build_title()?;

        Ok(())
    }

    pub async fn ingest(&self, time: f64, value: Sendable<JsValue>) -> Result<()> {
        // TODO - ingest into graph
        //self.element().set_inner_html(format!("{} -> {:?}", time, value).as_str());
        //workflow_log::log_info!("{} -> {:?}", time, value);
        let item = js_sys::Object::new();
        let date = js_sys::Date::new(&JsValue::from(time));
        //date.set_date((js_sys::Math::random() * 10.0) as u32);
        let _ = item.set("date", &date);
        //let _ = item.set("value", &(js_sys::Math::random() * 100.0).into());
        let _ = item.set("value", &value);
        workflow_log::log_info!("item: {item:?}");
        self.data.push(&item.into());
        let min_date = self.min_date();

        loop {
            let item = self.data.at(0);
            if let Ok(item) = item.dyn_into::<js_sys::Object>() {
                if let Ok(item_date_v) = item.get("date") {
                    if let Ok(item_date) = item_date_v.dyn_into::<js_sys::Date>() {
                        //workflow_log::log_info!("item_date: {item_date:?} min_date:{min_date:?}");
                        if item_date.lt(&min_date) {
                            self.data.shift();
                            continue;
                        }
                    }
                }
            }
            break;
        }

        self.update_axis_and_title()?;

        let area_color = self.area_color();

        let context = &self.context;
        context.begin_path();
        self.area.as_ref().unwrap().call1(&JsValue::NULL, &self.data)?;
        context.set_fill_style(&JsValue::from(&area_color));
        context.set_stroke_style(&JsValue::from(&area_color));
        context.fill();
        Ok(())
    }
}
