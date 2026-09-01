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

use crate::stats::SS_TIER as SS;
/// The two rarities that get named instead of counted by grade. Every Angelic
/// and Unholy item is SS-graded, so without taking them back out of the grade
/// count one drop would appear twice on the line.
const NAMED: [&str; 2] = ["Unholy", "Angelic"];

/// The application Discord knows this app by. It names the artwork the status
/// is drawn with and ships inside every build: public by design, not a secret.
const APP_ID: &str = "1537867623281467452";

/// Discord's own limit on activity updates is five in twenty seconds — one
/// every four. This was fifteen, and with a three-second poll on top of it a
/// zone change could take eighteen seconds to reach the profile: long enough
/// that the player has left the zone again, which reads as the status being
/// broken rather than slow.
const SEND_GAP: Duration = Duration::from_secs(4);
/// How long an activity is given to be acknowledged before the client holding
/// it is written off. See `Link`.
const ANSWER_GAP: Duration = Duration::from_secs(5);
/// Discord may simply not be running, and asking is a connection that fails.
const RETRY_GAP: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_secs(1);

/// Discord truncates a longer line itself; doing it here keeps the cut on a
/// character boundary and in a place we chose.
const LINE: usize = 120;

static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Everything the status is made of. Comparing one against the last one sent is
/// what keeps the app off Discord's rate limit while the player stands still.
#[derive(PartialEq, Clone)]
struct Card {
    details: String,
    state: String,
    hover: String,
    /// unix milliseconds; Discord counts the elapsed time itself
    start: i64,
    /// the zone that is currently satanic, when the character is standing in it
    satanic: Option<String>,
}

