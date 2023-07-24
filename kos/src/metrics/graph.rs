use web_sys::Element;


pub struct Graph {
    element: Element,
}

impl Graph {
    pub fn new(window : web_sys::Window) -> Graph {

        let document = window.document().unwrap();
        let element = document.create_element("div").unwrap();

        Graph { element }
    }
}