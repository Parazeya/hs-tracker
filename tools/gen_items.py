"""Generate the item tables for both the renderer and the Rust backend.

Two sources, both current with the season:

  tools/data/helper/items.json  — datamined identities, rarities and grades
                                  (hero-siege-helper.vercel.app, see fetch_items.py)
  translationsItem.csv          — the game's own display names, keyed by the
                                  same tkey, read straight from the install
  translationsAttributes.csv    — the game's own wording for the stats those
                                  items roll, same install

Item identity is (type, gameId, weaponType), the triple the packets carry.
Names never come from seeds; a named item's grade never comes from a packet —
the drop that lands on the ground does not state one.
"""

import json
import os
import re
import sys
from pathlib import Path

# Item names carry characters a Windows console's default code page cannot
# encode, and printing one killed the run after the tables were already built.
sys.stdout.reconfigure(encoding="utf-8", errors="replace")
sys.stderr.reconfigure(encoding="utf-8", errors="replace")

DATA = Path(__file__).parent / "data" / "helper" / "items.json"
GAME = Path(os.environ.get("HERO_SIEGE_BIN", r"C:\Program Files (x86)\Steam\steamapps\common\HeroSiege\bin"))
OUT = Path(__file__).parent.parent / "src" / "items.js"
OUT_RS = Path(__file__).parent.parent / "src-tauri" / "src" / "items.rs"

# the five the tracker announces; the game's "Satanic Set" is the Set rarity
# The tracker spells one rarity differently from the tables; everything else it
# calls what the game calls it.
RENAMED = {"Satanic Set": "Set"}

# The five the tracker announces, chimes, counts and files in the journal. This
# gates the drop rates, zones and places below — those are for chase items, and
# emitting them for every white base would be noise.
#
# It does NOT gate the rarity itself. Every item needs one, or `resolve_rarity`
# falls back to the packet for relics, runes and potions — and the packet's
# rarity field reads as "Angelic" for nearly everything.
NOTABLE = {"Satanic", "Set", "Heroic", "Angelic", "Unholy"}

# The ordinary bases are left out of the tables entirely, and that is what keeps
# the identity triple unambiguous.
#
# The game numbers named items and ordinary bases in two independent spaces, so
# the same (type, gameId, weaponType) belongs to one of each: 412 triples are
# shared, every one of them a named item against a base. `items.setdefault`
# below kept whichever the file listed first, silently — which is how a white
# `helmet_normal_cap` and the Satanic `helmet_harlequin_crest` both became
# 0:0:0, and the cap lost. A pickup of that cap then asked the table for a name
# and was handed a Satanic one.
#
# Among named items alone there is not one collision — 543 triples, 543 items —
# so dropping the bases removes the ambiguity rather than papering over it. The
# parser's `c == 1` rule (only the game's own named-item flag may reach the name
# table) stops being the single thing holding this together and becomes the
# second line it was meant to be.
#
# Nothing is lost that is shown anywhere: an ordinary base is deliberately left
# nameless by the parser, never announced, and never counted by rarity.
#
# It is NOT "drop everything the game calls Common". Keys, gems, runes and
# materials are Common too, and the tracker reads all of them BY NAME: the dull
# keys are filtered by name, the notable groups are matched by name, and a
# resource's grade comes from the name table.
#
# So the rule is the smallest one that removes the ambiguity: a base is left out
# only when a NAMED item already claims its triple. That drops exactly the 412
# losers and keeps the other 534 ordinary entries — and it needs no list of item
# types, so a season that renumbers them cannot silently invalidate it.
# What the engine leaves in a record that never states a drop rate — the game's
# own way of saying "not from the world". `exe.rs` reads it out of the default
# struct rather than assuming it, and calls it NO_DROP.
NO_DROP = 50_000_000

ORDINARY = {"Common", "Superior", "Rare"}
TIERS = {"D": 1, "C": 2, "B": 3, "A": 4, "S": 5, "SS": 6}

# The datamined stat ids and the game's attribute table are two vocabularies,
# not one spelled twice: `defense_base` against `stat_defense`,
# `all_attributes_flat` against `stat_all_stats`, `life_percent` against
# `stat_max_hp`. Stripping the suffix and prefixing `stat_` pairs 66 of the 305
# ids and pairs some of those wrongly — it hands `life_percent` the table's
# "to Life", which is the flat stat sitting next to it. So the pairing is
# written out and checked against the file at generation time.
#
# An id the table has no entry for is deliberately absent here: the proc lines
# ("chance when striking to cast ...") are built in the game's code from pieces
# that are not in the file, and the codex, orb and vault affixes are named
# after their codex rather than their effect. Those reach items.js as their raw
# id and the page shows the id, which is the honest thing to show — a label



# ── the same files, in every language they carry ─────────────────────────────
#
# Each translations*.csv is `key|en|fi|pt|ru|zh|ja|ko|de|fr|sp|pl`, with the
# language codes on the first line of every section. The three readers below
# have always taken column one and thrown the rest away; these keep them, and
# `gen_items.py` writes them out beside the English tables as one file per
# language. See `LANG_OUT`.
#
# There is no Turkish. The community has a channel for it and the game ships no
# column, so nothing here can invent one.
LANGS: list[str] = []
# tkey -> {lang: name}, for each of the three files that name something
ITEM_TR: dict[str, dict[str, str]] = {}
ROOM_TR: dict[str, dict[str, str]] = {}
# translationsMain.csv, where the rarities and a few other words of the game's
# own vocabulary live — `item_type_relic` is in the item file, `satanic` is not
MAIN_TR: dict[str, dict[str, str]] = {}
# translationsEnemy.csv, which names the bosses the tally counts. Keyed by the
# ENGLISH name rather than by tkey, because that is what the Rust table stores
# and what reaches the page — see TALLIES in stats.rs.
ENEMY_TR: dict[str, dict[str, str]] = {}

# The Satanic Zone buffs and curses, by the English name the app prints.
BUFF_TR: dict[str, dict[str, str]] = {}

# Where a chase item drops, by the English name the tables spell it with.
PLACE_TR: dict[str, dict[str, str]] = {}


def _langs_of(line: str) -> list[str]:
    """The codes on a section heading, or nothing if this is not one."""
    if not line.startswith("["):
        return []
    parts = [p.strip() for p in line.split("|")[1:]]
    return parts if parts and all(re.fullmatch(r"[a-z]{2}", p) for p in parts) else []


