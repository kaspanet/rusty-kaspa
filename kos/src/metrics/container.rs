use crate::imports::*;
use web_sys::Element;
use workflow_dom::inject::*;

static mut DOM_INIT: bool = false;

pub struct Container {
    element: Element,
}

unsafe impl Sync for Container {}
unsafe impl Send for Container {}

impl Container {
    pub async fn try_init() -> Result<()> {
        if !unsafe { DOM_INIT } {
            let layout_css = include_str!("container.css");
            inject_css(layout_css)?;
            unsafe {
                DOM_INIT = true;
            }
        }

        Ok(())
    }

    pub async fn try_new(window: &web_sys::Window) -> Result<Container> {
        let document = window.document().unwrap();
        let element = document.create_element("div").unwrap();
        element.set_class_name("layout");

        let body = document.query_selector("body").unwrap().ok_or_else(|| "Unable to get body element".to_string())?;

        body.append_child(&element).unwrap();

        let layout = Container { element };

        Ok(layout)
    }

    pub fn element(&self) -> &Element {
        &self.element
    }
}
