use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct Currency {
    pub gss: i64,
    pub gsh: i64,
    pub gns: i64,
    pub gnh: i64,
    pub gbp: i64,
    /// gold gained reported directly by the packet, when there is no total
    pub delta: i64,
}

impl Currency {
    /// The one purse with anything in it, when exactly one has.
    ///
    /// Which purse a character banks into is stated by its save, and a save
    /// arrives when the game feels like saving — until then the balance cannot
    /// be read at all, and a player who has just started sees a bank of zero
    /// while the session counts up beside it. A packet with money in a single
    /// purse can only be that character's. Several, and there is nothing to go
    /// on, so it keeps waiting: showing the wrong purse is worse than showing
    /// none, and that is a mistake this app has made before.
    pub fn only_purse(&self) -> Option<&'static str> {
        let mut found = None;
        for (name, value) in
            [("GSS", self.gss), ("GSH", self.gsh), ("GNS", self.gns), ("GNH", self.gnh), ("GBP", self.gbp)]
        {
            if value > 0 {
                if found.is_some() {
                    return None;
                }
                found = Some(name);
            }
        }
        found
    }

    pub fn for_mode(&self, mode: &str) -> i64 {
        match mode {
            "GSS" => self.gss,
            "GSH" => self.gsh,
            "GNS" => self.gns,
            "GNH" => self.gnh,
            "GBP" => self.gbp,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    Gold(Currency),
    XpGain(i64),
    Account {
        experience: i64,
        has_experience: bool,
        season: i64,
        hardcore: i64,
        blood_pact: i64,
        name: String,
        level: i64,
        herolevel: i64,
        difficulty: i64,
        kills: i64,
        /// every `statistic…` counter the save carries, by flattened name —
        /// bosses put down, chests opened, floors cleared, deaths
        tallies: HashMap<String, i64>,
    },
    Mail(bool),
    /// the room the character stands in, straight from the client's heartbeat
    Room(String),
    /// what the same heartbeat says about the character: magic find, the two
    /// levels, and whether the room it is in is the satanic zone
    Vitals {
        mf: i64,
        level: i64,
        hlevel: i64,
        satanic_here: bool,
    },
    /// A find the server put in chat: "Ragnar just found [Azazel's Despair]".
    /// The line goes to everybody on the shard, so who found it matters — it
    /// is only ours when the name is ours. Answered in `GameStats`, which is
    /// the side that knows the character.
    Found {
        finder: String,
        name: String,
    },
    ItemAdded {
        rarity: Value,
        mf: bool,
        tier: i64,
        item_type: i64,
        item_id: i64,
        weapon_type: i64,
        seed: i64,
        name: String,
        announced: bool,
        amount: i64,
        fingerprint: String,
        /// the item's own hash: the same value at the drop and at the pickup,
        /// which is what ties the two sightings together
        hash: String,
        /// generated on the ground (the moment it drops), not picked up
        ground: bool,
    },
    SatanicZone {
        zone: String,
        buffs: Vec<u8>,
        debuffs: Vec<u8>,
    },
}

const BUF_CAP: usize = 1 << 20;
/// A carried tail is the truncated end of one message. Anything bigger is a
/// stray brace in framing noise — carrying that would stall capture forever.
const CARRY_CAP: usize = 8 << 10;
const CARRY_ROUNDS: u8 = 3;
const BUF_TTL: Duration = Duration::from_secs(15);
/// What we send is only flushed when the ack changes, and the ack only changes
/// when the server sends something back. Character saves — the one source of
/// kills and experience — would sit here until the next server burst, which is
/// why counters used to move only on a zone change. A quiet buffer is flushed
/// on its own.
const IDLE_FLUSH: Duration = Duration::from_millis(250);

struct Pending {
    data: Vec<u8>,
    at: Instant,
}

/// One side of one TCP connection: source address and both ports. The game
/// holds several connections to the same server at once — a busy one (the
/// world) and a quiet one (character saves). Keyed by address alone they share
/// a buffer, and during a fight the world traffic shreds the save that is being
/// assembled: exactly the case where counters used to stop moving.
pub type Flow = (IpAddr, u16, u16);

/// Payloads are buffered per flow and ack, and flushed when the ack from that
/// flow changes. A message that straddles two flushes would be lost, so the
/// unterminated tail is carried over to the next flush of the same flow.
#[derive(Default)]
pub struct Reassembler {
    bufs: HashMap<(Flow, u32), Pending>,
    last_ack: HashMap<Flow, u32>,
    carry: HashMap<Flow, (Vec<u8>, u8)>,
}

impl Reassembler {
    pub fn push(&mut self, flow: Flow, ack: u32, payload: &[u8]) -> Option<Vec<u8>> {
        if payload.is_empty() {
            return None;
        }
        self.evict_stale();
        let last = *self.last_ack.entry(flow).or_insert(ack);
        let buf = self.bufs.entry((flow, ack)).or_insert_with(|| Pending {
            data: Vec::new(),
            at: Instant::now(),
        });
        if buf.data.len() < BUF_CAP {
            buf.data.extend_from_slice(payload);
        }
        buf.at = Instant::now();
        if ack == last {
            return None;
        }
        self.last_ack.insert(flow, ack);
        let flushed = self.bufs.remove(&(flow, last))?;
        Some(self.finish(flow, flushed.data))
    }

    /// Buffers nobody has added to for a moment, so a stream that only talks
    /// one way still gets read.
    pub fn drain_idle(&mut self) -> Vec<(IpAddr, Vec<u8>)> {
        let now = Instant::now();
        let ripe: Vec<(Flow, u32)> = self
            .bufs
            .iter()
            .filter(|(_, b)| !b.data.is_empty() && now.duration_since(b.at) >= IDLE_FLUSH)
            .map(|(k, _)| *k)
            .collect();
        ripe.into_iter()
            .filter_map(|key| {
                let pending = self.bufs.remove(&key)?;
                Some((key.0 .0, self.finish(key.0, pending.data)))
            })
            .collect()
    }

    fn finish(&mut self, flow: Flow, flushed: Vec<u8>) -> Vec<u8> {
        // stitch the previous tail back on — unless it has been waiting for
        // its ending too long to be a real message
        let mut data = match self.carry.remove(&flow) {
            Some((tail, rounds)) if rounds < CARRY_ROUNDS => tail,
            _ => Vec::new(),
        };
        let rounds = if data.is_empty() { 0 } else { 1 };
        data.extend_from_slice(&flushed);

        // A truncated message is the last thing in the stream: nothing whole
        // follows it. If complete values do follow, the open bracket was just
        // a framing byte and holding it back would stall capture for good.
        let cut = unterminated_start(&data);
        let truncated = cut < data.len()
            && data.len() - cut <= CARRY_CAP
            && !has_complete_json(&data[cut + 1..]);
        if truncated {
            let tail = data.split_off(cut);
            self.carry.insert(flow, (tail, rounds + 1));
        }
        data
    }

    fn evict_stale(&mut self) {
        // A flow that flushes cleanly leaves no buffer behind, so its ack and
        // carry entries would otherwise outlive every sweep keyed on `bufs`.
        if self.bufs.len() <= 64 && self.last_ack.len() <= 512 && self.carry.len() <= 512 {
            return;
        }
        let now = Instant::now();
        self.bufs.retain(|_, b| now.duration_since(b.at) < BUF_TTL);
        // capturing every host this machine talks to means flows come and go;
        // their ack and carry entries go with them
        let live: std::collections::HashSet<Flow> = self.bufs.keys().map(|(flow, _)| *flow).collect();
        self.last_ack.retain(|flow, _| live.contains(flow));
        self.carry.retain(|flow, _| live.contains(flow));
        if self.bufs.len() > 512 {
            self.bufs.clear();
            self.last_ack.clear();
            self.carry.clear();
        }
    }
}

fn has_complete_json(buf: &[u8]) -> bool {
    let mut i = 0;
    while i < buf.len() {
        if (buf[i] == b'{' || buf[i] == b'[') && matching_json_end(buf, i).is_some() {
            return true;
        }
        i += 1;
    }
    false
}

/// Index where a JSON value starts that never closes in this buffer, so the
/// caller can keep it for the next chunk. `len()` when everything is complete.
fn unterminated_start(buf: &[u8]) -> usize {
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == b'{' || buf[i] == b'[' {
            match matching_json_end(buf, i) {
                Some(end) => i = end + 1,
                None => return i,
            }
        } else {
            i += 1;
        }
    }
    buf.len()
}