def read_translated(path: Path, into: dict[str, dict[str, str]], keep=lambda key: True) -> dict[str, str]:
    """Fill `into` with every language, and hand back the English column.

    The heading is read rather than assumed: a season that adds a language adds
    a column, and a fixed list would silently drop it.
    """
    global LANGS
    english: dict[str, str] = {}
    langs: list[str] = LANGS
    for line in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
        if (head := _langs_of(line)):
            langs = head
            if not LANGS:
                LANGS = head
            continue
        key, _, rest = line.partition("|")
        key = key.strip()
        if not key or key.startswith("[") or not rest or not keep(key):
            continue
        values = [v.strip() for v in rest.split("|")]
        if not values or not values[0]:
            continue
        english.setdefault(key, values[0])
        said = into.setdefault(key, {})
        for lang, value in zip(langs, values):
            if value:
                said.setdefault(lang, value)
    return english


def game_names() -> dict[str, str]:
    """tkey -> English name, as the game itself shows it."""
    path = GAME / "translationsItem.csv"
    if not path.exists():
        print(f"note: {path} not found — falling back to the datamined names")
        return {}
    return read_translated(path, ITEM_TR)


def room_names() -> dict[str, str]:
    """room -> the name the game shows for it.

    The client's heartbeat says where the character is by room: `Act_05_03`,
    `Town_01_rm`, `Shadow_Realm_rm`. Composing a label from the numbers gives
    "Act 5 . Zone 3", and anything that is not an act falls back to the raw name
    with its underscores swapped for spaces — "Shadow Realm rm" in front of the
    player, suffix and all.

    The game names every one of them itself, keyed by exactly the string the
    heartbeat sends, so there is nothing to compose and nothing to guess: a
    season that adds an act adds its rooms here with it.
    """
    out: dict[str, str] = {}
    files = sorted(GAME.glob("translations*.csv"))
    if not files:
        # The other two readers say so and carry on, because a name can fall
        # back to the datamined one. This cannot: the rooms are only here, and
        # without them ACT_ZONES is empty, every "Overworld" place parses to
        # nothing, and the act-to-zone data silently reverts to the snapshot's —
        # which is a season behind. The run would print its usual success line
        # and the diff would look like a routine refresh.
        print(f"note: no translations*.csv under {GAME} — rooms and zones cannot be read")
    is_room = lambda key: bool(
        re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*_rm", key) or re.fullmatch(r"Act_\d+_\d+", key)
    )
    for path in files:
        for key, name in read_translated(path, ROOM_TR, is_room).items():
            out.setdefault(key, name)
    return out



# identity key -> the tkey its English name was read from, so the other ten
# languages can be looked up for exactly the same item
tkey_of: dict[str, str] = {}
translations = game_names()
rooms = room_names()

# Which zones each act actually has, out of the rooms the game names: act 1 runs
# to 6 and act 4 stops at 5. "Overworld" and a range that overshoots both need
# this, and a season that adds an act brings its own answer with it.
ACT_ZONES: dict[int, list[int]] = {}
for room in rooms:
    part = re.fullmatch(r"Act_(\d+)_(\d+)", room)
    if part:
        ACT_ZONES.setdefault(int(part.group(1)), []).append(int(part.group(2)))
for numbers in ACT_ZONES.values():
    numbers.sort()

# Nothing correct can be written without them, so stop rather than write
# something wrong over the tables that are already right.
if not ACT_ZONES:
    sys.exit(
        f"no rooms were read from {GAME}.\n"
        "Point HERO_SIEGE_BIN at the game's bin folder (see .env.example) and run this again;\n"
        "carrying on would rewrite items.js and items.rs with no rooms and a season-old act table."
    )

ROMAN = {"I": 1, "II": 2, "III": 3, "IV": 4, "V": 5, "VI": 6, "VII": 7, "VIII": 8, "IX": 9, "X": 10}


def place_codes(place: str) -> list[str]:
    """The game's own words for where a thing drops, as the codes we key on.

    `dropPlaces` is written to be read by a player — "Act IX Zone 4-5", "Act IV
    Dungeons", "Act III Zone 1, 2, 5", a bare "Boss Dungeons" for a thing that
    falls in all of them — and the panel needs "9-4" and "9-5", because that is
    what the satanic zone announcement can be matched against.

    Both say the same thing, and only one of them is current: `dropPlaces` is
    read out of the game itself, while the locations in the datamined table are
    a snapshot of some earlier season. They disagree for 19 items and the game
    is right about all of them — The Colossal Avenger has moved from act 6 to
    act 9, and the snapshot has never heard of act 9 at all.

    Anything that is not an act answers nothing: "Grimbone" and "Crystal Chest"
    and "Sheeponia (Inferno Only)" are places too, and they stay in `dropPlaces`
    as the words they are.
    """
    text = place.strip().rstrip(".")
    acts: list[int] = []
    named = re.match(r"Act\s+([IVX]+(?:\s*&\s*[IVX]+)*)\s+(.+)", text, re.I)
    if named:
        for numeral in re.split(r"\s*&\s*", named.group(1)):
            act = ROMAN.get(numeral.upper())
            if act:
                acts.append(act)
        rest = named.group(2)
    else:
        # A bare "Dungeons" is every act's, which is how the snapshot has it too
        acts = sorted(ACT_ZONES)
        rest = text
    if not acts:
        return []

    low = rest.strip().lower()
    if low.startswith("boss dungeon"):
        return [f"{act}-BD" for act in acts]
    if low.startswith("dungeon"):
        return [f"{act}-D" for act in acts]
    if low.startswith("overworld"):
        return [f"{act}-{zone}" for act in acts for zone in ACT_ZONES.get(act, [])]

    listed = re.fullmatch(r"zones?\s+(.+)", low)
    if not listed:
        return []
    wanted: set[int] = set()
    for part in listed.group(1).split(","):
        span = re.fullmatch(r"(\d+)\s*-\s*(\d+)", part.strip())
        if span and int(span.group(1)) <= int(span.group(2)):
            wanted.update(range(int(span.group(1)), int(span.group(2)) + 1))
        elif part.strip().isdigit():
            wanted.add(int(part.strip()))
    return [f"{act}-{zone}" for act in acts for zone in sorted(wanted)]

items, rarities, tiers, drops, zones, places, chases = {}, {}, {}, {}, {}, {}, {}
# What each identity says about itself, beside the name it goes by. See by_id.
#
# The rate is here as well as in `drops` because `drops` is keyed by NAME, and
# the seven Essence Vaults are one name with seven rates — 1 in 6,750 for the
# Superior one up to 1 in 100,000 for the Unholy. A name-keyed table cannot
# hold that, so it holds none of them, and the panel showed seven rows with no
# chance at all. An identity can.
facts: dict[str, tuple[str, str, int, int]] = {}
dropped: list[tuple[str, str, str]] = []
# What each name is claimed to be, by every item that answers to it. See the
# pruning below the loop.
claimed: dict[str, set] = {}

