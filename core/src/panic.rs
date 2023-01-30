use std::{panic, process};

/// Configures the panic hook to exit the program on every panic
pub fn configure_panic() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Invoke the default hook and exit the process
        default_hook(panic_info);
        println!("Exiting...");
        // TODO: setup a wait time and fold the log system properly
        process::exit(1);
    }));
}
