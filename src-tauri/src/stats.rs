use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::parser::GameEvent;

/// Only the tests care which season is live; the engine reads "seasonal" off
/// the character's own season number, so a new season needs no code change.
#[cfg(test)]
pub const CURRENT_SEASON: i64 = 9;

pub const RARITIES: &[(&str, &str)] = &[
    ("1", "Common"),
    ("2", "Superior"),
    ("3", "Rare"),
    ("4", "Set"),
    ("5", "Mythic"),
    ("6", "Satanic"),
    ("7", "Angelic"),
    ("8", "Blessed"),
    ("9", "Heroic"),
    ("10", "Unholy"),
];

pub const JOURNAL_RARITIES: &[&str] = &["Satanic", "Set", "Heroic", "Angelic", "Unholy"];

/// The top grade an item can carry, and the one number a farmer judges a run by:
/// how many chase items it produced, whatever colour they came out. Grades run
/// 1..6 and the interface writes them D, C, B, A, S, SS.
pub const SS_TIER: i64 = 6;

/// How long a bank balance step waits for the deposit packet that explains it.
const IN_FLIGHT: Duration = Duration::from_secs(15);

/// How long a run goes without a sign of life before it stops being a run.
///
/// Five minutes is long enough that a fight with a boss, a trip to town or a
/// slow stretch of a map is not mistaken for a break, and short enough that a
/// break does not end up divided into the per-hour figures.
const IDLE_AFTER: Duration = Duration::from_secs(300);

// stack resources by item type
const RESOURCES: &[(i64, &str)] = &[(12, "keys"), (13, "collectibles"), (14, "materials"), (15, "socketables")];

/// What a character wears and carries: helmets through charms, vials, and orbs.
/// The grade counters are about gear and nothing else — a key or a reagent has
/// a grade of its own and would otherwise sit in the SS column beside a weapon,
/// which is not the thing the column is counting.
///
/// Vials were missing and are equipped like charms. Being neither gear nor a
/// stackable, one landed in its rarity column and in no grade column at all, so
/// the panel showed twelve Heroic beside ten SS with nothing to explain the
/// gap. Every Heroic item in the tables is graded SS, so a Heroic that is not
/// an SS could never have been anything but this.
const GEAR: [i64; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 18];

/// The relic type. Relics are not gear and not a resource — they reach no
/// counter at all — so this is here for the one thing that asks about them:
/// whether a drop is a relic the player ticked. See `hunted_relic`.
const RELIC: i64 = 16;

/// Containers, which carry a real rarity and are worth keeping in the tables —
/// the game shows an Angelic Vault in gold and a Superior one in blue — but
/// which nobody wants a chime for. They come in seven rarities under one
/// display name, so a rarity alert on them fires constantly and says nothing
/// about what was actually found. Matched by name because that is what reaches
/// here; the seven share it exactly.
fn is_container(name: &str) -> bool {
    name.to_lowercase().contains("essence vault")
}

/// Keys that drop by the handful and open nothing worth counting: they would
/// bury the Angelic and Satanic keys the counter exists for.
const DULL_KEYS: [&str; 2] = ["basic key", "crystal key"];

/// What the save counts besides kills, in the order it is shown: the bosses the
/// character has put down, then the chests it has opened. The game sends all of
/// its `statistic…` counters on every save, so a session's worth of each is the
/// difference between two saves — exactly how kills already work.
///
/// Keys are the game's own names flattened to letters and digits. A name the
/// game changes simply stops matching: the counter disappears from the panel
/// rather than showing a wrong number.
pub const TALLIES: &[(&str, &str, &str)] = &[
    ("statisticsatankills", "Satan", "boss"),
    ("statisticdamienkills", "Damien", "boss"),
    ("statisticreaperkills", "Reaper", "boss"),
    ("statisticanubiskills", "Anubis", "boss"),
    ("statisticguragkills", "Gurag", "boss"),
    ("statisticmeviuskills", "Mevius", "boss"),
    ("statisticodinkills", "Odin", "boss"),
    ("statisticcthulhukills", "Cthulhu", "boss"),
    ("statistickarpkingkills", "Karp King", "boss"),
    ("statisticuberdamienkills", "Uber Damien", "boss"),
    ("statisticuberreaperkills", "Uber Reaper", "boss"),
    ("statisticuberlunakills", "Uber Luna", "boss"),
    ("statisticuberendrixiakills", "Uber Endrixia", "boss"),
    ("statisticubergabrielkills", "Uber Gabriel", "boss"),
    ("statisticuberkingrakhulkills", "Uber King Rakhul", "boss"),
    ("statisticubersheepkingkills", "Uber Sheep King", "boss"),
    ("statisticubersungleekills", "Uber Sung Lee", "boss"),
    ("statisticuberamunrakills", "Uber Amun Ra", "boss"),
    ("statisticuberarchitectkills", "Uber Architect", "boss"),
    ("statisticuberpapalegbakills", "Uber Papa Legba", "boss"),
    ("statisticubercaptaingrimtidekills", "Uber Captain Grimtide", "boss"),
    ("statisticuberbloodmaidenkills", "Uber Blood Maiden", "boss"),
    ("statisticuberphantomleviathankills", "Uber Phantom Leviathan", "boss"),
    ("statisticuberchaostowerkills", "Uber Chaos Tower", "boss"),
    // Not bosses, and filing them as bosses made the overlay's boss figure go
    // up on a staircase: a Chaos Tower floor counts when it is CLEARED, so
    // stepping to the next one took the count from 45 to 46 with nothing
    // killed. A wormhole is the same shape. They are worth counting and they
    // are worth counting apart.
    ("statisticchaostowerfloorclears", "Chaos Tower floors", "clear"),
    ("statisticwormholeclears", "Wormholes", "clear"),
    ("statisticcommonchestsopened", "Common", "chest"),
    ("statisticrarechestopened", "Rare", "chest"),
    ("statisticcrystalchestopened", "Crystal", "chest"),
    ("statisticrubychestsopened", "Ruby", "chest"),
    ("statisticdungeonchestsopened", "Dungeon", "chest"),
];

#[derive(Clone, Serialize, Deserialize)]
pub struct TallyCount {
    pub label: String,
    /// "boss", "chest" or "clear" — which list it belongs under
    pub group: String,
    pub total: i64,
}

/// Drops worth their own counter, matched by resolved item name. The rune
/// groups follow the game's own grades — S is Qi through Zed, SS is the four
/// level-100 runes. Override the whole list in settings.json if the game
/// regrades anything.
pub fn default_notable() -> Vec<(String, Vec<String>)> {
    let group = |label: &str, names: &[&str]| {
        (label.to_string(), names.iter().map(|n| n.to_lowercase()).collect())
    };
    vec![
        group("Angelic Key", &["Angelic Key"]),
        group("Satanic Dice", &["Satanic Dice"]),
        group("S runes", &["Qi", "Xo", "Sur", "Ber", "Jah", "Drax", "Zed"]),
        // Sus, Kek and Jord came with the 2026-08-21 patch. "Satanic Key" used
        // to sit above these and has been dropped: no item in the game carries
        // that name, so the counter could never move.
        group("SS runes", &["Fawn", "Flo", "Nju", "Jol", "Sus", "Kek", "Jord"]),
    ]
}

#[derive(Clone, Serialize)]
pub struct NotableCount {
    pub label: String,
    pub total: i64,
}
const JOURNAL_CAP: usize = 400;
const SERIES_CAP: usize = 4000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Default, Clone, Serialize)]
pub struct ItemCount {
    pub total: i64,
    pub mf: i64,
}

#[derive(Clone, Serialize)]
pub struct SatanicZone {
    pub zone: String,
    pub buffs: Vec<u8>,
    pub debuffs: Vec<u8>,
}

/// The same ids in any order. Short lists — three or four — so sorting two
/// copies costs less than the allocation a set would want.
fn same_set(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

#[derive(Clone, Serialize)]
pub struct CharacterInfo {
    pub name: String,
    pub level: i64,
    pub herolevel: i64,
    pub difficulty: i64,
    /// Which grade of Hell, 1..5, and 0 when the character is not on Hell.
    pub hell_sub: i64,
    pub hardcore: bool,
    pub season: i64,
}

#[derive(Clone, Serialize)]
pub struct DropEntry {
    pub ts_ms: u64,
    pub rarity: String,
    pub mf: bool,
    pub tier: i64,
    pub item_type: i64,
    pub item_id: i64,
    pub weapon_type: i64,
    pub seed: i64,
    pub name: String,
    pub announced: bool,
    pub ground: bool,
    pub zone: Option<String>,
    /// the room it fell in, e.g. "Act_07_02" — where a drop happened is half of
    /// what makes it worth reporting
    pub room: Option<String>,
    /// which alert to play, decided here so the announcement, the drop and the
    /// pickup of one item cannot chime three times
    pub sound: Option<String>,
    /// passed the alert rules — the ticker and the journal are for these
    pub announce: bool,
    /// passed the flourish's own rules, which are a different question
    pub flourish: bool,
}

/// How many of a run's finds are kept with it. A long farm can drop hundreds;
/// the list is there to remember the run, not to replace the journal.
const RUN_DROPS: usize = 40;

/// A finished session, as it goes into the history.
#[derive(Clone, Serialize, Deserialize)]
pub struct Run {
    pub started_ms: u64,
    pub ended_ms: u64,
    pub secs: u64,
    pub character: Option<String>,
    pub level: i64,
    pub difficulty: i64,
    /// Which grade of Hell it was played on, 1..5, and 0 when it was not Hell.
    /// Absent from runs filed before this was read at all.
    #[serde(default)]
    pub hell_sub: i64,
    /// The character's hero level and magic find as the run ended — not an
    /// average. Two runs whose drop counts differ threefold look identical
    /// without them, so any comparison built on this history needs them.
    #[serde(default)]
    pub herolevel: i64,
    #[serde(default)]
    pub mf: i64,
    #[serde(default)]
    pub hardcore: bool,
    #[serde(default)]
    pub season: i64,
    pub gold: i64,
    pub xp: i64,
    pub kills: i64,
    /// rarity -> how many dropped
    pub items: HashMap<String, i64>,
    pub notable: Vec<RunDrop>,
    /// room -> seconds spent there, longest first
    pub zones: Vec<(String, u64)>,
    /// bosses put down and chests opened; absent from runs filed before 0.9.8
    #[serde(default)]
    pub tallies: Vec<TallyCount>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RunDrop {
    pub name: String,
    pub rarity: String,
    pub tier: i64,
    pub ts_ms: u64,
}

#[derive(Clone, Serialize)]
pub struct SeriesPoint {
    pub t: u64,
    pub gold: i64,
    pub xp: i64,
}

pub struct GameStats {
    pub(crate) start: Instant,
    /// wall clock for the same moment, so a finished run can say when it was
    started_ms: u64,
    /// how long the character has stood in each room this run, and since when
    /// the current one has been counting
    zone_time: HashMap<String, u64>,
    room_since: Option<Instant>,
    /// A paused session keeps its counters and stops its clock. `paused_at` is
    /// when it stopped — back-dated when the pause was the app noticing that
    /// nothing had happened for a while, so the idle minutes do not count as
    /// farming. `by_hand` marks a pause the player asked for, which no amount of
    /// activity may lift.
    paused_at: Option<Instant>,
    paused_total: Duration,
    by_hand: bool,
    /// the last time the run actually moved: gold, experience, a kill, a drop
    last_progress: Instant,
    has_mail: bool,
    /// whether anything has said yet; see `mail_state`
    mail_known: bool,
    total_gold: i64,
    gold_earned: i64,
    total_xp: i64,
    xp_earned: i64,
    total_kills: i64,
    kills_earned: i64,
    items: HashMap<&'static str, ItemCount>,
    /// how many items of each grade the session has produced (1 = D .. 6 = SS)
    graded: HashMap<i64, i64>,
    /// the last figure the save reported for each `statistic…` counter, and how
    /// far it has moved this session. A counter is only in the baseline once a
    /// save has named it, so the very first boss of a fresh install still counts
    tally_base: HashMap<&'static str, i64>,
    tally_earned: HashMap<&'static str, i64>,
    resources: HashMap<&'static str, i64>,
    satanic: Option<SatanicZone>,
    /// When the server last named it, in unix milliseconds.
    ///
    /// The zone rotates on the half hour, and the game only asks the server as
    /// part of saving — so between saves this app is repeating an answer it was
    /// given, and after the game closes it goes on repeating it indefinitely.
    /// Without the moment attached the panel states a zone from three hours ago
    /// as this hour's, next to a countdown that has run out twice since.
    satanic_at: Option<u64>,
    /// the character's magic find as the client last reported it, and whether
    /// the room it is standing in is the satanic one — both straight from the
    /// heartbeat rather than worked out from zone codes
    mf: i64,
    satanic_here: bool,
    room: Option<String>,
    /// which act the save last said the character is in, or 0
    act: i64,
    /// When the game last MOVED the satanic zone, waiting to be told. Not set
    /// by the first sighting of a zone: a tracker started mid-rotation learns
    /// where the zone is from the next reply the server sends, and that is not
    /// the zone rotating.
    ///
    /// The client asks and the server answers — the zone is on neither the
    /// heartbeat nor anything the client volunteers. See `GameEvent::ZoneRegion`.
    sz_changed: Option<Instant>,
    /// The zone above was carried over a reset and nothing has confirmed it
    /// since — so a packet that disagrees with it is this session catching up,
    /// not a rotation. A reset is also what the game starting does, and the
    /// zone does not hold still while the game is closed: without this, coming
    /// back after an hour away announced a rotation nobody was there for.
    stale_zone: bool,
    /// Whose totals the experience, kill and tally marks are measured from.
    ///
    /// The game keeps those statistics per character, and one set of marks
    /// cannot serve two. Without this, a visit to an alt and back diffed the
    /// alt's whole lifetime against the main's and booked the difference as
    /// this session's earnings — in one capture that was 98% of the kills and
    /// 65 million experience, and the ruined run was then filed to runs.json,
    /// where it stays.
    baseline_for: Option<String>,
    /// Gold credited by a rise in the bank balance that no deposit packet has
    /// accounted for yet, and when it was credited.
    ///
    /// The client says it banked some coins and the server answers with the new
    /// balance; they are the same coins, and `banked` cancels the second against
    /// the first. That only ever worked in one direction — and the two do not
    /// always arrive in that order. Forty-one of a hundred and eighty-two
    /// deposits in one capture came after the balance they caused, and every one
    /// of those was counted twice.
    pending_step: i64,
    pending_since: Option<Instant>,
    /// The highest the bank has been this session, for the purse it is being
    /// read on.
    ///
    /// A balance that falls and climbs back is not earnings, and there are two
    /// ways it happens. The player withdraws and puts it back — nothing was
    /// earned. Or a second character's purse is read in between: it arrives in
    /// fields with the same names, so a visit to an alt with a hundred coins
    /// and a return to a main with seventy-eight thousand looked like
    /// seventy-five thousand earned, which is what it did in one capture.
    ///
    /// Only what the bank has never held before is credited. The deposits
    /// themselves are counted where they are reported; this is the backstop for
    /// when that packet is missed, and a backstop should not invent.
    gold_high: i64,
    /// Which region the zone we are holding was answered for, and which region
    /// asked the question still in flight.
    ///
    /// The server answers the satanic zone per region, and one account moves
    /// between them: in the capture on disk a single account asked under ten
    /// different identifiers, and seven of the twenty-one times the zone code
    /// changed, it changed because a different region was asking — not because
    /// anything rotated. Announcing those is announcing another region's zone
    /// as news, and under a filter that alerts on buffs it is another region's
    /// buffs the player is being called away from a fight for.
    zone_region: Option<String>,
    zone_asked_by: Option<String>,
    season_mode: Option<&'static str>,
    gold_mode: Option<&'static str>,
    last_currency: Option<crate::parser::Currency>,
    xp_authoritative: bool,
    /// totals restored from the last run: the next packet of that kind
    /// re-anchors on them instead of counting the difference as earned
    stale_bank: bool,
    /// gold counted from a deposit and not yet seen in a balance
    banked: i64,
    stale_save: bool,
    last_save: Option<Instant>,
    last_bank: Option<Instant>,
    prefs: Prefs,
    notable: HashMap<String, i64>,
    seen_fingerprints: std::collections::HashSet<String>,
    /// tier by item hash, so the pickup of an item knows what the drop said
    tier_seen: HashMap<String, i64>,
    /// items already added to the counters, by identity
    counted: std::collections::HashSet<String>,
    /// items already announced, by identity — the roll on the ground and the
    /// pickup that follows are two sightings of one item
    told: std::collections::HashSet<String>,
    /// Fingerprints the player has just let go of; see `GameEvent::ItemsLetGo`.
    ///
    /// An entry is spent the moment it is used: an item can only come back once
    /// per time it was put down, and a later genuine find must not be swallowed
    /// by a memory of something that happened an hour ago.
    let_go: std::collections::HashSet<String>,
    /// The account this client is logged in as, once it has said so.
    ///
    /// `None` until then, and nothing is refused while it is `None`: not
    /// knowing who we are is a reason to count a find, not to drop one.
    account: Option<String>,
    announced_at: HashMap<String, Instant>,
    character: Option<CharacterInfo>,
    drops: VecDeque<DropEntry>,
    series: Vec<SeriesPoint>,
    /// bumped by every change, so the pusher can skip unchanged snapshots
    revision: u64,
    /// bumped only by what `extra()` actually shows. The graph series and the
    /// 400-entry drop journal are the heaviest payload the app sends, and the
    /// counters move on every heartbeat the client emits — gating them on
    /// `revision` shipped 130 KB a second for a journal nobody had added to.
    extra_rev: u64,
}

/// One list of the active custom filter, ready to be matched against a drop.
///
/// Two ways of saying which items it holds, and a drop is on the list if
/// EITHER answers. Names came first and still carry the ordinary case; rules
/// arrived with the category picker, where writing out "every Satanic helmet"
/// as 36 names would have frozen today's table into the settings file.
pub struct Listed {
    /// the sound key, `list-<id>`
    pub key: String,
    /// item names, lowercased; see `listed_sound` for the qualified spelling
    pub names: Vec<String>,
    pub rules: Vec<Rule>,
}

/// A whole category on a list: every item of a rarity, of a type, or of both.
///
/// It is matched against the drop itself rather than expanded into names, and
/// that is the point of it. The item tables ship inside the binary, so a list
/// of names written out today would go on meaning today's table after an
/// update added items to the category — the player ticked "every Satanic
/// helmet" and would get every Satanic helmet the app knew in August. Asking
/// the question of the drop keeps the answer current, and it costs nothing:
/// the rarity, the type and the weapon type are all in hand at the one place
/// the decision is made.
///
/// `None` is "any", on every field. A rule with all three None matches every
/// named drop there is, which is not a category — `apply_stats_settings`
/// refuses it rather than letting one silently swallow the whole game.
#[derive(Clone, Default)]
pub struct Rule {
    /// lowercased, against the rarity the TABLES give the item — not the one
    /// the packet claims. See `listed_sound` for why those are two vocabularies.
    pub rarity: Option<String>,
    pub item_type: Option<i64>,
    /// the weapon type inside item type 3; ignored without an item type
    pub weapon: Option<i64>,
}

impl Rule {
    fn matches(&self, rarity: Option<&str>, item_type: i64, weapon_type: i64) -> bool {
        if let Some(want) = self.rarity.as_deref() {
            if rarity != Some(want) {
                return false;
            }
        }
        if let Some(want) = self.item_type {
            if want != item_type {
                return false;
            }
            if self.weapon.is_some_and(|w| w != weapon_type) {
                return false;
            }
        }
        true
    }
}

/// Everything the player chose, as against everything the session earned.
///
/// It is one struct so that a reset cannot mislay a piece of it. These were
/// seven loose fields carried across `reset` by hand in a tuple, and the
/// flourish's own filter was never added to that tuple: every finished run —
/// including the one the game ends by closing — quietly disarmed the drop
/// announcement until the settings were saved again.
pub struct Prefs {
    /// Whether notifications fire when an item hits the ground (true) or when
    /// it is picked up (false).
    pub prefer_ground: bool,
    pub alerts: Vec<String>,
    pub min_tier: i64,
    /// What the flourish window answers to. It is asked here rather than after
    /// the fact because a drop that fails the alert rules never leaves `apply`
    /// — which made the flourish's own settings look like they did nothing at
    /// all.
    pub fx_rarities: Vec<String>,
    pub fx_tier: i64,
    /// announce anything the custom filter's lists match, whatever its rarity
    pub fx_listed: bool,
    pub notable_defs: Vec<(String, Vec<String>)>,
    /// The custom lists, in the order the player put them in — an item matched
    /// by one is announced by it, and the FIRST that matches wins. The order is
    /// the whole reason this is a Vec and not a map.
    pub sound_lists: Vec<Listed>,
    /// Satanic zone buffs worth an alert. Empty means every rotation — see
    /// `take_zone_change`.
    pub zone_buffs: Vec<u8>,
    /// Which relics are hunted, by id-in-type. Empty means NONE, the opposite
    /// of `zone_buffs` above: that list narrows an alert the game already
    /// makes, this one is the whole alert. See `hunted_relic`.
    pub relics: Vec<u16>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            prefer_ground: true,
            alerts: JOURNAL_RARITIES.iter().map(|r| r.to_string()).collect(),
            min_tier: 0,
            fx_rarities: Vec::new(),
            fx_tier: 6,
            fx_listed: false,
            notable_defs: default_notable(),
            sound_lists: Vec::new(),
            // every rotation, until the player narrows it
            zone_buffs: Vec::new(),
            // and no relic at all, until the player picks one
            relics: Vec::new(),
        }
    }
}