# Named items first, so a base can be checked against a settled table.
ENTRIES = json.loads(DATA.read_text(encoding="utf-8"))
ENTRIES.sort(key=lambda e: ((e.get("metadata") or {}).get("rarity") in ORDINARY,))

for entry in ENTRIES:
    meta = entry.get("metadata") or {}
    identity = meta.get("ID")
    name = translations.get(meta.get("tkey"), meta.get("name") or "").strip()
    if not name or name == "Unknown Name":
        continue
    if isinstance(identity, list) and len(identity) == 3:
        game_id, weapon_type, item_type = identity
        if item_type >= 0:
            key = f"{item_type}:{game_id}:{weapon_type}"
            taken = items.get(key)
            if taken is None:
                items[key] = name
                if meta.get("tkey"):
                    tkey_of[key] = meta["tkey"]
            elif meta.get("rarity") in ORDINARY:
                dropped.append((key, name, taken))
            elif taken != name:
                # Two NAMED items on one triple would mean the tables are lying
                # about one of them, and no rule here could tell which.
                print(f"warning: {key} is both {taken!r} and {name!r}")
    stated = meta.get("rarity")
    rarity = RENAMED.get(stated, stated)
    if rarity:
        rarities.setdefault(name.lower(), rarity)
    notable = rarity in NOTABLE
    tier = TIERS.get(meta.get("tier"))
    if tier:
        tiers.setdefault(name.lower(), tier)
    # "one in N" — the bigger the number, the rarer the drop
    #
    # Except for one number, which is not a chance at all. NO_DROP is what the
    # engine leaves in a record that never sets a rate, and it means the item
    # does not fall out of the world: it comes from a boss, a chest or a tower,
    # and the place it comes from is in `dropPlaces` beside it. Passing it
    # through printed "1/50M" against Grimbone's own helmet — a number a reader
    # would take for a one-in-fifty-million chance rather than for silence.
    # 1,058 of the 2,094 items carry it, 109 of them with a place named.
    #
    # Exactly that number, not everything above it. Three items are written with
    # a rate rarer than the default and meant it: 50,696,969, 111,111,111 and
    # 999,999,999 are hand-picked, and Lucifer's Crown really is one in a
    # hundred and eleven million.
    rate = (entry.get("droprate") or {}).get("base")
    if notable and isinstance(rate, (int, float)) and rate > 0 and rate != NO_DROP:
        drops.setdefault(name.lower(), int(rate))
    # where it is tied to: "8-2" is act 8 zone 2, "-D" a dungeon, "-BD" a boss
    # dungeon. Being tied is not exclusivity — the item drops anywhere, just
    # several times more often there, which is the "chase" rate the game shows
    # in green on the tooltip.
    where = entry.get("dropPlaces")
    where = [where] if isinstance(where, str) else (where or [])
    # Only from an entry that is an item. The table also carries the names of
    # the categories themselves — `item_type_boots` is the word "Boots", with
    # no identity, no rarity and no grade — and counting those as a second
    # opinion made every base whose name matches its own category look
    # disputed.
    if rarity and isinstance(identity, list) and len(identity) == 3:
        claimed.setdefault(name.lower(), set()).add(
            (rarity, tier, rate if isinstance(rate, (int, float)) else None)
        )
    codes = sorted({c for place in where for c in place_codes(place)})
    if not codes:
        # The game named no place at all for this one — 23 items — so the
        # snapshot is all there is. It is kept rather than dropped: stale is
        # better than silent, and every code the game does state overrides it.
        #
        # Codes only. The snapshot's `locations` also carries plain words —
        # "Sheeponia", "Circle of Hatred", "Unstable Rift" — and those went into
        # a table whose whole purpose is to be matched against a zone code like
        # "8-2". They can never match one, so they sat there as entries nothing
        # could ever read, and showed up as a zone code in the Codex. The words
        # are not lost: `dropPlaces` is where they belong and already has them.
        codes = sorted(
            c
            for d in (meta.get("drop") or [])
            for c in (d.get("locations") or [])
            if re.fullmatch(r"\d+-(?:\d+|D|BD)", str(c))
        )
    if notable and codes:
        zones.setdefault(name.lower(), codes)
    if notable and (codes or where):
        # A zone code is not the only way to know where a thing drops. This used
        # to sit under the line above and so needed a code, which cost fourteen
        # items their odds the moment plain words like "Sheeponia" stopped
        # counting as one: every Sheeponia drop — the Sheep King's three,
        # Steve's five, Loaded Dice, aimbot.exe — lost the one-in-N the Codex
        # shows, for a change that was only ever about a lookup table. `where`
        # holds the same places in words, and is what `dropPlaces` is built from.
        #
        # Still gated on knowing a place at all: the number means "this often in
        # its zone", so an item with no zone has nothing for it to be measured
        # against.
        #
        # The chase rate is the base rate times a factor the game carries beside
        # it — 142 of the snapshot's own chase numbers are exactly that product,
        # and not one of them is anything else. Computing it from the rate the
        # game states now keeps the pair in step through a season that retunes
        # either half, which taking the snapshot's number does not.
        factor = entry.get("chaseDropRate")
        if isinstance(rate, (int, float)) and rate > 0 and rate != NO_DROP and isinstance(factor, (int, float)) and factor > 0:
            # Truncated, not rounded: the snapshot's own chase numbers are the
            # exact product with the fraction cut off, and matching that keeps
            # a rebuild from moving 89 unrelated numbers by one.
            chases.setdefault(name.lower(), max(1, int(rate * factor)))
        else:
            best = [
                d["chase"]
                for d in (meta.get("drop") or [])
                if isinstance(d.get("chase"), (int, float)) and d["chase"] > 0 and d["chase"] != NO_DROP
            ]
            if best:
                chases.setdefault(name.lower(), int(min(best)))
    if notable and where:
        places.setdefault(name.lower(), sorted({str(w) for w in where}))
    if isinstance(identity, list) and len(identity) == 3 and identity[2] >= 0:
        game_id, weapon_type, item_type = identity
        own = rate if isinstance(rate, (int, float)) and rate > 0 and rate != NO_DROP else 0
        facts.setdefault(
            f"{item_type}:{game_id}:{weapon_type}", (name, rarity, tier, int(own))
        )
header = """// Generated by tools/gen_items.py — do not edit by hand.
// Identities, rarities and grades: datamined tables from hero-siege-helper.
// Display names: the game's own translationsItem.csv."""

