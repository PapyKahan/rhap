use std::fs::OpenOptions;
use std::io::Write;

/// Centralized logging function - writes messages only to log file
pub fn log_to_file_only(level: &str, message: &str) {
    // Try to write to log file with error checking
    match OpenOptions::new().create(true).append(true).open("rhap_debug.log") {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "[{}] {}", level, message) {
                eprintln!("[LOG ERROR] Failed to write to log file: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[LOG ERROR] Failed to open log file: {}", e);
        }
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::log_to_file_and_console("INFO", &format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::log_to_file_and_console("ERROR", &format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logging::log_to_file_and_console("DEBUG", &format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_alsa {
    ($($arg:tt)*) => {
        $crate::logging::log_to_file_and_console("ALSA", &format!($($arg)*));
    };
}