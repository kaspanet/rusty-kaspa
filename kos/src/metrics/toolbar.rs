#![allow(dead_code)]

use crate::imports::*;
use crate::result::Result;
use std::{collections::HashMap, sync::MutexGuard};

use kaspa_cli::metrics::Metric;
use web_sys::{Document, Element};
use workflow_d3::graph::GraphDuration;
#[allow(unused_imports)]
use workflow_d3::{
    container::Container,
    graph::{Graph, DAYS, HOURS, MINUTES},
};
use workflow_dom::inject::inject_css;

#[derive(Clone)]
pub struct Count(usize, String);

impl Count {
    pub fn list() -> [Count; 4] {
        [Count(1, "L".into()), Count(2, "M".into()), Count(3, "S".into()), Count(4, "T".into())]
    }

    fn get_cols(&self) -> String {
        let w = 100.0 / self.0 as f64;
        format!("width: calc({w}vw - 10px);")
    }

    fn get_rows(&self) -> String {
        let h = 100.0 / self.0 as f64;
        format!("height: {h}vh;")
    }
}

type Rows = Count;
type Cols = Count;

pub enum Action {
    Duration(u64),
    Cols(Count),
    Rows(Count),
}

pub struct ToolbarInner {
    pub document: Document,
    pub element: Element,
    pub callbacks: CallbackMap,
    pub container: Arc<Mutex<Option<Arc<Container>>>>,
    pub graphs: Arc<Mutex<HashMap<Metric, Arc<Graph>>>>,
    pub controls: Arc<Mutex<Vec<Arc<dyn Control>>>>,
    pub layout: Arc<Mutex<(Cols, Rows)>>,
}

unsafe impl Send for ToolbarInner {}
unsafe impl Sync for ToolbarInner {}

const STYLE: &str = include_str!("toolbar.css");
static mut DOM_INIT: bool = false;

#[derive(Clone)]
pub struct Toolbar {
    inner: Arc<ToolbarInner>,
}

impl Toolbar {
    pub fn try_new(
        window: &web_sys::Window,
        container: &Arc<Mutex<Option<Arc<Container>>>>,
        graphs: &Arc<Mutex<HashMap<Metric, Arc<Graph>>>>,
    ) -> Result<Self> {
        if !unsafe { DOM_INIT } {
            inject_css(Some("toolbar-style"), STYLE)?;
            unsafe {
                DOM_INIT = true;
            }
        }

        let document = window.document().unwrap();
        let element = document.create_element("div").unwrap();
        element.set_class_name("toolbar");
        let body = document.query_selector("body").unwrap().expect("Toolbar unable to get body element");
        body.append_child(&element).unwrap();

        Ok(Self {
            inner: Arc::new(ToolbarInner {
                document,
                element,
                container: container.clone(),
                graphs: graphs.clone(),
                callbacks: CallbackMap::default(),
                controls: Arc::new(Mutex::new(Vec::new())),
                layout: Arc::new(Mutex::new((Count(3, "M".into()), Count(5, "L".into())))),
            }),
        })
    }

    pub fn document(&self) -> &Document {
        &self.inner.document
    }

    pub fn element(&self) -> &Element {
        &self.inner.element
    }

    pub fn layout(&self) -> MutexGuard<(Cols, Rows)> {
        self.inner.layout.lock().unwrap()
    }

    pub fn controls(&self) -> MutexGuard<Vec<Arc<dyn Control>>> {
        self.inner.controls.lock().unwrap()
    }

    pub fn push(&self, control: impl Control + 'static) {
        let control = Arc::new(control);
        self.controls().push(control);
    }

    pub fn try_init(&self) -> Result<()> {
        let durations = vec![
            ("1m", "1 Minute"),
            ("5m", "1 Minutes"),
            ("15m", "15 Minutes"),
            ("30m", "30 Minutes"),
            ("1H", "1 Hour"),
            ("4H", "4 Hours"),
            ("8H", "8 Hours"),
            ("12H", "12 Hours"),
            ("24H", "24 Hours"),
            ("36H", "36 Hours"),
        ];

        for (html, tip) in durations {
            let this = self.clone();
            let duration = GraphDuration::parse(html.to_lowercase()).unwrap();
            self.push(RadioButton::try_new(
                self,
                self.element(),
                "duration",
                html,
                &format!("Set graph time range to {tip}"),
                Arc::new(move |btn| this.action(btn, Action::Duration(duration))),
            )?);
        }

        Separator::try_new(self)?;

        for width in Count::list() {
            let this = self.clone();
            self.push(RadioButton::try_new(
                self,
                self.element(),
                "width",
                // &format!("{}", width.1),
                &width.1.to_string(),
                &format!("Set graph layout to {} columns", width.0),
                Arc::new(move |btn| this.action(btn, Action::Cols(width.clone()))),
            )?);
        }
        Separator::try_new(self)?;

        for height in Count::list() {
            let this = self.clone();
            self.push(RadioButton::try_new(
                self,
                self.element(),
                "height",
                // &format!("{}", height.1),
                &height.1.to_string(),
                &format!("Set graph layout to {} rows", height.0),
                Arc::new(move |btn| this.action(btn, Action::Rows(height.clone()))),
            )?);
        }

        Ok(())
    }

