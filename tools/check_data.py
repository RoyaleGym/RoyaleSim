#!/usr/bin/env python3
"""The data gate: is data/derived/cards.json (and globals.json) something the
engine can trust to be what Supercell shipped?

WHY THIS EXISTS
    Extraction bugs do not crash.  A projectile reference with a typo resolves
    to null and the Musketeer silently does no damage; a column read as tiles
    instead of millitiles makes every unit 1000x too small and every test still
    runs.  So each class of silent defect gets a gate here, checked against an
    expectation that does NOT come from the extractor, and each gate has a plant
    that proves it can see the defect it exists for.

VINTAGE
    The data is ~2018 game data; the target is the live 2026 game.  A gate passing
    here means "the extraction is faithful to the 2018 files", never "the number
    is right for the live game".

USAGE
    python tools/check_data.py                 # run every gate
    python tools/check_data.py --plant ref     # prove a gate can fail (see PLANTS)
    python tools/check_data.py --all-plants    # run every plant; exit 0 only if all land

    A plant is only evidence if (1) the tree is green WITHOUT it and (2) the gate
    it AIMS AT goes red WITH it -- not merely some gate somewhere.  Plants mutate
    loaded copies only; nothing on disk is touched.

TERRITORY LANDMARKS (added 2026-09-13)
    The engine forbids enemy troops inside each alive crown tower's NoDeploySize
    rectangle (calibration.json arena.TERRITORY_MODEL).  The UNIT of
    NoDeploySizeW/H is not written anywhere in the data; it is inferred because
    under TILES four independent landmarks come out exact -- king rect x span is
    the arena width (two edges), the princess rects meet on the centre line, a
    princess rect's far edge is the far river bank -- and under half-tiles none
    does.  WHY A GATE: a size perturbed by one, or a unit reinterpreted, moves
    the troop pocket by whole half-rows on every board and no engine test would
    know the data moved under it.  It reads data/derived/arena.json (run
    tools/extract_arena.py first) and the princess tower centre y from
    crates/royalesim/src/arena.rs PRINCESS_TOWER_Y_TILES_100 (the one arena
    number the tilemap does not carry; parsed, not copied).  It CANNOT tell
    whether the live 2026 game still uses these sizes, or whether the rect is
    closed or open at its edge -- only a recording can (the registry's
    promotion rule).
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from collections import Counter
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import extract_cards as ec
import extract_globals as eg

ROOT = ec.ROOT
CARDS = ec.OUT
GLOBALS = eg.OUT
ARENA = ROOT / "data" / "derived" / "arena.json"
ARENA_RS = ROOT / "crates" / "royalesim" / "src" / "arena.rs"

# --- independent expectations -------------------------------------------------

# Where each reference column points.  "unit" = characters OR buildings (a death
# spawn like BalloonBomb is a building row).
REF_COLUMNS = {
    "characters": {
        "Projectile": "projectiles",
        "CustomFirstProjectile": "projectiles",
        "ProjectileSpecial": "projectiles",
        "SpawnProjectile": "projectiles",
        "DeathSpawnProjectile": "projectiles",
        "SpawnCharacter": "unit",
        "DeathSpawnCharacter": "unit",
        "AttachedCharacter": "unit",
        "MorphCharacter": "unit",
        "SpawnAreaObject": "area_effect_objects",
        "DeathAreaEffect": "area_effect_objects",
        "AreaEffectOnDash": "area_effect_objects",
        "AreaEffectOnMorph": "area_effect_objects",
        "AppearAreaObject": "area_effect_objects",
        "BuffOnDamage": "character_buffs",
        "StartingBuff": "character_buffs",
        "AreaBuff": "character_buffs",
    },
    "projectiles": {
        "SpawnCharacter": "unit",
        "SpawnProjectile": "projectiles",
        "SpawnAreaEffectObject": "area_effect_objects",
        "TargetBuff": "character_buffs",
    },
    "spells_characters": {
        "SummonCharacter": "unit",
        "SummonCharacterSecond": "unit",
        "Projectile": "projectiles",
        "CustomFirstProjectile": "projectiles",
        "AreaEffectObject": "area_effect_objects",
    },
    "area_effect_objects": {
        "Projectile": "projectiles",
        "SpawnCharacter": "unit",
        "Buff": "character_buffs",
        "SpawnsAEO": "area_effect_objects",
    },
}
REF_COLUMNS["buildings"] = REF_COLUMNS["characters"]
REF_COLUMNS["spells_buildings"] = REF_COLUMNS["spells_characters"]
REF_COLUMNS["spells_other"] = REF_COLUMNS["spells_characters"]

# The reference collision-radius table (millitiles), keyed by internal name.
KNOWN_RADII = {
    "MiniPekka": 450,
    "IceSpirits": 400,
    "FireSpirits": 400,
    "HogRider": 600,
    "Prince": 600,
    "Assassin": 600,
    "Giant": 750,
    "Golem": 750,
    "Pekka": 750,
    "MegaKnight": 750,
    "ZapMachine": 1000,
    "GiantSkeleton": 1000,
    "Tesla": 500,
    "Cannon": 600,
    "InfernoTower": 600,
    "Mortar": 600,
    "Xbow": 600,
    "PrincessTower": 1000,
    "KingTower": 1400,
}
MODAL_TROOP_RADIUS = 500
# Table entries with no row in the 2018 data: skipped with a note, not failed.
ABSENT_FROM_DATA = {"Wall Breakers": "no row in this ~2018 data (the card post-dates it)"}
# A disagreement between the reference table and the data that is NOT resolved.
# It is pinned to the exact shipped value so the gate still fires if the data
# moves; it is printed as a WARNING on every run so nobody forgets it exists.
KNOWN_DISAGREEMENTS = {
    "Prince": {
        "table": 600,
        "ships": 650,
        "why": "2018 characters.csv ships 650. Either the reference table is a later "
        "value or it is wrong; nothing in this repo settles which.",
    },
}

ALLOWED_SPEEDS = {0, 30, 45, 60, 90, 120}

# (field, lo, hi, allow_zero).  Bands are chosen so that, for the typical row of
# each field, the classic unit confusions land outside them: a distance read as
# TILES (<= 32) or as SUBTILES (x18), a duration read as SECONDS (<= 10) or as
# TICKS (x1/50).  Extreme rows (Balloon Range 100 x18 = 1800) can still slip
# through individually; a whole-column confusion cannot.
DIST_BANDS = [
    ("collision_radius_milli", 300, 2000, False),
    ("range_milli", 100, 15000, False),
    ("minimum_range_milli", 100, 15000, False),
    ("sight_range_milli", 1000, 15000, False),
    ("area_damage_radius_milli", 300, 6000, True),
    ("death_damage_radius_milli", 300, 6000, False),
]
PROJ_DIST_BANDS = [
    ("radius_milli", 300, 6000, False),
    ("radius_y_milli", 100, 6000, False),
    ("projectile_radius_milli", 300, 6000, False),
    ("projectile_range_milli", 1000, 15000, False),
    ("pushback_milli", 100, 5000, False),
    ("min_distance_milli", 100, 15000, False),
]
DUR_BANDS = [
    ("hit_speed_ms", 250, 12000, False),
    ("load_time_ms", 50, 6000, True),
    ("deploy_time_ms", 500, 5000, False),
    ("lifetime_ms", 1000, 120000, False),
    ("hide_time_ms", 100, 5000, False),
    ("up_time_ms", 100, 5000, False),
]

# Every field the engine needs, by kind.  None is a failure; 0/False are fine.
REQUIRED = {
    "troop": [
        "elixir",
        "rarity",
        "hitpoints",
        "damage",
        "hit_speed_ms",
        "load_time_ms",
        "speed",
        "range_milli",
        "sight_range_milli",
        "collision_radius_milli",
        "mass",
        "deploy_time_ms",
        "attacks_air",
        "attacks_ground",
        "target_only_buildings",
        "flying_height",
        "area_damage_radius_milli",
        "count",
        "shield_hitpoints",
        "crown_tower_damage_percent",
        "level_scaling",
    ],
    "building": [
        "elixir",
        "rarity",
        "hitpoints",
        "damage",
        "hit_speed_ms",
        "load_time_ms",
        "speed",
        "range_milli",
        "sight_range_milli",
        "collision_radius_milli",
        "deploy_time_ms",
        "attacks_air",
        "attacks_ground",
        "lifetime_ms",
        "area_damage_radius_milli",
        "crown_tower_damage_percent",
        "count",
        "level_scaling",
    ],
    "spell": ["elixir", "rarity", "attacks_air", "attacks_ground", "count", "level_scaling"],
    "tower": [
        "hitpoints",
        "damage",
        "hit_speed_ms",
        "load_time_ms",
        "range_milli",
        "sight_range_milli",
        "collision_radius_milli",
        "attacks_air",
        "attacks_ground",
        "crown_tower_damage_percent",
        "level_scaling",
    ],
}


def dig(obj, path: str):
    for part in path.split("."):
        if obj is None:
            return None
        obj = obj.get(part) if isinstance(obj, dict) else None
    return obj


# Mechanic archetypes: each thin-slice card must carry the data for the thing it
# is IN the slice to exercise.  (display, dotted path, predicate description, predicate)
ARCHETYPE = [
    ("Knight", "projectile", "melee: no projectile", lambda v: v is None),
    ("Archers", "count", "two archers", lambda v: v == 2),
    ("Archers", "projectile.name", "ranged via projectile", lambda v: v is not None),
    ("Musketeer", "attacks_air", "hits air", lambda v: v is True),
    ("Giant", "target_only_buildings", "building-targeter", lambda v: v is True),
    ("Hog Rider", "target_only_buildings", "building-targeter", lambda v: v is True),
    ("Hog Rider", "speed", "fast (Speed 120)", lambda v: v == 120),
    ("Minions", "flying_height", "flies", lambda v: isinstance(v, int) and v > 0),
    ("Minions", "count", "swarm of 3", lambda v: isinstance(v, int) and v >= 3),
    ("Baby Dragon", "flying_height", "flies", lambda v: isinstance(v, int) and v > 0),
    ("Baby Dragon", "area_damage_radius_milli", "splash", lambda v: isinstance(v, int) and v > 0),
    ("Valkyrie", "area_damage_radius_milli", "splash", lambda v: isinstance(v, int) and v > 0),
    ("Skeleton Army", "count", "swarm >= 10", lambda v: isinstance(v, int) and v >= 10),
    ("Cannon", "attacks_air", "ground only", lambda v: v is False),
    ("Tesla", "attacks_air", "hits air", lambda v: v is True),
    ("Fireball", "damage", "damage spell", lambda v: isinstance(v, int) and v > 0),
    ("Fireball", "area_damage_radius_milli", "radius", lambda v: isinstance(v, int) and v > 0),
    (
        "Zap",
        "spell.area_effect_object.buff.speed_multiplier_raw",
        "stun stops movement",
        lambda v: v == -100,
    ),
    (
        "Zap",
        "spell.area_effect_object.buff_time_ms",
        "stun duration",
        lambda v: isinstance(v, int) and v > 0,
    ),
    (
        "Arrows",
        "area_damage_radius_milli",
        "wide (> Fireball)",
        lambda v: isinstance(v, int) and v > 2500,
    ),
    (
        "The Log",
        "projectile.spawn_projectile.projectile_range_milli",
        "rolls a distance",
        lambda v: isinstance(v, int) and v > 0,
    ),
    ("The Log", "attacks_air", "ground only", lambda v: v is False),
    ("Prince", "charge.damage_special", "charge damage", lambda v: isinstance(v, int) and v > 0),
    ("Wizard", "area_damage_radius_milli", "ranged splash", lambda v: isinstance(v, int) and v > 0),
    ("Goblin Barrel", "spell.spawn.character", "spawns goblins", lambda v: v == "Goblin"),
    ("Goblin Barrel", "spell.spawn.count", "spawn count", lambda v: isinstance(v, int) and v > 0),
    # Added 2026-09-13 with the engine's spells (crates/royalesim/src/card.rs SPELLS):
    # every field below is one the engine READS, and each is a hop a re-join gets wrong.
    (
        "Arrows",
        "spell.first_projectile.damage",
        "damage carrier is the CustomFirstProjectile",
        lambda v: isinstance(v, int) and v > 0,
    ),
    ("Arrows", "projectile.damage", "deco projectile carries no damage", lambda v: v is None),
    (
        "Fireball",
        "projectile.pushback_milli",
        "knockback distance",
        lambda v: isinstance(v, int) and v > 0,
    ),
    ("Fireball", "projectile.pushback_all", "respects IgnorePushback", lambda v: v is False),
    (
        "The Log",
        "projectile.spawn_projectile.pushback_all",
        "pushes all ground troops",
        lambda v: v is True,
    ),
    (
        "The Log",
        "projectile.spawn_projectile.projectile_radius_milli",
        "roll half-width",
        lambda v: isinstance(v, int) and v > 0,
    ),
    ("The Log", "spell.spell_as_deploy", "troop-territory spell", lambda v: v is True),
    (
        "Goblin Barrel",
        "spell.spawn.deploy_time_ms",
        "spawn deploy-time override",
        lambda v: isinstance(v, int) and v > 0,
    ),
    ("Giant", "ignore_pushback", "IgnorePushback (2018 data)", lambda v: v is True),
]


# --- the gate -----------------------------------------------------------------------


def gate(
    t: dict, doc: dict, file_doc: dict | None, file_globals: dict | None
) -> tuple[list[str], list[str], list[str]]:
    """Return (failures, warnings, info)."""
    fail: list[str] = []
    warn: list[str] = []
    info: list[str] = []

    # 1. references resolve ------------------------------------------------------
    units = set(t["characters"].records) | set(t["buildings"].records)
    targets = {
        "unit": units,
        "projectiles": set(t["projectiles"].records),
        "area_effect_objects": set(t["area_effect_objects"].records),
        "character_buffs": set(t["character_buffs"].records),
    }
    nrefs = 0
    for tbl, cols in REF_COLUMNS.items():
        for name, rec in t[tbl].records.items():
            for col, kind in cols.items():
                v = rec.get(col)
                if v is None:
                    continue
                nrefs += 1
                if v not in targets[kind]:
                    fail.append(f"refs resolve: {tbl}.{name}.{col} -> {v!r} is not a {kind} row")
    info.append(f"{nrefs} references checked")

    # 2. collision radii ------------------------------------------------------------
    U = doc["units"]
    for name, want in KNOWN_RADII.items():
        if name not in U:
            fail.append(f"collision radius: {name} has no row (expected {want})")
            continue
        got = U[name]["collision_radius_milli"]
        dis = KNOWN_DISAGREEMENTS.get(name)
        if got == want:
            continue
        if dis and dis["table"] == want and dis["ships"] == got:
            warn.append(
                f"collision radius: {name} table says {want}, data ships {got} -- "
                f"UNRESOLVED DISAGREEMENT: {dis['why']}"
            )
            continue
        fail.append(f"collision radius: {name} expected {want}, got {got}")
    for disp, why in ABSENT_FROM_DATA.items():
        info.append(f"collision radius: {disp} skipped -- {why}")
    bandit = t["characters"].get("Assassin")
    if not bandit or "bandit" not in str(bandit.get("FileName", "")).lower():
        fail.append("collision radius: Assassin is no longer identifiable as Bandit")
    troop_radii = Counter(
        U[c["summon_character"]]["collision_radius_milli"]
        for c in doc["cards"]
        if c["kind"] == "troop"
    )
    modal = troop_radii.most_common(1)[0][0] if troop_radii else None
    if modal != MODAL_TROOP_RADIUS:
        fail.append(
            f"collision radius: modal troop radius expected {MODAL_TROOP_RADIUS}, got {modal} "
            f"({dict(troop_radii)})"
        )

    # 3. speed set (raw column, every character and building row) -------------------
    for tbl in ("characters", "buildings"):
        for name, rec in t[tbl].records.items():
            sp = rec.get("Speed")
            if (0 if sp is None else sp) not in ALLOWED_SPEEDS:
                fail.append(f"speed in allowed set: {tbl}.{name}.Speed = {sp}")

    # 4/5. magnitude bands --------------------------------------------------------------
    def band(prefix: str, where: str, field: str, v, lo, hi, zero_ok):
        if v is None or (zero_ok and v == 0):
            return
        if not isinstance(v, int) or not (lo <= v <= hi):
            fail.append(f"{prefix}: {where}.{field} = {v!r} outside [{lo}, {hi}]")

    for name, u in U.items():
        for f, lo, hi, z in DIST_BANDS:
            band("distance band", f"units.{name}", f, u[f], lo, hi, z)
        for f, lo, hi, z in DUR_BANDS:
            band("duration band", f"units.{name}", f, u[f], lo, hi, z)
    for name, p in doc["projectiles"].items():
        for f, lo, hi, z in PROJ_DIST_BANDS:
            band("distance band", f"projectiles.{name}", f, p[f], lo, hi, z)
    for name, a in doc["area_effect_objects"].items():
        band(
            "distance band",
            f"area_effect_objects.{name}",
            "radius_milli",
            a["radius_milli"],
            300,
            6000,
            False,
        )
        band(
            "duration band",
            f"area_effect_objects.{name}",
            "hit_speed_ms",
            a["hit_speed_ms"],
            50,
            5000,
            False,
        )
        band(
            "duration band",
            f"area_effect_objects.{name}",
            "buff_time_ms",
            a["buff_time_ms"],
            100,
            20000,
            False,
        )
    for c in doc["cards"]:
        if c["kind"] == "spell":
            band(
                "distance band",
                f"cards.{c['name']}",
                "area_damage_radius_milli",
                c["area_damage_radius_milli"],
                300,
                6000,
                False,
            )
    # Tick-rate evidence, reported not gated: durations off the 50 ms grid.
    off = sorted(
        {
            f"{n}.{f}={u[f]}"
            for n, u in U.items()
            for f, *_ in DUR_BANDS
            if isinstance(u[f], int) and u[f] % 50
        }
    )
    info.append(f"unit durations NOT a multiple of 50 ms: {off or 'none'}")

    # 6. completeness ------------------------------------------------------------------
    by = {c["name"]: c for c in doc["cards"]}
    for disp, internal in ec.THIN_SLICE.items():
        c = by.get(internal)
        if c is None:
            fail.append(f"completeness: {disp} ({internal}) is not in cards.json")
            continue
        missing = [f for f in REQUIRED[c["kind"]] if c.get(f) is None]
        if c["kind"] == "spell" and c.get("damage") is None and dig(c, "spell.spawn") is None:
            missing.append("damage-or-spawn")
        if c["kind"] == "spell" and c.get("damage") is not None:
            missing += [
                f
                for f in ("area_damage_radius_milli", "crown_tower_damage_percent")
                if c.get(f) is None
            ]
        if missing:
            fail.append(f"completeness: {disp} ({internal}) missing {missing}")
    for name in ec.TOWERS:
        tw = next((x for x in doc["towers"] if x["name"] == name), None)
        if tw is None:
            fail.append(f"completeness: tower {name} missing")
            continue
        missing = [f for f in REQUIRED["tower"] if tw.get(f) is None]
        if missing:
            fail.append(f"completeness: tower {name} missing {missing}")
    for disp, path, desc, pred in ARCHETYPE:
        c = by.get(ec.THIN_SLICE[disp])
        v = dig(c, path) if c else None
        if not pred(v):
            fail.append(f"archetype: {disp} {desc} ({path} = {v!r})")
    # spawned entities must themselves be real units with stats
    for c in doc["cards"]:
        for ref in (
            dig(c, "spell.spawn.character"),
            dig(c, "second_summon.character"),
            dig(c, "death_spawn.character"),
        ):
            if ref is not None and ref not in U:
                fail.append(f"completeness: {c['name']} spawns {ref!r}, which has no unit record")
            elif ref is not None and U[ref].get("rarity") not in doc["rarities"]:
                # The engine scales a spawned unit on ITS rarity's table (card.rs SPELLS).
                fail.append(
                    f"completeness: {c['name']} spawns {ref!r}, whose rarity "
                    f"{U[ref].get('rarity')!r} is not a rarities.csv row"
                )

    # 7. level scaling -------------------------------------------------------------------
    lf, ln = ec.check_hog_ladder(doc["rarities"], doc["units"], doc["cards"])
    fail += lf
    info += ln

    # 8. invariants on the artefact --------------------------------------------------------
    def floats(o, p="$"):
        if isinstance(o, float):
            yield p
        elif isinstance(o, dict):
            for k, v in o.items():
                yield from floats(v, f"{p}.{k}")
        elif isinstance(o, list):
            for i, v in enumerate(o):
                yield from floats(v, f"{p}[{i}]")

    for label, d in (
        ("cards.json", doc),
        ("cards.json on disk", file_doc),
        ("globals.json on disk", file_globals),
    ):
        if d is not None:
            fl = list(floats(d))
            if fl:
                fail.append(f"no floats: {label} has {len(fl)} float(s), first at {fl[0]}")
    for label, d in (("cards.json", doc), ("globals.json on disk", file_globals)):
        if d is not None and "PRE-2025" not in d.get("vintage_warning", ""):
            fail.append(f"vintage marked: {label} lacks a PRE-2025 vintage_warning")

    # 10. territory landmarks ---------------------------------------------------------------
    fail_t, info_t = territory_landmarks(doc)
    fail += fail_t
    info += info_t

    # 9. the file on disk is what the extractor produces now ----------------------------
    if file_doc is None:
        fail.append("derived cards.json fresh: file missing -- run tools/extract_cards.py")
    elif file_doc != json.loads(ec.render(doc)):
        fail.append(
            "derived cards.json fresh: file on disk differs from a rebuild of the raw CSVs "
            "-- run tools/extract_cards.py"
        )
    if file_globals is None:
        fail.append("derived globals.json fresh: file missing -- run tools/extract_globals.py")
    else:
        g, order = eg.read_globals(eg.SRC)
        if file_globals != json.loads(json.dumps(eg.build(g, order))):
            fail.append(
                "derived globals.json fresh: file on disk differs from a rebuild "
                "-- run tools/extract_globals.py"
            )
    return fail, warn, info


def princess_y_tiles() -> Fraction | None:
    """PRINCESS_TOWER_Y_TILES_100 from arena.rs, comments stripped: a struck note
    must not read as the live value."""
    if not ARENA_RS.exists():
        return None
    code = "\n".join(
        line.split("//", 1)[0] for line in ARENA_RS.read_text(encoding="utf-8").splitlines()
    )
    m = re.findall(r"pub const PRINCESS_TOWER_Y_TILES_100: i32 = (\d+);", code)
    return Fraction(int(m[0]), 100) if len(m) == 1 else None


def territory_landmarks(doc: dict) -> tuple[list[str], list[str]]:
    """Gate 10.  All geometry in exact Fractions of a tile, from arena.json's integer
    half-cell indices (never its float convenience fields)."""
    fail: list[str] = []
    info: list[str] = []
    tag = "territory landmarks"
    if not ARENA.exists():
        return [f"{tag}: {ARENA.relative_to(ROOT)} missing -- run tools/extract_arena.py"], info
    arena = json.loads(ARENA.read_text(encoding="utf-8"))
    py = princess_y_tiles()
    if py is None:
        return [
            f"{tag}: could not parse exactly one PRINCESS_TOWER_Y_TILES_100 from {ARENA_RS.name}"
        ], info
    half = Fraction(1, arena["half_tiles_per_tile"])
    width = Fraction(arena["tiles"][0])
    far_bank = (arena["water_half_rows"][1] + 1) * half
    bridges = sorted(arena["bridges"], key=lambda b: b["half_cols"][0])
    kings = sorted(arena["king_blocks"], key=lambda k: k["half_rows"][0])
    if len(bridges) != 2 or len(kings) != 2:
        return [
            f"{tag}: arena.json has {len(bridges)} bridges / {len(kings)} king blocks, need 2 / 2"
        ], info
    centre = lambda lo_hi: Fraction(lo_hi[0] + lo_hi[1] + 1, 2) * half  # noqa: E731
    king_cx = centre(kings[0]["half_cols"])
    left_cx, right_cx = centre(bridges[0]["half_cols"]), centre(bridges[1]["half_cols"])
    sizes = {}
    for name in ("KingTower", "PrincessTower"):
        tw = next((x for x in doc["towers"] if x["name"] == name), None)
        v = tw.get("no_deploy_size_tiles") if tw else None
        if not (
            isinstance(v, list) and len(v) == 2 and all(isinstance(n, int) and n > 0 for n in v)
        ):
            fail.append(
                f"{tag}: {name} no_deploy_size_tiles = {v!r} (vacuity: need [W, H] positive ints)"
            )
        else:
            sizes[name] = v
    if fail:
        return fail, info

    def landmarks(unit: Fraction) -> list[tuple[str, Fraction, Fraction]]:
        kw = sizes["KingTower"][0] * unit / 2
        pw, ph = sizes["PrincessTower"][0] * unit / 2, sizes["PrincessTower"][1] * unit / 2
        return [
            ("king rect left edge is the arena's left wall", king_cx - kw, Fraction(0)),
            ("king rect right edge is the arena's right wall", king_cx + kw, width),
            (
                "left and right princess rects meet on the centre line (worst edge offset)",
                max(abs(left_cx + pw - width / 2), abs(right_cx - pw - width / 2)),
                Fraction(0),
            ),
            ("princess rect far edge is the far river bank", py + ph, far_bank),
        ]

    tiles_reading = landmarks(Fraction(1))
    half_reading = landmarks(Fraction(1, 2))
    for label, got, want in tiles_reading:
        if got != want:
            fail.append(
                f"{tag}: {label} -- got {float(got)} tiles, want {float(want)} (sizes {sizes})"
            )
    hits = sum(g == w for _, g, w in tiles_reading)
    half_hits = sum(g == w for _, g, w in half_reading)
    info.append(
        f"{tag}: NoDeploySize read as tiles hits {hits}/4 landmarks, as half-tiles {half_hits}/4 "
        f"(king {sizes['KingTower']}, princess {sizes['PrincessTower']})"
    )
    return fail, info


# --- plants ----------------------------------------------------------------------------


def _raw(t, tbl, name, col, value):
    t[tbl].records[name][col] = value


PLANTS = {
    # name: (aimed-at gate prefix, where it mutates, mutation)
    "ref": (
        "refs resolve: characters.Witch.SpawnCharacter",
        "tables",
        lambda t: _raw(t, "characters", "Witch", "SpawnCharacter", "Skeletonn"),
    ),
    "radius": (
        "collision radius: MiniPekka",
        "tables",
        lambda t: _raw(t, "characters", "MiniPekka", "CollisionRadius", 500),
    ),
    "prince": (
        "collision radius: Prince",  # the pinned disagreement must still see a move
        "tables",
        lambda t: _raw(t, "characters", "Prince", "CollisionRadius", 700),
    ),
    "speed": (
        "speed in allowed set: characters.Knight",
        "tables",
        lambda t: _raw(t, "characters", "Knight", "Speed", 61),
    ),
    "tiles": (
        "distance band: units.Musketeer.range_milli",  # a range read as tiles
        "tables",
        lambda t: _raw(t, "characters", "Musketeer", "Range", 6),
    ),
    "seconds": (
        "duration band: units.Giant.hit_speed_ms",  # a duration read as seconds
        "tables",
        lambda t: _raw(t, "characters", "Giant", "HitSpeed", 2),
    ),
    "missing": (
        "completeness: Tesla",
        "tables",
        lambda t: t["spells_buildings"].records.pop("Tesla"),
    ),
    "field": (
        "completeness: Fireball",
        "tables",
        lambda t: _raw(t, "projectiles", "FireballSpell", "Damage", None),
    ),
    "stun": (
        "archetype: Zap stun duration",
        "tables",
        lambda t: _raw(t, "area_effect_objects", "Zap", "BuffTime", None),
    ),
    "ladder": (
        "hog ladder",
        "tables",
        lambda t: t["rarities"].arrays["Rare"]["PowerLevelMultiplier"].__setitem__(2, 134),
    ),
    "float": (
        "no floats: cards.json",
        "doc",
        lambda d: d["units"]["Knight"].__setitem__("hitpoints", 660.0),
    ),
    "nodeploy": (
        "territory landmarks: king rect",  # a no-deploy size off by one tile
        "tables",
        lambda t: _raw(t, "buildings", "KingTower", "NoDeploySizeW", 17),
    ),
    "nodeploy_far_edge": (
        "territory landmarks: princess rect far edge",
        "tables",
        lambda t: _raw(t, "buildings", "PrincessTower", "NoDeploySizeH", 20),
    ),
    "nodeploy_meet": (
        "territory landmarks: left and right princess rects meet",
        "tables",
        lambda t: _raw(t, "buildings", "PrincessTower", "NoDeploySizeW", 12),
    ),
    "deco_damage": (
        "archetype: Arrows deco projectile carries no damage",
        "tables",
        lambda t: _raw(t, "projectiles", "ArrowsSpellDeco", "Damage", 115),
    ),
    "log_pushback_all": (
        "archetype: The Log pushes all ground troops",
        "tables",
        lambda t: _raw(t, "projectiles", "LogProjectileRolling", "PushbackAll", None),
    ),
    "unit_rarity": (
        "completeness: GoblinBarrel spawns 'Goblin', whose rarity",
        "tables",
        lambda t: _raw(t, "characters", "Goblin", "Rarity", None),
    ),
    "min_distance_tiles": (
        "distance band: projectiles.LogProjectile.min_distance_milli",  # read as tiles
        "tables",
        lambda t: _raw(t, "projectiles", "LogProjectile", "MinDistance", 3),
    ),
    "stale": (
        "derived cards.json fresh",
        "file",
        lambda f: f["units"]["Knight"].__setitem__("hitpoints", 661),
    ),
}


def load_json(p: Path):
    return json.loads(p.read_text(encoding="utf-8")) if p.exists() else None


def run_plant(name: str) -> bool:
    aimed_at, where, mutate = PLANTS[name]
    t = ec.load_tables()
    base_fail, _, _ = gate(t, ec.build(t), load_json(CARDS), load_json(GLOBALS))
    if base_fail:
        print(
            f"PLANT '{name}' INCONCLUSIVE -- the tree is already red without the plant, so "
            f"nothing it does is evidence. Fix these first:",
            file=sys.stderr,
        )
        for f in base_fail:
            print(f"   {f}", file=sys.stderr)
        return False

    t = ec.load_tables()
    file_doc = load_json(CARDS)
    if where == "tables":
        mutate(t)
    doc = ec.build(t)
    if where == "doc":
        mutate(doc)
    if where == "file":
        file_doc = copy.deepcopy(file_doc)
        mutate(file_doc)
    fail, _, _ = gate(t, doc, file_doc, load_json(GLOBALS))
    hit = [f for f in fail if f.startswith(aimed_at)]
    if hit:
        extra = len(fail) - len(hit)
        print(
            f"PLANT '{name}' LANDED -- '{aimed_at}' went red as intended"
            + (f" (+{extra} neighbouring gate(s) also red)" if extra else "")
            + ":"
        )
        for f in hit:
            print(f"   {f}")
        return True
    print(
        f"PLANT '{name}' DID NOT LAND -- '{aimed_at}' stayed GREEN with the defect present. "
        f"Red instead: {fail}",
        file=sys.stderr,
    )
    return False


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--plant", choices=sorted(PLANTS))
    ap.add_argument("--all-plants", action="store_true")
    args = ap.parse_args()

    print("*** " + ec.VINTAGE_WARNING)
    if args.plant:
        return 0 if run_plant(args.plant) else 1
    if args.all_plants:
        results = {p: run_plant(p) for p in PLANTS}
        landed = sum(results.values())
        print(
            f"\n{landed}/{len(results)} plants landed"
            + (
                ""
                if landed == len(results)
                else f"; NOT landed: {[p for p, ok in results.items() if not ok]}"
            )
        )
        return 0 if landed == len(results) else 1

    t = ec.load_tables()
    doc = ec.build(t)
    fail, warn, info = gate(t, doc, load_json(CARDS), load_json(GLOBALS))
    for i in info:
        print("  INFO " + i)
    for w in warn:
        print("  WARN " + w)
    if fail:
        print(f"DATA GATE FAILED ({len(fail)}):", file=sys.stderr)
        for f in fail:
            print(f"   {f}", file=sys.stderr)
        return 1
    print(
        f"DATA GATE green: {len(doc['cards'])} cards, {len(doc['units'])} units, "
        f"{len(ec.THIN_SLICE)} thin-slice cards complete, {len(warn)} warning(s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