/// `totalGuildXp` and `total_guild_xp` are the same key: compare the
/// alphanumeric-lowercase forms without building them (this runs for every
/// key of every packet, several times per packet).
fn norm_eq(a: &str, b: &str) -> bool {
    let (mut ai, mut bi) = (
        a.bytes().filter(u8::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()),
        b.bytes().filter(u8::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()),
    );
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (x, y) if x == y => continue,
            _ => return false,
        }
    }
}

/// String values that look like JSON are re-parsed, as the original does.
fn coerce(v: &Value) -> Value {
    if let Value::String(s) = v {
        let t = s.trim();
        if t.starts_with('{') || t.starts_with('[') {
            if let Ok(parsed) = serde_json::from_str(t) {
                return parsed;
            }
        }
    }
    v.clone()
}

/// Borrowing lookup — no clone, no coercion. Use this unless the value has to
/// be re-parsed from a JSON string.
fn field_ref<'a>(obj: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let map = obj.as_object()?;
    for n in names {
        if let Some(v) = map.get(*n) {
            return Some(v);
        }
    }
    map.iter()
        .find(|(k, _)| names.iter().any(|n| norm_eq(n, k)))
        .map(|(_, v)| v)
}

/// Normalized field lookup; string values that hold JSON are re-parsed.
pub fn field(obj: &Value, names: &[&str]) -> Option<Value> {
    field_ref(obj, names).map(coerce)
}

fn has(obj: &Value, names: &[&str]) -> bool {
    field_ref(obj, names).is_some()
}

/// The protocol writes whole numbers as floats ("d": 5.0, "rs": 2032.0), so
/// every numeric read has to accept both spellings.
pub fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let t = s.trim();
            t.parse::<i64>().ok().or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
        }
        _ => None,
    }
}

fn int_field(obj: &Value, names: &[&str]) -> i64 {
    field_ref(obj, names).and_then(as_int).unwrap_or(0)
}

/// The counters the character save keeps beside experience and kills: bosses
/// killed, chests opened, floors cleared, deaths. The game names them all
/// `statistic…` and sends every one on every save. Which of them are worth
/// showing is not the parser's business, so it hands over the lot — flattened
/// to letters and digits, the way the rest of the field lookups are, so
/// `statisticUberDamienKills` and `statistic_uber_damien_kills` are one key.
fn tallies(obj: &Value) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    let Some(map) = obj.as_object() else { return out };
    for (key, value) in map {
        let flat: String =
            key.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
        if flat.len() > "statistic".len() && flat.starts_with("statistic") {
            if let Some(n) = as_int(value) {
                out.insert(flat, n);
            }
        }
    }
    out
}

