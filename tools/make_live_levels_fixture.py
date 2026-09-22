#!/usr/bin/env python3
"""REGENERATE crates/royalesim/tests/fixtures/live_levels.json.

    python tools/make_live_levels_fixture.py            # rewrite
    python tools/make_live_levels_fixture.py --check    # diff only (exit 1 on a difference)

    ROYALELIVE_REPORTS  the captures folder (required; there is no default)

WHAT IT HOLDS
    Every distinct (card, level, max_hp) the LIVE client (CR 16.402) published across all the
    ground-truth captures (*.native.oracle.jsonl.gz under ROYALELIVE_REPORTS: every entity's
    `card_id`, `level` and `max_hp` per frame), with how many frames carried it. `card_id` is
    Supercell's global id -- class x 1_000_000 + the row index of the card's spells_*.csv
    (26 spells_characters, 27 spells_buildings, 28 spells_other, 203 spells_hero_form whose
    rows are the base cards' with a `_hero` suffix) -- resolved against the 15.535.29 files.
    Towers (card_id -1) are kept as `tower: true` rows for the TOWER ladder, a key of its own
    (calibration combat.TOWER_HITPOINT_LADDER; globals HITPOINT_INCREASE_PERCENT_PER_*_LEVEL):
    2400 / 1400 -> 3312 / 2030 at tower level 6, 4824 / 3052 at 11.

    A row's `unit` is the object of that card whose 15.535 base hitpoints, on the ladder
    cards.json gives it (level_scaling: the OBJECT's rarity from `base_level`, floor), equal
    the live max_hp: the card's own unit, or a unit it releases (spawner, death spawn, spell
    spawn, second summon, the rolling projectile's SpawnCharacter). A row no object matches
    is kept with `unit: null` and `note`: it is either a 15.535 -> 16.402 balance change
    (Ice Golem 514 -> 480, Ice Spirit 85 -> 84, Goblin Brawler 422 -> 438) or a
    unit this schema does not reach (the Goblin Drill's building) -- data for the next
    vintage, never silently dropped.

    WHY: this is the measurement behind calibration.json combat.STAT_BASE_LEVEL and the
    floor in level scaling. The Rust gate is tests/levels.rs: for every resolved row whose
    card the loader simulates, `CardDb::scaled(unit, level, hitpoints)` must equal max_hp.

    A capture name carries the seat it was recorded from as a 5-digit tag; the fixture names
    the seats A, B, ... in sort order within the file (tools/make_client16402_paths_fixture.py
    `seat_letters`) and drops the frames- / frames-auto- prefix and the file suffix, so a
    capture entry is "20260920-002736-A" and nothing else.
"""
from __future__ import annotations

import csv
import glob
import gzip
import json
import os
import re
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIVE = os.environ.get("ROYALELIVE_REPORTS")
SUFFIX = ".native.oracle.jsonl.gz"
SEAT_TAG = re.compile(r"-(\d{5})(?=[.:])")
RAW = os.path.join(ROOT, "data", "raw", "cr-15.535.29", "csv_logic")
CARDS = os.path.join(ROOT, "data", "derived", "cards.json")
OUT = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "live_levels.json")
CLASSES = {26: "spells_characters.csv", 27: "spells_buildings.csv", 28: "spells_other.csv", 203: "spells_hero_form.csv"}
# 16.402 balance changes against the 15.535 rows, read off the misses.
KNOWN_DELTAS = {
    "IceGolemite": "16.402 Ice Golem hitpoints 480 (15.535: 514): 1228 = 480 x 256 %",
    "IceSpirits": "16.402 Ice Spirit hitpoints 84 (15.535: 85): 215 = 84 x 256 %",
    "FirespiritHut": "the Furnace's Fire Spirit: 16.402 hitpoints 84 (15.535 FireSpirits 85), released through an action graph the spawner block does not walk",
    "Heal": "the Heal Spirit: 16.402 hitpoints 84 (15.535 HealSpirit 85); the 15.535 Heal card has no projectile and no area effect (an action graph), so no object is reached",
    "GoblinCage": "16.402 Goblin Brawler hitpoints 438 (15.535: 422): 1121 = 438 x 256 %",
    "GoblinDrill": "the drill building (max_hp 1313 at 11) and its Goblins are spawned through an action graph this schema does not walk",
}


