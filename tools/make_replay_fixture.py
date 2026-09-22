#!/usr/bin/env python3
"""A live capture -> a whole-battle REPLAY FIXTURE (script + truth) for replay_parity.rs.

    python tools/make_replay_fixture.py <capture.native.oracle.jsonl.gz>
                    [--placements <p.jsonl> ...] [--out <dir>] [--truth-stride N] [--until-tick T]
    python tools/make_replay_fixture.py --all [--reports <dir>] [--out <dir>] [--truth-stride N]
    python tools/make_replay_fixture.py <capture> --check <fixture.json>   # STALE or current

    The committed sample crates/royalesim/tests/fixtures/replay/sample.json is this tool run on
    the 20260920-003751 seat-B capture with `--until-tick 1440`; `--check` on that capture
    answers whether the sample is still what the maker produces. `--check` ignores the `census`
    note, which records the state of the run rather than anything about the battle.

    ROYALELIVE_REPORTS  the captures folder (the default for --reports; required for
                        --all when --reports is not given)

    Default output dir: data/derived/replay/ (gitignored). `--all` walks every
    *.native.oracle.jsonl.gz in --reports and writes the MANIFEST (manifest.json)
    beside the fixtures: every capture, playable or not, with why.

WHAT A FIXTURE IS
    The capture is a per-tick record of the real game's entity table (the ground-truth
    captures of the live client, CR 16.402): both sides, whole battle. This tool splits
    it into

      script   what a player DID: the decks, the tower level and starting hp, each
               side's card levels, and the DEPLOY LIST -- one deploy per group of the
               card's OWN units that first appears on one tick with one (side, card),
               at that tick, plus the spell casts the capture's `effects` stream shows.
      truth    what the game then SHOWED: per frame tick, per entity, the columns the
               harness scores (side, card, x, y, hp, alive, target, path-node count,
               behavior_state), run-length encoded per entity column.

    replay_parity.rs plays the script through the engine and scores the engine's
    entities against the truth, entity by entity, tick by tick.

THE SIDE CONVENTION (written into every fixture as `frame`)
    Native side 0 = engine Blue, native side 1 = engine Red, and native millitiles map
    to engine subtiles by the identity scale (x18): in every capture native side 0
    defends LOW y (its king at y 3000, the header's `towers`), which is exactly what
    Blue does in the engine (lib.rs `Team`: "Blue defends the low-y side"). So nothing
    is rotated. If a capture ever recorded side 0 at the top, the tool ROTATES that
    capture's every position by (W - x, H - y) in native units (18000 x 32000) and
    swaps the sides -- the seat rotation tests/common/mod.rs `mirror()` and
    arena.rs `to_frame` use -- and says so in `frame.transform`. The absolute frame is
    kept on purpose: the shipped pathfinder is the game's own absolute-grid search
    (not seat-symmetric, tests/mirror.rs), so a rotated replay would score a different
    tie-break than the game ran.

DEPLOYS VERSUS SPAWNS
    A truth entity carries its CARD's id even when it was not deployed: Tombstone
    Skeletons carry 27000009, Golemites the Golem's id, a Battle Ram's Barbarians
    26000036, a Witch's Skeletons the Witch's (spawner troops carry the spawner's card
    id in every capture). Each entity is classed by its
    `max_hp` at its `level` against the hitpoints of every object its card can put on
    the board (cards.json: the card's own summon, its second summon, its spawner /
    death-spawn / spell-released units, on the ladder cards.json gives each one --
    the same resolution tools/make_live_levels_fixture.py uses): an entity whose hp is
    the card's OWN unit's (or its second summon's) is a deploy summon; any other is a
    SPAWNED unit and is truth only, never a deploy. An hp that matches no object (a
    16.402 balance delta) takes the nearest object within NEAREST_MAX_ERROR_PERCENT
    and is flagged `hp_match: nearest`; beyond that it is an UNKNOWN OBJECT (role
    `unknown_object`, truth only, never a deploy and never paired: the game put
    something on the board under the card's id that cards.json does not derive from
    the card -- the Goblin Drill's surfaced building and its Goblins, the Furnace's
    Fire Spirits, whose spawners are action graphs the extractor does not decode). An
    unknown-object group that coincides with a deploy TAP of its card makes the fixture
    unplayable from that tick (the engine cannot reproduce the deploy). A cross-check
    is recorded per group: a deploy group of a card with a spawner or death_spawn block
    (mechanic_register.json families) is expected to be that card's own unit; a spawned
    group's tick is expected NOT to coincide with a hand card leaving (a tap for that
    card in the placements log within the tap window). The fixture records the FNV-1a
    64 hash of the cards.json it was classified against (`cards_json_fnv1a64`); the
    harness notes a mismatch with the engine's, the manifest says when the census is
    stale.

DEPLOY POSITION AND TICK
    The tick is the tick the entities came to exist with their deploy timer running
    (kind 14, behavior_state 4); the harness issues the deploy so that the engine's
    units materialise on that tick (spawn_unit the tick before). The captures miss
    frames (below), so the first frame a group is SEEN in can be later than its spawn.
    Two constraints pin it: the spawn lies in (previous frame's tick, first-seen tick],
    and the deploy-end transition (the first frame with behavior_state != 4, at tick
    t1, after a state-4 frame at t0) lies at spawn + DeployTime / 50 ms - 1 -- measured
    on every gap-free spawn of the corpus (19 ticks for the 1000 ms cards, 23
    for the Princess's 1200, 69 for the X-Bow's 3500; the action-graph huts flip at 10
    and are not simulable anyway). The intersection is the spawn tick; when it still
    holds several ticks and the group is a SINGLE unit, its first step decides
    (`first_step_spawn_tick`: the first frame off the spawn point, when the frame
    before it was seen, is spawn + DeployTime / 50) if that lands inside the range;
    else the latest is used and `tick_evidence` says "range [a, b]"; when the range is
    empty the first-seen tick is used and `tick_evidence` says so. `first_seen` keeps
    the raw frame tick.

    The position is the TAP when the placements log recorded beside the capture (the
    taps a scripted player made: side, card, tick, requested tile) has one for that
    side and card in the window [tap tick + 5, tap tick + 80] (the tap-to-entity
    latency in the captures runs 23-38 ticks) and the group has several members:
    `requested` (screen tiles)
    converted to native by the log's own rule (side 0: (18 - x, y); side 1:
    (x, 32 - y)), i.e. the tile centre the game snapped the tap to -- the centre a
    formation was laid around, which its members' centroid need not be (the game
    displaces a formation off a footprint or an edge, and the centroid then says where
    the units went, not where the player tapped; the engine is charged with that
    difference). A single unit is placed where it appeared (its centroid, which is the
    tap snapped to the game's grid). Without a tap the centroid is used. `source` says
    which. A spell cast comes from the `effects` stream (`spell_casts`: the stream
    lists every projectile object on every frame it exists, so ONE cast is the run of
    class-28 objects of one (side, card) with no gap over CAST_GAP_TICKS between
    sightings -- a Fireball seen on 15 frames, a Log's airborne object then its rolling
    object, an Arrows volley of 9-30 objects on one tick are one cast each; its tick
    is the first frame and its aim point the mean of the first frame's projectile
    targets: a point spell's exact tap, a Log's landing = the roll's start, a volley's
    centre; `cast_objects` / `cast_frames` / `aim` record the evidence) or, for a
    spell the effects stream does not show (no projectile: Zap, Rage, Freeze, ...),
    from the tap plus this capture's median tap latency, flagged `timing: estimated`; a spell tap
    with no latency measurement is listed under `unresolved` instead of guessed.

FRAMES
    A capture holds one frame per 50 ms tick; frames are missed now and then (tick
    deltas of 2-12) and, in the earliest captures, REPEATED while the game was frozen
    (the ledger names them). Frames are de-duplicated by tick (first wins;
    `frames_duplicate` counts the rest) and a group whose first frame follows a gap
    carries `first_seen_gap` > 1: its real spawn tick may be up to that many ticks
    earlier.

PLAYABILITY
    Unplayable, with the reason in the fixture and the manifest: a deploy card whose id
    the id table (Supercell's global ids: class x 1_000_000 + the row index of the
    card's spells_*.csv in the 15.535 files -- 26 characters, 27 buildings, 28 spells,
    203 hero forms whose rows are the base cards' with a `_hero` suffix) does not
    resolve; a deploy card the ENGINE cannot load, when a card
    census is present (data/derived/replay/card_census.json, written by
    `cargo run --example replay_parity -- --census`; the engine's loader is the only
    authority on that list, so it is not re-derived here); a tapped deploy whose entity
    is an unknown object (above); a capture that starts mid-battle with non-tower
    entities already on the board; a capture with no frames.

    Needs data/derived/cards.json; data/derived/mechanic_register.json
    (tools/mechanic_register.py) is read for the family labels and only warned about
    when absent.

NAMES
    The fixture names the seats A, B, ... in sort order over the whole run
    (tools/capture_names.py) and drops the file prefix and suffix, so `capture` is
    "20260920-003751-B" and the `placements` list the same stamps; the fixture file is
    <capture>.replay.json. A folder carrying one capture under several names contributes
    it once, and a run refuses to write two fixtures to one path.
"""