# A name two different items disagree about is not a name this app can answer
# for.
#
# Everything below is keyed by the name the game prints, and eleven names belong
# to two items that say different things about themselves: the orb "Angel" is a
# Heroic SS and the gun of that name is a Satanic Set S; the relic "Death's
# Scythe" is a Common D and the polearm is a Set S. The first one written won
# and the second inherited its answer, so picking up a worthless relic played
# the Set chime, coloured the announcement green, added to the Set column and
# showed a drop chance belonging to a weapon.
#
# The only thing this table is given is the name — a drop that arrives through
# the chat line carries no identity at all — so where the name cannot be
# answered it is dropped rather than answered wrongly.
#
# Except where only one of the claimants is a rarity the tracker announces. The
# table exists to say which of the five a find is, and a Common relic sharing a
# name with a Satanic charm is not a competing claim about that: it is a claim
# about something the announcer never speaks of. Shrunken Head is a Satanic
# charm and a Common relic, Death's Scythe a Set weapon and a Common relic, and
# both were left unanswered until this rule.
#
# What is left is genuinely two of the five under one name — the game itself
# calls both a Set gun and a Heroic orb "Angel" — and nothing here can tell them
# apart. Those names are written out below so the tracker knows the silence is
# deliberate, because the fallback it used instead is worse than none: the
# packet's rarity field takes two values over a whole session and one of them is
# Angelic, which is how a Satanic charm came to be announced as an Angelic find.
#
# Per table, not wholesale: two items can agree on what they are and still fall
# at different rates, and that costs the Codex a number, not the rarity.
def disagree(claims: set, field: int) -> bool:
    return len({c[field] for c in claims}) > 1


def rates_disagree(claims: set) -> bool:
    """Whether two claimants really say different things about how often it falls.

    NO_DROP is not one of the things they can say. It is the engine's default,
    left standing in a record that never states a rate at all, so a claimant
    carrying it is silent rather than contradicting anyone — the same reason the
    rarity rule above steps over a Common relic that shares a name.

    Four names turned on this and lost a number the game states outright:
    Shrunken Head is a Satanic charm the game gives 1 in 52,950 and a Common
    relic that does not drop, and the charm's figure is exactly what the game's
    own tooltip prints. Death's Scythe, Satan's Horn and Justice are the same
    shape. Six names collide here in all; the two that remain — Angel and
    Essence Vault — really are two items with two different rates.
    """
    return len({c[2] for c in claims if c[2] != NO_DROP}) > 1


def only_notable(claims: set, field: int):
    """The one claim a notable rarity makes about this, when only one does."""
    mine = [c for c in claims if c[0] in NOTABLE]
    return mine[0][field] if len(mine) == 1 else None


muddled = {
    "rarity": {n for n, c in claimed.items() if disagree(c, 0)},
    "tier": {n for n, c in claimed.items() if disagree(c, 1)},
    "rate": {n for n, c in claimed.items() if rates_disagree(c)},
}
settled, refused = {}, set()
for name in muddled["rarity"]:
    keep = only_notable(claimed[name], 0)
    if keep:
        rarities[name] = keep
        settled[name] = keep
    else:
        rarities.pop(name, None)
        refused.add(name)
for name in muddled["tier"]:
    # the grade follows whichever claimant the rarity did
    keep = only_notable(claimed[name], 1) if name in settled else None
    if keep:
        tiers[name] = keep
    else:
        tiers.pop(name, None)
for name in muddled["rate"]:
    for table in (drops, chases, zones, places):
        table.pop(name, None)
# Where only one claimant states a rate at all, that rate is the answer, and the
# name keeps it rather than being dropped for a disagreement that is not one.
settled_rate = 0
for name, claims in claimed.items():
    if name in muddled["rate"]:
        continue
    stated = {c[2] for c in claims if c[2] != NO_DROP}
    if len(stated) == 1 and drops.get(name) in (None, NO_DROP):
        drops[name] = stated.pop()
        settled_rate += 1
for field, names in muddled.items():
    if names:
        print(f"note: {len(names)} names two items disagree about, so no {field}: {', '.join(sorted(names))}")
if settled_rate:
    print(f"note: {settled_rate} names kept the one rate a claimant actually states, "
          f"the other saying only the engine's \"does not drop\"")
if settled:
    print(f"note: {len(settled)} settled by the one notable claimant: "
          f"{', '.join(f'{n} = {r}' for n, r in sorted(settled.items()))}")
if refused:
    print(f"note: {len(refused)} left unanswered, and the packet refused for them: "
          f"{', '.join(sorted(refused))}")

# What an identity says about itself, where its name says otherwise.
#
# The tables above are keyed by name because a find announced in the chat line
# carries a name and nothing else. A drop packet is not so poor: it names the
# exact item, as (type, gameId, weaponType), and the parser already reads that
# triple to get the name in the first place. So the ambiguity the pruning above
# has to live with — two items, one name — is not an ambiguity there at all.
#
# Only the entries the name tables get wrong or refuse, because everywhere else
# the name is the same answer and a second copy of it would only be a second
# thing to keep in step.
by_id = {
    key: (name, rarity, tier, rate)
    for key, (name, rarity, tier, rate) in facts.items()
    if rarity
    and (
        rarities.get(name.lower()) != rarity
        or tiers.get(name.lower()) != tier
        or (rate and drops.get(name.lower()) != rate)
    )
}
print(f"note: {len(by_id)} identities the name tables cannot answer for: "
      f"{', '.join(sorted({n for n, _, _, _ in by_id.values()}))}")

rs_rooms = sorted(rooms.items())

rooms_js = json.dumps(rooms, ensure_ascii=False, separators=(",", ":"))

