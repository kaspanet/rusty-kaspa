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

pub enum Action {
    Duration(u64),
}

pub struct ToolbarInner {
    pub document: Document,
    pub element: Element,
    pub callbacks: CallbackMap,
    pub container: Arc<Mutex<Option<Arc<Container>>>>,
    pub graphs: Arc<Mutex<HashMap<Metric, Arc<Graph>>>>,
    pub controls: Arc<Mutex<Vec<Arc<dyn Control>>>>,
}

unsafe impl Send for ToolbarInner {}
unsafe impl Sync for ToolbarInner {}

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
            }),
        })
    }

    pub fn document(&self) -> &Document {
        &self.inner.document
    }

    pub fn element(&self) -> &Element {
        &self.inner.element
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
        }
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
        parent.append_child(&radio)?;
        toolbar.element().append_child(&element).unwrap();
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