from __future__ import annotations

import argparse
import csv
import glob
import gzip
import json
import math
import os
import statistics
import sys
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capture_names import SEAT_TAG, distinct_captures, seat_letters  # noqa: E402

LIVE = os.environ.get("ROYALELIVE_REPORTS")
RAW = os.path.join(ROOT, "data", "raw", "cr-15.535.29", "csv_logic")
CARDS = os.path.join(ROOT, "data", "derived", "cards.json")
REGISTER = os.path.join(ROOT, "data", "derived", "mechanic_register.json")
OUT_DEFAULT = os.path.join(ROOT, "data", "derived", "replay")
CENSUS = "card_census.json"

FORMAT = "replay-fixture-1"
CAPTURE_SUFFIX = ".native.oracle.jsonl.gz"
# Native arena size in millitiles (18 x 32 tiles); only used to rotate a capture whose
# side 0 sits at the top, which no capture of the corpus does.
NATIVE_W, NATIVE_H = 18_000, 32_000
# Entity kinds in the captures: 12 building deploying/inactive, 13 building up, 14 troop
# deploying, 15 troop active. Towers carry card_id -1.
KIND_TROOP_DEPLOYING = 14
# Spell cards are class 28 of Supercell's global ids (class x 1_000_000 + row); the id
# table is the row order of the 15.535 spells_*.csv files per class.
SPELL_CLASS = 28
HERO_CLASS = 203
ID_CLASSES = {
    26: "spells_characters.csv",
    27: "spells_buildings.csv",
    28: "spells_other.csv",
    HERO_CLASS: "spells_hero_form.csv",
}
# Tap window: a group first seen in [tap + TAP_MIN, tap + TAP_MAX] belongs to that tap
# (measured latency 23-38 ticks over the corpus; the game also refuses the
# odd tap right after tick 150, which then matches nothing).
TAP_MIN, TAP_MAX = 5, 80
# An hp that matches no object of the card exactly takes the nearest object only within
# this relative error: the 16.402 balance deltas the corpus carries against the 15.535
# cards.json are 1.2 % (Ice Spirit 84 vs 85) and 6.6 % (Ice Golem 480 vs 514); the
# smallest hp that is a DIFFERENT object is 49 % off (the Goblin Drill's surfaced
# building 1313 against its dig troop 2560). Beyond it the entity is `unknown_object`.
NEAREST_MAX_ERROR_PERCENT = 10
# Spell casts: the effects stream lists every projectile OBJECT on every frame it
# exists (a Fireball 15 frames, a Log's airborne object then its rolling object, Arrows
# 9-30 objects on one tick). A cast is the run of objects of one (side, card) with no
# gap longer than this many ticks between sightings: the Log's airborne -> rolling
# handoff is 2 ticks, the captures' frame gaps run to 12, and a second cast of the same
# card by the same side needs the card cycled back (seconds).
CAST_GAP_TICKS = 20


# ---------------------------------------------------------------------------
# id table


def load_id_table() -> dict[int, str]:
    """Supercell global id -> card name: class x 1_000_000 + the row index of the card in
    its 15.535 spells_*.csv (ID_CLASSES); a hero-form row names its base card."""
    table: dict[int, str] = {}
    for cls, f in ID_CLASSES.items():
        path = os.path.join(RAW, f)
        if not os.path.exists(path):
            continue
        with open(path, encoding="utf-8-sig") as fh:
            rows = [r[0].strip() for r in list(csv.reader(fh))[2:] if r and r[0].strip()]
        for ix, name in enumerate(rows):
            base = name[: -len("_hero")] if cls == HERO_CLASS and name.endswith("_hero") else name
            table.setdefault(cls * 1_000_000 + ix, base)
    return table