out = rf"""{header}
// Item identity is (type, gameId, weaponType); key "type:id:wt".

export const ITEMS = {json.dumps(items, ensure_ascii=False, separators=(",", ":"))};

export const RARITY_BY_NAME = {json.dumps(rarities, ensure_ascii=False, separators=(",", ":"))};

export const TIER_BY_NAME = {json.dumps(tiers, ensure_ascii=False, separators=(",", ":"))};

// What an identity is, where the name it goes by says otherwise: [name, rarity,
// grade], keyed the same way as ITEMS. Two items can wear one name — the game
// calls both a Set gun and a Heroic orb "Angel" — and the seven Essence Vaults
// share theirs across every rarity there is.
export const BY_ID = {json.dumps({k: [n, r, t or 0, d or 0] for k, (n, r, t, d) in sorted(by_id.items())}, ensure_ascii=False, separators=(",", ":"))};

// how rare a named item is: "one in N", so a bigger number is a rarer drop
export const DROP_RATE = {json.dumps(drops, ensure_ascii=False, separators=(",", ":"))};

// items tied to a place: "8-2" = act 8 zone 2, "8-D" a dungeon, "8-BD" a boss
// dungeon. Being tied is not exclusivity — the item drops anywhere, only more
// often here.
export const DROP_ZONES = {json.dumps(zones, ensure_ascii=False, separators=(",", ":"))};

// the better "one in N" that applies inside those places — the number the
// game's own tooltip prints in green
export const DROP_CHASE = {json.dumps(chases, ensure_ascii=False, separators=(",", ":"))};

// the same in words, for bosses and chests that have no zone code
export const DROP_PLACES = {json.dumps(places, ensure_ascii=False, separators=(",", ":"))};

/// The zone's code, out of whatever the game called it.
///
/// The room is "Act_08_02" and the satanic zone announcement is "SZ_8_2" — the
/// same patch of ground, spelled two ways, and the tables call it neither: they
/// call it "8-2". Both spellings end in the act and the zone, so both read the
/// same way.
export function zoneCode(name) {{
  const m = /^(?:Act|SZ|Satanic)_(\d+)_(\d+)/i.exec(String(name ?? ''));
  return m ? `${{Number(m[1])}}-${{Number(m[2])}}` : null;
}}

export const ROOMS = {rooms_js};

/// What the game calls a room, keyed by exactly the string the heartbeat sends.
export function roomName(room) {{
  return ROOMS[String(room ?? '')] ?? null;
}}

export function zoneLabel(room) {{
  const text = String(room ?? '');
  const known = ROOMS[text];
  if (known) return known;
  if (/^Town/i.test(text)) return 'town';
  const m = /^Act_(\d+)_(\d+)/i.exec(text);
  return m ? `Act ${{Number(m[1])}} · Zone ${{Number(m[2])}}` : text.replace(/_rm$/i, '').replace(/_/g, ' ');
}}

export const TYPE_NAMES = {{
  0: 'Helmet', 1: 'Chest', 2: 'Boots', 3: 'Weapon', 4: 'Gloves', 5: 'Amulet',
  6: 'Shield', 7: 'Ring', 8: 'Belt', 10: 'Charm', 11: 'Consumable', 12: 'Key',
  13: 'Collectible', 14: 'Material', 15: 'Socketable', 16: 'Relic', 18: 'Flask',
  19: 'Essence Vault',
}};

export const WEAPON_NAMES = {{
  1: 'Sword', 2: 'Dagger', 3: 'Mace', 4: 'Axe', 5: 'Claw', 6: 'Polearm',
  7: 'Chainsaw', 8: 'Staff', 9: 'Cane', 10: 'Wand', 11: 'Book', 12: 'Spellblade',
  13: 'Bow', 14: 'Gun', 15: 'Flask', 16: 'Throwing', 17: 'Novelty',
}};

export function itemName(type, id, weaponType) {{
  return ITEMS[`${{type}}:${{id}}:${{weaponType}}`] ?? ITEMS[`${{type}}:${{id}}:0`] ?? null;
}}

export function typeLabel(type, weaponType) {{
  if (type === 3 && weaponType > 0) return WEAPON_NAMES[weaponType] ?? `Weapon ${{weaponType}}`;
  return TYPE_NAMES[type] ?? `Type ${{type}}`;
}}

// the game grades items D..SS
export const TIER_LETTERS = ['D', 'C', 'B', 'A', 'S', 'SS'];

export function tierLabel(tier) {{
  return TIER_LETTERS[tier - 1] ?? (tier > 0 ? `T${{tier}}` : '');
}}

export function rarityByName(name) {{
  return RARITY_BY_NAME[String(name).toLowerCase().trim()] ?? null;
}}

export function tierByName(name) {{
  return TIER_BY_NAME[String(name).toLowerCase().trim()] ?? 0;
}}
"""

OUT.write_text(out, encoding="utf-8", newline="\n")


def rs_key(key: str) -> int:
    item_type, game_id, weapon_type = (int(part) for part in key.split(":"))
    return (item_type << 24) | (game_id << 8) | weapon_type


