use crate::imports::*;
use crate::metrics::container::*;
#[allow(unused_imports)]
use kaspa_cli::metrics::{Metric, MetricsData};
use web_sys::Element;
use workflow_core::sendable::Sendable;
use workflow_dom::inject::*;

static mut DOM_INIT: bool = false;

pub struct Graph {
    element: Element,
    #[allow(dead_code)]
    metric: Metric,
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

    pub async fn try_new(window: &web_sys::Window, container: &Arc<Container>, metric: &Metric) -> Result<Graph> {
        let document = window.document().unwrap();
        let element = document.create_element("div").unwrap();
        container.element().append_child(&element).unwrap();
        element.set_class_name("graph");

        // TODO - init d3 handling

        let graph = Graph { element, metric: metric.clone() };

        Ok(graph)
    }

    pub fn element(&self) -> &Element {
        &self.element
    }

    pub async fn ingest(&self, time: u64, value: Sendable<JsValue>) -> Result<()> {
        // TODO - ingest into graph
        self.element().set_inner_html(format!("{} -> {:?}", time, value).as_str());

        Ok(())
    }
}