fn msg_text(obj: &Value) -> String {
    match field_ref(obj, &["message"]) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

/// JSON is scanned over the WHOLE buffer, so framing bytes between or inside
/// messages cannot cut one in half. The line-oriented formats (base64 blob,
/// query string) are still read per printable segment, which is how they are
/// framed.
pub fn extract_messages(buf: &[u8]) -> Vec<Value> {
    let mut out = extract_json_values(buf);
    for seg in buf.split(|b| *b < 0x20 || *b == 0x7f) {
        // binary noise splits into thousands of short segments; decide on the
        // raw bytes, before paying for a String
        let blob = seg.len() > 100;
        let query = seg.contains(&b'=') && seg.contains(&b'&');
        if !blob && !query {
            continue;
        }
        let s = String::from_utf8_lossy(seg);
        if blob {
            out.extend(base64_payload(&s));
        }
        if query {
            out.extend(query_payload(&s));
        }
    }
    out
}

fn base64_payload(s: &str) -> Option<Value> {
    if s.len() <= 100 {
        return None;
    }
    if let Some(rest) = s.split_once("[INV]").map(|(_, r)| r) {
        return b64_json(rest);
    }
    if s.contains('&') {
        return None;
    }
    b64_json(s)
}

fn query_payload(s: &str) -> Option<Value> {
    if !s.contains('=') || !s.contains('&') {
        return None;
    }
    // start at the first key=, so protocol noise ahead of it is dropped
    let start = s.find(|c: char| c.is_ascii_alphanumeric() || c == '_')?;
    let map: serde_json::Map<String, Value> = form_urlencoded::parse(&s.as_bytes()[start..])
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k.into_owned(), parse_query_value(v.into_owned())))
        .collect();
    (!map.is_empty()).then_some(Value::Object(map))
}

fn parse_query_value(v: String) -> Value {
    let t = v.trim();
    if t.starts_with('{') || t.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str(t) {
            return parsed;
        }
    }
    Value::String(v)
}

