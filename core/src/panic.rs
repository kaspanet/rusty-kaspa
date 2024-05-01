use kaspa_core::error;
use std::{panic, process};

/// Configures the panic hook to exit the program on every panic
pub fn configure_panic() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Invoke the default hook and exit the process
        default_hook(panic_info);
        println!("Exiting...");

        // Get the panic location and message
        let (file, line) = match panic_info.location() {
            Some(location) => (location.file(), location.line()),
            None => ("unknown", 0),
        };

        let message = match panic_info.payload().downcast_ref::<&str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "unknown",
            },
        };
        // Log the panic
        error!("Panic at {}:{}: {}", file, line, message);

        process::exit(1);
    }));
}
