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
# It does NOT gate the rarity itself, and that distinction is the whole point of
# splitting it out. The name table used to hold only these five, so any other
# item — a relic, a rune, a potion — had no rarity there and `resolve_rarity`
# fell back to the packet, whose rarity field is documented in parser.rs as
# taking two values over 6,617 rolls, one of which reads as "Angelic". A Common
# relic was announced as a Heroic one, and a pickup could be filed under a
# rarity the item never had.
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
# materials are Common too, and the tracker reads all of them BY NAME — the dull
# keys are filtered by name, the notable list (Angelic Key, Satanic Dice, the
# rune grades) is matched by name, and a resource's grade comes from the name
# table. Dropping those broke all three when it was tried, which is how this
# rule came to be written the narrow way instead.
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
# invented in this file would read like the game's own and not be.
STAT_KEYS = {
    "additive_arcane_dmg_flat": "stat_magic_damage",
    "additive_cold_dmg_flat": "stat_cold_damage",
    "additive_fire_dmg_flat": "stat_fire_damage",
    "additive_lightning_dmg_flat": "stat_lightning_damage",
    "additive_physical_dmg_flat": "stat_physical_damage",
    "additive_poison_dmg_flat": "stat_poison_damage",
    "ailment_critical_strike_chance_percent": "stat_ailment_crit_chance",
    "ailment_critical_strike_dmg_percent": "stat_ailment_crit_damage",
    "ailment_damage_percent": "stat_increased_ailment_damage",
    "all_attributes_flat": "stat_all_stats",
    "all_attributes_percent": "stat_all_stats_p",
    "all_resist_percent": "stat_all_resistance",
    "all_skills_flat": "stat_all_skills",
    "all_skills_flat_class": "stat_all_skills",
    "aoe_damage_percent": "stat_aoe_damage",
    "aoe_size_percent": "stat_aoe_size",
    "aoe_skills_flat": "stat_aoe_skills",
    "arcane_resist_percent": "stat_magic_resistance",
    "arcane_skill_dmg_flat": "stat_spell_magic_damage",
    "arcane_skill_dmg_percent": "stat_magic_damage_p",
    "arcane_skills_flat": "stat_magic_skills",
    "armor_based_on_level_flat": "stat_armor_per_level",
    "armor_converted_to_attack_dmg_percent": "stat_armor_attack_damage",
    "armor_converted_to_magic_skill_dmg_percent": "stat_armor_skill_damage",
    "armor_flat": "stat_armor",
    "armor_increase_percent": "stat_armor_p",
    "attack_dmg_base": "stat_base_damage",
    "attack_dmg_based_on_level_flat": "stat_damage_per_level",
    "attack_dmg_percent": "stat_physical_damage_p",
    "attack_range_percent": "stat_attack_range",
    "attack_rating_based_on_level_flat": "stat_attack_rating_per_level",
    "attack_rating_flat": "stat_attack_rating",
    "attack_rating_percent": "stat_attack_rating_p",
    "attack_speed_percent": "stat_attack_speed_p",
    "attacks_fire_additional_projectiles_flat": "stat_extra_projectile_02",
    "attacks_per_second_base": "attacks_per_second",
    "bleed_damage_percent": "stat_increased_bleed_damage",
    "block_chance_base": "stat_block_chance",
    "block_chance_percent": "stat_block_rating",
    "buffing_aura_effectiveness_increased_percent": "stat_buff_aura_effectiveness",
    "buffing_aura_range_increased_percent": "stat_buff_aura_range",
    "burning_damage_percent": "stat_increased_burning_damage",
    "cannot_be_frozen_none": "stat_cannot_be_frozen",
    "chaining_projectile": "stat_chaining_projectile",
    "chance_to_replenish_life_when_blocking_percent": "stat_life_after_block_chance",
    "cold_dmg_absorbed_percent": "stat_cold_absorb",
    "cold_resist_percent": "stat_cold_resistance",
    "cold_skill_dmg_flat": "stat_spell_cold_damage",
    "cold_skill_dmg_percent": "stat_cold_damage_p",
    "cold_skills_flat": "stat_cold_skills",
    "cooldown_recovery_percent": "stat_cooldown_reduction",
    "critical_strike_chance_percent": "stat_critical_rate",
    "critical_strike_dmg_percent": "stat_critical_damage",
    "crushing_blow_chance_percent": "stat_crushing_blow",
    "crushing_blow_stacks_flat": "stat_crushing_blow_stack",
    "damage_from_resist": "stat_all_resistances_damage",
    "damage_redirected_to_octa": "stat_octaReflect",
    "deadly_blow_chance_percent": "stat_deadly_blow",
    "deadly_blow_effect": "stat_deadly_blow_p",
    "decreased_merchant_prices_percent": "stat_merchant_prices",
    "defense_base": "stat_defense",
    "defense_based_on_level_flat": "stat_defense_per_level",
    "defense_percent": "stat_defense_p",
    "defense_vs_missiles_percent": "stat_defense_vs_missile",
    "dexterity_based_on_level_flat": "stat_dexterity_per_level",
    "dexterity_flat": "stat_dexterity",
    "dexterity_increase_percent": "stat_dexterity_p",
    "dmg_returned_to_attacker_percent": "stat_damage_return",
    "dmg_taken_to_life_percent": "stat_damage_taken_life",
    "dmg_taken_to_mana_percent": "stat_damage_taken_mana",
    "double_jump": "stat_double_jump",
    "effectiveness_of_non_buff_proc_relics_percent": "stat_relic_proc_effectiveness",
    "enemy_all_resist_percent": "stat_ignore_all_resist",
    "enemy_arcane_resist_percent": "stat_ignore_magic_resist",
    "enemy_cold_resist_percent": "stat_ignore_cold_resist",
    "enemy_fire_resist_percent": "stat_ignore_fire_resist",
    "enemy_lightning_resist_percent": "stat_ignore_lightning_resist",
    "enemy_poison_resist_percent": "stat_ignore_poison_resist",
    "energy_based_on_level_flat": "stat_energy_per_level",
    "energy_converted_to_cold_skill_dmg_percent": "stat_energy_cold",
    "energy_converted_to_lightning_skill_dmg_percent": "stat_energy_lightning",
    "energy_converted_to_magic_skill_dmg_percent": "stat_energy_skill_damage",
    "energy_flat": "stat_energy",
    "energy_increase_percent": "stat_energy_p",
    "enhanced_defense_based_on_level_percent": "stat_enhanced_defense_per_level",
    "enhanced_defense_percent": "stat_enhanced_defense",
    "enhanced_dmg_based_on_level_percent": "stat_enhanced_damage_per_level",
    "enhanced_dmg_percent": "stat_enhanced_damage",
    "explosion_area_of_effect_percent": "stat_explosion_aoe",
    "explosion_damage_percent": "stat_explosion_damage",
    "explosion_skills_flat": "stat_explosion_skills",
    "extra_arcane_dmg_to_shadowburn_percent": "stat_extraArcaneSpellburn",
    "extra_cold_dmg_to_frostbitten_percent": "stat_extraColdFrostburn",
    "extra_dmg_to_bleeding_percent": "stat_extraDamageBleeding",
    "extra_dmg_to_burning_percent": "stat_extraDamageBurning",
    "extra_dmg_to_deep_frozen_percent": "stat_extraDamageFrozen",
    "extra_dmg_to_monsters_afflicted_with_ailments_percent": "stat_extraDamageAilments",
    "extra_dmg_to_poisoned_percent": "stat_extraDamagePoisoned",
    "extra_dmg_to_shadowburn_percent": "stat_extraDamageSpellBurn",
    "extra_dmg_to_stasis_percent": "stat_extraDamageStasis",
    "extra_dmg_to_stunned_percent": "stat_extraDamageStunned",
    "extra_fire_dmg_to_burning_percent": "stat_extraFireBurning",
    "extra_gold_from_kills_percent": "stat_extra_gold",
    "extra_lightning_dmg_to_stasis_percent": "stat_extraLightningStasis",
    "extra_physical_dmg_to_bleeding_percent": "stat_extraPhysicalBleed",
    "extra_poison_dmg_to_poisoned_percent": "stat_extraPoisonPoisoned",
    "faster_cast_rate_percent": "stat_faster_cast_rate",
    "faster_cast_rate_percent_converted_to_movement_speed_percent": "stat_fcr_movement_speed",
    "faster_hit_recovery_percent": "stat_faster_hit_recovery",
    "fire_dmg_absorbed_percent": "stat_fire_absorb",
    "fire_resist_percent": "stat_fire_resistance",
    "fire_skill_dmg_flat": "stat_spell_fire_damage",
    "fire_skill_dmg_percent": "stat_fire_damage_p",
    "fire_skills_flat": "stat_fire_skills",
    "follower_relic_attack_speed_percent": "stat_relic_follower_attack_speed",
    "follower_relic_damage_flat": "stat_relic_follower_damage_f",
    "follower_relic_damage_percent": "stat_relic_follower_damage_p",
    "follower_relic_projectiles_flat": "stat_relic_follower_projectiles",
    "forking_projectiles": "stat_forking_projectile",
    "frostbite_damage_percent": "stat_increased_frost_bite_damage",
    "guardian_additional_attack_percent": "stat_guardian_burst",
    "guardian_attack_range_percent": "stat_guardian_attack_range",
    "guardian_attack_speed_percent": "stat_guardian_attack_speed",
    "guardian_damage_percent": "stat_guardian_damage",
    "guardian_duration_percent": "stat_guardian_duration",
    "half_freeze_duration_none": "stat_half_freeze_duration",
    "increased_experience_gain_below_100_percent": "stat_exp_gain_below",
    "increased_experience_gain_percent": "stat_exp_gain",
    "intelligence_based_on_level_flat": "stat_intelligence_per_level",
    "intelligence_converted_to_arcane_skill_dmg_percent": "stat_intelligence_arcane",
    "intelligence_converted_to_cold_skill_dmg_percent": "stat_intelligence_cold",
    "intelligence_converted_to_fire_skill_dmg_percent": "stat_intelligence_fire",
    "intelligence_converted_to_lightning_skill_dmg_percent": "stat_intelligence_lightning",
    "intelligence_converted_to_magic_skill_dmg_percent": "stat_intelligence_skill_damage",
    "intelligence_flat": "stat_intelligence",
    "intelligence_increase_percent": "stat_intelligence_p",
    "jumping_power_percent": "stat_jump_power",
    "less_dmg_with_cd_skills_percent": "stat_cdr_damage",
    "life_after_kill_flat": "stat_hp_kill",
    "life_based_on_level_flat": "stat_life_per_level",
    "life_flat": "stat_life",
    "life_per_second_flat": "stat_life_per_second",
    "life_percent": "stat_max_hp",
    "life_replenish_when_blocking_percent": "stat_life_after_block_value",
    "life_stolen_per_hit_percent": "stat_life_per_hit",
    "light_radius_flat": "stat_light_radius",
    "lightning_dmg_absorbed_percent": "stat_lightning_absorb",
    "lightning_resist_percent": "stat_lightning_resistance",
    "lightning_skill_dmg_flat": "stat_spell_lightning_damage",
    "lightning_skill_dmg_percent": "stat_lightning_damage_p",
    "lightning_skills_flat": "stat_lightning_skills",
    "magic_dmg_absorbed_percent": "stat_magic_absorb",
    "magic_find_based_on_level_percent": "stat_magic_find_per_level",
    "magic_find_percent": "stat_magic_find",
    "magic_skill_dmg_flat": "stat_spell_elemental_damage",
    "magic_skill_dmg_percent": "stat_skill_damage",
    "mana_after_kill_flat": "stat_mana_kill",
    "mana_based_on_level_flat": "stat_mana_per_level",
    "mana_costs_decreased_percent": "stat_mana_cost",
    "mana_flat": "stat_mana",
    "mana_per_second_flat": "stat_mana_per_second",
    "mana_percent": "stat_max_mana",
    "mana_stolen_per_hit_percent": "stat_mana_per_hit",
    "max_all_resist_percent": "stat_max_all_resistance",
    "max_arcane_resist_percent": "stat_max_magic_resistance",
    "max_cold_resist_percent": "stat_max_cold_resistance",
    "max_fire_resist_percent": "stat_max_fire_resistance",
    "max_lightning_resist_percent": "stat_max_lightning_resistance",
    "max_poison_resist_percent": "stat_max_poison_resistance",
    "max_weapon_damage_flat": "stats_max_damage",
    "max_weapon_damage_percent": "stats_max_damage_p",
    "melee_skills_flat": "stat_melee_skills",
    "min_weapon_damage_flat": "stats_min_damage",
    "mirror_ring_none": "stat_copy_slot",
    "monsters_rest_in_peace": "stat_monsters_rest_in_peace",
    "movement_phasing_none": "stat_phasing",
    "movement_speed_percent": "stat_increased_speed",
    "open_wound_chance_percent": "stat_open_wounds",
    "orbit_skills_flat": "stat_orbit_skills",
    "orbital_projectile_duration_increased_percent": "stat_orbit_duration",
    "orbiting_relic_amount_flat": "stat_relic_orbit_amount",
    "orbiting_relic_damage_flat": "stat_relic_orbit_damage_f",
    "orbiting_relic_execute_below_health_percent": "stat_relic_orbit_execute",
    "orbiting_relic_radius_increase_percent": "stat_relic_orbit_radius",
    "orbiting_relic_size_percent": "stat_relic_orbit_size",
    "phys_dmg_taken_as_arcane_percent": "stat_phys_taken_magic",
    "phys_dmg_taken_as_cold_percent": "stat_phys_taken_cold",
    "physical_skills_flat": "stat_physical_skills",
    "piercing_attack_none": "stat_piercing",
    "poison_damage_percent": "stat_increased_poisoned_damage",
    "poison_dmg_absorbed_percent": "stat_poison_absorb",
    "poison_length_reduced_percent": "stat_poison_length_reduced",
    "poison_resist_percent": "stat_poison_resistance",
    "poison_skill_dmg_flat": "stat_spell_poison_damage",
    "poison_skill_dmg_percent": "stat_poison_damage_p",
    "poison_skills_flat": "stat_poison_skills",
    "poison_tick_frequency_percent": "stat_poison_frequency",
    "proc_non_buff_relic_chance_percent": "stat_relic_proc_rate",
    "projectile_damage_increase": "stat_projectile_damage_p",
    "projectile_random_direction": "stat_projectile_direction",
    "projectile_return": "stat_projectile_return",
    "projectile_size_increase": "stat_projectile_size",
    "projectile_skills_flat": "stat_projectile_skills",
    "projectile_speed_flat": "stat_projectileSpeed",
    "random_skill_element_flat": "stat_random_skill",
    "ranged_skills_flat": "stat_ranged_skills",
    "reduced_all_dmg_taken_percent": "stat_all_damage_reduction",
    "reduced_dmg_taken_flat": "stat_damage_reduced",
    "reduced_magic_dmg_taken_percent": "stat_magic_damage_reduction",
    "reduced_physical_dmg_taken_percent": "stat_damage_reduction",
    "replenish_life_percent": "stat_hp_rep",
    "replenish_mana_percent": "stat_mana_rep",
    "self_inflicted_chance_when_attacking": "stat_self_strike_chance",
    "self_inflicted_chance_when_casting_flat": "stat_self_strike_cast_chance",
    "self_inflicted_damage_from_attacking_flat": "stat_self_strike_damage",
    "self_inflicted_damage_from_casting_flat": "stat_self_strike_cast_damage",
    "sentry_amount": "stat_sentry_amount",
    "sentry_attack_speed_percent": "stat_sentry_attack_speed",
    "sentry_damage_percent": "stat_sentry_damage",
    "sentry_duration": "stat_sentry_duration",
    "sentry_skills_flat": "stat_sentry_skills",
    "shadowburn_damage_percent": "stat_increased_shadow_burn_damage",
    "skill_movement_diminish_percent": "stat_movement_diminish",
    "skill_relic_additional_casts": "stat_relic_skill_burst",
    "skill_relic_cooldown_percent": "stat_relic_skill_cooldown_dec",
    "skill_relic_damage_flat": "stat_relic_skill_damage_f",
    "skill_relic_damage_percent": "stat_relic_skill_damage_p",
    "skill_relic_projectile_size_percent": "stat_relic_skill_size",
    "skill_relic_projectiles_flat": "stat_relic_skill_projectiles",
    "slows_target_percent": "stat_slow_target",
    "socketed_flat": "sockets",
    "stasis_damage_percent": "stat_increased_stasis_damage",
    "strength_based_on_level_flat": "stat_strength_per_level",
    "strength_flat": "stat_strength",
    "strength_increase_percent": "stat_strength_p",
    "summon_all_resist_percent": "stat_summon_resistances",
    "summon_amount_flat": "stat_summon_amount",
    "summon_attack_speed_percent": "stat_summon_attack_speed",
    "summon_dmg_percent": "stat_summon_damage",
    "summon_life_percent": "stat_summon_life",
    "summon_movement_speed_percent": "stat_summon_movement_speed",
    "summon_reduced_dmg_taken_percent": "stat_summon_damage_reduction",
    "summon_skills_flat": "stat_summon_skills",
    "target_defense_ignored_percent": "stat_target_defense",
    "to_subskills_flat": "stat_sub_skills",
    "total_attack_speed_percent": "stat_attack_speed_t",
    "total_faster_cast_rate_percent": "stat_faster_cast_rate_t",
    "vitality_based_on_level_flat": "stat_vitality_per_level",
    "vitality_converted_to_attack_dmg_percent": "stat_vitality_attack_damage",
    "vitality_converted_to_magic_skill_dmg_percent": "stat_vitality_skill_damage",
    "vitality_flat": "stat_vitality",
    "vitality_increase_percent": "stat_vitality_p",
}