impl Default for GameStats {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            started_ms: now_ms(),
            zone_time: HashMap::new(),
            room_since: None,
            paused_at: None,
            paused_total: Duration::ZERO,
            by_hand: false,
            last_progress: Instant::now(),
            has_mail: false,
            mail_known: false,
            total_gold: 0,
            gold_earned: 0,
            total_xp: 0,
            xp_earned: 0,
            total_kills: 0,
            kills_earned: 0,
            items: RARITIES.iter().map(|(_, name)| (*name, ItemCount::default())).collect(),
            graded: HashMap::new(),
            tally_base: HashMap::new(),
            tally_earned: HashMap::new(),
            resources: RESOURCES.iter().map(|(_, name)| (*name, 0)).collect(),
            satanic: None,
            satanic_at: None,
            mf: 0,
            satanic_here: false,
            room: None,
            act: 0,
            sz_changed: None,
            stale_zone: false,
            baseline_for: None,
            pending_step: 0,
            pending_since: None,
            gold_high: 0,
            zone_region: None,
            zone_asked_by: None,
            season_mode: None,
            gold_mode: None,
            last_currency: None,
            xp_authoritative: false,
            stale_bank: false,
            banked: 0,
            stale_save: false,
            last_save: None,
            last_bank: None,
            prefs: Prefs::default(),
            notable: HashMap::new(),
            seen_fingerprints: std::collections::HashSet::new(),
            tier_seen: HashMap::new(),
            counted: std::collections::HashSet::new(),
            told: std::collections::HashSet::new(),
            let_go: std::collections::HashSet::new(),
            account: None,
            announced_at: HashMap::new(),
            character: None,
            drops: VecDeque::new(),
            series: Vec::new(),
            revision: 0,
            extra_rev: 0,
        }
    }
}

/// Which act a room name belongs to, where the name says so.
///
/// `Act_08_02` is act 8 and `Town_04_rm` is act 4 — the game numbers its towns
/// by the act they serve. Everything else, the Shadow Realm and the wormholes
/// and the arenas, belongs to no act and answers `None`.
fn act_of_room(room: &str) -> Option<i64> {
    let rest = room
        .strip_prefix("Act_")
        .or_else(|| room.strip_prefix("act_"))
        .or_else(|| room.strip_prefix("Town_"))
        .or_else(|| room.strip_prefix("town_"))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|n| *n > 0)
}


impl GameStats {
    /// Character, zone and the diff baselines survive a session reset — only
    /// the earned counters restart, so the next packet still yields a diff.
    pub fn reset(&mut self) {
        let revision = self.revision;
        let extra_rev = self.extra_rev;
        // These travel with `satanic` below rather than in the carry tuple,
        // which is long enough: a zone without the region it was answered for
        // reads as though a different region had asked, and the next reply is
        // swallowed for a reason that is not true.
        let zone_region = self.zone_region.take();
        let zone_asked_by = self.zone_asked_by.take();
        // The zone is carried, so the moment it was answered is carried with
        // it: without that a reset makes an hours-old zone look freshly
        // confirmed, which is the one thing this field exists to prevent.
        let satanic_at = self.satanic_at.take();
        // Whether the box has been asked about survives a Reset, for the same
        // reason its contents do: the button restarts the counters, not the
        // account, and forgetting would ring the chime a second time.
        let mail_known = self.mail_known;
        // Who is playing does not change because the counters were restarted,
        // and the window between a reset and the client next naming itself is
        // long enough to pick something up in.
        let account = self.account.take();
        let carry = (
            self.character.take(),
            self.satanic.take(),
            self.mf,
            self.satanic_here,
            self.sz_changed.take(),
            self.season_mode.take(),
            self.gold_mode.take(),
            self.last_currency.take(),
            self.total_gold,
            self.total_xp,
            self.total_kills,
            self.xp_authoritative,
            self.stale_bank,
            // The deposit the client has reported and the balance has not
            // confirmed yet. It belongs to the same in-flight balance as
            // `total_gold` above, and dropping it here left the balance that
            // lands after a reset with nothing to cancel against: coins banked
            // before the reset were counted a second time, as the new
            // session's earnings. Replaying a real capture, 88% of the points
            // a reset could fall on hold one.
            self.banked,
            // `banked` cancels a deposit the balance has not confirmed yet; this
            // is the same debt the other way round — a balance step credited
            // whose own deposit packet has not arrived. Only one of the pair
            // travelled, so a Reset landing between the two halves left the
            // survivor with nothing to cancel against and the coins counted
            // twice. The note above says exactly that about `banked`; its mirror
            // was missed. `pending_since` goes with it, or the debt never
            // expires.
            self.pending_step,
            self.pending_since,
            // The peak a climb is measured from. `total_gold` travels and this
            // did not, so after a Reset a bank drawn down and put back read as
            // income: withdraw half a million, reset, put it back, and the new
            // run opens claiming it earned it.
            self.gold_high,
            // the balance travels, so the moment it was read travels with it:
            // without it the carried gold reports an age of zero and looks fresh
            self.last_bank,
            self.stale_save,
            // every setting in one piece; see `Prefs` for why
            std::mem::take(&mut self.prefs),
            // the marks the boss and chest counters are measured from: a reset
            // starts the tally again, it does not make the game recount
            std::mem::take(&mut self.tally_base),
            // Where the character is standing, whether there is mail waiting,
            // and whose marks these are. None of the three is a thing the run
            // earned, and dropping them made the panel go blank on a reset and
            // fill in again only when the game next mentioned them.
            self.room.take(),
            self.has_mail,
            self.baseline_for.take(),
        );
        *self = Self::default();
        (
            self.character,
            self.satanic,
            self.mf,
            self.satanic_here,
            self.sz_changed,
            self.season_mode,
            self.gold_mode,
            self.last_currency,
            self.total_gold,
            self.total_xp,
            self.total_kills,
            self.xp_authoritative,
            self.stale_bank,
            self.banked,
            self.pending_step,
            self.pending_since,
            self.gold_high,
            // the balance travels, so the moment it was read travels with it:
            // without it the carried gold reports an age of zero and looks fresh
            self.last_bank,
            self.stale_save,
            self.prefs,
            self.tally_base,
            self.room,
            self.has_mail,
            self.baseline_for,
        ) = carry;
        // The room travels; without this its clock does not, and the room the
        // next session starts in banks no time at all. Reset while standing in
        // one place and farm there for half an hour, and the run card's "where
        // it happened" is empty for a run that happened entirely in one room.
        self.room_since = self.room.is_some().then(Instant::now);
        self.satanic_at = satanic_at;
        self.mail_known = mail_known;
        self.account = account;
        self.zone_region = zone_region;
        self.zone_asked_by = zone_asked_by;
        self.revision = revision + 1;
        // the journal, the series and the character all just went
        self.extra_rev = extra_rev + 1;
    }

    /// A reset taken across a stretch we were not watching: the game starting,
    /// which is also the app starting beside a game already running.
    ///
    /// The zone travels with the character, but nothing says it is still the
    /// zone — the game has been shut for an hour and the rotation does not wait
    /// for us. So the next packet is this session catching up, not news, and it
    /// is swallowed once.
    ///
    /// A plain `reset` must not do this. The Reset button pressed mid-farm
    /// leaves the game running and the zone exactly as it was; arming the guard
    /// there swallowed the next zone packet instead — and since a zone packet
    /// arrives only every few minutes, the one swallowed was often the rotation
    /// the player was waiting for.
    pub fn reset_after_blackout(&mut self) {
        self.reset();
        self.stale_zone = self.satanic.is_some();
    }

    /// Totals from the previous run, so a restart shows the last known bank
    /// and experience instead of zeros until the game saves again.
    pub fn restore(&mut self, carried: &Carried) {
        if carried.gold > 0 {
            self.total_gold = carried.gold;
            self.gold_mode = carried.mode.as_deref().and_then(currency_mode);
        }
        self.total_xp = carried.xp.max(0);
        self.xp_authoritative = carried.xp > 0;
        self.total_kills = carried.kills.max(0);
        self.stale_bank = carried.gold > 0;
        self.stale_save = carried.xp > 0 || carried.kills > 0;
    }

    pub fn carried(&self) -> Carried {
        Carried {
            gold: self.total_gold,
            mode: self.gold_mode.map(|m| m.to_string()),
            xp: self.total_xp,
            kills: self.total_kills,
        }
    }

    /// What is known about mail, which is not the same as whether there is any.
    ///
    /// Cheap enough to poll: the chime must fire even while every window that
    /// shows the counters is hidden.
    ///
    /// `None` until the server has answered once. The plain flag reads false
    /// before that, which is indistinguishable from an empty box — and the
    /// chime, watching for false becoming true, rang on every launch for a
    /// letter that had been sitting there for days.
    pub fn mail_state(&self) -> Option<bool> {
        self.mail_known.then_some(self.has_mail)
    }

    /// The satanic zone has moved and this rotation is worth telling the player
    /// about. Taken rather than read, so one rotation is announced once however
    /// often the pusher looks.
    ///
    /// The zone comes back with it: the snapshot only travels to windows that
    /// are on screen, so a player who has hidden the overlay — the case the
    /// chime exists for — would otherwise have the alert read from a stale one.
    ///
    /// An empty pick is not "nothing", it is every rotation. The list narrows
    /// the alert, and a player who has narrowed it to nothing has narrowed
    /// nothing. Reading it the other way makes the picker's own "clear" button
    /// a mute switch that nothing on the page admits to.
    pub fn take_zone_change(&mut self) -> Option<SatanicZone> {
        let zone = self.sz_changed.take().and_then(|_| self.satanic.clone())?;
        let wanted = &self.prefs.zone_buffs;
        (wanted.is_empty() || zone.buffs.iter().any(|b| wanted.contains(b))).then_some(zone)
    }

    /// Add the time spent in the current room to its total and start counting
    /// again from now.
    fn bank_room_time(&mut self) {
        self.bank_room_time_at(Instant::now());
    }

    /// The same, but counting only up to a given moment and leaving the clock
    /// stopped: pausing must not credit the room with the idle minutes that
    /// caused the pause.
    fn bank_room_time_at(&mut self, at: Instant) {
        let (Some(room), Some(since)) = (self.room.clone(), self.room_since) else {
            self.room_since = Some(Instant::now());
            return;
        };
        let secs = at.saturating_duration_since(since).as_secs();
        if secs > 0 {
            *self.zone_time.entry(room).or_insert(0) += secs;
        }
        self.room_since = Some(Instant::now());
    }

    /// What this run amounted to, or nothing when there is nothing to say. A
    /// glance at the app, a restart, a game that closed a minute after opening —
    /// none of those are runs, and a history full of them is noise.
    pub fn finish(&mut self) -> Option<Run> {
        self.bank_room_time();
        let secs = self.active().as_secs();
        let nothing_happened = self.gold_earned == 0 && self.xp_earned == 0 && self.kills_earned == 0;
        if secs < 60 || nothing_happened {
            return None;
        }
        let mut zones: Vec<(String, u64)> = self.zone_time.iter().map(|(k, v)| (k.clone(), *v)).collect();
        zones.sort_by_key(|(_, secs)| std::cmp::Reverse(*secs));
        zones.truncate(6);
        // the finds, newest first, and only the ones that were worth announcing
        let notable: Vec<RunDrop> = self
            .drops
            .iter()
            .rev()
            .filter(|d| !d.name.is_empty())
            .take(RUN_DROPS)
            .map(|d| RunDrop {
                name: d.name.clone(),
                rarity: d.rarity.clone(),
                tier: d.tier,
                ts_ms: d.ts_ms,
            })
            .collect();
        Some(Run {
            started_ms: self.started_ms,
            ended_ms: now_ms(),
            secs,
            character: self.character.as_ref().map(|c| c.name.clone()),
            level: self.character.as_ref().map_or(0, |c| c.level),
            difficulty: self.character.as_ref().map_or(0, |c| c.difficulty),
            hell_sub: self.character.as_ref().map_or(0, |c| c.hell_sub),
            herolevel: self.character.as_ref().map_or(0, |c| c.herolevel),
            // the last heartbeat's figure, which is what "at the end" means
            mf: self.mf,
            hardcore: self.character.as_ref().is_some_and(|c| c.hardcore),
            season: self.character.as_ref().map_or(0, |c| c.season),
            gold: self.gold_earned,
            xp: self.xp_earned,
            kills: self.kills_earned,
            items: self.items.iter().map(|(name, c)| (name.to_string(), c.total)).collect(),
            notable,
            zones,
            tallies: self.tallies(),
        })
    }

    /// How long the session has actually been running: the clock less whatever
    /// it has spent paused. Every rate divides by this, so a run left standing
    /// while the player made tea reports what the farming was worth, not what
    /// the wall clock says.
    fn active(&self) -> Duration {
        let mut ran = self.start.elapsed().saturating_sub(self.paused_total);
        if let Some(at) = self.paused_at {
            ran = ran.saturating_sub(at.elapsed());
        }
        ran
    }

    pub fn paused(&self) -> bool {
        self.paused_at.is_some()
    }

    /// Stop the clock as of `since`, which is now for a pause the player asked
    /// for and the last sign of life for one the app decided on.
    fn hold(&mut self, since: Instant, by_hand: bool) {
        if self.paused_at.is_none() {
            self.bank_room_time_at(since);
            self.room_since = None;
            self.paused_at = Some(since);
            // The idle watch notices a pause five minutes after it began and
            // back-dates it, so the session clock steps backwards — and the
            // graph's points are stamped with that clock. Points recorded
            // during those five minutes now sit beyond the end of the series,
            // and the line drawn from them doubles back on itself and runs off
            // the right edge of the canvas. They were never real time anyway:
            // the run was not running while they were taken.
            let cut = self.active().as_secs();
            if self.series.iter().any(|p| p.t > cut) {
                self.series.retain(|p| p.t <= cut);
                self.extra_rev += 1;
            }
            self.revision += 1;
        }
        self.by_hand |= by_hand;
    }

    fn release(&mut self) {
        if let Some(at) = self.paused_at.take() {
            self.paused_total += at.elapsed();
            self.room_since = Some(Instant::now());
            self.revision += 1;
        }
        self.by_hand = false;
        self.last_progress = Instant::now();
    }

    /// The pause button and the hotkey. A hand-made pause outranks the idle
    /// watch: it lasts until the same hand lifts it.
    pub fn set_paused(&mut self, on: bool) {
        if on {
            self.hold(Instant::now(), true);
        } else {
            self.release();
        }
    }

    /// Quiet for long enough to stop the clock. Asked once a tick by the
    /// pusher, which is the only thing here with a heartbeat.
    ///
    /// All of this existed already — `hold` takes a `by_hand` of false, and
    /// `progressed` lifts exactly the pause that sets it — and nothing ever
    /// called it. The README, the field doc on `last_progress` and two comments
    /// beside it all described a feature that was never wired up, and the
    /// per-hour figures quietly counted every break.
    pub fn watch_idle(&mut self) {
        if self.paused() || self.last_progress.elapsed() < IDLE_AFTER {
            return;
        }
        // Stopped as of the last sign of life rather than as of now: the five
        // minutes it took to notice were not part of the run either.
        self.hold(self.last_progress, false);
    }

    /// The run moved. Anything that lifts an idle pause goes through here.
    fn progressed(&mut self) {
        self.last_progress = Instant::now();
        if self.paused() && !self.by_hand {
            self.release();
        }
    }

    /// How many items of one grade this session has produced. Grades run 1..6,
    /// which the interface writes as D through SS.
    pub fn graded(&self, tier: i64) -> i64 {
        self.graded.get(&tier).copied().unwrap_or(0)
    }

