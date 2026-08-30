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
    /// Inventory fingerprints that just left the player's bags.
    ///
    /// Dropping a worn item on the floor and picking it back up is two ordinary
    /// inventory operations, and the second is indistinguishable from finding
    /// the thing — same shape, same named flag, same identity. The only thing
    /// that tells them apart is having watched the first: the fingerprint the
    /// game gives an item survives the round trip exactly, so an addition of one
    /// the player has just let go of is a return.
    ItemsLetGo(Vec<String>),
    /// The account this client is logged in as.
    ///
    /// Every fingerprint the game hands out carries the account it was made
    /// for in its second field, and it keeps it for the life of the item — so
    /// knowing our own number is the whole of telling our things from other
    /// people's. The client says it in nearly every request it sends, which is
    /// why this is worth reading rather than deducing.
    WhoseAccount(String),
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
        /// Which grade of Hell, 1..5. Hell is not one difficulty but five, and
        /// the game says which in its own field — so "Hell" alone was never the
        /// whole answer to what a character is playing.
        hell_sub: i64,
        /// Which act the character is in, or 0 — see `act_of`. The room itself
        /// only ever comes with the heartbeat; this comes with every save.
        act: i64,
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
        /// absent where the packet did not state it — see `dict_to_events`
        mf: Option<i64>,
        level: i64,
        hlevel: i64,
        satanic_here: Option<bool>,
    },
    /// The client asking the server where the satanic zone is. It carries the
    /// identifier of the region it is asking on behalf of, and the reply that
    /// follows answers for that region and no other — which is the whole reason
    /// this is an event rather than noise. See `GameStats`.
    ZoneRegion(String),
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
        /// An Odyssey item the game did not flag as named. Odyssey keeps its
        /// own item space, so neither its packet nor a name read out of the
        /// seasonal tables says anything about what this is worth. A named
        /// Odyssey item is exempt: `c == 1` is the game's own claim that this
        /// is that item, and the tables are right about it whatever mode
        /// dropped it — which is what keeps the Satanic count on an Odyssey
        /// character correct.
        unscaled: bool,
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
/// The longest a single JSON value may span. Nothing the game sends is near it:
/// the biggest message in a 16,104-message capture is 35,674 bytes.
const MAX_SPAN: usize = 256 << 10;
/// What all the scans over one buffer may read between them, as a multiple of
/// its length. Openers that lead nowhere are what made the scan quadratic, and
/// this is the ceiling that stops a buffer of them from stalling capture.
const SCAN_BUDGET: usize = 8;
/// A carried tail is the truncated end of one message, so the cap has to clear
/// the biggest message the game sends: 35,674 bytes in a 16,104-message capture.
/// At 8 KB it did not, and the tail of every large answer — which is to say
/// every answer that lists drops — was refused on length alone.
///
/// It is not what tells a real tail from framing noise any more; see
/// `opens_a_value`. It is only a bound on what one flow may hold.
const CARRY_CAP: usize = 64 << 10;
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
        // Stitch the previous tail back on, and count how many flushes it has
        // waited. The count has to come out of the map with the tail:
        // recomputing it from whether the tail was empty — which is what this
        // did — made it 0 or 1 forever, so it never reached CARRY_ROUNDS and
        // the give-up below could never fire.
        //
        // Giving up used to mean dropping the tail. That threw away more than
        // the stray brace it was aimed at: everything the brace was holding
        // went with it. A `[` or `{` in binary framing is followed by whatever
        // the stream sent next, and if a drop answer sat behind it, three
        // flushes later the drop was gone — never counted, never chimed, never
        // journalled, and nothing said so.
        //
        // So the bytes are kept and parsed instead. What was behind the opener
        // is read on this pass, and no new carry is taken from it: carrying
        // again would hand the same opener another three flushes of hostages.
        let (mut data, rounds, giving_up) = match self.carry.remove(&flow) {
            Some((tail, rounds)) if rounds < CARRY_ROUNDS => (tail, rounds, false),
            Some((tail, _)) => (tail, 0, true),
            None => (Vec::new(), 0, false),
        };
        data.extend_from_slice(&flushed);

        // Is the unterminated value at the end a real message, or a framing
        // byte that happens to be a brace?
        //
        // This used to ask whether anything complete followed the opener, on
        // the reasoning that a truncated message is the last thing in the
        // stream. It is — but the test cannot see that, because it reads what
        // is INSIDE the truncated message: a drop answer whose first item
        // object has already closed looks exactly like a stray brace with a
        // whole message after it. Every split message with a closed value in it
        // was thrown away, and a drop cut this way is never counted, never
        // chimed and never journalled — nothing later recovers it.
        //
        // The two cannot be told apart by brackets at all: a stray brace never
        // closes either, so everything after it counts as nested. What tells
        // them apart is the next byte, which `opens_a_value` reads.
        let cut = unterminated_start(&data);
        let truncated = !giving_up
            && cut < data.len()
            && data.len() - cut <= CARRY_CAP
            && opens_a_value(&data[cut..]);
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

