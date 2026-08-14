//! What the run looks like from the outside: the session in Discord.
//!
//! The Discord client on the same machine listens on a named pipe (Windows) or
//! a socket in the runtime directory (everywhere else). An application that
//! connects to it may set one activity, which Discord then shows under the
//! player's name. Nothing travels further than that pipe — the status is drawn
//! by the local client, and the tracker still talks to no server of its own.
//!
//! The status exists only while Hero Siege does. The app starts with the
//! machine and sits in the tray all day; a status that announced it all day
//! would say nothing about what the player is actually doing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::error::Error;
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use tauri::{AppHandle, Manager};

use crate::sniffer::Shared;

/// The top grade an item can carry, and the one number a farmer wants: how many
/// chase items the run has produced, whatever colour they came out.
const SS: i64 = 6;
/// The two rarities that get named instead of counted by grade. Every Angelic
/// and Unholy item is SS-graded, so without taking them back out of the grade
/// count one drop would appear twice on the line.
const NAMED: [&str; 2] = ["Unholy", "Angelic"];

/// The application Discord knows this app by. It names the artwork the status
/// is drawn with and ships inside every build: public by design, not a secret.
const APP_ID: &str = "1537867623281467452";

/// Discord takes five activity updates per twenty seconds. One per fifteen is
/// well inside that and still keeps up with a run.
const SEND_GAP: Duration = Duration::from_secs(15);
/// Discord may simply not be running, and asking is a connection that fails.
const RETRY_GAP: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_secs(3);

/// Discord truncates a longer line itself; doing it here keeps the cut on a
/// character boundary and in a place we chose.
const LINE: usize = 120;

static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Everything the status is made of. Comparing one against the last one sent is
/// what keeps the app off Discord's rate limit while the player stands still.
#[derive(PartialEq)]
struct Card {
    details: String,
    state: String,
    hover: String,
    /// unix milliseconds; Discord counts the elapsed time itself
    start: i64,
    /// the character is standing in the zone that is currently satanic
    satanic: bool,
}

/// "Act_08_02" is the game's name for a room; the player thinks of it as
/// "Act 8 · Zone 2".
fn zone_label(room: &str) -> String {
    if room.get(..4).is_some_and(|head| head.eq_ignore_ascii_case("town")) {
        return "Town".into();
    }
    match zone_pair(room) {
        Some((act, zone)) => format!("Act {act} · Zone {zone}"),
        None => room.replace('_', " "),
    }
}

/// The act and zone out of a room ("Act_08_02") or a satanic zone ("SZ_8_2"),
/// which name the same place two different ways.
fn zone_pair(name: &str) -> Option<(u32, u32)> {
    let mut parts = name.split('_').skip(1);
    let act = parts.next()?.parse().ok()?;
    let zone = parts.next()?.parse().ok()?;
    Some((act, zone))
}

/// Two short lines have no room for a full number.
fn compact(n: i64) -> String {
    let mag = n.unsigned_abs() as f64;
    let (value, unit) = match mag {
        m if m < 1_000.0 => return n.to_string(),
        m if m < 1_000_000.0 => (n as f64 / 1e3, "k"),
        m if m < 1_000_000_000.0 => (n as f64 / 1e6, "M"),
        _ => (n as f64 / 1e9, "B"),
    };
    if value.abs() < 10.0 {
        format!("{value:.1}{unit}")
    } else {
        format!("{value:.0}{unit}")
    }
}

fn clip(mut text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    text = text.chars().take(limit.saturating_sub(1)).collect();
    text.push('…');
    text
}

const DIFFICULTIES: [&str; 3] = ["Normal", "Nightmare", "Hell"];