    /// When this session began, as wall clock. Discord counts the elapsed time
    /// itself and wants the moment, not the duration.
    pub fn started_ms(&self) -> u64 {
        self.started_ms
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn extra_revision(&self) -> u64 {
        self.extra_rev
    }

    /// Which list this drop is on, if any.
    ///
    /// A list holds the name the game prints, and for most items that is the
    /// whole story. Eleven names belong to two items each, though, and the
    /// seven Essence Vaults share one name between every rarity there is — so
    /// a list naming a vault would fire for all seven and say nothing about
    /// which had dropped. Where the identity answers for an item its name
    /// cannot, the list may say so as `Essence Vault (Angelic)`, and both
    /// spellings are matched: the bare name still means any of them.
    ///
    /// A list may also hold a whole category as a `Rule`, and a drop is on the
    /// list if either the names or a rule answers. The rarity a rule is asked
    /// about is the one the TABLES give this item, identity first: the packet's
    /// ten-value scale is a different vocabulary — it has no Runeword and its
    /// `Common` is not the tables' — and a rule the player built from a list of
    /// rarities on the screen must be answered in the vocabulary that screen
    /// used. Identity before name because the name lies for eleven items:
    /// `Shrunken Head` is a Satanic charm by name and a Common relic by
    /// identity, and only one of them should answer to "every Satanic charm".
    fn listed_sound(
        &self,
        name: &str,
        known: Option<&crate::items::Known>,
        item_type: i64,
        weapon_type: i64,
    ) -> Option<String> {
        if name.is_empty() {
            return None;
        }
        let lower = name.to_lowercase();
        let qualified = known.map(|k| format!("{lower} ({})", k.rarity.to_lowercase()));
        // Only looked up when some list actually holds a category. Every named
        // drop passes through here and most filters have no rules at all;
        // `to_lowercase` allocates, and this is the hot path.
        let rarity = self
            .prefs
            .sound_lists
            .iter()
            .any(|l| !l.rules.is_empty())
            .then(|| {
                known
                    .map(|k| k.rarity)
                    .or_else(|| crate::items::rarity_by_name(name))
                    .map(str::to_lowercase)
            })
            .flatten();
        self.prefs
            .sound_lists
            .iter()
            .find(|l| {
                l.names.contains(&lower)
                    || qualified.as_ref().is_some_and(|q| l.names.contains(q))
                    || l.rules.iter().any(|r| {
                        r.matches(rarity.as_deref(), item_type, weapon_type)
                    })
            })
            .map(|l| l.key.clone())
    }

    /// Whether this drop is a relic the player is hunting.
    ///
    /// By identity, and it has to be by identity: all 156 relics are Common, so
    /// the rarity a list can qualify a name with says nothing here, and three
    /// relic names belong to another item as well — `Shrunken Head` to a
    /// Satanic charm, `Death's Scythe` to a Set polearm, `Satan's Horn` to a
    /// Common collectible. The last of those shares the rarity too, so no
    /// spelling of the name, qualified or bare, could ever tell the two apart.
    /// `resolve_rarity` records what picking by name cost the first of them.
    ///
    /// It answers into the same slot `listed_sound` does rather than beside it,
    /// so a hunted relic travels the one path everything else travels — it is
    /// wanted, it announces, it reaches the journal, and it takes the flourish
    /// where a listed item would. Two questions, one decision point.
    fn hunted_relic(&self, item_type: i64, item_id: i64) -> Option<String> {
        if item_type != RELIC || self.prefs.relics.is_empty() {
            return None;
        }
        let id = u16::try_from(item_id).ok()?;
        self.prefs.relics.contains(&id).then(|| "relic".to_string())
    }

    /// Every setting at once.
    ///
    /// This was five setters called in a row from `apply_stats_settings`, which
    /// is the very hazard `Prefs` was introduced to end: they had already
    /// drifted — four bumped `revision` and the flourish one did not — and a
    /// setting added to `Settings` and forgotten here was a silent no-op. One
    /// struct literal means the compiler notices instead.
    pub fn set_prefs(&mut self, prefs: Prefs) {
        self.revision += 1;
        self.prefs = prefs;
    }

    /// The old verbs, kept for the tests alone.
    ///
    /// Production goes through `set_prefs` so that a setting added to
    /// `Settings` and forgotten in `apply_stats_settings` is a compile error.
    /// The tests want to change one thing and say so, which is a different
    /// job, and behind `cfg(test)` they cannot drift back into the app.
    #[cfg(test)]
    pub fn set_prefer_ground(&mut self, prefer_ground: bool) {
        self.revision += 1;
        self.prefs.prefer_ground = prefer_ground;
    }

    #[cfg(test)]
    pub fn set_filter(&mut self, alerts: Vec<String>, min_tier: i64) {
        self.revision += 1;
        self.prefs.alerts = alerts;
        self.prefs.min_tier = min_tier;
    }

    #[cfg(test)]
    pub fn set_flourish_filter(&mut self, rarities: Vec<String>, tier: i64) {
        self.revision += 1;
        self.prefs.fx_rarities = rarities;
        self.prefs.fx_tier = tier;
    }

    #[cfg(test)]
    pub fn set_sound_lists(&mut self, lists: Vec<(String, Vec<String>)>) {
        let lists = lists
            .into_iter()
            .map(|(key, names)| Listed {
                key,
                names: names.into_iter().map(|n| n.trim().to_lowercase()).collect(),
                rules: Vec::new(),
            })
            .collect();
        self.set_listed(lists);
    }

    /// The same, for the tests that care about rules or about which list wins.
    #[cfg(test)]
    pub fn set_listed(&mut self, lists: Vec<Listed>) {
        self.revision += 1;
        self.prefs.sound_lists = lists;
    }

    fn count_notable(&mut self, name: &str, amount: i64) {
        if name.is_empty() {
            return;
        }
        // the game calls a rune "Ber"; everyone else says "Ber Rune"
        let lower = name.to_lowercase();
        let bare = lower.trim_end_matches(" rune").to_string();
        let label = self
            .prefs
            .notable_defs
            .iter()
            .find(|(_, names)| {
                names.iter().any(|n| *n == lower || n.trim_end_matches(" rune") == bare)
            })
            .map(|(label, _)| label.clone());
        if let Some(label) = label {
            *self.notable.entry(label).or_insert(0) += amount;
        }
    }

    /// A minimum tier is a promise to stay quiet about anything lesser, so an
    /// item whose grade cannot be established stays quiet too. The server's own
    /// announcements bypass this — they are rare finds by definition.
    fn passes_filter(&self, rarity: &str, tier: i64) -> bool {
        self.prefs.alerts.iter().any(|r| r == rarity) && tier >= self.prefs.min_tier
    }

    /// The flourish's slider starts at D, and there is no "any" below it — so D
    /// has to mean any, an item the tables do not grade included. Read as a
    /// plain minimum it excluded exactly those, and the setting that promised
    /// everything announced the least.
    ///
    /// `listed` says the item is on a list of the custom filter. When the
    /// announcement is set to follow that filter, being on a list is enough on
    /// its own: the point of putting an item on a list is that it matters, and
    /// having to describe it a second time in rarity and grade switches is the
    /// kind of duplication that makes a filter look broken.
    fn worth_a_flourish(&self, rarity: &str, tier: i64, listed: bool) -> bool {
        if self.prefs.fx_listed && listed {
            return true;
        }
        let graded = tier >= self.prefs.fx_tier || self.prefs.fx_tier <= 1;
        self.prefs.fx_rarities.iter().any(|r| r == rarity) && graded
    }

    /// Whether a name in chat is the character being tracked.
    ///
    /// Before the first save there is no character to compare against, and an
    /// unattributed find is treated as somebody else's: a run that misses one
    /// chime of its own in the first seconds is a smaller wrong than one that
    /// announces the whole shard's luck.
    fn is_us(&self, who: &str) -> bool {
        let Some(me) = self.character.as_ref().map(|c| c.name.trim()) else { return false };
        !me.is_empty() && me.eq_ignore_ascii_case(who.trim())
    }

    /// Returns the journal entry when this event produced a new tracked drop.
    pub fn apply(&mut self, event: &GameEvent) -> Option<DropEntry> {
        self.revision += 1;
        match event {
            // A find the server put in chat. Everyone on the shard reads that
            // line, so most of them are somebody else's — and somebody else's
            // Angelic sounding the horn in the middle of your own run is noise
            // wearing the costume of news. Ours is taken as though the client
            // had reported it; anyone else's is dropped here and counts for
            // nothing, sounds for nothing and lights nothing up.
            GameEvent::Found { finder, name } => {
                if !self.is_us(finder) {
                    return None;
                }
                return self.apply(&GameEvent::ItemAdded {
                    rarity: serde_json::Value::Null,
                    unscaled: false,
                    mf: false,
                    tier: 0,
                    item_type: 0,
                    item_id: 0,
                    weapon_type: 0,
                    seed: 0,
                    name: name.clone(),
                    announced: true,
                    amount: 1,
                    // An identity of its own, and a different one each time.
                    // Sharing one per name collapsed every later find of the
                    // same item into the first for the whole run: farm two
                    // Doctor's Potions and the second was silently nothing.
                    // Pairing this with the client's own sighting is done by
                    // name and a clock, above, not by pretending to an identity
                    // the two could never share.
                    fingerprint: format!("chat:{}:{}", name.to_lowercase(), now_ms()),
                    hash: String::new(),
                    ground: false,
                });
            }
            GameEvent::Gold(c) => self.apply_currency(c),
            // guild XP is 15% of character XP, so the reported gain scales back
            // up; account totals later correct any drift (their diff goes 0)
            GameEvent::XpGain(xp) => {
                let gained = (*xp as f64 / 0.15) as i64;
                if gained > 0 {
                    self.total_xp += gained;
                    self.xp_earned += gained;
                    self.progressed();
                }
            }
            GameEvent::Account {
                experience,
                has_experience,
                season,
                hardcore,
                blood_pact,
                act,
                name,
                level,
                herolevel,
                difficulty,
                hell_sub,
                kills,
                tallies,
            } => {
                if *has_experience {
                    self.last_save = Some(Instant::now());
                }
                // A save from a different character than the marks were taken
                // from. Its totals are not a continuation of anything: they are
                // another character's life, and the difference between the two
                // is not something this session earned.
                //
                // So the marks are thrown away and taken again from this save,
                // which is exactly what the tracker does on its first save of a
                // run — the same path, for the same reason. Nothing is credited
                // by the packet that re-anchors.
                let switched = *has_experience
                    && !name.is_empty()
                    && self.baseline_for.as_deref().is_some_and(|had| had != name);
                if switched {
                    self.stale_save = true;
                    self.xp_authoritative = false;
                    self.total_kills = 0;
                    self.tally_base.clear();
                    // The purse travels with the character too, and two of them
                    // can be on the same mode: comparing a balance against one
                    // that belongs to somebody else invented 75,541 gold in a
                    // single event. `stale_bank` re-anchors on the next balance
                    // without claiming the step.
                    self.stale_bank = true;
                    self.banked = 0;
                    self.pending_step = 0;
                    self.pending_since = None;
                    self.gold_high = 0;
                }
                if *has_experience && !name.is_empty() {
                    self.baseline_for = Some(name.clone());
                }
                if self.stale_save && *has_experience && *experience > 0 {
                    self.total_xp = *experience;
                    self.xp_authoritative = true;
                    self.total_kills = *kills;
                    self.stale_save = false;
                } else if *has_experience && *experience > 0 {
                    // only trust a diff between two authoritative totals; the
                    // first one just calibrates (guild-XP guesses precede it)
                    if self.xp_authoritative {
                        let diff = experience - self.total_xp;
                        if diff > 0 {
                            self.xp_earned += diff;
                            self.progressed();
                        }
                    }
                    self.total_xp = *experience;
                    self.xp_authoritative = true;
                }
                // The game rebases these statistics itself: after an instance
                // restart a save can report fewer kills than the one before.
                // Those monsters were still killed, so a lower total only
                // moves the baseline — the counter never stalls waiting for
                // the old peak to come back.
                if *kills > 0 && self.total_kills != *kills {
                    if self.total_kills != 0 {
                        let diff = kills - self.total_kills;
                        if diff > 0 {
                            self.kills_earned += diff;
                            self.progressed();
                        }
                    }
                    self.total_kills = *kills;
                }
                // the same rebase for the bosses and the chests: the first save
                // to name a counter only sets the mark it is measured from
                for (key, _, _) in TALLIES {
                    let Some(&now) = tallies.get(*key) else { continue };
                    match self.tally_base.entry(key) {
                        std::collections::hash_map::Entry::Occupied(mut seen) => {
                            let diff = now - seen.get();
                            if diff > 0 {
                                *self.tally_earned.entry(key).or_insert(0) += diff;
                            }
                            seen.insert(now);
                        }
                        std::collections::hash_map::Entry::Vacant(fresh) => {
                            fresh.insert(now);
                        }
                    }
                }
                // Where the character is, coarsely. The save says which act;
                // the room itself only ever comes with the heartbeat, and that
                // arrives when the game feels like it. A login-identity packet
                // carries no act at all, and a zero is not a place.
                if *act > 0 && self.act != *act {
                    self.act = *act;
                    self.revision += 1;
                }
                // A room in another act is not where the character is.
                //
                // The room only ever arrives with the game's own state packet,
                // and since the August 2026 patch that packet comes about
                // twenty times less often than it used to — a couple of
                // hundred lines of traffic apart at best, and thousands at
                // worst. So the last room heard is often long out of date, and
                // it was outranking the act, which the save states on every
                // write: the panel sat on `Flooded Plains` through visits to
                // three other acts.
                //
                // The save cannot say which zone, so this does not replace the
                // room with a better one — it retires a room that has been
                // outlived, and the act stands alone until the game names a
                // zone again. A room whose name says nothing about an act, the
                // Shadow Realm and its like, is left where it is: there is
                // nothing to contradict it with.
                if let (Some(room), true) = (self.room.as_deref(), *act > 0) {
                    if matches!(act_of_room(room), Some(was) if was != *act) {
                        self.bank_room_time();
                        self.room = None;
                        self.room_since = None;
                        self.revision += 1;
                    }
                }
                // a login-identity packet carries no experience and may report
                // a different season than the character actually plays, so it
                // only fills in what the real account packet has not set yet
                let full = *has_experience;
                if full || self.season_mode.is_none() {
                    // Any season at all means the seasonal purse. Comparing
                    // against a season number written into the source meant the
                    // bank read from the wrong bucket the day a new season
                    // started, and it read as the non-seasonal one — which is
                    // exactly what a returning player has least of.
                    self.season_mode = Some(if *season > 0 {
                        if *hardcore == 1 { "GSH" } else { "GSS" }
                    } else if *blood_pact != 0 {
                        "GBP"
                    } else if *hardcore == 1 {
                        "GNH"
                    } else {
                        "GNS"
                    });
                }
                if full || self.character.is_none() {
                    self.extra_rev += 1;
                    self.character = Some(CharacterInfo {
                        name: name.clone(),
                        level: *level,
                        herolevel: *herolevel,
                        difficulty: *difficulty,
                        hell_sub: *hell_sub,
                        hardcore: *hardcore == 1,
                        season: *season,
                    });
                }
                // Currency usually arrives before the purse is known, so the
                // last packet is read again now that the save has named one.
                //
                // Without its delta. `apply_currency` counts a deposit the
                // moment it sees one, and replaying the packet whole counted
                // it again on every save for the rest of the session: bank
                // 2600 and it read 2600, then 5200 after the next save, then
                // 7800 — and the inflated `banked` then swallowed the next
                // real deposit whole. It is the balance this replay is for.
                if let Some(c) = self.last_currency.clone() {
                    self.apply_currency(&crate::parser::Currency { delta: 0, ..c });
                }
            }
            GameEvent::WhoseAccount(id) => {
                if self.account.as_deref() != Some(id.as_str()) {
                    self.account = Some(id.clone());
                }
            }
            GameEvent::ItemsLetGo(gone) => {
                // Bounded the way the sighting set beside it is. Selling, using
                // and crafting all remove things that never come back, so this
                // would otherwise only grow.
                if self.let_go.len() > 4_000 {
                    self.let_go.clear();
                }
                self.let_go.extend(gone.iter().cloned());
            }
            GameEvent::Mail(has) => {
                self.has_mail = *has;
                self.mail_known = true;
            }
            GameEvent::Room(room) => {
                if self.room.as_deref() != Some(room.as_str()) {
                    // Close the books on the room being left: a run is worth
                    // little without knowing where it happened.
                    //
                    // Never while the clock is stopped. A hand pause is a trip
                    // to town, and a trip to town is a sequence of room
                    // changes; `bank_room_time` re-arms the clock on its way
                    // out, so the first change restarted the one the pause had
                    // just stopped and every change after it banked paused
                    // wall-clock seconds. A 600-second run reported 900 seconds
                    // in one room, the wrong room sorted to the top of the
                    // run's zones, and the run card's share bar drew past the
                    // end of its track. The room itself still moves, so the
                    // panel keeps saying where the character is.
                    if !self.paused() {
                        self.bank_room_time();
                        self.room_since = Some(Instant::now());
                    }
                    self.room = Some(room.clone());
                }
            }
            GameEvent::Vitals { mf, level, hlevel, satanic_here } => {
                // What the packet did not state is left where it was. The
                // client files crash reports carrying the same shape with no
                // magic find in them, and taking those as zero left the number
                // reading nothing for most of a session.
                let mf = mf.unwrap_or(self.mf);
                let satanic_here = satanic_here.unwrap_or(self.satanic_here);
                if mf != self.mf || satanic_here != self.satanic_here {
                    self.revision += 1;
                }
                self.mf = mf;
                self.satanic_here = satanic_here;
                // The save carries these too, but it arrives when the game
                // decides to save; the heartbeat is a few seconds old at worst.
                // Only what the heartbeat actually reported is taken.
                if let Some(c) = self.character.as_mut() {
                    if *level > 0 && c.level != *level {
                        c.level = *level;
                        self.revision += 1;
                        self.extra_rev += 1;
                    }
                    if *hlevel > 0 && c.herolevel != *hlevel {
                        c.herolevel = *hlevel;
                        self.revision += 1;
                        self.extra_rev += 1;
                    }
                }
            }
            GameEvent::ItemAdded {
                rarity,
                unscaled,
                mf,
                tier,
                item_type,
                item_id,
                weapon_type,
                seed,
                name,
                announced,
                amount,
                fingerprint,
                hash,
                ground,
            } => {
                // One item is seen twice: when the server rolls it and when it
                // lands in the bag. Its own hash ties the two together, so it
                // counts once — and the tier the roll reported is remembered
                // for the pickup, which never carries one.
                //
                // Not every packet carries that hash, and the two sightings
                // used to fall into identity spaces that could never meet when
                // it was missing: the roll minted `g:seed:type:id` while the
                // pickup used the inventory fingerprint, so `counted` admitted
                // both and one item was added to its rarity twice. The
                // fingerprint is what both sightings actually share — the
                // server's generation answer keys the item by it and the bag
                // reports the same string — so it is asked for first, and the
                // seed key is left for a roll that arrives without one.
                let identity = if !hash.is_empty() {
                    format!("h:{hash}")
                } else if !fingerprint.is_empty() {
                    fingerprint.clone()
                } else if *ground || *seed != 0 {
                    // a pickup with neither hash, fingerprint nor seed has
                    // nothing that tells one copy of an item from the next, and
                    // merging those would undercount rather than double-count
                    format!("g:{seed}:{item_type}:{item_id}")
                } else {
                    String::new()
                };
                // Picking up what you have just put down is not finding it.
                //
                // A worn item thrown on the floor and taken back is two ordinary
                // inventory operations; the second is shaped exactly like a
                // find, down to the named flag. Two of them were reported —
                // a Pendant of Eternity and a pair of Tectonic Grips — both
                // announced, chimed and journalled as though they had dropped.
                // The fingerprint survives the round trip unchanged, which is
                // what makes them tellable apart at all.
                if !fingerprint.is_empty() && self.let_go.remove(fingerprint) {
                    return None;
                }
                // Somebody else's item is not a find, wherever you picked it up.
                //
                // A fingerprint carries the account it was made for and keeps
                // it for the life of the item, so a friend's Torch of Shadows
                // dropped on the floor for you arrives named, flagged and
                // shaped exactly like a drop — it was announced and journalled
                // as one. Across four captures, 999 named things entered these
                // bags: 985 were made for this account and 14 were not, and not
                // one of the 14 had ever been seen falling. Five of them arrive
                // in a row from one account, which is what being handed a set
                // of gear looks like from the outside.
                //
                // Nothing is refused until the client has said who it is.
                if let (Some(mine), Some(theirs)) =
                    (self.account.as_deref(), crate::parser::fingerprint_account(fingerprint))
                {
                    if mine != theirs {
                        return None;
                    }
                }
                if !identity.is_empty() {
                    // a world sync repeats the very same sighting; that is noise
                    let sighting = format!("{}{identity}", if *ground { "d:" } else { "p:" });
                    if !self.seen_fingerprints.insert(sighting) {
                        return None;
                    }
                    if self.seen_fingerprints.len() > 20_000 {
                        self.seen_fingerprints.clear();
                        self.counted.clear();
                        self.told.clear();
                    }
                }
                let first = identity.is_empty() || self.counted.insert(identity.clone());
                // A named item always drops at its own grade, which the packet
                // never states — the wiki table does. Unnamed drops carry their
                // grade themselves, and their pickup inherits it.
                //
                // Proven against two captures: over every named sighting in
                // them the packet's grade field is zero, and no named identity
                // ever carries two different grades. Ordinary bases do — they
                // arrive at every grade from 1 to 6, and at 6666 — which is why
                // this stays a fallback rather than becoming an override.
                // The identity first, where the name alone cannot say: eleven
                // names belong to two items each, and a grade read by name gave
                // a Common relic the S of the weapon sharing its name.
                let id = (*item_type, *item_id, *weapon_type);
                let known = crate::parser::known_item(name, *unscaled, id);
                let mut tier = *tier;
                if tier == 0 && !name.is_empty() {
                    tier = known.as_ref().map_or_else(
                        || crate::items::tier_by_name(name),
                        |k| k.tier,
                    );
                }
                if !hash.is_empty() {
                    if tier > 0 {
                        self.tier_seen.insert(hash.clone(), tier);
                    } else if let Some(known) = self.tier_seen.get(hash) {
                        tier = *known;
                    }
                    if self.tier_seen.len() > 4000 {
                        self.tier_seen.clear();
                    }
                }
                let rarity_key = crate::parser::resolve_rarity(rarity, name, *unscaled, id);
                let is_resource =
                    RESOURCES.iter().any(|(t, _)| t == item_type) || is_container(name);
                // A sighting counts once, whichever of the two got here first:
                // `counted` is keyed on the item's identity, not on how it was
                // seen. So a roll on the floor does reach the counters — an
                // item is found when it lands, and a player who leaves a
                // Satanic base on the ground still found it. It was this
                // comment that was wrong, not the code: it claimed rolls
                // "never" reach the counters, and in one capture they were 93%
                // of the Set column and 42% of the Satanic one.
                if !announced && first {
                    let n = (*amount).max(1);
                    // Gear only. Counting by grade alone put Angelic keys and
                    // socketables in the SS column — items the player never
                    // dropped as gear and would not call an SS find.
                    if tier > 0 && GEAR.contains(item_type) {
                        *self.graded.entry(tier).or_insert(0) += n;
                    }
                    if !is_resource {
                        if let Some(count) = self.items.get_mut(rarity_key.as_str()) {
                            count.total += n;
                            if *mf {
                                count.mf += n;
                            }
                        }
                    }
                    if let Some((_, res)) = RESOURCES.iter().find(|(t, _)| t == item_type) {
                        let dull = DULL_KEYS.contains(&name.to_lowercase().as_str());
                        if !dull {
                            *self.resources.get_mut(res).unwrap() += n;
                        }
                    }
                    self.count_notable(name, n);
                    self.progressed();
                }
                // One notification per item: either when it hits the ground or
                // when it lands in the bag, never both. That is what `told`
                // below is for, and it is enough on its own — an item is
                // announced by whichever sighting gets here first and passes.
                //
                // So preferring the drop moment means preferring it, not
                // requiring it. Requiring it is what this used to do, and since
                // the ground is the default it meant a pickup could never
                // announce anything at all: in one capture, two hundred rolls
                // announced and none of the thousand pickups did, and the two
                // sets barely overlapped — a hundred and seventeen of the rolls
                // were items left on the floor, and nine hundred of the pickups
                // had no roll this app ever saw.
                //
                // It also cost the case the old comment promised: a roll on the
                // ground carries no grade, so under a minimum grade it fails,
                // and the pickup that could have proved the grade was refused a
                // hearing. Now it gets one, because the roll never reached
                // `told`.
                //
                // The other way round still means only the bag: an item nobody
                // picks up is not something the player asked to be told about.
                // a list the user built outranks every switch below it
                let listed = self
                    .listed_sound(name, known.as_ref(), *item_type, *weapon_type)
                    .or_else(|| self.hunted_relic(*item_type, *item_id));
                let listed_hit = listed.is_some();
                let wanted = *announced || listed_hit || self.prefs.prefer_ground || !*ground;
                let announce = *announced
                    || listed_hit
                    || (!is_resource && self.passes_filter(&rarity_key, tier));
                let flourish = !is_resource && self.worth_a_flourish(&rarity_key, tier, listed_hit);
                if wanted && (announce || flourish) {
                    // One item, one notification, whichever sighting got here
                    // first. The rule above says as much, but a list was
                    // exempt from it — `wanted` is true for a listed item
                    // either way — so everything on a list chimed twice, once
                    // as it hit the ground and again as it went in the bag.
                    // The exemption was meant to outrank the rarity switches,
                    // which is what `announce` already does.
                    if !identity.is_empty() && !self.told.insert(identity.clone()) {
                        return None;
                    }
                    // The server announces a notable find in chat the moment
                    // it drops — the only signal that arrives before the item
                    // is picked up and says what it is. The local drop and the
                    // pickup that follow stay silent so it chimes once.
                    let lower = name.to_lowercase();
                    let echo = self
                        .announced_at
                        .get(&lower)
                        .is_some_and(|t| t.elapsed() < Duration::from_secs(60));
                    if *announced {
                        self.announced_at.insert(lower, Instant::now());
                        self.announced_at.retain(|_, t| t.elapsed() < Duration::from_secs(120));
                    } else if echo {
                        // The server's chat line and the client's own sighting
                        // of the same drop are one find. Only the chime was
                        // being suppressed, so the item still got a second
                        // journal line, a second ticker row and a second entry
                        // in the run's finds — which is what a fingerprint on
                        // the chat event was supposed to prevent and could not,
                        // the two sightings having no identity in common.
                        return None;
                    }
                    // Only what the alert rules asked for chimes. A drop that
                    // got this far on the flourish's rules alone is a picture,
                    // not a sound: with the alerts set to SS and the flourish
                    // to D, every D item was making a noise the player had
                    // just finished switching off.
                    let sound = if echo || !announce {
                        None
                    } else {
                        listed.or_else(|| {
                            self.prefs.alerts.contains(&rarity_key).then(|| rarity_key.to_lowercase())
                        })
                    };
                    let entry = DropEntry {
                        ts_ms: now_ms(),
                        sound,
                        rarity: rarity_key,
                        ground: *ground,
                        mf: *mf,
                        tier,
                        item_type: *item_type,
                        item_id: *item_id,
                        weapon_type: *weapon_type,
                        seed: *seed,
                        name: name.clone(),
                        announced: *announced,
                        zone: self.satanic.as_ref().map(|s| s.zone.clone()),
                        room: self.room.clone(),
                        announce,
                        flourish,
                    };
                    // the journal is the alert rules' list; a drop that only
                    // earned a flourish does not belong in it
                    if announce {
                        if self.drops.len() >= JOURNAL_CAP {
                            self.drops.pop_front();
                        }
                        self.drops.push_back(entry.clone());
                        self.extra_rev += 1;
                    }
                    return Some(entry);
                }
            }
            GameEvent::ZoneRegion(id) => {
                self.zone_asked_by = Some(id.clone());
            }
            GameEvent::SatanicZone { zone, buffs, debuffs } => {
                // A roll different from the one we are holding is the
                // rotation; a zone where there was none is this process finding
                // out where the zone is, which is not news the player asked to
                // hear. Nor is the first reply after a blackout — see
                // `stale_zone`, and note that a plain reset is not one.
                //
                // The buffs decide it as much as the code does. There are forty
                // rooms and the roll can land on the one it just left, with a
                // different set on it — and under a filter that alerts on the
                // buffs, that is exactly the rotation the player asked to hear
                // about. The order the server lists them in is its own business,
                // so they are compared as sets.
                let rerolled = |s: &SatanicZone| {
                    s.zone != *zone || !same_set(&s.buffs, buffs) || !same_set(&s.debuffs, debuffs)
                };
                // Only against the zone the same region gave us. A reply for
                // another region is a different question's answer: it replaces
                // what we hold, because it is where the player now is, and it
                // announces nothing, because nothing rotated.
                //
                // Both unknown compares equal, which is what a capture with no
                // request in it gets — the old behaviour, rather than silence.
                let same_region = self.zone_region == self.zone_asked_by;
                let moved = same_region && self.satanic.as_ref().is_some_and(rerolled);
                if moved && !self.stale_zone {
                    self.sz_changed = Some(Instant::now());
                }
                self.stale_zone = false;
                self.zone_region = self.zone_asked_by.clone();
                self.satanic = Some(SatanicZone {
                    zone: zone.clone(),
                    buffs: buffs.clone(),
                    debuffs: debuffs.clone(),
                });
                self.satanic_at = Some(now_ms());
            }
        }
        None
    }

    /// Gold totals only make sense once the season mode is known, and only
    /// while it stays the same — a mode switch is a different purse.
    fn apply_currency(&mut self, c: &crate::parser::Currency) {
        self.last_currency = Some(c.clone());
        // The client says what it banks the moment it banks it, and the server
        // answers with the new balance. The deposit is counted straight away —
        // it is the only earnings signal that survives a tracker restart — and
        // then subtracted from the balance step so the same coins count once.
        // A step credited a moment ago is only in flight for as long as it
        // takes the other packet to arrive. Past that it was something else —
        // a quest paying into the bank, a sale — and it must not swallow a real
        // deposit later in the session.
        if self.pending_since.is_some_and(|at| at.elapsed() > IN_FLIGHT) {
            self.pending_step = 0;
            self.pending_since = None;
        }
        if c.delta > 0 {
            // Whichever of the two arrived first, the coins count once. Only
            // the forward direction used to cancel, so a balance that beat its
            // own deposit was credited and then credited again.
            let already = self.pending_step.min(c.delta);
            self.pending_step -= already;
            let fresh = c.delta - already;
            if fresh > 0 {
                self.gold_earned += fresh;
                self.progressed();
                self.banked += fresh;
            }
            self.last_bank = Some(Instant::now());
        }
        // the save names the purse; before it arrives, an unambiguous packet
        // will do, and the save corrects it if it disagrees
        let Some(mode) = self.season_mode.or_else(|| c.only_purse()) else { return };
        let current = c.for_mode(mode);
        if current == 0 {
            return;
        }
        self.last_bank = Some(Instant::now());
        if self.stale_bank {
            // carried over from the last run: only the deposits seen since the
            // tracker started are ours to claim
            self.total_gold = current;
            self.gold_high = current;
            self.gold_mode = Some(mode);
            self.stale_bank = false;
            self.banked = 0;
            return;
        }
        if self.total_gold != 0 && self.gold_mode == Some(mode) {
            // Measured from the highest it has been, not from where it last
            // was: climbing back to a level the bank has already held is a
            // return, not an income.
            let diff = current - self.total_gold.max(self.gold_high);
            if diff > 0 {
                let already = self.banked.min(diff);
                self.banked -= already;
                if diff > already {
                    let fresh = diff - already;
                    self.gold_earned += fresh;
                    // Remembered in case the deposit that caused it has not
                    // arrived yet; the branch above cancels against this.
                    self.pending_step += fresh;
                    self.pending_since = Some(Instant::now());
                    self.progressed();
                }
            }
        }
        self.total_gold = current;
        // A different purse is a different balance, and the mark that stops a
        // return being counted as income has nothing to say about it.
        //
        // Left standing it says the wrong thing for the whole session. A
        // returning player's seasonal purse is empty while the blood-pact one
        // still holds a million and a half, so the first save names the funded
        // purse and the mark anchors there; the next save names the seasonal
        // one and the balance drops to a few thousand. From then on every climb
        // is measured against the abandoned peak, `diff` is always negative,
        // and vendor income, mail and quest gold all register as exactly zero —
        // silently, and persisted into runs.json as a run that earned nothing.
        self.gold_high =
            if self.gold_mode == Some(mode) { self.gold_high.max(current) } else { current };
        self.gold_mode = Some(mode);
    }

    /// Called once a sampling interval by the watcher thread.
    pub fn sample(&mut self) {
        // a paused run has nothing to plot: its clock is not moving
        if self.paused() {
            return;
        }
        self.revision += 1;
        if self.series.len() >= SERIES_CAP {
            return;
        }
        self.extra_rev += 1;
        self.series.push(SeriesPoint {
            t: self.active().as_secs(),
            gold: self.gold_earned,
            xp: self.xp_earned,
        });
    }

    fn per_hour(&self, value: i64) -> i64 {
        let secs = self.active().as_secs();
        if secs == 0 {
            0
        } else {
            value * 3600 / secs as i64
        }
    }

    /// The bosses and chests this session has to its name, in the table's own
    /// order and without the ones still at zero — a list of everything the game
    /// counts would be a wall of noughts.
    fn tallies(&self) -> Vec<TallyCount> {
        TALLIES
            .iter()
            .filter_map(|(key, label, group)| {
                let total = *self.tally_earned.get(key)?;
                (total > 0).then(|| TallyCount {
                    label: label.to_string(),
                    group: group.to_string(),
                    total,
                })
            })
            .collect()
    }

    pub fn snapshot(&self, status: String) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|(name, c)| {
                (name.to_string(), ItemStats {
                    total: c.total,
                    mf: c.mf,
                    per_hour: self.per_hour(c.total),
                })
            })
            .collect();
        Snapshot {
            status,
            session_secs: self.active().as_secs(),
            paused: self.paused(),
            has_mail: self.has_mail,
            gold: Line {
                total: self.total_gold,
                earned: self.gold_earned,
                per_hour: self.per_hour(self.gold_earned),
            },
            xp: Line {
                total: self.total_xp,
                earned: self.xp_earned,
                per_hour: self.per_hour(self.xp_earned),
            },
            kills: Line {
                total: self.total_kills,
                earned: self.kills_earned,
                per_hour: self.per_hour(self.kills_earned),
            },
            save_age_secs: self.last_save.map(|t| t.elapsed().as_secs()),
            bank_age_secs: self.last_bank.map(|t| t.elapsed().as_secs()),
            carried_bank: self.stale_bank,
            carried_totals: self.stale_save,
            resources: self.resources.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            notable: self
                .prefs
                .notable_defs
                .iter()
                .map(|(label, _)| NotableCount {
                    label: label.clone(),
                    total: self.notable.get(label).copied().unwrap_or(0),
                })
                .collect(),
            items,
            satanic_zone: self.satanic.clone(),
            satanic_at: self.satanic_at,
            room: self.room.clone(),
            act: self.act,
            mf: self.mf,
            satanic_here: self.satanic_here,
            character: self.character.clone(),
            tallies: self.tallies(),
            ss: self.graded(SS_TIER),
        }
    }

    pub fn extra(&self) -> Extra {
        Extra {
            character: self.character.clone(),
            series: self.series.clone(),
            drops: self.drops.iter().rev().cloned().collect(),
        }
    }
}