def rs_str(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


rs_items = sorted((rs_key(k), v) for k, v in items.items())
rs_muddled = sorted(refused)
# Three of the four: `Known` carries a name, a rarity and a grade, and nothing
# in the Rust asks how often a thing falls.
rs_by_id = sorted((rs_key(k), n, r, t or 0) for k, (n, r, t, _) in by_id.items())
rs_rarities = sorted(rarities.items())
rs_tiers = sorted(tiers.items())


rs_lines = [
    header.replace("//", "//", 1),
    "// Item identity is (type, id, weaponType), packed as (type << 24) | (id << 8) | wt.",
    "",
    f"static ITEMS: [(u32, &str); {len(rs_items)}] = [",
]
rs_lines += [f"    ({k}, {rs_str(v)})," for k, v in rs_items]
rs_lines += ["];", "", f"static RARITY_BY_NAME: [(&str, &str); {len(rs_rarities)}] = ["]
rs_lines += [f"    ({rs_str(k)}, {rs_str(v)})," for k, v in rs_rarities]
rs_lines += ["];", "", f"static TIER_BY_NAME: [(&str, u8); {len(rs_tiers)}] = ["]
rs_lines += [f"    ({rs_str(k)}, {v})," for k, v in rs_tiers]
rs_lines += [
    "];",
    "",
    "/// Names two of the five answer to, which nothing but an identity could",
    "/// tell apart — the game calls both a Set gun and a Heroic orb \"Angel\".",
    "///",
    "/// Written out because the silence about them is deliberate, and what",
    "/// filled it before was worse: `resolve_rarity` fell through to the",
    "/// packet, whose rarity field takes two values over a whole session and",
    "/// one of them reads as Angelic.",
    f"static MUDDLED: [&str; {len(rs_muddled)}] = [",
]
rs_lines += [f"    {rs_str(k)}," for k in rs_muddled]
rs_lines += [
    "];",
    "",
    "/// What an identity is, where the name it goes by says otherwise.",
    "///",
    "/// The tables above are keyed by name because a find announced in the chat",
    "/// line carries a name and nothing else. A drop packet is not so poor: it",
    "/// names the exact item, and the parser reads that triple to get the name",
    "/// in the first place. So the two items sharing one name, which no name",
    "/// table can tell apart, are told apart here.",
    "///",
    "/// Name, rarity, then grade — the name so a caller can check that the",
    "/// identity and the name it was handed mean the same item.",
    f"static BY_ID: [(u32, &str, &str, u8); {len(rs_by_id)}] = [",
]
rs_lines += [f"    ({k}, {rs_str(n)}, {rs_str(r)}, {t})," for k, n, r, t in rs_by_id]
rs_lines += [
    "];",
    "",
    "/// What the tables know about one item.",
    "pub struct Known {",
    "    pub name: &'static str,",
    "    pub rarity: &'static str,",
    "    /// 1 = D .. 6 = SS.",
    "    pub tier: i64,",
    "}",
    "",
    "/// What an identity is, for the few whose name cannot say. See BY_ID.",
    "pub fn known_by_identity(item_type: i64, id: i64, weapon_type: i64) -> Option<Known> {",
    "    let by = |wt| {",
    "        packed(item_type, id, wt)",
    "            .and_then(|key| BY_ID.binary_search_by_key(&key, |(k, ..)| *k).ok())",
    "            .map(|i| BY_ID[i])",
    "    };",
    "    let (_, name, rarity, tier) = by(weapon_type).or_else(|| by(0))?;",
    "    Some(Known { name, rarity, tier: tier as i64 })",
    "}",
    "",
    "/// Item name by identity; falls back to the weapon-type-agnostic key.",
    "pub fn item_name(item_type: i64, id: i64, weapon_type: i64) -> Option<&'static str> {",
    "    lookup(item_type, id, weapon_type).or_else(|| lookup(item_type, id, 0))",
    "}",
    "",
    "fn lookup(item_type: i64, id: i64, weapon_type: i64) -> Option<&'static str> {",
    "    let key = packed(item_type, id, weapon_type)?;",
    "    ITEMS.binary_search_by_key(&key, |(k, _)| *k).ok().map(|i| ITEMS[i].1)",
    "}",
    "",
    "/// The identity triple as the tables key it, or None if it is not one.",
    "/// Whether `said` is what this item is called, in any language the game has.",
    "///",
    "/// English first, because that is what the tables are keyed by and what a",
    "/// client in English sends; the aliases only when it is not.",
    "pub fn same_item(said: &str, english: &str, item_type: i64, id: i64, weapon_type: i64) -> bool {",
    "    let said = said.trim();",
    "    if said.eq_ignore_ascii_case(english) {",
    "        return true;",
    "    }",
    "    let Some(key) = packed(item_type, id, weapon_type) else { return false };",
    "    let lower = said.to_lowercase();",
    "    // several items can share a name in a language the game was loose with,",
    "    // so the whole run of that name is checked and not just the first",
    "    let at = ALIASES.partition_point(|(n, _)| *n < lower.as_str());",
    "    ALIASES[at..]",
    "        .iter()",
    "        .take_while(|(n, _)| *n == lower.as_str())",
    "        .any(|(_, k)| *k == key || *k == key & 0xFFFF_FF00)",
    "}",
    "",
    "fn packed(item_type: i64, id: i64, weapon_type: i64) -> Option<u32> {",
    "    if !(0..256).contains(&item_type) || !(0..65536).contains(&id) || !(0..256).contains(&weapon_type) {",
    "        return None;",
    "    }",
    "    Some(((item_type as u32) << 24) | ((id as u32) << 8) | weapon_type as u32)",
    "}",
    "",
    "/// Whether the tables refuse this name on purpose. See MUDDLED.",
    "pub fn muddled(name: &str) -> bool {",
    "    let key = name.trim().to_lowercase();",
    "    MUDDLED.binary_search(&key.as_str()).is_ok()",
    "}",
    "",
    "/// Rarity of a named item; names are matched lowercased.",
    "pub fn rarity_by_name(name: &str) -> Option<&'static str> {",
    "    let key = name.trim().to_lowercase();",
    "    RARITY_BY_NAME",
    "        .binary_search_by_key(&key.as_str(), |(k, _)| *k)",
    "        .ok()",
    "        .map(|i| RARITY_BY_NAME[i].1)",
    "}",
    "",
    "/// Grade (1 = D .. 6 = SS) of a named item. The grade is fixed per item,",
    "/// and the packet announcing a drop never states it.",
    "pub fn tier_by_name(name: &str) -> i64 {",
    "    let key = name.trim().to_lowercase();",
    "    TIER_BY_NAME",
    "        .binary_search_by_key(&key.as_str(), |(k, _)| *k)",
    "        .ok()",
    "        .map_or(0, |i| TIER_BY_NAME[i].1 as i64)",
    "}",
    "",
]
rs_lines += [f"static ROOMS: [(&str, &str); {len(rs_rooms)}] = ["]
rs_lines += [f"    ({rs_str(k)}, {rs_str(v)})," for k, v in rs_rooms]
rs_lines += [
    "];",
    "",
    "/// What the game calls the room the heartbeat named.",
    "///",
    "/// The heartbeat says `Act_05_03` or `Shadow_Realm_rm`; the game itself",
    "/// calls those Fuji Coast and the Shadow Realm, keyed by exactly that",
    "/// string. Composing a label out of the numbers instead reads acts and",
    "/// zones correctly and everything else not at all — `Shadow_Realm_rm` came",
    "/// out as \"Shadow Realm rm\", suffix and all, in the Discord presence.",
    "pub fn room_name(room: &str) -> Option<&'static str> {",
    "    ROOMS.binary_search_by_key(&room, |(k, _)| *k).ok().map(|i| ROOMS[i].1)",
    "}",
    "",
]

# ── the same items, under the names the other ten languages give them ────────
#
# The parser refuses an identity whose name does not match the table, and that
# guard is load-bearing: a find announced in the chat line carries a triple of
# zeroes, which is a real item ("Harlequinn's Crest"), so without it every chat
# announcement would come back as that helmet.
#
# But the name it compares is the one the *game* printed, and a German client
# prints German. So the guard rejected every drop for anyone not playing in
# English, and the rarity, the grade and the chime fell back to whatever the
# packet claimed. Here is the same item under its other names, so the guard can
# ask "is this that item in any language" instead of "is this that item in
# English". See `same_item`.
rs_aliases = sorted(
    {
        (said.lower(), (int(t) << 24) | (int(i) << 8) | int(w))
        for key, tkey in tkey_of.items()
        for t, i, w in [key.split(":")]
        for lang, said in ITEM_TR.get(tkey, {}).items()
        if lang != "en" and said and 0 <= int(t) < 256 and 0 <= int(i) < 65536 and 0 <= int(w) < 256
    }
)
rs_lines += [
    "",
    "/// Every other name the game has for an item, sorted by the name.",
    f"static ALIASES: [(&str, u32); {len(rs_aliases)}] = [",
]
rs_lines += [f"    ({rs_str(n)}, {k})," for n, k in rs_aliases]
rs_lines += ["];"]

OUT_RS.write_text("\n".join(rs_lines), encoding="utf-8", newline="\n")

print(
    f"items: {len(items)}, rarities: {len(rarities)}, grades: {len(tiers)}, "
    f"drop rates: {len(drops)}, zones: {len(zones)}, chase rates: {len(chases)} -> {OUT.name}, {OUT_RS.name}"
)


# ── one file per language ────────────────────────────────────────────────────
#
# Everything above is English, and stays English: `items.js` is what the app
# loads first and what every other language falls back to. The rest ride in
# `src/lang/<code>.json`, fetched only when someone picks that language — ten
# alphabets in one bundle is 1.4 MB nobody reading English needs.
#
# Only the names move. A rarity, a grade, a drop rate and a zone are the same
# fact in any language and live in the base file alone.
#
# The type labels are the one place the game's own word is not taken for
# English: it calls type 18 a "Vial" and weapon 15 a "Flask", and this app has
# said "Flask" for the first since it was written — renaming it would rename 22
# items in every list a user has already built. So English is ours and the other
# ten are the game's, which is what a reader of those languages expects to see.
LANG_OUT = OUT.parent / "lang"

TYPE_TKEY = {
    0: "item_type_helmet", 1: "item_type_bodyarmor", 2: "item_type_boots",
    3: "item_type_weapon", 4: "item_type_gloves", 5: "item_type_amulet",
    6: "item_type_shield", 7: "item_type_ring", 8: "item_type_belt",
    10: "item_type_charm", 11: "item_type_consumable", 12: "item_type_key",
    13: "item_type_collectible", 14: "item_type_material",
    15: "item_type_socketable", 16: "item_type_relic", 18: "item_type_flasks",
    19: "item_type_vault",
}
WEAPON_TKEY = {
    1: "item_type_sword", 2: "item_type_dagger", 3: "item_type_mace",
    4: "item_type_axe", 5: "item_type_claw", 6: "item_type_polearm",
    7: "item_type_chainsaw", 8: "item_type_staff", 9: "item_type_cane",
    10: "item_type_wand", 11: "item_type_book", 12: "item_type_spellblade",
    13: "item_type_bow", 14: "item_type_gun", 15: "item_type_flasks",
    16: "item_type_throwing", 17: "item_type_novelty",
}


def _said(table, key, lang):
    said = table.get(key or "", {})
    value = said.get(lang, "")
    # A language the game left blank for one row falls back to English, and the
    # page falls back to English again for a row that is missing entirely — so
    # writing the English out here would only make the file bigger.
    return value if value and value != said.get("en") else ""


# The rarities and a few words beside them are named by the game, so they are
# taken from it rather than written out again — except where it has no word
# (there is no "Set" rarity key, only "Set item") or where its own is
# machine-made: the Russian for `set_bonus` reads "install bonus". So the game
# supplies the default and `said.py` overrides it, which is the opposite of the
# rule hs-map uses, and for that reason.
RARITY_TKEY = {
    "Satanic": "satanic", "Heroic": "heroic", "Angelic": "angelic",
    "Unholy": "unholy", "Mythic": "mythic", "Common": "common",
    "Runeword": "runeword", "Relic": "item_type_relic", "Rarity": "rarity",
}


def main_words() -> None:
    """Read translationsMain.csv for its side of the vocabulary."""
    path = GAME / "translationsMain.csv"
    if path.exists():
        read_translated(path, MAIN_TR)
    # The bosses, re-keyed by the English name. Only the ones the tracker
    # actually counts: the file names eight hundred and twelve monsters and the
    # tally table names about thirty, and shipping the other seven hundred and
    # eighty would triple every language file to translate nothing anyone sees.
    #
    # Not every counted boss is in there — Gurag and Odin are not — and those
    # stay English, which is what a missing entry does everywhere else.
    path = GAME / "translationsEnemy.csv"
    if not path.exists():
        return
    satanic_buffs()
    drop_places()
    counted = tallied_bosses()
    by_tkey: dict[str, dict[str, str]] = {}
    read_translated(path, by_tkey)
    for said in by_tkey.values():
        english = said.get("en")
        if english and english in counted:
            ENEMY_TR.setdefault(english, said)
    tally_names()


def drop_places() -> None:
    """The bosses, chests and dungeons a chase item is tied to.

    The place strings are built out of a small vocabulary — an act, a kind of
    dungeon, a boss, a chest — and the game names most of the pieces somewhere
    across its sixteen tables. What it does not name (Common Chest, Uber Damien,
    Gabriel) is left to said.py, and what neither names stays English, which is
    what a missing entry does everywhere else.

    Only the piece is looked up, never the whole string: "Sheeponia (Inferno
    Only)" is a place and a difficulty, and the two are joined on the screen.
    """
    wanted: set[str] = set()
    for where in places.values():
        for one in where:
            bare = re.sub(r"\s*\((?:Inferno Only|Inferno Difficulty|Inferno)\)$", "", one)
            act = re.fullmatch(r"Act [IVX]+(?: & [IVX]+)? (.+)", bare)
            bare = act.group(1) if act else bare
            bare = re.sub(r"^Zone .+$", "", bare)
            if bare:
                wanted.add(bare)
    if not wanted:
        return
    for path in sorted(GAME.glob("translations*.csv")):
        rows: dict[str, dict[str, str]] = {}
        read_translated(path, rows)
        for said in rows.values():
            english = said.get("en")
            if english in wanted:
                PLACE_TR.setdefault(english, said)


def satanic_buffs() -> None:
    """The Satanic Zone buffs and curses, re-keyed by the English name.

    The game names all fifty-eight of them in translationsAttributes.csv, so the
    app has no business translating them itself. It writes a curly apostrophe
    and this app writes a straight one, and the numeral on the two Loot Goblin
    tiers is the app's own addition, so the key is folded before it is matched.
    """
    path = GAME / "translationsAttributes.csv"
    if not path.exists():
        return
    rows: dict[str, dict[str, str]] = {}
    read_translated(path, rows, keep=lambda k: k.startswith(("satanicBuff", "satanicDebuff")))
    for said in rows.values():
        english = (said.get("en") or "").replace("’", "'")
        if not english:
            continue
        BUFF_TR.setdefault(english, said)
        # Loot Goblin is one row in the game and two tiers in the app
        if english == "Loot Goblin":
            for numeral in ("I", "II"):
                BUFF_TR.setdefault(
                    f"{english} {numeral}",
                    {k: f"{v} {numeral}" for k, v in said.items()},
                )


# What the tally calls a boss, against what the game calls it.
#
# The tracker counts the uber forms apart from the normal ones and says so in
# the label; the game gives the uber form a proper name of its own and never
# writes "Uber" at all — its `e_uberAnubis_1` is simply Amun Ra. So the word is
# ours and the name is the game's, and they are joined here. Where the two
# already agree the label is looked up as it stands and needs no row.
TALLY_ALIAS = {
    "Uber Amun Ra": "Amun Ra",
    "Uber Architect": "Architect of Ruin",
    "Uber Blood Maiden": "Blood Maiden",
    "Uber Captain Grimtide": "Captain Grimtide",
    "Uber Chaos Tower": "Rogue Chaos Tower",
    "Uber Damien": "Damien",
    "Uber Endrixia": "Endrixia",
    "Uber King Rakhul": "King Rakhul",
    "Uber Luna": "Possessed Luna",
    "Uber Papa Legba": "Papa Legba",
    "Uber Phantom Leviathan": "Phantom Leviathan",
    "Uber Reaper": "Grim Reaper",
    "Uber Sung Lee": "Sung Lee",
    "Reaper": "Grim Reaper",
}


def tally_names() -> None:
    """The counted bosses, under the name the tally prints them with."""
    wanted = set(TALLY_ALIAS.values())
    if not wanted:
        return
    found: dict[str, dict[str, str]] = {}
    for path in sorted(GAME.glob("translations*.csv")):
        rows: dict[str, dict[str, str]] = {}
        read_translated(path, rows)
        for said in rows.values():
            english = said.get("en")
            if english in wanted:
                found.setdefault(english, said)
    uber = MAIN_TR.get("uber", {})
    for label, english in TALLY_ALIAS.items():
        said = found.get(english)
        if not said:
            continue
        if not label.startswith("Uber "):
            ENEMY_TR.setdefault(label, said)
            continue
        # our word in front of the game's name
        ENEMY_TR.setdefault(
            label,
            {
                lang: f"{uber.get(lang, 'Uber')} {name}"
                for lang, name in said.items()
                if lang != "en"
            }
            | {"en": label},
        )


def tallied_bosses() -> set[str]:
    """The boss and chest labels `stats.rs` sends to the page, read from it.

    Read rather than listed, so a season that adds a boss to TALLIES gets its
    name translated without anyone remembering to copy it here twice.
    """
    rs = OUT_RS.parent / "stats.rs"
    if not rs.exists():
        return set()
    body = rs.read_text(encoding="utf-8", errors="replace")
    start = body.find("TALLIES")
    if start < 0:
        return set()
    chunk = body[start : body.find("];", start)]
    return {m.group(1) for m in re.finditer(r'"([A-Z][^"]{2,40})"', chunk)}


def app_words():
    """What this app says itself, keyed by the English. See tools/said.py."""
    try:
        from said import SAID
    except ImportError:
        sys.path.insert(0, str(Path(__file__).parent))
        from said import SAID
    return SAID


def backend_words(written: dict[str, set[str]]) -> None:
    """Say so when the Rust side asks for a word no language answers.

    The page's words are found by reading its own call sites, and for a while
    that was the whole check — which is how three tray entries shipped in
    English. The backend asks by string too, and none of its strings appears
    anywhere in `src/`: the tray menu, the file dialogs, the errors it hands
    back and the Discord card. This is the only place that can see both halves.
    """
    src = OUT_RS.parent
    if not src.is_dir():
        return
    asked: dict[str, set[str]] = {}
    for path in sorted(src.glob("*.rs")):
        body = path.read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r'say::say\(\s*"((?:[^"\\]|\\.)*)"\s*\)', body):
            asked.setdefault(m.group(1).replace('\\"', '"'), set()).add(path.name)
    if not asked:
        return
    unanswered = sorted(k for k in asked if not any(k in words for words in written.values()))
    if unanswered:
        print(f"note: {len(unanswered)} words the backend says are in no language:")
        for key in unanswered:
            print(f"  {key!r} — {', '.join(sorted(asked[key]))}")


