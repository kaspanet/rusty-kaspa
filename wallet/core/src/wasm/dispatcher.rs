//!
//! EventDispatcher - subscription-based channel multiplexer client for WASM.
//!

// use workflow_wasm::abi::ref_from_abi;
// use crate::error::Error;
use crate::result::Result;
use crate::wasm::notify::WalletNotificationCallback;
use futures::{select, FutureExt};
use js_sys::Function;
use serde::Serialize;
use serde_wasm_bindgen::to_value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use workflow_core::channel::{DuplexChannel, Multiplexer, MultiplexerChannel};
use workflow_core::sendable::Sendable;
use workflow_core::task::*;
use workflow_log::log_error;

pub struct Inner {
    callback: Mutex<Option<Sendable<Function>>>,
    task_running: AtomicBool,
    task_ctl: DuplexChannel,
}

///
/// EventDispatcher is a WASM channel bridge that provides
/// access to Rust channel events in WASM32 environment.
///
/// IMPORTANT: You can register only a single listener at a time.
///
/// @see {@link UtxoProcessor}, {@link Wallet}
///
/// @category General
///
#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct EventDispatcher {
    inner: Arc<Inner>,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        EventDispatcher::new()
    }
}

impl EventDispatcher {
    pub async fn start_notification_task<T>(&self, multiplexer: &Multiplexer<T>) -> Result<()>
    where
        T: Clone + Serialize + Send + Sync + 'static,
    {
        let inner = self.inner.clone();

        if inner.task_running.load(Ordering::SeqCst) {
            panic!("ReflectorClient task is already running");
        }
        let ctl_receiver = inner.task_ctl.request.receiver.clone();
        let ctl_sender = inner.task_ctl.response.sender.clone();
        inner.task_running.store(true, Ordering::SeqCst);

        let channel = MultiplexerChannel::from(multiplexer);

        spawn(async move {
            loop {
                select! {
                    _ = ctl_receiver.recv().fuse() => {
                        break;
                    },
                    msg = channel.receiver.recv().fuse() => {
                        // log_info!("notification: {:?}",msg);
                        if let Ok(notification) = &msg {
                            if let Some(callback) = inner.callback.lock().unwrap().as_ref() {
                                // if let Ok(event) = JsValue::try_from(notification) {
                                if let Ok(event) = to_value(notification) {
                                    if let Err(err) = callback.0.call1(&JsValue::undefined(), &event) {
                                        log_error!("Error while executing notification callback: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            channel.close();
            ctl_sender.send(()).await.ok();
        });

        Ok(())
    }
}

#[wasm_bindgen]
impl EventDispatcher {
    #[wasm_bindgen(constructor)]
    pub fn new() -> EventDispatcher {
        EventDispatcher {
            inner: Arc::new(Inner {
                callback: Mutex::new(None),
                task_running: AtomicBool::new(false),
                task_ctl: DuplexChannel::oneshot(),
            }),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn listener(&self) -> JsValue {
        if let Some(callback) = self.inner.callback.lock().unwrap().as_ref() {
            callback.as_ref().clone().into()
        } else {
            JsValue::UNDEFINED
        }
    }

    #[wasm_bindgen(setter, js_name = "listener")]
    pub fn listener_setter(&self, callback: JsValue) -> Result<()> {
        if callback.is_function() {
            let fn_callback: Function = callback.into();
            self.inner.callback.lock().unwrap().replace(fn_callback.into());
        } else {
            self.remove_listener()?;
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = "registerListener")]
    pub fn register_listener(&self, callback: WalletNotificationCallback) -> Result<()> {
        if callback.is_function() {
            let fn_callback: Function = callback.into();
            self.inner.callback.lock().unwrap().replace(fn_callback.into());
        } else {
            self.remove_listener()?;
        }
        Ok(())
    }

    /// `removeListenet` must be called when releasing ReflectorClient
    /// to stop the background event processing task
    #[wasm_bindgen(js_name = "removeListener")]
    pub fn remove_listener(&self) -> Result<()> {
        *self.inner.callback.lock().unwrap() = None;
        Ok(())
    }

    #[wasm_bindgen(js_name = "stop")]
    pub async fn stop_notification_task(&self) -> Result<()> {
        let inner = &self.inner;
        if inner.task_running.load(Ordering::SeqCst) {
            inner.task_running.store(false, Ordering::SeqCst);
            inner.task_ctl.signal(()).await.map_err(|err| JsValue::from_str(&err.to_string()))?;
        }
        Ok(())
    }
}

// impl TryFrom<JsValue> for EventDispatcher {
//     type Error = Error;

//     fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
//         Ok(ref_from_abi!(EventDispatcher, &js_value)?)
//     }
// }