def seat_letters(names: list[str]) -> dict[str, str]:
    tags = sorted({m.group(1) for n in names for m in [SEAT_TAG.search(n)] if m})
    return {tag: chr(ord("A") + i) for i, tag in enumerate(tags)}


def capture_name(raw: str, seats: dict[str, str]) -> str:
    """The public name of a capture file: prefix and suffix dropped, the seat tag a letter."""
    name = SEAT_TAG.sub(lambda m: "-" + seats[m.group(1)], raw)
    name = name.removesuffix(SUFFIX)
    return name.removeprefix("frames-auto-").removeprefix("frames-")


def row_names(f: str) -> list[str]:
    with open(os.path.join(RAW, f), encoding="utf-8-sig") as fh:
        rows = list(csv.reader(fh))
    return [r[0].strip() for r in rows[2:] if r and r[0].strip()]


def resolve_id(tables: dict[int, list[str]], card_id: int) -> str | None:
    cls, ix = divmod(card_id, 1_000_000)
    names = tables.get(cls)
    if names is None or ix >= len(names):
        return None
    name = names[ix]
    if cls == 203 and name.endswith("_hero"):
        name = name[: -len("_hero")]
    return name


def reachable_units(doc: dict, card: dict) -> dict[str, int]:
    """name -> base hitpoints of every object of `card` with hitpoints."""
    units = doc["units"]
    out: dict[str, int] = {}
    if card.get("hitpoints") is not None:
        out[card.get("summon_character", card["name"])] = card["hitpoints"]

    def add(name: str | None, depth: int = 0) -> None:
        u = units.get(name) if name else None
        if u is None:
            return
        if u["hitpoints"] is not None:
            out.setdefault(name, u["hitpoints"])
        if depth < 2:
            add((u.get("death_spawn") or {}).get("character"), depth + 1)
            add((u.get("spawner") or {}).get("character"), depth + 1)

    spell = card.get("spell") or {}
    proj = card.get("projectile") or {}
    for ref in (
        card.get("summon_character"),
        (card.get("second_summon") or {}).get("character"),
        (card.get("spawner") or {}).get("character"),
        (card.get("death_spawn") or {}).get("character"),
        (spell.get("spawn") or {}).get("character"),
        proj.get("spawn_character"),
        (proj.get("spawn_projectile") or {}).get("spawn_character"),
    ):
        add(ref)
    return out


def ladder_percent(doc: dict, card: dict, unit: str, level: int) -> int | None:
    """The percent cards.json's blocks give `unit` of `card` at unified `level`."""
    ls = card["level_scaling"]
    if unit == card.get("summon_character", card["name"]) and card.get("hitpoints") is not None:
        base_level = ls.get("base_level", doc["rarities"][ls["rarity"]]["relative_level"] + 1)
        table = ls["multiplier_percent_by_level"]
    else:
        r = doc["rarities"][doc["units"][unit]["rarity"]]
        base_level = r["relative_level"] + 1
        table = r["multiplier_percent_by_level"]
    ix = level - base_level
    return table[ix] if 0 <= ix < len(table) else None


