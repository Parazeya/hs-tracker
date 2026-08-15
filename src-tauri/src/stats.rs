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

// stack resources by item type
const RESOURCES: &[(i64, &str)] = &[(12, "keys"), (13, "collectibles"), (14, "materials"), (15, "socketables")];

/// Keys that drop by the handful and open nothing worth counting: they would
/// bury the Angelic and Satanic keys the counter exists for.
const DULL_KEYS: [&str; 2] = ["basic key", "crystal key"];

/// What the save counts besides kills, in the order it is shown: the bosses the
/// character has put down, then the chests it has opened. The game sends all 33
/// of its `statistic…` counters on every save, so a session's worth of each is
/// the difference between two saves — exactly how kills already work.
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
    ("statisticuberchaostowerkills", "Uber Chaos Tower", "boss"),
    ("statisticchaostowerfloorclears", "Chaos Tower floors", "boss"),
    ("statisticwormholeclears", "Wormholes", "boss"),
    ("statisticcommonchestsopened", "Common", "chest"),
    ("statisticrarechestopened", "Rare", "chest"),
    ("statisticcrystalchestopened", "Crystal", "chest"),
    ("statisticrubychestsopened", "Ruby", "chest"),
    ("statisticdungeonchestsopened", "Dungeon", "chest"),
];

#[derive(Clone, Serialize, Deserialize)]
pub struct TallyCount {
    pub label: String,
    /// "boss" or "chest" — which list it belongs under
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
        group("Satanic Key", &["Satanic Key"]),
        group("Satanic Dice", &["Satanic Dice"]),
        group("S runes", &["Qi", "Xo", "Sur", "Ber", "Jah", "Drax", "Zed"]),
        group("SS runes", &["Fawn", "Flo", "Nju", "Jol"]),
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

#[derive(Clone, Serialize)]
pub struct CharacterInfo {
    pub name: String,
    pub level: i64,
    pub herolevel: i64,
    pub difficulty: i64,
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
    /// the character's magic find as the client last reported it, and whether
    /// the room it is standing in is the satanic one — both straight from the
    /// heartbeat rather than worked out from zone codes
    mf: i64,
    satanic_here: bool,
    room: Option<String>,
    sz_changed: Option<Instant>,
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
    prefer_ground: bool,
    alerts: Vec<String>,
    min_tier: i64,
    /// What the flourish window answers to. It is asked here rather than after
    /// the fact because a drop that fails the alert rules never leaves this
    /// function — which made the flourish's own settings look like they did
    /// nothing at all.
    fx_rarities: Vec<String>,
    fx_tier: i64,
    notable_defs: Vec<(String, Vec<String>)>,
    /// (sound key, item names) — an item on one of these is announced by it
    sound_lists: Vec<(String, Vec<String>)>,
    notable: HashMap<String, i64>,
    seen_fingerprints: std::collections::HashSet<String>,
    /// tier by item hash, so the pickup of an item knows what the drop said
    tier_seen: HashMap<String, i64>,
    /// items already added to the counters, by identity
    counted: std::collections::HashSet<String>,
    announced_at: HashMap<String, Instant>,
    character: Option<CharacterInfo>,
    drops: VecDeque<DropEntry>,
    series: Vec<SeriesPoint>,
    /// bumped by every change, so the pusher can skip unchanged snapshots
    revision: u64,
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
            mf: 0,
            satanic_here: false,
            room: None,
            sz_changed: None,
            season_mode: None,
            gold_mode: None,
            last_currency: None,
            xp_authoritative: false,
            stale_bank: false,
            banked: 0,
            stale_save: false,
            last_save: None,
            last_bank: None,
            prefer_ground: true,
            alerts: JOURNAL_RARITIES.iter().map(|r| r.to_string()).collect(),
            min_tier: 0,
            fx_rarities: Vec::new(),
            fx_tier: 6,
            notable_defs: default_notable(),
            sound_lists: Vec::new(),
            notable: HashMap::new(),
            seen_fingerprints: std::collections::HashSet::new(),
            tier_seen: HashMap::new(),
            counted: std::collections::HashSet::new(),
            announced_at: HashMap::new(),
            character: None,
            drops: VecDeque::new(),
            series: Vec::new(),
            revision: 0,
        }
    }
}

