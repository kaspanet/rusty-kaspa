use std::sync::atomic::{AtomicBool,Ordering};
use std::sync::{Arc,Mutex};
use crate::service::Service;
use crate::trace;

pub struct Core {
    pub keep_running : AtomicBool,
    services : Mutex<Vec<Arc<dyn Service>>>,

}

impl Core {
    pub fn new() -> Core {
        
        Core {
            keep_running: AtomicBool::new(true),
            services : Mutex::new(Vec::new()),
        }
    }

    pub fn shutdown(self: &Arc<Core>) {

        let keep_running = self.keep_running.load(Ordering::SeqCst);
        if !keep_running {
            return;
        }

        trace!("signaling core shutdown...");
        self.keep_running.store(false, Ordering::SeqCst);

        {
            for service in self.services.lock().unwrap().iter() {
                let ident = service.clone().ident();
                trace!("shutting down: {}", ident);
                service.clone().stop();
            }
        }
        
        trace!("core is shutting down...");
    }

    pub fn bind<T>(&self, service : Arc<T>) where T : Service {
        self.services.lock().unwrap().push(service);
    }

    pub fn run(self : &Arc<Core>) {

        let mut workers = Vec::new();
        for service in self.services.lock().unwrap().iter() {
            workers.append(&mut service.clone().start(self.clone()));
        }
        trace!("core is starting {} workers", workers.len());

        // println!("starting termination...");
        for worker in workers {
            match worker.join() {
                Ok(()) => {},
                Err(err) => {
                    trace!("thread join failure: {:?}", err);
                }
            }
        }
    
        trace!("... core is shut down");
    }

}

