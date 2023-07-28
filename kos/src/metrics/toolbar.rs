#![allow(dead_code)]

use crate::imports::*;
use crate::result::Result;
use std::{collections::HashMap, sync::MutexGuard};

use kaspa_cli::metrics::Metric;
use web_sys::{Document, Element};
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
        // let graphs = self.graphs.clone();
        let this = self.clone();
        self.push(Button::try_new(self, "1H", Arc::new(move || this.action(Action::Duration(12345))))?);

        Ok(())
    }

    pub fn action(&self, action: Action) {
        match action {
            Action::Duration(_duration) => {
                // TODO - set duration on graphs

                // let graphs = self.graphs.lock().unwrap();
                // if let Ok(timeline) = GraphTimeline::try_from(duration) {
                //     for graph in &graphs {
                //         graph.set_timeline(&timeline);
                //     }
                // }
            }
        }
    }
}

type ButtonCallback = dyn Fn() + 'static;

pub struct Button {
    pub callbacks: CallbackMap,
    pub element: Element,
}

impl Button {
    pub fn try_new(toolbar: &Toolbar, html: &str, callback: Arc<ButtonCallback>) -> Result<Self> {
        let element = toolbar.document().create_element("div").unwrap();
        element.set_class_name("button");
        element.set_inner_html(html);
        toolbar.element().append_child(&element).unwrap();
        let callback = Arc::new(callback);
        let click = callback!(move || {
            callback();
        });

        let callbacks = CallbackMap::default();
        element.add_event_listener_with_callback("click", click.get_fn())?;
        callbacks.retain(click)?;

        Ok(Self { callbacks, element })
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