def canon_name(name: str, card_names: set[str]) -> str:
    """A placements-log card name (display or internal) -> the cards.json name."""
    if name in card_names:
        return name
    squashed = name.replace(" ", "")
    if squashed in card_names:
        return squashed
    return name


def public_name(raw: str, seats: dict[str, str]) -> str:
    """A capture or placements-log file name as the fixture records it (module doc, NAMES)."""
    name = SEAT_TAG.sub(lambda m: "-" + seats[m.group(1)], os.path.basename(raw))
    for suffix in (CAPTURE_SUFFIX, ".jsonl"):
        name = name.removesuffix(suffix)
    for prefix in ("frames-auto-", "frames-", "placements-"):
        name = name.removeprefix(prefix)
    return name


# ---------------------------------------------------------------------------
# card objects and ladders (tools/make_live_levels_fixture.py's resolution)


def reachable_units(doc: dict, card: dict) -> dict[str, int]:
    """name -> base hitpoints of every object of `card` with hitpoints."""
    units = doc["units"]
    out: dict[str, int] = {}
    if card.get("hitpoints") is not None:
        out[card.get("summon_character") or card["name"]] = card["hitpoints"]

    def add(name, depth=0):
        u = units.get(name) if name else None
        if u is None:
            return
        if u.get("hitpoints") is not None:
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


def own_units(card: dict) -> set[str]:
    """The objects a DEPLOY of `card` puts on the board: its summon and second summon."""
    out = {card.get("summon_character") or card["name"]}
    second = (card.get("second_summon") or {}).get("character")
    if second:
        out.add(second)
    return out


def ladder_percent(doc: dict, card: dict, unit: str, level: int):
    ls = card["level_scaling"]
    if unit == (card.get("summon_character") or card["name"]) and card.get("hitpoints") is not None:
        base_level = ls.get("base_level", doc["rarities"][ls["rarity"]]["relative_level"] + 1)
        table = ls["multiplier_percent_by_level"]
    else:
        r = doc["rarities"][doc["units"][unit]["rarity"]]
        base_level = r["relative_level"] + 1
        table = r["multiplier_percent_by_level"]
    ix = level - base_level
    return table[ix] if 0 <= ix < len(table) else None


def classify_unit(
    doc: dict, card: dict | None, level: int, max_hp: int
) -> tuple[str | None, bool, str]:
    """(object name, is a deploy summon, how the hp matched: exact | nearest | no_card |
    no_object | unknown_object). `unknown_object`: no object of the card comes within
    NEAREST_MAX_ERROR_PERCENT of the hp (the nearest and its error are in the string):
    the game put something on the board under this card's id that cards.json does not
    derive from the card (an action-graph spawner's emission, a form the loader has no
    object for) -- truth only, never a deploy."""
    if card is None:
        return None, True, "no_card"
    own = own_units(card)
    best = None
    for unit, base in reachable_units(doc, card).items():
        pct = ladder_percent(doc, card, unit, level)
        if pct is None:
            continue
        hp = base * pct // 100
        if hp == max_hp:
            return unit, unit in own, "exact"
        err = abs(hp - max_hp) * 100 // max(max_hp, 1)
        if best is None or err < best[0]:
            best = (err, unit, hp)
    if best is None:
        return None, True, "no_object"
    err, unit, hp = best
    if err > NEAREST_MAX_ERROR_PERCENT:
        return None, False, f"unknown_object (nearest {unit} {hp} at level {level}, {err}% off)"
    return unit, unit in own, "nearest"


def fnv1a64(data: bytes) -> str:
    """FNV-1a 64 as 16 hex digits (the harness computes the same, `harness.rs fnv1a64`)."""
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


# ---------------------------------------------------------------------------
# capture reading


def read_capture(path: str):
    header = None
    frames = []
    with gzip.open(path, "rt", encoding="utf-8") as fh:
        for line in fh:
            d = json.loads(line)
            if d.get("record") == "header":
                header = d
            elif d.get("record") == "frame":
                frames.append(d["state"])
    return header, frames


def dedupe(frames):
    """First frame per tick wins; frames must come in non-decreasing tick order."""
    out, seen, dup, back = [], set(), 0, 0
    last = -1
    for f in frames:
        t = f["tick"]
        if t in seen:
            dup += 1
            continue
        if t < last:
            back += 1
            continue
        seen.add(t)
        out.append(f)
        last = t
    return out, dup, back


def spawn_tick_bounds(ticks: list[int], first_index: int) -> tuple[int, int]:
    """(previous frame tick + 1, first-seen tick): where the spawn can lie."""
    lo = ticks[first_index - 1] + 1 if first_index > 0 else ticks[first_index]
    return lo, ticks[first_index]


def first_step_spawn_tick(
    ticks: list[int], positions: list[tuple[int, int, int]], d: int
) -> int | None:
    """The spawn tick the FIRST STEP pins, or None: a single unit steps first on spawn
    + DeployTime / 50 (movement.DEPLOY_TIMING; 432 of 432 gap-free single-unit spawns
    of the corpus that stepped at all), so when the first frame off the spawn
    point (t_m) follows a seen frame still on it (t_m - 1), the spawn is t_m - d. When
    frames were missed between the last frame on the point and t_m, the steps taken
    by t_m are counted from the unit's own per-tick displacement on the NEXT seen
    frame (a fresh walk is straight and uniform): k steps within a quarter step of
    k x that displacement, 1 <= k <= the gap, put the first step at t_m - (k - 1). Not
    read for formation members: the game's separation pushes them off their point
    while they still deploy. `positions` = [(frame index, x, y), ...] for the member."""
    if d < 1 or not positions:
        return None
    _, x0, y0 = positions[0]
    for k in range(1, len(positions)):
        fi, x, y = positions[k]
        if (x, y) == (x0, y0):
            continue
        pfi = positions[k - 1][0]
        gap = ticks[fi] - ticks[pfi]
        if gap == 1:
            return ticks[fi] - d
        if k + 1 >= len(positions):
            return None
        nfi, nx, ny = positions[k + 1]
        dt = ticks[nfi] - ticks[fi]
        if not 1 <= dt <= 3:
            return None
        per_tick = math.hypot(nx - x, ny - y) / dt
        d0 = math.hypot(x - x0, y - y0)
        if per_tick <= 0:
            return None
        steps = round(d0 / per_tick)
        if 1 <= steps <= gap and abs(d0 - steps * per_tick) <= per_tick / 4:
            return ticks[fi] - (steps - 1) - d
        return None
    return None