    pub fn action(&self, _btn: &dyn Control, action: Action) {
        match action {
            Action::Duration(duration) => {
                let graphs = self.inner.graphs.lock().unwrap();
                for graph in (*graphs).values() {
                    graph.set_duration(duration);
                }
            }
            Action::Cols(cols) => {
                let layout = {
                    let mut layout = self.layout();
                    layout.0 = cols;
                    layout.clone()
                };
                self.update_layout(layout);
            }
            Action::Rows(rows) => {
                let layout = {
                    let mut layout = self.layout();
                    layout.1 = rows;
                    layout.clone()
                };
                self.update_layout(layout);
            }
        }
    }

    fn update_layout(&self, layout: (Cols, Rows)) {
        dispatch(async move {
            let style = Graph::default_style().await.expect("unable to get graph style");
            // let re = Regex::new(r"(min|max)?-?(width|height)\s*:\d+(vw|vh);").unwrap();
            // let style = re.replace_all(style.as_str(), "");
            let new_style = format!(
                r#"{}
                    .graph {{ {}{} }}
                "#,
                style,
                layout.0.get_cols(),
                layout.1.get_rows()
            );
            Graph::replace_graph_style("graph", &new_style).await.unwrap_or_else(|err| {
                log_error!("unable to replace graph style: {}", err);
            })
        })
    }
}

type ButtonCallback = dyn Fn(&Button) + 'static;

#[derive(Clone)]
pub struct Button {
    pub callbacks: CallbackMap,
    pub element: Element,
}

impl Button {
    pub fn try_new(toolbar: &Toolbar, parent: &Element, html: &str, tooltip: &str, callback: Arc<ButtonCallback>) -> Result<Self> {
        let element = toolbar.document().create_element("div").unwrap();
        element.set_class_name("button");
        element.set_attribute("title", tooltip)?;
        element.set_inner_html(html);
        parent.append_child(&element).unwrap();
        let callbacks = CallbackMap::default();
        let button = Self { callbacks, element };
        let this = button.clone();
        let callback = Arc::new(callback);
        let click = callback!(move || {
            callback(&this);
        });

        button.element.add_event_listener_with_callback("click", click.get_fn())?;
        button.callbacks.retain(click)?;

        Ok(button)
    }
}

type RadioButtonCallback = dyn Fn(&RadioButton) + 'static;

#[derive(Clone)]
pub struct RadioButton {
    pub callbacks: CallbackMap,
    pub element: Element,
}

impl RadioButton {
    pub fn try_new(
        toolbar: &Toolbar,
        parent: &Element,
        name: &str,
        html: &str,
        tooltip: &str,
        callback: Arc<RadioButtonCallback>,
    ) -> Result<Self> {
        let element = toolbar.document().create_element("label").unwrap();
        element.set_class_name("button");
        element.set_attribute("title", tooltip)?;
        element.set_inner_html(html);
        let radio = toolbar.document().create_element("input").unwrap();
        radio.set_attribute("name", name)?;
        radio.set_attribute("type", "radio")?;
        radio.set_attribute("value", html)?;
        element.append_child(&radio)?;
        parent.append_child(&element).unwrap();
        let callbacks = CallbackMap::default();
        let button = Self { callbacks, element };
        let this = button.clone();
        let callback = Arc::new(callback);
        let click = callback!(move || {
            callback(&this);
        });

        button.element.add_event_listener_with_callback("click", click.get_fn())?;
        button.callbacks.retain(click)?;

        Ok(button)
    }
}

pub struct Caption {
    pub element: Element,
}

impl Caption {
    pub fn try_new(toolbar: &Toolbar, html: &str) -> Result<Self> {
        let element = toolbar.document().create_element("div").unwrap();
        element.set_class_name("caption");
        element.set_inner_html(html);
        toolbar.element().append_child(&element).unwrap();

        Ok(Self { element })
    }
}

pub struct Separator {
    pub element: Element,
}

impl Separator {
    pub fn try_new(toolbar: &Toolbar) -> Result<Self> {
        let element = toolbar.document().create_element("div").unwrap();
        element.set_class_name("separator");
        toolbar.element().append_child(&element).unwrap();

        Ok(Self { element })
    }
}

pub trait Control {}

impl Control for Button {}
impl Control for RadioButton {}
