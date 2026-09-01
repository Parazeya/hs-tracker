//! The words this side prints, in whatever language the page is wearing.
//!
//! The catalogue is not here. It is generated into `src/lang/*.json` and read
//! by the runtime on the page, and shipping a second copy into the binary
//! would have meant two tables to keep level with each other. Instead the page
//! hands the whole of its own vocabulary over whenever the language settles,
//! and this side looks a line up in it: the tray menu, the file dialogs, the
//! errors it hands back, and the Discord card.
//!
//! A word nobody sent comes back as its own English, which is what a missing
//! entry does everywhere else. That is also the state the app starts in — the
//! tray is built before the page has finished loading — so the menu is rebuilt
//! once the words arrive rather than assumed to be right the first time.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

fn table() -> &'static RwLock<HashMap<String, String>> {
    static WORDS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
    WORDS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Take the page's vocabulary. Replaces whatever was held before: a switch to
/// English sends an empty table, and every line falls back to its own English.
pub fn learn(words: HashMap<String, String>) {
    if let Ok(mut held) = table().write() {
        *held = words;
    }
}

/// One line, in the language the page last sent.
pub fn say(english: &str) -> String {
    table()
        .read()
        .ok()
        .and_then(|words| words.get(english).cloned())
        .filter(|said| !said.is_empty())
        .unwrap_or_else(|| english.to_string())
}

/// The name of a room, by the key the game's own heartbeat sends. The page
/// sends these under a prefix of their own because a room key is not English
/// and would collide with nothing, but reading it back as one would be a lie
/// about where the word came from.
pub fn room(key: &str) -> Option<String> {
    table()
        .read()
        .ok()
        .and_then(|words| words.get(&format!("room:{key}")).cloned())
        .filter(|said| !said.is_empty())
}