def refine_spawn_tick(
    ticks: list[int],
    first_index: int,
    states: list[tuple[int, int]],
    deploy_ms: int | None,
    tick_ms: int = 50,
    positions: list[tuple[int, int, int]] | None = None,
) -> tuple[int, int, str]:
    """(spawn tick, first-seen tick, evidence) from the frame gap, the deploy-end
    transition and -- for a single unit, `positions` given -- the first step.
    `states` = [(frame index, behavior_state), ...] for one member."""
    lo, hi = spawn_tick_bounds(ticks, first_index)
    if lo == hi:
        return hi, hi, "exact"
    d = (deploy_ms or 0) // tick_ms
    if d >= 1 and states and states[0][1] == 4:
        tr = next((k for k, (_, st) in enumerate(states) if st != 4), None)
        if tr is not None and tr > 0:
            t0, t1 = ticks[states[tr - 1][0]], ticks[states[tr][0]]
            a, b = t0 + 1 - (d - 1), t1 - (d - 1)
            lo2, hi2 = max(lo, a), min(hi, b)
            if lo2 <= hi2:
                if lo2 == hi2:
                    return hi2, hi, "exact (deploy-end transition)"
                step = first_step_spawn_tick(ticks, positions or [], d)
                if step is not None and lo2 <= step <= hi2:
                    return step, hi, f"exact (first step, in the transition range [{lo2}, {hi2}])"
                return hi2, hi, f"range [{lo2}, {hi2}] (deploy-end transition), latest used"
            return (
                hi,
                hi,
                f"first-seen tick: the deploy-end transition [{a}, {b}] contradicts the frame gap"
                f" [{lo}, {hi}]",
            )
    return hi, hi, f"range [{lo}, {hi}] (frame gap, no transition seen), latest used"


def rle(values) -> list:
    """[v0, run0, v1, run1, ...]."""
    out: list = []
    for v in values:
        if out and out[-2] == v:
            out[-1] += 1
        else:
            out.extend([v, 1])
    return out


# ---------------------------------------------------------------------------
# placements


def placement_files_for(capture: str, reports_dir: str) -> list[str]:
    """Both sides' placement logs of the battle this capture recorded: same stamp, either
    seat; a twin recorded a couple of seconds later has a stamp within 10 s."""
    base = os.path.basename(capture)
    if not base.startswith("frames-"):
        return []
    stamp = SEAT_TAG.sub("", base)[len("frames-") :].split(".")[0]
    stamp = stamp[len("auto-") :] if stamp.startswith("auto-") else stamp
    if len(stamp) < 15:
        return []
    day, hms = stamp[:8], stamp[9:15]
    want = int(hms[:2]) * 3600 + int(hms[2:4]) * 60 + int(hms[4:6])
    out = []
    for f in sorted(glob.glob(os.path.join(reports_dir, f"placements-{day}-*.jsonl"))):
        s = os.path.basename(f)[len("placements-") + 9 :][:6]
        if not s.isdigit():
            continue
        have = int(s[:2]) * 3600 + int(s[2:4]) * 60 + int(s[4:6])
        if abs(have - want) <= 10:
            out.append(f)
    return out


def read_placements(paths: list[str], card_names: set[str], name_to_id: dict[str, int]):
    """-> (taps, decks): taps = [{side, card, id, tick, native, kind, cycled}],
    decks = {side: [ids]}."""
    taps, decks = [], {}
    for p in paths:
        side = None
        with open(p, encoding="utf-8") as fh:
            recs = [json.loads(line) for line in fh if line.strip()]
        for r in recs:
            if "local_side_native" in r:
                side = r["local_side_native"]
        for r in recs:
            if "deck" in r and side is not None:
                decks[side] = [int(x) for x in r["deck"]]
        for r in recs:
            card = r.get("card") or r.get("cycled")
            if not card:
                continue
            s = r.get("side", side)
            if s is None:
                continue
            tile = r.get("requested") or r.get("tile")
            native = None
            if isinstance(tile, list) and len(tile) == 2:
                sx, sy = float(tile[0]), float(tile[1])
                nx, ny = (18.0 - sx, sy) if s == 0 else (sx, 32.0 - sy)
                native = [round(nx * 1000), round(ny * 1000)]
            name = canon_name(card, card_names)
            taps.append(
                {
                    "side": s,
                    "card": name,
                    "id": name_to_id.get(name),
                    "tick": int(r["tick"]),
                    "native": native,
                    "kind": "cast" if r.get("actual") == "cast" else "deploy",
                    "cycled": "cycled" in r,
                }
            )
    taps.sort(key=lambda t: (t["tick"], t["side"], t["card"]))
    return taps, decks


# ---------------------------------------------------------------------------
# spell casts


def spell_casts(frames: list[dict], side_of=lambda s: s) -> list[dict]:
    """The casts in the `effects` stream: one per run of class-28 objects of one (side,
    card) with no gap over CAST_GAP_TICKS between sightings (CAST_GAP_TICKS). Each:
    side, card_id, first_index, last_index, frames (sightings), objects (distinct
    object ids over the run), aim = the mean of the projectile targets
    (`projectile_x/y`, else x/y) of the objects on the FIRST frame -- one object's own
    target for a point spell (Fireball: the tap exactly), the airborne object's
    landing for a rolling one (the Log: where the roll starts, which the game may have
    clamped to the caster's territory), the pattern's centre for a volley (Arrows) --
    and aim_rule saying which."""
    open_casts: dict[tuple, dict] = {}
    out: list[dict] = []
    for fi, f in enumerate(frames):
        tick = f["tick"]
        by_key: dict[tuple, list[dict]] = defaultdict(list)
        for eff in f.get("effects") or []:
            cid = eff.get("card_id", -1)
            if cid < 0 or cid // 1_000_000 != SPELL_CLASS:
                continue
            by_key[(side_of(eff["side"]), cid)].append(eff)
        for key, effs in by_key.items():
            c = open_casts.get(key)
            if c is not None and tick - c["last_tick"] > CAST_GAP_TICKS:
                out.append(c)
                c = None
            if c is None:
                pts = [(e.get("projectile_x", e["x"]), e.get("projectile_y", e["y"])) for e in effs]
                c = open_casts[key] = {
                    "side": key[0],
                    "card_id": key[1],
                    "first_index": fi,
                    "last_index": fi,
                    "last_tick": tick,
                    "frames": 0,
                    "ids": set(),
                    "aim": [
                        sum(x for x, _ in pts) // len(pts),
                        sum(y for _, y in pts) // len(pts),
                    ],
                    "aim_rule": "the object's projectile target"
                    if len(pts) == 1
                    else f"mean of {len(pts)} objects' projectile targets on the first frame",
                }
            c["last_index"] = fi
            c["last_tick"] = tick
            c["frames"] += 1
            c["ids"].update(str(e.get("id")) for e in effs)
    out.extend(open_casts.values())
    for c in out:
        c["objects"] = len(c.pop("ids"))
        c.pop("last_tick")
    out.sort(key=lambda c: (c["first_index"], c["side"], c["card_id"]))
    return out


