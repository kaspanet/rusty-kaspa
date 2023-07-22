use crate::imports::*;
use application_runtime::{is_nw, is_web};
use web_sys::Element;
use workflow_core::sendable::*;
use workflow_core::task::*;
use workflow_dom::{inject::inject_css, utils::*};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Notification {
    Transaction,
    Clipboard,
    Processing,
}

#[derive(Clone)]
pub struct Notifier {
    elements: Arc<Mutex<Option<HashMap<Notification, Element>>>>,
    current: Arc<Mutex<Option<Element>>>,
}

unsafe impl Send for Notifier {}
unsafe impl Sync for Notifier {}

impl Notifier {
    pub fn try_new() -> Result<Notifier> {
        Ok(Notifier { elements: Arc::new(Mutex::new(None)), current: Arc::new(Mutex::new(None)) })
    }

    pub fn try_init(&self) -> Result<()> {
        let elements = if is_nw() || is_web() { Some(Self::create_elements()?) } else { None };
        *self.elements.lock().unwrap() = elements;
        Ok(())
    }

    pub fn notify(&self, kind: Notification) {
        let element = self.elements.lock().unwrap(); //.as_ref();
        if let Some(elements) = element.as_ref() {
            if let Some(el) = elements.get(&kind) {
                el.class_list().add_1("show").unwrap();
                let el = Sendable(el.clone());
                spawn(async move {
                    sleep(Duration::from_millis(10)).await;
                    el.class_list().remove_1("show").unwrap();
                })
            }
        }
    }

    pub async fn notify_async(&self, kind: Notification) {
        self.notify(kind);
        yield_executor().await;
    }

    pub async fn show(&self, kind: Notification) {
        if let Some(elements) = self.elements.lock().unwrap().as_ref() {
            if let Some(el) = elements.get(&kind) {
                el.class_list().add_1("show").unwrap();
                self.current.lock().unwrap().replace(el.clone());
            }
        }

        sleep(Duration::from_millis(10)).await;
        yield_executor().await;
    }

    pub async fn hide(&self) {
        if let Some(el) = self.current.lock().unwrap().take() {
            el.class_list().remove_1("show").unwrap();
        }
    }

    pub fn create_elements() -> Result<HashMap<Notification, Element>> {
        let mut elements = HashMap::new();

        inject_css(include_str!("./notifier.css"))?;

        let document = document();
        let body = body()?;

        let el = document.create_element("div").unwrap();
        el.set_class_name("notification transaction");
        body.append_child(&el).unwrap();
        elements.insert(Notification::Transaction, el);

        let el = document.create_element("div").unwrap();
        el.set_class_name("notification processing");
        body.append_child(&el).unwrap();
        elements.insert(Notification::Processing, el);

        let el = document.create_element("div").unwrap();
        el.set_class_name("notification clipboard");
        body.append_child(&el).unwrap();
        elements.insert(Notification::Clipboard, el);

        Ok(elements)
    }
}
