use std::io::{self, IsTerminal};
use std::sync::atomic::{AtomicBool, Ordering};

/// ANSI color codes for logging
/// Colors are only applied to console output, not to file logs
pub struct LogColors;

static COLORS_ENABLED: AtomicBool = AtomicBool::new(true);

impl LogColors {
    // Note: Color constants removed - colors are now applied by the CustomFormatter in main.rs
    // based on message content patterns. This avoids ANSI codes being embedded in strings.
    
    /// Initialize color support detection
    /// Should be called once at startup
    pub fn init() {
        // Check if NO_COLOR environment variable is set (common convention to disable colors)
        let no_color = std::env::var("NO_COLOR").is_ok();
        
        // Check if stderr is a terminal (where tracing logs go)
        let is_terminal = io::stderr().is_terminal();
        
        // On Windows, enable virtual terminal processing
        #[cfg(windows)]
        let _ = Self::enable_windows_vt();
        
        // Enable colors only if:
        // 1. NO_COLOR is not set
        // 2. We're writing to a terminal
        // 3. On Windows, also check if ANSI is supported (Windows 10+)
        let enabled = !no_color && is_terminal && Self::check_windows_ansi_support();
        
        COLORS_ENABLED.store(enabled, Ordering::Relaxed);
    }
    
    /// Enable virtual terminal processing on Windows
    /// This allows ANSI escape codes to work in Windows console
    #[cfg(windows)]
    fn enable_windows_vt() -> bool {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Console::{
            GetStdHandle, GetConsoleMode, SetConsoleMode,
            ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE
        };
        
        unsafe {
            let handle = GetStdHandle(STD_ERROR_HANDLE);
            if handle == INVALID_HANDLE_VALUE {
                return false;
            }
            
            let mut mode: u32 = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return false;
            }
            
            // Enable virtual terminal processing
            mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING;
            if SetConsoleMode(handle, mode) == 0 {
                return false;
            }
            
            true
        }
    }
    
    #[cfg(not(windows))]
    fn enable_windows_vt() -> bool {
        true
    }
    
    /// Check if Windows supports ANSI colors
    /// Modern Windows terminals (PowerShell 5.1+, Windows Terminal, etc.) support ANSI
    #[cfg(windows)]
    fn check_windows_ansi_support() -> bool {
        // Modern Windows terminals support ANSI codes
        // PowerShell 5.1+ and Windows Terminal handle them natively
        // If colors don't work, user can set NO_COLOR=1 to disable
        true
    }
    
    #[cfg(not(windows))]
    fn check_windows_ansi_support() -> bool {
        true
    }
    
    /// Check if colors are enabled
    fn colors_enabled() -> bool {
        COLORS_ENABLED.load(Ordering::Relaxed)
    }
    
    /// Check if colors should be used (for tracing-subscriber with_ansi)
    pub fn should_colorize() -> bool {
        Self::colors_enabled()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    /// These functions are kept for API compatibility but no longer add ANSI codes
    pub fn asic_to_bridge(s: &str) -> String {
        s.to_string()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    pub fn bridge_to_asic(s: &str) -> String {
        s.to_string()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    pub fn validation(s: &str) -> String {
        s.to_string()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    pub fn block(s: &str) -> String {
        s.to_string()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    pub fn api(s: &str) -> String {
        s.to_string()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    pub fn error(s: &str) -> String {
        s.to_string()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    pub fn separator(s: &str) -> String {
        s.to_string()
    }
    
    /// Return string as-is (colors are now applied by the formatter, not here)
    pub fn label(s: &str) -> String {
        s.to_string()
    }
}

