#!/usr/bin/env python3
"""Join Supercell's stat tables into data/derived/cards.json, the card data the
engine loads.

WHY THIS EXISTS
    A card is not a row.  Its elixir cost and rarity live in a spells_* table;
    its combat stats live on the character or building that spell summons; a
    ranged unit's damage lives on its PROJECTILE, not on the unit (Musketeer's
    Damage column is blank -- the 100 is on MusketeerProjectile); a spell's
    damage can be two hops away (The Log -> LogProjectile -> spawns
    LogProjectileRolling, which carries the 240).  Every consumer that
    re-derives this join gets one of those hops wrong, so it is done once, here,
    and every resolved number records the table.row.column it came from.

VINTAGE -- READ THIS
    Two vintages, selected with --vintage:

      2018       retroroyale's ~2018 client data (roster dating below).  Stats are
                 the 2018 LEVEL-1 base values under the 2018 rarity-relative level
                 system (Knight 660 HP), not modern card-level-11 values.
      15.535.29  the 15.535.29 client's own data (2026, a verified asset pack
                 decoded by tools/decode_sc_assets.py): the LIVE build's data.
                 Stats are UNIFIED LEVEL-1 base values: every base card's objects
                 (character, projectile, area effect) carry their own
                 `Rarity = "Common"` and scale on the Common ladder at the unified
                 level, whatever the CARD's rarity (Knight 690 HP at level 1, 1766
                 at the tournament level 11; the Rare Hog Rider 663 at level 1,
                 1697 at 11).  See LEVEL SCALING below.

    The default is 15.535.29 (the 2018 file stays reproducible and byte-identical:
    `--vintage 2018`, written to data/derived/cards-2018.json).  Every number in
    the output is EVIDENCE, NOT SPEC, whichever vintage: the 15.535 files are the
    client's own tables, but the engine's reading of each column is what
    calibration.json and the live captures settle.

THE 15.535 FILES ARE A CSV PLUS TOML OVERLAYS
    In the 15.535 build a characters.csv row is mostly just the Name (97 of 123
    rows carry nothing else); the stats live in csv_logic/characters/*.toml under
    [CHARACTER.Name] / [BUILDING.Name] / [PROJECTILE.Name] / [AEO.Name] /
    [BUFF.Name] / [ACTION.Name] / [SPELL_*.Name] sections, plus the table-level
    overlays (characters.toml, characters_evo.toml, buildings.toml, ...).  The
    effective row is CSV row + overlay, overlay wins; a unit that exists only in a
    TOML (ThreeMusketeer_Rework, GoblinHut_Rework) is a row of its own.  The
    loaders are tools/mechanic_register.py's (read_csv with its continuation-row
    folding; the [KIND.Name] routing and its cosmetic-field list); the overlay
    merge here is a TYPED twin of its merge_toml because the schema below carries
    ints and booleans, not strings.  Values on a CSV-typed column are coerced to
    the column's type and the build FAILS on a mismatch; a column the CSV lacks
    keeps its TOML type.  A column the 2018 schema reads that the 15.535 files do
    not carry at all (projectiles SpawnCharacterLevelIndex, character_buffs
    ImmuneToAntiMagic) is emitted as null and listed in provenance.columns_absent.

    A CARD WITHOUT A SummonCharacter (15.535: IceWizard, ElectroWizard and
    TriWizards deploy through an area effect whose OnStartingAction is an
    ActionSpawn; ThreeMusketeers through SummonCharactersList) has its unit
    resolved by walking that graph -- AreaEffectObject -> OnStartingAction ->
    ActionGroup SubActions / ActionSpawn(ToLocation) SpawnData, AreaEffectType
    hops followed -- and the walk is recorded on the card as `summon_resolution`.

    EXCLUDED FROM `cards`: spells_evolved.csv (109 rows, 68 NotInUse; every row
    summons a distinct *_EV1 character from characters_evo.toml) and
    spells_hero_form.csv (113 rows: each *_hero row is the base card's row with
    the _hero suffix -- 69 of the 77 with a base card are identical to it column
    for column, the other 8 differ in icons / EvolvedSpells / SummonRadius -- and
    the hero characters are [EXT.*] sections extending the base character in
    characters/hero_form/*.toml).  Both are recorded under `excluded_tables`.  The
    *_EV1 units ARE in `units` (they are character rows); nothing releases them.

CONTINUATION ROWS
    Supercell's CSVs express per-level arrays as rows with a blank Name that
    follow the named row.  Handled deliberately: every blank-Name row is folded
    into the preceding named row as element 1..n of a per-column array (element
    0 is the named row's own value, positions preserved, blanks kept as null).
    A blank-Name row before any named row is a hard error.  In the 2018 data only
    rarities.csv uses them among the joined tables; the stat tables
    (characters/buildings/projectiles/spells/area effects/buffs) have ZERO, and
    the extractor prints that count every run so a future data set that starts
    using per-level arrays for stats is noticed instead of truncated to level 1.
    In 15.535 the stat tables DO carry them, for StringArray columns only
    (characters.csv IgnoreBuff / AttackSequence, buildings.csv BoostItems,
    spells_characters.csv Tribe): they fold into `list_columns` on the unit, never
    into a stat (the build FAILS if a continuation row ever carries a column the
    schema reads as a stat, SCALAR_STAT_COLUMNS below).

LEVEL SCALING
    rarities.csv PowerLevelMultiplier is a per-rarity column of percentages.
    Row i (0-based) is the multiplier for level i+2; level 1 is the base stat
    (100%).  The final row of each rarity is the "no further upgrade" row
    (UpgradeCost 0) and its multiplier is therefore for a level that does not
    exist -- kept in the raw table, not in the per-level ladder.  Verified here
    against the published Hog Rider ladder, and the build FAILS if it drifts.
    15.535: LevelCount Common 16 / Rare 14 / Epic 11 / Legendary 8 / Champion 6
    (RelativeLevel 0 / 2 / 5 / 8 / 10), and a TournamentLevelIndex column (the
    0-based rarity-LOCAL level of unified level 11: 10 / 8 / 5 / 2 / 0), emitted per
    rarity as `tournament_level_index`.  The ladder itself is the 2018 one extended:
    110, 121, 133, 146, 160, 176, 193, 212, 233, 256, 281, 309, 339, 372, 409, 450,
    ... so the Hog Rider check still holds shape for shape.

    WHICH LADDER, AND FROM WHICH LEVEL (calibration.json combat.STAT_BASE_LEVEL,
    `LEVEL_BASE_READING` below, written on every card as `level_scaling.reading`):
    a stat is the OBJECT's own Rarity column's local level 1, scaled on that
    rarity's ladder at unified level L - RelativeLevel(object rarity); the CARD's
    rarity only bounds the levels the card can be played at.  In the 2018 data the
    character row carried the card's rarity (HogRider Rare, 800 at unified 3), so
    the object ladder and the card ladder coincide and the 2018 file is unchanged
    (its one exception, the Skeleton Army's Common Skeleton on the Epic ladder, is
    frozen with the file: tests/stacked_tie.rs's format-3 fixture was recorded
    against it).  In 15.535 every base object says Common and the base is the
    unified level-1 value: Knight 690 -> 1766 at 11, HogRider 663 -> 1697, Musketeer
    282 -> 721, Giant 1550 -> 3968, all floor(base x ladder / 100) -- MEASURED on the
    16.402 live captures' `level` / `max_hp` columns (tools/make_live_levels_fixture.py
    -> crates/royalesim/tests/fixtures/live_levels.json, pinned by
    crates/royalesim/tests/levels.rs).  The other candidate, the card's own ladder
    at its local level (Hog Rider 663 x 212 % = 1405 at 11), is refuted by the same
    rows and stays runnable as `--level-base card_rarity_local_1`.  So for 15.535
    `level_scaling.multiplier_percent_by_level` is the LADDER rarity's list (16
    entries, unified levels 1..16), `base_level` the unified level of the base stat,
    `level_count` / `relative_level` the card rarity's range.

USAGE
    python tools/extract_cards.py                     # 15.535.29 (the default), writes cards.json
    python tools/extract_cards.py --vintage 2018      # the 2018 file, byte-identical to before, cards-2018.json
    python tools/extract_cards.py --summary           # also print the thin slice
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mechanic_register as mr  # the 15.535 loaders live there

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "derived" / "cards.json"



@dataclass(frozen=True)
class Vintage:
    key: str
    raw: Path
    version: str
    warning: str
    source: str
    vintage: str
    roster_dating: str
    # CSV per table key (the joined tables).
    sources: dict[str, str]
    # 15.535 only: TOML overlays per table key, in application order, and the
    # per-character overlay directory routed in by [KIND.Name] section.
    overlays: dict[str, list[str]] = field(default_factory=dict)
    # csv_logic subdirectories of per-object overlays ([KIND.Name] sections):
    # characters/ (the roster) and events/ (event-mode cards such as SuperKnight,
    # which spells_characters.csv lists with NotVisible = TRUE).
    character_dirs: tuple[str, ...] = ()

    @property
    def is_2018(self) -> bool:
        return self.key == "2018"


SOURCES_2018 = {
    "characters": "characters.csv",
    "buildings": "buildings.csv",
    "projectiles": "projectiles.csv",
    "spells_characters": "spells_characters.csv",
    "spells_buildings": "spells_buildings.csv",
    "spells_other": "spells_other.csv",
    "area_effect_objects": "area_effect_objects.csv",
    # Not in the original join list, but Zap's stun lives here (ZapFreeze) and a
    # stun spell without its stun is not the card.
    "character_buffs": "character_buffs.csv",
    "rarities": "rarities.csv",
}

VINTAGES = {
    "2018": Vintage(
        key="2018",
        raw=ROOT / "data" / "raw" / "retroroyale-2018" / "csv_logic",
        version="cards-2018.1",
        warning=(
            "PRE-2025 VINTAGE (~2018 client data). The simulator targets the LIVE 2026 game. "
            "Every number in this file is EVIDENCE, NOT SPEC. Stats are 2018 level-1 base values "
            "under the 2018 rarity-relative level system; balance changes since 2018 are NOT reflected."
        ),
        source="retroroyale/ClashRoyale GameAssets csv_logic/",
        vintage="~2018 client data (PRE-2025)",
        roster_dating=(
            "INFERENCE, medium-low confidence: ships Mega Knight, Bandit (Assassin), "
            "Skeleton Barrel (SkeletonBalloon), Flying Machine (DartBarrell) and Cannon Cart "
            "(MovingCannon); no row resembles Wall Breakers, Royal Ghost or Magic Archer. "
            "Consistent with a client from roughly the first half of 2018."
        ),
        sources=SOURCES_2018,
    ),
    "15.535.29": Vintage(
        key="15.535.29",
        raw=ROOT / "data" / "raw" / "cr-15.535.29" / "csv_logic",
        version="cards-15535.1",
        warning=(
            "LIVE 2026 BUILD (the 15.535.29 client's asset pack, decoded by tools/decode_sc_assets.py). "
            "The simulator targets the LIVE 2026 game and this is that game's own card data (the live "
            "captures are 16.402). Every number in this file is EVIDENCE, NOT SPEC: "
            "the columns are the client's, what the engine does with each is calibration.json's. "
            "Stats are UNIFIED LEVEL-1 base values: every base object's own Rarity column says Common, "
            "and a stat scales on the OBJECT's rarity ladder at unified level L - its RelativeLevel "
            "(calibration.json combat.STAT_BASE_LEVEL = object_rarity_local_1, measured on the 16.402 "
            "live captures: Knight 690 -> 1766 at level 11, Hog Rider 663 -> 1697, floor). The CARD's "
            "rarity only bounds its levels, under the 15.535 rarities.csv (LevelCount Common 16 / "
            "Rare 14 / Epic 11 / Legendary 8 / Champion 6, RelativeLevel 0 / 2 / 5 / 8 / 10; the "
            "tournament level is unified 11 = TournamentLevelIndex 10 / 8 / 5 / 2 / 0, the 0-based "
            "local level). Effective row = CSV row + TOML overlay (overlay wins); evolutions and hero "
            "forms are excluded from `cards`."
        ),
        source="Supercell csv_logic/ of the 15.535.29 client, decoded by tools/decode_sc_assets.py "
        "(data/raw/cr-15.535.29/MANIFEST.json carries the per-file hashes of the decode)",
        vintage="15.535.29 client (2026, LIVE build family)",
        roster_dating=(
            "The client version is in the file (MANIFEST.json version 15.535.29); the roster is the "
            "live 2026 one (Boss Bandit, Merge Maiden, Ronin, Goblinstein, Berserker, Suspicious Bush "
            "all present)."
        ),
        sources=SOURCES_2018,
        overlays={
            "characters": ["characters.toml", "characters_evo.toml"],
            "buildings": ["buildings.toml", "buildings_evo.toml"],
            "projectiles": ["projectiles.toml", "projectiles_evo.toml"],
            "area_effect_objects": ["area_effect_objects.toml", "area_effect_objects_evo.toml"],
            "character_buffs": ["character_buffs.toml", "character_buffs_evo.toml"],
            "spells_characters": ["spells_characters.toml"],
            "spells_buildings": ["spells_buildings.toml"],
            "spells_other": ["spells_other.toml"],
            "actions": ["actions.toml"],
        },
        character_dirs=("characters", "events"),
    ),
}
# The 15.535 file became the default once it passed every gate (cargo test, clippy,
# tools/check_data.py).
DEFAULT_VINTAGE = "15.535.29"

# Where each vintage's file goes: the default vintage IS cards.json (what the engine
# loads); any other vintage sits beside it under its own name, so the two can be
# regenerated together (tests/stacked_tie.rs's format-3 fixture reads the 2018 one).
def default_out(vintage: str) -> Path:
    return OUT if vintage == DEFAULT_VINTAGE else OUT.with_name(f"cards-{vintage}.json")


# calibration.json combat.STAT_BASE_LEVEL (module doc, LEVEL SCALING): which ladder
# a stat scales on and from which level. The shipped reading is the OBJECT's own
# Rarity row (measured live); the other candidate is runnable with --level-base.
LEVEL_BASE_READINGS = ("object_rarity_local_1", "card_rarity_local_1")
LEVEL_BASE_READING = LEVEL_BASE_READINGS[0]

# The per-character overlay sections, routed to a table key (mechanic_register's
# SECTION_KIND folds characters and buildings into one CHARACTER table; the 2018
# schema keeps `source_table`, so here [CHARACTER.] and [BUILDING.] stay apart).
SECTION_TABLE = {
    "CHARACTER": "characters",
    "BUILDING": "buildings",
    "PROJECTILE": "projectiles",
    "AEO": "area_effect_objects",
    "BUFF": "character_buffs",
    "ACTION": "actions",
    "SPELL_CHARACTER": "spells_characters",
    "SPELL_BUILDING": "spells_buildings",
    "SPELL_OTHER": "spells_other",
}
# Sections that describe evolutions, hero forms, abilities, stat display,
# extensions and shapes: not part of a base card's row. SPELL_EVOLVED / SPELL_HERO
# are excluded on purpose (module doc); the rest are mechanic_register.SKIP_SECTIONS
# plus the tables this schema does not join.
SKIP_SECTIONS = (mr.SKIP_SECTIONS - {"EXT"}) | {"SPELL_EVOLVED", "SPELL_HERO", "ABILITY", "CARD_GROUP", "FILTER"}
EXCLUDED_TABLES = {
    "spells_evolved.csv": (
        "evolutions: every row summons a distinct *_EV1 character (characters_evo.toml / "
        "buildings_evo.toml); 68 of 109 rows are NotInUse. Not a base card; not in `cards`."
    ),
    "spells_hero_form.csv": (
        "hero forms: each *_hero row is the base card's row with the _hero suffix (69 of the 77 "
        "with a base card identical column for column; the other 8 differ only in icons, "
        "EvolvedSpells or SummonRadius) summoning the BASE character; the hero characters are "
        "[EXT.*] sections in characters/hero_form/*.toml. Not in `cards`."
    ),
}

# Backwards-compatible module names (tools/check_data.py reads these): the DEFAULT
# vintage's paths and warning.
VINTAGE = VINTAGES[DEFAULT_VINTAGE]
RAW = VINTAGE.raw
VINTAGE_WARNING = VINTAGE.warning
SOURCES = VINTAGE.sources

# The thin slice: display name -> the internal name Supercell keys it by.
# The engine should key on the internal name; display names are for humans.
THIN_SLICE = {
    "Knight": "Knight",
    "Archers": "Archer",
    "Musketeer": "Musketeer",
    "Giant": "Giant",
    "Hog Rider": "HogRider",
    "Minions": "Minions",
    "Baby Dragon": "BabyDragon",
    "Valkyrie": "Valkyrie",
    "Skeleton Army": "SkeletonArmy",
    "Cannon": "Cannon",
    "Tesla": "Tesla",
    "Fireball": "Fireball",
    "Zap": "Zap",
    "Arrows": "Arrows",
    "The Log": "Log",
    "Prince": "Prince",
    "Wizard": "Wizard",
    "Goblin Barrel": "GoblinBarrel",
}
TOWERS = ["PrincessTower", "KingTower"]

# Internal -> public name where CamelCase splitting does not give it.  Best
# effort, for readability only; `name` is the key.  Assassin=Bandit is confirmed
# by its FileName (sc/chr_bandit.sc); the rest are community-known code names.
DISPLAY_OVERRIDES = {
    "Archer": "Archers",
    "Log": "The Log",
    "Pekka": "P.E.K.K.A",
    "MiniPekka": "Mini P.E.K.K.A",
    "Assassin": "Bandit",
    "ZapMachine": "Sparky",
    "DartBarrell": "Flying Machine",
    "MovingCannon": "Cannon Cart",
    "SkeletonBalloon": "Skeleton Barrel",
    "Xbow": "X-Bow",
    "AxeMan": "Executioner",
    "AngryBarbarians": "Elite Barbarians",
    "RageBarbarian": "Lumberjack",
    "DarkWitch": "Night Witch",
    "BlowdartGoblin": "Dart Goblin",
    "SkeletonWarriors": "Guards",
    "FirespiritHut": "Furnace",
    "IceGolemite": "Ice Golem",
    "Elixir Collector": "Elixir Collector",
}

# Columns that are presentation-only; dropped from the `raw` passthrough so the
# engine partition is not tempted to read an animation name as a mechanic.
COSMETIC = re.compile(
    r"(Export|Effect|Shadow|FileName|^TID|Sound|Anim|HealthBar|^Scale$|Icon|Filter|Trail|"
    r"Crowd|Glow|Frame|Indicator|^Red$|^Green$|^Blue$|SortOrder|CastSound|ShowHealthNumber|"
    r"ProjectileStart|AttachedCharacterHeight|TurretMovement|HasRotationOnTimeline|RotateAngleSpeed|"
    r"ProjectileYOffset|use360Frames|DontStopMoveAnim|AttackShakeTime|VisualHitSpeed|TargetEffectY|"
    r"ReleaseDate|UnlockArena|StatsUnderInfo|DamageExportName)"
)
# Kept in `raw` even though mechanic_register's wider cosmetic list would drop them
# (15.535 only; the 2018 `raw` never went through that list).
KEEP_IN_RAW = {"Name", "Rarity", "NotInUse", "Mirror"}


# --- reading ------------------------------------------------------------------


class Table:
    """One Supercell CSV: name row, type row, data rows, continuation rows folded."""

    def __init__(self, key: str, path: Path):
        self.key = key
        self.path = path
        self.files = [path]
        rows = list(csv.reader(path.open(encoding="utf-8-sig")))
        self.header = rows[0]
        self.types = [t.strip().lower() for t in rows[1]]
        if len(self.types) != len(self.header):
            raise SystemExit(f"{path}: {len(self.header)} names but {len(self.types)} types")
        self.records: dict[str, dict] = {}
        self.arrays: dict[str, dict[str, list]] = {}
        self.continuation_rows = 0
        current = None
        for lineno, r in enumerate(rows[2:], start=3):
            r = r + [""] * (len(self.header) - len(r))
            name = r[0].strip()
            if name:
                if name in self.records:
                    raise SystemExit(f"{path}:{lineno}: duplicate Name {name!r}")
                current = name
                self.records[name] = {
                    h: self._conv(h, i, r[i], lineno) for i, h in enumerate(self.header)
                }
                self.arrays[name] = {}
            else:
                if current is None:
                    raise SystemExit(f"{path}:{lineno}: continuation row before any named row")
                self.continuation_rows += 1
                depth = 1 + max((len(v) - 1 for v in self.arrays[current].values()), default=0)
                for i, h in enumerate(self.header[1:], start=1):
                    arr = self.arrays[current].setdefault(h, [self.records[current][h]])
                    arr.extend([None] * (depth - len(arr)))
                    arr.append(self._conv(h, i, r[i], lineno))
        # drop array columns that never received a value past element 0
        for cols in self.arrays.values():
            for h in [h for h, v in cols.items() if all(x is None for x in v[1:])]:
                del cols[h]

    def _conv(self, h: str, i: int, v: str, lineno: int):
        v = v.strip()
        if v == "":
            return None
        t = self.types[i]
        if t == "int":
            try:
                return int(v)
            except ValueError:
                raise SystemExit(
                    f"{self.path}:{lineno}: column {h} typed int holds {v!r}"
                ) from None
        if t == "boolean":
            lv = v.lower()
            if lv not in ("true", "false"):
                raise SystemExit(f"{self.path}:{lineno}: column {h} typed boolean holds {v!r}")
            return lv == "true"
        return v

    def get(self, name: str | None) -> dict | None:
        return None if name is None else self.records.get(name)

    def has_column(self, col: str) -> bool:
        return col in self.header


class Row(dict):
    """A record of an OverlayTable. An absent column reads as None (the 15.535
    files do not carry every column the 2018 schema reads) and the table's column
    set says whether the column exists at all -- `flag()` below turns "absent" into
    null rather than false."""

    __slots__ = ("columns",)

    def __init__(self, columns: set[str], *a, **k):
        super().__init__(*a, **k)
        self.columns = columns

    def __missing__(self, key):
        return None

    def has_column(self, col: str) -> bool:
        return col in self.columns


# Every column the normalisers below read as a SCALAR stat. A continuation row that
# carries one of these would be a per-level array this schema truncates to level 1
# (module doc, CONTINUATION ROWS): the build refuses it. Any other continued column
# (15.535: Monk's AttackSequence 1 / 2, the IgnoreBuff and BoostItems lists) is a
# list column, folded into `list_columns`, never into a stat. rarities.csv is
# exempt: PowerLevelMultiplier IS the per-level array, decoded by `rarity_table`.
SCALAR_STAT_COLUMNS = {
    "Rarity", "Hitpoints", "Damage", "HitSpeed", "LoadTime", "LoadFirstHit", "Speed",
    "StopMovementAfterMS", "WaitMS", "Range", "MinimumRange", "SightRange", "CollisionRadius",
    "Mass", "DeployTime", "AttacksAir", "AttacksGround", "TargetOnlyBuildings", "FlyingHeight",
    "AreaDamageRadius", "SelfAsAoeCenter", "Projectile", "ShieldHitpoints",
    "CrownTowerDamagePercent", "LifeTime", "IgnorePushback", "TileSizeOverride", "DeathDamage",
    "DeathDamageRadius", "DeathSpawnCharacter", "DeathSpawnCount", "DeathSpawnRadius",
    "DeathSpawnDeployTime", "DeathAreaEffect", "SpawnCharacter", "SpawnNumber", "SpawnInterval",
    "SpawnStartTime", "SpawnPauseTime", "SpawnLimit", "SpawnRadius", "DamageSpecial", "ChargeRange",
    "ChargeSpeedMultiplier", "DashDamage", "DashMinRange", "DashMaxRange", "DashRadius",
    "DashCooldown", "DashImmuneToDamageTime", "DashPushBack", "JumpEnabled", "JumpHeight",
    "JumpSpeed", "HidesWhenNotAttacking", "HideTimeMs", "UpTimeMs", "BuffOnDamage",
    "BuffOnDamageTime", "AttachedCharacter", "NoDeploySizeW", "NoDeploySizeH",
    "ProjectileStartRadius", "Kamikaze", "KamikazeTime",
    # projectiles / area effects / buffs / spells
    "Homing", "Radius", "RadiusY", "AoeToAir", "AoeToGround", "OnlyEnemies", "Pushback",
    "PushbackAll", "MaximumTargets", "ProjectileRadius", "ProjectileRadiusY", "ProjectileRange",
    "MinDistance", "SpawnCharacterCount", "SpawnCharacterDeployTime", "SpawnCharacterLevelIndex",
    "SpawnAreaEffectObject", "TargetBuff", "BuffTime", "SpawnProjectile", "LifeDuration",
    "NoEffectToCrownTowers", "Buff", "OnlyOwnTroops", "HitsGround", "HitsAir", "IgnoreBuildings",
    "SpawnMaxCount", "SpawnInitialDelay", "SpeedMultiplier", "HitSpeedMultiplier",
    "SpawnSpeedMultiplier", "DamagePerSecond", "HitFrequency", "HealPerSecond", "DamageReduction",
    "DamageMultiplier", "ImmuneToAntiMagic", "AttractPercentage", "ManaCost", "NotInUse",
    "CustomDeployTime", "SummonCharacter", "SummonNumber", "SummonRadius", "SummonCharacterSecond",
    "SummonCharacterSecondCount", "SummonWidth", "SummonDeployDelay", "SummonDeployDelaySecond",
    "SpawnAngleShift", "CustomFirstProjectile", "AreaEffectObject", "InstantDamage",
    "MultipleProjectiles", "ProjectileWaves", "ProjectileWaveInterval", "SpellAsDeploy",
    "CanPlaceOnBuildings", "CanDeployOnEnemySide", "DurationSeconds",
}


def csv_types(path: Path) -> tuple[list[str], dict[str, str], int, int]:
    """(header, column -> type, continuation rows, blank rows) of a Supercell CSV."""
    rows = list(csv.reader(path.open(encoding="utf-8-sig")))
    header, types = rows[0], [t.strip().lower() for t in rows[1]]
    if len(types) != len(header):
        raise SystemExit(f"{path}: {len(header)} names but {len(types)} types")
    cont = blank = 0
    guard = path.name != "rarities.csv"
    for lineno, r in enumerate(rows[2:], start=3):
        if r and r[0].strip():
            continue
        if any(c.strip() for c in r):
            cont += 1
            for i, c in enumerate(r):
                if guard and c.strip() and header[i] in SCALAR_STAT_COLUMNS:
                    raise SystemExit(
                        f"{path}:{lineno}: continuation row carries the stat column {header[i]}"
                    )
        else:
            blank += 1
    return header, dict(zip(header, types, strict=True)), cont, blank


# Operator lists in the evolution overlays: `HitSpeed = ["=", 4700]`, `Radius = ["+", 500]`,
# `Damage = ["%", 50]` on a row that names a `Base` row. "=" assigns; "+" adds to the
# base's value; "%" is READ AS percent-of-base (tdiv(base x arg, 100)) -- a guess,
# recorded on the row as `base_ops` so no consumer mistakes it for a column.
BASE_OPS = {"=", "+", "-", "%"}


def coerce(where: str, col: str, typ: str | None, v, allow_floats: bool = False):
    """A TOML overlay value onto a CSV-typed column (or its own type when the CSV
    lacks the column). No floats anywhere the schema emits: the engine is
    integer-only (the actions table, never emitted, may carry them)."""
    if isinstance(v, float):
        if allow_floats:
            return v
        raise SystemExit(f"{where}.{col}: float {v!r} (no floats in card data)")
    if isinstance(v, str) and v.strip() == "":
        # `DeathSpawnCharacter = ""` (RageBarbarian), `SpawnCharacter = ""`
        # (GoblinHut_Rework): the overlay CLEARS the column -- a blank cell, None.
        return None
    if typ is None:
        return v
    if typ == "int":
        if isinstance(v, bool):
            raise SystemExit(f"{where}.{col}: typed int, TOML gives boolean {v!r}")
        if isinstance(v, int):
            return v
        if isinstance(v, str):
            try:
                return int(v.strip())
            except ValueError:
                raise SystemExit(f"{where}.{col}: typed int, TOML gives {v!r}") from None
        raise SystemExit(f"{where}.{col}: typed int, TOML gives {type(v).__name__}")
    if typ == "boolean":
        if isinstance(v, bool):
            return v
        if isinstance(v, str) and v.strip().lower() in ("true", "false"):
            return v.strip().lower() == "true"
        raise SystemExit(f"{where}.{col}: typed boolean, TOML gives {v!r}")
    # string / String / StringArray
    if isinstance(v, bool):
        return "TRUE" if v else "FALSE"
    return str(v) if not isinstance(v, str) else v


class OverlayTable:
    """A 15.535 table: the CSV (through mechanic_register.read_csv, its
    continuation rows folded into list columns) with its TOML overlays applied,
    overlay wins. Same surface as `Table` (records / arrays / header / get /
    continuation_rows / has_column) so the normalisers do not know which they hold.
    A table with no CSV (actions) starts empty and is built from overlays alone."""

    def __init__(self, key: str, path: Path | None):
        self.key = key
        self.path = path
        self.files: list[Path] = []
        self.header: list[str] = []
        self.types: dict[str, str] = {}
        self.columns: set[str] = set()
        self.records: dict[str, Row] = {}
        self.arrays: dict[str, dict[str, list]] = {}
        self.continuation_rows = 0
        self.blank_rows = 0
        # name -> the overlay files that touched it (provenance for a stat's origin)
        self.overlaid: dict[str, list[str]] = {}
        # name -> the columns an overlay SET (a base row fills only the others)
        self.set_fields: dict[str, set[str]] = {}
        # name -> {col: (op, arg)} operator lists awaiting the base row's value
        self.pending_ops: dict[str, dict[str, tuple[str, int]]] = {}
        self.allow_floats = path is None
        if path is not None:
            self.files.append(path)
            self.header, self.types, self.continuation_rows, self.blank_rows = csv_types(path)
            self.columns = set(self.header)
            for name, cols in mr.read_csv(path).items():
                rec = Row(self.columns, {h: None for h in self.header})
                self.arrays[name] = {}
                for col, vals in cols.items():
                    typ = self.types[col]
                    conv = [self._conv(f"{path.name}:{name}", col, typ, v) for v in vals]
                    rec[col] = conv[0]
                    if len(conv) > 1:
                        self.arrays[name][col] = conv
                self.records[name] = rec

    @staticmethod
    def _conv(where: str, col: str, typ: str, v: str):
        v = v.strip()
        if typ == "int":
            try:
                return int(v)
            except ValueError:
                raise SystemExit(f"{where}: column {col} typed int holds {v!r}") from None
        if typ == "boolean":
            lv = v.lower()
            if lv not in ("true", "false"):
                raise SystemExit(f"{where}: column {col} typed boolean holds {v!r}")
            return lv == "true"
        return v

    def overlay(self, source: Path, body: dict, label: str) -> None:
        """Apply one {Name: {field: value}} mapping (a table-level TOML, or one
        [KIND.*] section of a per-character TOML). A field whose value is a list
        becomes element 0 plus the list in `arrays`; an inline table (a
        [KIND.Name.Field] sub-object such as StatsTags) is kept as a dict and never
        read as a stat."""
        if source not in self.files:
            self.files.append(source)
        for name, fields in body.items():
            if not isinstance(fields, dict):
                raise SystemExit(f"{label}: [{name}] is not a table")
            rec = self.records.get(name)
            if rec is None:
                rec = Row(self.columns, {h: None for h in self.header})
                self.records[name] = rec
                self.arrays[name] = {}
            self.overlaid.setdefault(name, []).append(label)
            seen = self.set_fields.setdefault(name, set())
            for col, v in fields.items():
                self.columns.add(col)
                seen.add(col)
                typ = self.types.get(col)
                where = f"{label}.{name}"
                if isinstance(v, dict):
                    rec[col] = v
                    continue
                if isinstance(v, list):
                    if len(v) == 2 and isinstance(v[0], str) and v[0] in BASE_OPS:
                        arg = coerce(where, col, typ, v[1], self.allow_floats)
                        if v[0] == "=":
                            rec[col] = arg
                        else:
                            self.pending_ops.setdefault(name, {})[col] = (v[0], arg)
                        self.arrays[name].pop(col, None)
                        continue
                    conv = [coerce(where, col, typ, x, self.allow_floats) for x in v]
                    rec[col] = conv[0] if conv else None
                    if len(conv) > 1:
                        self.arrays[name][col] = conv
                    elif col in self.arrays[name]:
                        del self.arrays[name][col]
                    continue
                rec[col] = coerce(where, col, typ, v, self.allow_floats)
                self.arrays[name].pop(col, None)

    def resolve_bases(self, lookup) -> None:
        """`Base = "X"` (the evolution overlays; "KIND.X" strips the kind): the row
        inherits every column it did not set from X's EFFECTIVE row, then the
        pending operator lists are applied against X's value. `lookup(name)` finds a
        row in this table or its sibling (characters <-> buildings)."""
        done: set[str] = set()

        def resolve(name: str, chain: tuple[str, ...]) -> None:
            if name in done:
                return
            if name in chain:
                raise SystemExit(f"{self.key}: Base cycle {(*chain, name)}")
            rec = self.records[name]
            base_name = rec.get("Base")
            if isinstance(base_name, str) and base_name:
                base_name = base_name.split(".")[-1]
                base_tbl, base = lookup(base_name)
                if base is None:
                    raise SystemExit(f"{self.key}.{name}: Base {base_name!r} is not a row")
                if base_tbl is self:
                    resolve(base_name, (*chain, name))
                mine = self.set_fields.get(name, set())
                for col, bv in base.items():
                    if col == "Base" or col in mine or isinstance(bv, dict):
                        continue
                    if rec[col] is None and bv is not None:
                        rec[col] = bv
                        self.columns.add(col)
                for col, arr in base_tbl.arrays.get(base_name, {}).items():
                    if col not in mine:
                        self.arrays[name].setdefault(col, list(arr))
                ops = self.pending_ops.pop(name, {})
                if ops:
                    applied = []
                    for col, (op, arg) in ops.items():
                        bv = base[col]
                        if not isinstance(bv, int) or isinstance(bv, bool) or not isinstance(arg, int):
                            raise SystemExit(
                                f"{self.key}.{name}.{col}: {op!r} against base {base_name}.{col} = {bv!r}"
                            )
                        if op == "+":
                            val = bv + arg
                        elif op == "-":
                            val = bv - arg
                        else:  # "%"
                            val = bv * arg // 100
                        rec[col] = val
                        applied.append(
                            {
                                "column": col,
                                "op": op,
                                "arg": arg,
                                "base": base_name,
                                "base_value": bv,
                                "value": val,
                                "reading": "guess: % as percent of the base value",
                            }
                        )
                    rec["base_ops"] = applied
            elif name in self.pending_ops:
                raise SystemExit(f"{self.key}.{name}: operator list without a Base row")
            done.add(name)

        for name in list(self.records):
            resolve(name, ())

    def get(self, name: str | None) -> Row | None:
        return None if name is None else self.records.get(name)

    def has_column(self, col: str) -> bool:
        return col in self.columns


class Tables(dict):
    """The loaded tables of one vintage (`t["characters"]`, ...) plus the vintage
    and the level-base reading the build writes (`LEVEL_BASE_READINGS`)."""

    def __init__(self, vintage: Vintage):
        super().__init__()
        self.vintage = vintage
        self.columns_absent: list[str] = []
        self.level_base = LEVEL_BASE_READING


def load_tables(vintage: str | Vintage | None = None) -> Tables:
    v = VINTAGES[vintage] if isinstance(vintage, str) else (vintage or VINTAGE)
    t = Tables(v)
    if v.is_2018:
        for k, f in v.sources.items():
            t[k] = Table(k, v.raw / f)
        return t
    if not v.raw.is_dir():
        raise SystemExit(
            f"missing {v.raw}: decode the {v.key} assets first (tools/decode_sc_assets.py)"
        )
    for k, f in v.sources.items():
        t[k] = OverlayTable(k, v.raw / f)
    t["actions"] = OverlayTable("actions", None)
    for k, files in v.overlays.items():
        for f in files:
            p = v.raw / f
            if not p.is_file():
                raise SystemExit(f"missing overlay {p}")
            t[k].overlay(p, tomllib.load(p.open("rb")), f)
    for sub in v.character_dirs:
        for p in sorted((v.raw / sub).glob("*.toml")):
            doc = tomllib.load(p.open("rb"))
            for section, body in doc.items():
                if section == "EXT" and isinstance(body, dict):
                    # [EXT.Name] Base = "KIND.Other": a real object of KIND that extends
                    # Other (the three Musketeers of ThreeMusketeers are three EXTs of
                    # ThreeMusketeer_Rework). Routed by the Base's kind; `resolve_bases`
                    # fills the inherited columns. (characters/hero_form/ is not globbed,
                    # so the hero-form EXTs stay out, module doc.)
                    for ext_name, ext in body.items():
                        base = ext.get("Base") if isinstance(ext, dict) else None
                        kind = base.split(".")[0] if isinstance(base, str) and "." in base else None
                        ext_key = SECTION_TABLE.get(kind or "")
                        if ext_key is None:
                            raise SystemExit(f"{p.name}: [EXT.{ext_name}] Base {base!r} names no table")
                        t[ext_key].overlay(p, {ext_name: ext}, f"{sub}/{p.name} [EXT]")
                    continue
                key = SECTION_TABLE.get(section)
                if key is None:
                    if section not in SKIP_SECTIONS:
                        raise SystemExit(f"{p.name}: unknown section [{section}]")
                    continue
                # A [CHARACTER.X] that says IsBuilding is a building row (none does
                # in 15.535, every building overlay is a [BUILDING.] section; kept
                # so a future file cannot file a building under `characters`).
                if key == "characters" and isinstance(body, dict):
                    moved = {
                        n: f for n, f in body.items() if isinstance(f, dict) and f.get("IsBuilding") is True
                    }
                    if moved:
                        t["buildings"].overlay(p, moved, f"{sub}/{p.name} [{section}]")
                        body = {n: f for n, f in body.items() if n not in moved}
                t[key].overlay(p, body, f"{sub}/{p.name} [{section}]")

    def unit_lookup(name: str):
        for k in ("characters", "buildings"):
            r = t[k].get(name)
            if r is not None:
                return t[k], r
        return None, None

    for k, tb in t.items():
        if isinstance(tb, OverlayTable):
            if k in ("characters", "buildings"):
                tb.resolve_bases(unit_lookup)
            else:
                tb.resolve_bases(lambda n, tb=tb: (tb, tb.get(n)))
    return t


# --- normalisation helpers -------------------------------------------------------


def ct_percent(raw: int | None) -> int:
    """Effective crown-tower damage percent.

    HYPOTHESIS, not verified: a NEGATIVE raw value is a delta (Fireball -60 ->
    40%, matching the community-recorded 2018 spell tower damage of 40%); a
    blank is 100%. No positive value occurs in this data, so how a positive
    value would be read is untested and treated as absolute.
    """
    if raw is None:
        return 100
    return 100 + raw if raw < 0 else raw


def flag(rec: dict, col: str) -> bool | None:
    """A boolean column: a blank cell is false (Supercell loader default); a column
    the TABLE does not carry at all is null, not false."""
    if isinstance(rec, Row) and not rec.has_column(col):
        return None
    return bool(rec[col])


# THE ACTION GRAPH (15.535 only). A row's *Action columns (OnStartingAction,
# OnDeathAction, OnAttackAction, ...) name scripted actions in actions.toml; a
# reworked card keeps its mechanic THERE and clears the columns this schema reads
# (GoblinHut_Rework: SpawnNumber 0, OnStartingAction = an ActionGoblinHutLifeState
# that spawns a Spear Goblin every 2200 ms; Furnace_rework: an ActionInterval ->
# ActionSpawnToLocation every 5000 ms). The graph is walked and summarised on the
# object as `action_graph` so the engine can REFUSE a card whose mechanic it would
# otherwise run as a plainer card. A class is COSMETIC when it only plays effects,
# sounds, animations or health-bar decoration; every other class is a mechanic.
ACTION_COLUMN = re.compile(r"Action$|^VisualActions$")
COSMETIC_ACTION_CLASSES = {
    "ActionPlayEffect",
    "ActionGroup",
    "ActionAnimatorLayer",
    "ActionSetAnimationModifier",
    "ActionRunForcedAnimationOnce",
    "ActionPlayAnimationIfHasTarget",
    "ActionAddHealthBarPart",
    "ActionEnabbleHPBarConditionForDuration",
    "ActionTargetIndicatorAttack",
    "ActionChaosS2BadgeTracker",
}


def action_graph(t: dict, rec: dict) -> dict | None:
    """The actions a row's *Action columns reach (15.535), or None when the table
    has no such column or the row sets none: {roots, class_types, spawns, mechanic}."""
    if "actions" not in t or not isinstance(rec, Row):
        return None
    roots = {c: rec[c] for c in sorted(rec.columns) if ACTION_COLUMN.search(c) and isinstance(rec[c], str) and rec[c]}
    if not roots:
        return None
    acts = t["actions"]
    seen: list[str] = []
    spawns: list[str] = []

    def walk(name: str) -> None:
        if name in seen:
            return
        a = acts.get(name)
        if a is None:
            return
        seen.append(name)
        if a["ClassType"] in ("ActionSpawn", "ActionSpawnToLocation") and isinstance(a["SpawnData"], str):
            spawns.append(f"{a['SpawnType']}:{a['SpawnData']}")
        refs = [v for k, v in a.items() if k != "Name" and isinstance(v, str)]
        refs += [x for lst in acts.arrays.get(name, {}).values() for x in lst if isinstance(x, str)]
        for v in refs:
            if v in acts.records:
                walk(v)

    for v in roots.values():
        walk(v)
    classes = sorted({acts.get(n)["ClassType"] for n in seen if isinstance(acts.get(n)["ClassType"], str)})
    return {
        "roots": roots,
        "class_types": classes,
        "spawns": spawns,
        "mechanic": any(c not in COSMETIC_ACTION_CLASSES for c in classes),
    }


def raw_logic(rec: dict) -> dict:
    out = {}
    for h, v in rec.items():
        if v is None or isinstance(v, dict) or h == "base_ops" or COSMETIC.search(h):
            continue
        if isinstance(rec, Row) and h not in KEEP_IN_RAW and mr.is_cosmetic(h):
            continue
        out[h] = v
    return out


def display_name(internal: str) -> str:
    return DISPLAY_OVERRIDES.get(internal, re.sub(r"(?<=[a-z])(?=[A-Z])", " ", internal))


def norm_buff(t: dict[str, Table], name: str | None) -> dict | None:
    b = t["character_buffs"].get(name)
    if b is None:
        return None
    return {
        "name": name,
        # Encoding of these three is UNVERIFIED: Rage ships 135 and
        # IceWizardSlowDown ships -35, which reads as "positive = absolute
        # percent, negative = delta". Freeze/ZapFreeze -100 = full stop either way.
        "speed_multiplier_raw": b["SpeedMultiplier"],
        "hit_speed_multiplier_raw": b["HitSpeedMultiplier"],
        "spawn_speed_multiplier_raw": b["SpawnSpeedMultiplier"],
        "damage_per_second": b["DamagePerSecond"],
        "hit_frequency_ms": b["HitFrequency"],
        "heal_per_second": b["HealPerSecond"],
        "damage_reduction": b["DamageReduction"],
        "damage_multiplier": b["DamageMultiplier"],
        # Only meaningful for a buff that deals damage; a stun claiming "100% to
        # towers" would be a number nobody shipped.
        "crown_tower_damage_percent": (
            ct_percent(b["CrownTowerDamagePercent"]) if b["DamagePerSecond"] is not None else None
        ),
        "immune_to_anti_magic": flag(b, "ImmuneToAntiMagic"),
        "no_effect_to_crown_towers": flag(b, "NoEffectToCrownTowers"),
        "attract_percentage": b["AttractPercentage"],
        # A damaging or healing buff
        # needs all four to be the card: how a second copy of itself composes
        # (EnableStacking, against calibration status.SAME_BUFF_REAPPLY), what it
        # does to a BUILDING (Earthquake 350) as opposed to a crown tower
        # (CrownTowerDamagePercent, already above), and whose clock its pulses run
        # on (HitTickFromSource: the area effect's, not the victim's).
        # (the 2018 files carry EnableStacking only; the other two read as null there)
        "building_damage_percent": b.get("BuildingDamagePercent"),
        "enable_stacking": flag(b, "EnableStacking"),
        "hit_tick_from_source": (
            flag(b, "HitTickFromSource") if isinstance(b, Row) or "HitTickFromSource" in b else None
        ),
    }


def norm_projectile(t: dict[str, Table], name: str | None, chain: tuple[str, ...] = ()) -> dict | None:
    p = t["projectiles"].get(name)
    if p is None:
        return None
    if name in chain:
        # 15.535 evolutions: BombSkeletonProjectile_2_EV1 inherits (Base) its own
        # SpawnProjectile from BombSkeletonProjectile_EV1 -- a self-chain the game
        # bounds with SpawnChain. Recorded as a stub, never expanded.
        return {"name": name, "recursive_spawn_chain": True}
    if len(chain) > 4:
        raise SystemExit(f"projectile chain too deep at {name}: {chain}")
    out = {
        "name": name,
        "speed": p["Speed"],
        "damage": p["Damage"],
        "crown_tower_damage_percent": ct_percent(p["CrownTowerDamagePercent"]),
        "crown_tower_damage_percent_raw": p["CrownTowerDamagePercent"],
        "homing": flag(p, "Homing"),
        "radius_milli": p["Radius"],
        "radius_y_milli": p["RadiusY"],
        "aoe_to_air": flag(p, "AoeToAir"),
        "aoe_to_ground": flag(p, "AoeToGround"),
        "only_enemies": flag(p, "OnlyEnemies"),
        "pushback_milli": p["Pushback"],
        "pushback_all": flag(p, "PushbackAll"),
        "maximum_targets": p["MaximumTargets"],
        "projectile_radius_milli": p["ProjectileRadius"],
        "projectile_radius_y_milli": p["ProjectileRadiusY"],
        "projectile_range_milli": p["ProjectileRange"],
        # MinDistance (added 2026-09-13 for The Log's airborne phase, LogProjectile
        # 3000): what it MEANS is unsettled (docs/spell-spec.md, Log unsettled); the
        # engine reads it only under calibration spells.SPELL_AS_DEPLOY_LAUNCH_MODEL.
        "min_distance_milli": p["MinDistance"],
        "spawn_character": p["SpawnCharacter"],
        "spawn_character_count": p["SpawnCharacterCount"],
        "spawn_character_deploy_time_ms": p["SpawnCharacterDeployTime"],
        "spawn_character_level_index": p["SpawnCharacterLevelIndex"],
        "spawn_area_effect_object": p["SpawnAreaEffectObject"],
        "target_buff": norm_buff(t, p["TargetBuff"]),
        "buff_time_ms": p["BuffTime"],
        "spawn_projectile": norm_projectile(t, p["SpawnProjectile"], (*chain, name)),
    }
    if isinstance(p, Row):
        out["action_graph"] = action_graph(t, p)
    return out


def norm_aeo(t: dict[str, Table], name: str | None) -> dict | None:
    a = t["area_effect_objects"].get(name)
    if a is None:
        return None
    out = {
        "name": name,
        "life_duration_ms": a["LifeDuration"],
        "radius_milli": a["Radius"],
        "hit_speed_ms": a["HitSpeed"],
        "damage": a["Damage"],
        "crown_tower_damage_percent": ct_percent(a["CrownTowerDamagePercent"]),
        "crown_tower_damage_percent_raw": a["CrownTowerDamagePercent"],
        "no_effect_to_crown_towers": flag(a, "NoEffectToCrownTowers"),
        "buff": norm_buff(t, a["Buff"]),
        "buff_time_ms": a["BuffTime"],
        "only_enemies": flag(a, "OnlyEnemies"),
        "only_own_troops": flag(a, "OnlyOwnTroops"),
        "hits_ground": flag(a, "HitsGround"),
        "hits_air": flag(a, "HitsAir"),
        "ignore_buildings": flag(a, "IgnoreBuildings"),
        "pushback_milli": a["Pushback"],
        "maximum_targets": a["MaximumTargets"],
        "projectile": norm_projectile(t, a["Projectile"]),
        "spawn_character": a["SpawnCharacter"],
        "spawn_interval_ms": a["SpawnInterval"],
        "spawn_max_count": a["SpawnMaxCount"],
        "spawn_initial_delay_ms": a["SpawnInitialDelay"],
        # A PULSING area effect re-applies
        # its buff every HitSpeed ms while a unit stands in it, and CapBuffTimeToAreaEffectTime
        # shortens the last application to the life the area has left (Rage, Earthquake).
        "cap_buff_time_to_area_effect_time": flag(a, "CapBuffTimeToAreaEffectTime"),
        "affects_hidden": flag(a, "AffectsHidden"),
    }
    if isinstance(a, Row):
        out["action_graph"] = action_graph(t, a)
    return out


def unit_record(t: dict[str, Table], name: str) -> tuple[str, dict]:
    for key in ("characters", "buildings"):
        r = t[key].get(name)
        if r is not None:
            return key, r
    raise KeyError(name)


def norm_unit(t: dict[str, Table], name: str, with_raw: bool = False) -> dict:
    table, c = unit_record(t, name)
    defaults: list[str] = []
    proj = norm_projectile(t, c["Projectile"])
    attacks = c["HitSpeed"] is not None and (c["Damage"] is not None or proj is not None)

    if c["Damage"] is not None:
        damage, dmg_src = c["Damage"], f"{table}.{name}.Damage"
        ct_raw = c["CrownTowerDamagePercent"]
        ct = ct_percent(ct_raw)
    elif proj is not None and proj["damage"] is not None:
        damage, dmg_src = proj["damage"], f"projectiles.{proj['name']}.Damage"
        ct_raw = proj["crown_tower_damage_percent_raw"]
        ct = proj["crown_tower_damage_percent"]
    else:
        damage, dmg_src, ct_raw, ct = None, None, c["CrownTowerDamagePercent"], None
    if damage is not None and ct_raw is None:
        defaults.append("crown_tower_damage_percent")

    if not attacks:
        area = None
    elif c["AreaDamageRadius"] is not None:
        area = c["AreaDamageRadius"]
    elif proj is not None and proj["radius_milli"] is not None:
        area = proj["radius_milli"]
    else:
        area, _ = 0, defaults.append("area_damage_radius_milli")

    def dflt(field: str, value, fallback):
        if value is None:
            defaults.append(field)
            return fallback
        return value

    u = {
        "name": name,
        "source_table": table,
        # Rarity of the UNIT row (added 2026-09-13): a unit that is not itself a card
        # -- the Goblin a Goblin Barrel releases -- has no card row to carry it, and
        # its level scaling needs the rarity table (rarities.csv) it belongs to.
        # 15.535: every character row says Common; the card's rarity is the spell's.
        "rarity": c["Rarity"],
        "hitpoints": c["Hitpoints"],
        "damage": damage,
        "damage_source": dmg_src,
        "hit_speed_ms": c["HitSpeed"],
        "load_time_ms": dflt("load_time_ms", c["LoadTime"], 0) if attacks else c["LoadTime"],
        "load_first_hit": flag(c, "LoadFirstHit"),
        # A blank Speed is a thing that does not move (buildings, BrokenCannon).
        "speed": dflt("speed", c["Speed"], 0),
        # THE STOMP COLUMNS.  A Giant does not walk at Speed: it walks faster for
        # StopMovementAfterMS and then stands still for WaitMS, and the native
        # oracle's per-tick displacement is the FASTER figure (calibration.json
        # movement.STOMP_SPEED_RULE / STOMP_PAUSE_SCHEDULE, measured 2026-09-18 --
        # free per-unit speed fit: Giant 52 and Golem 54 while both ship Speed 45).
        # Blank on everything but Giant, RoyalGiant, Golem and IceGolemite in 2018; the
        # 2026 build adds GoblinGiant with the same two columns and the SAME values
        # for the cards both data sets share.
        "stop_movement_after_ms": c["StopMovementAfterMS"],
        "wait_ms": c["WaitMS"],
        "range_milli": c["Range"],
        "minimum_range_milli": c["MinimumRange"],
        "sight_range_milli": c["SightRange"],
        "collision_radius_milli": c["CollisionRadius"],
        # null for buildings: immovable, and a zero mass would divide. (The engine's
        # loader gives a blank Mass the game's own derived value, move16402.rs
        # `loaded_mass`.)
        "mass": c["Mass"],
        "deploy_time_ms": c["DeployTime"],
        "attacks_air": flag(c, "AttacksAir"),
        "attacks_ground": flag(c, "AttacksGround"),
        "target_only_buildings": flag(c, "TargetOnlyBuildings"),
        "flying_height": dflt("flying_height", c["FlyingHeight"], 0),
        "area_damage_radius_milli": area,
        "self_as_aoe_center": flag(c, "SelfAsAoeCenter"),
        "projectile": proj,
        "shield_hitpoints": dflt("shield_hitpoints", c["ShieldHitpoints"], 0),
        "crown_tower_damage_percent": ct,
        "lifetime_ms": c["LifeTime"],
        "ignore_pushback": flag(c, "IgnorePushback"),
        "tile_size_override": c["TileSizeOverride"],
        "death_damage": c["DeathDamage"],
        "death_damage_radius_milli": c["DeathDamageRadius"],
        "death_spawn": None
        if c["DeathSpawnCharacter"] is None
        else {
            "character": c["DeathSpawnCharacter"],
            "count": c["DeathSpawnCount"],
            "radius_milli": c["DeathSpawnRadius"],
            "deploy_time_ms": c["DeathSpawnDeployTime"],
        },
        "death_area_effect": c["DeathAreaEffect"],
        "spawner": None
        if c["SpawnCharacter"] is None
        else {
            "character": c["SpawnCharacter"],
            "number": c["SpawnNumber"],
            "interval_ms": c["SpawnInterval"],
            "start_time_ms": c["SpawnStartTime"],
            "pause_time_ms": c["SpawnPauseTime"],
            "limit": c["SpawnLimit"],
            "radius_milli": c["SpawnRadius"],
        },
        # THE SUMMON RING'S RADIUS FALLBACK (calibration.json formation.LAYOUT): a
        # card with a blank SummonRadius lays its summons on the character's
        # SpawnRadius when that is set, else its CollisionRadius (the Skeleton
        # Warriors' 923-native ring is SpawnRadius 800 scaled, measured live).
        # Carried on every unit regardless of a SpawnCharacter (SkeletonWarrior
        # ships 800 and spawns nothing).
        "spawn_radius_milli": c["SpawnRadius"],
        # SpawnAngleShift: degrees added to the summon ring's base angle; Bat ships
        # 45 (the live Bats' ring is turned 45 degrees). Blank = 0.
        "spawn_angle_shift_deg": c.get("SpawnAngleShift"),
        # ProjectileStartRadius: where a projectile is born -- this far from the
        # attacker's centre toward the target (calibration.json
        # combat.PROJECTILE_LAUNCH, measured on the live tower arrows: 299-300 from
        # the tower's centre on the launch frame). Blank = 0 (born at the centre).
        # Carried on every unit; only the projectile ones read it.
        "projectile_start_radius_milli": c.get("ProjectileStartRadius"),
        # Kamikaze: the unit dies on its own hit (calibration.json
        # combat.KAMIKAZE_DEATH; the live Battle Ram is gone on the frame its one hit
        # lands). KamikazeTime delays the death (SkeletonBalloon 500); blank = 0 =
        # at once. Battle Ram, the Spirits, Wall Breakers, Skeleton Barrel.
        "kamikaze": flag(c, "Kamikaze"),
        "kamikaze_time_ms": c.get("KamikazeTime"),
        # THE UNDERGROUND SPAWN WALK (SpawnPathfindSpeed / SpawnPathfindMorph). A
        # row with SpawnPathfindSpeed is not born at the tap: the recordings show it
        # appearing at its owner's king tower and travelling UNDERGROUND at that speed
        # to the tap. With SpawnPathfindMorph it then MORPHS into the named row on
        # arrival -- the
        # GoblinDrillDig troop becomes the GoblinDrill building, a different hp, a
        # different LifeTime and a spawner the dig row does not have. The engine runs
        # neither, so card.rs REFUSES any card whose summon ships either column
        # (Miner, GoblinDrill); carried here so the loader can see them.
        "spawn_pathfind": None
        if c.get("SpawnPathfindSpeed") is None and c.get("SpawnPathfindMorph") is None
        else {"speed": c.get("SpawnPathfindSpeed"), "morph": c.get("SpawnPathfindMorph")},
        # ChargeRange's unit is NOT established: Prince ships 250, which is not
        # plausible as 0.25 tiles of run-up. Passed through raw on purpose.
        "charge": None
        if c["DamageSpecial"] is None
        else {
            "damage_special": c["DamageSpecial"],
            "charge_range_raw": c["ChargeRange"],
            "charge_speed_multiplier_percent": c["ChargeSpeedMultiplier"],
        },
        "dash": None
        if c["DashDamage"] is None
        else {
            "damage": c["DashDamage"],
            "min_range_milli": c["DashMinRange"],
            "max_range_milli": c["DashMaxRange"],
            "radius_milli": c["DashRadius"],
            "cooldown_ms": c["DashCooldown"],
            "immune_to_damage_time_ms": c["DashImmuneToDamageTime"],
            "pushback_milli": c["DashPushBack"],
        },
        # THE RIVER JUMP (JumpEnabled / JumpHeight / JumpSpeed). Only a JumpEnabled row
        # hops the water (its search prices water at WATER_COST, and the walk replaces
        # the water nodes with a leap at JumpSpeed native units per tick --
        # calibration.json movement.JUMP_WATER_HOP, measured on the live 16.402 hops).
        # MegaKnight / Assassin carry JumpHeight / JumpSpeed WITHOUT JumpEnabled (their
        # dash-jump, a different state) and get no block. 2018 vintage: HogRider only;
        # the 15.535 card data adds Prince, DarkPrince, the Battle Ram's Ram and
        # RoyalHog with the identical 4000 / 160.
        "jump": None
        if not c["JumpEnabled"]
        else {
            "height_raw": c["JumpHeight"],
            "speed": c["JumpSpeed"],
        },
        "hides_when_not_attacking": flag(c, "HidesWhenNotAttacking"),
        "hide_time_ms": c["HideTimeMs"],
        "up_time_ms": c["UpTimeMs"],
        "buff_on_damage": None
        if c["BuffOnDamage"] is None
        else {"buff": norm_buff(t, c["BuffOnDamage"]), "time_ms": c["BuffOnDamageTime"]},
        "attached_character": c["AttachedCharacter"],
        "defaults_applied": defaults,
    }
    if isinstance(c, Row):
        # 15.535: the scripted actions the row reaches (None when it names none).
        u["action_graph"] = action_graph(t, c)
    if with_raw:
        u["raw"] = raw_logic(c)
        if c.get("base_ops"):
            u["base_ops"] = c["base_ops"]
        arr = t[table].arrays.get(name) or {}
        if arr:
            u["level_arrays" if isinstance(t[table], Table) else "list_columns"] = arr
        if isinstance(t[table], OverlayTable):
            u["overlays"] = t[table].overlaid.get(name, [])
    return u


# --- rarities and the ladder --------------------------------------------------------


def rarity_table(t: dict[str, Table]) -> dict:
    out = {}
    for name, r in t["rarities"].records.items():
        arr = t["rarities"].arrays[name]
        plm = arr.get("PowerLevelMultiplier", [r["PowerLevelMultiplier"]])
        n = r["LevelCount"]
        if len(plm) < n - 1:
            raise SystemExit(f"rarities.csv {name}: {len(plm)} multipliers for {n} levels")
        # 15.535 only: the 0-based index of the tournament level in the ladder
        # (unified level 11 for every rarity). Absent from the 2018 file.
        tix = r.get("TournamentLevelIndex")
        if tix is not None and not (0 <= tix < n):
            raise SystemExit(f"rarities.csv {name}: TournamentLevelIndex {tix} outside 0..{n - 1}")
        out[name] = {
            "level_count": n,
            "relative_level": r["RelativeLevel"],
            "tournament_level_index": tix,
            "power_level_multiplier_raw": plm,
            "multiplier_percent_by_level": [100, *plm[: n - 1]],
            "unused_tail": plm[n - 1 :],
        }
    return out


PUBLISHED_HOG_LADDER = [800, 880, 968, 1064, 1168, 1280, 1408, 1544]


def check_hog_ladder(rarities: dict, units: dict, cards: list[dict]) -> tuple[list[str], list[str]]:
    """Return (failures, notes)."""
    fail, notes = [], []
    hog_card = next((c for c in cards if c["name"] == "HogRider"), None)
    if hog_card is None:
        return ["hog ladder: HogRider card missing"], notes
    base = units["HogRider"]["hitpoints"]
    # The card's own block: the card ladder in 2018, the object (Common) ladder from
    # the unified base level in 15.535 -- the same shape either way.
    mult = hog_card["level_scaling"]["multiplier_percent_by_level"]
    got = [base * m // 100 for m in mult[: len(PUBLISHED_HOG_LADDER)]]
    if base == PUBLISHED_HOG_LADDER[0]:
        want = PUBLISHED_HOG_LADDER
    else:
        notes.append(f"HogRider base HP in this data is {base}, not 800: checking ladder SHAPE")
        want = [base * p // PUBLISHED_HOG_LADDER[0] for p in PUBLISHED_HOG_LADDER]
    if got != want:
        fail.append(f"hog ladder: rarity table gives {got}, published {want}")
    inexact = [base * m % 100 for m in mult[: len(PUBLISHED_HOG_LADDER)]]
    if not any(inexact):
        notes.append(
            "every Hog ladder product is an exact integer, so this check CANNOT "
            "distinguish floor/round/ceil -- rounding mode remains unverified"
        )
    return fail, notes


# --- cards --------------------------------------------------------------------------

UNIT_FIELDS_FOR_CARD = [
    "hitpoints",
    "damage",
    "damage_source",
    "hit_speed_ms",
    "load_time_ms",
    "load_first_hit",
    "speed",
    "stop_movement_after_ms",
    "wait_ms",
    "range_milli",
    "minimum_range_milli",
    "sight_range_milli",
    "collision_radius_milli",
    "mass",
    "deploy_time_ms",
    "attacks_air",
    "attacks_ground",
    "target_only_buildings",
    "flying_height",
    "area_damage_radius_milli",
    "self_as_aoe_center",
    "projectile",
    "shield_hitpoints",
    "crown_tower_damage_percent",
    "lifetime_ms",
    "ignore_pushback",
    "death_damage",
    "death_damage_radius_milli",
    "death_spawn",
    "death_area_effect",
    "spawner",
    "spawn_radius_milli",
    "spawn_angle_shift_deg",
    "projectile_start_radius_milli",
    "kamikaze",
    "kamikaze_time_ms",
    "spawn_pathfind",
    "charge",
    "dash",
    "jump",
    "hides_when_not_attacking",
    "hide_time_ms",
    "up_time_ms",
    "buff_on_damage",
    "defaults_applied",
]


def level_scaling(t: Tables, rarities: dict, rarity: str, object_rarity: str | None = None) -> dict:
    """The per-card ladder (module doc, LEVEL SCALING). `rarity` is the CARD's
    (its level range); `object_rarity` the Rarity column of the object that
    carries the stats (the unit row, a spell's damage carrier), when the table has
    one. 2018: the two coincide on every row this schema reads except the Skeleton
    Army's, and the block is the card ladder, unchanged. 15.535: under the shipped
    reading the block is the OBJECT ladder from `base_level` (unified), so a Rare
    card's table runs 1..16 on the Common ladder; under `card_rarity_local_1` it is
    the card ladder from the card's local level 1, the earlier arithmetic."""
    r = rarities[rarity]
    out = {
        "rarity": rarity,
        "level_count": r["level_count"],
        "multiplier_percent_by_level": r["multiplier_percent_by_level"],
        "applies_to": ["hitpoints", "damage"],
        "rounding": "UNVERIFIED -- see calibration.json combat.DAMAGE_ARITHMETIC",
    }
    if t.vintage.is_2018:
        return out
    if t.level_base not in LEVEL_BASE_READINGS:
        raise SystemExit(f"level base reading {t.level_base!r} is not one of {LEVEL_BASE_READINGS}")
    ladder = object_rarity if (t.level_base == LEVEL_BASE_READINGS[0] and object_rarity) else rarity
    lr = rarities[ladder]
    out.update(
        {
            "relative_level": r["relative_level"],
            "reading": t.level_base,
            "ladder_rarity": ladder,
            "base_level": lr["relative_level"] + 1,
            "multiplier_percent_by_level": lr["multiplier_percent_by_level"],
            "rounding": "floor -- MEASURED on the 16.402 live captures' max_hp (tests/levels.rs); "
            "the composition with crown-tower percent and shields is still calibration.json "
            "combat.DAMAGE_ARITHMETIC",
        }
    )
    return out


def object_rarity(t: Tables, table: str, name: str | None) -> str | None:
    """The Rarity column of one row, when that table carries the column (2018
    projectiles / area effects do not: None, and the card's rarity stands in)."""
    rec = t[table].get(name) if name else None
    if rec is None or not t[table].has_column("Rarity"):
        return None
    v = rec["Rarity"] if isinstance(rec, Row) else rec.get("Rarity")
    return v if isinstance(v, str) and v else None


def spawned_characters(t: dict, aeo_name: str | None) -> list[tuple[str, str]]:
    """15.535: the characters an area effect's OnStartingAction graph spawns, in
    order, as (character, path). ActionGroup SubActions are walked in order; an
    AreaEffectType spawn is followed into that area effect; anything else stops."""
    out: list[tuple[str, str]] = []
    seen: set[tuple[str, str]] = set()

    def walk_aeo(name: str, path: str) -> None:
        a = t["area_effect_objects"].get(name)
        if a is None or ("aeo", name) in seen:
            return
        seen.add(("aeo", name))
        act = a["OnStartingAction"]
        if isinstance(act, str):
            walk_action(act, f"{path}area_effect_objects.{name}.OnStartingAction -> ")

    def walk_action(name: str, path: str) -> None:
        act = t["actions"].get(name)
        if act is None or ("action", name) in seen:
            return
        seen.add(("action", name))
        cls = act["ClassType"]
        here = f"{path}actions.{name}"
        if cls == "ActionGroup":
            subs = t["actions"].arrays.get(name, {}).get("SubActions") or (
                [act["SubActions"]] if isinstance(act["SubActions"], str) else []
            )
            for s in subs:
                walk_action(s, f"{here}.SubActions -> ")
        elif cls in ("ActionSpawn", "ActionSpawnToLocation"):
            data, kind = act["SpawnData"], act["SpawnType"]
            if kind == "CharacterType" and isinstance(data, str):
                out.append((data, f"{here}.SpawnData"))
            elif kind == "AreaEffectType" and isinstance(data, str):
                walk_aeo(data, f"{here}.SpawnData -> ")

    if aeo_name:
        walk_aeo(aeo_name, "")
    return out


def resolve_summon(t: dict, key: str, s: dict) -> dict:
    """The unit a card row releases: SummonCharacter when set; else (15.535) the
    SummonCharactersList overlay, else the deploy area effect's spawn graph."""
    name = s["Name"]
    if s["SummonCharacter"] is not None:
        return {
            "character": s["SummonCharacter"],
            "count": s["SummonNumber"] if s["SummonNumber"] is not None else 1,
            "source": f"{key}.{name}.SummonCharacter",
            "others": [],
        }
    lst = t[key].arrays.get(name, {}).get("SummonCharactersList")
    if not lst and isinstance(s.get("SummonCharactersList"), str):
        lst = [s["SummonCharactersList"]]
    if lst:
        return {
            "character": lst[0],
            "count": len(lst),
            "source": f"{key}.{name}.SummonCharactersList (overlay; {len(lst)} entries)",
            "others": [x for x in lst[1:] if x != lst[0]],
        }
    if "actions" in t:
        found = spawned_characters(t, s["AreaEffectObject"])
        if found:
            first = found[0][0]
            return {
                "character": first,
                "count": sum(1 for c, _ in found if c == first),
                "source": f"{key}.{name}.AreaEffectObject -> {found[0][1]}",
                "others": [c for c, _ in found if c != first],
            }
    raise SystemExit(f"{key}.{name}: no SummonCharacter and no resolvable spawn graph")


def summon_card(t, rarities, kind, key, s) -> dict:
    res = resolve_summon(t, key, s)
    u = norm_unit(t, res["character"])
    card = {
        "name": s["Name"],
        "display_name": display_name(s["Name"]),
        "kind": kind,
        "elixir": s["ManaCost"],
        "rarity": s["Rarity"],
        "summon_character": res["character"],
    }
    for f in UNIT_FIELDS_FOR_CARD:
        card[f] = u[f]
    if "action_graph" in u:
        card["action_graph"] = u["action_graph"]
    if s["CustomDeployTime"] is not None:
        card["deploy_time_ms"] = s["CustomDeployTime"]
    card["count"] = res["count"]
    card["summon_radius_milli"] = s["SummonRadius"]
    # The summon LINE (RoyalHogs: SummonWidth 3500 with SummonRadius 1 -- the four
    # hogs spread along x) and the deploy STAGGER (member k of the SummonNumber
    # primaries leaves its deploy state k x SummonDeployDelay ms after the first;
    # the second summon's members (j + 1) x SummonDeployDelaySecond after it):
    # calibration.json formation.LAYOUT / DEPLOY_STAGGER, measured on the live
    # corpus' deploy-end ticks. `.get`: the 2018 file has none of the three
    # columns (null, like a blank).
    card["summon_width_milli"] = s.get("SummonWidth")
    card["summon_deploy_delay_ms"] = s.get("SummonDeployDelay")
    card["summon_deploy_delay_second_ms"] = s.get("SummonDeployDelaySecond")
    card["second_summon"] = (
        None
        if s["SummonCharacterSecond"] is None
        else {"character": s["SummonCharacterSecond"], "count": s["SummonCharacterSecondCount"]}
    )
    if card["second_summon"] is None and res["others"]:
        # The spawn graph released a second kind of unit (TriWizards: an Electro
        # Wizard and an Ice Wizard): carried where the schema carries a second summon.
        second = res["others"][0]
        card["second_summon"] = {"character": second, "count": res["others"].count(second)}
    if not res["source"].endswith(".SummonCharacter"):
        card["summon_resolution"] = {"source": res["source"], "characters": [res["character"], *res["others"]]}
    card["deploy_projectile"] = norm_projectile(t, s["Projectile"])
    # The ladder is the UNIT row's Rarity (15.535: Common on every base card, so a
    # Rare card scales on the Common ladder from unified level 1; module doc).
    card["level_scaling"] = level_scaling(t, rarities, s["Rarity"], u["rarity"])
    return card


def spell_card(t, rarities, s) -> dict:
    proj = norm_projectile(t, s["Projectile"])
    first = norm_projectile(t, s["CustomFirstProjectile"])
    aeo = norm_aeo(t, s["AreaEffectObject"])

    # Damage resolution, in order.  The first object that carries damage is the
    # damage source, and the radius / tower percent / air-ground filter are read
    # from THAT object -- mixing them across hops is how Arrows would end up with
    # the decorative volley's radius and the real volley's damage.
    candidates = []
    if first:
        candidates.append(("projectiles", first))
    if proj:
        candidates.append(("projectiles", proj))
        if proj["spawn_projectile"]:
            candidates.append(("projectiles", proj["spawn_projectile"]))
    if aeo:
        candidates.append(("area_effect_objects", aeo))
    src = next(((tbl, o) for tbl, o in candidates if o["damage"] is not None), None)

    card = {
        "name": s["Name"],
        "display_name": display_name(s["Name"]),
        "kind": "spell",
        "elixir": s["ManaCost"],
        "rarity": s["Rarity"],
    }
    unit_null = {f: None for f in UNIT_FIELDS_FOR_CARD}
    card.update(unit_null)
    card.update(
        {
            "load_first_hit": False,
            "target_only_buildings": False,
            "self_as_aoe_center": False,
            "ignore_pushback": False,
            "hides_when_not_attacking": False,
            "defaults_applied": [],
        }
    )
    if src:
        tbl, o = src
        card["damage"] = o["damage"]
        card["damage_source"] = f"{tbl}.{o['name']}.Damage"
        card["crown_tower_damage_percent"] = o["crown_tower_damage_percent"]
        if tbl == "projectiles":
            card["area_damage_radius_milli"] = (
                o["radius_milli"] if o["radius_milli"] is not None else o["projectile_radius_milli"]
            )
            card["attacks_air"], card["attacks_ground"] = o["aoe_to_air"], o["aoe_to_ground"]
        else:
            card["area_damage_radius_milli"] = o["radius_milli"]
            card["attacks_air"], card["attacks_ground"] = o["hits_air"], o["hits_ground"]
    elif s["InstantDamage"] is not None:
        card["damage"] = s["InstantDamage"]
        card["damage_source"] = f"spells.{s['Name']}.InstantDamage"
        card["area_damage_radius_milli"] = s["Radius"]
    else:
        card["attacks_air"] = card["attacks_ground"] = False
    card["projectile"] = proj
    card["deploy_time_ms"] = s["CustomDeployTime"]
    card["count"] = 1
    card["spell"] = {
        "radius_milli": s["Radius"],
        "first_projectile": first,
        # Arrows ships 15 here with a damage-less "Deco" projectile and a separate
        # damaging CustomFirstProjectile. The 15 is read as VISUAL, not as damage
        # waves (docs/spell-spec.md):
        # the 2018 card was one hit (wiki), and the 2023 data carries waves in a
        # separate ProjectileWaves column; the engine never multiplies damage by it.
        # The damage above comes from the first projectile only.
        "multiple_projectiles": s["MultipleProjectiles"],
        # ProjectileWaves / ProjectileWaveInterval (live Arrows: 3 waves, 200 ms) do NOT
        # exist in the 2018 data; null there means "column absent", and the engine reads
        # null as 1 wave / 0 ms. The 15.535 spells_other.csv carries both (Arrows 3 / 200).
        "projectile_waves": s.get("ProjectileWaves"),
        "projectile_wave_interval_ms": s.get("ProjectileWaveInterval"),
        "area_effect_object": aeo,
        "spell_as_deploy": flag(s, "SpellAsDeploy"),
        "can_place_on_buildings": flag(s, "CanPlaceOnBuildings"),
        "can_deploy_on_enemy_side": flag(s, "CanDeployOnEnemySide"),
        "pushback_milli": s["Pushback"],
        "duration_seconds": s["DurationSeconds"],
        "spawn": None,
    }
    spawn_src = next((o for o in (proj, first) if o and o["spawn_character"]), None)
    if spawn_src:
        card["spell"]["spawn"] = {
            "character": spawn_src["spawn_character"],
            "count": spawn_src["spawn_character_count"],
            "deploy_time_ms": spawn_src["spawn_character_deploy_time_ms"],
            "level_index": spawn_src["spawn_character_level_index"],
            "source": f"projectiles.{spawn_src['name']}",
        }
    elif aeo and aeo["spawn_character"]:
        card["spell"]["spawn"] = {
            "character": aeo["spawn_character"],
            "count": aeo["spawn_max_count"],
            "deploy_time_ms": None,
            "level_index": None,
            "source": f"area_effect_objects.{aeo['name']}",
        }
    # The ladder is the DAMAGE CARRIER's Rarity when its table carries the column
    # (15.535 projectiles / area effects: Common on every base spell -- Fireball 269
    # -> 688 at level 11 on the Common ladder, not 570 on the Rare one); a spell with
    # no damage (Goblin Barrel) reads its projectile's; 2018 tables have no such
    # column and the card's rarity stands in.
    carrier = (
        object_rarity(t, src[0], src[1]["name"])
        if src
        else object_rarity(t, "projectiles", s["Projectile"])
        or object_rarity(t, "area_effect_objects", s["AreaEffectObject"])
    )
    card["level_scaling"] = level_scaling(t, rarities, s["Rarity"], carrier)
    return card


def sha256_of(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def build(t: Tables) -> dict:
    v = t.vintage
    rarities = rarity_table(t)
    cards, not_in_use = [], []
    for key, kind in (
        ("spells_characters", "troop"),
        ("spells_buildings", "building"),
        ("spells_other", "spell"),
    ):
        for name, s in t[key].records.items():
            if s["NotInUse"]:
                not_in_use.append(f"{key}.{name}")
                continue
            if kind == "spell":
                card = spell_card(t, rarities, s)
            else:
                card = summon_card(t, rarities, kind, key, s)
            if not v.is_2018:
                # NotVisible: an event-mode / hidden card (SuperKnight, TriWizards, the
                # Chess recruits...) that is not in the collection. Kept in `cards` -- it
                # is a row the client runs -- and flagged so a deck pool can skip it.
                card["not_visible"] = flag(s, "NotVisible")
            cards.append(card)

    units = {}
    for key in ("characters", "buildings"):
        for name in t[key].records:
            if name in units:
                raise SystemExit(f"unit name {name!r} in both characters and buildings")
            units[name] = norm_unit(t, name, with_raw=True)

    towers = []
    for name in TOWERS:
        u = norm_unit(t, name)
        rec = {
            "name": name,
            "display_name": display_name(name),
            "kind": "building",
            "tower": True,
            "elixir": None,
            "rarity": t["buildings"].get(name)["Rarity"],
        }
        for f in UNIT_FIELDS_FOR_CARD:
            rec[f] = u[f]
        rec["count"] = 1
        # NoDeploySizeW/H: set on exactly the crown towers (and a NOTINUSE king copy)
        # in this data. Carried as TILES -- the unit under which all four arena
        # landmarks are exact; tools/check_data.py "territory landmarks" gates that
        # reading against arena.json, so this line does not get to assert it alone.
        # The engine forbids enemy troops inside the closed rect (arena.rs TROOP
        # TERRITORY; calibration.json arena.TERRITORY_MODEL).
        brow = t["buildings"].get(name)
        w, h = brow["NoDeploySizeW"], brow["NoDeploySizeH"]
        if not (isinstance(w, int) and isinstance(h, int) and w > 0 and h > 0):
            raise SystemExit(
                f"buildings.csv {name}: NoDeploySizeW/H = {w!r}/{h!r}, need two positive ints"
            )
        rec["no_deploy_size_tiles"] = [w, h]
        rec["no_deploy_size_raw"] = {"NoDeploySizeW": w, "NoDeploySizeH": h}
        rec["no_deploy_size_provenance"] = (
            f"buildings.csv {name}.NoDeploySizeW/NoDeploySizeH "
            + (
                "(~2018 data, evidence not spec); "
                if v.is_2018
                else f"({v.key} csv_logic/characters/*.toml overlay [BUILDING.{name}], evidence not spec); "
            )
            + "unit TILES inferred from 4/4 arena landmarks, gated by tools/check_data.py"
        )
        rec["level_scaling"] = level_scaling(t, rarities, rec["rarity"], u["rarity"])
        rec["level_scaling"]["rounding"] += (
            "; TOWER SCALING UNVERIFIED: the building row says rarity Common, but globals.csv "
            "also ships HITPOINT_INCREASE_PERCENT_PER_TOWER_LEVEL=8 and ..._KING_LEVEL=7. "
            "Which regime towers use is not established."
            + (
                ""
                if v.is_2018
                else " LIVE 16.402 (crates/royalesim/tests/fixtures/live_levels.json): the king 3312 "
                "at tower level 6 and 4824 at 11, the princess 2030 and 3052, against these 2400 / "
                "1400 rows -- NOT the Common ladder (x160 / x256): the tower regime is its own."
            )
        )
        towers.append(rec)

    stat_tables = [
        "characters",
        "buildings",
        "projectiles",
        "spells_characters",
        "spells_buildings",
        "spells_other",
        "area_effect_objects",
        "character_buffs",
    ]
    files: dict[str, dict] = {}
    for k in [*v.sources, *(["actions"] if "actions" in t else [])]:
        tb = t[k]
        for p in tb.files:
            rel = p.relative_to(v.raw).as_posix()
            entry = files.setdefault(rel, {"sha256": sha256_of(p)})
            if p == tb.path:
                entry["continuation_rows"] = tb.continuation_rows
    provenance = {
        "source": v.source,
        "vintage": v.vintage,
        "roster_dating": v.roster_dating,
        "files": dict(sorted(files.items())) if not v.is_2018 else files,
        "generated_by": "tools/extract_cards.py",
    }
    if not v.is_2018:
        provenance["vintage_key"] = v.key
        # Columns the 2018 schema reads that this build's files do not carry at all
        # (emitted as null wherever they are read).
        absent = sorted(
            f"{k}.{col}"
            for k, cols in {
                "projectiles": ["SpawnCharacterLevelIndex"],
                "character_buffs": ["ImmuneToAntiMagic"],
                "rarities": ["TournamentLevelIndex"],
            }.items()
            for col in cols
            if not t[k].has_column(col)
        )
        provenance["columns_absent"] = absent
        provenance["excluded_tables"] = EXCLUDED_TABLES
        # calibration.json combat.STAT_BASE_LEVEL: which reading built this file
        # (tools/check_data.py refuses a file built under another than the ledger's).
        provenance["level_base_reading"] = t.level_base
        provenance["overlay_rule"] = (
            "effective row = CSV row + TOML overlay (table-level *.toml, then csv_logic/characters/"
            "*.toml [KIND.Name] sections in file-name order); overlay wins; a TOML value on a "
            "CSV-typed column is coerced to that type and the build fails on a mismatch"
        )
    doc = {
        "version": v.version,
        "vintage_warning": v.warning,
        "provenance": provenance,
        "conventions": {
            "distance_units": "millitiles (1 tile = 1000); engine converts via fixed::milli()",
            "duration_units": "milliseconds",
            "speed_units": "raw Speed column; conversion is calibration.json "
            "time.SPEED_TO_SUBTILES_PER_TICK",
            "booleans": "a blank boolean cell is false (Supercell loader default)"
            + ("" if v.is_2018 else "; a boolean column the table lacks entirely is null"),
            "nulls": "null means the source row has no value and no safe default exists; "
            "a default that WAS applied is listed in each record's defaults_applied",
            "crown_tower_damage_percent": "effective percent; HYPOTHESIS that a negative raw "
            "value is a delta from 100 (raw kept alongside where it exists)",
            "level_scaling": "multiplier_percent_by_level[L-1] is the percent of the level-1 stat "
            "at level L"
            + (
                ""
                if v.is_2018
                else " -- L counted from level_scaling.base_level (unified; 1 on every base card: the "
                "OBJECT's Rarity row is Common), the card's rarity / level_count / relative_level "
                "bound the playable levels; level_scaling.reading names the ledger candidate "
                "(combat.STAT_BASE_LEVEL) the block was built under"
            ),
            "stat_tables_have_continuation_rows": any(t[k].continuation_rows for k in stat_tables),
        },
        "thin_slice": [THIN_SLICE[d] for d in THIN_SLICE],
        "thin_slice_display": THIN_SLICE,
        "rarities": rarities,
        "cards": cards,
        "towers": towers,
        "units": units,
        "projectiles": {n: norm_projectile(t, n) for n in t["projectiles"].records},
        "area_effect_objects": {n: norm_aeo(t, n) for n in t["area_effect_objects"].records},
        "buffs": {n: norm_buff(t, n) for n in t["character_buffs"].records},
        "not_in_use_skipped": not_in_use,
    }
    if v.is_2018:
        # The 2018 file is byte-identical to the single-vintage extractor's output:
        # nothing the vintage split added is emitted for it.
        for r in doc["rarities"].values():
            del r["tournament_level_index"]
    return doc


def render(doc: dict) -> str:
    return json.dumps(doc, indent=1) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--vintage", choices=sorted(VINTAGES), default=DEFAULT_VINTAGE)
    ap.add_argument(
        "--level-base",
        choices=LEVEL_BASE_READINGS,
        default=LEVEL_BASE_READING,
        help="calibration.json combat.STAT_BASE_LEVEL candidate to build the ladders under (15.535 only)",
    )
    ap.add_argument(
        "--out",
        type=Path,
        default=None,
        help="default: data/derived/cards.json for the default vintage, cards-<vintage>.json otherwise",
    )
    args = ap.parse_args()
    v = VINTAGES[args.vintage]

    print("*** " + v.warning)
    t = load_tables(v)
    t.level_base = args.level_base
    for tb in t.values():
        print(
            f"  {tb.key:20s} {len(tb.header):3d} csv columns {len(tb.records):3d} rows "
            f"{tb.continuation_rows:3d} continuation rows"
            + (f" {len(tb.files) - 1:3d} overlay files" if isinstance(tb, OverlayTable) else "")
        )
    doc = build(t)

    fail, notes = check_hog_ladder(doc["rarities"], doc["units"], doc["cards"])
    hog = doc["units"]["HogRider"]
    hog_ls = next(c for c in doc["cards"] if c["name"] == "HogRider")["level_scaling"]
    mult = hog_ls["multiplier_percent_by_level"]
    print(
        f"  Hog Rider ladder (base {hog['hitpoints']}, card {hog_ls['rarity']}, ladder "
        f"{hog_ls.get('ladder_rarity', hog_ls['rarity'])} from unified level "
        f"{hog_ls.get('base_level', '(card local 1)')}): {[hog['hitpoints'] * m // 100 for m in mult[:8]]}"
    )
    for n in notes:
        print("  NOTE " + n)
    if not v.is_2018:
        for name, r in doc["rarities"].items():
            print(
                f"  rarity {name:12s} levels {r['level_count']:2d} relative {r['relative_level']:2d} "
                f"tournament index {r['tournament_level_index']} = local level "
                f"{r['tournament_level_index'] + 1} = unified {r['tournament_level_index'] + 1 + r['relative_level']}"
            )
        print(f"  columns absent from this build: {doc['provenance']['columns_absent']}")
        resolved = [c["name"] for c in doc["cards"] if "summon_resolution" in c]
        print(f"  cards whose unit came through the spawn graph / overlay list: {resolved}")
    if fail:
        print("CARDS GATE FAILED:", file=sys.stderr)
        for f in fail:
            print("   " + f, file=sys.stderr)
        return 1

    out = args.out or default_out(v.key)
    out.parent.mkdir(parents=True, exist_ok=True)
    # newline="\n": derived artifacts must be byte-identical on every OS.
    out.write_text(render(doc), encoding="utf-8", newline="\n")
    print(
        f"cards -> {out.relative_to(ROOT) if out.is_relative_to(ROOT) else out}: "
        f"{len(doc['cards'])} cards, {len(doc['towers'])} towers, "
        f"{len(doc['units'])} units, {len(doc['not_in_use_skipped'])} NotInUse rows skipped "
        f"[{v.version}]"
    )
    missing = [d for d, i in THIN_SLICE.items() if not any(c["name"] == i for c in doc["cards"])]
    print(
        f"  thin slice: {len(THIN_SLICE) - len(missing)}/{len(THIN_SLICE)} present"
        + (f"; MISSING {missing}" if missing else "")
    )

    if args.summary:
        by = {c["name"]: c for c in doc["cards"]}
        for d, i in THIN_SLICE.items():
            c = by.get(i)
            if c is None:
                continue
            print(
                f"  {d:14s} {c['kind']:8s} e{c['elixir']} {c['rarity']:9s} hp={c['hitpoints']} "
                f"dmg={c['damage']} hs={c['hit_speed_ms']} spd={c['speed']} "
                f"rng={c['range_milli']} r={c['collision_radius_milli']} n={c['count']} "
                f"aoe={c['area_damage_radius_milli']} "
                f"ct%={c['crown_tower_damage_percent']} src={c['damage_source']}"
            )
        for tw in doc["towers"]:
            print(
                f"  {tw['name']:14s} hp={tw['hitpoints']} dmg={tw['damage']} "
                f"hs={tw['hit_speed_ms']} "
                f"rng={tw['range_milli']} r={tw['collision_radius_milli']} "
                f"no_deploy={tw['no_deploy_size_tiles']}"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