/// The rarity the packet claims, if it maps to a known one.
pub fn rarity_from_packet(rarity: &Value) -> Option<String> {
    // numbers arrive as floats ("d": 5.0) — normalise before matching
    let key = match crate::parser::as_int(rarity) {
        Some(n) => n.to_string(),
        None => match rarity {
            Value::String(s) => s.trim().to_string(),
            _ => return None,
        },
    };
    if let Some((_, name)) = RARITIES.iter().find(|(id, _)| *id == key) {
        return Some(name.to_string());
    }
    if key.is_empty() || key.parse::<i64>().is_ok() {
        return None;
    }
    let mut chars = key.chars();
    let titled = match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => key,
    };
    RARITIES.iter().any(|(_, n)| *n == titled).then_some(titled)
}

#[cfg(test)]
#[allow(dead_code)]
pub fn rarity_name(rarity: &Value) -> String {
    rarity_from_packet(rarity).unwrap_or_else(|| "Unknown".into())
}

/// The currency the account plays with, as the packets name it.
fn currency_mode(mode: &str) -> Option<&'static str> {
    ["GSS", "GSH", "GNS", "GNH", "GBP"].iter().copied().find(|m| *m == mode)
}

#[derive(Serialize, Deserialize, Default)]
pub struct Carried {
    pub gold: i64,
    pub mode: Option<String>,
    pub xp: i64,
    pub kills: i64,
}

#[derive(Serialize)]
pub struct Line {
    pub total: i64,
    pub earned: i64,
    pub per_hour: i64,
}

#[derive(Serialize)]
pub struct ItemStats {
    pub total: i64,
    pub mf: i64,
    pub per_hour: i64,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub status: String,
    pub session_secs: u64,
    /// the clock is stopped: by hand, or because nothing has happened for a while
    pub paused: bool,
    /// how long ago the game last reported these — it only does so when it
    /// saves the character or banks gold
    pub save_age_secs: Option<u64>,
    pub bank_age_secs: Option<u64>,
    /// the totals are still the ones the last run left behind: the game has
    /// not confirmed them yet this session
    pub carried_bank: bool,
    pub carried_totals: bool,
    pub has_mail: bool,
    pub gold: Line,
    pub xp: Line,
    pub kills: Line,
    pub resources: HashMap<String, i64>,
    pub notable: Vec<NotableCount>,
    pub items: HashMap<String, ItemStats>,
    pub satanic_zone: Option<SatanicZone>,
    /// unix milliseconds; see `GameStats::satanic_at`
    pub satanic_at: Option<u64>,
    /// where the character is standing, e.g. "Act_08_02"
    pub room: Option<String>,
    /// Which act the character is in, from the save, where the room is not
    /// known. The room comes only with the heartbeat and the heartbeat is
    /// occasional; this arrives with every save.
    pub act: i64,
    /// magic find, live off the heartbeat, and whether this room is the
    /// satanic zone — the game says so itself
    pub mf: i64,
    pub satanic_here: bool,
    pub character: Option<CharacterInfo>,
    /// bosses put down and chests opened this session
    pub tallies: Vec<TallyCount>,
    /// SS-graded drops this session. The grades already exist per tier; this is
    /// the top one, pulled out because it is the one a run is judged on.
    pub ss: i64,
}

#[derive(Serialize)]
pub struct Extra {
    pub character: Option<CharacterInfo>,
    pub series: Vec<SeriesPoint>,
    pub drops: Vec<DropEntry>,
}

#[cfg(test)]
mod tests {
    /// Every tally is filed under what the game counted, and the game says so
    /// in the name of the field.
    ///
    /// Two were not, and it showed on the overlay: a Chaos Tower floor counts
    /// when it is CLEARED, and filed as a boss it made the boss figure rise
    /// from 45 to 46 on a staircase, with nothing killed. A wormhole is the
    /// same. The field names are the check — `...kills` is a kill, `...opened`
    /// is a chest, `...clears` is neither — so a new counter cannot join the
    /// wrong list quietly.
    #[test]
    fn a_tally_is_filed_under_what_the_game_counted() {
        for (key, label, group) in super::TALLIES {
            let want = match *group {
                "boss" => "kills",
                "chest" => "opened",
                "clear" => "clears",
                other => panic!("{label}: no such group as {other:?}"),
            };
            assert!(
                key.ends_with(want),
                "{label} is filed under {group}, but the game calls its counter {key}"
            );
        }
    }

    use super::*;
    use crate::parser::{self, Currency, GameEvent};
    use serde_json::json;

    fn item(rarity: serde_json::Value, mf: bool) -> GameEvent {
        named_item(rarity, mf, "", "")
    }

    fn named_item(rarity: serde_json::Value, mf: bool, name: &str, fingerprint: &str) -> GameEvent {
        GameEvent::ItemAdded {
            rarity,
            unscaled: false,
            mf,
            tier: 3,
            item_type: 0,
            item_id: 0,
            weapon_type: 0,
            seed: 0,
            name: name.into(),
            announced: false,
            amount: 1,
            fingerprint: fingerprint.into(),
            hash: String::new(),
            ground: false,
        }
    }

    fn tiered_satanic(tier: i64, fingerprint: &str) -> GameEvent {
        match named_item(json!(6), false, "", fingerprint) {
            GameEvent::ItemAdded { rarity, mf, item_type, item_id, weapon_type, seed, name, announced, amount, fingerprint, ground, .. } => {
                GameEvent::ItemAdded {
                    rarity, unscaled: false, mf, tier, item_type, item_id, weapon_type, seed, name,
                    announced, amount, fingerprint, hash: String::new(), ground,
                }
            }
            other => other,
        }
    }