impl GameStats {
    /// Character, zone and the diff baselines survive a session reset — only
    /// the earned counters restart, so the next packet still yields a diff.
    pub fn reset(&mut self) {
        let revision = self.revision;
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
            self.stale_save,
            self.prefer_ground,
            std::mem::take(&mut self.alerts),
            self.min_tier,
            std::mem::take(&mut self.notable_defs),
            std::mem::take(&mut self.sound_lists),
            // the marks the boss and chest counters are measured from: a reset
            // starts the tally again, it does not make the game recount
            std::mem::take(&mut self.tally_base),
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
            self.stale_save,
            self.prefer_ground,
            self.alerts,
            self.min_tier,
            self.notable_defs,
            self.sound_lists,
            self.tally_base,
        ) = carry;
        self.revision = revision + 1;
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

    /// Cheap enough to poll: the mail chime must fire even while every window
    /// that shows the counters is hidden.
    pub fn has_mail(&self) -> bool {
        self.has_mail
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

    /// Called on the watcher's beat. A run that has shown no sign of life for
    /// `after` is not a run in progress, so the clock stops — back to the moment
    /// it went quiet, not to now.
    pub fn watch_idle(&mut self, after: Option<Duration>) {
        let Some(after) = after else {
            // the setting went off; only a hand-made pause survives it
            if self.paused() && !self.by_hand {
                self.release();
            }
            return;
        };
        if self.paused() || self.last_progress.elapsed() < after {
            return;
        }
        let since = self.last_progress;
        self.hold(since, false);
    }

    /// The run moved. Anything that lifts an idle pause goes through here.
    fn progressed(&mut self) {
        self.last_progress = Instant::now();
        if self.paused() && !self.by_hand {
            self.release();
        }
    }

    /// How many items of one grade this session has produced.
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

    /// Whether notifications fire when an item hits the ground (true) or when
    /// it is picked up (false).
    pub fn set_prefer_ground(&mut self, prefer_ground: bool) {
        self.revision += 1;
        self.prefer_ground = prefer_ground;
    }

    /// Which drops are worth a sound and a ticker line. Counters ignore this —
    /// statistics should stay complete even when alerts are narrowed down.
    pub fn set_filter(&mut self, alerts: Vec<String>, min_tier: i64) {
        self.revision += 1;
        self.alerts = alerts;
        self.min_tier = min_tier;
    }

    /// The flourish has rules of its own, and they are not the alert rules.
    pub fn set_flourish_filter(&mut self, rarities: Vec<String>, tier: i64) {
        self.fx_rarities = rarities;
        self.fx_tier = tier;
    }

    /// Lists the user built by hand: their sound wins over the rarity alerts,
    /// and an item on one is announced even when the filter would hide it.
    pub fn set_sound_lists(&mut self, lists: Vec<(String, Vec<String>)>) {
        self.revision += 1;
        self.sound_lists = lists
            .into_iter()
            .map(|(key, names)| (key, names.into_iter().map(|n| n.trim().to_lowercase()).collect()))
            .collect();
    }

    fn listed_sound(&self, name: &str) -> Option<String> {
        if name.is_empty() {
            return None;
        }
        let lower = name.to_lowercase();
        self.sound_lists
            .iter()
            .find(|(_, names)| names.contains(&lower))
            .map(|(key, _)| key.clone())
    }

    pub fn set_notable(&mut self, defs: Vec<(String, Vec<String>)>) {
        self.revision += 1;
        if !defs.is_empty() {
            self.notable_defs = defs;
        }
    }

    fn count_notable(&mut self, name: &str, amount: i64) {
        if name.is_empty() {
            return;
        }
        // the game calls a rune "Ber"; everyone else says "Ber Rune"
        let lower = name.to_lowercase();
        let bare = lower.trim_end_matches(" rune").to_string();
        let label = self
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
        self.alerts.iter().any(|r| r == rarity) && tier >= self.min_tier
    }

    fn worth_a_flourish(&self, rarity: &str, tier: i64) -> bool {
        self.fx_rarities.iter().any(|r| r == rarity) && tier >= self.fx_tier
    }

    /// Returns the journal entry when this event produced a new tracked drop.
    pub fn apply(&mut self, event: &GameEvent) -> Option<DropEntry> {
        self.revision += 1;
        match event {
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
                name,
                level,
                herolevel,
                difficulty,
                kills,
                tallies,
            } => {
                if *has_experience {
                    self.last_save = Some(Instant::now());
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
                    self.character = Some(CharacterInfo {
                        name: name.clone(),
                        level: *level,
                        herolevel: *herolevel,
                        difficulty: *difficulty,
                        hardcore: *hardcore == 1,
                        season: *season,
                    });
                }
                // currency usually arrives before the mode is known
                if let Some(c) = self.last_currency.clone() {
                    self.apply_currency(&c);
                }
            }
            GameEvent::Mail(has) => self.has_mail = *has,
            GameEvent::Room(room) => {
                if self.room.as_deref() != Some(room.as_str()) {
                    // close the books on the room being left: a run is worth
                    // little without knowing where it happened
                    self.bank_room_time();
                    self.room = Some(room.clone());
                    self.room_since = Some(Instant::now());
                }
            }
            GameEvent::Vitals { mf, level, hlevel, satanic_here } => {
                if *mf != self.mf || *satanic_here != self.satanic_here {
                    self.revision += 1;
                }
                self.mf = *mf;
                self.satanic_here = *satanic_here;
                // The save carries these too, but it arrives when the game
                // decides to save; the heartbeat is a few seconds old at worst.
                // Only what the heartbeat actually reported is taken.
                if let Some(c) = self.character.as_mut() {
                    if *level > 0 && c.level != *level {
                        c.level = *level;
                        self.revision += 1;
                    }
                    if *hlevel > 0 && c.herolevel != *hlevel {
                        c.herolevel = *hlevel;
                        self.revision += 1;
                    }
                }
            }
            GameEvent::ItemAdded {
                rarity,
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
                let identity = if !hash.is_empty() {
                    format!("h:{hash}")
                } else if *ground {
                    format!("g:{seed}:{item_type}:{item_id}")
                } else {
                    fingerprint.clone()
                };
                if !identity.is_empty() {
                    // a world sync repeats the very same sighting; that is noise
                    let sighting = format!("{}{identity}", if *ground { "d:" } else { "p:" });
                    if !self.seen_fingerprints.insert(sighting) {
                        return None;
                    }
                    if self.seen_fingerprints.len() > 20_000 {
                        self.seen_fingerprints.clear();
                        self.counted.clear();
                    }
                }
                let first = identity.is_empty() || self.counted.insert(identity);
                // A named item always drops at its own grade, which the packet
                // never states — the wiki table does. Unnamed drops carry their
                // grade themselves, and their pickup inherits it.
                let mut tier = *tier;
                if tier == 0 && !name.is_empty() {
                    tier = crate::items::tier_by_name(name);
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
                let rarity_key = crate::parser::resolve_rarity(rarity, name);
                let is_resource = RESOURCES.iter().any(|(t, _)| t == item_type);
                // ground rolls are the drop moment, not an acquisition: they
                // drive the ticker and sounds, never the counters
                if !announced && first {
                    let n = (*amount).max(1);
                    // by grade, resources included: an Angelic Key is an SS drop
                    // like any other, whatever shelf it lands on
                    if tier > 0 {
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
                // when it lands in the bag, never both. The drop on the ground
                // carries no tier — only the pickup does — so a minimum tier
                // makes the alert wait for the pickup, which can prove it.
                // a list the user built outranks every switch below it
                let listed = self.listed_sound(name);
                let listed_hit = listed.is_some();
                let wanted = if *announced || listed.is_some() {
                    true
                } else if self.prefer_ground {
                    *ground
                } else {
                    !*ground
                };
                let announce = *announced
                    || listed_hit
                    || (!is_resource && self.passes_filter(&rarity_key, tier));
                let flourish = !is_resource && self.worth_a_flourish(&rarity_key, tier);
                if wanted && (announce || flourish) {
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
                    }
                    let sound = if echo {
                        None
                    } else {
                        listed.or_else(|| {
                            self.alerts.contains(&rarity_key).then(|| rarity_key.to_lowercase())
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
                    }
                    return Some(entry);
                }
            }
            GameEvent::SatanicZone { zone, buffs, debuffs } => {
                if self.satanic.as_ref().map(|s| &s.zone) != Some(zone) {
                    self.sz_changed = Some(Instant::now());
                }
                self.satanic = Some(SatanicZone {
                    zone: zone.clone(),
                    buffs: buffs.clone(),
                    debuffs: debuffs.clone(),
                });
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
        if c.delta > 0 {
            self.gold_earned += c.delta;
            self.progressed();
            self.banked += c.delta;
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
            self.gold_mode = Some(mode);
            self.stale_bank = false;
            self.banked = 0;
            return;
        }
        if self.total_gold != 0 && self.gold_mode == Some(mode) {
            let diff = current - self.total_gold;
            if diff > 0 {
                let already = self.banked.min(diff);
                self.banked -= already;
                if diff > already {
                    self.gold_earned += diff - already;
                    self.progressed();
                }
            }
        }
        self.total_gold = current;
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
                .notable_defs
                .iter()
                .map(|(label, _)| NotableCount {
                    label: label.clone(),
                    total: self.notable.get(label).copied().unwrap_or(0),
                })
                .collect(),
            items,
            satanic_zone: self.satanic.clone(),
            room: self.room.clone(),
            mf: self.mf,
            satanic_here: self.satanic_here,
            character: self.character.clone(),
            tallies: self.tallies(),
        }
    }

    pub fn extra(&self) -> Extra {
        Extra {
            character: self.character.clone(),
            series: self.series.clone(),
            drops: self.drops.iter().rev().cloned().collect(),
            sz_active_secs: self.sz_changed.map(|t| t.elapsed().as_secs()),
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
    /// where the character is standing, e.g. "Act_08_02"
    pub room: Option<String>,
    /// magic find, live off the heartbeat, and whether this room is the
    /// satanic zone — the game says so itself
    pub mf: i64,
    pub satanic_here: bool,
    pub character: Option<CharacterInfo>,
    /// bosses put down and chests opened this session
    pub tallies: Vec<TallyCount>,
}

#[derive(Serialize)]
pub struct Extra {
    pub character: Option<CharacterInfo>,
    pub series: Vec<SeriesPoint>,
    pub drops: Vec<DropEntry>,
    pub sz_active_secs: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{self, Currency, GameEvent};
    use serde_json::json;

    fn item(rarity: serde_json::Value, mf: bool) -> GameEvent {
        named_item(rarity, mf, "", "")
    }

    fn named_item(rarity: serde_json::Value, mf: bool, name: &str, fingerprint: &str) -> GameEvent {
        GameEvent::ItemAdded {
            rarity,
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
                    rarity, mf, tier, item_type, item_id, weapon_type, seed, name, announced, amount,
                    fingerprint, hash: String::new(), ground,
                }
            }
            other => other,
        }
    }

    fn notable_item(name: &str, item_type: i64, amount: i64) -> GameEvent {
        GameEvent::ItemAdded {
            rarity: json!(1),
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

    fn account_xp(season: i64, hardcore: i64, blood_pact: i64, experience: i64) -> GameEvent {
        GameEvent::Account {
            experience,
            has_experience: experience > 0,
            season,
            hardcore,
            blood_pact,
            name: "Test".into(),
            level: 10,
            herolevel: 20,
            difficulty: 2,
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
            has_experience: true,
            season: CURRENT_SEASON,
            hardcore: 0,
            blood_pact: 0,
            name: name.into(),
            level: 100,
            herolevel: 112,
            difficulty: 2,
            kills,
            tallies: HashMap::new(),
        }
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

    #[test]
    fn a_list_outranks_the_rarity_alerts() {
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        // nothing would normally be announced: no rarity is armed at all
        s.set_filter(vec![], 6);
        s.set_sound_lists(vec![("list-chase".into(), vec!["AK-47".into()])]);
        let drop = |name: &str, hash: &str| GameEvent::ItemAdded {
            rarity: json!(2),
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
    fn a_quiet_run_stops_its_own_clock() {
        let mut s = GameStats::default();
        // the first save only calibrates; nothing has been earned yet
        s.apply(&account_packet("x", 5, 500));

        // no patience at all, so the next beat finds the run standing still
        s.watch_idle(Some(Duration::ZERO));
        assert!(s.paused(), "a run with nothing happening stops counting");

        s.apply(&account_packet("x", 9, 900));
        assert!(!s.paused(), "and the next sign of life starts it again");

        // a pause the player asked for is theirs alone to lift
        s.set_paused(true);
        s.apply(&account_packet("x", 20, 2000));
        assert!(s.paused(), "activity does not undo a hand-made pause");
        s.watch_idle(None);
        assert!(s.paused(), "nor does switching the idle watch off");
        s.set_paused(false);
        assert!(!s.paused());
    }

    #[test]
    fn bosses_and_chests_count_from_the_first_save_on() {
        let save = |satan: i64, odin: i64, ruby: i64| match account_packet("x", 1, 1) {
            GameEvent::Account { experience, season, hardcore, blood_pact, name, level, herolevel, difficulty, kills, .. } => {
                GameEvent::Account {
                    experience, has_experience: true, season, hardcore, blood_pact, name, level,
                    herolevel, difficulty, kills,
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
        // a piece of gear that states SS, then an Angelic Key: a resource, and
        // graded SS by the table rather than by the packet
        s.apply(&tiered_satanic(6, "a"));
        s.apply(&notable_item("Angelic Key", 12, 1));
        assert_eq!(s.graded(6), 2, "a key is a drop like any other");
        // grade B, and a name the table cannot grade at all
        s.apply(&tiered_satanic(3, "b"));
        s.apply(&notable_item("Mystery Blade", 3, 1));
        assert_eq!(s.graded(3), 1);
        assert_eq!(s.graded(6), 2, "an item the table cannot grade is not an SS");
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
        // the server rolls the item (tier included, hash "abc"), then the same
        // hash turns up in the bag with no tier of its own
        let mut s = GameStats::default();
        s.set_prefer_ground(true);
        let sighting = |ground: bool, tier: i64| GameEvent::ItemAdded {
            rarity: json!(6),
            mf: false,
            tier,
            item_type: 8,
            item_id: 1,
            weapon_type: 0,
            seed: 123,
            name: "Azazel's Despair".into(),
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

    #[test]
    fn the_pickup_inherits_the_tier_the_roll_reported() {
        let mut s = GameStats::default();
        s.set_prefer_ground(false);
        s.set_filter(vec!["Satanic".into()], 5);
        let sighting = |ground: bool, tier: i64| GameEvent::ItemAdded {
            rarity: json!(6),
            mf: false,
            tier,
            item_type: 8,
            item_id: 1,
            weapon_type: 0,
            seed: 7,
            name: "Azazel's Despair".into(),
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
    fn a_reset_clears_the_session_but_keeps_the_character() {
        let mut s = GameStats::default();
        s.apply(&account(CURRENT_SEASON, 1, 0));
        s.apply(&GameEvent::XpGain(15));
        s.reset();
        let snap = s.snapshot(String::new());
        assert_eq!(snap.xp.earned, 0);
        assert_eq!(snap.character.as_ref().unwrap().name, "Test");
    }
}