def write_languages():
    if not LANGS:
        print("note: no language columns were read — no side files written")
        return
    LANG_OUT.mkdir(exist_ok=True)
    main_words()
    words = app_words()
    written = []
    shipped: dict[str, set[str]] = {}
    for lang in LANGS:
        if lang == "en":
            continue
        side = {
            "items": {k: v for k, t in sorted(tkey_of.items()) if (v := _said(ITEM_TR, t, lang))},
            "types": {str(n): v for n, t in TYPE_TKEY.items() if (v := _said(ITEM_TR, t, lang))},
            "weapons": {str(n): v for n, t in WEAPON_TKEY.items() if (v := _said(ITEM_TR, t, lang))},
            "rooms": {k: v for k in sorted(rooms) if (v := _said(ROOM_TR, k, lang))},
            # The Satanic Zone places, by act and index — the game keys them
            # Act_05_05, and an announcement says Satanic_5_5, so the numbers
            # are the join and no English name has to survive the trip.
            "acts": {
                f"{int(m.group(1))}_{int(m.group(2))}": v
                for k in sorted(ROOM_TR)
                if (m := re.fullmatch(r"Act_(\d+)_(\d+)", k)) and (v := _said(ROOM_TR, k, lang))
            },
            # and the words this app writes itself, in the same file: a reader
            # of another language is fetching it anyway
            "ui": {
                **{
                    en: v
                    for en, tk in RARITY_TKEY.items()
                    if (v := _said(ITEM_TR, tk, lang) or _said(MAIN_TR, tk, lang))
                },
                # the bosses the tally counts, by the English name the Rust
                # table stores and the page renders
                **{en: v for en in sorted(ENEMY_TR) if (v := _said(ENEMY_TR, en, lang))},
                # the Satanic Zone's buffs and curses, named by the game
                **{en: v for en in sorted(BUFF_TR) if (v := _said(BUFF_TR, en, lang))},
                # and the places a chase item is tied to
                **{en: v for en in sorted(PLACE_TR) if (v := _said(PLACE_TR, en, lang))},
                **{en: said[lang] for en, said in sorted(words.items()) if said.get(lang)},
            },
        }
        path = LANG_OUT / f"{lang}.json"
        path.write_text(
            json.dumps(side, ensure_ascii=False, separators=(",", ":")),
            encoding="utf-8",
        )
        written.append(f"{lang}:{len(side['items'])}+{len(side['ui'])}")
        shipped[lang] = set(side["ui"])
    print(f"names+words in {len(written)} more languages -> src/lang/  ({', '.join(written)})")
    backend_words(shipped)


write_languages()
