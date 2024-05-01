use kaspa_core::error;
use std::{panic, process, thread};

/// Configures the panic hook to exit the program on every panic
pub fn configure_panic() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Invoke the default hook and exit the process
        default_hook(panic_info);
        println!("Exiting...");

        // Get the panic details
        let (file, line, column) = match panic_info.location() {
            Some(location) => (location.file(), location.line(), location.column()),
            None => ("unknown", 0, 0),
        };

        let message = match panic_info.payload().downcast_ref::<&str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "unknown",
            },
        };
        // Get the thread name
        let current_thread = thread::current();
        let thread_name = match current_thread.name() {
            Some(name) => name,
            None => "unnamed",
        };
        // Log the panic
        error!("Panic at the thread {} at {}:{}:{}: {}", thread_name, file, line, column, message);

        process::exit(1);
    }));
}