/// Balanced-bracket scan: a flushed buffer often carries SEVERAL concatenated
/// JSON messages; a greedy first-to-last span drops all of them.
fn extract_json_values(bytes: &[u8]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'{' || c == b'[' {
            if let Some(end) = matching_json_end(bytes, i) {
                if let Ok(v) = serde_json::from_slice::<Value>(&bytes[i..=end]) {
                    let excluded = v.as_object().is_some_and(|o| {
                        o.contains_key("inventory_charms") || o.contains_key("steam")
                    });
                    if !excluded {
                        out.push(v);
                    }
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn matching_json_end(b: &[u8], start: usize) -> Option<usize> {
    let mut stack = vec![b[start]];
    let mut in_str = false;
    let mut esc = false;
    for (i, &c) in b.iter().enumerate().skip(start + 1) {
        if esc {
            esc = false;
            continue;
        }
        match c {
            b'\\' => esc = true,
            b'"' => in_str = !in_str,
            b'{' | b'[' if !in_str => stack.push(c),
            b'}' | b']' if !in_str => {
                let open = *stack.last()?;
                if (c == b'}') != (open == b'{') {
                    return None;
                }
                stack.pop();
                if stack.is_empty() {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn b64_json(s: &str) -> Option<Value> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.trim().as_bytes())
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn events_from_messages(messages: &[Value]) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for m in messages {
        walk_dicts(m, &mut events);
    }
    events
}

fn walk_dicts(v: &Value, events: &mut Vec<GameEvent>) {
    match v {
        Value::Object(_) => events.extend(dict_to_events(v)),
        Value::Array(items) => {
            for item in items {
                walk_dicts(item, events);
            }
        }
        _ => {}
    }
}

const GOLD_FIELDS: &[&str] = &["currencyData", "currency_data"];
const XP_TOTAL_FIELDS: &[&str] = &["totalGuildXp", "total_guild_xp", "totalGuildExp", "total_guild_exp"];
const XP_GAIN_FIELDS: &[&str] = &["xp", "experienceGained", "experience_gained"];
const MAIL_FIELDS: &[&str] = &["newMail", "new_mail", "mail"];
const ITEM_WRAPPER_FIELDS: &[&str] = &["addedItemObject", "added_item_object"];
const ITEM_SIGNATURE_FIELDS: &[&str] = &["seed", "a", "itemId", "item_id", "gid"];
const ITEM_NAMED_SIGNATURE_FIELDS: &[&str] = &["seed", "itemId", "item_id", "gid"];
const ITEM_RARITY_FIELDS: &[&str] = &["rarity", "itemRarity", "item_rarity", "d"];
const SATANIC_ZONE_FIELDS: &[&str] = &["satanicZoneName", "satanic_zone_name"];
const ACCOUNT_SIGNATURE_FIELDS: &[&str] =
    &["name", "class", "class_id", "heroLevel", "herolevel", "season", "hardcore"];

/// One packet can carry several things at once (currency + items + zone), so
/// every matching rule contributes; matching only the first loses events.
fn dict_to_events(d: &Value) -> Vec<GameEvent> {
    if d.as_object().is_none_or(|o| o.contains_key("steam")) {
        return vec![];
    }
    let mut events = Vec::new();
    let message = msg_text(d).to_lowercase();

    let wrapped_currency = field(d, GOLD_FIELDS);
    let gold_delta = int_field(d, &["goldAmount", "gold_amount", "amount_gold"]);
    if wrapped_currency.is_some() || has_currency_totals(d) || gold_delta > 0 {
        let c = wrapped_currency.unwrap_or_else(|| d.clone());
        events.push(GameEvent::Gold(Currency {
            gss: int_field(&c, &["GSS", "gss"]),
            gsh: int_field(&c, &["GSH", "gsh"]),
            gns: int_field(&c, &["GNS", "gns"]),
            gnh: int_field(&c, &["GNH", "gnh"]),
            gbp: int_field(&c, &["GBP", "gbp"]),
            delta: gold_delta.max(0),
        }));
    }
    if has(d, XP_TOTAL_FIELDS) {
        events.push(GameEvent::XpGain(xp_gain(d)));
    }
    // The client's heartbeat, base64'd: where the character stands and how it
    // stands there. It arrives every few seconds, which is what makes it worth
    // reading — the character save, where most of these numbers also live,
    // arrives when the game feels like saving.
    if let Some(Value::String(blob)) = field(d, &["game_state", "gameState"]) {
        if let Some(state) = b64_json(&blob) {
            if let Some(room) = field_ref(&state, &["room"]).and_then(|v| v.as_str()) {
                if !room.is_empty() {
                    events.push(GameEvent::Room(room.to_string()));
                }
            }
            let mf = int_field(&state, &["mf"]);
            let level = int_field(&state, &["level"]);
            let hlevel = int_field(&state, &["hlevel", "heroLevel", "herolevel"]);
            if mf > 0 || level > 0 || hlevel > 0 {
                events.push(GameEvent::Vitals {
                    mf,
                    level,
                    hlevel,
                    // the game says outright whether this room is the satanic
                    // one; comparing zone codes was always a guess at it
                    satanic_here: int_field(&state, &["sz"]) == 1,
                });
            }
        }
    }
    if message.contains("mail") || has(d, MAIL_FIELDS) {
        events.push(GameEvent::Mail(mail_is_present(d)));
    }
    // server chat announcement: "Someone just found [Item Name]"
    if let Some((finder, name)) = announced_item_name(&msg_text(d)) {
        events.push(GameEvent::Found { finder, name });
    }
    events.extend(item_events(d));
    if has(d, SATANIC_ZONE_FIELDS) {
        events.push(satanic_event(d));
    }

    let full_account = has(d, &["experience"]) && has(d, ACCOUNT_SIGNATURE_FIELDS);
    // login identity payload: no experience/talents, but carries name, uid,
    // cross-region id, season and hardcore (and is not a nearby-player list)
    let identity_account = !full_account
        && has(d, &["name"])
        && has(d, &["accountUID", "accountUid", "unique_id", "uniqueId"])
        && has(d, &["cross_region_identifier", "crossRegionIdentifier", "cross_region_id", "crossRegionId"])
        && !has(d, &["platformUserName", "platform_user_name", "nameColor", "name_color", "slot"]);
    if (full_account || identity_account) && has(d, &["season"]) && has(d, &["hardcore"]) {
        let name = match field(d, &["name"]) {
            Some(Value::String(s)) => s,
            _ => String::new(),
        };
        events.push(GameEvent::Account {
            experience: int_field(d, &["experience"]),
            has_experience: has(d, &["experience"]),
            season: int_field(d, &["season"]),
            hardcore: int_field(d, &["hardcore"]),
            blood_pact: int_field(d, &["blood_pact", "bloodPact"]),
            name,
            level: int_field(d, &["level"]),
            herolevel: int_field(d, &["heroLevel", "herolevel"]),
            difficulty: int_field(d, &["difficulty"]),
            kills: int_field(
                d,
                &[
                    "statisticTotalMonsterKills",
                    "statistic_total_monster_kills",
                    "totalMonsterKills",
                    "total_monster_kills",
                ],
            ),
            tallies: tallies(d),
        });
    } else if !full_account && !identity_account && has(d, XP_GAIN_FIELDS) && !has(d, XP_TOTAL_FIELDS) {
        events.push(GameEvent::XpGain(xp_gain(d)));
    }
    events
}

fn has_currency_totals(d: &Value) -> bool {
    ["GSS", "GSH", "GNS", "GNH", "GBP"].iter().any(|f| has(d, &[f]))
}

/// XP gain is the first number in the message text, else the xp field.
fn xp_gain(d: &Value) -> i64 {
    let msg = msg_text(d);
    let digits: String = msg
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if let Ok(n) = digits.parse() {
        return n;
    }
    int_field(d, XP_GAIN_FIELDS)
}

const ITEM_DATA_FIELDS: &[&str] = &["itemData", "item_data"];
const PICKUP_FIELDS: &[&str] = &["pickup_add_data", "pickupAddData"];

fn is_item_like(v: &Value) -> bool {
    v.is_object() && (has(v, ITEM_SIGNATURE_FIELDS) || has(v, ITEM_RARITY_FIELDS))
}

/// itemData without a pickup/inventory route is a world-sync snapshot, not a
/// pickup — counting it would inflate everything.
fn is_inventory_item_data(d: &Value, item_data: &Value) -> bool {
    if has(item_data, PICKUP_FIELDS) {
        return true;
    }
    let route = match field(d, &["route", "__route"]) {
        Some(Value::String(s)) => s,
        _ => String::new(),
    };
    let ctx = format!("{route} {}", msg_text(d)).to_lowercase();
    ctx.contains("inventory") || ctx.contains("pickup")
}

fn object_items(v: &Value) -> Vec<(Option<String>, Value)> {
    match v {
        Value::Object(map) => map
            .iter()
            .filter(|(_, v)| v.is_object())
            .map(|(fp, item)| (Some(fp.clone()), item.clone()))
            .collect(),
        _ => vec![],
    }
}

/// Every shape a pickup can arrive in, in the order the reference client
/// checks. The bool marks a GROUND drop (generated near the player) as opposed
/// to an inventory addition (the pickup itself).
fn item_sources(d: &Value) -> Vec<(Option<String>, Value, bool)> {
    let own_fp = match field(d, &["addedItemFingerprint", "added_item_fingerprint", "fingerprint"]) {
        Some(Value::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    };
    let ops = field(d, &["operations"]).unwrap_or(Value::Null);

    let pickups = |v: Vec<(Option<String>, Value)>| -> Vec<(Option<String>, Value, bool)> {
        v.into_iter().map(|(fp, item)| (fp, item, false)).collect()
    };

    if let Some(add) = field(&ops, &["add"]) {
        return pickups(object_items(&add));
    }
    // stacked pickups (keys, materials): { stack: { <fp>: { pickup_add_data: {...} } } }
    if let Some(Value::Object(stacked)) = field(&ops, &["stack"]) {
        return stacked
            .iter()
            .filter_map(|(fp, v)| field(v, PICKUP_FIELDS).map(|item| (Some(fp.clone()), item, false)))
            .collect();
    }
    if let Some(added) = field(d, &["itemsAdded", "items_added"]) {
        return pickups(object_items(&added));
    }
    if let Some(item_data) = field(d, ITEM_DATA_FIELDS) {
        if is_inventory_item_data(d, &item_data) {
            if let Some(pickup) = field(&item_data, PICKUP_FIELDS) {
                return vec![(own_fp, pickup, false)];
            }
            let nested: Vec<(Option<String>, Value)> = match &item_data {
                Value::Object(map) => map
                    .iter()
                    .filter_map(|(fp, v)| field(v, PICKUP_FIELDS).map(|item| (Some(fp.clone()), item)))
                    .collect(),
                _ => vec![],
            };
            if !nested.is_empty() {
                return pickups(nested);
            }
            if is_item_like(&item_data) {
                return vec![(own_fp, item_data, false)];
            }
            return pickups(object_items(&item_data));
        }
        // Unrouted itemData is the server answering "here is what dropped".
        // Only `c == 1` items are named ones: their ids come from the unique
        // item space (5, 8, 30, 55 …), while `c == 0` drops are ordinary bases
        // numbered 0..20 — reading those through the name table turns every
        // white sword into whatever unique happens to share the number.
        let candidates = if is_item_like(&item_data) {
            vec![(own_fp, item_data)]
        } else {
            object_items(&item_data)
        };
        return candidates
            .into_iter()
            .filter(|(_, item)| int_field(item, &["c"]) == 1)
            .map(|(fp, item)| (fp, item, true))
            .collect();
    }
    if let Some(wrapped) = field(d, ITEM_WRAPPER_FIELDS) {
        return vec![(own_fp, wrapped, false)];
    }
    // Bare item payload. The short format ("a"/"d"/"b") only ever arrives
    // inside a container keyed by fingerprint, so at top level we demand a
    // spelled-out identity field — single letters are common everywhere else.
    if has(d, ITEM_NAMED_SIGNATURE_FIELDS) && has(d, ITEM_RARITY_FIELDS) {
        return vec![(own_fp, d.clone(), false)];
    }
    vec![]
}

fn item_events(d: &Value) -> Vec<GameEvent> {
    item_sources(d)
        .into_iter()
        .filter(|(_, item, _)| item.is_object())
        .map(|(fp, item, ground)| item_event(&item, fp.as_deref(), ground))
        .collect()
}

/// The inventory fingerprint ends with the item TYPE ("8-4653008-...-1" -> 1);
/// in the short format `b` is then the id-in-category, not the type.
fn fingerprint_type(fingerprint: Option<&str>) -> Option<i64> {
    fingerprint?.rsplit('-').next()?.parse().ok()
}

/// Packet rarity is unreliable (inventory syncs report Common/Rare for
/// Satanic gear); the wiki-sourced rarity of the resolved NAME wins over it.
pub fn resolve_rarity(packet: &Value, name: &str) -> String {
    let mapped = crate::stats::rarity_from_packet(packet);
    let known = if name.is_empty() { None } else { crate::items::rarity_by_name(name) };
    let weak = matches!(mapped.as_deref(), None | Some("Common") | Some("Superior") | Some("Rare") | Some("Mythic"));
    if let (Some(k), true) = (known, weak) {
        return k.to_string();
    }
    if let Some(m) = mapped {
        if m != "Common" {
            return m;
        }
        if let Some(k) = known {
            return k.to_string();
        }
        return m;
    }
    known.unwrap_or("Unknown").to_string()
}

fn item_event(obj: &Value, fingerprint: Option<&str>, ground: bool) -> GameEvent {
    let fp_type = fingerprint_type(fingerprint);
    let short_id = int_field(obj, &["b"]);
    let explicit_type = int_field(obj, &["type", "itemType", "item_type"]);
    let item_type = if explicit_type != 0 {
        explicit_type
    } else {
        fp_type.unwrap_or(short_id)
    };
    let explicit_id = int_field(obj, &["id", "itemId", "item_id"]);
    let item_id = if explicit_id != 0 {
        explicit_id
    } else if fp_type.is_some() {
        short_id
    } else {
        int_field(obj, &["gid"])
    };
    let weapon_type = {
        let wt = int_field(obj, &["weapon_type", "weaponType"]);
        if wt != 0 {
            wt
        } else if item_type == 3 {
            int_field(obj, &["j"])
        } else {
            0
        }
    };
    let explicit_name = match field(obj, &["name", "itemName", "item_name", "label"]) {
        Some(Value::String(s)) => s.trim().to_string(),
        _ => String::new(),
    };
    // Odyssey keeps its own item space, and its packet says so: it carries an
    // `h` that no seasonal item sends, and an `e` of 0 where a seasonal item
    // carries the season it belongs to. Its `d` is not a rarity on the scale
    // the rest of the game uses — every Odyssey pickup arrives as 7, white
    // ones included, and 7 is Angelic here, so a practice run filled up with
    // Angelic finds. What the field does mean there is not known, so nothing
    // is claimed about it: the drop is still seen, it simply has no rarity.
    // A capture of 12 Odyssey and 38 seasonal pickups splits on `h` exactly.
    let odyssey = has(obj, &["h"]);
    let rarity = if odyssey {
        Value::Null
    } else {
        field(obj, ITEM_RARITY_FIELDS).unwrap_or(Value::Number(0.into()))
    };
    // A name read out of the tables is a guess about which item this is, and a
    // guess must not become evidence about what it is worth. `resolve_rarity`
    // trusts the name over a weak packet rarity, so an ordinary base whose
    // id-in-category lands on a unique's slot was handed that unique's name
    // and then promoted to its rarity — a white sword counted as Satanic, a
    // potion as Angelic.
    //
    // The drop path already refuses this: it keeps only `c == 1`, the game's
    // own flag for a named item, "while `c == 0` drops are ordinary bases
    // numbered 0..20". The pickup path never learnt the rule, and a pickup is
    // what the counters see. It cannot simply drop `c == 0` — an ordinary item
    // going into the bag is still an item — so it stays uncounted-by-name
    // instead: asked of the table only when the game has said this is a named
    // item, or when the rarity on the packet is already one worth naming.
    let named_flag = int_field(obj, &["c"]) == 1;
    let worth_naming = crate::stats::rarity_from_packet(&rarity)
        .is_some_and(|r| crate::stats::JOURNAL_RARITIES.contains(&r.as_str()));
    let name = if !explicit_name.is_empty() {
        explicit_name
    } else if named_flag || worth_naming {
        crate::items::item_name(item_type, item_id, weapon_type).unwrap_or_default().to_string()
    } else {
        String::new()
    };
    GameEvent::ItemAdded {
        rarity,
        mf: int_field(obj, &["mf_drop", "mfDrop", "m"]) == 1,
        tier: int_field(obj, &["tier", "n"]),
        item_type,
        item_id,
        weapon_type,
        seed: int_field(obj, &["seed", "a"]),
        name,
        announced: false,
        amount: int_field(obj, &["amount", "o"]).max(1),
        fingerprint: fingerprint.unwrap_or_default().to_string(),
        hash: match field(obj, &["sh"]) {
            Some(Value::String(h)) => h,
            _ => String::new(),
        },
        ground,
    }
}

/// Case-insensitive search that stays in the ORIGINAL string: lowercasing can
/// change byte lengths (İ -> i̇), and offsets taken from a lowered copy then
/// slice mid-character and panic.
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len())
        .find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n) && haystack.is_char_boundary(i))
}

/// The finder and what they found, out of "Ragnar just found [Azazel's Despair]".
/// The finder can be empty — some lines are worded without one — and an empty
/// finder is nobody, which is not us.
fn announced_item_name(message: &str) -> Option<(String, String)> {
    const MARKER: &str = "just found [";
    let at = find_ascii_ci(message, MARKER)?;
    let start = at + MARKER.len();
    let end = message[start..].find(']')? + start;
    let name = message[start..end].trim();
    // whatever the line opens with, up to the marker; the game puts a colour
    // tag or a channel prefix in front of the name often enough
    let finder = message[..at].trim().rsplit(&[':', '>', ']'][..]).next().unwrap_or("").trim();
    (!name.is_empty()).then(|| (finder.to_string(), name.to_string()))
}

/// "No new mail", "You have no new mail." and "Mailbox empty" all mean empty.
fn mail_is_present(d: &Value) -> bool {
    let raw = field(d, MAIL_FIELDS);
    match raw {
        Some(Value::Bool(b)) => return b,
        Some(Value::Number(n)) => return n.as_i64().unwrap_or(0) > 0,
        _ => {}
    }
    let text = match raw {
        Some(Value::String(s)) => s,
        _ => msg_text(d),
    };
    let t = text.trim().to_lowercase();
    if t.is_empty() || ["0", "false", "none", "no", "clear"].contains(&t.as_str()) {
        return false;
    }
    if t.contains("no new mail") || t.contains("no mail") || t.contains("mailbox empty") {
        return false;
    }
    t.contains("mail") || t == "1" || t == "true" || t == "yes"
}

fn effect_ids(raw: Option<Value>) -> Vec<u8> {
    match raw {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|b| match b {
                Value::Number(n) => n.as_i64(),
                Value::String(s) => s.trim().parse().ok(),
                _ => None,
            })
            .filter_map(|n| u8::try_from(n).ok())
            .collect(),
        Some(Value::String(s)) => s
            .replace(',', "|")
            .split('|')
            .filter_map(|b| b.trim().parse().ok())
            .collect(),
        _ => vec![],
    }
}

fn satanic_event(d: &Value) -> GameEvent {
    let zone = match field(d, SATANIC_ZONE_FIELDS) {
        Some(Value::String(s)) => s,
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let buffs = effect_ids(field(
        d,
        &["buffs", "satanicZoneBuffs", "satanic_zone_buffs", "zoneBuffs", "zone_buffs"],
    ));
    let debuffs = effect_ids(field(
        d,
        &["debuffs", "satanicZoneDebuffs", "satanic_zone_debuffs", "zoneDebuffs", "zone_debuffs"],
    ));
    GameEvent::SatanicZone { zone, buffs, debuffs }
}

#[cfg(test)]
mod tests {
    /// A real generation answer: the white sword rolled from base id 8 must not
    /// be read as the unique that happens to sit at id 8, or every junk drop
    /// would chime as Satanic.
    #[test]
    fn only_named_drops_come_out_of_a_generation_answer() {
        let msg = serde_json::json!({
            "itemData": {
                "3-4964607-65875f2ed96610001-3": {"a": 1, "b": 8, "c": 0, "d": 2, "e": 10, "j": 0, "n": 3, "sh": "aa"},
                "3-4964607-65875f2ed96610002-3": {"a": 2, "b": 30, "c": 1, "d": 2, "e": 10, "j": 0, "sh": "bb"}
            },
            "itemGenHash": "x", "message": "ok", "status": 1
        });
        let events = events_from_messages(&[msg]);
        let items: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                GameEvent::ItemAdded { hash, ground, .. } => Some((hash.clone(), *ground)),
                _ => None,
            })
            .collect();
        assert_eq!(items, vec![("bb".to_string(), true)], "only the named drop is reported");
    }

    use super::*;
    use serde_json::json;

    #[test]
    fn renamed_fields_are_recognized() {
        let cases = json!([
            {"currency_data": {}},
            {"total_guild_xp": 10},
            {"added_item_object": {"rarity": "Satanic", "item_id": 1}},
            {"satanic_zone_name": "SZ_1_1", "zone_buffs": [1]},
        ]);
        let events = events_from_messages(std::slice::from_ref(&cases));
        assert!(matches!(events[0], GameEvent::Gold(_)));
        assert!(matches!(events[1], GameEvent::XpGain(_)));
        assert!(matches!(events[2], GameEvent::ItemAdded { .. }));
        assert!(matches!(events[3], GameEvent::SatanicZone { .. }));
    }

    #[test]
    fn nested_payloads_are_flattened() {
        let payloads = vec![
            json!([
                {"currency_data": {"gss": 100, "gsh": 0, "gns": 0, "gnh": 0, "gbp": 0}},
                {"total_guild_xp": 500, "message": "Gained 15 XP"},
            ]),
            json!({"satanic_zone_name": "SZ_1_1", "zone_buffs": [1, 26]}),
        ];
        let events = events_from_messages(&payloads);
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], GameEvent::Gold(c) if c.gss == 100));
        assert!(matches!(events[1], GameEvent::XpGain(15)));
        assert!(
            matches!(&events[2], GameEvent::SatanicZone { zone, buffs, .. } if zone == "SZ_1_1" && buffs == &[1, 26])
        );
    }

    #[test]
    fn json_string_values_are_deserialized() {
        let payload = json!({"currency_data": "{\"gss\": 321, \"gsh\": 0, \"gns\": 0, \"gnh\": 0, \"gbp\": 0}"});
        let events = events_from_messages(std::slice::from_ref(&payload));
        assert!(matches!(&events[0], GameEvent::Gold(c) if c.gss == 321));
    }

    #[test]
    fn json_survives_framing_bytes_inside_the_buffer() {
        // a length prefix between two messages must not swallow either
        let raw = b"\x00\x1f{\"currency_data\":{\"GSS\":7}}\x00\x05{\"total_guild_xp\":3,\"message\":\"Gained 9 XP\"}";
        let events = events_from_messages(&extract_messages(raw));
        assert!(events.iter().any(|e| matches!(e, GameEvent::Gold(c) if c.gss == 7)));
        assert!(events.iter().any(|e| matches!(e, GameEvent::XpGain(9))));
    }

    #[test]
    fn capture_accepts_json_arrays_with_junk_around() {
        let raw = b"\x01prefix [{\"total_guild_xp\": 500, \"message\": \"Gained 15 XP\"}] suffix\x00";
        let messages = extract_messages(raw);
        assert_eq!(messages.len(), 1);
        let events = events_from_messages(&messages);
        assert!(matches!(events[0], GameEvent::XpGain(15)));
    }

    #[test]
    fn inventory_update_ext_short_fields() {
        let payload = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": {
                "add": {
                    "8-1": {"e": 10, "m": 1, "a": 676909917, "j": 0, "b": 71, "d": 6, "c": 1},
                    "8-6": {"e": 10, "a": 624778371, "j": 0, "b": 8, "d": 9, "c": 0},
                }
            }
        });
        let events = events_from_messages(std::slice::from_ref(&payload));
        assert_eq!(events.len(), 2);
        let parsed: Vec<(String, bool, i64, i64)> = events
            .iter()
            .map(|e| match e {
                GameEvent::ItemAdded { rarity, mf, item_type, item_id, .. } => {
                    (rarity.to_string(), *mf, *item_type, *item_id)
                }
                _ => panic!("not an item"),
            })
            .collect();
        // fingerprint suffix carries the item type; `b` is then the id-in-category
        assert!(parsed.contains(&("6".into(), true, 1, 71)));
        assert!(parsed.contains(&("9".into(), false, 6, 8)));
    }

    #[test]
    fn an_odyssey_pickup_claims_no_rarity() {
        // straight out of a capture: every pickup on an Odyssey character, all
        // of them ordinary, arrives with d = 7 — which on the seasonal scale
        // is Angelic, and filled the session with Angelic finds
        let odyssey = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": { "add": {
                "7-4964607-6591f6c6d88770001-12": {"a": 395097030, "b": 1, "c": 0, "d": 7, "e": 0, "h": 1, "j": 0, "sh": "98f379b4da5b"}
            }}
        });
        let events = events_from_messages(std::slice::from_ref(&odyssey));
        let GameEvent::ItemAdded { name, rarity, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(resolve_rarity(rarity, name), "Unknown", "its scale is not ours to read");

        // the seasonal shape of the same capture keeps working
        let seasonal = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": { "add": {
                "7-4964607-64f8884a6cfbb000b-10": {"a": 42, "b": 0, "c": 0, "d": 2, "e": 10, "j": 0, "n": 1, "sh": "ab"}
            }}
        });
        let events = events_from_messages(std::slice::from_ref(&seasonal));
        let GameEvent::ItemAdded { name, rarity, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(resolve_rarity(rarity, name), "Superior");
    }

    #[test]
    fn an_ordinary_pickup_is_not_given_a_uniques_name() {
        // `c: 0` and a low `b` is an ordinary base going into the bag. Slot
        // 18:8 belongs to an Angelic potion, and reading this through the name
        // table made every white potion an Angelic find.
        let payload = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": { "add": { "8-18": {"e": 10, "a": 42, "j": 0, "b": 8, "d": 2, "c": 0} } }
        });
        let events = events_from_messages(std::slice::from_ref(&payload));
        let GameEvent::ItemAdded { name, rarity, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(name, "", "an ordinary base is nameless; the table knows only uniques");
        assert_eq!(resolve_rarity(rarity, name), "Superior", "and it keeps the rarity it was sent with");

        // the same slot, flagged by the game as a named item, still resolves
        let named = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": { "add": { "8-18": {"e": 10, "a": 42, "j": 0, "b": 8, "d": 2, "c": 1} } }
        });
        let events = events_from_messages(std::slice::from_ref(&named));
        let GameEvent::ItemAdded { name, rarity, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(name, "Gold Inlaid Mysterious Potion");
        assert_eq!(resolve_rarity(rarity, name), "Angelic");
    }

    #[test]
    fn currency_is_found_wrapped_bare_and_in_a_query_string() {
        let wrapped = json!({"currencyData": {"GSS": 700, "GSH": 0}});
        assert!(matches!(&events_from_messages(&[wrapped])[0], GameEvent::Gold(c) if c.gss == 700));

        let bare = json!({"account_id": 5, "GSS": 727015, "GNS": 12});
        assert!(matches!(&events_from_messages(&[bare])[0], GameEvent::Gold(c) if c.gss == 727015));

        // query payloads: currency_data carries JSON as a string value
        let raw = b"\x01account_id=5&currency_data=%7B%22GSS%22%3A727015%7D&checksum=ab\x00";
        let messages = extract_messages(raw);
        let events = events_from_messages(&messages);
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::Gold(c) if c.gss == 727015)),
            "no gold in {messages:?}"
        );
    }

    #[test]
    fn audit_announcement_with_non_ascii_name() {
        // 'İ' lowercases to two chars, so byte offsets taken from the
        // lowercased copy do not line up with the original
        let payload = json!({"message": "İSTANBUL just found [Doom Bringer]"});
        let events = events_from_messages(std::slice::from_ref(&payload));
        assert!(matches!(
            &events[0],
            GameEvent::Found { finder, name } if name == "Doom Bringer" && finder == "İSTANBUL"
        ));
    }

    #[test]
    fn audit_unrelated_packet_is_not_an_item() {
        // single-letter keys are common; "a"/"d" alone must not mint an item
        let payload = json!({"route": "party/update", "a": 5, "d": 6});
        let events = events_from_messages(std::slice::from_ref(&payload));
        assert!(
            !events.iter().any(|e| matches!(e, GameEvent::ItemAdded { .. })),
            "spurious item from {events:?}"
        );
        // a spelled-out payload is still an item
        let real = json!({"seed": 991, "rarity": 6, "type": 1});
        assert!(events_from_messages(std::slice::from_ref(&real))
            .iter()
            .any(|e| matches!(e, GameEvent::ItemAdded { .. })));
    }

    #[test]
    fn audit_mail_text_variants() {
        let mail = |text: &str| {
            let payload = json!({"message": text});
            events_from_messages(std::slice::from_ref(&payload))
                .into_iter()
                .find_map(|e| match e {
                    GameEvent::Mail(v) => Some(v),
                    _ => None,
                })
        };
        assert_eq!(mail("You have new mail!"), Some(true));
        assert_eq!(mail("No new mail"), Some(false));
        assert_eq!(mail("You have no new mail."), Some(false));
        assert_eq!(mail("Mailbox empty"), Some(false));
    }

    #[test]
    fn announced_finds_become_named_journal_items() {
        let payload = json!({"message": "Ragnar just found [Azazel's Despair]!"});
        let events = events_from_messages(std::slice::from_ref(&payload));
        assert!(matches!(
            &events[0],
            GameEvent::Found { finder, name } if name == "Azazel's Despair" && finder == "Ragnar"
        ));

        // straight from a capture: the channel prefix is not part of the name
        let server = json!({"message": "SERVER: Parahryushka Just found [Doctor's Potion]"});
        let events = events_from_messages(std::slice::from_ref(&server));
        assert!(matches!(
            &events[0],
            GameEvent::Found { finder, name } if name == "Doctor's Potion" && finder == "Parahryushka"
        ));
    }

    #[test]
    fn satanic_zone_carries_debuffs() {
        let payload = json!({"satanic_zone_name": "SZ_2_5", "zone_buffs": [17, 10], "zone_debuffs": "11|13"});
        let events = events_from_messages(std::slice::from_ref(&payload));
        assert!(matches!(
            &events[0],
            GameEvent::SatanicZone { buffs, debuffs, .. } if buffs == &[17, 10] && debuffs == &[11, 13]
        ));
    }

    #[test]
    fn steam_and_excluded_payloads_are_dropped() {
        assert!(events_from_messages(&[json!({"steam": 1, "xp": 5})]).is_empty());
        assert!(extract_messages(b"\x02{\"inventory_charms\": [1], \"a\": 2}\x00").is_empty());
    }

    #[test]
    fn reassembler_flushes_on_ack_change() {
        let mut asm = Reassembler::default();
        let flow = flow_from("1.2.3.4");
        assert!(asm.push(flow, 1, b"{\"a\":").is_none());
        assert!(asm.push(flow, 1, b"1}").is_none());
        let flushed = asm.push(flow, 2, b"next").unwrap();
        assert_eq!(flushed, b"{\"a\":1}");
    }

    fn flow_from(ip: &str) -> Flow {
        (ip.parse().unwrap(), 6600, 51000)
    }

    #[test]
    fn two_connections_from_one_host_do_not_shred_each_other() {
        // a fight floods the world connection while the save connection is
        // still sending; keyed by address alone the save was lost
        let mut asm = Reassembler::default();
        let save = flow_from("1.2.3.4");
        let world = ("1.2.3.4".parse().unwrap(), 6669, 51001);
        asm.push(save, 1, b"{\"currency_data\":{\"GSS\":");
        asm.push(world, 7, b"position noise");
        asm.push(world, 8, b"more noise");
        asm.push(save, 1, b"42}}");
        let flushed = asm.push(save, 2, b"x").expect("the save flushes on its own ack");
        let messages = extract_messages(&flushed);
        assert_eq!(messages.len(), 1, "the save survived the flood");
        assert_eq!(messages[0]["currency_data"]["GSS"], 42);
    }

    #[test]
    fn a_stray_brace_in_binary_noise_does_not_stall_parsing() {
        let mut asm = Reassembler::default();
        let flow = flow_from("1.2.3.4");
        // a lone '{' in framing bytes never closes; everything after it must
        // still be parsed instead of being carried forever
        asm.push(flow, 1, b"\x01{\x02noise{\"currency_data\":{\"GSS\":5}}");
        let flushed = asm.push(flow, 2, b"x").unwrap();
        let events = events_from_messages(&extract_messages(&flushed));
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::Gold(c) if c.gss == 5)),
            "capture stalled on a stray brace"
        );
    }

    #[test]
    fn reassembler_carries_a_message_split_across_flushes() {
        let mut asm = Reassembler::default();
        let flow = flow_from("1.2.3.4");
        // the ack moves on while the object is still open
        asm.push(flow, 1, b"{\"currency_data\":{\"GSS\":42");
        let first = asm.push(flow, 2, b"}}").unwrap();
        assert!(extract_messages(&first).is_empty(), "half a message must not parse");
        let second = asm.push(flow, 3, b"noise").unwrap();
        let events = events_from_messages(&extract_messages(&second));
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::Gold(c) if c.gss == 42)),
            "message lost across the flush boundary"
        );
    }
}
