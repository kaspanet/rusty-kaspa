//! Notifier provides a lightweight icon-based notification system. Icons show
//! up in the top right corner of the screen and disappear after a short delay.
//! Notification icons indicate clipboard copy and processing that may take
//! an extended period of time.

use crate::imports::*;
use application_runtime::{is_nw, is_web};
use web_sys::Element;
use workflow_dom::{inject::inject_css, utils::*};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Notification {
    Transaction,
    Clipboard,
    Processing,
}

struct Inner {
    elements: Mutex<Option<HashMap<Notification, Element>>>,
    current: Mutex<Option<Element>>,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

#[derive(Clone)]
pub struct Notifier {
    inner: Arc<Inner>,
}

unsafe impl Send for Notifier {}
unsafe impl Sync for Notifier {}

impl Notifier {
    pub fn try_new() -> Result<Notifier> {
        Ok(Notifier { inner: Arc::new(Inner { elements: Mutex::new(None), current: Mutex::new(None) }) })
    }

    pub fn try_init(&self) -> Result<()> {
        let elements = if is_nw() || is_web() { Some(Self::create_elements()?) } else { None };
        *self.inner.elements.lock().unwrap() = elements;
        Ok(())
    }

    pub fn notify(&self, kind: Notification) {
        if let Some(elements) = self.inner.elements.lock().unwrap().as_ref() {
            if let Some(el) = elements.get(&kind) {
                el.class_list().add_1("show").unwrap();
                let el = Sendable(el.clone());
                spawn(async move {
                    yield_executor().await;
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

    pub async fn show(&self, kind: Notification) -> NotifierGuard {
        // let mut inner = self.inner();
        if let Some(elements) = self.inner.elements.lock().unwrap().as_ref() {
            if let Some(el) = elements.get(&kind) {
                el.class_list().add_1("show").unwrap();
                self.inner.current.lock().unwrap().replace(el.clone());
            }
        }

        yield_executor().await;
        sleep(Duration::from_millis(10)).await;
        NotifierGuard::new(self)
    }

    pub async fn hide_async(&self) {
        if let Some(el) = self.inner.current.lock().unwrap().take() {
            el.class_list().remove_1("show").unwrap();
        }
    }

    pub fn hide(&self) {
        if let Some(el) = self.inner.current.lock().unwrap().take() {
            el.class_list().remove_1("show").unwrap();
        }
    }

    pub fn create_elements() -> Result<HashMap<Notification, Element>> {
        let mut elements = HashMap::new();

        inject_css(None, include_str!("./notifier.css"))?;

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

#[must_use = "if unused the notification will immediately disappear"]
#[clippy::has_significant_drop]
pub struct NotifierGuard {
    notifier: Notifier,
}

impl NotifierGuard {
    pub fn new(notifier: &Notifier) -> NotifierGuard {
        NotifierGuard { notifier: notifier.clone() }
    }

    pub fn hide(&self) {
        self.notifier.hide();
    }
}

impl Drop for NotifierGuard {
    fn drop(&mut self) {
        self.notifier.hide();
    }
}
