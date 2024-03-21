use crate::service::Service;
use crate::signals::Shutdown;
use crate::trace;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct Core {
    pub keep_running: AtomicBool,
    services: Mutex<Vec<Arc<dyn Service>>>,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    pub fn new() -> Core {
        Core { keep_running: AtomicBool::new(true), services: Mutex::new(Vec::new()) }
    }

    pub fn bind<T>(&self, service: Arc<T>)
    where
        T: Service,
    {
        self.services.lock().unwrap().push(service);
    }

    pub fn find(&self, ident: &'static str) -> Option<Arc<dyn Service>> {
        self.services.lock().unwrap().iter().find(|s| (*s).clone().ident() == ident).cloned()
    }

    /// Starts all services and blocks waiting to join them. For performing other operations in between
    /// use start and join explicitly
    pub fn run(self: &Arc<Core>) {
        self.join(self.start());
    }

    /// Start all services and return `std::thread` join handles
    pub fn start(self: &Arc<Core>) -> Vec<std::thread::JoinHandle<()>> {
        let mut workers = Vec::new();
        for service in self.services.lock().unwrap().iter() {
            workers.append(&mut service.clone().start(self.clone()));
        }
        trace!("core is starting {} workers", workers.len());
        workers
    }

    /// Join workers previously returned from `start`
    pub fn join(&self, workers: Vec<std::thread::JoinHandle<()>>) {
        for worker in workers {
            match worker.join() {
                Ok(()) => {}
                Err(err) => {
                    trace!("thread join failure: {:?}", err);
                }
            }
        }

        // Drop all services and cleanup
        self.services.lock().unwrap().clear();

        trace!("... core is shut down");
    }
}

impl Shutdown for Core {
    fn shutdown(self: &Arc<Core>) {
        if self.keep_running.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }

        trace!("signaling core shutdown...");

        {
            for service in self.services.lock().unwrap().iter() {
                let ident = service.clone().ident();
                trace!("shutting down: {}", ident);
                service.clone().stop();
            }
        }

        trace!("core is shutting down...");
    }
}