/// What to call the room the heartbeat named.
///
/// The game names its rooms itself, keyed by exactly the string the heartbeat
/// sends: `Act_02_05` is The Glacial Trail, `Town_01_rm` the Town of Inoya. So
/// the table answers first. Composing a label out of the numbers gives "Act 2 ·
/// Zone 5", which is true and is not what the game or this app's own panels
/// call it, and for a room that is not an act at all it leaves the raw suffix
/// on — `Shadow_Realm_rm`.
///
/// The arithmetic stays underneath as the fallback, for a room a patch adds
/// before the table is rebuilt.
fn zone_label(room: &str) -> String {
    if let Some(name) = crate::say::room(room) {
        return name;
    }
    if let Some(name) = crate::items::room_name(room) {
        return name.into();
    }
    if room.get(..4).is_some_and(|head| head.eq_ignore_ascii_case("town")) {
        return crate::say::say("Town");
    }
    match zone_pair(room) {
        Some((act, zone)) => format!("{} {act} · {} {zone}", crate::say::say("Act"), crate::say::say("Zone")),
        None => room.trim_end_matches("_rm").replace('_', " "),
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

/// The same list the panels draw, and for the same reason it is a list at all:
/// a character on the fourth difficulty was shown as nothing here while the
/// dashboard beside it read "Inferno".
const DIFFICULTIES: [&str; 4] = ["Normal", "Nightmare", "Hell", "Inferno"];

/// The badge, and the zone it names.
///
/// The game states outright when the character is in the satanic zone, but that
/// flag rides the same rare heartbeat the room does. It is held against the act,
/// which every save states, so walking out of the act clears the badge without
/// waiting for a heartbeat.
fn satanic_badge(here: bool, zone: Option<&str>, act: i64) -> Option<String> {
    let zone = zone.filter(|_| here)?;
    zone_pair(zone)
        .filter(|(zone_act, _)| i64::from(*zone_act) == act)
        .map(|_| zone_label(zone))
}

fn build(app: &AppHandle) -> Card {
    let shared = app.state::<Shared>();
    // the same order the pusher locks in, and neither holds both at once
    let status = shared.status().text();
    let stats = shared.stats();
    let snap = stats.snapshot(status);
    let start = stats.started_ms() as i64;
    let named: i64 = NAMED.iter().filter_map(|r| snap.items.get(*r)).map(|i| i.total).sum();
    let chase = (stats.graded(SS) - named).max(0);
    drop(stats);

    // Where the character is, as coarsely as the game will state it.
    //
    // The room would be the better line, but it arrives only in the game's own
    // state packet, which since the August 2026 patch comes about twenty times
    // less often — so it usually names a zone the player has
    // left, and a confidently wrong place is worse than an honest act. The act
    // comes with every save.
    //
    // Discord rejects an empty line and drops the connection over it, so an act
    // the game has not stated yet still needs a line of our own.
    let mut where_at = match snap.act {
        act if act > 0 => format!("{} {act}", crate::say::say("Act")),
        _ => crate::say::say("Somewhere in Hero Siege"),
    };
    if let Some(c) = &snap.character {
        let mode = DIFFICULTIES.get(c.difficulty as usize).copied();
        if let Some(mode) = mode {
            where_at.push_str(" · ");
            where_at.push_str(&crate::say::say(mode));
        }
        if c.hardcore {
            where_at.push(' ');
            where_at.push_str(&crate::say::say("HC"));
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
            haul.push(format!("{count} {}", crate::say::say(rarity)));
        }
    }
    if snap.gold.earned > 0 {
        haul.push(format!("{} {}", compact(snap.gold.earned), crate::say::say("gold")));
    }
    let state = if haul.is_empty() { crate::say::say("just started") } else { haul.join(" · ") };

    // the character's own progress, kept for the tooltip: the two visible lines
    // belong to the run
    let hover = match &snap.character {
        Some(c) => format!(
            "HS Tracker · {} {} · {} {}",
            crate::say::say("level"),
            c.level,
            crate::say::say("hero level"),
            c.herolevel,
        ),
        None => "HS Tracker".to_string(),
    };

    let satanic = satanic_badge(
        snap.satanic_here,
        snap.satanic_zone.as_ref().map(|sz| sz.zone.as_str()),
        snap.act,
    );

    Card { details: clip(where_at, LINE), state: clip(state, LINE), hover: clip(hover, LINE), start, satanic }
}

fn send(client: &mut DiscordIpcClient, card: &Card) -> Result<(), Error> {
    let mut assets = Assets::new().large_image("logo").large_text(card.hover.as_str());
    if let Some(zone) = &card.satanic {
        assets = assets
            .small_image("satanic")
            .small_text(format!("{} · {zone}", crate::say::say("Satanic Zone")));
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

/// A connected client, on a thread of its own.
///
/// Discord's end is a named pipe, and on Windows the crate reads it with
/// `read_exact` on a `File`, which has no timeout: `send` hands over an activity
/// and waits for an answer that may never come. On the status thread itself that
/// wait is permanent and silent — the profile freezes on whatever it last
/// showed.
///
/// Out here a wedged client costs one stopped thread. The loop stops hearing
/// answers, writes the client off and connects again.
struct Link {
    cards: std::sync::mpsc::Sender<Card>,
    answers: std::sync::mpsc::Receiver<bool>,
}

fn open_link() -> Link {
    let (cards, from_loop) = std::sync::mpsc::channel::<Card>();
    let (to_loop, answers) = std::sync::mpsc::channel::<bool>();
    std::thread::spawn(move || {
        // Connecting is inside here, not outside. The handshake ends in a read
        // on the same pipe, with the same absence of a timeout as everything
        // else on it — so a Discord that accepts the connection and never
        // answers would wedge whichever thread called this. Out here that is
        // the thread that exists to be wedged.
        let mut client = DiscordIpcClient::new(APP_ID);
        // Discord is simply not running, most of the time
        if client.connect().is_err() {
            let _ = to_loop.send(false);
            return;
        }
        for card in from_loop {
            let ok = send(&mut client, &card).is_ok();
            // Nobody listening means the loop has moved on without this client,
            // which is what a timeout above looks like from in here.
            if to_loop.send(ok).is_err() || !ok {
                break;
            }
        }
        // Reached only by a client that is still answering: one that is not
        // never gets here, and that is the point.
        let _ = client.clear_activity();
        let _ = client.recv();
        let _ = client.close();
    });
    Link { cards, answers }
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut link: Option<Link> = None;
        let mut shown: Option<Card> = None;
        let mut sent_at = Instant::now() - SEND_GAP;
        let mut next_try = Instant::now();
        loop {
            std::thread::sleep(POLL);

            if !(ENABLED.load(Ordering::Relaxed) && crate::sniffer::game_running()) {
                // The game closed or the setting went off: take the status down
                // rather than leave a finished run standing on the profile.
                // Dropping the link ends its thread, and the thread clears the
                // activity on its way out.
                link = None;
                shown = None;
                continue;
            }

            if link.is_none() {
                if Instant::now() < next_try {
                    continue;
                }
                next_try = Instant::now() + RETRY_GAP;
                // Whether Discord is there is not known yet and is not waited
                // for: the first card sent finds out, and a failure to connect
                // arrives as the same "no" a failure to send does.
                link = Some(open_link());
                shown = None;
            }

            if sent_at.elapsed() < SEND_GAP {
                continue;
            }
            let card = build(&app);
            if shown.as_ref() == Some(&card) {
                continue;
            }
            let Some(l) = link.as_ref() else { continue };
            // Handed over and then given a bounded wait. Anything else — a
            // client that has gone, a send that failed, an answer that never
            // came — is the same answer here: this client is finished, and the
            // next round makes another.
            let handed = l.cards.send(card.clone()).is_ok();
            let answered = handed && matches!(l.answers.recv_timeout(ANSWER_GAP), Ok(true));
            if !answered {
                link = None;
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
        // the game's own names, for the rooms it names
        assert_eq!(zone_label("Act_08_02"), "Flooded Plains");
        assert_eq!(zone_label("Act_02_05"), "The Glacial Trail");
        assert_eq!(zone_label("Town_01_rm"), "Town of Inoya");
        assert_eq!(zone_label("Shadow_Realm_rm"), "Shadow Realm", "and not \"Shadow Realm rm\"");

        // and the arithmetic underneath, for a room this table has not caught up
        // with — a patch can add one before the table is rebuilt
        assert_eq!(zone_label("Act_44_02"), "Act 44 · Zone 2");
        assert_eq!(zone_label("Town_01"), "Town");
        assert_eq!(zone_label("Chaos_Tower"), "Chaos Tower");
        assert_eq!(zone_label("Nowhere_At_All_rm"), "Nowhere At All");
    }

    #[test]
    fn a_room_and_a_satanic_zone_name_the_same_place() {
        assert_eq!(zone_pair("Act_08_02"), zone_pair("SZ_8_2"));
        assert_ne!(zone_pair("Act_08_02"), zone_pair("SZ_8_3"));
        assert_eq!(zone_pair("Town"), None);
    }

    #[test]
    fn the_badge_goes_out_with_the_act() {
        // the game said so, and the act agrees
        assert_eq!(satanic_badge(true, Some("Act_08_02"), 8).as_deref(), Some("Flooded Plains"));

        // it said so before the player walked into another act; the save has
        // moved on and the heartbeat has not
        assert_eq!(satanic_badge(true, Some("Act_08_02"), 3), None);

        // and it never said so at all
        assert_eq!(satanic_badge(false, Some("Act_08_02"), 8), None);
        assert_eq!(satanic_badge(true, None, 8), None);
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
