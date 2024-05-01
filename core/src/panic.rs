use kaspa_core::error;
use std::{panic, process, thread};

/// Configures the panic hook to exit the program on every panic
pub fn configure_panic() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Get the panic location details
        let (file, line, column) = match panic_info.location() {
            Some(location) => (location.file(), location.line(), location.column()),
            None => ("unknown", 0, 0),
        };

        let message = match panic_info.payload().downcast_ref::<&str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<dyn Any>",
            },
        };
        // Get the thread name
        let current_thread = thread::current();
        let thread_name = current_thread.name().unwrap_or("<unnamed>");
        // Log the panic
        error!("thread '{}' panicked at {}:{}:{}: {}", thread_name, file, line, column, message);
        // Invoke the default hook as well, since it might include additional info such as the full backtrace
        default_hook(panic_info);
        println!("Exiting...");
        process::exit(1);
    }));
}
