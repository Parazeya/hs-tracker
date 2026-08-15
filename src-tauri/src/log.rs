//! What went wrong, written down.
//!
//! A released build has no console: `eprintln!` goes nowhere, a panic takes the
//! window with it and leaves the player with nothing to send, and an error in
//! the front end blanks a panel in silence. All three end up here instead.
//!
//! It is a record of trouble, not a trace of everything: warnings, errors and
//! one line a session saying what was started. The packet log in Settings is a
//! separate thing and stays that way — this one is small enough to paste into a
//! chat, and is kept that way on purpose.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Rolled at half a megabyte, one older copy kept. A log nobody can send is a
/// log nobody reads.
const KEEP: u64 = 512 * 1024;

static WRITING: Mutex<()> = Mutex::new(());

pub fn path() -> PathBuf {
    crate::data_dir().join("hs-tracker.log")
}

/// Seconds since the epoch as a date and time, in UTC and labelled as such.
/// Nothing here needs a calendar crate — the point is to line entries up
/// against a Windows event or a dump, and an unlabelled clock three hours off
/// the one in Event Viewer makes that harder, not easier.
fn stamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let (days, rest) = (now / 86_400, now % 86_400);
    let (mut year, mut left) = (1970, days);
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let length = if leap { 366 } else { 365 };
        if left < length {
            break;
        }
        left -= length;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0;
    while month < 12 && left >= months[month] {
        left -= months[month];
        month += 1;
    }
    format!(
        "{year:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
        month + 1,
        left + 1,
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

pub fn say(level: &str, message: &str) {
    // a developer watching a terminal should still see it
    eprintln!("{level}: {message}");

    let Ok(_held) = WRITING.lock() else { return };
    let file = path();
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > KEEP) {
        let _ = std::fs::rename(&file, file.with_extension("log.1"));
    }
    let mut line = String::new();
    let _ = writeln!(line, "{} {:<5} {}", stamp(), level, message.replace('\n', "\n      "));
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = out.write_all(line.as_bytes());
    }
}

pub fn warn(message: impl AsRef<str>) {
    say("warn", message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    say("error", message.as_ref());
}

/// Installed before anything else can go wrong.
pub fn init(version: &str) {
    std::panic::set_hook(Box::new(|info| {
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic with nothing to say".into());
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let trace = std::backtrace::Backtrace::force_capture();
        say("panic", &format!("{what}\n  at {at}\n{trace}"));
    }));
    say(
        "start",
        &format!("HS Tracker {version} on {} ({})", std::env::consts::OS, std::env::consts::ARCH),
    );
}
