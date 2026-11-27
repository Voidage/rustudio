//! Network policy for shipped games (FORK-PLAN §9).
//!
//! The launcher sets `RUSTUDIO_NO_NETWORK=1` unless `game.json` opts in with
//! `"network": true`. The editor and a directly launched player leave it
//! unset and keep network access. A service that would open a connection
//! checks [`blocked`] first and reports the attempt through
//! [`blocked_message`], which logs one line per service and request.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::sync::{Mutex, OnceLock};

pub fn blocked() -> bool {
    blocked_from(std::env::var_os("RUSTUDIO_NO_NETWORK").as_deref())
}

/// Only the literal `1` blocks; an explicitly exported `RUSTUDIO_NO_NETWORK=0`
/// is the developer's escape hatch and keeps network access.
fn blocked_from(value: Option<&OsStr>) -> bool {
    value.is_some_and(|v| v == "1")
}

/// `RUSTUDIO_HTTP_ENABLED=1` (a game bundle): run with HttpService enabled,
/// the effect of Studio's Experience Settings > "Allow HTTP requests". The
/// editor leaves it unset and keeps the place's own setting.
pub fn http_enabled() -> bool {
    std::env::var_os("RUSTUDIO_HTTP_ENABLED").is_some_and(|v| v == "1")
}

/// The line for a blocked attempt and its deduplication. The first call for
/// a service/request pair prints it; later calls stay quiet.
pub fn blocked_message(service: &str, what: &str) -> String {
    let message = format!("{service} request blocked: this game has no network ({what})");
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let key = format!("{service}\u{0}{what}");
    if seen.lock().unwrap_or_else(|e| e.into_inner()).insert(key) {
        eprintln!("[net] {message}");
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_literal_1_blocks() {
        assert!(!blocked_from(None));
        assert!(!blocked_from(Some(OsStr::new("0"))));
        assert!(!blocked_from(Some(OsStr::new("yes"))));
        assert!(blocked_from(Some(OsStr::new("1"))));
    }

    #[test]
    fn the_blocked_line_names_the_service_and_request() {
        let line = blocked_message("HttpService", "GET https://example.com/a");
        assert_eq!(
            line,
            "HttpService request blocked: this game has no network (GET https://example.com/a)"
        );
    }
}