def build() -> dict:
    tables = {cls: row_names(f) for cls, f in CLASSES.items()}
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    seen: Counter = Counter()
    files = []
    raw_names = sorted(os.path.basename(f) for f in glob.glob(os.path.join(LIVE, "*" + SUFFIX)))
    seats = seat_letters(raw_names)
    for f in sorted(glob.glob(os.path.join(LIVE, "*" + SUFFIX))):
        files.append(capture_name(os.path.basename(f), seats))
        with gzip.open(f, "rt", encoding="utf-8") as fh:
            for line in fh:
                d = json.loads(line)
                if d.get("record") != "frame":
                    continue
                for e in d["state"]["entities"]:
                    cid, level, mh = e.get("card_id"), e.get("level"), e.get("max_hp")
                    if cid is None or level is None or mh is None or mh <= 0:
                        continue
                    # towers carry card_id -1 and are told apart by kind (12 king, 13 princess)
                    seen[(cid, e.get("kind") if cid < 0 else None, level, mh)] += 1
    rows = []
    for (cid, kind, level, mh), n in sorted(seen.items(), key=lambda kv: (kv[0][0], kv[0][2], kv[0][3], kv[0][1] or 0)):
        if cid < 0:
            rows.append({"card_id": cid, "card": None, "tower": True, "kind": kind, "level": level, "max_hp": mh, "frames": n, "unit": None, "note": "a crown tower (the captures' kind 12 / 13 does not separate king from princess; the king is the larger max_hp per level: 3312 / 2030 at 6, 4824 / 3052 at 11 against the 2400 / 1400 rows) -- the tower ladder is combat.TOWER_HITPOINT_LADDER (globals *_PER_TOWER_LEVEL / *_PER_KING_LEVEL)"})
            continue
        name = resolve_id(tables, cid)
        row = {"card_id": cid, "card": name, "level": level, "max_hp": mh, "frames": n, "unit": None}
        card = cards.get(name or "")
        if card is None:
            row["note"] = "no base card row for this id in the 15.535 files"
        else:
            for unit, base in reachable_units(doc, card).items():
                pct = ladder_percent(doc, card, unit, level)
                if pct is not None and base * pct // 100 == mh:
                    row["unit"] = unit
                    row["base_hitpoints"] = base
                    row["percent"] = pct
                    break
            if row["unit"] is None:
                row["note"] = KNOWN_DELTAS.get(name, "no object of this card matches: a 16.402 balance change or an unreached unit")
        rows.append(row)
    matched = sum(1 for r in rows if r["unit"])
    return {
        "generated_by": "tools/make_live_levels_fixture.py",
        "source": "live captures of the real game (CR 16.402), every entity's card_id / level / max_hp per frame; ids resolved against data/raw/cr-15.535.29/csv_logic/spells_*.csv row order",
        "captures": files,
        "cards_json_version": doc["version"],
        "level_base_reading": doc["provenance"].get("level_base_reading"),
        "reading": "max_hp == floor(base_hitpoints x percent / 100) with percent = the OBJECT's rarity ladder at unified level - its RelativeLevel (cards.json level_scaling.base_level); a row with unit null is a 16.402 balance delta or an unreached object, listed in `note`",
        "rows_matched": matched,
        "rows_total": len(rows),
        "rows": rows,
    }


def main() -> int:
    check = "--check" in sys.argv[1:]
    if not LIVE:
        print("set ROYALELIVE_REPORTS to the folder holding the *" + SUFFIX + " captures", file=sys.stderr)
        return 2
    if not os.path.isdir(LIVE):
        print(f"no captures at {LIVE} (ROYALELIVE_REPORTS)", file=sys.stderr)
        return 2
    doc = build()
    text = json.dumps(doc, indent=1) + "\n"
    if check:
        old = ""
        if os.path.exists(OUT):
            with open(OUT, encoding="utf-8") as fh:
                old = fh.read()
        if old != text:
            print("live_levels.json differs from the captures; rerun without --check", file=sys.stderr)
            return 1
        print("live_levels.json is current")
        return 0
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    unmatched = [r for r in doc["rows"] if not r["unit"] and not r.get("tower")]
    print(f"wrote {OUT}: {doc['rows_matched']} / {doc['rows_total']} rows matched from {len(doc['captures'])} captures")
    for r in unmatched:
        print(f"  unmatched {r['card']} L{r['level']} max_hp {r['max_hp']} x{r['frames']}: {r['note']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