    fn satanic_zone(zone: &str) -> GameEvent {
        rolled_zone(zone, vec![1, 2, 3])
    }

    fn rolled_zone(zone: &str, buffs: Vec<u8>) -> GameEvent {
        GameEvent::SatanicZone { zone: zone.into(), buffs, debuffs: vec![4] }
    }

    fn notable_item(name: &str, item_type: i64, amount: i64) -> GameEvent {
        GameEvent::ItemAdded {
            rarity: json!(1),
            unscaled: false,
            mf: false,
            tier: 0,
            item_type,
            item_id: 0,
            weapon_type: 0,
            seed: 0,
            name: name.into(),
            announced: false,
            amount,
            fingerprint: format!("fp-{name}"),
            hash: String::new(),
            ground: false,
        }
    }

    fn ground_item(rarity: serde_json::Value, name: &str, seed: i64) -> GameEvent {
        GameEvent::ItemAdded {
            rarity,
            unscaled: false,
            mf: false,
            tier: 0,
            item_type: 1,
            item_id: 7,
            weapon_type: 0,
            seed,
            name: name.into(),
            announced: false,
            amount: 1,
            fingerprint: String::new(),
            hash: String::new(),
            ground: true,
        }
    }

    fn account(season: i64, hardcore: i64, blood_pact: i64) -> GameEvent {
        account_xp(season, hardcore, blood_pact, 0)
    }

    fn in_act(act: i64) -> GameEvent {
        match account_xp(1, 0, 0, 1) {
            GameEvent::Account { experience, has_experience, season, hardcore, blood_pact, name, level, herolevel, difficulty, hell_sub, kills, tallies, .. } => {
                GameEvent::Account {
                    experience, act, has_experience, season, hardcore, blood_pact, name, level,
                    herolevel, difficulty, hell_sub, kills, tallies,
                }
            }
            other => other,
        }
    }

    /// A room the save has outlived is not where the character is.
    ///
    /// The game names the room about twenty times less often than it used to,
    /// so the last one heard goes stale while the save keeps saying which act.
    /// Left to outrank it, `Flooded Plains` sat in the panel through visits to
    /// three other acts.
    #[test]
    fn a_room_from_another_act_is_retired() {
        let mut s = GameStats::default();
        s.apply(&in_act(8));
        s.apply(&GameEvent::Room("Act_08_02".into()));
        assert_eq!(s.snapshot(String::new()).room.as_deref(), Some("Act_08_02"));

        // the save moves on; the room it belonged to does not survive it
        s.apply(&in_act(6));
        let snap = s.snapshot(String::new());
        assert_eq!(snap.room, None, "the room was in act 8");
        assert_eq!(snap.act, 6, "and the act is all that is known now");

        // a town belongs to its act too
        s.apply(&GameEvent::Room("Town_06_rm".into()));
        s.apply(&in_act(2));
        assert_eq!(s.snapshot(String::new()).room, None, "towns are numbered by act");

        // and a room that names no act is left alone: nothing contradicts it
        s.apply(&GameEvent::Room("Shadow_Realm_rm".into()));
        s.apply(&in_act(9));
        assert_eq!(
            s.snapshot(String::new()).room.as_deref(),
            Some("Shadow_Realm_rm"),
            "the Shadow Realm belongs to no act"
        );
    }

    fn account_xp(season: i64, hardcore: i64, blood_pact: i64, experience: i64) -> GameEvent {
        GameEvent::Account {
            experience,
            act: 0,
            has_experience: experience > 0,
            season,
            hardcore,
            blood_pact,
            name: "Test".into(),
            level: 10,
            herolevel: 20,
            difficulty: 2,
            hell_sub: 0,
            kills: 0,
            tallies: HashMap::new(),
        }
    }

    #[test]
    fn items_count_by_rarity_id_and_name() {
        let mut s = GameStats::default();
        s.set_prefer_ground(false);
        s.apply(&item(json!(6), true));
        s.apply(&item(json!("Satanic"), false));
        s.apply(&item(json!("satanic"), false));
        s.apply(&item(json!(999), false));
        let snap = s.snapshot(String::new());
        assert_eq!(snap.items["Satanic"].total, 3);
        assert_eq!(snap.items["Satanic"].mf, 1);
        assert_eq!(s.extra().drops.len(), 3);
    }

    /// A real drop packet, all the way to the line the journal shows.
    ///
    /// `resolve_rarity` is checked on its own in `parser`, and the counters
    /// were never checked against a packet at all — which is how a Set gun
    /// reached the journal green and chiming as an Angelic find. The shape and
    /// the fingerprint are the server's, out of a capture; `-3` with `b: 11`
    /// and `j: 14` is Angel the Set gun, `-13` with `b: 30` is Justice the
    /// tarot card, and `d: 7` is the packet calling both of them Angelic.
    #[test]
    fn a_drop_reaches_the_journal_as_the_item_it_is() {
        let dropped = |fingerprint: &str, id: i64, wt: i64, sh: &str| {
            crate::parser::events_from_messages(&[json!({
                "status": 1,
                "message": "ok",
                "itemGenHash": "abc",
                "operationTime": 1,
                "itemData": {
                    fingerprint: {"a": 61067529, "b": id, "c": 1, "d": 7, "e": 0,
                                  "gd": {"pos": [11, 0]}, "j": wt, "sh": sh}
                }
            })])
        };

        let mut s = GameStats::default();
        let gun = s.apply(&dropped("7-4964607-65a04f84c51d80001-3", 11, 14, "ecc3352481d6")[0]);
        let gun = gun.expect("a Set find is worth announcing");
        assert_eq!((gun.name.as_str(), gun.rarity.as_str(), gun.tier), ("Angel", "Set", 5));

        // the card shares that name with nothing but a Heroic orb, and is
        // neither: it is a Common the journal has no reason to announce
        let card = s.apply(&dropped("7-4964607-65a04f84c51d80002-13", 30, 0, "b0a1c2d3e4f5")[0]);
        assert!(card.is_none(), "a Common tarot card is not news");

        let snap = s.snapshot(String::new());
        assert_eq!(snap.items["Set"].total, 1);
        assert_eq!(snap.items["Angelic"].total, 0, "which is what the packet claimed for both");
        // a collectible is counted as one, and the grade columns are gear only
        assert_eq!(snap.resources["collectibles"], 1);
        assert_eq!(snap.items["Common"].total, 0);
    }

    /// A vault can be listed by the rarity it came in.
    ///
    /// Seven Essence Vaults share one display name, so a list naming it fired
    /// for all seven. The packet is a pickup out of a capture — the ground
    /// sighting of a vault is refused, `c: 0` being an ordinary base there —
    /// with `-19` the vault type and `b` saying which of the seven.
    #[test]
    fn a_list_can_name_one_of_the_seven_vaults() {
        let vault = |which: i64, sh: &str| {
            crate::parser::events_from_messages(&[json!({
                "status": 1,
                "message": "Success on inventory update ext",
                "goldAmount": 0,
                "newHashes": {},
                "operations": { "add": {
                    "99-4964607-1a042be8def-19": {"a": 98768726, "b": which, "c": 0, "d": 8,
                                                  "e": 0, "j": 0, "sh": sh}
                }}
            })])
            .into_iter()
            .next()
            .expect("a pickup")
        };

        // named at all, which a vault was not: nothing marks it, and its type
        // is not one the packet's own rarity would have got it named for
        let GameEvent::ItemAdded { name, item_type, item_id, .. } = &vault(5, "a") else {
            panic!("not an item")
        };
        assert_eq!((name.as_str(), *item_type, *item_id), ("Essence Vault", 19, 5));

        let mut s = GameStats::default();
        s.set_sound_lists(vec![("list-vault".into(), vec!["Essence Vault (Angelic)".into()])]);
        let angelic = s.apply(&vault(5, "a")).expect("the one the list names");
        assert_eq!(angelic.rarity, "Angelic");
        assert_eq!(angelic.sound.as_deref(), Some("list-vault"));
        assert!(s.apply(&vault(0, "b")).is_none(), "the Superior one is another item");

        // the bare name still means any of them
        s.set_sound_lists(vec![("list-any".into(), vec!["Essence Vault".into()])]);
        let superior = s.apply(&vault(0, "c")).expect("any vault");
        assert_eq!(superior.rarity, "Superior");
        assert_eq!(superior.sound.as_deref(), Some("list-any"));
    }

    #[test]
    fn float_rarities_are_recognised() {
        // the protocol writes whole numbers as floats
        assert_eq!(rarity_from_packet(&json!(6.0)).as_deref(), Some("Satanic"));
        assert_eq!(rarity_from_packet(&json!("9.0")).as_deref(), Some("Heroic"));
    }

    #[test]
    fn filter_silences_alerts_without_touching_counters() {
        let mut s = GameStats::default();
        s.set_prefer_ground(false);
        s.set_filter(vec!["Satanic".into()], 4);
        // right rarity, tier below the floor
        assert!(s.apply(&tiered_satanic(2, "8-1-1")).is_none(), "low tier must not alert");
        // right rarity and tier
        assert!(s.apply(&tiered_satanic(7, "8-2-1")).is_some());
        // filtered-out rarity
        assert!(s.apply(&named_item(json!(9), false, "", "8-3-1")).is_none());
        let snap = s.snapshot(String::new());
        assert_eq!(snap.items["Satanic"].total, 2, "counters ignore the filter");
        assert_eq!(snap.items["Heroic"].total, 1);
    }

    #[test]
    fn notable_drops_are_counted_by_name() {
        let mut s = GameStats::default();
        s.apply(&notable_item("Angelic Key", 12, 2));
        s.apply(&notable_item("Jol", 15, 1));
        s.apply(&notable_item("Zed", 15, 1));
        s.apply(&notable_item("Ol", 15, 1));
        let snap = s.snapshot(String::new());
        let by = |label: &str| snap.notable.iter().find(|n| n.label == label).unwrap().total;
        assert_eq!(by("Angelic Key"), 2);
        assert_eq!(by("SS runes"), 1, "Jol is one of the four level-100 runes");
        assert_eq!(by("S runes"), 1, "Zed is graded S");
    }

    #[test]
    fn identity_packets_do_not_override_the_real_season_mode() {
        let mut s = GameStats::default();
        s.apply(&account_xp(CURRENT_SEASON, 0, 0, 5_000)); // full packet: GSS
        // a later login-identity packet claims season 0 with no experience
        s.apply(&account(0, 0, 0));
        assert_eq!(s.season_mode, Some("GSS"));
        assert_eq!(s.character.as_ref().unwrap().level, 10);
    }

    /// The two packets exactly as the game sent them, in both possible orders.
    fn account_packet(name: &str, kills: i64, experience: i64) -> GameEvent {
        GameEvent::Account {
            experience,
            act: 0,
            has_experience: true,
            season: CURRENT_SEASON,
            hardcore: 0,
            blood_pact: 0,
            name: name.into(),
            level: 100,
            herolevel: 112,
            difficulty: 2,
            hell_sub: 0,
            kills,
            tallies: HashMap::new(),
        }
    }

    #[test]
    fn a_save_does_not_bank_the_same_deposit_again() {
        let mut s = GameStats::default();
        s.apply(&account_packet("x", 0, 0)); // names the purse, calibrates
        s.apply(&GameEvent::Gold(Currency { gss: 10_000, delta: 2_600, ..Default::default() }));
        assert_eq!(s.snapshot(String::new()).gold.earned, 2_600);

        // the save replays the last currency packet to re-read the balance now
        // that the purse is known; it must not count the deposit a second time
        for _ in 0..3 {
            s.apply(&account_packet("x", 0, 0));
        }
        assert_eq!(
            s.snapshot(String::new()).gold.earned,
            2_600,
            "three saves must not turn one deposit into four"
        );

        // and a genuine later deposit is still counted in full
        s.apply(&GameEvent::Gold(Currency { gss: 12_161, delta: 2_161, ..Default::default() }));
        assert_eq!(s.snapshot(String::new()).gold.earned, 4_761);
    }

    #[test]
    fn a_deposit_counts_once_when_the_new_balance_follows_it() {
        // real order from a capture: the client banks 2600, then the server
        // reports the balance that already contains it
        let mut s = GameStats::default();
        s.apply(&account_packet("Parahryushka", 0, 84_833_801));
        let feed = |s: &mut GameStats, packet: serde_json::Value| {
            for e in parser::events_from_messages(&[packet]) {
                s.apply(&e);
            }
        };
        feed(&mut s, json!({"currencyData": {"GSS": 720_239}}));
        feed(&mut s, json!({"amount_gold": "2600"}));
        feed(&mut s, json!({"currencyData": {"GSS": 722_839}}));
        let snap = s.snapshot(String::new());
        assert_eq!(snap.gold.earned, 2600, "the deposit counts, the balance does not repeat it");
        assert_eq!(snap.gold.total, 722_839);
        // gold that appears without a deposit (mail, selling) still counts
        feed(&mut s, json!({"currencyData": {"GSS": 723_000}}));
        assert_eq!(s.snapshot(String::new()).gold.earned, 2761);
    }

    /// A friend's item is not a find, even off the floor.
    ///
    /// Both messages are out of the capture that reported this, verbatim: the
    /// client naming itself, then the server confirming the pickup of a Torch
    /// of Shadows. The two accounts in them are the whole of the difference —
    /// the client is 4964607 and the fingerprint says 133690701 — and there is
    /// nothing else in the packet to go on: it is named, it is flagged `c: 1`,
    /// and it entered the bags exactly as a real find does.
    #[test]
    fn an_item_made_for_somebody_else_is_not_a_find() {
        let mut s = GameStats::default();
        let feed = |s: &mut GameStats, packet: serde_json::Value| {
            for e in parser::events_from_messages(&[packet]) {
                s.apply(&e);
            }
        };

        let hello = json!({
            "account_id": "49646",
            "beta": "0",
            "hardcore": "0",
            "season": "0",
            "unique_account_id": "4964607"
        });
        let pickup = json!({
            "goldAmount": 0,
            "message": "Success on inventory update ext",
            "newHashes": {},
            "operations": {
                "add": {
                    "99-133690701-1a03ba73b5f-10":
                        {"a": 781190902, "b": 23, "c": 1, "d": 4, "e": 0, "j": 0, "sh": "a4a54a715ab5", "w": 1}
                }
            },
            "status": 1
        });

        feed(&mut s, hello);
        feed(&mut s, pickup);
        let found = |s: &mut GameStats| {
            s.snapshot(String::new()).items.get("Heroic").map(|i| i.total).unwrap_or(0)
        };
        assert_eq!(found(&mut s), 0, "a friend handing you a thing is not you finding one");

        // and another of the same kind, made for us, still is — its own roll and
        // its own hash, or the sighting it repeats would be the one above
        let ours = json!({
            "goldAmount": 0,
            "message": "Success on inventory update ext",
            "operations": {
                "add": {
                    "99-4964607-1a03ba74c88-10":
                        {"a": 55123904, "b": 23, "c": 1, "d": 4, "e": 0, "j": 0, "sh": "0f21b6c4d7e3", "w": 1}
                }
            },
            "status": 1
        });
        feed(&mut s, ours);
        assert_eq!(found(&mut s), 1, "our own still counts");
    }

    /// Mail already in the box is not mail arriving.
    ///
    /// The chime watches for "none" becoming "some". Nothing said which of
    /// those a silent tracker was in, so the first answer from the server —
    /// which on a launch is the state of a box that may have been full for
    /// days — looked like a change and rang. Reported by a player on Bloodpact,
    /// a mode with no mailbox in it, who heard it on every start.
    #[test]
    fn mail_that_was_already_there_does_not_announce_itself() {
        let mut s = GameStats::default();
        assert_eq!(s.mail_state(), None, "before anything is said, nothing is known");

        // the server's first word, on a box that is already full
        s.apply(&GameEvent::Mail(true));
        assert_eq!(s.mail_state(), Some(true));

        // a fresh tracker told there is none knows that too, and differently
        let mut empty = GameStats::default();
        empty.apply(&GameEvent::Mail(false));
        assert_eq!(empty.mail_state(), Some(false));
        empty.apply(&GameEvent::Mail(true));
        assert_eq!(empty.mail_state(), Some(true), "and that is the arrival worth a chime");

        // A Reset restarts the counters, not the account. If it forgot, the
        // next answer would look like a first one and ring again.
        s.reset();
        assert_eq!(s.mail_state(), Some(true), "the box is not emptied by the Reset button");
    }

    /// Picking up what you have just put down is not finding it.
    ///
    /// A worn item thrown on the floor and taken back arrives as two ordinary
    /// inventory operations, and the second is shaped exactly like a find. The
    /// two below are the ones that were reported, with the fingerprints they
    /// carried in the capture: a Pendant of Eternity and a pair of Tectonic
    /// Grips, dropped and picked straight back up, both announced as though
    /// they had fallen.
    #[test]
    fn an_item_you_dropped_yourself_is_not_found_when_you_pick_it_up() {
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        s.set_filter(vec!["Heroic".into()], 0);

        let pendant = "7-4964607-65953287a338c0001-5";
        let pickup = |fp: &str| GameEvent::ItemAdded {
            rarity: json!(9),
            unscaled: false,
            mf: false,
            tier: 6,
            item_type: 5,
            item_id: 25,
            weapon_type: 0,
            seed: 0,
            name: "Pendant of Eternity".into(),
            announced: false,
            amount: 1,
            fingerprint: fp.into(),
            hash: String::new(),
            ground: false,
        };

        // it leaves the bags, then comes straight back
        s.apply(&GameEvent::ItemsLetGo(vec![pendant.into()]));
        assert!(s.apply(&pickup(pendant)).is_none(), "the same item returning is not a find");
        assert_eq!(s.snapshot(String::new()).items["Heroic"].total, 0);

        // and the memory is spent: put down once, back once. A second arrival
        // with that fingerprint is a real sighting again.
        assert!(s.apply(&pickup(pendant)).is_some(), "only the return itself is excused");
    }

    /// The same pair with a reset between them: bank the loot at the vendor,
    /// then start the next run clean. `banked` is the only thing that stops
    /// the balance from being counted a second time, and the reset used to
    /// leave it behind — so the coins the finished run was already credited
    /// with turned up again as the new session's earnings.
    #[test]
    fn a_reset_between_a_deposit_and_its_balance_earns_the_gold_once() {
        let mut s = GameStats::default();
        s.apply(&account_packet("Parahryushka", 0, 84_833_801));
        let feed = |s: &mut GameStats, packet: serde_json::Value| {
            for e in parser::events_from_messages(&[packet]) {
                s.apply(&e);
            }
        };
        feed(&mut s, json!({"currencyData": {"GSS": 720_239}}));
        feed(&mut s, json!({"amount_gold": "2600"}));
        assert_eq!(s.snapshot(String::new()).gold.earned, 2600, "the finished run keeps it");
        s.reset();
        feed(&mut s, json!({"currencyData": {"GSS": 722_839}}));
        assert_eq!(s.snapshot(String::new()).gold.earned, 0, "the new session did not earn it");
    }

    /// The same crossing, the other way round.
    ///
    /// The test above has the deposit arrive first and the balance confirm it
    /// after the Reset. Here the balance moves first and the client's own report
    /// of the deposit arrives after — the pair the engine cancels with
    /// `pending_step`, which did not travel across a Reset while its mirror
    /// `banked` did. So the report landed with nothing to cancel against and the
    /// fresh run opened claiming coins banked before it began.
    #[test]
    fn a_reset_between_a_balance_and_its_deposit_earns_the_gold_once() {
        let mut s = GameStats::default();
        s.apply(&account_packet("Parahryushka", 0, 84_833_801));
        let feed = |s: &mut GameStats, packet: serde_json::Value| {
            for e in parser::events_from_messages(&[packet]) {
                s.apply(&e);
            }
        };
        feed(&mut s, json!({"currencyData": {"GSS": 720_239}}));
        feed(&mut s, json!({"currencyData": {"GSS": 722_839}}));
        assert_eq!(s.snapshot(String::new()).gold.earned, 2600, "the balance climbed by 2600");
        s.reset();
        feed(&mut s, json!({"amount_gold": "2600"}));
        assert_eq!(
            s.snapshot(String::new()).gold.earned,
            0,
            "the deposit that caused that climb is not a second 2600"
        );
    }

