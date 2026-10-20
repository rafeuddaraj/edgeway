//! Minimal logging to standard error.
//!
//! GUI apps on macOS send stderr to the unified log only when launched from a
//! terminal, so this stays free when nobody is watching. Set `EDGEWAY_QUIET=1`
//! to silence it entirely.

use std::sync::OnceLock;

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("EDGEWAY_QUIET").is_none())
}

macro_rules! ew_log {
    ($($arg:tt)*) => {
        if $crate::log::enabled() {
            eprintln!("[edgeway] {}", format_args!($($arg)*));
        }
    };
}