# ---------------------------------------------------------------------------
# the fixture


def build(
    capture: str,
    placements: list[str],
    stride: int,
    until_tick: int | None,
    census: dict | None,
    id_table: dict[int, str],
    doc: dict,
    register: dict,
    name_to_id: dict[str, int],
    card_names: set[str],
    seats: dict[str, str] | None = None,
) -> dict:
    header, raw_frames = read_capture(capture)
    frames, dup, back = dedupe(raw_frames)
    if until_tick is not None:
        frames = [f for f in frames if f["tick"] <= until_tick]
    reasons: list[str] = []
    # The seat map is the RUN's, so a capture's letter does not depend on which placement
    # logs happen to sit beside it.
    if seats is None:
        seats = seat_letters([os.path.basename(capture)] + [os.path.basename(p) for p in placements])
    fx: dict = {
        "format": FORMAT,
        "generated_by": "tools/make_replay_fixture.py",
        "cards_json_fnv1a64": doc.get("_fnv1a64"),
        "capture": public_name(capture, seats),
        "placements": [public_name(p, seats) for p in placements],
        "frame": {
            "blue_native_side": 0,
            "transform": "identity",
            "native_per_tile": 1000,
            "subtiles_per_native": 18,
        },
        "truth_stride": stride,
        "frames_total": len(raw_frames),
        "frames_duplicate": dup,
        "frames_out_of_order": back,
    }
    if not frames:
        fx["playable"] = False
        fx["unplayable_reasons"] = ["no frames"]
        return fx

    # -- side convention: side 0 must defend low y; else rotate the whole capture
    towers_hdr = (header or {}).get("towers") or []
    king0 = [t for t in towers_hdr if t["side"] == 0]
    king1 = [t for t in towers_hdr if t["side"] == 1]
    rotate = bool(king0 and king1) and min(t["y"] for t in king0) > min(t["y"] for t in king1)
    if rotate:
        fx["frame"] = {
            "blue_native_side": 1,
            "transform": "rotate180: (W - x, H - y), sides swapped",
            "native_per_tile": 1000,
            "subtiles_per_native": 18,
        }

    def side_of(s):
        return 1 - s if rotate else s

    def pos_of(x, y):
        return (NATIVE_W - x, NATIVE_H - y) if rotate else (x, y)

    # -- entities across frames
    ticks = [f["tick"] for f in frames]
    ents: dict[int, dict] = {}
    per_tick_rows: list[dict[int, tuple]] = []
    for fi, f in enumerate(frames):
        ptr_to_key = {e["id"]: e["generation_key"] for e in f["entities"]}
        rows = {}
        for e in f["entities"]:
            k = e["generation_key"]
            x, y = pos_of(e["x"], e["y"])
            tgt = ptr_to_key.get(e.get("target") or "", -1) if e.get("target") else -1
            rows[k] = (x, y, e["hp"], tgt, len(e.get("path_nodes") or []), e["behavior_state"])
            rec = ents.get(k)
            if rec is None:
                rec = ents[k] = {
                    "key": k,
                    "side": side_of(e["side"]),
                    "card_id": e["card_id"],
                    "level": e["level"],
                    "max_hp": e["max_hp"],
                    "kind_first": e["kind"],
                    "first_index": fi,
                    "last_index": fi,
                    "frames": 0,
                    "x0": x,
                    "y0": y,
                }
            rec["last_index"] = fi
            rec["frames"] += 1
            if len(rec.setdefault("states", [])) < 400:
                rec["states"].append((fi, e["behavior_state"]))
                rec.setdefault("positions", []).append((fi, x, y))
        per_tick_rows.append(rows)

    # -- mid-battle start
    first_non_tower = [e for e in ents.values() if e["first_index"] == 0 and e["card_id"] >= 0]
    if ticks[0] > 0 and first_non_tower:
        reasons.append(
            f"capture starts mid-battle: first frame is tick {ticks[0]} with"
            f" {len(first_non_tower)} non-tower entities on the board"
        )

    # -- towers: the six at the first frame, by (side, position) -> engine slot
    towers = []
    for e in ents.values():
        if e["card_id"] != -1 or e["first_index"] != 0:
            continue
        x0, y0 = e["x0"], e["y0"]
        hp0 = per_tick_rows[0][e["key"]][2]
        towers.append(
            {
                "key": e["key"],
                "side": e["side"],
                "x": x0,
                "y": y0,
                "hp": hp0,
                "max_hp": e["max_hp"],
                "level": e["level"],
                "kind_first": e["kind_first"],
            }
        )
    for t in towers:
        same = [u for u in towers if u["side"] == t["side"]]
        king = max(same, key=lambda u: u["max_hp"])
        if t is king:
            t["slot"] = 0
        else:
            t["slot"] = (
                1 if t["x"] < king["x"] else 2
            )  # engine lanes: 1 = low x (engine-left), 2 = high x
    towers.sort(key=lambda t: (t["side"], t["slot"]))
    if len(towers) != 6:
        reasons.append(f"first frame holds {len(towers)} towers, not 6")
    tower_level = {}
    for s in (0, 1):
        lv = [t["level"] for t in towers if t["side"] == s]
        tower_level[s] = Counter(lv).most_common(1)[0][0] if lv else None

    # -- classification of every non-tower entity
    cards_by_name = {c["name"]: c for c in doc["cards"]}
    unknown_ids: set[int] = set()
    for e in ents.values():
        if e["card_id"] < 0:
            e["role"] = "tower"
            continue
        name = id_table.get(e["card_id"])
        e["card"] = name
        if name is None:
            unknown_ids.add(e["card_id"])
            e["role"] = "unknown"
            e["unit"], e["deploy_summon"], e["hp_match"] = None, True, "no_card"
            continue
        unit, is_own, how = classify_unit(doc, cards_by_name.get(name), e["level"], e["max_hp"])
        e["unit"], e["deploy_summon"], e["hp_match"] = unit, is_own, how
        if how.startswith("unknown_object"):
            e["role"] = "unknown_object"
        else:
            e["role"] = "summon" if is_own else "spawned"
    for cid in sorted(unknown_ids):
        reasons.append(f"card id {cid} is not in the id table")

    # -- deploy groups: (side, card_id, first frame index) of deploy summons
    groups: dict[tuple, list[dict]] = defaultdict(list)
    for e in ents.values():
        if e["role"] == "summon":
            groups[(e["side"], e["card_id"], e["first_index"])].append(e)
    taps, decks = read_placements(placements, card_names, name_to_id)
    if rotate:
        for t in taps:
            t["side"] = 1 - t["side"]
            if t["native"]:
                t["native"] = [NATIVE_W - t["native"][0], NATIVE_H - t["native"][1]]
        decks = {1 - s: d for s, d in decks.items()}
    used_taps: set[int] = set()
    deploys = []
    latencies = []
    for (side, cid, fi), members in sorted(
        groups.items(), key=lambda kv: (kv[0][2], kv[0][0], kv[0][1])
    ):
        members.sort(key=lambda e: e["key"])
        name0 = members[0]["card"]
        tick, first_seen, evidence = refine_spawn_tick(
            ticks,
            fi,
            members[0].get("states", []),
            (cards_by_name.get(name0) or {}).get("deploy_time_ms"),
            positions=members[0].get("positions", []) if len(members) == 1 else None,
        )
        cx = sum(e["x0"] for e in members) // len(members)
        cy = sum(e["y0"] for e in members) // len(members)
        tap_ix = None
        for ix, t in enumerate(taps):
            if ix in used_taps or t["kind"] != "deploy" or t["side"] != side or t["id"] != cid:
                continue
            if t["tick"] + TAP_MIN <= first_seen <= t["tick"] + TAP_MAX:
                tap_ix = ix
                break
        name = members[0]["card"]
        card = cards_by_name.get(name) or {}
        d = {
            "tick": tick,
            "first_seen": first_seen,
            "tick_evidence": evidence,
            "side": side,
            "card": name,
            "card_id": cid,
            "kind": card.get("kind", "troop"),
            "level": Counter(e["level"] for e in members).most_common(1)[0][0],
            "count": len(members),
            "keys": [e["key"] for e in members],
            "centroid": [cx, cy],
            "first_seen_gap": ticks[fi] - ticks[fi - 1] if fi > 0 else 0,
            "hp_match": Counter(e["hp_match"] for e in members).most_common(1)[0][0],
            "families": sorted(
                (register.get("cards", {}).get(name) or {}).get("families", {}).keys()
            ),
        }
        if tap_ix is not None:
            t = taps[tap_ix]
            used_taps.add(tap_ix)
            latencies.append(first_seen - t["tick"])
            d["tap"] = {
                "tick": t["tick"],
                "native": t["native"],
                "cycled": t["cycled"],
            }
            if t["native"] and len(members) > 1:
                d["pos"], d["source"] = list(t["native"]), "tap_tile"
            else:
                d["pos"], d["source"] = [cx, cy], "centroid"
        else:
            d["pos"], d["source"] = [cx, cy], "centroid"
        deploys.append(d)

    # spawned and unknown-object groups: truth only, cross-checked against the taps.
    # An unknown-object group that COINCIDES with a deploy tap of its card is a deploy
    # the engine cannot reproduce (the game put an object cards.json does not derive
    # from the card): the fixture is unplayable from that tick (a prefix cut).
    spawned_groups = []
    sp: dict[tuple, list[dict]] = defaultdict(list)
    for e in ents.values():
        if e["role"] in ("spawned", "unknown_object"):
            sp[(e["side"], e["card_id"], e["first_index"])].append(e)
    for (side, cid, fi), members in sorted(
        sp.items(), key=lambda kv: (kv[0][2], kv[0][0], kv[0][1])
    ):
        tick = ticks[fi]
        # a tap a deploy group already answered is not this group's (the Goblin
        # Drill's surfaced building and Goblins follow its tap inside the window)
        coincides = [
            t
            for ix, t in enumerate(taps)
            if ix not in used_taps
            and t["kind"] == "deploy"
            and t["side"] == side
            and t["id"] == cid
            and t["tick"] + TAP_MIN <= tick <= t["tick"] + TAP_MAX
        ]
        name = members[0]["card"]
        role = members[0]["role"]
        hp_match = Counter(e["hp_match"] for e in members).most_common(1)[0][0]
        spawned_groups.append(
            {
                "tick": tick,
                "side": side,
                "card": name,
                "card_id": cid,
                "role": role,
                "unit": members[0]["unit"],
                "count": len(members),
                "keys": sorted(e["key"] for e in members),
                "hp_match": hp_match,
                "coincides_with_a_tap": bool(coincides),
            }
        )
        if role == "unknown_object" and coincides:
            reasons.append(
                f"{name}: deployed at tick {tick} as an object cards.json does not derive from"
                f" the card ({hp_match}; hp {members[0]['max_hp']})"
            )

    # -- spells: from the effects stream, else from taps with the measured latency
    for cast in spell_casts(frames, side_of):
        cid, fi = cast["card_id"], cast["first_index"]
        name = id_table.get(cid)
        if name is None:
            unknown_ids.add(cid)
            reasons.append(f"spell id {cid} is not in the id table")
            continue
        ax, ay = pos_of(*cast["aim"])
        tick = ticks[fi]
        side = cast["side"]
        tap_ix = None
        for ix, t in enumerate(taps):
            if ix in used_taps or t["kind"] != "cast" or t["side"] != side or t["id"] != cid:
                continue
            if t["tick"] + TAP_MIN <= tick <= t["tick"] + TAP_MAX:
                tap_ix = ix
                break
        d = {
            "tick": tick,
            "first_seen": tick,
            "tick_evidence": "first frame of the projectile"
            + (
                f" (frame gap {ticks[fi] - ticks[fi - 1]})"
                if fi > 0 and ticks[fi] - ticks[fi - 1] > 1
                else ""
            ),
            "side": side,
            "card": name,
            "card_id": cid,
            "kind": "spell",
            "level": None,
            "count": 0,
            "keys": [],
            "pos": [ax, ay],
            "source": "effect",
            "aim": cast["aim_rule"],
            "cast_objects": cast["objects"],
            "cast_frames": cast["frames"],
            "cast_last_tick": ticks[cast["last_index"]],
            "first_seen_gap": ticks[fi] - ticks[fi - 1] if fi > 0 else 0,
            "families": sorted(
                (register.get("cards", {}).get(name) or {}).get("families", {}).keys()
            ),
        }
        if tap_ix is not None:
            used_taps.add(tap_ix)
            latencies.append(tick - taps[tap_ix]["tick"])
            d["tap"] = {
                "tick": taps[tap_ix]["tick"],
                "native": taps[tap_ix]["native"],
            }
        deploys.append(d)
    median_latency = int(statistics.median(latencies)) if latencies else None
    unresolved = []
    for ix, t in enumerate(taps):
        if ix in used_taps or t["kind"] != "cast":
            continue
        if t["id"] is None or median_latency is None or not t["native"]:
            unresolved.append(
                {
                    "tick": t["tick"],
                    "side": t["side"],
                    "card": t["card"],
                    "why": "no projectile in the effects stream and no latency measurement on"
                    " this capture"
                    if median_latency is None
                    else "no projectile in the effects stream and no id / position",
                }
            )
            continue
        name = id_table.get(t["id"])
        est = t["tick"] + median_latency
        if until_tick is not None and est > until_tick:
            continue
        deploys.append(
            {
                "tick": est,
                "first_seen": None,
                "tick_evidence": f"tap tick {t['tick']} + median latency {median_latency}",
                "side": t["side"],
                "card": name,
                "card_id": t["id"],
                "kind": "spell",
                "level": None,
                "count": 0,
                "keys": [],
                "pos": list(t["native"]),
                "source": "tap_tile",
                "timing": "estimated",
                "tap": {
                    "tick": t["tick"],
                    "native": t["native"],
                },
                "families": sorted(
                    (register.get("cards", {}).get(name) or {}).get("families", {}).keys()
                ),
            }
        )
    deploys.sort(key=lambda d: (d["tick"], d["side"], d["card_id"]))
    # spells at the level of the side's units (a cast carries no level in the captures)
    for s in (0, 1):
        lv = [d["level"] for d in deploys if d["side"] == s and d["level"] is not None]
        side_level = Counter(lv).most_common(1)[0][0] if lv else tower_level.get(s)
        for d in deploys:
            if d["side"] == s and d["level"] is None:
                d["level"] = side_level
                d["level_source"] = "side mode"

    # -- decks and levels per side
    script_decks = {}
    card_levels = {}
    for s in (0, 1):
        used = []
        for d in deploys:
            if d["side"] == s and d["card"] not in used:
                used.append(d["card"])
        recorded = [id_table.get(cid) for cid in decks.get(s, [])]
        recorded = [n for n in recorded if n]
        padding = [n for n in recorded if n not in used]
        script_decks[str(s)] = {"deploy_order": used, "recorded": recorded, "padding": padding}
        lv = Counter()
        per_card: dict[str, Counter] = defaultdict(Counter)
        for d in deploys:
            if d["side"] == s and d["level"] is not None and d["kind"] != "spell":
                lv[d["level"]] += d["count"]
                per_card[d["card"]][d["level"]] += d["count"]
        mode = lv.most_common(1)[0][0] if lv else tower_level.get(s)
        card_levels[str(s)] = {
            "mode": mode,
            "per_card": {c: cnt.most_common(1)[0][0] for c, cnt in sorted(per_card.items())},
        }

    # -- engine loadability (the census, when present)
    if census is not None:
        loadable = set(census.get("loadable", []))
        rejected = census.get("rejected", {})
        for name in sorted({d["card"] for d in deploys if d["card"]}):
            if name not in loadable:
                reasons.append(f"{name}: {rejected.get(name, 'not in the engine card set')}")
        have = census.get("cards_json_fnv1a64")
        if have != doc.get("_fnv1a64"):
            fx["census"] = (
                f"STALE: built against cards.json {have}, this run reads {doc.get('_fnv1a64')}"
                " (cargo run --example replay_parity -- --census)"
            )
    else:
        fx["census"] = (
            "absent: engine loadability not checked here"
            " (cargo run --example replay_parity -- --census)"
        )

    # -- truth: RLE per entity column over its contiguous frame run
    sel = list(range(0, len(frames), max(stride, 1)))
    sel_set = set(sel)
    truth_ticks = [ticks[i] for i in sel]
    truth_entities = []
    for e in sorted(ents.values(), key=lambda e: e["key"]):
        idx = [i for i in range(e["first_index"], e["last_index"] + 1) if i in sel_set]
        if not idx:
            continue
        cols = [[], [], [], [], [], []]
        gaps = 0
        for i in idx:
            row = per_tick_rows[i].get(e["key"])
            if row is None:
                gaps += 1
                row = (None, None, None, None, None, None)
            for c in range(6):
                cols[c].append(row[c])
        truth_entities.append(
            {
                "key": e["key"],
                "side": e["side"],
                "card_id": e["card_id"],
                "card": e.get("card")
                if e["card_id"] >= 0
                else (
                    "KingTower"
                    if any(t["key"] == e["key"] and t["slot"] == 0 for t in towers)
                    else "PrincessTower"
                ),
                "role": e["role"],
                "unit": e.get("unit"),
                "level": e["level"],
                "max_hp": e["max_hp"],
                "t0": sel.index(idx[0]),
                "n": len(idx),
                "absent_frames": gaps,
                "x": rle(cols[0]),
                "y": rle(cols[1]),
                "hp": rle(cols[2]),
                "target": rle(cols[3]),
                "path_n": rle(cols[4]),
                "state": rle(cols[5]),
            }
        )

    fx.update(
        {
            "playable": not reasons,
            "unplayable_reasons": reasons,
            "local_side_native": (header or {}).get("local_side_native"),
            "ticks": {"first": ticks[0], "last": ticks[-1], "frames": len(frames)},
            "tap_latency_ticks": {"median": median_latency, "samples": sorted(latencies)},
            "towers": [
                {k: t[k] for k in ("slot", "side", "x", "y", "hp", "max_hp", "level")}
                for t in towers
            ],
            "tower_level": {str(s): tower_level[s] for s in (0, 1)},
            "card_levels": card_levels,
            "decks": script_decks,
            "deploys": deploys,
            "spawned_groups": spawned_groups,
            "unresolved": unresolved,
            "truth": {
                "columns": ["x", "y", "hp", "target", "path_n", "state"],
                "encoding": "per entity, each column run-length encoded as [value, run, ...]"
                " over its frames from index t0 for n frames of `ticks`; a null value is a"
                " frame the entity was absent from inside its run; alive = present",
                "ticks": truth_ticks,
                "entities": truth_entities,
            },
        }
    )
    return fx


