use std::sync::Arc;
use std::time::Duration;
use std::thread;
use std::thread::{spawn,JoinHandle};
use std::sync::atomic::{Ordering, AtomicBool, AtomicU64};
use num_format::{Locale, ToFormattedString};

use crate::trace;
use crate::core::Core;
use crate::service::Service;

pub struct Monitor {
    terminate : AtomicBool,
    send_count : Arc<AtomicU64>,
    recv_count : Arc<AtomicU64>,
}

impl Monitor {

    pub fn new(
        send_count : Arc<AtomicU64>,
        recv_count : Arc<AtomicU64>,
    ) -> Monitor {
        Monitor {
            terminate : AtomicBool::new(false),
            send_count,
            recv_count,
        }
    }

    pub fn worker(self:&Arc<Monitor>, _core : Arc<Core>) {

        let mut last_send = 0u64;
        let mut last_recv = 0u64;

        loop {
            thread::sleep(Duration::from_millis(1000));
            
            let send_count = self.send_count.load(Ordering::SeqCst);
            let recv_count = self.recv_count.load(Ordering::SeqCst);
            
            let recv_rate = recv_count - last_recv;
            let send_rate = send_count - last_send;
            last_recv = recv_count;
            last_send = send_count;
            let pending = send_count - recv_count;

            trace!("sent: {} received: {} pending: {} -> send rate tx/s: {} receive rate tx/s: {}",
                send_count.to_formatted_string(&Locale::en),
                recv_count.to_formatted_string(&Locale::en),
                pending.to_formatted_string(&Locale::en),
                send_rate.to_formatted_string(&Locale::en),
                recv_rate.to_formatted_string(&Locale::en),
            );
            // trace!("monitor ... {}",chrono::offset::Local::now());

            if self.terminate.load(Ordering::SeqCst) == true {
                break;
            }
        }

        trace!("monitor thread exiting");
    }

}

// service trait implementation for Monitor
impl Service for Monitor {
    
    fn ident(self:Arc<Monitor>) -> String {
        "monitor".into()
    }

    fn start(self:Arc<Monitor>, core : Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self:Arc<Monitor>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}