    /// A bank drawn down and put back is not income, Reset or no Reset.
    #[test]
    fn the_peak_a_climb_is_measured_from_survives_a_reset() {
        let mut s = GameStats::default();
        s.apply(&account_packet("Parahryushka", 0, 84_833_801));
        let feed = |s: &mut GameStats, packet: serde_json::Value| {
            for e in parser::events_from_messages(&[packet]) {
                s.apply(&e);
            }
        };
        feed(&mut s, json!({"currencyData": {"GSS": 1_000_000}}));
        feed(&mut s, json!({"currencyData": {"GSS": 500_000}})); // withdrawn to gamble
        s.reset();
        feed(&mut s, json!({"currencyData": {"GSS": 1_000_000}})); // and put back
        assert_eq!(
            s.snapshot(String::new()).gold.earned,
            0,
            "returning to a level the bank has already held is a return, not income"
        );
    }

    #[test]
    fn a_deposit_before_the_first_balance_still_counts() {
        // a restart mid-session: the carried balance only re-anchors, but the
        // gold banked while the tracker was up is ours
        let mut s = GameStats::default();
        s.restore(&Carried { gold: 717_188, mode: Some("GSS".into()), xp: 0, kills: 0 });
        s.apply(&account_packet("Parahryushka", 0, 84_833_801));
        for e in parser::events_from_messages(&[json!({"amount_gold": "2600"})]) {
            s.apply(&e);
        }
        for e in parser::events_from_messages(&[json!({"currencyData": {"GSS": 722_839}})]) {
            s.apply(&e);
        }
        let snap = s.snapshot(String::new());
        assert_eq!(snap.gold.earned, 2600);
        assert_eq!(snap.gold.total, 722_839);
    }

    #[test]
    fn totals_carried_from_the_last_run_do_not_count_as_earned() {
        let mut s = GameStats::default();
        s.restore(&Carried { gold: 700_000, mode: Some("GSS".into()), xp: 90_000_000, kills: 912_000 });
        // whatever the game reports first is the new baseline, not a windfall
        s.apply(&account_packet("Parahryushka", 913_000, 91_000_000));
        for e in parser::events_from_messages(&[json!({"currencyData": {"GSS": 715_517}})]) {
            s.apply(&e);
        }
        let snap = s.snapshot(String::new());
        assert_eq!(snap.gold.total, 715_517);
        assert_eq!(snap.gold.earned, 0, "a restart must not invent earnings");
        assert_eq!(snap.xp.earned, 0);
        assert_eq!(snap.kills.earned, 0);
        // and from there it counts normally again
        s.apply(&account_packet("Parahryushka", 913_100, 91_500_000));
        for e in parser::events_from_messages(&[json!({"currencyData": {"GSS": 716_000}})]) {
            s.apply(&e);
        }
        let snap = s.snapshot(String::new());
        assert_eq!(snap.kills.earned, 100);
        assert_eq!(snap.xp.earned, 500_000);
        assert_eq!(snap.gold.earned, 483);
    }

    #[test]
    fn a_rune_counts_under_either_spelling() {
        let mut s = GameStats::default();
        s.apply(&notable_item("Ber", 15, 1));
        s.apply(&notable_item("Jah Rune", 15, 1));
        let snap = s.snapshot(String::new());
        let group = snap.notable.iter().find(|n| n.label == "S runes").expect("group exists");
        assert_eq!(group.total, 2, "both spellings land in the same group");
    }

    /// A list is meant to add a voice, not to take the others away.
    ///
    /// "Target Items" with three names in it should give those three a sound of
    /// their own and leave every other drop exactly as it was — the rarity
    /// switches above the filter still deciding. The other test here arms no
    /// rarity at all, so it cannot tell an additive filter from an exclusive
    /// one; this is the case a player actually sits in.
    /// A vial is gear, and counts in both columns like the rest of it.
    ///
    /// It is equipped, the same as a charm. Being on neither list it used to
    /// reach its rarity column and no grade column at all, so the panel showed
    /// twelve Heroic beside ten SS with nothing to explain the gap. Every
    /// Heroic item in the tables is graded SS, so a Heroic that is not also an
    /// SS could never have been anything but this.
    #[test]
    fn a_vial_is_gear_and_counts_in_both_columns() {
        let mut s = GameStats::default();
        let vial = GameEvent::ItemAdded {
            rarity: json!(9),
            unscaled: false,
            mf: false,
            tier: 6, // SS
            item_type: 18, // Vial: not a resource, and not gear either
            item_id: 5,
            weapon_type: 0,
            seed: 1,
            name: "Elixir of Unworldly Cognition".into(),
            announced: false,
            amount: 1,
            fingerprint: String::new(),
            hash: "v".into(),
            ground: true,
        };
        s.apply(&vial);
        let snap = s.snapshot(String::new());
        assert_eq!(snap.items.get("Heroic").map(|i| i.total), Some(1), "a Heroic find");
        assert_eq!(s.graded(6), 1, "and an SS one, which is what the columns disagreed about");
    }

    #[test]
    fn a_list_adds_a_voice_rather_than_taking_the_others_away() {
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        // the ordinary switches, as a player leaves them
        // Both names below are real items the tables call Heroic, and a known
        // name outranks whatever the packet claims - so that is what to arm.
        s.set_filter(vec!["Heroic".into()], 0);
        s.set_sound_lists(vec![("list-target".into(), vec!["AK-47".into()])]);
        let drop = |name: &str, hash: &str| GameEvent::ItemAdded {
            rarity: json!(9), // Heroic
            unscaled: false,
            mf: false,
            tier: 0,
            item_type: 3,
            item_id: 15,
            weapon_type: 14,
            seed: 1,
            name: name.into(),
            announced: false,
            amount: 1,
            fingerprint: String::new(),
            hash: hash.into(),
            ground: true,
        };

        let listed = s.apply(&drop("AK-47", "a")).expect("on a list, so it is announced");
        assert_eq!(listed.sound.as_deref(), Some("list-target"), "the list's own sound");

        let plain = s.apply(&drop("Eternity", "b")).expect("on no list, but its rarity is armed");
        assert_eq!(plain.sound.as_deref(), Some("heroic"), "still the rarity's sound");
    }

    /// A category on a list matches by what the item IS, not by a list of names.
    ///
    /// The whole reason rules exist: writing "every Satanic helmet" out as the
    /// 36 names the tables hold today would freeze August's table into the
    /// settings file, and the 37th Satanic helmet an update adds would fall in
    /// silence on a list the player believes says "every Satanic helmet".
    #[test]
    fn a_rule_puts_a_whole_category_on_a_list() {
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        // nothing armed by rarity, so only the list can speak
        s.set_filter(vec![], 6);
        s.set_listed(vec![Listed {
            key: "list-helms".into(),
            names: Vec::new(),
            rules: vec![Rule {
                rarity: Some("satanic".into()),
                item_type: Some(0),
                weapon: None,
            }],
        }]);
        let drop = |name: &str, item_type: i64, item_id: i64, weapon_type: i64, hash: &str| {
            GameEvent::ItemAdded {
                rarity: json!(6), // Satanic
                unscaled: false,
                mf: false,
                tier: 0,
                item_type,
                item_id,
                weapon_type,
                seed: 1,
                name: name.into(),
                announced: false,
                amount: 1,
                fingerprint: String::new(),
                hash: hash.into(),
                ground: true,
            }
        };

        let helm = s.apply(&drop("Harlequinn's Crest", 0, 0, 0, "a")).expect("a Satanic helmet");
        assert_eq!(helm.sound.as_deref(), Some("list-helms"), "the rule put it on the list");

        // The same rarity, the wrong type.
        assert!(
            s.apply(&drop("Godfather", 3, 0, 1, "b")).is_none(),
            "a Satanic sword is not a Satanic helmet"
        );
        // The right type, the wrong rarity.
        assert!(
            s.apply(&drop("Uabel's Helmet", 0, 7, 0, "c")).is_none(),
            "a Set helmet is not a Satanic one"
        );
    }

    /// The order of the lists is the priority, whichever way each one is written.
    ///
    /// The "earlier list wins" rule was a search through the names of each list
    /// in turn. With rules beside the names it is still one search through the
    /// lists in turn — not names first and rules afterwards, which would have
    /// let a later list's name quietly outrank an earlier list's category.
    #[test]
    fn the_earlier_list_wins_whether_it_says_a_name_or_a_category() {
        let drop = || GameEvent::ItemAdded {
            rarity: json!(6),
            unscaled: false,
            mf: false,
            tier: 0,
            item_type: 0,
            item_id: 0,
            weapon_type: 0,
            seed: 1,
            name: "Harlequinn's Crest".into(),
            announced: false,
            amount: 1,
            fingerprint: String::new(),
            hash: "one".into(),
            ground: true,
        };
        let category = || Listed {
            key: "list-category".into(),
            names: Vec::new(),
            rules: vec![Rule { rarity: Some("satanic".into()), item_type: Some(0), weapon: None }],
        };
        let by_name = || Listed {
            key: "list-name".into(),
            names: vec!["harlequinn's crest".into()],
            rules: Vec::new(),
        };

        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        s.set_listed(vec![category(), by_name()]);
        assert_eq!(
            s.apply(&drop()).and_then(|d| d.sound).as_deref(),
            Some("list-category"),
            "the category is first, so the category sounds"
        );

        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        s.set_listed(vec![by_name(), category()]);
        assert_eq!(
            s.apply(&drop()).and_then(|d| d.sound).as_deref(),
            Some("list-name"),
            "the name is first, so the name sounds"
        );
    }

    /// A rule asks the tables what the item is, and the identity answers first.
    ///
    /// `Shrunken Head` is two items: a Satanic charm at 10:37:0 and a Common
    /// relic at 16:28:0. Reading the rarity off the name alone would have put
    /// the relic in "every Satanic charm" — the very mistake `resolve_rarity`
    /// records having cost that relic its own chime once already.
    #[test]
    fn a_rule_reads_the_identity_before_the_name_it_shares() {
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        s.set_filter(vec![], 6);
        s.set_listed(vec![Listed {
            key: "list-charms".into(),
            names: Vec::new(),
            rules: vec![Rule { rarity: Some("satanic".into()), item_type: Some(10), weapon: None }],
        }]);

        let charm = s
            .apply(&GameEvent::ItemAdded {
                rarity: json!(6),
                unscaled: false,
                mf: false,
                tier: 0,
                item_type: 10,
                item_id: 37,
                weapon_type: 0,
                seed: 1,
                name: "Shrunken Head".into(),
                announced: false,
                amount: 1,
                fingerprint: String::new(),
                hash: "charm".into(),
                ground: true,
            })
            .expect("the Satanic charm is in the category");
        assert_eq!(charm.sound.as_deref(), Some("list-charms"));

        // The relic of the same name. It arrives nameless, as every relic does,
        // and a rule needs a name before it will look at anything.
        assert!(
            s.apply(&relic(28, "relic28", true)).is_none(),
            "the Common relic sharing that name is not a Satanic charm"
        );
    }

    #[test]
    fn a_list_outranks_the_rarity_alerts() {
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        // nothing would normally be announced: no rarity is armed at all
        s.set_filter(vec![], 6);
        s.set_sound_lists(vec![("list-chase".into(), vec!["AK-47".into()])]);
        let drop = |name: &str, hash: &str| GameEvent::ItemAdded {
            rarity: json!(2),
            unscaled: false,
            mf: false,
            tier: 0,
            item_type: 3,
            item_id: 15,
            weapon_type: 14,
            seed: 1,
            name: name.into(),
            announced: false,
            amount: 1,
            fingerprint: String::new(),
            hash: hash.into(),
            ground: true,
        };
        let listed = s.apply(&drop("AK-47", "a")).expect("a listed item is always announced");
        assert_eq!(listed.sound.as_deref(), Some("list-chase"));
        // and an item that is on no list still obeys the switches
        assert!(s.apply(&drop("Eternity", "b")).is_none(), "unlisted items follow the filter");

        // the same item picked up is the same item: it chimed on the way down
        let picked = match drop("AK-47", "a") {
            GameEvent::ItemAdded { rarity, mf, tier, item_type, item_id, weapon_type, seed, name, announced, amount, fingerprint, hash, .. } => {
                GameEvent::ItemAdded {
                    rarity, unscaled: false, mf, tier, item_type, item_id, weapon_type, seed, name,
                    announced, amount, fingerprint, hash, ground: false,
                }
            }
            other => other,
        };
        assert!(s.apply(&picked).is_none(), "a list is not told twice about one item");
    }

    #[test]
    fn a_lone_purse_is_read_before_the_save_names_it() {
        let mut s = GameStats::default();
        let c = Currency { gss: 753_900, ..Default::default() };
        // no account packet yet: one purse has money, so it can only be that one
        s.apply(&GameEvent::Gold(c.clone()));
        assert_eq!(s.snapshot(String::new()).gold.total, 753_900);

        // two purses in play and there is nothing to go on — better a blank
        // than the wrong number
        let mut two = GameStats::default();
        let both = Currency { gss: 100, gns: 200, ..Default::default() };
        two.apply(&GameEvent::Gold(both));
        assert_eq!(two.snapshot(String::new()).gold.total, 0);

        // and the save still has the last word
        s.apply(&account(CURRENT_SEASON, 0, 0));
        s.apply(&GameEvent::Gold(c));
        assert_eq!(s.snapshot(String::new()).gold.total, 753_900);
        assert_eq!(s.gold_earned, 0, "reading a balance is not earning it");
    }

    #[test]
    fn the_flourish_asks_its_own_question() {
        let mut s = GameStats::default();
        // alerts want only the very top; the flourish is set wider
        s.set_filter(vec!["Unholy".into()], 6);
        s.set_flourish_filter(vec!["Satanic".into()], 5);
        s.set_prefer_ground(false);

        // an S-grade Satanic: nothing for the alerts, everything for the window
        let drop = s.apply(&tiered_satanic(5, "a")).expect("the flourish wants it");
        assert!(!drop.announce, "the alert rules did not ask for this one");
        assert!(drop.flourish);
        assert!(s.extra().drops.is_empty(), "and it does not join the journal");

        // below the flourish's grade and below the alerts' — nothing at all
        assert!(s.apply(&tiered_satanic(4, "b")).is_none());

        // switching the flourish off leaves the alerts as they were
        s.set_flourish_filter(Vec::new(), 6);
        assert!(s.apply(&tiered_satanic(5, "c")).is_none());
    }

    #[test]
    fn only_our_own_finds_are_announced() {
        let mut s = GameStats::default();
        s.set_filter(vec!["Set".into()], 6);
        s.set_flourish_filter(vec!["Set".into()], 1);
        s.apply(&account_packet("Parahryushka", 0, 0));

        let found = |who: &str| GameEvent::Found {
            finder: who.into(),
            name: "Doctor's Potion".into(),
        };

        // the whole shard reads this line; it is not our run
        assert!(s.apply(&found("SomebodyElse")).is_none(), "another player's luck is theirs");
        assert!(s.extra().drops.is_empty(), "and it does not reach the journal either");

        let ours = s.apply(&found("parahryushka")).expect("our own find still counts");
        assert_eq!(ours.sound.as_deref(), Some("set"));

        // before the first save there is nobody to be, so nothing is claimed
        let mut cold = GameStats::default();
        assert!(cold.apply(&found("Parahryushka")).is_none());
    }

    #[test]
    fn the_flourish_is_seen_and_not_heard() {
        let mut s = GameStats::default();
        // the alerts are set to the very top and the flourish to the bottom,
        // which is the pair the settings panel invites
        s.set_filter(vec!["Satanic".into()], 6);
        s.set_flourish_filter(vec!["Satanic".into()], 1);
        s.set_prefer_ground(false);

        let drop = s.apply(&tiered_satanic(1, "a")).expect("the flourish wants it");
        assert!(drop.flourish);
        assert!(!drop.announce, "a D item is below the alerts");
        assert_eq!(drop.sound, None, "so it must not make a sound either");

        // the grade the tables cannot establish is still every bit of "any"
        let unknown = s.apply(&tiered_satanic(0, "b")).expect("D means anything at all");
        assert!(unknown.flourish);
    }

    #[test]
    fn a_finished_run_leaves_the_settings_alone() {
        let mut s = GameStats::default();
        s.set_filter(vec!["Satanic".into()], 6);
        s.set_flourish_filter(vec!["Satanic".into()], 1);
        s.set_prefer_ground(false);

        // the game closing files the run and starts another; every preference
        // used to be copied across by hand, and this one was being dropped
        s.reset();

        let drop = s.apply(&tiered_satanic(1, "a")).expect("the flourish is still armed");
        assert!(drop.flourish);
    }

    #[test]
    fn bosses_and_chests_count_from_the_first_save_on() {
        let save = |satan: i64, odin: i64, ruby: i64| match account_packet("x", 1, 1) {
            GameEvent::Account { experience, season, hardcore, blood_pact, name, level, herolevel, difficulty, hell_sub, kills, .. } => {
                GameEvent::Account {
                    experience, has_experience: true, season, hardcore, blood_pact, name, level,
                    act: 0,
                    herolevel, difficulty, hell_sub, kills,
                    tallies: HashMap::from([
                        ("statisticsatankills".to_string(), satan),
                        ("statisticodinkills".to_string(), odin),
                        ("statisticrubychestsopened".to_string(), ruby),
                    ]),
                }
            }
            other => other,
        };
        let counted = |s: &GameStats, label: &str| {
            s.tallies().iter().find(|t| t.label == label).map_or(0, |t| t.total)
        };

        let mut s = GameStats::default();
        // the character arrives with a history; none of it belongs to this session
        s.apply(&save(60, 0, 376));
        assert!(s.tallies().is_empty(), "the first save only sets the mark");

        s.apply(&save(63, 1, 380));
        assert_eq!(counted(&s, "Satan"), 3);
        assert_eq!(counted(&s, "Ruby"), 4);
        // a counter that stood at zero still counts its first kill
        assert_eq!(counted(&s, "Odin"), 1);

        s.reset();
        assert!(s.tallies().is_empty(), "a reset starts the tally again");
        s.apply(&save(64, 1, 380));
        assert_eq!(counted(&s, "Satan"), 1, "and the game is not made to recount");
        assert_eq!(counted(&s, "Odin"), 0);
    }

    #[test]
    fn the_session_tallies_drops_by_grade() {
        let mut s = GameStats::default();
        // a piece of gear that states SS, then an Angelic Key: graded SS by
        // the table, but a resource — the grade columns count gear, so the key
        // is announced and journalled without joining the SS tally
        s.apply(&tiered_satanic(6, "a"));
        s.apply(&notable_item("Angelic Key", 12, 1));
        assert_eq!(s.graded(6), 1, "a key is not an SS find");
        // grade B, and a name the table cannot grade at all
        s.apply(&tiered_satanic(3, "b"));
        s.apply(&notable_item("Mystery Blade", 3, 1));
        assert_eq!(s.graded(3), 1);
        assert_eq!(s.graded(6), 1, "an item the table cannot grade is not an SS");
        // and the overlay's chip reads the top grade, not some other one
        assert_eq!(s.snapshot(String::new()).ss, 1);
        s.reset();
        assert_eq!(s.graded(6), 0, "the tally belongs to the session");
    }