fn build(app: &AppHandle) -> Card {
    let shared = app.state::<Shared>();
    // the same order the pusher locks in, and neither holds both at once
    let status = shared.status.lock().unwrap().text();
    let stats = shared.stats.lock().unwrap();
    let snap = stats.snapshot(status);
    let start = stats.started_ms() as i64;
    let named: i64 = NAMED.iter().filter_map(|r| snap.items.get(*r)).map(|i| i.total).sum();
    let chase = (stats.graded(SS) - named).max(0);
    drop(stats);

    // Where the character is, and what it is playing on. Discord rejects an
    // empty line and would take the connection down with it, so a room the game
    // has not named yet is a line of our own.
    let mut where_at = match snap.room.as_deref().filter(|room| !room.is_empty()) {
        Some(room) => zone_label(room),
        None => "Somewhere in Hero Siege".into(),
    };
    if let Some(c) = &snap.character {
        let mode = DIFFICULTIES.get(c.difficulty as usize).copied();
        if let Some(mode) = mode {
            where_at.push_str(" · ");
            where_at.push_str(mode);
        }
        if c.hardcore {
            where_at.push_str(" HC");
        }
    }

    // What the run has produced. The drops come first: they are the point of
    // the app, and the gold is the number that always moves anyway. Grades below
    // SS are left out — a line naming every rarity is a line nobody reads.
    let mut haul: Vec<String> = Vec::new();
    if chase > 0 {
        haul.push(format!("{chase} SS"));
    }
    for rarity in NAMED {
        let count = snap.items.get(rarity).map_or(0, |item| item.total);
        if count > 0 {
            haul.push(format!("{count} {rarity}"));
        }
    }
    if snap.gold.earned > 0 {
        haul.push(format!("{} gold", compact(snap.gold.earned)));
    }
    let state = if haul.is_empty() { "just started".to_string() } else { haul.join(" · ") };

    // the character's own progress, kept for the tooltip: the two visible lines
    // belong to the run
    let hover = match &snap.character {
        Some(c) => format!("HS Tracker · level {} · hero level {}", c.level, c.herolevel),
        None => "HS Tracker".to_string(),
    };

    let satanic = match (snap.room.as_deref(), &snap.satanic_zone) {
        (Some(room), Some(sz)) => zone_pair(room).is_some() && zone_pair(room) == zone_pair(&sz.zone),
        _ => false,
    };

    Card { details: clip(where_at, LINE), state: clip(state, LINE), hover: clip(hover, LINE), start, satanic }
}

fn send(client: &mut DiscordIpcClient, card: &Card) -> Result<(), Error> {
    let mut assets = Assets::new().large_image("logo").large_text(card.hover.as_str());
    if card.satanic {
        assets = assets.small_image("satanic").small_text("Standing in the Satanic Zone");
    }
    client.set_activity(
        Activity::new()
            .details(card.details.as_str())
            .state(card.state.as_str())
            .assets(assets)
            .timestamps(Timestamps::new().start(card.start)),
    )?;
    // Discord answers every activity it is handed. Nothing here wants the
    // answer, but one nobody reads stays in the pipe, and a pipe that fills up
    // is a write that never returns.
    client.recv()?;
    Ok(())
}

fn drop_client(client: &mut Option<DiscordIpcClient>, clear: bool) {
    if let Some(mut c) = client.take() {
        if clear {
            let _ = c.clear_activity();
            let _ = c.recv();
        }
        let _ = c.close();
    }
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut client: Option<DiscordIpcClient> = None;
        let mut shown: Option<Card> = None;
        let mut sent_at = Instant::now() - SEND_GAP;
        let mut next_try = Instant::now();
        loop {
            std::thread::sleep(POLL);

            if !(ENABLED.load(Ordering::Relaxed) && crate::sniffer::game_running()) {
                // the game closed or the setting went off: take the status down
                // rather than leave a finished run standing on the profile
                drop_client(&mut client, true);
                shown = None;
                continue;
            }

            if client.is_none() {
                if Instant::now() < next_try {
                    continue;
                }
                next_try = Instant::now() + RETRY_GAP;
                let mut fresh = DiscordIpcClient::new(APP_ID);
                // Discord is simply not running, most of the time
                if fresh.connect().is_err() {
                    continue;
                }
                client = Some(fresh);
                shown = None;
            }

            if sent_at.elapsed() < SEND_GAP {
                continue;
            }
            let card = build(&app);
            if shown.as_ref() == Some(&card) {
                continue;
            }
            let Some(c) = client.as_mut() else { continue };
            if send(c, &card).is_err() {
                // Discord went away mid-run; the next round reconnects
                drop_client(&mut client, false);
                shown = None;
                next_try = Instant::now() + RETRY_GAP;
                continue;
            }
            sent_at = Instant::now();
            shown = Some(card);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooms_read_as_places() {
        assert_eq!(zone_label("Act_08_02"), "Act 8 · Zone 2");
        assert_eq!(zone_label("Town_01"), "Town");
        assert_eq!(zone_label("Chaos_Tower"), "Chaos Tower");
    }

    #[test]
    fn a_room_and_a_satanic_zone_name_the_same_place() {
        assert_eq!(zone_pair("Act_08_02"), zone_pair("SZ_8_2"));
        assert_ne!(zone_pair("Act_08_02"), zone_pair("SZ_8_3"));
        assert_eq!(zone_pair("Town"), None);
    }

    #[test]
    fn long_numbers_shorten() {
        assert_eq!(compact(940), "940");
        assert_eq!(compact(7_317), "7.3k");
        assert_eq!(compact(42_000), "42k");
        assert_eq!(compact(2_400_000), "2.4M");
        assert_eq!(compact(3_140_000_000), "3.1B");
    }

    #[test]
    fn a_line_is_cut_where_we_choose() {
        assert_eq!(clip("Act 8".into(), 120), "Act 8");
        assert_eq!(clip("абвгд".into(), 3), "аб…");
    }
}