def game_names() -> dict[str, str]:
    """tkey -> English name, as the game itself shows it."""
    path = GAME / "translationsItem.csv"
    if not path.exists():
        print(f"note: {path} not found — falling back to the datamined names")
        return {}
    names = {}
    for line in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
        key, _, rest = line.partition("|")
        key = key.strip()
        if key and not key.startswith("[") and rest:
            name = rest.split("|")[0].strip()
            if name:
                names[key] = name
    return names


def room_names() -> dict[str, str]:
    """room -> the name the game shows for it.

    The client's heartbeat says where the character is by room: `Act_05_03`,
    `Town_01_rm`, `Shadow_Realm_rm`. The tracker used to turn those into
    "Act 5 . Zone 3" by arithmetic and anything else into the raw name with its
    underscores swapped for spaces, which put "Shadow Realm rm" in front of the
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
    for path in files:
        for line in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
            key, _, rest = line.partition("|")
            key = key.strip()
            if not key or not rest:
                continue
            if not (re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*_rm", key) or re.fullmatch(r"Act_\d+_\d+", key)):
                continue
            name = rest.split("|")[0].strip()
            if name:
                out.setdefault(key, name)
    return out

def attribute_names() -> dict[str, str]:
    """key -> English label, as the game words the stat on a tooltip."""
    path = GAME / "translationsAttributes.csv"
    if not path.exists():
        print(f"note: {path} not found — item stats will carry no labels")
        return {}
    # The file is in sections and the section headings are skipped: no key
    # appears in two of them, and the game names two of the stats we want
    # ("Attacks per Second", "Sockets") under [Global Stats] rather than
    # [Item Stats].
    names = {}
    for line in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
        key, _, rest = line.partition("|")
        key = key.strip()
        if key and not key.startswith("[") and rest:
            label = rest.split("|")[0].strip()
            if label:
                names[key] = label
    return names


translations = game_names()
attributes = attribute_names()
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

items, rarities, tiers, drops, zones, places, chases, rolls = {}, {}, {}, {}, {}, {}, {}, {}
# What each identity says about itself, beside the name it goes by. See by_id.
facts: dict[str, tuple[str, str, int]] = {}
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
        facts.setdefault(f"{item_type}:{game_id}:{weapon_type}", (name, rarity, tier))
    if entry.get("stats"):
        rolls.setdefault(name.lower(), entry["stats"])

stat_ids: list[str] = []
stat_index: dict[str, int] = {}


def stat_row(roll: dict) -> list:
    """One rolled stat as [id, min, max, min2, max2, spell or class]."""
    sid = str(roll.get("sid"))
    if sid not in stat_index:
        stat_index[sid] = len(stat_ids)
        stat_ids.append(sid)
    # a proc line names the spell it casts; a class-only bonus names the class
    named = roll.get("Spell Name") or roll.get("Class Name")
    row = [stat_index[sid], roll.get("min1"), roll.get("max1"), roll.get("min2"), roll.get("max2"), named]
    # Interning the ids and cutting the columns a roll does not use is most of
    # what keeps this table near a hundred kilobytes rather than four hundred.
    while len(row) > 2 and row[-1] is None:
        row.pop()
    return row


# Stats are carried only for a name the item table knows, because the page that
# shows them has no way to reach any other item.
shown = {name.lower() for name in items.values()}
stats = {key: [stat_row(r) for r in rs] for key, rs in rolls.items() if key in shown}
stat_labels = [attributes.get(STAT_KEYS.get(sid, ""), "") for sid in stat_ids]

for sid, key in sorted(STAT_KEYS.items()):
    if attributes and key not in attributes:
        # a season that renames a key would otherwise take the label away in
        # silence, and the page would quietly fall back to the raw id
        print(f"warning: {sid} is mapped to {key!r}, which the attribute table does not carry")

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


def only_notable(claims: set, field: int):
    """The one claim a notable rarity makes about this, when only one does."""
    mine = [c for c in claims if c[0] in NOTABLE]
    return mine[0][field] if len(mine) == 1 else None


muddled = {
    "rarity": {n for n, c in claimed.items() if disagree(c, 0)},
    "tier": {n for n, c in claimed.items() if disagree(c, 1)},
    "rate": {n for n, c in claimed.items() if disagree(c, 2)},
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
for field, names in muddled.items():
    if names:
        print(f"note: {len(names)} names two items disagree about, so no {field}: {', '.join(sorted(names))}")
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
    key: (name, rarity, tier)
    for key, (name, rarity, tier) in facts.items()
    if rarity and (rarities.get(name.lower()) != rarity or tiers.get(name.lower()) != tier)
}
print(f"note: {len(by_id)} identities the name tables cannot answer for: "
      f"{', '.join(sorted({n for n, _, _ in by_id.values()}))}")

rs_rooms = sorted(rooms.items())

rooms_js = json.dumps(rooms, ensure_ascii=False, separators=(",", ":"))

out = rf"""{header}
// Item identity is (type, gameId, weaponType); key "type:id:wt".

export const ITEMS = {json.dumps(items, ensure_ascii=False, separators=(",", ":"))};

export const RARITY_BY_NAME = {json.dumps(rarities, ensure_ascii=False, separators=(",", ":"))};

export const TIER_BY_NAME = {json.dumps(tiers, ensure_ascii=False, separators=(",", ":"))};

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
rs_by_id = sorted((rs_key(k), n, r, t or 0) for k, (n, r, t) in by_id.items())
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
OUT_RS.write_text("\n".join(rs_lines), encoding="utf-8", newline="\n")

unnamed = [sid for sid, label in zip(stat_ids, stat_labels) if not label]
print(
    f"items: {len(items)}, rarities: {len(rarities)}, grades: {len(tiers)}, "
    f"drop rates: {len(drops)}, zones: {len(zones)}, chase rates: {len(chases)} -> {OUT.name}, {OUT_RS.name}"
)
print(
    f"stats: {sum(len(v) for v in stats.values())} rolls on {len(stats)} items, "
    f"{len(stat_ids) - len(unnamed)} of {len(stat_ids)} stat ids named by the game -> {OUT.name} only"
)
if unnamed:
    print("  no label in the attribute table: " + ", ".join(sorted(unnamed)))