    #[test]
    fn a_named_drop_is_graded_by_the_item_table() {
        // the packet that announces a named drop carries no tier, but the item
        // itself always has one — SS for the AK-47
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        s.set_filter(vec!["Satanic".into(), "Heroic".into()], 6);
        let drop = |name: &str, hash: &str| GameEvent::ItemAdded {
            rarity: json!(2),
            unscaled: false,
            mf: false,
            tier: 0,
            item_type: 3,
            item_id: 15,
            weapon_type: 14,
            seed: 1,
            name: name.into(),
            announced: false,
            amount: 1,
            fingerprint: String::new(),
            hash: hash.into(),
            ground: true,
        };
        let ss = s.apply(&drop("AK-47", "a")).expect("an SS drop passes an SS filter");
        assert_eq!(ss.tier, 6);
        // a Satanic helm the table grades C — announced rarity, wrong grade
        assert!(s.apply(&drop("Sky Crusader Helm", "b")).is_none(), "tier C is below SS");
        // and an item the table does not know cannot prove SS either
        assert!(s.apply(&drop("Mystery Blade", "c")).is_none(), "an ungraded item stays quiet");
    }

    #[test]
    fn the_servers_announcement_chimes_and_the_pickup_stays_quiet() {
        // "SERVER: Parahryushka Just found [Doctor's Potion]" — the game says
        // it the moment the item lands, before anything else knows the tier
        let mut s = GameStats::default();
        s.set_filter(vec!["Set".into()], 5);
        let announced = s
            .apply(&GameEvent::ItemAdded {
                rarity: Value::Null,
                unscaled: false,
                mf: false,
                tier: 0,
                item_type: 0,
                item_id: 0,
                weapon_type: 0,
                seed: 0,
                name: "Doctor's Potion".into(),
                announced: true,
                amount: 1,
                fingerprint: String::new(),
                hash: String::new(),
                ground: false,
            })
            .expect("an announced find is always shown");
        assert_eq!(announced.rarity, "Set");
        assert_eq!(announced.sound.as_deref(), Some("set"));
        // walking over it must not chime a second time
        let picked = s.apply(&GameEvent::ItemAdded {
            rarity: json!(4),
            unscaled: false,
            mf: false,
            tier: 6,
            item_type: 13,
            item_id: 86,
            weapon_type: 0,
            seed: 1,
            name: "Doctor's Potion".into(),
            announced: false,
            amount: 1,
            fingerprint: "13-1-1".into(),
            hash: String::new(),
            ground: false,
        });
        assert!(picked.is_none_or(|d| d.sound.is_none()), "one item, one chime");
    }

    #[test]
    fn the_tier_filter_belongs_to_pickup_alerts() {
        // the tier is per roll, not per item, and the drop packet never carries
        // it — so it can only narrow alerts that fire when an item is picked up
        let ak = |tier: i64, ground: bool, fp: &str| GameEvent::ItemAdded {
            rarity: json!(2),
            unscaled: false,
            mf: true,
            tier,
            item_type: 3,
            item_id: 15,
            weapon_type: 14,
            seed: 924_824_705,
            name: "AK-47".into(),
            announced: false,
            amount: 1,
            fingerprint: fp.into(),
            hash: String::new(),
            ground,
        };

        // alerting on the drop: rarity decides, the tier is unknown and ignored
        let mut on_drop = GameStats::default();
        on_drop.set_prefer_ground(true);
        on_drop.set_filter(vec!["Heroic".into()], 6);
        let entry = on_drop.apply(&ak(0, true, "")).expect("the drop is announced by rarity");
        assert_eq!(entry.sound.as_deref(), Some("heroic"));

        // alerting on the pickup: the tier is known and does its job
        let mut on_pickup = GameStats::default();
        on_pickup.set_prefer_ground(false);
        on_pickup.set_filter(vec!["Heroic".into()], 6);
        assert!(on_pickup.apply(&ak(3, false, "3-1-1")).is_none(), "tier B is below SS");
        assert!(on_pickup.apply(&ak(6, false, "3-1-2")).is_some(), "tier SS passes");
    }

    #[test]
    fn without_a_minimum_tier_the_drop_itself_is_announced() {
        // real capture of an SS weapon hitting the ground: rarity comes from
        // the name, and the packet carries no tier at all
        let mut s = GameStats::default();
        s.set_filter(vec!["Heroic".into()], 0);
        let entry = s.apply(&GameEvent::ItemAdded {
            rarity: json!(2),
            unscaled: false,
            mf: true,
            tier: 0,
            item_type: 3,
            item_id: 15,
            weapon_type: 14,
            seed: 924_824_705,
            name: "AK-47".into(),
            announced: false,
            amount: 1,
            fingerprint: String::new(),
            hash: String::new(),
            ground: true,
        });
        let entry = entry.expect("an SS drop must be announced");
        // the packet claims Superior; the name is what decides
        assert_eq!(entry.rarity, "Heroic");
        assert!(entry.ground);
    }

    #[test]
    fn a_rolled_back_kill_total_keeps_the_counter_moving() {
        // a real capture: saves 76..80 of one character, where the game itself
        // dropped the total by 3637 after an instance restart and climbed again
        let saves = [909_625, 909_625, 905_988, 906_175, 906_286];
        let mut s = GameStats::default();
        for kills in saves {
            s.apply(&account_packet("Parahryushka", kills, 75_807_189));
        }
        let snap = s.snapshot(String::new());
        // the rollback only re-anchors; the 298 kills made after it still count
        assert_eq!(snap.kills.earned, 906_286 - 905_988);
        assert_eq!(snap.kills.total, 906_286);
    }

    #[test]
    fn income_on_the_new_purse_survives_the_purse_changing() {
        // A returning player: the seasonal purse is empty and only the blood
        // pact one is funded, so the first packet has to guess and guesses that
        // one. The save then names the seasonal purse and the balance drops by
        // a million and a half. Everything earned from there on was measured
        // against the abandoned peak and came out negative, so gold earned read
        // exactly zero for the rest of the session.
        let mut s = GameStats::default();
        s.apply(&GameEvent::Gold(Currency { gbp: 1_706_231, ..Default::default() }));
        assert_eq!(s.gold_mode, Some("GBP"), "one funded purse names itself");

        s.apply(&account_packet("x", 0, 0));
        s.apply(&GameEvent::Gold(Currency { gss: 5_000, gbp: 1_706_231, ..Default::default() }));
        assert_eq!(s.gold_mode, Some("GSS"), "the save outranks the guess");

        s.apply(&GameEvent::Gold(Currency { gss: 6_000, gbp: 1_706_231, ..Default::default() }));
        assert_eq!(s.snapshot(String::new()).gold.earned, 1_000);
    }

    #[test]
    fn the_room_a_reset_carries_starts_its_clock() {
        // The room travels across a reset so the panel does not go blank. Its
        // clock did not, so the room the next session opens in banked nothing:
        // reset while standing somewhere, farm there for half an hour, and the
        // run card's "where it happened" was empty.
        let mut s = GameStats::default();
        s.apply(&GameEvent::Room("Act_09_06".into()));
        s.reset();
        assert_eq!(s.room.as_deref(), Some("Act_09_06"));
        assert!(s.room_since.is_some(), "the carried room is being timed");
    }

    #[test]
    fn two_currency_packets_make_earned_gold() {
        // the game reports the bank total, in either spelling, only when it
        // changes — the first one calibrates, the second one earns
        let mut s = GameStats::default();
        s.apply(&account_packet("Parahryushka", 0, 75_807_189));
        for total in [693_835, 694_452] {
            let packet = json!({
                "currencyData": {"GBP": 1706231, "GNH": 0, "GNS": 78101, "GSH": 0,
                                 "GSS": total, "account_id": 49646},
                "message": "Success!", "status": "1"
            });
            for e in parser::events_from_messages(&[packet]) {
                s.apply(&e);
            }
        }
        let snap = s.snapshot(String::new());
        assert_eq!(snap.gold.total, 694_452);
        assert_eq!(snap.gold.earned, 617);
    }

    #[test]
    fn real_login_packets_yield_the_bank_total() {
        let currency = json!({"currency_data": {"GBP": 1706231, "GNH": 0, "GNS": 78101, "GSH": 0, "GSS": 687514}});
        let account = json!({
            "name": "Parahryushka", "class": 3, "level": 100, "herolevel": 112,
            "difficulty": 2, "season": CURRENT_SEASON, "hardcore": 0, "blood_pact": 0,
            "experience": 63419870, "statisticTotalMonsterKills": 4210
        });
        for order in [[&currency, &account], [&account, &currency]] {
            let mut s = GameStats::default();
            for payload in order {
                for e in crate::parser::events_from_messages(std::slice::from_ref(payload)) {
                    s.apply(&e);
                }
            }
            let snap = s.snapshot(String::new());
            assert_eq!(snap.gold.total, 687_514, "gold total lost");
            assert_eq!(snap.xp.total, 63_419_870);
            assert_eq!(snap.kills.total, 4210);
        }
    }

    #[test]
    fn gold_replays_the_currency_that_preceded_the_account() {
        let mut s = GameStats::default();
        let gold = |g| GameEvent::Gold(Currency { gss: g, ..Default::default() });
        // Currency arrives before the season mode is known. One purse has money
        // and the others do not, so it is read at once; the account packet then
        // confirms the purse rather than revealing it.
        s.apply(&gold(100));
        assert_eq!(s.snapshot(String::new()).gold.total, 100);

        s.apply(&account(CURRENT_SEASON, 0, 0));
        assert_eq!(s.snapshot(String::new()).gold.total, 100);
        assert_eq!(s.gold_earned, 0, "a balance already shown is not earned again");
        s.apply(&gold(150));
        s.apply(&gold(120));
        let snap = s.snapshot(String::new());
        assert_eq!(snap.gold.total, 120);
        assert_eq!(snap.gold.earned, 50);
    }

    #[test]
    fn guild_xp_before_the_first_account_total_does_not_inflate() {
        let mut s = GameStats::default();
        s.apply(&GameEvent::XpGain(15)); // 100 character xp guessed
        s.apply(&account_xp(CURRENT_SEASON, 0, 0, 50_000_000));
        assert_eq!(s.snapshot(String::new()).xp.earned, 100);
        s.apply(&account_xp(CURRENT_SEASON, 0, 0, 50_000_500));
        assert_eq!(s.snapshot(String::new()).xp.earned, 600);
    }

    #[test]
    fn a_drop_and_its_pickup_are_one_item() {
        // The server rolls the item (tier included, hash "abc"), then the same
        // hash turns up in the bag with no tier of its own. The name is one the
        // tables call Satanic, because they are what decides the grade now — a
        // packet claiming 6 over a name the tables call Heroic would be counted
        // as the Heroic it is.
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        let sighting = |ground: bool, tier: i64| GameEvent::ItemAdded {
            rarity: json!(6),
            unscaled: false,
            mf: false,
            tier,
            item_type: 8,
            item_id: 1,
            weapon_type: 0,
            seed: 123,
            name: "Abomination's Gut Ripper".into(),
            announced: false,
            amount: 1,
            fingerprint: "8-1-1".into(),
            hash: "abc".into(),
            ground,
        };
        let dropped = s.apply(&sighting(true, 5)).expect("the roll is announced");
        assert_eq!(dropped.tier, 5);
        assert!(s.apply(&sighting(true, 5)).is_none(), "a world sync repeats the roll");
        assert!(s.apply(&sighting(false, 0)).is_none(), "no second alert for the pickup");
        assert_eq!(s.snapshot(String::new()).items["Satanic"].total, 1, "counted once");
    }

    /// The same pair without the hash. Both sightings carry the inventory
    /// fingerprint — the generation answer keys the item by it and the bag
    /// reports the same string — and keying the roll by seed instead put the
    /// two in spaces that could never meet, so one find was counted twice.
    #[test]
    fn a_hashless_drop_and_its_pickup_are_one_item_too() {
        let mut s = GameStats::default();
        let sighting = |ground: bool| GameEvent::ItemAdded {
            rarity: json!(3),
            unscaled: false,
            mf: false,
            tier: 0,
            item_type: 8,
            item_id: 2,
            weapon_type: 0,
            seed: 55,
            name: "Azazel's Despair".into(),
            announced: false,
            amount: 1,
            fingerprint: "7-4964607-65875ac569ff60006-8".into(),
            hash: String::new(),
            ground,
        };
        s.apply(&sighting(true));
        s.apply(&sighting(false));
        assert_eq!(s.snapshot(String::new()).items["Heroic"].total, 1, "one item, one count");
    }

    #[test]
    fn the_pickup_inherits_the_tier_the_roll_reported() {
        let mut s = GameStats::default();
        s.set_prefer_ground(false);
        s.set_filter(vec!["Satanic".into()], 5);
        let sighting = |ground: bool, tier: i64| GameEvent::ItemAdded {
            rarity: json!(6),
            unscaled: false,
            mf: false,
            tier,
            item_type: 8,
            item_id: 1,
            weapon_type: 0,
            seed: 7,
            name: "Abomination's Gut Ripper".into(),
            announced: false,
            amount: 1,
            fingerprint: "8-1-2".into(),
            hash: "def".into(),
            ground,
        };
        assert!(s.apply(&sighting(true, 6)).is_none(), "alerts are set to pickup time");
        let picked = s.apply(&sighting(false, 0)).expect("the pickup alerts");
        assert_eq!(picked.tier, 6, "the tier came from the roll");
    }

    #[test]
    fn pickup_alerts_when_ground_alerts_are_off() {
        let mut s = GameStats::default();
        s.set_prefer_ground(false);
        assert!(s.apply(&ground_item(json!(6), "Azazel's Despair", 55)).is_none());
        assert!(s.apply(&named_item(json!(6), false, "Azazel's Despair", "8-2-1")).is_some());
    }

    #[test]
    fn resynced_items_are_counted_once_and_named_rarity_wins() {
        let mut s = GameStats::default();
        // packet claims Rare, the wiki knows this name as Heroic
        s.apply(&named_item(json!(3), false, "Azazel's Despair", "8-1-1"));
        s.apply(&named_item(json!(3), false, "Azazel's Despair", "8-1-1"));
        let snap = s.snapshot(String::new());
        assert_eq!(snap.items["Heroic"].total, 1);
        assert_eq!(snap.items["Rare"].total, 0);
    }

    /// The graph series and the drop journal are the heaviest thing the app
    /// sends, and the client's heartbeat arrives every few seconds all run
    /// long. Neither carries anything either of them shows.
    #[test]
    fn the_heavy_payload_moves_only_when_it_has_something_new() {
        let mut s = GameStats::default();
        s.set_prefer_ground(false);
        let quiet = s.extra_revision();
        s.apply(&GameEvent::Room("Act_07_03".into()));
        s.apply(&GameEvent::Vitals { mf: Some(120), level: 0, hlevel: 0, satanic_here: Some(false) });
        s.apply(&GameEvent::Gold(Currency { gss: 500, ..Default::default() }));
        s.apply(&GameEvent::XpGain(15));
        assert_eq!(s.extra_revision(), quiet, "a heartbeat adds nothing to the journal");
        assert!(s.revision() > quiet, "the counters themselves did move");

        assert!(s.apply(&named_item(json!(6), false, "Azazel's Despair", "8-9-1")).is_some());
        assert!(s.extra_revision() > quiet, "a journalled drop is something new");
    }

    #[test]
    fn xp_gain_uses_original_factor() {
        let mut s = GameStats::default();
        s.apply(&GameEvent::XpGain(15));
        let snap = s.snapshot(String::new());
        assert_eq!(snap.xp.total, 100);
        assert_eq!(snap.xp.earned, 100);
    }

    /// A session that has been running for a while, without waiting for one.
    fn aged(secs: u64) -> GameStats {
        GameStats { start: Instant::now() - Duration::from_secs(secs), ..GameStats::default() }
    }

    /// You pause because you are going to town, and going to town is a
    /// sequence of room changes. The clock is stopped for all of it.
    #[test]
    fn a_paused_run_does_not_credit_a_room_with_the_pause() {
        // 1500 seconds on the wall, 900 of them paused: a 600-second run
        let mut s = aged(1500);
        s.apply(&account_packet("Test", 1_000, 10_000)); // the baseline
        s.apply(&account_packet("Test", 1_400, 60_000)); // +400 kills, +50k xp
        s.apply(&GameEvent::Room("Act_07_03".into()));

        s.set_paused(true);
        s.apply(&GameEvent::Room("Town_01".into()));
        assert!(s.room_since.is_none(), "a room change must not restart a stopped clock");
        assert_eq!(s.room.as_deref(), Some("Town_01"), "the panel still says where we are");

        // and a room clock left running by any other route banks nothing while
        // the session is held
        s.room_since = Some(Instant::now() - Duration::from_secs(900));
        s.paused_at = Some(Instant::now() - Duration::from_secs(900));
        s.apply(&GameEvent::Room("Act_07_03".into()));
        s.set_paused(false);

        let run = s.finish().expect("a session with earnings is worth keeping");
        let banked: u64 = run.zones.iter().map(|(_, secs)| secs).sum();
        assert!(banked <= run.secs, "{banked} room-seconds inside a {}-second run", run.secs);
        assert!(
            !run.zones.iter().any(|(room, _)| room == "Town_01"),
            "the paused town trip is not where the run happened: {:?}",
            run.zones
        );
    }

    #[test]
    fn a_glance_at_the_app_is_not_a_run() {
        let mut s = GameStats::default();
        assert!(s.finish().is_none(), "nothing happened and no time passed");

        // long enough, but the game never reported anything
        let mut s = aged(900);
        assert!(s.finish().is_none(), "an idle session is not a run either");
    }

    #[test]
    fn a_finished_run_carries_the_session_and_where_it_happened() {
        let mut s = aged(600);
        s.apply(&account_packet("Test", 1_000, 10_000)); // the baseline
        s.apply(&account_packet("Test", 1_400, 60_000)); // +400 kills, +50k xp

        s.apply(&GameEvent::Room("Act_07_02".into()));
        s.room_since = Some(Instant::now() - Duration::from_secs(300));
        s.apply(&GameEvent::Room("Act_07_03".into()));
        s.room_since = Some(Instant::now() - Duration::from_secs(60));

        let run = s.finish().expect("a session with earnings is worth keeping");
        assert_eq!(run.kills, 400);
        assert_eq!(run.xp, 50_000);
        assert!(run.secs >= 600, "{}", run.secs);
        assert_eq!(run.character.as_deref(), Some("Test"));
        // the room it spent longest in comes first
        assert_eq!(run.zones.first().map(|(room, _)| room.as_str()), Some("Act_07_02"));
        assert!(run.zones[0].1 >= 300, "{:?}", run.zones);
    }

    #[test]
    fn the_key_counter_ignores_the_ones_that_rain_down() {
        let mut s = GameStats::default();
        s.apply(&notable_item("Basic Key", 12, 3));
        s.apply(&notable_item("Crystal Key", 12, 2));
        s.apply(&notable_item("Angelic Key", 12, 1));
        assert_eq!(s.snapshot(String::new()).resources["keys"], 1);
    }

    #[test]
    fn season_mode_selection() {
        let mode = |season, hardcore, blood_pact| {
            let mut s = GameStats::default();
            s.apply(&account(season, hardcore, blood_pact));
            s.season_mode.unwrap()
        };
        assert_eq!(mode(CURRENT_SEASON, 0, 0), "GSS");
        assert_eq!(mode(CURRENT_SEASON, 1, 0), "GSH");
        assert_eq!(mode(0, 0, 1), "GBP");
        assert_eq!(mode(0, 1, 0), "GNH");
        assert_eq!(mode(0, 0, 0), "GNS");
        // a season the tracker has never heard of is still a season: the purse
        // is the seasonal one, not the non-seasonal leftovers
        assert_eq!(mode(CURRENT_SEASON + 3, 0, 0), "GSS");
    }