def fixture_text(fx: dict) -> str:
    return json.dumps(fx, separators=(",", ":")) + "\n"


def comparable(fx: dict) -> dict:
    """The fixture without the fields that describe the RUN rather than the battle."""
    return {k: v for k, v in fx.items() if k != "census"}


def write_fixture(fx: dict, out_dir: str, written: dict[str, str] | None = None) -> str:
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, fx["capture"] + ".replay.json")
    # Two captures on one path would leave one battle's fixture holding the other's
    # content and list the same file twice in the manifest.
    if written is not None and path in written:
        raise SystemExit(f"{path} was already written from {written[path]}; this run would overwrite it")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(fixture_text(fx))
    if written is not None:
        written[path] = fx["capture"]
    return path


def load_census(out_dir: str) -> dict | None:
    """The engine's census from --out, else from the default output dir."""
    for d in (out_dir, OUT_DEFAULT):
        p = os.path.join(d, CENSUS)
        if os.path.exists(p):
            with open(p, encoding="utf-8") as fh:
                return json.load(fh)
    return None


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("capture", nargs="?", help="a *.native.oracle.jsonl.gz live capture")
    ap.add_argument(
        "--placements",
        nargs="*",
        default=None,
        help="placement logs (default: the same-stamp files beside the capture)",
    )
    ap.add_argument("--out", default=OUT_DEFAULT)
    ap.add_argument("--truth-stride", type=int, default=1)
    ap.add_argument(
        "--until-tick",
        type=int,
        default=None,
        help="drop frames after this tick (a trimmed sample)",
    )
    ap.add_argument(
        "--all", action="store_true", help="every capture in --reports, plus the manifest"
    )
    ap.add_argument(
        "--reports", default=LIVE, help="the captures folder (default: ROYALELIVE_REPORTS)"
    )
    ap.add_argument(
        "--check",
        metavar="FIXTURE",
        default=None,
        help="compare with this existing fixture instead of writing (exit 1 if it differs)",
    )
    args = ap.parse_args()
    if not args.capture and not args.all:
        ap.error("a capture or --all")
    if args.check and args.all:
        ap.error("--check takes one capture, not --all")
    if args.all and not args.reports:
        ap.error("--all needs --reports or ROYALELIVE_REPORTS set to the captures folder")
    with open(CARDS, "rb") as fh:
        raw = fh.read()
    doc = json.loads(raw.decode("utf-8"))
    doc["_fnv1a64"] = fnv1a64(raw)
    if os.path.exists(REGISTER):
        with open(REGISTER, encoding="utf-8") as fh:
            register = json.load(fh)
    else:
        register = {}
        print(
            f"warning: {REGISTER} missing (python tools/mechanic_register.py):"
            " no mechanic families",
            file=sys.stderr,
        )
    id_table = load_id_table()
    card_names = {c["name"] for c in doc["cards"]}
    # the base class id per card name (a hero-form id, class 203, names the same card)
    name_to_id = {
        name: cid for cid, name in sorted(id_table.items(), reverse=True) if name in card_names
    }
    census = load_census(args.out)
    captures = (
        [args.capture]
        if args.capture
        else distinct_captures(glob.glob(os.path.join(args.reports, "*" + CAPTURE_SUFFIX)))
    )
    jobs = [
        (
            cap,
            args.placements
            if args.placements is not None
            else placement_files_for(cap, os.path.dirname(os.path.abspath(cap))),
        )
        for cap in captures
    ]
    # ONE seat map for the run, seeded from the whole captures folder when there is one,
    # so a capture's letter is the same whether it is built alone or with --all.
    pool = [os.path.basename(c) for c, _ in jobs]
    pool += [os.path.basename(p) for _, ps in jobs for p in ps]
    if args.reports and os.path.isdir(args.reports):
        pool += [os.path.basename(f) for f in glob.glob(os.path.join(args.reports, "*" + CAPTURE_SUFFIX))]
    seats = seat_letters(pool)
    manifest = []
    written: dict[str, str] = {}
    for cap, placements in jobs:
        fx = build(
            cap,
            placements,
            args.truth_stride,
            args.until_tick,
            census,
            id_table,
            doc,
            register,
            name_to_id,
            card_names,
            seats,
        )
        if args.check:
            with open(args.check, encoding="utf-8") as fh:
                have = json.load(fh)
            if comparable(have) != comparable(fx):
                print(f"STALE: {args.check} differs from what this capture builds")
                return 1
            print(f"{args.check} is current")
            return 0
        path = write_fixture(fx, args.out, written)
        size = os.path.getsize(path)
        row = {
            "capture": fx["capture"],
            "fixture": os.path.basename(path),
            "playable": fx["playable"],
            "reasons": fx["unplayable_reasons"],
            "deploys": len(fx.get("deploys", [])),
            "spawned_groups": len(fx.get("spawned_groups", [])),
            "ticks": fx.get("ticks"),
            "frames_duplicate": fx["frames_duplicate"],
            "bytes": size,
        }
        manifest.append(row)
        status = (
            "playable" if fx["playable"] else "UNPLAYABLE " + "; ".join(fx["unplayable_reasons"])
        )
        print(
            f"{row['fixture']}: {row['deploys']} deploys, {row['spawned_groups']} spawned groups,"
            f" {size // 1024} KB -- {status}"
        )
    if args.all:
        mpath = os.path.join(args.out, "manifest.json")
        census_state = "absent"
        if census is not None:
            census_state = (
                "present"
                if census.get("cards_json_fnv1a64") == doc["_fnv1a64"]
                else f"STALE (built against cards.json {census.get('cards_json_fnv1a64')})"
            )
        with open(mpath, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(
                {
                    "generated_by": "tools/make_replay_fixture.py --all",
                    "cards_json_fnv1a64": doc["_fnv1a64"],
                    "census": census_state,
                    "run_captures": [os.path.basename(c) for c, _ in jobs],
                    "playable": sum(1 for r in manifest if r["playable"]),
                    "unplayable": sum(1 for r in manifest if not r["playable"]),
                    "captures": manifest,
                },
                fh,
                indent=1,
            )
            fh.write("\n")
        print(
            f"manifest: {mpath}"
            f" ({sum(1 for r in manifest if r['playable'])} playable / {len(manifest)})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
