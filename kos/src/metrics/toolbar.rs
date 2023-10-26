#![allow(dead_code)]

use crate::imports::*;
use crate::result::Result;
use std::{collections::HashMap, sync::MutexGuard};

use kaspa_cli_lib::metrics::Metric;
use web_sys::{Document, Element, MouseEvent};
use workflow_d3::graph::GraphDuration;
#[allow(unused_imports)]
use workflow_d3::{container::Container, graph::Graph};
use workflow_dom::inject::inject_css;

#[derive(Clone, Debug)]
pub struct Count(usize, String);

impl Count {
    pub fn cols() -> [Count; 4] {
        [Count(1, "L".into()), Count(2, "M".into()), Count(3, "S".into()), Count(4, "T".into())]
    }

    pub fn rows() -> [Count; 5] {
        [Count(1, "F".into()), Count(2, "L".into()), Count(4, "M".into()), Count(6, "S".into()), Count(8, "T".into())]
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

#[derive(Debug)]
pub enum Action {
    Duration(Duration),
    Cols(Count),
    Rows(Count),
}

pub struct ToolbarInner {
    pub document: Document,
    pub element: Element,
    pub callbacks: CallbackMap,
    pub container: Arc<Mutex<Option<Arc<Container>>>>,
    pub graphs: Arc<Mutex<HashMap<Metric, Arc<Graph>>>>,
    pub controls: Arc<Mutex<Vec<Arc<dyn Control + Send + Sync>>>>,
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
                layout: Arc::new(Mutex::new((Count::cols().get(2).unwrap().clone(), Count::rows().get(3).unwrap().clone()))),
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

    pub fn controls(&self) -> MutexGuard<Vec<Arc<dyn Control + Send + Sync>>> {
        self.inner.controls.lock().unwrap()
    }

    pub fn push(&self, control: impl Control + Send + Sync + 'static) {
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
            ("2H", "2 Hours"),
            ("4H", "4 Hours"),
            ("8H", "8 Hours"),
            ("12H", "12 Hours"),
            ("24H", "24 Hours"),
            ("36H", "36 Hours"),
            ("48H", "48 Hours"),
            ("72H", "72 Hours"),
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

        for cols in Count::cols() {
            let this = self.clone();
            self.push(RadioButton::try_new(
                self,
                self.element(),
                "cols",
                // &format!("{}", width.1),
                &cols.1.to_string(),
                &format!("Set graph layout to {} columns", cols.0),
                Arc::new(move |btn| this.action(btn, Action::Cols(cols.clone()))),
            )?);
        }
        Separator::try_new(self)?;

        for rows in Count::rows() {
            let this = self.clone();
            self.push(RadioButton::try_new(
                self,
                self.element(),
                "rows",
                // &format!("{}", height.1),
                &rows.1.to_string(),
                &format!("Set graph layout to {} rows", rows.0),
                Arc::new(move |btn| this.action(btn, Action::Rows(rows.clone()))),
            )?);
        }

        let this = self.clone();
        spawn(async move {
            // sleep(Duration::from_millis(100)).await;
            this.select("duration", "5m").expect("unable to locate duration element");
            this.select("cols", "S").expect("unable to locate width element");
            this.select("rows", "S").expect("unable to locate height element");
            this.update_layout(this.inner.layout.lock().unwrap().clone());
        });

        Ok(())
    }

    pub fn select(&self, name: &str, value: &str) -> Result<()> {
        let el = self.document().query_selector(&format!("input[name='{}'][value='{}']", name, value))?.unwrap();
        let event = MouseEvent::new("click").unwrap();
        el.dispatch_event(&event).unwrap();
        Ok(())
    }

    pub fn action(&self, _btn: &dyn Control, action: Action) {
        // log_info!("action: {:?}", action);
        match action {
            Action::Duration(duration) => {
                let graphs = self.inner.graphs.lock().unwrap();
                for graph in (*graphs).values() {
                    graph.set_duration(duration).unwrap_or_else(|err| {
                        log_error!("unable to set graph duration: {}", err);
                    });
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

type ButtonCallback = dyn Fn(&Button) + Send + Sync + 'static;

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

type RadioButtonCallback = dyn Fn(&RadioButton) + Send + Sync + 'static;

#[derive(Clone)]
pub struct RadioButton {
    pub callbacks: CallbackMap,
    pub element: Element,
}

unsafe impl Send for RadioButton {}
unsafe impl Sync for RadioButton {}

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