    #[test]
    fn five_quiet_minutes_stop_the_clock() {
        let mut s = GameStats::default();
        s.watch_idle();
        assert!(!s.paused(), "a run that has just started is not idle");

        s.last_progress = Instant::now() - IDLE_AFTER - Duration::from_secs(1);
        s.watch_idle();
        assert!(s.paused(), "quiet for longer than the limit");

        // Anything the run does lifts it, without being asked to.
        s.progressed();
        assert!(!s.paused(), "a kill, a drop or a coin is a sign of life");

        // A pause the player asked for is a different thing and outranks it.
        s.set_paused(true);
        s.progressed();
        assert!(s.paused(), "a hand-made pause lasts until the same hand lifts it");
        s.set_paused(false);
        assert!(!s.paused());
    }

    #[test]
    fn a_pickup_is_heard_when_the_roll_that_preceded_it_was_not() {
        // One sighting of an item, either half of the pair, with a shared
        // fingerprint so both are the same item.
        fn sighting(ground: bool, tier: i64, print: &str) -> GameEvent {
            GameEvent::ItemAdded {
                rarity: json!(6),
                unscaled: false,
                mf: false,
                tier,
                item_type: 0,
                item_id: 0,
                weapon_type: 0,
                seed: 0,
                // no name: nothing for the item table to grade it by, which is
                // the case a minimum grade actually bites on
                name: String::new(),
                announced: false,
                amount: 1,
                fingerprint: print.into(),
                hash: String::new(),
                ground,
            }
        }

        // The default, and the case that was broken: preferring the drop moment
        // used to mean refusing the bag outright, so an item whose roll this app
        // never saw — or whose roll had no grade to pass the minimum — was never
        // announced at all.
        let mut s = GameStats::default();
        assert!(s.prefs.prefer_ground);
        s.prefs.min_tier = 3;

        assert!(s.apply(&sighting(true, 0, "a")).is_none(), "no grade on the floor, no alert");
        assert!(
            s.apply(&sighting(false, 5, "a")).is_some(),
            "the bag proves the grade, and the roll never reached `told`"
        );

        // Most pickups have no roll this app saw at all. They are announced.
        let mut t = GameStats::default();
        assert!(t.apply(&sighting(false, 5, "b")).is_some());

        // And never twice: announced as it lands, quiet as it is taken.
        let mut u = GameStats::default();
        assert!(u.apply(&sighting(true, 5, "c")).is_some(), "announced as it lands");
        assert!(u.apply(&sighting(false, 5, "c")).is_none(), "and not again in the bag");

        // Asking for the bag alone still means the bag alone.
        let mut v = GameStats::default();
        v.set_prefer_ground(false);
        assert!(v.apply(&sighting(true, 5, "d")).is_none(), "the floor is not what was asked for");
        assert!(v.apply(&sighting(false, 5, "d")).is_some());
    }

    #[test]
    fn another_characters_lifetime_is_not_this_sessions_earnings() {
        fn save(name: &str, xp: i64, kills: i64, chests: i64) -> GameEvent {
            let mut tallies = HashMap::new();
            tallies.insert("statisticcommonchestsopened".to_string(), chests);
            GameEvent::Account {
                experience: xp,
                act: 0,
                has_experience: true,
                season: CURRENT_SEASON,
                hardcore: 0,
                blood_pact: 0,
                name: name.into(),
                level: 100,
                herolevel: 100,
                difficulty: 2,
                hell_sub: 0,
                kills,
                tallies,
            }
        }

        let mut s = GameStats::default();
        s.apply(&save("Main", 1_000_000, 900_000, 5_000)); // the first save only marks
        s.apply(&save("Main", 1_000_500, 900_010, 5_001));
        // Character select, an alt, and back. The alt's totals are its own life.
        s.apply(&save("Alt", 40, 3, 1));
        s.apply(&save("Alt", 90, 5, 1));
        s.apply(&save("Main", 1_001_000, 900_020, 5_002));

        let snap = s.snapshot(String::new());
        assert_eq!(snap.xp.earned, 550, "500 on the main, 50 on the alt, and nothing between them");
        assert_eq!(snap.kills.earned, 12, "10 + 2, not nine hundred thousand");
        let chests = snap.tallies.iter().find(|t| t.label == "Common").map(|t| t.total);
        assert_eq!(chests, Some(1), "one chest on the main; the alt opened none after its mark");
    }

    #[test]
    fn climbing_back_to_a_balance_the_bank_has_held_is_not_income() {
        let balance = |gns: i64| GameEvent::Gold(crate::parser::Currency {
            gss: 0, gsh: 0, gns, gnh: 0, gbp: 0, delta: 0,
        });
        let mut s = GameStats::default();
        s.apply(&account(0, 0, 0));
        s.apply(&balance(78_101)); // the run starts on the main's purse

        // A visit to another character. Its purse arrives in the very same
        // fields, and it holds a hundred coins.
        s.apply(&balance(107));
        // And back. This used to read as seventy-eight thousand earned.
        s.apply(&balance(78_101));
        assert_eq!(s.snapshot(String::new()).gold.earned, 0, "nothing was earned by walking there and back");

        // Past the high-water mark is earnings again.
        s.apply(&balance(80_101));
        assert_eq!(s.snapshot(String::new()).gold.earned, 2_000);
    }

    #[test]
    fn a_deposit_and_the_balance_that_answers_it_are_one_lot_of_coins() {
        // The client says it banked ten thousand and the server says the bank
        // now holds ten thousand more. Both orders, same answer.
        let deposit = |amount: i64| GameEvent::Gold(crate::parser::Currency {
            gss: 0, gsh: 0, gns: 0, gnh: 0, gbp: 0, delta: amount,
        });
        let balance = |gns: i64| GameEvent::Gold(crate::parser::Currency {
            gss: 0, gsh: 0, gns, gnh: 0, gbp: 0, delta: 0,
        });

        for order in ["deposit first", "balance first"] {
            let mut s = GameStats::default();
            s.apply(&account(0, 0, 0)); // non-seasonal: the GNS purse
            s.apply(&balance(100_000)); // the run starts with what is already there
            if order == "deposit first" {
                s.apply(&deposit(10_000));
                s.apply(&balance(110_000));
            } else {
                s.apply(&balance(110_000));
                s.apply(&deposit(10_000));
            }
            assert_eq!(
                s.snapshot(String::new()).gold.earned,
                10_000,
                "{order}: the same ten thousand, counted once"
            );
        }
    }

    #[test]
    fn a_reset_clears_the_session_but_keeps_the_character() {
        let mut s = GameStats::default();
        s.apply(&account(CURRENT_SEASON, 1, 0));
        s.apply(&GameEvent::XpGain(15));
        s.reset();
        let snap = s.snapshot(String::new());
        assert_eq!(snap.xp.earned, 0);
        assert_eq!(snap.character.as_ref().unwrap().name, "Test");
    }

    #[test]
    fn only_a_real_rotation_announces_the_satanic_zone() {
        let mut s = GameStats::default();
        // The zone is not on the heartbeat: it is a reply the server sends when
        // the client asks, which it does on an area load. So this is a session's
        // whole diet of zone packets — a handful, minutes apart, and the same
        // zone repeated whenever the player reloads without a rotation between.
        s.apply(&satanic_zone("Act_08_02"));
        assert!(s.take_zone_change().is_none(), "learning where the zone is is not it moving");
        s.apply(&satanic_zone("Act_08_02"));
        assert!(s.take_zone_change().is_none(), "the same zone said twice is one zone");

        s.apply(&satanic_zone("Act_03_01"));
        let moved = s.take_zone_change().expect("the zone moved");
        assert_eq!(moved.zone, "Act_03_01", "and it says which zone it moved to");
        assert!(s.take_zone_change().is_none(), "announced once, not until the next look");
    }

    #[test]
    fn a_blackout_swallows_one_packet_and_a_plain_reset_does_not() {
        let mut s = GameStats::default();
        s.apply(&satanic_zone("Act_03_01"));
        s.take_zone_change();

        // The game closing and opening again. The zone travels across it, the
        // game has been shut for an hour, and where the rotation has got to in
        // the meantime is news to this app rather than news to tell.
        s.reset_after_blackout();
        s.apply(&satanic_zone("Act_11_01"));
        assert!(s.take_zone_change().is_none(), "a blackout must not be reported as a rotation");
        s.apply(&satanic_zone("Act_02_04"));
        assert!(s.take_zone_change().is_some(), "the packet after it re-arms the guard");

        // The Reset button, pressed mid-farm. Nothing went dark: the game has
        // been up all along and the zone is exactly where we last saw it. This
        // used to arm the same guard, and because a zone packet arrives only
        // every few minutes, the packet it swallowed was often the rotation the
        // player pressed Reset to start counting.
        s.reset();
        s.apply(&satanic_zone("Act_05_05"));
        assert!(s.take_zone_change().is_some(), "a session reset must not swallow a rotation");
    }

    #[test]
    fn another_regions_answer_is_not_a_rotation() {
        let mut s = GameStats::default();
        s.apply(&GameEvent::ZoneRegion("8909978777".into()));
        s.apply(&rolled_zone("Act_04_05", vec![15, 23, 9]));
        s.take_zone_change();

        // The same account, asking on another region's behalf. The zone it
        // gets back is a different question's answer: it is where the player
        // now is, and it is not something that rotated. Seven of the twenty-one
        // zone changes in the capture on disk are this.
        s.apply(&GameEvent::ZoneRegion("2029974116".into()));
        s.apply(&rolled_zone("Act_04_05", vec![2, 5, 7]));
        assert!(s.take_zone_change().is_none(), "another region answering is not news");

        s.apply(&GameEvent::ZoneRegion("8917481016".into()));
        s.apply(&rolled_zone("Act_02_01", vec![1]));
        assert!(s.take_zone_change().is_none(), "nor is the next one, on a third region");

        // and once it settles, that region's own rotation still lands
        s.apply(&GameEvent::ZoneRegion("8917481016".into()));
        s.apply(&rolled_zone("Act_07_02", vec![1]));
        assert!(s.take_zone_change().is_some(), "the region that asked before is the one that moved");
    }

    #[test]
    fn a_capture_with_no_question_in_it_behaves_as_it_always_did() {
        // Nothing in the older captures carries the request, and a rule that
        // needs one would report no rotation at all for them.
        let mut s = GameStats::default();
        s.apply(&satanic_zone("Act_01_01"));
        s.take_zone_change();
        s.apply(&satanic_zone("Act_02_02"));
        assert!(s.take_zone_change().is_some(), "both regions unknown compares equal");
    }

    #[test]
    fn a_reroll_onto_the_same_zone_is_still_a_rotation() {
        let mut s = GameStats::default();
        s.apply(&rolled_zone("Act_08_02", vec![1, 2, 3]));
        s.take_zone_change();

        s.apply(&rolled_zone("Act_08_02", vec![3, 2, 1]));
        assert!(s.take_zone_change().is_none(), "the same buffs in another order are the same roll");

        s.apply(&rolled_zone("Act_08_02", vec![1, 2, 14]));
        assert!(
            s.take_zone_change().is_some(),
            "the same room with a different set on it is a new rotation, and under a              filter that alerts on the buffs it is exactly the one worth hearing"
        );
    }

    /// One relic sighting, as the parser now hands it over: nameless, type 16,
    /// and carrying the packet rarity the capture actually shows.
    fn relic(id: i64, hash: &str, ground: bool) -> GameEvent {
        GameEvent::ItemAdded {
            // `d: 9` on the wire, which reads as "Heroic" in the rarity table.
            // The parser nulls it because no `c == 0` base can be Heroic — that
            // is what keeps 459 of the 1,652 relic sightings out of the Heroic
            // column and out of the chime.
            rarity: Value::Null,
            unscaled: false,
            mf: false,
            tier: 0,
            item_type: 16,
            item_id: id,
            weapon_type: 0,
            seed: 24533420,
            name: String::new(),
            announced: false,
            amount: 1,
            fingerprint: format!("99-4964607-{hash}-16"),
            hash: hash.into(),
            ground,
        }
    }

    /// A relic nobody ticked is silent, and one that was ticked chimes once.
    ///
    /// The silence half is the half that protects a shipping install: relics
    /// drop 43 times an hour in the owner's own capture — one every 83 seconds
    /// — so letting them off the floor must change nothing at all until a relic
    /// is picked. They reach no counter and no chime because their rarity comes
    /// out "Unknown", which is on none of the five alert lists.
    #[test]
    fn a_relic_is_silent_until_it_is_hunted() {
        let mut s = GameStats::default();
        assert!(s.prefs.relics.is_empty(), "nothing hunted out of the box");
        let quiet = s.apply(&relic(127, "8b5bdb8ad9be", true));
        assert!(
            quiet.is_none_or(|d| d.sound.is_none()),
            "an unticked relic passes in silence, however many of them fall"
        );
        // And moves nothing a player reads. This is the half that protects a
        // shipping install: 827 relic drops appeared out of nowhere the moment
        // the floor was opened to them, and if any of those had landed in a
        // rarity column or a grade column the panel would have started lying.
        // They cannot, because a relic resolves to no journal rarity and is not
        // gear — but "cannot" is what the `d == 9` reading of "Heroic" also
        // looked like, so it is asserted rather than argued.
        let snap = s.snapshot(String::new());
        for (rarity, count) in &snap.items {
            assert_eq!(count.total, 0, "{rarity} moved on a relic drop");
        }
        assert_eq!(snap.ss, 0, "and the SS figure a run is judged on did not move");
        assert!(s.graded.is_empty(), "nor any grade column behind it");
        assert!(s.extra().drops.is_empty(), "and nothing reached the drop feed");

        // Now hunt it. A fresh engine, because the first sighting is already
        // counted against that hash.
        let mut s = GameStats::default();
        s.prefs.relics = vec![127];
        let hit = s.apply(&relic(127, "8b5bdb8ad9be", true)).expect("a hunted relic is announced");
        assert_eq!(hit.sound.as_deref(), Some("relic"), "and it chimes on its own key");
        // The chime is not the whole alert: the drop feed is where a player
        // looks to see WHICH relic it was, and the entry carries no name, so
        // the windows read it off the identity. `item_name` is what they call.
        let journal = s.extra().drops;
        assert_eq!(journal.len(), 1, "and it reaches the drop feed");
        assert_eq!((journal[0].item_type, journal[0].item_id), (16, 127));
        assert_eq!(crate::items::item_name(16, 127, 0), Some("Jungle Vial"), "which is how it is named there");

        // The pickup that follows carries the same hash, so it is the same
        // item and must not chime again — the rule every other drop obeys.
        let picked = s.apply(&relic(127, "8b5bdb8ad9be", false));
        assert!(picked.is_none_or(|d| d.sound.is_none()), "one relic, one chime");

        // A different relic on the same list-less engine stays quiet: ticking
        // one is not ticking the type.
        let other = s.apply(&relic(134, "cc808046c7c7", true));
        assert!(other.is_none_or(|d| d.sound.is_none()), "relic 134 was not ticked");
    }

    /// The relic pick is the OPPOSITE way round to the zone-buff pick above it
    /// in the settings, and this is the test that says so.
    ///
    /// `zone_buffs` narrows an alert the game already makes, so an empty list
    /// lets every rotation through. `relics` IS the alert, so an empty list is
    /// silence. The two sit one section apart on the same panel; a reader who
    /// carries the first rule to the second gets it exactly backwards, which is
    /// why both screens say what their empty state means in words.
    #[test]
    fn an_empty_relic_pick_is_silence_where_an_empty_buff_pick_is_everything() {
        let mut s = GameStats::default();
        s.prefs.relics = Vec::new();
        for (i, id) in [0, 55, 127, 155].into_iter().enumerate() {
            let hash = format!("hash{i}");
            let d = s.apply(&relic(id, &hash, true));
            assert!(d.is_none_or(|d| d.sound.is_none()), "relic {id}: an empty pick hunts nothing");
        }

        s.prefs.relics = vec![0, 155];
        assert_eq!(
            s.apply(&relic(0, "edge-low", true)).and_then(|d| d.sound).as_deref(),
            Some("relic"),
            "id 0 is a real relic and must not be read as 'no relic'"
        );
        assert_eq!(
            s.apply(&relic(155, "edge-high", true)).and_then(|d| d.sound).as_deref(),
            Some("relic"),
            "155 is the last id the table holds"
        );
    }

    /// Ticking a relic must not change what a list already on disk matches.
    ///
    /// Three relic names belong to another item too — `Shrunken Head` to a
    /// Satanic charm, `Death's Scythe` to a Set polearm, `Satan's Horn` to a
    /// Common collectible — and the last shares the rarity as well, so no
    /// spelling could separate it. That is why relics are matched by identity
    /// and left nameless: a list holding "death's scythe" goes on meaning the
    /// polearm and nothing else.
    #[test]
    fn hunting_a_relic_does_not_touch_a_list_that_shares_its_name() {
        let mut s = GameStats::default();
        s.set_sound_lists(vec![("list-a".into(), vec!["death's scythe".into()])]);
        s.prefs.relics = vec![60]; // relic 60 IS "Death's Scythe"

        let d = s.apply(&relic(60, "relic60", true)).expect("the relic is hunted");
        assert_eq!(d.sound.as_deref(), Some("relic"), "by identity, not by the name it shares");

        // The Set polearm of that name, arriving named as it always has.
        let polearm = s
            .apply(&GameEvent::ItemAdded {
                rarity: json!(4),
                unscaled: false,
                mf: false,
                tier: 0,
                item_type: 3,
                item_id: 2,
                weapon_type: 6,
                seed: 7,
                name: "Death's Scythe".into(),
                announced: false,
                amount: 1,
                fingerprint: "3-7-2".into(),
                hash: "polearm".into(),
                ground: true,
            })
            .expect("still on the list it was always on");
        assert_eq!(polearm.sound.as_deref(), Some("list-a"), "the list is untouched");
    }

    #[test]
    fn the_buff_pick_narrows_the_alert_and_an_empty_pick_narrows_nothing() {
        let mut s = GameStats::default();
        s.apply(&rolled_zone("Act_01_01", vec![7]));
        s.take_zone_change();

        // Nothing picked: every rotation. This is the default, and it is what
        // an upgrade from a settings file with no list in it must keep doing.
        assert!(s.prefs.zone_buffs.is_empty());
        s.apply(&rolled_zone("Act_02_02", vec![7]));
        assert!(s.take_zone_change().is_some(), "an empty pick lets every rotation through");
        s.apply(&rolled_zone("Act_02_03", vec![]));
        assert!(s.take_zone_change().is_some(), "including one the game gave no buffs at all");

        s.prefs.zone_buffs = vec![14, 15, 16];
        s.apply(&rolled_zone("Act_03_01", vec![7, 21]));
        assert!(s.take_zone_change().is_none(), "none of the three: it passes in silence");
        s.apply(&rolled_zone("Act_03_02", vec![]));
        assert!(s.take_zone_change().is_none(), "and a zone with no buffs cannot match a pick");

        s.apply(&rolled_zone("Act_04_01", vec![7, 15]));
        let hit = s.take_zone_change().expect("one of the three is enough");
        assert_eq!(hit.buffs, vec![7, 15], "and the alert carries what the zone rolled");

        // A rotation ruled out is still a rotation dealt with: the flag goes
        // either way, so the next look does not announce it late.
        s.apply(&rolled_zone("Act_05_01", vec![21]));
        assert!(s.take_zone_change().is_none());
        assert!(s.take_zone_change().is_none(), "a filtered rotation is not left pending");
    }
}