/// Whether an unterminated opener reads as the beginning of a real value.
///
/// The game's messages are JSON objects whose first member is a quoted key. The
/// framing bytes a stray brace lives in are binary, and a `{` in them is
/// followed by whatever byte came next — `\x02` in the capture this is tested
/// against. So the byte after the opener is the discriminator that the bracket
/// structure cannot give: an object must be followed by a quote or its own
/// close, and an array by something that can start a value.
///
/// A tail that ends at the opener itself is a truncation too, and the most
/// ordinary one there is.
fn opens_a_value(tail: &[u8]) -> bool {
    let Some(&opener) = tail.first() else { return false };
    let mut i = 1;
    while tail.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
        i += 1;
    }
    let Some(&next) = tail.get(i) else { return true };
    match opener {
        b'{' => next == b'"' || next == b'}',
        b'[' => matches!(next, b'"' | b'{' | b'[' | b']' | b'-' | b't' | b'f' | b'n' | b'0'..=b'9'),
        _ => false,
    }
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
///
/// An opener that never closes costs a walk to the end of the buffer and the
/// scan resumes at the next byte, so a buffer full of them cost the square of
/// its length. The filter captures every plaintext TCP byte on the machine,
/// the buffer runs to `BUF_CAP`, and this runs on the capture thread between
/// `next_packet` calls: 1 MB of `{` measured 536 seconds in release, nine
/// minutes in which the pcap ring overflows and the game's own packets are
/// lost. The budget holds one flush to a constant multiple of its own length
/// whatever the bytes turn out to be.
fn extract_json_values(bytes: &[u8]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut budget = bytes.len().saturating_mul(SCAN_BUDGET).saturating_add(MAX_SPAN);
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'{' || c == b'[' {
            if budget == 0 {
                break;
            }
            let mut walked = 0;
            let end = json_end(bytes, i, &mut walked);
            budget = budget.saturating_sub(walked);
            if let Some(end) = end {
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
    json_end(b, start, &mut 0)
}

/// Where the value opened at `start` closes, and how many bytes were read to
/// find that out — the caller that scans from every position in a buffer needs
/// the second number to keep the whole scan linear.
///
/// The walk stops at `MAX_SPAN`: the longest message in a full session's
/// capture is 35,674 bytes, so a span still open a quarter of a megabyte later
/// is a framing byte being chased, not a message being read.
fn json_end(b: &[u8], start: usize, walked: &mut usize) -> Option<usize> {
    let stop = b.len().min(start.saturating_add(MAX_SPAN));
    let mut stack = vec![b[start]];
    let mut in_str = false;
    let mut esc = false;
    let mut found = None;
    let mut i = start + 1;
    while i < stop {
        let c = b[i];
        i += 1;
        if esc {
            esc = false;
            continue;
        }
        match c {
            b'\\' => esc = true,
            b'"' => in_str = !in_str,
            b'{' | b'[' if !in_str => stack.push(c),
            b'}' | b']' if !in_str => {
                let Some(&open) = stack.last() else { break };
                if (c == b'}') != (open == b'{') {
                    break;
                }
                stack.pop();
                if stack.is_empty() {
                    found = Some(i - 1);
                    break;
                }
            }
            _ => {}
        }
    }
    *walked += i - start;
    found
}

/// Which act the character is in, out of the save.
///
/// The exact room is only ever in `game_state`, and the game sends that when it
/// feels like it — in town, and when a panel is opened. A player grinding one
/// zone can go a thousand packets without one, which left the zone panel saying
/// "waiting for the game" for as long as they kept playing.
///
/// The save has no room in it, but it does carry `act_previous`, whose second
/// element is the act. Checked against every room the same capture reported:
/// `[1,7,..]` before `Act_07_05`, `[1,5,..]` before `Act_05_03`, `[1,4,..]`
/// before `Town_04_rm`, and eight more, without an exception. It is coarser
/// than a room, and it arrives with every save, which is often.
fn act_of(d: &Value) -> i64 {
    let from = |v: Option<&Value>| match v {
        Some(Value::Array(xs)) => xs.get(1).and_then(as_int).unwrap_or(0),
        _ => 0,
    };
    let direct = from(field_ref(d, &["act_previous", "actPrevious"]));
    if direct > 0 {
        return direct;
    }
    // the same save arrives wrapped as well as bare
    match field_ref(d, &["slot_data", "slotData"]) {
        Some(slot) => from(field_ref(slot, &["act_previous", "actPrevious"])),
        None => 0,
    }
}

/// A flag the game may send as a boolean or as a number.
///
/// `sz` was an integer once and is `true`/`false` now; reading it as an integer
/// alone made every room a non-satanic one.
fn as_bool(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(other) => as_int(other) == Some(1),
        None => false,
    }
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
const REGION_ID_FIELDS: &[&str] =
    &["crossregion_identifier", "crossRegionIdentifier", "cross_region_identifier"];
const ACCOUNT_ID_FIELDS: &[&str] = &["unique_account_id", "uniqueAccountId"];
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
    // The client's heartbeat, base64'd, in either of the two shapes it takes.
    //
    // One carries magic find and a satanic-zone flag; the other carries a
    // session's telemetry — how long it has been logged in, how many pickups,
    // which panel is open — and no magic find at all. They are not two versions
    // of one packet, they are two reports about the same session, and which one
    // arrives depends on where the character is: over a whole capture the first
    // came 52 times from a town and once from anywhere else, while the second
    // came 86 times and every single one of them from an act.
    //
    // So the second is the only thing that says where the player is while they
    // are actually playing. Refusing it — which reading `reason_id` as "this is
    // a crash report" did — leaves the whole zone panel waiting forever. Its
    // `region` says "ERROR" and its `season` says 0, which is what made it look
    // like wreckage; its room and its levels track the real ones exactly.
    if let Some(Value::String(blob)) = field(d, &["game_state", "gameState"]) {
        if let Some(state) = b64_json(&blob) {
            if let Some(room) = field_ref(&state, &["room"]).and_then(|v| v.as_str()) {
                if !room.is_empty() {
                    events.push(GameEvent::Room(room.to_string()));
                }
            }
            // Only what this packet actually carries. A field it does not
            // mention is `None`, not zero: the heartbeat has grown a second
            // shape before and it will again, and a missing number must leave
            // the last real one standing rather than erase it.
            let mf = has(&state, &["mf"]).then(|| int_field(&state, &["mf"]));
            let level = int_field(&state, &["level"]);
            let hlevel = int_field(&state, &["hlevel", "heroLevel", "herolevel"]);
            if mf.is_some() || level > 0 || hlevel > 0 {
                events.push(GameEvent::Vitals {
                    mf,
                    level,
                    hlevel,
                    // the game says outright whether this room is the satanic
                    // one; comparing zone codes was always a guess at it
                    satanic_here: has(&state, &["sz"])
                        .then(|| as_bool(field_ref(&state, &["sz"]))),
                });
            }
        }
    }
    // A whole word, not a substring, and not something a person said.
    //
    // "Lost Master's Platemail" is a real Set item and the server announces Set
    // finds to everybody on the shard, so a stranger's drop was ringing our
    // chime; whole words settled that. They do not settle a player typing the
    // word, and in one capture three did — "mail", "mailbox", "mailbox in
    // town" — each of which rang the chime and latched the indicator for
    // everyone on the shard. It was reported by a player on Bloodpact, a mode
    // with no mailbox in it at all.
    if !is_chat(d) && (says_mail(&message) || has(d, MAIL_FIELDS)) {
        events.push(GameEvent::Mail(mail_is_present(d)));
    }
    // server chat announcement: "Someone just found [Item Name]"
    if let Some((finder, name)) = announced_item_name(&msg_text(d)) {
        events.push(GameEvent::Found { finder, name });
    }
    events.extend(item_events(d));
    if let Some(region) = zone_request_region(d) {
        events.push(GameEvent::ZoneRegion(region));
    }
    if let Some(account) = our_account(d) {
        events.push(GameEvent::WhoseAccount(account));
    }
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
            hell_sub: int_field(d, &["hell_subdifficulty", "hellSubdifficulty"]),
            act: act_of(d),
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
    // The guild share of somebody else's experience, and the one event here
    // with no shape of its own to check against: a single field called `xp`.
    //
    // Field names are matched with the punctuation and case stripped out, which
    // is what lets `experience_gained` and `experienceGained` be one rule — and
    // it also means any two-letter key that normalises to `xp` matches. The
    // capture filter takes every plaintext message this machine sends or
    // receives, so that is not only the game: a junk packet from another device
    // on the same network carried `"Xp\u{fffd}"`, matched, and was the only
    // experience event in twenty-three thousand messages. It came to nothing
    // because its value was not a number, but a number would have been divided
    // by the guild share and credited.
    //
    // So this one is held to the same standard as the item path: it has to
    // arrive in something shaped like a game message. Every one of them carries
    // a `status` or a `message`; the junk packet carried two fields and neither.
    } else if !full_account
        && !identity_account
        && (has(d, &["status"]) || !msg_text(d).is_empty())
        && has(d, XP_GAIN_FIELDS)
        && !has(d, XP_TOTAL_FIELDS)
    {
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

/// The item types whose id is their identity rather than a slot in a base
/// table: keys, collectibles, materials, socketables and vaults.
///
/// `stats::RESOURCES` names the first four, which feed counters. A vault feeds
/// none — it is here to be named, which is what a custom list needs to match
/// one. Type 19 holds the seven Essence Vaults and nothing else, so `c == 0`
/// on it is simply what a vault is.
const SELF_NUMBERED: [i64; 5] = [12, 13, 14, 15, 19];

/// Relics. Type 16 holds the 156 of them and nothing else.
///
/// They are here because the ground path keeps only `c == 1` and a relic is
/// never that: across the owner's two captures, 1,652 relic sightings — 827 on
/// the floor, 70 picked up, 755 in inventory snapshots — carry `c == 0` without
/// a single exception, and every id seen lands inside the table's own 0..155.
/// So the flag that means "named item" cannot be what admits a relic; the type
/// has to.
///
/// Read off the FINGERPRINT, not the item: a relic packet carries no `type`
/// field at all, so the trailing number of the key it arrives under is the only
/// place its type is written. A relic reaching `is_item_like` without a
/// fingerprint therefore stays refused — none of the 827 did.
///
/// Deliberately NOT added to `SELF_NUMBERED` above. That list is what makes a
/// type nameable, and naming relics would change what lists already saved on
/// disk mean: `Death's Scythe` is a Set polearm AND relic 60, `Shrunken Head` a
/// Satanic charm AND relic 28, and a list holding either name today can only
/// ever have meant the non-relic one, because no relic has ever reached the
/// engine to match it. Relics stay nameless and are alerted on by identity
/// instead — see `GameStats::hunted_relic`.
const RELIC: i64 = 16;

/// What only a market message carries. See `item_sources`.
const MARKET_FIELDS: &[&str] =
    &["marketId", "market_id", "market_tokens", "marketTokens", "seller_name", "sellerName", "price"];

const ITEM_DATA_FIELDS: &[&str] = &["itemData", "item_data"];
const PICKUP_FIELDS: &[&str] = &["pickup_add_data", "pickupAddData"];
/// The session credentials the client attaches to everything it asks for. Their
/// presence means the packet is ours going out, not the server's coming back.
const CLIENT_ENVELOPE_FIELDS: &[&str] = &["identifier", "checksum"];

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
    // A trip to the market is not a find.
    //
    // Taking an item back off the trade board is answered like this:
    //
    // ```text
    //   {"message": "Removal success", "marketId": "138066",
    //    "fingerprint": "8-4559708-...", "itemData": "{...\"c\":1...}"}
    // ```
    //
    // which is an item, named, with its identity — the same shape a drop answer
    // has, and it was read as one. Two players' worth of gear came back from the
    // board and every piece of it was announced, chimed and journalled as though
    // it had just fallen. Listing an item sends the mirror of it, with a price
    // and a seller.
    //
    // The marks are unambiguous: in a 25 MB capture, 16,410 messages carry an
    // item and exactly four carry any of these — the two removals and the two
    // listings that caused this report. A drop answer has no price, no seller
    // and no market id, because none of those things is true of the floor.
    if MARKET_FIELDS.iter().any(|f| has(d, &[f])) {
        return vec![];
    }
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
        // Unrouted itemData is the server answering "here is what dropped" —
        // which only holds while the server is the one talking. We put an item
        // in the same field ourselves when we post it to the market, and every
        // listing was read as a fresh drop at our feet: journalled, counted,
        // announced and chimed, for an item we already owned and were selling.
        //
        // Our own requests are the ones carrying the session credentials, and
        // in a full session's capture not one of the 7702 packets from a server
        // carried them — the drop answers arrive with `itemGenHash` and nothing
        // else that names us.
        if has(d, CLIENT_ENVELOPE_FIELDS) {
            return vec![];
        }
        // Only `c == 1` items are named ones: their ids come from the unique
        // item space (5, 8, 30, 55 …), while `c == 0` drops are ordinary bases
        // numbered 0..20 — reading those through the name table turns every
        // white sword into whatever unique happens to share the number.
        let candidates = if is_item_like(&item_data) {
            vec![(own_fp, item_data)]
        } else {
            object_items(&item_data)
        };
        // A quest's reward is not lying on the floor either.
        //
        // Walking into a zone whose quest pays a named item has the client ask
        // for the item to be made, and the answer is a drop answer in every
        // respect — one named item, no owner, `itemGenHash` — so the thing was
        // announced the moment the zone loaded, before the quest was so much as
        // started. It is not in the world: it goes into `fortune_item` in the
        // save, and only reaches the bags through an ordinary pickup later.
        //
        // What tells them apart is that a thing in the world says where it is.
        // Six captures — the five kept here and the one from the machine that
        // reported this — carry 5,098 named items in a server's answer: 4,137
        // give the world's id for the spot they are on, 943 give a player whose
        // slot they sit in, and 18 give nothing. Seventeen of those eighteen are
        // the trade board, refused above; the eighteenth is the Mana Bender's
        // Will this was reported for.
        return candidates
            .into_iter()
            .filter(|(fp, item)| {
                int_field(item, &["c"]) == 1 || fingerprint_type(fp.as_deref()) == Some(RELIC)
            })
            .filter(|(_, item)| !belongs_to_a_player(item))
            .filter(|(_, item)| lies_on_the_floor(item))
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

/// Whether this item is in somebody's slot rather than lying on the ground.
///
/// The server answers "here is what dropped" and "here is what the merchant has"
/// in the same shape, down to the message, which is `ok` for both. What tells
/// them apart is what the item says about where it is: a thing on the ground has
/// a place on the map, and a thing in a shop window has a player.
///
/// ```text
///   somewhere in the world   "gd": 2422649   or   "gd": {"pos": [11, 0]}
///   in somebody's slot       "gd": {"player": 0}
/// ```
///
/// The number and the position are both the world; the captures here send the
/// number for named items and the position for ordinary ones. See
/// `lies_on_the_floor`, which reads the same field for the other half of the
/// question.
///
/// Opening the Black Market poured its whole stock into the journal as a
/// cascade of finds at the player's feet — twenty-five named items in one
/// packet, none of which had dropped. Over a session's capture the two never
/// mix: every `{player}` sits in one of the two merchant listings and every
/// `{pos}` in one of the three drop answers.
///
/// Both spellings, because the game uses both. This read `gd` alone and three
/// captures in this repository disagree about which one carries the marker: in
/// one, `gd` holds the ownership 268 times and `gid` never; in another `gid`
/// holds it 28 times and `gd` holds a position instead. Whichever the patch of
/// the day sends, a listing spelled the way this did not know went back to
/// pouring the merchant's stock into the journal as finds — the very thing this
/// function exists to stop, silently undone by a rename.
///
/// Both names are read separately, because taking whichever comes first reads
/// the wrong one on the capture where `gd` holds a position and `gid` holds the
/// owner.
fn belongs_to_a_player(item: &Value) -> bool {
    OWNER_FIELDS
        .iter()
        .filter_map(|f| field_ref(item, &[*f]))
        .any(|v| matches!(v, Value::Object(map) if map.contains_key("player")))
}

/// Whether the item says where in the world it is.
///
/// The field above says one of three things, and only the third is a claim to
/// be somewhere: a plain number, which is the world's id for the spot the thing
/// is lying on; `{"pos": [11, 0]}`, a place on the map; or `{"player": 0}`,
/// which is a slot and is `belongs_to_a_player`'s to refuse. An item that says
/// none of the three is not anywhere at all — see the fortune item in
/// `item_sources`.
///
/// So this asks only whether the item says anything, and leaves which of the
/// two worldly answers it gave alone. Reading the number and not the position
/// would have been enough for every capture kept here — of 5,098 named items in
/// a server's answer, 4,137 carry a number, 943 a player and 18 nothing, and
/// not one carries a position — but the position is what ordinary items on the
/// ground carry, so a named one arriving that way is a shape the game already
/// speaks, and refusing it would lose a real find.
fn lies_on_the_floor(item: &Value) -> bool {
    OWNER_FIELDS.iter().any(|f| field_ref(item, &[*f]).is_some())
}

/// Where an item says whose slot it is in, or which spot on the floor it is
/// lying on. See `belongs_to_a_player` and `lies_on_the_floor`.
const OWNER_FIELDS: &[&str] = &["gd", "gid"];

fn item_events(d: &Value) -> Vec<GameEvent> {
    // What left the bags, before what entered them: the two arrive in one
    // message when an item is moved, and the engine has to know the item is
    // coming back before it is told that it arrived.
    //
    // Both spellings, for the reason `belongs_to_a_player` reads two: a list of
    // fingerprints is what the game sends today, and the sibling operation
    // `add` is keyed by fingerprint instead — so the object form is the one a
    // rename would land on. Every object-shaped `remove` in the captures here
    // is empty, so reading its keys costs nothing and would survive the swap.
    let fps: Vec<String> = match field(d, &["operations"]).as_ref().and_then(|o| field(o, &["remove"])) {
        Some(Value::Array(gone)) => gone.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Some(Value::Object(gone)) => gone.keys().cloned().collect(),
        _ => vec![],
    };
    let mut out = match fps.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>() {
        fps if fps.is_empty() => vec![],
        fps => vec![GameEvent::ItemsLetGo(fps)],
    };
    out.extend(item_events_added(d));
    out
}

fn item_events_added(d: &Value) -> Vec<GameEvent> {
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
/// What the tables know about the item a packet is announcing, where the name
/// it goes by cannot say. See `items::BY_ID`.
///
/// A drop packet names the exact item — `(type, id, weaponType)` — and the
/// naming above already reads that triple. Only the name is passed on, though,
/// and two items can answer to one name, so the fact that the packet had said
/// which of them this was got thrown away on the way here.
///
/// Only where the identity names the same item. A find announced in the chat
/// line has no identity of its own, and the zeroes it carries are a real triple
/// belonging to a real item. Odyssey is refused here as it is everywhere: it
/// numbers its items in a space of its own, so a triple read on that scale is
/// not the triple this table is keyed by.
pub fn known_item(name: &str, unscaled: bool, id: (i64, i64, i64)) -> Option<crate::items::Known> {
    if unscaled || name.is_empty() {
        return None;
    }
    crate::items::known_by_identity(id.0, id.1, id.2)
        .filter(|k| k.name.eq_ignore_ascii_case(name.trim()))
}

pub fn resolve_rarity(packet: &Value, name: &str, unscaled: bool, id: (i64, i64, i64)) -> String {
    // The identity first, because it is the only exact answer here. A name is
    // what an item goes by, and eleven of them are gone by twice: the tables
    // below can say what "Angel" is worth only by picking one of the two items
    // called that, and picking wrongly cost a Common relic named Shrunken Head
    // the Satanic charm's rarity, its grade and its chime.
    if let Some(k) = known_item(name, unscaled, id) {
        if !k.rarity.is_empty() {
            return k.rarity.to_string();
        }
    }
    // An item off a scale these tables do not read claims nothing at all.
    //
    // This used to fall out by accident: the name table held only the five
    // rarities worth announcing, so an ordinary item found nothing there and
    // the packet — already refused for Odyssey — left it Unknown. Filling the
    // table in for every item turned that accident into a claim, and an
    // Odyssey rune came back as a seasonal Common.
    let known = if name.is_empty() || unscaled {
        None
    } else {
        crate::items::rarity_by_name(name)
    };
    // A named item's rarity is a fact about that item, and the tables carry it
    // from the game's own data. The packet does not: over the 6,617 rolls one
    // session's capture recorded, its rarity field took two values, and one of
    // them reads as "Angelic" here.
    //
    // This used to defer to the packet whenever its claim was not one of the
    // four the code called weak — so a claim of Angelic outranked the tables,
    // and a D-grade Satanic ring was announced, chimed and filed as an Angelic
    // find. Where the name is known, the name is the answer; the packet is only
    // consulted for items the tables have never heard of.
    if let Some(k) = known {
        return k.to_string();
    }
    // A name the tables refuse on purpose gets no answer from the packet either.
    //
    // Two of the five can answer to one name — the game calls both a Set gun and
    // a Heroic orb "Angel" — and the tables drop such a name rather than pick.
    // A drop packet is answered by its identity above; what reaches here is a
    // find announced in the chat line, which carries the name and nothing else,
    // and about that one there is genuinely nothing to say.
    //
    // The silence used to fall through to the packet, which claims Angelic for
    // nearly everything, so a Satanic charm named Shrunken Head and a Set gun
    // named Angel were both announced, chimed and journalled as Angelic finds.
    // Reported as "Non-Angelics showing up as angelic items".
    //
    // Unknown is a plain answer. Angelic is a wrong one.
    if crate::items::muddled(name) {
        return "Unknown".into();
    }
    crate::stats::rarity_from_packet(packet).unwrap_or_else(|| "Unknown".into())
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
    let claimed = field(obj, ITEM_RARITY_FIELDS).unwrap_or(Value::Number(0.into()));
    // An ordinary base cannot be Angelic or Unholy. Those two grades belong only
    // to named items — the Discord line leans on the same fact, taking them back
    // out of the SS count because every Angelic and Unholy item is SS-graded —
    // and `c` is the game's own flag for which id space an item came from, the
    // one the ground path already keeps only `c == 1` of.
    //
    // The `d == 7` that once filled a practice run with Angelic finds came back
    // in packets without the `h` that used to mark them: of 42 such pickups in a
    // capture, 17 carried `h` and 25 did not, and the 25 were indistinguishable
    // from seasonal ones except by a `d` nothing should believe anyway. Across
    // the 6,617 rolls the drop path saw in that session it took two values, 2
    // and 7, and the pickup path a third, 1 — not the ten-point scale the table
    // reads it as, and 7 is "Angelic" there.
    // Refusing the claim where it is impossible rather than where it is
    // recognised covers the keys, potions and white bases whatever mode made
    // them, and leaves the grades an ordinary base really can carry alone.
    let plain_base = has(obj, &["c"]) && int_field(obj, &["c"]) == 0;
    // Any rarity only a named item can carry, not just two of them.
    //
    // This listed 7 and 10 by hand — Angelic and Unholy — on the reasoning that
    // an ordinary base cannot be either. The reasoning was right and the list
    // was short: 9 is Heroic, no ordinary base is Heroic either, and Heroic is
    // in `JOURNAL_RARITIES`, so a base claiming it was named, announced,
    // chimed and counted. `Wind Token` is the one that showed it — a white
    // charm with `b: 17` and `c: 0`, which the tables answer for as the Satanic
    // charm sharing that triple, because the game numbers bases and named items
    // in two independent spaces.
    //
    // On 22,205 base sightings across two captures the field takes every value
    // from 1 to 43 — 6,985 of them 9, and 92 of them 24, which is not a rarity
    // at all — against 8,295 that say Superior. It is not a rarity on a base;
    // it is only ever believed here where believing it is harmless.
    //
    // Derived from the journal list rather than written out again, so the two
    // cannot drift apart: those five are exactly the rarities that make an item
    // worth naming below.
    let named_only = crate::stats::rarity_from_packet(&claimed)
        .is_some_and(|r| crate::stats::JOURNAL_RARITIES.contains(&r.as_str()));
    let rarity = if odyssey || (plain_base && named_only) { Value::Null } else { claimed };
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
    //
    // Keys, gems, runes and materials are the exception, and they have to be:
    // for those types `c == 0` is simply what they are — 471 of them against 2
    // in a capture — and the id IS the identity, so the table is right about
    // them. Naming them is not a guess. Three counters read that name and go
    // quiet without it: the two dull keys are filtered out by name, the notable
    // list (Angelic Key, Satanic Dice, the rune grades) is matched by name, and
    // a resource's grade comes from `tier_by_name`. Refusing the packet's rarity
    // above had silently taken all three down with it, because `worth_naming`
    // is read from the rarity this decides.
    //
    // Type 18 is NOT in the list, on purpose: slot 18:8 is an Angelic potion,
    // and reading ordinary potions through it is the exact bug the rule above
    // exists to stop.
    // The grade, on the same terms as the rarity above.
    //
    // Odyssey keeps its own item space and its `d` is already refused as
    // unreadable there. Its `n` is no more ours to read, and it says 6 — the
    // top grade — on ordinary white bases: ten of them in one capture, every
    // one `c == 0` and nameless, against not a single `n == 6` in the 235
    // seasonal pickups from the same session. Those ten were the SS column.
    //
    // Refusing it does not lose the grade. A named item's grade comes from the
    // item table by name, which is this program's own reading of the game and
    // is right whatever mode dropped the thing; `GameStats` already looks it up
    // whenever the packet offers nothing. So refusing a number we cannot read
    // is what lets the number we can read be used.
    //
    // The range check is separate and applies to every mode: `n` also arrives
    // as 6666 — five times in that capture, in both modes — which is not a
    // grade at all. It was counted as one because the only test it had to pass
    // was being greater than zero.
    let claimed_tier = int_field(obj, &["tier", "n"]);
    let tier = if odyssey || !(1..=crate::stats::SS_TIER).contains(&claimed_tier) {
        0
    } else {
        claimed_tier
    };
    let named_flag = int_field(obj, &["c"]) == 1;
    let resource = SELF_NUMBERED.contains(&item_type);
    let worth_naming = crate::stats::rarity_from_packet(&rarity)
        .is_some_and(|r| crate::stats::JOURNAL_RARITIES.contains(&r.as_str()));
    let name = if !explicit_name.is_empty() {
        explicit_name
    } else if named_flag || worth_naming || resource {
        crate::items::item_name(item_type, item_id, weapon_type).unwrap_or_default().to_string()
    } else {
        String::new()
    };
    GameEvent::ItemAdded {
        rarity,
        unscaled: odyssey && !named_flag,
        mf: int_field(obj, &["mf_drop", "mfDrop", "m"]) == 1,
        tier,
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
    says_mail(&t) || t == "1" || t == "true" || t == "yes"
}

/// Whether this packet is somebody talking.
///
/// A chat line carries who said it and where: a room, a colour for the name, an
/// account behind it. The server's own answers carry none of that — mail is
/// `{"message": "New mail!", "status": 1}` and nothing else. So the shape tells
/// them apart without having to read the words.
fn is_chat(d: &Value) -> bool {
    has(d, &["chatRoom", "chat_room", "room"])
        && has(d, &["nameColor", "name_color", "msgColor", "msg_color"])
}

/// Whether a line is about mail, rather than merely containing the letters.
fn says_mail(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.match_indices("mail").any(|(at, _)| {
        let before = lower[..at].chars().next_back();
        let after = lower[at + 4..].chars().next();
        let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
        // "mailbox" and "mails" are still about mail; "platemail" is not
        edge(before) && (edge(after) || lower[at..].starts_with("mailbox") || after == Some('s'))
    })
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

/// The client's zone question, told apart from every other packet that
/// carries the same identifier by being nothing else: two fields, no message,
/// no payload. The login packet names the region too, and reading that one as a
/// question would move the answer to whichever region logged in last.
///
/// `_src` is the capture writer's own annotation and is not part of the packet.
fn zone_request_region(d: &Value) -> Option<String> {
    let o = d.as_object()?;
    if o.keys().filter(|k| k.as_str() != "_src").count() != 2 {
        return None;
    }
    if !has(d, ACCOUNT_ID_FIELDS) {
        return None;
    }
    match field(d, REGION_ID_FIELDS)? {
        Value::String(s) if !s.is_empty() => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Who we are, out of the client's own requests.
///
/// Only the client sends this field: it is how a request is told from an
/// answer elsewhere in this file. So reading it anywhere it appears cannot
/// pick up somebody else's number by mistake.
fn our_account(d: &Value) -> Option<String> {
    match field(d, ACCOUNT_ID_FIELDS)? {
        Value::String(s) if !s.is_empty() => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The account a fingerprint was made for — its second field.
///
/// ```text
///   99-4964607-1a03b93f92c-10     ours
///   99-133690701-1a03ba73b5f-10   a friend's, picked up off the floor
/// ```
pub fn fingerprint_account(fingerprint: &str) -> Option<&str> {
    let mut parts = fingerprint.split('-');
    let _kind = parts.next()?;
    let account = parts.next()?;
    // there has to be more after it, or this is not a fingerprint at all
    parts.next()?;
    (!account.is_empty() && account.bytes().all(|b| b.is_ascii_digit())).then_some(account)
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

    /// The save says which act, and it is the only thing that says so often.
    ///
    /// Shape and values out of a capture taken 2026-08-22, where the player was
    /// in Act 6 and the game had not sent a heartbeat for over a thousand
    /// packets. `act_previous[1]` tracked every room the same capture reported
    /// — [1,7,..] before Act_07_05, [1,5,..] before Act_05_03, [1,4,..] before
    /// Town_04_rm — without an exception.
    #[test]
    fn the_save_says_which_act() {
        let bare = json!({
            "name": "Babazeya2", "level": 100, "herolevel": 26, "difficulty": 1,
            "season": 0, "hardcore": 0, "experience": 2370181,
            "act_previous": [1, 6, 0, 0],
            "act_zones_6": [0, 2, 1, 1, 1, 0, 0, 0, 0, 0]
        });
        let act = events_from_messages(std::slice::from_ref(&bare)).into_iter().find_map(|e| match e {
            GameEvent::Account { act, .. } => Some(act),
            _ => None,
        });
        assert_eq!(act, Some(6), "the second element is the act");

        // and the same save arrives wrapped in a slot as well
        let wrapped = json!({
            "account_id": "49646", "slot": "9", "save_counter": "1074",
            "name": "Babazeya2", "level": 100, "herolevel": 26, "difficulty": 1,
            "season": 0, "hardcore": 0, "experience": 2370181,
            "slot_data": { "act_previous": [1, 6, 0, 0] }
        });
        let act = events_from_messages(std::slice::from_ref(&wrapped)).into_iter().find_map(|e| match e {
            GameEvent::Account { act, .. } => Some(act),
            _ => None,
        });
        assert_eq!(act, Some(6));
    }

    /// Both shapes of the heartbeat say where the character is; only one of
    /// them says what its magic find is.
    ///
    /// Both packets are out of a capture taken 2026-08-21. The telemetry one is
    /// the only report that arrives while the character is in an act — 86 of
    /// them, every one from an act, against 52 of the other from a town — so
    /// refusing it leaves the zone panel with nothing to show. It carries no
    /// magic find, which is what must not be read as a zero.
    #[test]
    fn a_heartbeat_without_magic_find_still_says_where_we_are() {
        // region "ERROR", season 0, room Act_07_05, no mf
        let telemetry = json!({
            "checksum": "d5", "description": "x", "reason_id": 3, "region_id": 1, "slot": "9",
            "game_state": "eyJyZWdpb24iOiJFUlJPUiIsImhsZXZlbCI6Niwic2xvdCI6OSwibG9naW5fc2Vzc2lvbl90aW1lIjoiMzowNjoxMiIsImxhc3RfdWkiOiJVSV9IdWRfVGFsZW50X29iaiIsImxhc3RfdWlfbm9kZSI6Ik1hcFpvbmUiLCJzZWFzb24iOjAsInJvb20iOiJBY3RfMDdfMDUiLCJwaWNrdXBzIjo4NjM5LCJnYW1lX3Nlc3Npb25fdGltZSI6IjE6MDA6MzMiLCJoYXJkY29yZSI6MCwibGV2ZWwiOjEwMCwicHJldl9yb29tIjoiVG93bl8wN19ybSJ9"
        });
        let events = events_from_messages(std::slice::from_ref(&telemetry));
        assert!(
            matches!(events.first(), Some(GameEvent::Room(r)) if r == "Act_07_05"),
            "the act room comes only from this one: {events:?}"
        );
        assert!(
            matches!(events.get(1), Some(GameEvent::Vitals { mf: None, .. })),
            "and it says nothing about magic find: {events:?}"
        );

        // the other one says everything it says
        let beat = json!({
            "account_id": "49646", "checksum": "d5", "identifier": "dtt",
            "game_state": "eyJzbG90Ijo5LCJobGV2ZWwiOjIsIm1mIjoxNDQ3LCJyb29tIjoiVG93bl8wMV9ybSIsInNlc1RpbWUiOjI3MTcsImxldmVsIjoxMDAsInN6IjpmYWxzZX0="
        });
        let events = events_from_messages(std::slice::from_ref(&beat));
        assert!(matches!(events.first(), Some(GameEvent::Room(r)) if r == "Town_01_rm"));
        assert!(matches!(
            events.get(1),
            Some(GameEvent::Vitals { mf: Some(1447), level: 100, hlevel: 2, satanic_here: Some(false) })
        ), "{events:?}");

        // `sz` is a JSON boolean now and was a number once. It was still being
        // compared against 1, which a boolean never equals, so the game's own
        // word for "this room is the Satanic Zone" had stopped being read.
        let inside = json!({
            "account_id": "49646", "checksum": "d5", "identifier": "dtt",
            "game_state": "eyJzbG90Ijo5LCJobGV2ZWwiOjIyLCJtZiI6MjI2OSwicm9vbSI6IkFjdF8wMV8wNSIsInNlc1RpbWUiOjEsImxldmVsIjoxMDAsInN6Ijp0cnVlfQ=="
        });
        let events = events_from_messages(std::slice::from_ref(&inside));
        assert!(matches!(
            events.get(1),
            Some(GameEvent::Vitals { satanic_here: Some(true), .. })
        ), "{events:?}");
    }

    /// Leaving town and playing on, as the two packets really arrive.
    ///
    /// The town heartbeat states magic find; the one that comes from an act
    /// does not, and it is the only one that says which act. Between them the
    /// player must end up in the right room with the magic find they had — the
    /// second packet answering "nothing" about a number is not the same as it
    /// answering zero, and reading it as zero left a dash in the overlay for
    /// most of a session.
    #[test]
    fn leaving_town_keeps_the_magic_find_and_takes_the_room() {
        let in_town = json!({
            "account_id": "49646", "checksum": "d5", "identifier": "dtt",
            "game_state": "eyJzbG90Ijo5LCJobGV2ZWwiOjIsIm1mIjoxNDQ3LCJyb29tIjoiVG93bl8wMV9ybSIsInNlc1RpbWUiOjI3MTcsImxldmVsIjoxMDAsInN6IjpmYWxzZX0="
        });
        let in_an_act = json!({
            "checksum": "d5", "description": "x", "reason_id": 3, "region_id": 1, "slot": "9",
            "game_state": "eyJyZWdpb24iOiJFUlJPUiIsImhsZXZlbCI6Niwic2xvdCI6OSwibG9naW5fc2Vzc2lvbl90aW1lIjoiMzowNjoxMiIsImxhc3RfdWkiOiJVSV9IdWRfVGFsZW50X29iaiIsImxhc3RfdWlfbm9kZSI6Ik1hcFpvbmUiLCJzZWFzb24iOjAsInJvb20iOiJBY3RfMDdfMDUiLCJwaWNrdXBzIjo4NjM5LCJnYW1lX3Nlc3Npb25fdGltZSI6IjE6MDA6MzMiLCJoYXJkY29yZSI6MCwibGV2ZWwiOjEwMCwicHJldl9yb29tIjoiVG93bl8wN19ybSJ9"
        });
        let mut s = crate::stats::GameStats::default();
        for e in events_from_messages(std::slice::from_ref(&in_town)) {
            s.apply(&e);
        }
        assert_eq!(s.snapshot(String::new()).mf, 1447);

        for e in events_from_messages(std::slice::from_ref(&in_an_act)) {
            s.apply(&e);
        }
        let snap = s.snapshot(String::new());
        assert_eq!(snap.room.as_deref(), Some("Act_07_05"), "the zone panel needs this");
        assert_eq!(snap.mf, 1447, "silence about a number is not zero");
    }

    /// A quest's reward is not lying on the floor.
    ///
    /// Walking into a zone whose quest pays a named item announced it there and
    /// then, before the quest was started: the client asks for the item to be
    /// made on entering, and the answer is a drop answer in every respect. The
    /// message below is the one that reported this, verbatim — the item is a
    /// Mana Bender's Will, and it went into `fortune_item` in the save rather
    /// than into the world.
    /// Somebody saying "mailbox" is not mail arriving.
    ///
    /// All three lines are out of one capture, verbatim but for the account
    /// numbers: three strangers in the global chat, each of whom rang the mail
    /// chime and lit the indicator for everyone reading. Reported by a player
    /// on Bloodpact, which has no mailbox in it.
    #[test]
    fn a_stranger_saying_mailbox_is_not_mail() {
        for said in ["mail", "mailbox", "mailbox in town"] {
            let line = json!({
                "chatRoom": 0,
                "language": 0,
                "message": said,
                "msgColor": 16777215,
                "msgPlus": 0,
                "msgType": 0,
                "name": "Hior",
                "nameColor": 7844807,
                "platform": 0,
                "region": 7,
                "slot": 1,
                "uid": 5070307
            });
            let events = events_from_messages(std::slice::from_ref(&line));
            assert!(
                !events.iter().any(|e| matches!(e, GameEvent::Mail(_))),
                "{said:?} in chat rang the chime: {events:?}"
            );
        }

        // and the server's own answers still do, both ways round
        let came = json!({"message": "New mail!", "status": 1});
        assert!(
            matches!(events_from_messages(&[came]).first(), Some(GameEvent::Mail(true))),
            "the server saying so is still mail"
        );
        let none = json!({"message": "No new mail", "status": "0"});
        assert!(
            matches!(events_from_messages(&[none]).first(), Some(GameEvent::Mail(false))),
            "and so is the server saying there is none"
        );
    }

    #[test]
    fn a_quest_reward_waiting_to_be_earned_is_not_a_drop() {
        let fortune = json!({
            "itemData": {
                "7-4964607-659e0185c44750001-7":
                    {"a": 850937459, "b": 12, "c": 1, "d": 9, "e": 0, "j": 0, "sh": "2c490ef57269"}
            },
            "itemGenHash": "",
            "message": "ok",
            "operationTime": 0.0011980533599853516,
            "status": 1
        });
        assert!(
            events_from_messages(std::slice::from_ref(&fortune)).is_empty(),
            "a reward that has not been earned has not dropped"
        );
    }

    /// The merchant's window is not a pile of loot.
    ///
    /// Both packets are out of a capture taken 2026-08-21, the moment the Black
    /// Market was opened: the same shape as a drop answer, the same `ok`, and
    /// twenty-five named items that never dropped. The one difference is that a
    /// thing in the world says where it is and a thing in a shop says whose it
    /// is.
    #[test]
    fn a_shop_window_is_not_the_ground() {
        let stock = json!({
            "status": 1,
            "message": "ok",
            "itemGenHash": "abc",
            "operationTime": 1,
            "itemData": {
                "7-4964607-659930d6954b90001-3":
                    {"a": 180867568, "b": 1, "c": 1, "d": 9, "e": 0, "gd": {"player": 0}, "j": 14, "sh": "0d734caaa919"},
                "7-4964607-659930d695af90003-1":
                    {"a": 337583057, "b": 80, "c": 1, "d": 9, "e": 0, "gd": {"player": 0}, "j": 0, "sh": "a62f096b3a4f"}
            }
        });
        assert!(
            events_from_messages(std::slice::from_ref(&stock)).is_empty(),
            "nothing in a shop window has dropped"
        );

        // and the drop answer beside it still lands. The same field carries the
        // world's id for the spot the thing is lying on when it is lying on one;
        // that number is the whole difference, and this message is verbatim.
        let dropped = json!({
            "status": 1,
            "message": "ok",
            "itemData": {
                "99-4964607-1a025546fef-1":
                    {"a": 392508565, "b": 84, "c": 1, "d": 9, "e": 0, "gd": 2422649, "j": 0, "m": 1, "sh": "97ef9213eaf6"}
            }
        });
        let events = events_from_messages(std::slice::from_ref(&dropped));
        assert!(
            matches!(events.first(), Some(GameEvent::ItemAdded { ground: true, .. })),
            "a thing with a place in the world is a drop: {events:?}"
        );

        // and so is one that gives its place as a position rather than an id.
        // No named item arrives that way in any capture kept here, but every
        // ordinary item on the ground does, so it is a shape the game speaks
        // and a filter that refused it would throw away a real find.
        let by_position = json!({
            "status": 1,
            "message": "ok",
            "itemGenHash": "abc",
            "operationTime": 1,
            "itemData": {
                "7-4964607-65991cc0616140001-18":
                    {"a": 61067529, "b": 5, "c": 1, "d": 6, "e": 0, "gd": {"pos": [11, 0]}, "j": 0, "sh": "ecc3352481d6"}
            }
        });
        let events = events_from_messages(std::slice::from_ref(&by_position));
        assert!(
            matches!(events.first(), Some(GameEvent::ItemAdded { ground: true, .. })),
            "a position is a place too: {events:?}"
        );
    }

    /// A white charm is not the Satanic charm that shares its number.
    ///
    /// Straight out of a capture taken 2026-08-21, five times over. `c: 0` is
    /// the game's own flag for an ordinary base and `10:17:0` is a triple two
    /// items hold — the white charm the player actually picked up, and
    /// `Wind Token`, which the tables answer with because bases are not in
    /// them. The `d: 9` is what let it through: Heroic is a rarity worth
    /// naming, so the base was named, announced and chimed as a find the
    /// player never made.
    #[test]
    fn a_white_charm_is_not_wind_token() {
        let msg = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": { "add": {
                "99-4964607-1a02570a09d-10": {
                    "a": 327060669, "b": 17, "c": 0, "d": 9, "e": 0,
                    "gd": 2850811, "j": 0, "sh": "2a0bb624530e"
                }
            }}
        });
        let events = events_from_messages(std::slice::from_ref(&msg));
        let GameEvent::ItemAdded { name, rarity, item_type, item_id, .. } = &events[0] else {
            panic!("not an item: {events:?}")
        };
        assert_eq!((*item_type, *item_id), (10, 17), "it is the charm at 10:17:0");
        assert_eq!(
            crate::items::item_name(10, 17, 0),
            Some("Wind Token"),
            "and the tables do answer for that triple"
        );
        assert_eq!(rarity, &Value::Null, "but a base claims no rarity");
        assert!(name.is_empty(), "so it is left nameless, not called Wind Token");
    }

    /// A relic on the floor is seen, and the ordinary bases beside it are not.
    ///
    /// Verbatim from the owner's capture, trimmed only of the bases that make
    /// no new point. Every relic in 1,652 sightings carries `c == 0`, so the
    /// `c == 1` rule that admits a named drop threw all 827 of them away and no
    /// relic had ever reached the engine at all — which is why nothing could be
    /// alerted on one. The type has to do the admitting, read off the
    /// fingerprint because a relic packet has no `type` field.
    ///
    /// The trap is in the same packet on purpose: `99-...-555-3` is a `c == 0`
    /// type-3 base with a `gd` of its own, and it must still be refused. Letting
    /// the relic in by type must not let its neighbours in by accident.
    #[test]
    fn a_relic_on_the_floor_is_seen_and_the_bases_beside_it_are_not() {
        let msg = json!({
            "message": "ok",
            "status": 1,
            "itemData": {
                "99-4964607-1a025650554-16": {
                    "a": 24533420, "b": 127, "c": 0, "d": 9, "e": 0,
                    "gd": 2694669, "j": 0, "sh": "8b5bdb8ad9be"
                },
                "99-4964607-1a025650555-3": {
                    "a": 663812958, "b": 4, "c": 0, "d": 9, "e": 0,
                    "gd": 2694670, "j": 14, "n": 4, "sh": "4e3012a3a7db"
                }
            }
        });
        let events = events_from_messages(std::slice::from_ref(&msg));
        assert_eq!(events.len(), 1, "the type-3 base is still refused: {events:?}");
        let GameEvent::ItemAdded { name, item_type, item_id, weapon_type, ground, hash, .. } =
            &events[0]
        else {
            panic!("not an item: {events:?}")
        };
        assert_eq!((*item_type, *item_id, *weapon_type), (16, 127, 0), "relic 127");
        assert!(*ground, "it is lying on the floor, which is where a relic is worth hearing about");
        assert_eq!(hash, "8b5bdb8ad9be");
        // Nameless on purpose. Naming type 16 would change what a list already
        // saved on disk means — see the comment on `RELIC`. The windows read
        // the name off the identity instead, and `item_name` is what they call.
        assert!(name.is_empty(), "relics are not named here");
        assert_eq!(crate::items::item_name(16, 127, 0), Some("Jungle Vial"));
    }

    /// The same relic going into the bag, verbatim from the same capture.
    ///
    /// The pickup path never had the `c == 1` wall, so this arrived before the
    /// change too — but 70 pickups against 827 drops is why the floor is the
    /// sighting that matters. Both carry `sh: 8b5bdb8ad9be`, which is what lets
    /// the engine treat them as one item and chime once.
    #[test]
    fn the_pickup_of_a_relic_is_the_same_item_as_its_drop() {
        let msg = json!({
            "message": "Success on inventory update ext",
            "status": 1,
            "operations": { "add": {
                "99-4964607-1a025650554-16": {
                    "a": 24533420, "b": 127, "c": 0, "d": 9, "e": 0,
                    "j": 0, "sh": "8b5bdb8ad9be"
                }
            }}
        });
        let events = events_from_messages(std::slice::from_ref(&msg));
        let GameEvent::ItemAdded { item_type, item_id, ground, hash, .. } = &events[0] else {
            panic!("not an item: {events:?}")
        };
        assert_eq!((*item_type, *item_id), (16, 127));
        assert!(!*ground, "this one is in the bag");
        assert_eq!(hash, "8b5bdb8ad9be", "the same hash the floor sighting carried");
    }

    /// A relic is Common and D-graded, and the packet is not allowed to say
    /// otherwise.
    ///
    /// The name table used to hold a rarity only for the five worth
    /// announcing, so everything else — relics, runes, potions, keys — found
    /// nothing there and `resolve_rarity` fell back to the packet. That field
    /// takes two values over thousands of rolls and one of them reads as
    /// Angelic here, which is how a Common relic was announced, chimed and
    /// filed as an Angelic find.
    #[test]
    fn a_relic_is_what_the_tables_say_it_is() {
        let relic = "Jungle Vial";
        assert_eq!(crate::items::rarity_by_name(relic), Some("Common"), "the table is right");
        assert_eq!(crate::items::tier_by_name(relic), 1, "tier D");
        for claim in [json!(7), json!(10), json!(2), Value::Null] {
            assert_eq!(
                resolve_rarity(&claim, relic, false, NO_IDENTITY),
                "Common",
                "a packet claiming {claim} must not outrank the tables"
            );
        }
        // and an item off a scale these tables do not read still claims nothing
        assert_eq!(resolve_rarity(&json!(7), relic, true, NO_IDENTITY), "Angelic");
    }

    /// The client's heartbeat, as one really arrived on 2026-08-21 — the
    /// packet's own base64, not a hand-written one.
    ///
    /// It is the only place magic find comes from, and it rides on a packet
    /// carrying the session credentials, which the item path throws away
    /// wholesale. This pins the field names against a real packet so a patch
    /// that renames one of them fails here rather than quietly emptying the
    /// top row of the overlay.


    #[test]
    fn the_heartbeat_reports_magic_find() {
        // a real packet out of a capture taken 2026-08-21
        let packet = json!({
            "_src": "10.8.1.2",
            "account_id": "49646",
            "checksum": "d5996db42bdba7d1",
            "identifier": "dttIzbvwpWWqbgmBOQlDr1SjoQKkaHLB",
            "game_state": "eyJzbG90Ijo5LCJobGV2ZWwiOjIsIm1mIjoxNDQ3LCJyb29tIjoiVG93bl8wMV9ybSIsInNlc1RpbWUiOjI3MTcsImxldmVsIjoxMDAsInN6IjpmYWxzZX0=",
            "slot": "9"
        });
        let events = events_from_messages(std::slice::from_ref(&packet));
        let vitals = events.iter().find_map(|e| match e {
            GameEvent::Vitals { mf, level, hlevel, satanic_here } => {
                Some((*mf, *level, *hlevel, *satanic_here))
            }
            _ => None,
        });
        assert_eq!(vitals, Some((Some(1447), 100, 2, Some(false))), "events were {events:?}");
    }
    /// The capture filter takes every plaintext TCP byte the machine sends or
    /// receives, so a bulk transfer on any other port arrives here as one
    /// buffer of up to `BUF_CAP`. Every opener used to be chased to the end of
    /// it and the scan resumed one byte later, which is the square of the
    /// length: 256 KB of `{` measured 288 seconds in a debug build, and this
    /// runs on the capture thread, so the game's own packets are dropped for
    /// the duration. Messages ahead of the noise are still read.
    #[test]
    fn a_buffer_of_open_braces_does_not_stall_capture() {
        let mut buf = br#"{"currencyData": {"GSS": 1}}"#.to_vec();
        buf.extend(std::iter::repeat(b'{').take(512 << 10));
        let at = std::time::Instant::now();
        let messages = super::extract_messages(&buf);
        let took = at.elapsed();
        assert!(took < std::time::Duration::from_secs(5), "512 KB of braces took {took:?}");
        assert_eq!(messages.len(), 1, "the message before the noise is still read");
    }

    /// Both spellings of the ownership marker, because the game sends both.
    ///
    /// The guard knew `gd` only. Captures in this repository carry the marker
    /// under `gid` 28 times in one and under `gd` 268 times in another, with
    /// `gd` holding a plain position in the first — so on the days the game
    /// says `gid`, every item in the merchant's window was read as a find at
    /// the player's feet, which is exactly the flood the guard was written for.
    #[test]
    fn the_merchant_is_recognised_under_either_spelling_of_the_marker() {
        for owner in ["gd", "gid"] {
            let item = json!({ owner: {"player": 0}, "a": 372940672, "b": 8, "c": 1 });
            assert!(belongs_to_a_player(&item), "{owner} says whose slot it is in");
        }
        // and a position under either name is still the ground
        for where_at in ["gd", "gid"] {
            let item = json!({ where_at: {"pos": [11, 0]}, "a": 372940672 });
            assert!(!belongs_to_a_player(&item), "{where_at} with a position is a drop");
        }
        assert!(!belongs_to_a_player(&json!({"a": 1})), "saying nothing is still a drop");
    }

    /// Taking an item back off the trade board is not finding it.
    ///
    /// The server answers a removal with the item in full — named, identified,
    /// `c: 1` — which is the shape of a drop answer and was counted as one. The
    /// two messages below are from the capture that reported it: a Stormloop and
    /// a Thunder Guardian's Plate, both Heroic, both announced as finds the
    /// moment their owner took them back off the board.
    #[test]
    fn an_item_taken_back_off_the_market_is_not_a_drop() {
        let removal = json!({
            "message": "Removal success",
            "marketId": "138066",
            "fingerprint": "8-4559708-64f87be967fea0001-7",
            "itemData": "{\"d\":1,\"r\":0,\"sh\":\"0d820197c506\",\"b\":44,\"c\":1,\"e\":10,\"i\":548925603,\"q\":2,\"a\":566876198,\"j\":0,\"w\":1}",
            "logSuccess": 1,
            "status": "1"
        });
        assert!(
            events_from_messages(&[removal]).is_empty(),
            "a removal from the market announced nothing"
        );

        // and the listing that goes the other way, which carries a price
        let listing = json!({
            "item_name": "Stormloop",
            "price": "12121221",
            "market_tokens": "2",
            "seller_name": "Parahryushka",
            "rarity": "9",
            "fingerprint": "8-4559708-64f87be967fea0001-7",
            "item_data": {"a": 566876198, "b": 44, "c": 1, "d": 1, "e": 10, "j": 0, "w": 1}
        });
        assert!(events_from_messages(&[listing]).is_empty(), "nor does putting one up");
    }

    #[test]
    fn an_item_posted_to_the_market_is_not_a_drop() {
        // the listing as captured, credentials replaced
        let listing = serde_json::json!({
            "account_id": "7-49646",
            "beta": "0",
            "bricked_status": "undefined",
            "checksum": "0000",
            "damage_types": "|",
            "fingerprint": "7-4964607-65875ac569ff60006-3",
            "hardcore": "0",
            "identifier": "session-token",
            "item_data": {"a": 998596353, "b": 14, "c": 1, "d": 2, "e": 10, "j": 3, "m": 1, "sh": "5d6053f71623", "w": 1},
            "item_mask": "1086337038",
            "item_name": "Pillar of Niflheim",
        });
        assert!(
            super::events_from_messages(&[listing]).is_empty(),
            "selling an item we already own is not finding one"
        );

        // and a drop answer still is a drop. Not the same item: the capture
        // this listing came from never saw that one fall, so the alternative to
        // this line was inventing the answer for it — which is how a fixture
        // ends up proving whatever it was written to prove. This one is a
        // different message, copied out whole.
        let dropped = serde_json::json!({
            "itemData": {
                "99-4964607-1a025546fef-1": {"a": 392508565, "b": 84, "c": 1, "d": 9, "e": 0, "gd": 2422649, "j": 0, "m": 1, "sh": "97ef9213eaf6"}
            },
            "message": "ok",
            "status": 1,
        });
        let found: Vec<_> = super::events_from_messages(&[dropped])
            .into_iter()
            .filter(|e| matches!(e, super::GameEvent::ItemAdded { ground: true, .. }))
            .collect();
        assert_eq!(found.len(), 1, "the generation answer is still read as a drop");
    }

    /// A real generation answer: the white sword rolled from base id 8 must not
    /// be read as the unique that happens to sit at id 8, or every junk drop
    /// would chime as Satanic.
    #[test]
    fn only_named_drops_come_out_of_a_generation_answer() {
        // Both items are made up — the ids are 1 and 2 — so the world id on them
        // is made up too. What is not made up is that a thing in the world says
        // where it is; see `lies_on_the_floor`.
        let msg = serde_json::json!({
            "itemData": {
                "3-4964607-65875f2ed96610001-3": {"a": 1, "b": 8, "c": 0, "d": 2, "e": 10, "gd": 2422649, "j": 0, "n": 3, "sh": "aa"},
                "3-4964607-65875f2ed96610002-3": {"a": 2, "b": 30, "c": 1, "d": 2, "e": 10, "gd": 2422650, "j": 0, "sh": "bb"}
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

    /// A find announced in the chat line carries a name and nothing else.
    const NO_IDENTITY: (i64, i64, i64) = (-1, -1, -1);

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
    fn only_the_bare_question_names_the_region_the_zone_answers_for() {
        // What the client sends just before the server names the zone. Two
        // fields and nothing else, which is what tells it apart.
        let ask = json!({"crossregion_identifier": "8909978777", "unique_account_id": "4964607"});
        let events = events_from_messages(std::slice::from_ref(&ask));
        assert!(
            matches!(&events[0], GameEvent::ZoneRegion(id) if id == "8909978777"),
            "the question carries the region it is asked on behalf of"
        );

        // The login packet names the same region and is not a question. Reading
        // it as one moves the answer to whichever region logged in last.
        let login = json!({
            "account_id": "49646", "beta": "0", "crossregion_identifier": "4659145238",
            "hardcore": "0", "season": "10", "unique_account_id": "4964607",
        });
        assert!(
            !events_from_messages(std::slice::from_ref(&login))
                .iter()
                .any(|e| matches!(e, GameEvent::ZoneRegion(_))),
            "a packet that merely mentions the region is not asking about the zone"
        );
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
        assert!(parsed.contains(&("6".into(), true, 1, 71)), "a named item keeps its claim");
        // and the base beside it does not: 9 is Heroic, which no base is
        assert!(parsed.contains(&("null".into(), false, 6, 8)));
    }

    /// The same complaint as the Odyssey one, from packets that carry none of
    /// the marks that caught it: `d = 7` on an ordinary base, with the season in
    /// `e` and no `h` at all. Straight out of a capture — a shard, a key and a
    /// potion, none of which can be Angelic.
    /// A D-grade Satanic ring, announced and filed as an Angelic find. The
    /// tables have it right; the packet's claim was outranking them.
    #[test]
    fn the_tables_outrank_the_packet_for_a_named_item() {
        let ring = "Apex Striker's Ring";
        assert_eq!(crate::items::rarity_by_name(ring), Some("Satanic"), "the table is right");
        assert_eq!(crate::items::tier_by_name(ring), 1, "and grades it D");
        // whatever the packet claims about it
        assert_eq!(resolve_rarity(&json!(7), ring, false, NO_IDENTITY), "Satanic", "a packet claiming Angelic");
        assert_eq!(resolve_rarity(&json!(2), ring, false, NO_IDENTITY), "Satanic", "and one claiming Superior");
        assert_eq!(resolve_rarity(&Value::Null, ring, false, NO_IDENTITY), "Satanic", "and one claiming nothing");
        // an item the tables have never heard of still keeps what it was sent
        assert_eq!(resolve_rarity(&json!(7), "No Such Item", false, NO_IDENTITY), "Angelic");
    }

    /// Two items, one name, and both were announced as Angelic finds.
    ///
    /// Reported as "Non-Angelics showing up as angelic items": Shrunken Head is
    /// a Satanic charm and Angel a Set gun. Neither was in the rarity table,
    /// because each name is claimed by two different items — the charm and a
    /// Common relic, the gun and a Heroic orb — and a name two items disagree
    /// about is dropped rather than answered wrongly. The silence then fell
    /// through to the packet, which claims Angelic for nearly everything.
    ///
    /// Where only one of the claimants is one of the five, that one is the
    /// answer: a Common relic sharing a name with a Satanic charm is not a
    /// competing claim about which of the five a find is. Where two of the five
    /// answer to one name, a name is not enough, and the packet is refused too
    /// — a drop packet is answered by its identity instead, below.
    #[test]
    fn a_name_two_items_answer_to_is_never_angelic_by_default() {
        // settled, because only one claimant is one of the five
        assert_eq!(crate::items::rarity_by_name("Shrunken Head"), Some("Satanic"));
        assert_eq!(crate::items::rarity_by_name("Death's Scythe"), Some("Set"));
        for claim in [json!(7), json!(2), Value::Null] {
            assert_eq!(resolve_rarity(&claim, "Shrunken Head", false, NO_IDENTITY), "Satanic");
            assert_eq!(resolve_rarity(&claim, "Death's Scythe", false, NO_IDENTITY), "Set");
        }

        // and the grade follows the same claimant, not the relic's D
        assert_eq!(crate::items::tier_by_name("Shrunken Head"), 5, "the charm is S");

        // unsettled, because the game calls both a Set gun and a Heroic orb this
        assert!(crate::items::muddled("Angel"));
        assert_eq!(crate::items::rarity_by_name("Angel"), None, "the table will not pick");
        assert_eq!(
            resolve_rarity(&json!(7), "Angel", false, NO_IDENTITY),
            "Unknown",
            "and the packet does not get to call it Angelic"
        );

        // an item the tables have never heard of is a different case: nothing
        // has refused it, so what it was sent still stands
        assert!(!crate::items::muddled("No Such Item"));
        assert_eq!(resolve_rarity(&json!(7), "No Such Item", false, NO_IDENTITY), "Angelic");
    }

    /// The identity says which of the two a drop is, where the name cannot.
    ///
    /// A capture of 45 minutes of play holds 20 finds named Angel and 23 named
    /// Justice — 15 of the Angels the Set gun and 5 the Heroic orb, told apart
    /// by the triple every one of those packets carried.
    #[test]
    fn an_identity_tells_apart_two_items_of_one_name() {
        let angel_gun = (3, 11, 14);
        let angel_orb = (15, 116, 0);
        let justice = (13, 30, 0);

        for claim in [json!(7), json!(8), Value::Null] {
            assert_eq!(resolve_rarity(&claim, "Angel", false, angel_gun), "Set");
            assert_eq!(resolve_rarity(&claim, "Angel", false, angel_orb), "Heroic");
            assert_eq!(resolve_rarity(&claim, "Justice", false, justice), "Common");
        }
        assert_eq!(known_item("Angel", false, angel_gun).map(|k| k.tier), Some(5));
        assert_eq!(known_item("Angel", false, angel_orb).map(|k| k.tier), Some(6));

        // and it answers for the loser of a settled name too: the relic is
        // Common D, and took the charm's Satanic S for as long as the name
        // alone was asked
        let relic = (16, 28, 0);
        assert_eq!(crate::items::rarity_by_name("Shrunken Head"), Some("Satanic"));
        assert_eq!(resolve_rarity(&json!(7), "Shrunken Head", false, relic), "Common");
        assert_eq!(known_item("Shrunken Head", false, relic).map(|k| k.tier), Some(1));
        assert_eq!(resolve_rarity(&json!(7), "Shrunken Head", false, (10, 37, 0)), "Satanic");

        // the identity answers for the item it names and no other. A chat-line
        // find carries zeroes, which are Harlequinn's Crest's own triple.
        assert!(known_item("Angel", false, (0, 0, 0)).is_none());
        assert!(known_item("", false, angel_gun).is_none(), "a nameless base is not this");
        assert!(known_item("Angel", true, angel_gun).is_none(), "Odyssey numbers its own");
    }

    /// Read a whole session back through the parser.
    ///
    /// `lib.rs` has been writing `debug-capture.jsonl` since the beginning so
    /// that a real session could be replayed "when counters look wrong", and
    /// until now nothing replayed it: every check here is a packet built by
    /// hand, which proves the rule and not the traffic.
    ///
    /// Ignored, because it wants a file the repository does not carry. Point it
    /// at one and read the report:
    ///
    ///     HS_CAPTURE=... cargo test replay_a_capture -- --ignored --nocapture
    #[test]
    #[ignore = "needs a capture; see the doc comment"]
    fn replay_a_capture() {
        use std::collections::BTreeMap;
        use std::io::{BufRead, BufReader};

        let path = std::env::var("HS_CAPTURE").expect("set HS_CAPTURE to a capture file");
        let file = std::fs::File::open(&path).expect("open the capture");
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        // name -> (what the tables answer, what the packet claims) -> how often
        let mut found: BTreeMap<String, BTreeMap<(String, String), usize>> = BTreeMap::new();
        // for the refused names: which item each of them actually was
        let mut who: BTreeMap<String, BTreeMap<(i64, i64, i64), usize>> = BTreeMap::new();
        let mut tiers: BTreeMap<String, i64> = BTreeMap::new();
        // Counted, not skipped: a base with no name still lands in a rarity
        // column, which is where a practice run once filled up with Angelic.
        let mut nameless: BTreeMap<String, usize> = BTreeMap::new();
        let mut lines = 0usize;

        for line in BufReader::new(file).lines() {
            let line = line.expect("read");
            if line.trim().is_empty() {
                continue;
            }
            lines += 1;
            let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
            for event in events_from_messages(std::slice::from_ref(&value)) {
                let kind = match &event {
                    GameEvent::Gold(_) => "gold",
                    GameEvent::XpGain(_) => "xp",
                    GameEvent::ItemsLetGo(_) => "let go",
                    GameEvent::WhoseAccount(_) => "account",
                    GameEvent::Account { .. } => "login",
                    GameEvent::ItemAdded { .. } => "item",
                    GameEvent::SatanicZone { .. } => "satanic zone",
                    _ => "other",
                };
                *kinds.entry(kind).or_default() += 1;
                let GameEvent::ItemAdded {
                    name, rarity, tier, unscaled, item_type, item_id, weapon_type, ..
                } = &event
                else {
                    continue;
                };
                let said =
                    resolve_rarity(rarity, name, *unscaled, (*item_type, *item_id, *weapon_type));
                if name.is_empty() {
                    *nameless.entry(said).or_default() += 1;
                    continue;
                }
                let packet = crate::stats::rarity_from_packet(rarity)
                    .unwrap_or_else(|| "-".into());
                *found.entry(name.clone()).or_default().entry((said, packet)).or_default() += 1;
                // the grade as the counters settle it, not as the packet
                // left it: a named item's grade is never on the wire
                let mut grade = *tier;
                if grade == 0 {
                    grade = known_item(name, *unscaled, (*item_type, *item_id, *weapon_type))
                        .map_or_else(|| crate::items::tier_by_name(name), |k| k.tier);
                }
                tiers.insert(name.clone(), grade);
                if crate::items::muddled(name) {
                    *who.entry(name.clone())
                        .or_default()
                        .entry((*item_type, *item_id, *weapon_type))
                        .or_default() += 1;
                }
            }
        }

        println!("
{lines} lines of {path}");
        for (kind, n) in &kinds {
            println!("  {n:>7}  {kind}");
        }
        let bases: usize = nameless.values().sum();
        println!("  {bases:>7}  of them nameless bases, which still fill a column:");
        for (said, n) in &nameless {
            println!("  {n:>7}    {said}");
        }

        let grade = |name: &String| {
            ["", "D", "C", "B", "A", "S", "SS"]
                .get(tiers[name] as usize)
                .copied()
                .unwrap_or("?")
        };
        let total = |by: &BTreeMap<(String, String), usize>| by.values().sum::<usize>();

        // What the fix is for: names the tables refuse, which used to be handed
        // to the packet, whose claim here is the one the report complained of.
        println!("
names the tables refuse on purpose:");
        for (name, by) in found.iter().filter(|(n, _)| crate::items::muddled(n)) {
            for ((said, packet), n) in by {
                println!("  {n:>4}x  {name} -> {said} (the packet said {packet})");
            }
            for ((t, id, wt), n) in &who[name] {
                println!("          {n:>4}x  as ({t}, {id}, {wt})");
            }
        }

        // Everywhere else the two differ, the tables are the ones being trusted.
        println!("
where the tables and the packet disagree:");
        let mut rows: Vec<_> = found
            .iter()
            .flat_map(|(name, by)| by.iter().map(move |(k, n)| (name, k, n)))
            .filter(|(_, (said, packet), _)| said != packet && packet != "-")
            .collect();
        rows.sort_by_key(|(name, (said, _), n)| (said.clone(), std::cmp::Reverse(**n), (*name).clone()));
        for (name, (said, packet), n) in &rows {
            println!("  {n:>4}x  {said:<9} {:<2} {name} (the packet said {packet})", grade(name));
        }

        println!("
named things, by what the tracker made of them:");
        let mut rows: Vec<_> = found.iter().collect();
        rows.sort_by_key(|(name, by)| {
            let said = by.keys().next().map(|(s, _)| s.clone()).unwrap_or_default();
            (said, std::cmp::Reverse(total(by)), (*name).clone())
        });
        for (name, by) in rows {
            let said: Vec<_> = by.keys().map(|(s, _)| s.as_str()).collect();
            println!("  {:>4}x  {:<9} {:<2} {name}", total(by), said.join("/"), grade(name));
        }
    }

    /// The same two, through a whole packet rather than one function.
    ///
    /// Built on a drop packet taken out of a real capture — the shape, the
    /// fields and the fingerprint are the server's, and only the identity is
    /// changed: `-10` and `b: 37` is Shrunken Head, `-3` with `b: 11` and
    /// `j: 14` is Angel. `d: 7` is the packet claiming Angelic, which is what
    /// it claims for nearly everything — over one session it says 8 on 155,459
    /// of 171,295 items, and the four other values it takes there are wrong
    /// about the item every time.
    #[test]
    fn a_real_drop_packet_named_by_two_items_is_not_announced_as_angelic() {
        let dropped = |fingerprint: &str, id: i64, wt: i64| {
            json!({
                "status": 1,
                "message": "ok",
                "itemGenHash": "abc",
                "operationTime": 1,
                "itemData": {
                    fingerprint: {"a": 61067529, "b": id, "c": 1, "d": 7, "e": 0,
                                  "gd": {"pos": [11, 0]}, "j": wt, "sh": "ecc3352481d6"}
                }
            })
        };
        // exactly what the counters are handed, identity and all
        let found = |packet: Value| {
            events_from_messages(&[packet]).into_iter().find_map(|e| match e {
                GameEvent::ItemAdded { name, rarity, item_type, item_id, weapon_type, .. } => {
                    Some((rarity, name, (item_type, item_id, weapon_type)))
                }
                _ => None,
            })
        };

        let (rarity, name, id) = found(dropped("7-4964607-65a04f84c51d80001-10", 37, 0))
            .expect("a named drop");
        assert_eq!(name, "Shrunken Head");
        assert_eq!(
            resolve_rarity(&rarity, &name, false, id),
            "Satanic",
            "the charm, not the relic that shares its name"
        );

        let (rarity, name, id) = found(dropped("7-4964607-65a04f84c51d80001-3", 11, 14))
            .expect("a named drop");
        assert_eq!(name, "Angel");
        assert_eq!(id, (3, 11, 14), "the packet said which Angel this is");
        assert_eq!(
            resolve_rarity(&rarity, &name, false, id),
            "Set",
            "the gun, and not the Angelic the packet claimed"
        );
    }

    /// Refusing the packet's rarity must not cost a resource its name: the dull
    /// keys are filtered by name, the notable list is matched by name, and a
    /// resource's grade comes from the name. Suppressing all three was the price
    /// of the rule above until the resource types were let through.
    #[test]
    fn a_resource_keeps_its_name_when_its_rarity_is_refused() {
        let pickup = |fp: &str, item: serde_json::Value| {
            let msg = json!({
                "status": 1,
                "message": "Success on inventory update ext",
                "operations": { "add": { fp: item } }
            });
            events_from_messages(&[msg]).into_iter().find_map(|e| match e {
                GameEvent::ItemAdded { name, rarity, tier, .. } => Some((name, rarity, tier)),
                _ => None,
            })
        };
        // a key: type 12, ordinary as every key is, arriving with the d = 7 the
        // rule above refuses
        let (name, rarity, _) = pickup("7-1-12", json!({"a": 1, "b": 0, "c": 0, "d": 7, "e": 10, "j": 0})).unwrap();
        assert_eq!(name, "Basic Key", "a key is still a key");
        assert_eq!(rarity, Value::Null, "and still claims no grade of its own");

        // and an equipment base in the same shape stays nameless, which is the
        // whole point of the rule
        let (name, _, _) = pickup("7-1-3", json!({"a": 1, "b": 0, "c": 0, "d": 7, "e": 10, "j": 7})).unwrap();
        assert_eq!(name, "", "an ordinary weapon is not read through the unique table");
    }

    #[test]
    fn an_ordinary_base_carries_no_named_rarity() {
        let pickup = |item: serde_json::Value| {
            let msg = json!({
                "status": 1,
                "message": "Success on inventory update ext",
                "operations": { "add": { "7-4964607-6593db690c6090001-3": item } }
            });
            events_from_messages(&[msg]).into_iter().find_map(|e| match e {
                GameEvent::ItemAdded { rarity, .. } => Some(rarity),
                _ => None,
            })
        };
        let plain = json!({"a": 116892350, "b": 0, "c": 0, "d": 7, "e": 10, "j": 7, "n": 2, "sh": "cb"});
        assert_eq!(pickup(plain), Some(Value::Null), "an ordinary base claims no grade");

        // Heroic is no more a base's rarity than Angelic is.
        //
        // This asserted the opposite — that 9 on a base was "attested" — on the
        // strength of having seen it in a capture. Seeing it is not evidence
        // that it means Heroic: across 22,205 base sightings the field takes
        // every value from 1 to 43, and 6,985 of them are 9. Nor is any
        // ordinary base Heroic in the game's own data: all 413 items whose key
        // marks them a base are Common, and not one is Heroic, Angelic,
        // Satanic, Set or Unholy. Believing the 9 named a white charm
        // `Wind Token` and announced it as a Satanic find.
        let ordinary = json!({"a": 1, "b": 8, "c": 0, "d": 9, "e": 10, "j": 0, "sh": "cb"});
        assert_eq!(pickup(ordinary), Some(Value::Null), "nor is a base Heroic");

        // the grades a base really can carry are still believed
        let white = json!({"a": 1, "b": 8, "c": 0, "d": 2, "e": 10, "j": 0, "sh": "cb"});
        assert_eq!(pickup(white), Some(json!(2)), "Superior on a base is its own");

        // and a named item keeps its own claim, Angelic included
        let named = json!({"a": 1, "b": 71, "c": 1, "d": 7, "e": 10, "j": 0, "sh": "cb"});
        assert_eq!(pickup(named), Some(json!(7)), "a named item may be Angelic");
    }

    #[test]
    fn a_grade_is_only_believed_when_it_is_one() {
        let grade_of = |v: &Value| {
            let events = events_from_messages(std::slice::from_ref(v));
            match &events[0] {
                GameEvent::ItemAdded { tier, .. } => *tier,
                _ => panic!("not an item"),
            }
        };
        let pickup = |body: serde_json::Value| {
            json!({
                "status": 1,
                "message": "Success on inventory update ext",
                "operations": { "add": { "7-4964607-6598765fd97540002-3": body } }
            })
        };

        // Straight out of a capture: an Odyssey pickup, an ordinary base by its
        // own `c`, with no name — claiming the top grade. Ten of these were the
        // SS column on a practice run.
        assert_eq!(
            grade_of(&pickup(json!({"a": 1, "b": 0, "c": 0, "d": 3, "e": 0, "h": 1, "n": 6, "sh": "x"}))),
            0,
            "Odyssey's grade is on its own scale, like its rarity"
        );

        // And a number that is not a grade in any mode. Also from the capture,
        // in both modes.
        assert_eq!(
            grade_of(&pickup(json!({"a": 1, "b": 0, "c": 0, "d": 7, "e": 10, "n": 6666, "sh": "x"}))),
            0,
            "6666 is not a grade; it only ever passed for being above zero"
        );

        // A seasonal grade is still read, including the top one.
        assert_eq!(grade_of(&pickup(json!({"a": 1, "c": 1, "d": 2, "e": 10, "n": 4, "sh": "x"}))), 4);
        assert_eq!(grade_of(&pickup(json!({"a": 1, "c": 1, "d": 2, "e": 10, "n": 6, "sh": "x"}))), 6);
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
        let GameEvent::ItemAdded { name, rarity, unscaled, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(resolve_rarity(rarity, name, *unscaled, NO_IDENTITY), "Unknown", "its scale is not ours to read");

        // the seasonal shape of the same capture keeps working
        let seasonal = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": { "add": {
                "7-4964607-64f8884a6cfbb000b-10": {"a": 42, "b": 0, "c": 0, "d": 2, "e": 10, "j": 0, "n": 1, "sh": "ab"}
            }}
        });
        let events = events_from_messages(std::slice::from_ref(&seasonal));
        let GameEvent::ItemAdded { name, rarity, unscaled, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(resolve_rarity(rarity, name, *unscaled, NO_IDENTITY), "Superior");
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
        let GameEvent::ItemAdded { name, rarity, unscaled, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(name, "", "an ordinary base is nameless; the table knows only uniques");
        assert_eq!(resolve_rarity(rarity, name, *unscaled, NO_IDENTITY), "Superior", "and it keeps the rarity it was sent with");

        // the same slot, flagged by the game as a named item, still resolves
        let named = json!({
            "status": 1,
            "message": "Success on inventory update ext",
            "operations": { "add": { "8-18": {"e": 10, "a": 42, "j": 0, "b": 8, "d": 2, "c": 1} } }
        });
        let events = events_from_messages(std::slice::from_ref(&named));
        let GameEvent::ItemAdded { name, rarity, unscaled, .. } = &events[0] else { panic!("not an item") };
        assert_eq!(name, "Gold Inlaid Mysterious Potion");
        assert_eq!(resolve_rarity(rarity, name, *unscaled, NO_IDENTITY), "Angelic");
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
    fn an_item_with_mail_in_its_name_is_not_mail() {
        let mail = |text: &str| {
            events_from_messages(&[json!({ "message": text })])
                .into_iter()
                .find_map(|e| match e {
                    GameEvent::Mail(has) => Some(has),
                    _ => None,
                })
        };
        // a real Set item, announced to the whole shard
        assert_eq!(mail("Ragnar just found [Lost Master's Platemail]"), None);
        assert_eq!(mail("Chainmail Coif picked up"), None);
        // and the lines that really are about mail
        assert_eq!(mail("You have new mail"), Some(true));
        assert_eq!(mail("Mailbox empty"), Some(false));
        assert_eq!(mail("No new mail"), Some(false));
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

    /// What the stray brace was holding comes back.
    ///
    /// An opener in framing noise looks exactly like a truncated message — the
    /// bytes after it are whatever the stream sent next — so it is carried, and
    /// after three flushes the carry is given up. Giving up used to mean
    /// dropping the tail, and a real message sitting behind the brace went with
    /// it: never counted, never chimed, never journalled. Now the bytes are
    /// parsed on the way out.
    #[test]
    fn a_message_held_hostage_by_a_stray_opener_is_recovered() {
        let mut asm = Reassembler::default();
        let flow = flow_from("1.2.3.4");
        // `[` then a quote is a plausible enough start for the carry to take it
        asm.push(flow, 1, b"\x01[\"noise");
        asm.push(flow, 2, b"{\"currency_data\":{\"GSS\":11}}");
        for ack in 3..8 {
            if let Some(flushed) = asm.push(flow, ack, b"x") {
                let events = events_from_messages(&extract_messages(&flushed));
                if events.iter().any(|e| matches!(e, GameEvent::Gold(c) if c.gss == 11)) {
                    return;
                }
            }
        }
        panic!("the message behind the stray opener was never parsed");
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
    fn a_message_cut_after_one_of_its_values_closes_is_still_carried() {
        let mut asm = Reassembler::default();
        let flow = flow_from("1.2.3.4");
        // The shape every real drop answer has: an inner object that has
        // already closed by the time the cut lands. Deciding the carry on
        // "does anything complete follow the opener" read that inner object
        // and called the whole message framing noise, so the drop was dropped.
        asm.push(flow, 1, b"{\"itemData\":{\"7-1-1\":{\"n\":1}},\"currency_data\":{\"GSS\":7");
        let first = asm.push(flow, 2, b"}}").unwrap();
        assert!(extract_messages(&first).is_empty(), "half a message must not parse");
        let second = asm.push(flow, 3, b"noise").unwrap();
        let events = events_from_messages(&extract_messages(&second));
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::Gold(c) if c.gss == 7)),
            "a message cut after a closed inner value was thrown away"
        );
    }

    #[test]
    fn a_tail_longer_than_eight_kilobytes_is_still_carried() {
        let mut asm = Reassembler::default();
        let flow = flow_from("1.2.3.4");
        // The biggest message in a real capture is 35 KB and the cap was 8 KB,
        // so the tail of every large answer was refused on length alone.
        let mut head = br#"{"pad":""#.to_vec();
        head.extend(std::iter::repeat(b'x').take(20 << 10));
        head.extend_from_slice(br#"","currency_data":{"GSS":9"#);
        asm.push(flow, 1, &head);
        let first = asm.push(flow, 2, b"}}").unwrap();
        assert!(extract_messages(&first).is_empty());
        let second = asm.push(flow, 3, b"noise").unwrap();
        let events = events_from_messages(&extract_messages(&second));
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::Gold(c) if c.gss == 9)),
            "a 20 KB tail was refused"
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
