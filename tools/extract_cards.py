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
    An area that carries a SpawnCharacter of its own (TriWizards' TriWizardSpawn,
    whose own TriWizard is the card's unit and whose two actions make the Electro
    Wizard's and the Ice Wizard's areas) is also written as `deploy_spawn_area`.

    EXCLUDED FROM `cards`: spells_evolved.csv (109 rows, 68 NotInUse; every row
    summons a distinct *_EV1 character from characters_evo.toml) and
    spells_hero_form.csv (113 rows: each *_hero row is the base card's row with
    the _hero suffix -- 69 of the 77 with a base card are identical to it column
    for column, the other 8 differ in icons / EvolvedSpells / SummonRadius -- and
    the hero characters are [EXT.*] sections extending the base character in
    characters/hero_form/*.toml).  Both are recorded under `excluded_tables`.  The
    *_EV1 units ARE in `units` (they are character rows); nothing releases them.
    The evolved rows the engine runs (EVOLUTIONS: Skeletons_EV1, Cannon_EV1,
    Musketeer_EV1) are written under the top-level list `evolutions`, each a card
    record with its base card (`form_of`) and its mechanic's block
    (`evolution_records`); 15.535 only.

    HERO FORMS (15.535 only): the forms in HERO_FORMS (Hero Musketeer, Hero Ice
    Golem) are written to the top-level `hero_forms` list, each a card record read
    from its two characters/hero_form files on a load of its own, plus `form_of`
    (its base card), `ability` (its button) and `tables` (the unit and area rows it
    names). Nothing else in the file changes.

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
        # spells_evolved.csv joins for the three evolved rows it extracts (`EVOLUTIONS`); its rows never enter `cards`.
        sources={**SOURCES_2018, "spells_evolved": "spells_evolved.csv"},
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
    # A damage type an ActionDealDamage names (BaseDamageType): the Ronin's reflect says whether its
    # damage scales with level. Read by `parry`; its own table, built from overlays only. The same
    # sections are also kept by name as the files write them (`Tables.damage_types`, `attack_select`).
    "DAMAGE_TYPE": "damage_types",
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
        "buildings_evo.toml); 68 of 109 rows are NotInUse. Not a base card; not in `cards`. "
        "The rows in EVOLUTIONS are written under `evolutions`, each with the base card it evolves (`form_of`)."
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
    "DeathSpawnDeployTime", "DeathSpawnPushback", "DeathAreaEffect", "SpawnCharacter", "SpawnNumber", "SpawnInterval",
    "SpawnStartTime", "SpawnPauseTime", "SpawnLimit", "SpawnRadius", "DamageSpecial", "ChargeRange",
    "ChargeSpeedMultiplier", "DashDamage", "DashMinRange", "DashMaxRange", "DashRadius",
    "DashCooldown", "DashImmuneToDamageTime", "DashPushBack", "DashConstantTime", "DashLandingTime",
    "JumpEnabled", "JumpHeight",
    "JumpSpeed", "HidesWhenNotAttacking", "HideTimeMs", "UpTimeMs", "BuffOnDamage",
    "BuffOnDamageTime", "AttachedCharacter", "NoDeploySizeW", "NoDeploySizeH",
    "ProjectileStartRadius", "ProjectileYOffset", "Kamikaze", "KamikazeTime",
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
    "CanPlaceOnBuildings", "CanDeployOnEnemySide", "TouchdownLimitedDeploy", "DurationSeconds",
    "CheckCollisions", "ProjectileStartExtraRadius", "RandomDelay", "SpawnCount", "Scatter",
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
        # 15.535: the csv_logic/characters/*.toml [VARIABLE.*] and [DAMAGE_TYPE.*] sections, by name, as the
        # files write them (read by the named-block builder `attack_select`; the [DAMAGE_TYPE.*] sections also
        # fill the `damage_types` overlay table, which `parry` reads). Empty for 2018.
        self.variables: dict[str, dict] = {}
        self.damage_types: dict[str, dict] = {}
        # 15.535: the per-character [SHAPE.*] sections, by name (the Vines' Vines_AOE_Shape), and
        # game_object_filters.toml, by filter name (the Vines' enemy_troops_for_vines, the Void's
        # ForcedCharacterTargets). Read by the striking-area builder `strike_area_block` only. Empty
        # for 2018.
        self.shapes: dict[str, dict] = {}
        self.filters: dict[str, dict] = {}
        # 15.535, the hero pass only (`load_tables(hero=...)`): the [ABILITY.*] sections of the hero files, by
        # name, and each base card's EvolvedSpells link to its forms. Empty on every other load.
        self.abilities: dict[str, dict] = {}
        self.hero_links: dict[str, list] = {}
        # 15.535, the hero pass only: the hero files' [TARGET_RESOLVER.*] sections, by name (the Hero Balloon's throw
        # picks its target through one). Empty on every other load.
        self.resolvers: dict[str, dict] = {}


def load_tables(vintage: str | Vintage | None = None, hero: dict | None = None) -> Tables:
    """The vintage's tables. `hero` (15.535 only): the hero forms whose characters/hero_form files are laid
    over these tables too (`overlay_hero_files`). Only the hero pass asks for them, on a load of its own, so
    nothing they add reaches a base record."""
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
    # [DAMAGE_TYPE.*] sections of the per-character files (SECTION_TABLE). No output lists this
    # table's files, and nothing but `parry` reads it, so every other row is unchanged by it.
    t["damage_types"] = OverlayTable("damage_types", None)
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
                # A DAMAGE_TYPE's class is read by name, as the file writes it, by a named-block builder
                # (`attack_select`), besides the overlay table SECTION_TABLE routes it to (`parry`).
                if section == "DAMAGE_TYPE" and isinstance(body, dict):
                    t.damage_types.update({n: f for n, f in body.items() if isinstance(f, dict)})
                if key is None:
                    if section not in SKIP_SECTIONS:
                        raise SystemExit(f"{p.name}: unknown section [{section}]")
                    # A VARIABLE's DefaultValue is read by name by a named-block builder (`attack_select`),
                    # and a SHAPE by `strike_area_block`; every other skipped section is dropped.
                    if section == "VARIABLE" and isinstance(body, dict):
                        t.variables.update({n: f for n, f in body.items() if isinstance(f, dict)})
                    if section == "SHAPE" and isinstance(body, dict):
                        t.shapes.update({n: f for n, f in body.items() if isinstance(f, dict)})
                    # A champion's [ABILITY.*] is read by name by `champion_dash_chain` (the hero forms' are the hero
                    # pass's, `overlay_hero_files`); nothing else reads it.
                    if section == "ABILITY" and isinstance(body, dict):
                        t.abilities.update({n: f for n, f in body.items() if isinstance(f, dict)})
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

    # The target filters an action names (TargetFilter, HitFilter), by name: read by `strike_area_block`.
    filters = v.raw / "game_object_filters.toml"
    if filters.is_file():
        t.filters.update({n: f for n, f in tomllib.load(filters.open("rb")).items() if isinstance(f, dict)})
    if hero:
        overlay_hero_files(t, v, hero)

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
    # The Skeleton Barrel's balloon pops. Measured on client 15.535.29: the barrel's step stays 90
    # through both pops (66% and 33%), and a ground-only Knight under it never targets it after one.
    # The pop only drops the balloon art (DropBalloonAtHpList, TransitionTime).
    "ActionSkeletonBarrelPopBalloon",
}


# THE INLINE WALK (15.535, area rows only; `norm_aeo`). An area's *Action column may be an inline table
# rather than an action's name (the Void's DarkMagicAOE: its OnStartingAction is an ActionGroup whose
# SubActions are inline tables, one an ActionLaserBall whose OnDetectedUnitActionList spawns three inline
# buffs), and a named action may hold inline sub-actions (the Goblin Curse circle's GoblinCurseCreateBuffs).
# `action_graph` then follows those tables too: an inline root is named "<Row>.<Column>", an inline table
# carrying a ClassType is walked like a named action, and an inline SpawnData table adds
# "<SpawnType>:<its Name>" to `spawns`. Area rows only: a unit row whose graph would change this way (the
# Berserker's inline OnStartingAction) keeps the reading it loads under. WALK_INLINE is module state so a
# test can turn the walk off (the plant `inline_roots_skipped`, tests/test_action_graph_inline.py).
WALK_INLINE = True


def action_graph(t: dict, rec: dict, sequence: list | None = None, inline_row: str | None = None) -> dict | None:
    """The actions a row's *Action columns reach (15.535), or None when the table
    has no such column or the row sets none: {roots, class_types, spawns, mechanic}.

    `sequence` is a unit row's AttackSequenceList (a list of inline tables): an entry's
    DoAttackAction is a root too (`AttackSequenceList[k].DoAttackAction`), so the attack an
    entry runs shows among the classes (the Three Musketeers' bayonet: ActionRunOnInstigator,
    ActionDealDamage). No loaded card's row carries one.

    `inline_row` (an area row's name, under WALK_INLINE): inline root tables and inline
    sub-actions are walked too (the module note above), an inline root named after the row."""
    if "actions" not in t or not isinstance(rec, Row):
        return None
    inline = inline_row is not None
    roots = {c: rec[c] for c in sorted(rec.columns) if ACTION_COLUMN.search(c) and isinstance(rec[c], str) and rec[c]}
    inline_roots: dict[str, dict] = {}
    if inline:
        row = inline_row
        for c in sorted(rec.columns):
            v = rec[c]
            if ACTION_COLUMN.search(c) and isinstance(v, dict) and isinstance(v.get("ClassType"), str):
                roots[c] = f"{row}.{c}"
                inline_roots[c] = v
        roots = dict(sorted(roots.items()))
    for k, entry in enumerate(sequence or []):
        if isinstance(entry, dict) and isinstance(entry.get("DoAttackAction"), str) and entry["DoAttackAction"]:
            roots[f"AttackSequenceList[{k}].DoAttackAction"] = entry["DoAttackAction"]
    if not roots:
        return None
    acts = t["actions"]
    seen: list[str] = []
    spawns: list[str] = []
    inline_classes: set[str] = set()
    # An inline table reached twice (a list column's first element is also the row's own value) is
    # walked once, by identity.
    seen_inline: set[int] = set()

    def spawn_of(a) -> None:
        if a.get("ClassType") in ("ActionSpawn", "ActionSpawnToLocation"):
            data = a.get("SpawnData")
            if isinstance(data, str):
                spawns.append(f"{a.get('SpawnType')}:{data}")
            elif inline and isinstance(data, dict) and isinstance(data.get("Name"), str):
                spawns.append(f"{a.get('SpawnType')}:{data['Name']}")

    def follow(v) -> None:
        # A value inside an action: an action's name, an inline action, or a list of either.
        if isinstance(v, str):
            if v in acts.records:
                walk(v)
        elif inline and isinstance(v, dict) and isinstance(v.get("ClassType"), str):
            walk_inline(v)
        elif inline and isinstance(v, list):
            for x in v:
                follow(x)

    def walk_inline(a: dict) -> None:
        if id(a) in seen_inline:
            return
        seen_inline.add(id(a))
        inline_classes.add(a["ClassType"])
        spawn_of(a)
        for k, v in a.items():
            if k not in ("Name", "SpawnData"):
                follow(v)

    def walk(name: str) -> None:
        if name in seen:
            return
        a = acts.get(name)
        if a is None:
            return
        seen.append(name)
        if a["ClassType"] in ("ActionSpawn", "ActionSpawnToLocation") and isinstance(a["SpawnData"], str):
            spawns.append(f"{a['SpawnType']}:{a['SpawnData']}")
        elif inline:
            spawn_of(a)
        refs = [v for k, v in a.items() if k != "Name" and isinstance(v, str)]
        refs += [x for lst in acts.arrays.get(name, {}).values() for x in lst if isinstance(x, str)]
        for v in refs:
            if v in acts.records:
                walk(v)
        if inline:
            for k, v in a.items():
                if k not in ("Name", "SpawnData") and isinstance(v, dict):
                    follow(v)
            for lst in acts.arrays.get(name, {}).values():
                for x in lst:
                    if isinstance(x, dict):
                        follow(x)

    for c, v in roots.items():
        if c in inline_roots:
            walk_inline(inline_roots[c])
        else:
            walk(v)
    named = {acts.get(n)["ClassType"] for n in seen if isinstance(acts.get(n)["ClassType"], str)}
    classes = sorted(named | inline_classes)
    return {
        "roots": roots,
        "class_types": classes,
        "spawns": spawns,
        "mechanic": any(c not in COSMETIC_ACTION_CLASSES for c in classes),
    }


# THE ACTION SCHEDULE (15.535 only). `action_graph` above SUMMARISES what a row's actions reach
# (class names, de-duplicated, string references only), so the loader can refuse a mechanic it does
# not run. The schedule is the other reading, for an area whose action the loader DOES run: what the
# area's OnStartingAction / OnHitAction runs, in order, with each entry's delay and parameters. It is a
# separate walker on purpose: changing what `action_graph` calls a mechanic would flip cards that load
# today.
#
# An ActionGroup's SubActions keep their REPEATS (Graveyard names one action twelve times) and each one's
# SubActionsDelay. An inline `[[ACTION.X.SubActions]]` table (the Goblin Curse's two buffs) arrives as a
# dict in `acts.arrays[X]["SubActions"]` and is read like a named action. A lone action is a one-entry
# schedule at delay 0. Every entry carries the parameters this reader understands, and anything else is
# kept under `unread`, so the loader refuses the area rather than run a plainer one (the global
# Lightning's ActionDelay is one such column).
X_EXPR = re.compile(
    r"^\s*x\s*\+\s*\(\s*(-?\d+)\s*\*\s*select\("
    r"\s*x\s*>\s*\(\s*map_width\s*/\s*2\s*\)\s*,\s*-1\s*,\s*1\s*\)\s*\)\s*$"
)
Y_EXPR = re.compile(r"^\s*y\s*-\s*\(\s*(-?\d+)\s*\*\s*team_y_direction\(\s*team_index\s*\)\s*\)\s*$")
# Action columns that only name the row or address effects, animation or display.
COSMETIC_ACTION_COLUMNS = {
    "Name",
    "ClassType",
    "Base",
    "IgnoreEffects",
    "InheritPrestigeFromParent",
    "Effect",
    "EffectFlags",
    "SpawnDeployBaseAnim",
    "StatsTags",
}


def action_schedule(t: dict, root) -> dict | None:
    """What an area's OnStartingAction / OnHitAction RUNS, in order (15.535): {root, entries}. An
    ActionGroup's SubActions with repeats kept and each one's SubActionsDelay, inline tables read like
    named actions; a lone action is a one-entry schedule at delay 0. None when the root names no
    action."""
    if "actions" not in t:
        return None
    acts = t["actions"]
    a = acts.get(root) if isinstance(root, str) else None
    if a is None:
        return None
    if a["ClassType"] == "ActionGroup":
        arr = acts.arrays.get(root, {})
        subs = arr.get("SubActions") or ([a.get("SubActions")] if a.get("SubActions") is not None else [])
        delays = arr.get("SubActionsDelay") or (
            [a.get("SubActionsDelay")] if a.get("SubActionsDelay") is not None else [0] * len(subs)
        )
        if len(delays) != len(subs):
            raise SystemExit(f"actions.{root}: {len(subs)} SubActions, {len(delays)} SubActionsDelay")
    else:
        subs, delays = [root], [0]
    entries = []
    for sub, delay in zip(subs, delays, strict=True):
        rec = acts.get(sub) if isinstance(sub, str) else sub
        entries.append(schedule_entry(t, rec, sub if isinstance(sub, str) else None, delay))
    return {"root": root, "entries": entries}


def schedule_entry(t: dict, rec, name: str | None, delay) -> dict:
    """One entry of `action_schedule`: its delay, its action's name (None for an inline table) and
    class; a cosmetic class is marked `cosmetic`; an ActionSpawn carries what it spawns, how, and
    where; every other class, and every column this reader does not understand, lands in `unread`."""
    cls = rec.get("ClassType") if isinstance(rec, dict) else None
    e: dict = {"delay_ms": delay, "action": name, "class": cls}
    if cls in COSMETIC_ACTION_CLASSES and cls != "ActionGroup":
        e["cosmetic"] = True
        return e
    if cls in ("ActionSpawn", "ActionSpawnToLocation"):
        e.update(
            {
                "spawn_type": rec.get("SpawnType"),
                "spawn": rec.get("SpawnData"),
                "use_deploy": bool(rec.get("UseDeploy")),
                "deploy_time_ms": rec.get("DeployTime"),
                "spawn_time_ms": rec.get("SpawnTime"),
            }
        )
        for axis, rx, col in (("x", X_EXPR, "XPositionExpression"), ("y", Y_EXPR, "YPositionExpression")):
            text = rec.get(col)
            if text is None:
                continue
            m = rx.match(text) if isinstance(text, str) else None
            if m:
                form = "nearer_wall_mirror" if axis == "x" else "team_y_direction"
                e[axis] = {"form": form, "offset_milli": int(m.group(1))}
            else:
                e.setdefault("unread", []).append(f"{col} {text!r}")
        if rec.get("RelativeX") is not None or rec.get("RelativeY") is not None:
            e["relative"] = {"x": rec.get("RelativeX") or 0, "y": rec.get("RelativeY") or 0}
        if e["spawn_type"] == "BuffType":
            e["buff"] = norm_buff(t, e["spawn"])
        known = {
            "SpawnType",
            "SpawnData",
            "UseDeploy",
            "DeployTime",
            "SpawnTime",
            "XPositionExpression",
            "YPositionExpression",
            "RelativeX",
            "RelativeY",
        } | COSMETIC_ACTION_COLUMNS
        extra = sorted(k for k, v in rec.items() if v is not None and k not in known)
        if extra:
            e.setdefault("unread", []).append("columns " + ", ".join(extra))
        return e
    e["unread"] = [f"class {cls}"]
    return e


# THE STRIKING AREAS WHOSE STRIKES ARE AN ACTION (15.535, area rows only; `norm_aeo`). Two exact shapes, each
# read key by key: an action that sets a key its builder does not know gives no block, and the loader then
# refuses the row by its action graph as before (fail closed). The cosmetic keys (effects, stat labels) are
# allowed anywhere.
#   ranked_catches (the Vines' Vines_AeO): OnStartingAction is a group of one selector
#     (ActionRunActionListOnObjectsInShapeWithPrio) after a delay; the selector runs one catch action at each
#     of its Delays, each on the highest-ranked target of its filter in its shape not caught yet; a catch is a
#     group of an ActionAirToGround and an ActionSelect of buff spawns (one per target size).
#   laser_ball (the Void's DarkMagicAOE): OnStartingAction is an inline group whose one live entry is an
#     ActionLaserBall after a delay: it strikes every target of its filter within DetectionRadius, first after
#     FirstHitDelay and then every HitFrequency, and hangs the buff of the tier its count falls in
#     (MaxUnitPerActionList, one limit fewer than the tiers).
STRIKE_COSMETIC_KEYS = frozenset({"MainEffectList", "StatsTags", "Effect", "HitEffect", "ScaledEffect"})
# A target filter's flags (game_object_filters.toml), by the name the block writes. Any other key gives no
# block; FilterDescriptionTID is a label.
FILTER_FLAGS = {
    "MatchTeamEnemy": "match_team_enemy",
    "MatchTeamOwn": "match_team_own",
    "MatchTypeCharacters": "match_type_characters",
    "FilterBuildings": "filter_buildings",
    "FilterSummoner": "filter_summoner",
    "FilterPrincessTowers": "filter_princess_towers",
    "FilterUnderground": "filter_underground",
    "FilterHidden": "filter_hidden",
    "FilterInvisible": "filter_invisible",
    "FilterFlying": "filter_flying",
    "FilterCloning": "filter_cloning",
    "FilterDashImmune": "filter_dash_immune",
    "FilterIfNoHitpointComponent": "filter_if_no_hitpoint_component",
}
FILTER_LABELS = frozenset({"FilterDescriptionTID"})
# The columns an inline laser-ball tier buff may set (`laser_ball_block`).
LASER_BALL_BUFF_KEYS = frozenset(
    {"Name", "Rarity", "DamagePerSecond", "CrownTowerDamagePerHit", "HitFrequency", "AddAsIndividualBuff"}
)
# The keys each action of the two shapes sets, exactly (less the cosmetic ones).
GROUP_KEYS = frozenset({"ClassType", "SubActions", "SubActionsDelay"})
SELECTOR_KEYS = frozenset(
    {"ClassType", "OncePerTarget", "TargetSelectionMode", "TargetFilter", "Actions", "Delays", "Shape"}
)
AIR_TO_GROUND_KEYS = frozenset(
    {"ClassType", "TransitionDuration", "TotalDuration", "AbortIfInstigatorDies", "Singleton", "AllowIsGroundTagOnIdle"}
)
SPAWN_KEYS = frozenset({"ClassType", "SpawnType", "SpawnTime", "SpawnData"})
LASER_BALL_KEYS = frozenset(
    {"ClassType", "DetectionRadius", "FirstHitDelay", "HitFrequency", "MaxUnitPerActionList", "HitFilter"}
    | {"OnDetectedUnitActionList"}
)


def _live_keys(a: dict) -> set[str]:
    """The keys an action sets, less the cosmetic ones."""
    return {k for k, v in a.items() if v is not None} - STRIKE_COSMETIC_KEYS


def _list_col(acts, name: str, col: str) -> list:
    """A named action's list column, whole (a one-element list reads as its element)."""
    arr = acts.arrays.get(name, {}).get(col)
    if arr is not None:
        return list(arr)
    v = acts.get(name)[col]
    return [] if v is None else [v]


def _cosmetic_table(v) -> bool:
    """An inline action that only plays an effect."""
    return isinstance(v, dict) and v.get("ClassType") == "ActionPlayEffect" and _live_keys(v) <= {"ClassType", "Effect"}


def filter_block(t: dict, name) -> dict | None:
    """A target filter (game_object_filters.toml) as {name, <flags>, tags}, or None for a name the file
    lacks or a key this reader does not know."""
    f = t.filters.get(name) if isinstance(name, str) else None
    if f is None:
        return None
    out: dict = {"name": name}
    for k, v in f.items():
        if k in FILTER_LABELS:
            continue
        if k == "FilterTags" and isinstance(v, str):
            out["tags"] = [s.strip() for s in v.split(",") if s.strip()]
        elif k in FILTER_FLAGS and isinstance(v, bool):
            out[FILTER_FLAGS[k]] = v
        else:
            return None
    return out


def _other_roots_cosmetic(t: dict, a: dict, root_col: str) -> bool:
    """Every *Action column of area row `a` but `root_col` is blank or only plays an effect."""
    acts = t["actions"]
    for c in a.columns:
        if c == root_col or not ACTION_COLUMN.search(c):
            continue
        v = a[c]
        if v is None or _cosmetic_table(v):
            continue
        if isinstance(v, str) and _cosmetic_action(acts, v):
            continue
        return False
    return True


def ranked_catches_block(t: dict, a: dict) -> dict | None:
    """The Vines' block (the module note above), or None."""
    acts = t["actions"]
    root = a["OnStartingAction"]
    g = acts.get(root) if isinstance(root, str) else None
    if g is None or g["ClassType"] != "ActionGroup" or _live_keys(g) != GROUP_KEYS:
        return None
    subs, delays = _list_col(acts, root, "SubActions"), _list_col(acts, root, "SubActionsDelay")
    if len(subs) != 1 or len(delays) != 1 or not isinstance(subs[0], str) or not isinstance(delays[0], int):
        return None
    sel_name, start = subs[0], delays[0]
    sel = acts.get(sel_name)
    if sel is None or sel["ClassType"] != "ActionRunActionListOnObjectsInShapeWithPrio":
        return None
    if _live_keys(sel) != SELECTOR_KEYS:
        return None
    catches, offsets = _list_col(acts, sel_name, "Actions"), _list_col(acts, sel_name, "Delays")
    if not catches or len(catches) != len(offsets) or len(set(catches)) != 1:
        return None
    if not all(isinstance(d, int) for d in offsets):
        return None
    catch = acts.get(catches[0])
    if catch is None or catch["ClassType"] != "ActionGroup" or _live_keys(catch) != GROUP_KEYS:
        return None
    parts, part_delays = _list_col(acts, catches[0], "SubActions"), _list_col(acts, catches[0], "SubActionsDelay")
    if len(parts) != 2 or len(part_delays) != 2 or any(d != 0 for d in part_delays):
        return None
    by_class = {acts.get(p)["ClassType"]: p for p in parts if isinstance(p, str) and acts.get(p) is not None}
    if set(by_class) != {"ActionAirToGround", "ActionSelect"}:
        return None
    ag = acts.get(by_class["ActionAirToGround"])
    if _live_keys(ag) != AIR_TO_GROUND_KEYS:
        return None
    pick_name = by_class["ActionSelect"]
    if _live_keys(acts.get(pick_name)) != {"ClassType", "SubActions", "PerActionConditions"}:
        return None
    options, option_time = [], []
    for o in _list_col(acts, pick_name, "SubActions"):
        s = acts.get(o) if isinstance(o, str) else None
        if s is None or s["ClassType"] != "ActionSpawn" or _live_keys(s) != SPAWN_KEYS:
            return None
        if s["SpawnType"] != "BuffType" or not isinstance(s["SpawnData"], str):
            return None
        b = norm_buff(t, s["SpawnData"])
        if b is None:
            return None
        options.append(b)
        option_time.append(s["SpawnTime"])
    shape = t.shapes.get(sel["Shape"]) if isinstance(sel["Shape"], str) else None
    if shape is None or shape.get("ClassType") != "Circle" or _live_keys(shape) != {"ClassType", "Radius"}:
        return None
    filt = filter_block(t, sel["TargetFilter"])
    if filt is None or not options or not _other_roots_cosmetic(t, a, "OnStartingAction"):
        return None
    return {
        "kind": "ranked_catches",
        "start_delay_ms": start,
        "catch_offsets_ms": offsets,
        "once_per_target": bool(sel["OncePerTarget"]),
        "selection_mode": sel["TargetSelectionMode"],
        "radius_milli": shape["Radius"],
        "filter": filt,
        "air_to_ground": {
            "transition_ms": ag["TransitionDuration"],
            "total_ms": ag["TotalDuration"],
            "abort_if_instigator_dies": bool(ag["AbortIfInstigatorDies"]),
            "singleton": bool(ag["Singleton"]),
            "allow_is_ground_tag_on_idle": bool(ag["AllowIsGroundTagOnIdle"]),
        },
        "options": options,
        "option_time_ms": option_time,
        # ActionSelect PerActionConditions: which option a target takes by its row or its radius. Carried and
        # read by nothing; the loader takes the options only when they are one buff (`strike_area_shape`).
        "conditions": _list_col(acts, pick_name, "PerActionConditions"),
    }


def laser_ball_block(t: dict, a: dict) -> dict | None:
    """The Void's block (the module note above), or None."""
    root = a["OnStartingAction"]
    if not isinstance(root, dict) or root.get("ClassType") != "ActionGroup" or _live_keys(root) != GROUP_KEYS:
        return None
    subs, delays = root["SubActions"], root["SubActionsDelay"]
    if not isinstance(subs, list) or not isinstance(delays, list) or len(subs) != len(delays):
        return None
    live = [(d, s) for s, d in zip(subs, delays, strict=True) if not _cosmetic_table(s)]
    if len(live) != 1 or not isinstance(live[0][0], int):
        return None
    start, lb = live[0]
    if not isinstance(lb, dict) or lb.get("ClassType") != "ActionLaserBall":
        return None
    if _live_keys(lb) != LASER_BALL_KEYS:
        return None
    limits, lists = lb["MaxUnitPerActionList"], lb["OnDetectedUnitActionList"]
    if not isinstance(limits, list) or not all(isinstance(n, int) and not isinstance(n, bool) for n in limits):
        return None
    if not isinstance(lists, list) or len(lists) != len(limits) + 1:
        return None
    tiers = []
    for e in lists:
        if not isinstance(e, dict) or e.get("ClassType") != "ActionSpawn" or e.get("SpawnType") != "BuffType":
            return None
        if _live_keys(e) - {"NextAction"} != {"ClassType", "SpawnType", "SpawnTime", "SpawnData"}:
            return None
        if e.get("NextAction") is not None and not _cosmetic_table(e["NextAction"]):
            return None
        data = e["SpawnData"]
        if not isinstance(data, dict) or _live_keys(data) - LASER_BALL_BUFF_KEYS:
            return None
        tiers.append(
            {
                "buff": norm_inline_buff(t, data),
                "time_ms": e["SpawnTime"],
                "add_as_individual_buff": bool(data.get("AddAsIndividualBuff")),
            }
        )
    filt = filter_block(t, lb["HitFilter"])
    if filt is None or not _other_roots_cosmetic(t, a, "OnStartingAction"):
        return None
    return {
        "kind": "laser_ball",
        "start_delay_ms": start,
        "first_hit_delay_ms": lb["FirstHitDelay"],
        "hit_frequency_ms": lb["HitFrequency"],
        "detection_radius_milli": lb["DetectionRadius"],
        "max_units_per_list": limits,
        "filter": filt,
        "tiers": tiers,
    }


def strike_area_block(t: dict, a: dict) -> dict | None:
    """An area row's striking-area block (15.535): the Vines' ranked catches or the Void's laser ball, or None."""
    if "actions" not in t or not isinstance(a, Row):
        return None
    return ranked_catches_block(t, a) or laser_ball_block(t, a)


def clone_action_block(t: dict, a: dict) -> dict | None:
    """THE CLONE'S ACTION (15.535, an area row that sets Clone; `norm_aeo`): OnHitAction is one ActionClone
    whose OnClonedAction is one ActionSpawn, read key by key. None for any other shape, and the loader then
    refuses the row by its action graph as before."""
    acts = t.get("actions")
    root = a["OnHitAction"]
    ac = acts.get(root) if acts is not None and isinstance(root, str) else None
    if ac is None or ac["ClassType"] != "ActionClone":
        return None
    if _live_keys(ac) - {"SpawnDeployBaseAnim"} != {"ClassType", "CardDataForStats", "OnClonedAction"}:
        return None
    on = acts.get(ac["OnClonedAction"]) if isinstance(ac["OnClonedAction"], str) else None
    if on is None or on["ClassType"] != "ActionSpawn" or _live_keys(on) != SPAWN_KEYS:
        return None
    if not isinstance(on["SpawnData"], str):
        return None
    return {
        "card_data_for_stats": ac["CardDataForStats"],
        "on_cloned": {
            "spawn_type": on["SpawnType"],
            "spawn": on["SpawnData"],
            "buff": norm_buff(t, on["SpawnData"]) if on["SpawnType"] == "BuffType" else None,
            "spawn_time_ms": on["SpawnTime"],
        },
    }


def life_state_spawner(t: dict, rec: dict) -> dict | None:
    """THE GOBLIN HUT'S CONTROLLER as a named block (15.535): the one root action of class
    ActionGoblinHutLifeState the row names, with the parameters the loader reads. None for
    every other row. Fail-closed: two such roots, or none, give None, and the loader then
    refuses the graph as before."""
    g = action_graph(t, rec)
    if not g:
        return None
    acts = t["actions"]
    found = [acts.get(v) for v in g["roots"].values()]
    found = [a for a in found if a is not None and a["ClassType"] == "ActionGoblinHutLifeState"]
    if len(found) != 1:
        return None
    a = found[0]
    return {
        "action_delay_ms": a["ActionDelay"],
        "spawn_interval_ms": a["SpawnInterval"],
        "character": a["SpawnData"],
        "number": a["SpawnNumber"],
        "offset_milli": a["SpawnOffset"],
        "offset_angle_deg": a["SingleDeployOffsetAngle"],
        "object_filter": a["ObjectFilter"],
    }


# THE INTERVAL SPAWNER'S TWO ROWS (15.535: the Furnace), by the keys each may set. An
# ActionInterval root that sets any other key, or runs anything but one ActionSpawnToLocation
# of a character whose only hook is a cosmetic effect, gives no block, and the loader then
# refuses the graph as before (fail closed). StatsTags is the card screen's label.
INTERVAL_KEYS = {
    "ClassType",
    "StartCounterAt",
    "Interval",
    "ActionToExecute",
    "AffectedBySpawnSpeed",
    "PauseTag",
    "StatsTags",
}
SPAWN_TO_LOCATION_KEYS = {
    "ClassType",
    "DeployTime",
    "ActionToRunOnSpawned",
    "SpawnType",
    "SpawnData",
    "MirroredX",
    "MirroredY",
}


def set_keys(row: dict) -> set[str]:
    """The keys an action row actually sets (a Row reads every column the table has, blank as None)."""
    return {k for k, v in row.items() if v is not None}


def interval_spawner(t: dict, rec: dict) -> dict | None:
    """THE INTERVAL SPAWNER as a named block (15.535): the one root action of class ActionInterval
    the row names, whose ActionToExecute is an ActionSpawnToLocation of a character. None for every
    other row, and for any row of that shape that sets a key outside INTERVAL_KEYS /
    SPAWN_TO_LOCATION_KEYS. The loader reads the block (card.rs `interval_spawner_of`)."""
    g = action_graph(t, rec)
    if not g:
        return None
    acts = t["actions"]
    found = [acts.get(v) for v in g["roots"].values()]
    found = [a for a in found if a is not None and a["ClassType"] == "ActionInterval"]
    if len(found) != 1:
        return None
    iv = found[0]
    if set_keys(iv) - INTERVAL_KEYS:
        return None
    sp = acts.get(iv["ActionToExecute"]) if isinstance(iv["ActionToExecute"], str) else None
    if sp is None or sp["ClassType"] != "ActionSpawnToLocation" or set_keys(sp) - SPAWN_TO_LOCATION_KEYS:
        return None
    hook = sp["ActionToRunOnSpawned"]
    if hook is not None:
        h = acts.get(hook) if isinstance(hook, str) else None
        if h is None or h["ClassType"] not in COSMETIC_ACTION_CLASSES:
            return None
    if sp["SpawnType"] != "CharacterType" or not isinstance(sp["SpawnData"], str) or not sp["SpawnData"]:
        return None
    tags = iv["PauseTag"]
    return {
        "start_counter_at_ms": iv["StartCounterAt"],
        "interval_ms": iv["Interval"],
        "affected_by_spawn_speed": iv["AffectedBySpawnSpeed"],
        "pause_tags": [x.strip() for x in tags.split(",") if x.strip()] if isinstance(tags, str) else [],
        "character": sp["SpawnData"],
        "deploy_time_ms": sp["DeployTime"],
        "mirrored_x": sp["MirroredX"],
        "mirrored_y": sp["MirroredY"],
    }


# The keys an idle buff's interval and its area spawn may set (`idle_buff_block`).
IDLE_INTERVAL_KEYS = {"Name", "ClassType", "Interval", "ActionToExecute", "ForceStopIfTrue"}
IDLE_SPAWN_KEYS = {"Name", "ClassType", "SpawnType", "SpawnData", "AbortIfInstigatorDies"}


def idle_buff_block(t: dict, c: dict, name: str, row: dict | None) -> dict:
    """AN IDLE BUFF THAT IS NOT AN INVISIBILITY (15.535): what a unit's BuffWhenNotAttacking does, for the loader
    (card.rs `idle_buff_of`): {buff_name, buff, time_ms, area} and, where the loader cannot read the buff's action,
    `action_graph`.

    `buff` is the buff's row, normalised as every buff is (`norm_buff_row`; None when the column names no row), and
    `time_ms` the unit row's BuffWhenNotAttackingTime. `area` is the one shape of the buff's OnStartAction the loader
    runs: an ActionInterval every `interval_ms` whose ActionToExecute is an ActionSpawn of an area (`area`, a name of
    `area_effect_objects`), stopped when the buff ends (ForceStopIfTrue "!TAG()" of the buff's own GameTagsToSet),
    nothing else set on either action but AbortIfInstigatorDies false on the spawn. The Super Knight's
    SuperKnight_ShieldBuff is that shape (every 50 ms, SuperKnight_ShieldAEO). Any other OnStartAction is written as
    `action_graph`, which the loader refuses; a buff with no OnStartAction writes neither (the Evo Knight's
    Knight_Fortify_EV1, a DamageReduction of its own)."""
    out: dict = {
        "buff_name": name,
        "buff": None if row is None else norm_buff_row(t, name, row),
        "time_ms": c["BuffWhenNotAttackingTime"],
        "area": None,
    }
    start = row["OnStartAction"] if row is not None else None
    if not start:
        return out
    acts = t["actions"]
    iv = acts.get(start) if isinstance(start, str) else None
    sp = acts.get(iv["ActionToExecute"]) if iv is not None and isinstance(iv["ActionToExecute"], str) else None
    readable = (
        iv is not None
        and sp is not None
        and iv["ClassType"] == "ActionInterval"
        and not set_keys(iv) - IDLE_INTERVAL_KEYS
        and isinstance(iv["Interval"], int)
        and iv["Interval"] > 0
        and iv["ForceStopIfTrue"] == f"!{row['GameTagsToSet']}()"
        and sp["ClassType"] == "ActionSpawn"
        and not set_keys(sp) - IDLE_SPAWN_KEYS
        and sp["SpawnType"] == "AreaEffectType"
        and isinstance(sp["SpawnData"], str)
        and sp["SpawnData"]
        and not sp["AbortIfInstigatorDies"]
    )
    if readable:
        out["area"] = {"interval_ms": iv["Interval"], "area": sp["SpawnData"]}
    else:
        out["action_graph"] = action_graph(t, row)
    return out


# THE ATTACK SELECTOR's one accepted shape (15.535; the Three Musketeers): the row's
# OnStartingAttackAction is an ActionFilter "target_in_range(<VARIABLE>) && target_is_ground" whose two
# branches are ActionSetAttackSequenceIndex, and the row's AttackSequenceList holds the row's own
# Projectile and one DoAttackAction: ActionRunOnInstigator -> ActionDealDamage (BaseDamageAmount, a
# DamageTypeBasic damage type) with an ActionPlayEffect after it.
ATTACK_SELECT_CONDITION = re.compile(r"^target_in_range\((\w+)\) && target_is_ground$")


def attack_select(t: dict, table: str, name: str, rec: dict) -> dict | None:
    """THE ATTACK SELECTOR as a named block (15.535): the melee branch's reach and damage, and which
    AttackSequenceList entry each branch picks. None for every other row. Fail-closed: a key this
    builder does not know on any action it walks, another condition, another sequence, or a damage
    type other than a plain one gives None, and the loader then refuses the graph as before."""
    if not isinstance(rec, Row) or "actions" not in t:
        return None
    acts = t["actions"]

    def action(n, cls: str, keys: set[str], required: set[str]):
        a = acts.get(n) if isinstance(n, str) else None
        if a is None or a["ClassType"] != cls:
            return None
        got = acts.set_fields.get(n, set())
        return a if required <= got <= keys else None

    filter_keys = {"ClassType", "Condition", "OnTrueAction", "OnFalseAction"}
    f = action(rec["OnStartingAttackAction"], "ActionFilter", filter_keys, filter_keys)
    if f is None:
        return None
    m = ATTACK_SELECT_CONDITION.match(f["Condition"] or "")
    reach = (t.variables.get(m.group(1)) or {}).get("DefaultValue") if m else None
    if not isinstance(reach, int) or isinstance(reach, bool) or reach <= 0:
        return None
    idx = {"ClassType", "AttackIndex"}
    on_true = action(f["OnTrueAction"], "ActionSetAttackSequenceIndex", idx, idx)
    on_false = action(f["OnFalseAction"], "ActionSetAttackSequenceIndex", idx, idx)
    if on_true is None or on_false is None:
        return None
    melee, ranged = on_true["AttackIndex"], on_false["AttackIndex"]
    seq = t[table].arrays.get(name, {}).get("AttackSequenceList")
    # AttackSequenceMode "None" with AttackSequence [0, 1]: the sequence does not advance by itself; the selector
    # alone sets the entry.
    order = t[table].arrays.get(name, {}).get("AttackSequence")
    if rec.get("AttackSequenceMode") != "None" or order != [0, 1]:
        return None
    if not isinstance(seq, list) or len(seq) != 2 or {melee, ranged} != {0, 1}:
        return None
    if seq[ranged] != {"Projectile": rec["Projectile"]} or set(seq[melee]) != {"DoAttackAction"}:
        return None
    run_keys = {"ClassType", "ActionToExecute", "NextAction"}
    run = action(seq[melee]["DoAttackAction"], "ActionRunOnInstigator", run_keys, {"ClassType", "ActionToExecute"})
    if run is None:
        return None
    deal_needs = {"ClassType", "BaseDamageAmount", "BaseDamageType"}
    deal = action(run["ActionToExecute"], "ActionDealDamage", deal_needs | {"StatsTags"}, deal_needs)
    if deal is None:
        return None
    effect = run["NextAction"]
    if effect is not None and action(effect, "ActionPlayEffect", {"ClassType", "Effect"}, {"ClassType"}) is None:
        return None
    dt = t.damage_types.get(deal["BaseDamageType"]) or {}
    damage = deal["BaseDamageAmount"]
    if dt.get("ClassType") != "DamageTypeBasic" or not set(dt) <= {"ClassType", "DamageEffect"}:
        return None
    if not isinstance(damage, int) or isinstance(damage, bool) or damage <= 0:
        return None
    return {
        "melee_range_milli": reach,
        "melee_ground_only": True,
        "melee_damage": damage,
        "melee_index": melee,
        "ranged_index": ranged,
    }


def summon_members(t: dict, key: str, s: dict, res: dict) -> list[dict] | None:
    """THE EXPLICIT SUMMON OFFSETS (15.535; the Three Musketeers): a SummonCharactersList card whose row
    ships SummonCharactersOffsetsX / Y, one member per list entry, in list order, with its offset in
    millitiles as the table gives it (the loader turns it into the owner's frame; calibration
    formation.EXPLICIT_OFFSETS_FRAME). None for a card resolved any other way or shipping no offsets.
    A list and an offsets table of different lengths is a data error the build refuses."""
    name = s["Name"]
    arr = t[key].arrays.get(name, {})

    def column(col: str) -> list | None:
        v = arr.get(col)
        if v is None and s.get(col) is not None:
            v = [s[col]]
        return v

    lst, xs, ys = column("SummonCharactersList"), column("SummonCharactersOffsetsX"), column("SummonCharactersOffsetsY")
    if not lst or (xs is None and ys is None) or ".SummonCharactersList (overlay;" not in res["source"]:
        return None
    if xs is None or ys is None or not (len(lst) == len(xs) == len(ys)):
        raise SystemExit(f"{key}.{name}: SummonCharactersList has {len(lst)} entries and the offsets {xs!r} / {ys!r}")
    if not all(isinstance(v, int) and not isinstance(v, bool) for v in [*xs, *ys]):
        raise SystemExit(f"{key}.{name}: SummonCharactersOffsetsX / Y are not integers: {xs!r} / {ys!r}")
    return [{"character": c, "offset_x_milli": x, "offset_y_milli": y} for c, x, y in zip(lst, xs, ys, strict=True)]


# THE RUNE GIANT'S ENCHANT (15.535): the keys each action row of the shape may carry. A row
# with any other key is a shape nobody has read, so the block is not written and the card stays
# refused. `StatsTags.*` arrive as one inline table, `StatsTags`.
ENCHANT_COLLECT_KEYS = frozenset({
    "ClassType", "ActionDelay", "UseAbility", "MaxFriendlyTroops", "DistanceToGetTargets",
    "DistanceToBuff", "DistanceToUnbuff", "TargetFilter", "Cooldown", "Projectile", "BuffDelay",
    "ActionWhenUnitBuffed", "StatsTags", "OnBuffAction",
})
ENCHANT_BUFF_KEYS = frozenset({
    "ClassType", "Singleton", "AttackAmount", "InstigatorDepth", "VisualActionForEnemyTarget",
    "GameTagsToSet", "AddedCrownTowerDamage", "AddedDamage", "FinishIfInstigatorDies", "StatsTags",
    "DamageMultiplierPerUnitNames", "DamageMultiplierPerUnitValues", "OnFinishedAction",
    "AttackAmountAction",
})
ENCHANT_VISUAL_KEYS = frozenset({
    "ClassType", "Singleton", "AddedDamageEffect", "OnAddedDamageAction", "AddedDamageMergeDuration",
})
# The three mechanic classes the block stands for: the loader checks the row's graph against them.
ENCHANT_CLASSES = ["ActionGiantBufferBuff", "ActionGiantBufferBuffVisual", "ActionGiantBufferCollectFriends"]
# The tag a row sets to be left out of the Rune Giant's pick (game_tags.csv: units that cannot be
# enchanted by the Rune Giant or the Chef).
ENCHANT_EXCLUDE_TAG = "NO_GIANTBUFFER_CHEF_ENCHANTMENT"


def _present(a: dict) -> set[str]:
    """The keys an action row sets."""
    return {k for k, v in a.items() if v is not None}


def _cosmetic_inline(v) -> bool:
    """An inline sub-action ([ACTION.name.Field]) that only plays an effect, or none at all."""
    return v is None or (isinstance(v, dict) and v.get("ClassType") == "ActionPlayEffect")


def _cosmetic_action(acts, name, seen: tuple[str, ...] = ()) -> bool:
    """A named action that only plays effects: an ActionPlayEffect, or an ActionSelect or
    ActionGroup whose every branch is one."""
    a = acts.get(name) if isinstance(name, str) else None
    if a is None or name in seen:
        return False
    if a["ClassType"] == "ActionPlayEffect":
        return True
    if a["ClassType"] in ("ActionSelect", "ActionGroup"):
        subs = acts.arrays.get(name, {}).get("SubActions") or [a["SubActions"]]
        return all(_cosmetic_action(acts, s, (*seen, name)) for s in subs)
    return False


def enchant_friends(t: dict, rec: dict) -> dict | None:
    """THE RUNE GIANT'S ENCHANT as a named block (15.535): the row's OnStartingAction is an
    ActionGiantBufferCollectFriends that sends a homing projectile to its friends, and the
    projectile's hit runs an ActionGiantBufferBuff on them. The parameters the loader reads, or
    None for every other row.

    Fail-closed, so the card stays refused with today's reason on: a root of another class; a
    key outside the known sets; the ability path (UseAbility) or an un-enchant distance; a
    non-cosmetic OnBuffAction, OnFinishedAction or AttackAmountAction; multiplier names and
    values of different lengths; a visual of another class; a projectile that deals damage, does
    not home, is not own-troops-only or may reset its target; an on-hit group that does not run
    the enchant at delay 0 beside cosmetic actions only."""
    if "actions" not in t or not isinstance(rec, Row):
        return None
    acts = t["actions"]
    root = rec["OnStartingAction"]
    a = acts.get(root) if isinstance(root, str) else None
    if a is None or a["ClassType"] != "ActionGiantBufferCollectFriends" or not _present(a) <= ENCHANT_COLLECT_KEYS:
        return None
    if a["UseAbility"] or a["DistanceToUnbuff"] not in (0, None) or not _cosmetic_inline(a["OnBuffAction"]):
        return None
    ename = a["ActionWhenUnitBuffed"]
    e = acts.get(ename) if isinstance(ename, str) else None
    if e is None or e["ClassType"] != "ActionGiantBufferBuff" or not _present(e) <= ENCHANT_BUFF_KEYS:
        return None
    if not e["Singleton"]:
        return None
    if not (_cosmetic_inline(e["OnFinishedAction"]) and _cosmetic_inline(e["AttackAmountAction"])):
        return None
    arrays = acts.arrays.get(ename, {})

    def listed(col: str) -> list:
        # element 0 is the column; a longer list is in `arrays` (OverlayTable.overlay)
        return arrays.get(col) or ([e[col]] if e[col] is not None else [])

    names, values = listed("DamageMultiplierPerUnitNames"), listed("DamageMultiplierPerUnitValues")
    if len(names) != len(values):
        return None
    if not all(isinstance(n, str) and isinstance(m, int) for n, m in zip(names, values, strict=True)):
        return None
    vname = e["VisualActionForEnemyTarget"]
    v = acts.get(vname) if isinstance(vname, str) else None
    if v is None or v["ClassType"] != "ActionGiantBufferBuffVisual" or not _present(v) <= ENCHANT_VISUAL_KEYS:
        return None
    if not _cosmetic_action(acts, v["OnAddedDamageAction"]):
        return None
    p = t["projectiles"].get(a["Projectile"]) if isinstance(a["Projectile"], str) else None
    if p is None or p["Damage"] not in (0, None) or not p["Homing"] or not p["OnlyOwnTroops"] or p["OnlyEnemies"]:
        return None
    if p["AllowResetTarget"] is not False or not isinstance(p["Speed"], int) or p["Speed"] <= 0:
        return None
    hit = p["OnHitTargetAction"]
    if not isinstance(hit, dict) or hit.get("ClassType") != "ActionGroup":
        return None
    subs, delays = hit.get("SubActions"), hit.get("SubActionsDelay")
    subs = subs if isinstance(subs, list) else [subs]
    delays = delays if isinstance(delays, list) else [delays] * len(subs)
    if len(subs) != len(delays) or list(zip(subs, delays, strict=True)).count((ename, 0)) != 1:
        return None
    if not all(s == ename or _cosmetic_action(acts, s) for s in subs):
        return None
    def tagged(r) -> bool:
        tags = r.get("GameTagsToSet")
        return isinstance(tags, str) and ENCHANT_EXCLUDE_TAG in [x.strip() for x in tags.split(",")]

    excluded = sorted(n for k in ("characters", "buildings") for n, r in t[k].records.items() if tagged(r))
    return {
        "collect": {
            "action": root,
            "action_delay_ms": a["ActionDelay"],
            "cooldown_ms": a["Cooldown"],
            "max_targets": a["MaxFriendlyTroops"],
            "pick_radius_milli": a["DistanceToGetTargets"],
            "buff_radius_milli": a["DistanceToBuff"],
            "buff_delay_ms": a["BuffDelay"],
            "target_filter": a["TargetFilter"],
        },
        "projectile": {"name": a["Projectile"], "speed": p["Speed"]},
        "enchant": {
            "action": ename,
            "attack_amount": e["AttackAmount"],
            "added_damage": e["AddedDamage"],
            "added_crown_tower_damage": e["AddedCrownTowerDamage"],
            "finish_if_instigator_dies_ms": e["FinishIfInstigatorDies"],
            "multipliers": [[n, m] for n, m in zip(names, values, strict=True)],
        },
        "excluded_units": excluded,
        "classes": list(ENCHANT_CLASSES),
    }


def _action_list(acts, name: str, col: str) -> list:
    """An action row's column as a list: the continued (list) value when the row has one, else
    the scalar alone, else empty."""
    got = acts.arrays.get(name, {}).get(col)
    if got is not None:
        return list(got)
    a = acts.get(name)
    return [] if a is None or a[col] is None else [a[col]]


def _transform_leaf(t: dict, name: str, delays: list | None, at: int | None, noops: list) -> dict | bool | None:
    """One action under a health trigger (`transform_at_hp`): the transformation it is (a dict),
    nothing the engine needs (None), or False for anything else, which refuses the whole block."""
    acts = t["actions"]
    a = acts.get(name)
    if a is None:
        return False
    ct = a["ClassType"]
    if ct == "ActionPlayEffect":
        return None
    if ct == "ActionChangeGameObjectData":
        into = a["NewCharacterData"]
        if not isinstance(into, str) or not into:
            return False  # a projectile's or an area's change: not a unit's
        return {
            "into": into,
            "reset_target": bool(flag(a, "ResetTarget")),
            "group_delays_ms": list(delays or []),
            "at": at or 0,
        }
    if ct == "ActionGroup":
        if delays is not None:
            return False  # a group inside a group
        subs = _action_list(acts, name, "SubActions")
        ds = _action_list(acts, name, "SubActionsDelay")
        if len(ds) not in (0, len(subs)):
            return False  # a delay list that does not pair with the sub-actions
        ds = ds or [0] * len(subs)
        out = None
        for k, sub in enumerate(subs):
            got = _transform_leaf(t, sub, ds, k, noops)
            if got is False:
                return False
            if got is not None:
                if out is not None:
                    return False  # two transformations in one group
                out = got
        return out
    if ct == "ActionSpawn" and a["SpawnType"] == "AreaEffectType" and isinstance(a["SpawnData"], str):
        # A TAUNT CANCEL: an area whose whole graph is ActionTaunt. The engine has no taunt, so
        # there is nothing for it to cancel; the loader accepts the spawn only by this name.
        g = action_graph(t, t["area_effect_objects"].get(a["SpawnData"]))
        if g and g["class_types"] == ["ActionTaunt"]:
            noops.append(f"AreaEffectType:{a['SpawnData']}")
            return None
    return False


def transform_at_hp(t: dict, rec: dict) -> dict | None:
    """THE HEALTH-THRESHOLD TRANSFORMATION as a named block (15.535): the row's one root action,
    OnStartingAction, is an ActionRunActionAtHealth whose HealthPercentages[i] runs Actions[i], and
    exactly one of those actions is an ActionChangeGameObjectData into a character row, run directly
    or as one sub-action of an ActionGroup. Effects are skipped, and the one spawn allowed is a taunt
    cancel (`_transform_leaf`). The Cannon Cart and the Goblin Demolisher carry it; every other row
    gives None, and the loader then refuses its graph as before. Fail-closed: any other action, a
    nested group, a delay list that does not pair, a second transformation, or a change of a
    projectile gives None.

    SubActionsDelay is written raw, in SubActions order, with the transformation's place in it
    (`at`): which of its two readings the client uses is not this function's to decide."""
    g = action_graph(t, rec)
    if not g or set(g["roots"]) != {"OnStartingAction"}:
        return None
    acts = t["actions"]
    root = g["roots"]["OnStartingAction"]
    a = acts.get(root)
    if a is None or a["ClassType"] != "ActionRunActionAtHealth":
        return None
    pcts = _action_list(acts, root, "HealthPercentages")
    names = _action_list(acts, root, "Actions")
    if not names or len(pcts) != len(names):
        return None
    hits: list[dict] = []
    noops: list[str] = []
    for p, n in zip(pcts, names, strict=True):
        got = _transform_leaf(t, n, None, None, noops)
        if got is False:
            return None
        if got is not None:
            hits.append({**got, "pct": p})
    if len(hits) != 1:
        return None
    return {**hits[0], "noop_spawns": noops}


def _group_leaves(acts, name: str) -> tuple[list[str], list[int]] | None:
    """An ActionGroup's sub-actions and their SubActionsDelay, raw and in SubActions order; None
    when `name` is not a group, a delay list does not pair with the sub-actions, or a sub-action is
    itself a group or names no action row."""
    a = acts.get(name)
    if a is None or a["ClassType"] != "ActionGroup":
        return None
    subs = _action_list(acts, name, "SubActions")
    ds = _action_list(acts, name, "SubActionsDelay")
    if not subs or len(ds) not in (0, len(subs)):
        return None
    for s in subs:
        sa = acts.get(s)
        if sa is None or sa["ClassType"] == "ActionGroup":
            return None
    return subs, list(ds) if ds else [0] * len(subs)


def parry(t: dict, rec: dict) -> dict | None:
    """THE COUNTER as a named block (15.535: the Ronin). The row's OnStartingAction is an
    ActionGroup of one ActionCounter and cosmetic effects. The counter's SelfAction group is one
    ActionRunForcedAnimationOnce and one ActionWithDuration (the cooldown tag). Its
    InstigatorAction group is one ActionSpawn of a BuffType, one ActionDealDamage and cosmetic
    effects, each at its SubActionsDelay. Every other root the row names must name no action row
    (the Ronin's VisualActions names a health bar that is not in the action tables).

    Fail-closed: any other action, a nested group, a delay list that does not pair, a second
    counter, spawn or damage gives None, and the loader then refuses the graph as before. The
    delays are written raw, in SubActions order, with each action's place in them: which of the
    two readings of SubActionsDelay the client uses is calibration actions.SUB_ACTIONS_DELAY's.
    `deploy_active` and `reflect_level_scaling` are True or False, and None when the column is not
    in the table at all (`flag`)."""
    g = action_graph(t, rec)
    if not g or "OnStartingAction" not in g["roots"]:
        return None
    acts = t["actions"]
    if any(acts.get(v) is not None for k, v in g["roots"].items() if k != "OnStartingAction"):
        return None
    root = _group_leaves(acts, g["roots"]["OnStartingAction"])
    if root is None:
        return None
    root_subs, root_delays = root
    counters = [k for k, s in enumerate(root_subs) if acts.get(s)["ClassType"] == "ActionCounter"]
    others = [s for s in root_subs if acts.get(s)["ClassType"] not in ("ActionCounter", "ActionPlayEffect")]
    if len(counters) != 1 or others:
        return None
    counter = acts.get(root_subs[counters[0]])
    own = _group_leaves(acts, counter["SelfAction"]) if isinstance(counter["SelfAction"], str) else None
    inst = _group_leaves(acts, counter["InstigatorAction"]) if isinstance(counter["InstigatorAction"], str) else None
    if own is None or inst is None:
        return None
    own_subs, own_delays = own
    own_classes = sorted(acts.get(s)["ClassType"] for s in own_subs)
    if own_classes != ["ActionRunForcedAnimationOnce", "ActionWithDuration"]:
        return None
    anim = next(acts.get(s) for s in own_subs if acts.get(s)["ClassType"] == "ActionRunForcedAnimationOnce")
    tag = next(acts.get(s) for s in own_subs if acts.get(s)["ClassType"] == "ActionWithDuration")
    inst_subs, inst_delays = inst
    cls = [acts.get(s)["ClassType"] for s in inst_subs]
    spawns = [k for k, c in enumerate(cls) if c == "ActionSpawn"]
    damages = [k for k, c in enumerate(cls) if c == "ActionDealDamage"]
    known = ("ActionSpawn", "ActionDealDamage", "ActionPlayEffect")
    if len(spawns) != 1 or len(damages) != 1 or any(c not in known for c in cls):
        return None
    spawn = acts.get(inst_subs[spawns[0]])
    damage = acts.get(inst_subs[damages[0]])
    if spawn["SpawnType"] != "BuffType" or not isinstance(spawn["SpawnData"], str):
        return None
    dt_name = damage["BaseDamageType"]
    dt = t["damage_types"].get(dt_name) if "damage_types" in t and isinstance(dt_name, str) else None
    return {
        "counter_cooldown_ms": counter["Cooldown"],
        "deploy_active": flag(counter, "DeployActive"),
        "damage_scalar_pct": counter["DamageScalar"],
        "defense_scalar_pct": counter["DefenseScalar"],
        "root_delays_ms": root_delays,
        "counter_at": counters[0],
        "self_delays_ms": own_delays,
        "self_forced_ms": anim["ForcedDuration"],
        "self_tag_ms": tag["ActionDuration"],
        "instigator_delays_ms": inst_delays,
        "stun_at": spawns[0],
        "reflect_at": damages[0],
        "stun": norm_buff(t, spawn["SpawnData"]),
        "stun_time_ms": spawn["SpawnTime"],
        "reflect_level_scaling": flag(dt, "EnableLevelScaling") if dt is not None else None,
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
    return norm_buff_row(t, name, b)


def norm_inline_buff(t: dict[str, Table], data: dict) -> dict:
    """A buff written inline where an action spawns it (the Void's three tiers: an ActionSpawn whose
    SpawnData is a table, not a buff's name), normalised as a 15.535 character_buffs row: every
    column the table knows and the inline table leaves out reads blank."""
    cols = set(t["character_buffs"].columns) | set(data)
    return norm_buff_row(t, data.get("Name"), Row(cols, dict(data)))


def norm_buff_row(t: dict[str, Table], name: str | None, b: dict) -> dict:
    out = {
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
    if isinstance(b, Row):
        # ControlledByParent (15.535 only, so the 2018 file stays byte-identical): the buff ends
        # with the area that applies it (the Tornado's), read with the area's ControlsBuff under
        # calibration status.AREA_BUFF_SOURCE_BINDING.
        out["controlled_by_parent"] = flag(b, "ControlledByParent")
        # THE BUFF'S DEATH SPAWN (15.535 only, so the 2018 file stays byte-identical): a unit that dies
        # while it carries the buff leaves `count` of `character` (the Mother Witch's VoodooCurse leaves a
        # VoodooHog, the Goblin Curse's mark a GoblinCurseGoblin), read under calibration
        # status.BUFF_DEATH_SPAWN_*. IgnoreBuildings: the buff never lands on a building.
        # CrownTowerDamagePerHit: the pulse a crown tower takes instead of the percent route
        # (status.CROWN_TOWER_DAMAGE_PER_HIT_SCALING).
        out["death_spawn"] = (
            None
            if b["DeathSpawn"] is None
            else {
                "character": b["DeathSpawn"],
                "count": b["DeathSpawnCount"],
                "is_enemy": flag(b, "DeathSpawnIsEnemy"),
                "deploy_delay": flag(b, "DeathSpawnDeployDelay"),
                "same_location": flag(b, "DeathSpawnSameLocation"),
                "other_buff_death_spawn_allowed": flag(b, "OtherBuffDeathSpawnAllowed"),
            }
        )
        out["ignore_buildings"] = flag(b, "IgnoreBuildings")
        out["crown_tower_damage_per_hit"] = b["CrownTowerDamagePerHit"]
        # Clone: the buff is the Clone spell's hold (the pair it copies is held, its lock and charge
        # kept; calibration spells.CLONE_HOLD_TARGETS). NotCloned: a copy does not take this buff
        # (spells.CLONE_COPY_BUFFS). AttachedInheritAs: the buff a caught mount's riders take (the Vines'
        # snare), carried and read by nothing. Each written only where set, so every other buff is
        # unchanged.
        if flag(b, "Clone"):
            out["clone"] = True
        if flag(b, "NotCloned"):
            out["not_cloned"] = True
        # Invisible: no enemy may target the carrier while it lasts (the Archer Queen's cape; the engine's target.rs
        # `invisible_at`). Written only where set, so every other buff is unchanged.
        if flag(b, "Invisible"):
            out["invisible"] = True
        if b["AttachedInheritAs"]:
            out["attached_inherit_as"] = b["AttachedInheritAs"]
        # UNKILLABLE among GameTagsToSet: the carrier's hitpoints do not fall below 1 while the buff lasts (the Hero
        # Berserker's rage). CharacterCrownTowerDamagePercent: the carrier's own hits deal this percent to a crown
        # tower while the buff lasts. Each written only where set, so every other buff is unchanged.
        if "UNKILLABLE" in str(b["GameTagsToSet"] or "").split(","):
            out["unkillable"] = True
        # NO_PUSHED_BY_ALLY among GameTagsToSet: the carrier's own side does not push it while the buff lasts (the Hero
        # Wizard's shot's HeroWizardNoMove). Written only where set.
        if "NO_PUSHED_BY_ALLY" in [x.strip() for x in str(b["GameTagsToSet"] or "").split(",")]:
            out["no_pushed_by_ally"] = True
        # NO_DAMAGE among GameTagsToSet: the carrier takes no damage while the buff lasts (the Evo Minion Horde's
        # MinionHorde_EV1_GhostBuff). Written only where set.
        if "NO_DAMAGE" in [x.strip() for x in str(b["GameTagsToSet"] or "").split(",")]:
            out["no_damage"] = True
        if b["CharacterCrownTowerDamagePercent"] is not None:
            out["character_crown_tower_damage_percent"] = b["CharacterCrownTowerDamagePercent"]
    return out


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
        # PingpongVisualTime (15.535 only, like action_graph, so the 2018 file stays byte-identical):
        # a range projectile that flies out and comes back over this many ms (the Executioner's
        # axe, 1500), read under calibration combat.RANGE_PROJECTILE. Despite the name it is not
        # cosmetic (tools/mechanic_register.py KEPT_BY_AUDIT).
        out["pingpong_visual_time_ms"] = p["PingpongVisualTime"]
        # 15.535 only, like action_graph, so the 2018 file stays byte-identical.
        # CheckCollisions, ProjectileStartExtraRadius and RandomDelay: a straight shot that is gone on the tick
        # it hits (the Hunter's pellet, the only row that sets the column; its evolution's pellet extends it),
        # its creation-tick reach and its random release delay, read under calibration
        # combat.PROJECTILE_COLLISIONS.
        out["check_collisions"] = flag(p, "CheckCollisions")
        out["projectile_start_extra_radius_milli"] = p["ProjectileStartExtraRadius"]
        out["random_delay_ms"] = p["RandomDelay"]
        # ChainedHitCount and ChainedHitRadius: a shot that goes on from its target to the next enemy,
        # ChainedHitCount targets in all (the Electro Dragon's 3, the Electro Spirit's 9: the only rows that set
        # them), read by card.rs `ChainHitDef`. Written only where set, so every other record is unchanged.
        if p["ChainedHitCount"]:
            out["chained_hit_count"] = p["ChainedHitCount"]
            out["chained_hit_radius_milli"] = p["ChainedHitRadius"]
        # SpawnCount, Scatter and SpawnRadius: the sparks a SpawnProjectile row releases where its carrier
        # lands (the Firecracker's FirecrackerExplosion: 5, "Line", 80), read under calibration
        # combat.SPAWN_PROJECTILE. SpawnRadius is carried raw: what it means is unsettled (it moves no spark's
        # start point; 80 over SpawnCount 5 is the 16 degrees between sparks, one row), and nothing reads it.
        out["spawn_count"] = p["SpawnCount"]
        out["scatter"] = p["Scatter"]
        out["spawn_radius_raw"] = p["SpawnRadius"]
        # ApplyBuffBeforeDamage: the TargetBuff lands before the hit's damage, so a unit the hit kills
        # still carries it (the Mother Witch's VoodooProjectile), read under calibration
        # status.APPLY_BUFF_BEFORE_DAMAGE. 15.535 only, so the 2018 file stays byte-identical.
        out["apply_buff_before_damage"] = flag(p, "ApplyBuffBeforeDamage")
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
        out["action_graph"] = action_graph(t, a, inline_row=name if WALK_INLINE else None)
        # ControlsBuff (15.535 only, so the 2018 file stays byte-identical): the buff this area
        # applies lives only as long as the area does (the Tornado), read with the buff's own
        # ControlledByParent under calibration status.AREA_BUFF_SOURCE_BINDING.
        out["controls_buff"] = flag(a, "ControlsBuff")
        # SpawnAreaEffectObject: the NAME of a one-shot area this area makes on its first
        # update (Rage's RageDamage; spells.CHILD_AREA_BIRTH), whose record is in the same
        # top-level map. BuffNumber: how many of the buff one application stacks (1 on every
        # row the loader reads). 15.535 only, so the 2018 file stays byte-identical.
        out["spawn_area_effect_object"] = a["SpawnAreaEffectObject"]
        out["buff_number"] = a["BuffNumber"]
        # HitBiggestTargets: the area strikes its highest-hp enemies one at a time through its
        # Projectile row (Lightning; spells.STRIKE_*). 15.535 only, so the 2018 file stays
        # byte-identical.
        out["hit_biggest_targets"] = flag(a, "HitBiggestTargets")
        # What the area's OnStartingAction and OnHitAction RUN, in order (`action_schedule`): the Goblin
        # Curse's area makes its curse circle, and the circle's hit hangs two buffs. SpawnTime: how long
        # the unit an area's projectile releases takes to deploy, as the area row says it (the Royal
        # Delivery's 250, which the loader holds against the projectile's own). 15.535 only, so the 2018
        # file stays byte-identical.
        out["schedule"] = action_schedule(t, a["OnStartingAction"]) if isinstance(a["OnStartingAction"], str) else None
        out["on_hit"] = action_schedule(t, a["OnHitAction"]) if isinstance(a["OnHitAction"], str) else None
        out["spawn_time_ms"] = a["SpawnTime"]
        # A STRIKING AREA WHOSE STRIKES ARE AN ACTION (`strike_area_block`: the Vines' ranked catches, the
        # Void's laser ball), and THE CLONE (Clone, and its action `clone_action_block`). Each written only
        # where the row is one, so every other area is unchanged.
        sa = strike_area_block(t, a)
        if sa is not None:
            out["strike_area"] = sa
        if flag(a, "Clone"):
            out["clone"] = True
            out["clone_action"] = clone_action_block(t, a)
    return out


def unit_record(t: dict[str, Table], name: str) -> tuple[str, dict]:
    for key in ("characters", "buildings"):
        r = t[key].get(name)
        if r is not None:
            return key, r
    raise KeyError(name)


def sequence_damage(tb, name: str) -> int | None:
    """The Damage of a row's AttackSequenceList (15.535 overlays: a list of inline tables,
    one per attack of the sequence), when EVERY entry carries one and they are all the same
    number; None otherwise. The Berserker's row leaves Damage blank and ships three entries
    of 40, so without this its hits resolve to nothing. A sequence whose entries differ, or
    whose entries carry a projectile instead (the Musketeer's and the Princess's
    evolutions), is a different mechanic and gives None, so no damage is guessed for it.
    The list never reaches `raw` (an inline table is not a column value there), so
    tools/check_card_reads.py has no column to score for it."""
    seq = tb.arrays.get(name, {}).get("AttackSequenceList")
    if seq is None:
        one = tb.get(name).get("AttackSequenceList")
        seq = [one] if isinstance(one, dict) else None
    if not seq or not all(isinstance(e, dict) for e in seq):
        return None
    dmg = {e.get("Damage") for e in seq}
    if len(dmg) != 1:
        return None
    (d,) = dmg
    return d if isinstance(d, int) and not isinstance(d, bool) else None


# THE COMBO (15.535; the Monk and the Mega Monk): a row whose AttackSequence advances by itself (no
# AttackSequenceMode, no AttackSequenceList) and that sets VariableDamage2 with no VariableDamageTime1. Its
# columns come in three stages -- Damage / MeleePushback / IsMeleePushbackAll, VariableDamage2 / MeleePushback2 /
# IsMeleePushbackAll2, VariableDamage3 / MeleePushback3 / IsMeleePushbackAll3 -- and entry k of the sequence deals
# stage k's damage and melee pushback (calibration combat.ATTACK_COMBO, knockback.COMBO_PUSHBACK). The Monk:
# [0, 1, 2], 55 / 55 / 165, the third pushing 1800.
COMBO_STAGES = (
    ("Damage", "MeleePushback", "IsMeleePushbackAll"),
    ("VariableDamage2", "MeleePushback2", "IsMeleePushbackAll2"),
    ("VariableDamage3", "MeleePushback3", "IsMeleePushbackAll3"),
)


def combo(t: dict, table: str, name: str, c: dict) -> dict | None:
    """THE COMBO as a named block: `sequence` the row's AttackSequence, `stages` the three stages' damage, melee
    pushback (millitiles, 0 on a blank) and IsMeleePushbackAll. None for every other row: a 2018 row, a row with no
    VariableDamage2, a ramp (VariableDamageTime1 set), a sequence that a mode or a list drives. Fail-closed: a
    sequence entry that names no stage, or an entry whose stage has no damage, gives None, and the row carries no
    combo."""
    if not isinstance(c, Row) or c.get("VariableDamage2") is None or c.get("VariableDamageTime1") is not None:
        return None
    arrays = t[table].arrays.get(name, {})
    if c.get("AttackSequenceMode") is not None or arrays.get("AttackSequenceList") is not None:
        return None
    order = arrays.get("AttackSequence")
    if not isinstance(order, list) or not order or any(k not in (0, 1, 2) or isinstance(k, bool) for k in order):
        return None
    # A characters.csv row writes the sequence's first element (0) blank and the reader drops it: the Mega Monk (an
    # event unit) reads [1, 2]. A sequence that does not start on stage 0 is refused rather than read without it.
    if order[0] != 0:
        return None
    stages = [
        {"damage": c.get(d), "pushback_milli": c.get(p) or 0, "pushback_all": bool(flag(c, a))}
        for d, p, a in COMBO_STAGES
    ]
    if any(stages[k]["damage"] is None for k in order):
        return None
    return {"sequence": list(order), "stages": stages}


def norm_unit(t: dict[str, Table], name: str, with_raw: bool = False) -> dict:
    table, c = unit_record(t, name)
    defaults: list[str] = []
    proj = norm_projectile(t, c["Projectile"])
    # A blank Damage with no damaging projectile falls back to the attack sequence's one
    # Damage (`sequence_damage`: the Berserker's 40, client 15.535.29; 102 at level 11).
    seq_damage = sequence_damage(t[table], name) if c["Damage"] is None else None
    attacks = c["HitSpeed"] is not None and (c["Damage"] is not None or proj is not None or seq_damage is not None)

    if c["Damage"] is not None:
        damage, dmg_src = c["Damage"], f"{table}.{name}.Damage"
        ct_raw = c["CrownTowerDamagePercent"]
        ct = ct_percent(ct_raw)
    elif proj is not None and proj["damage"] is not None:
        damage, dmg_src = proj["damage"], f"projectiles.{proj['name']}.Damage"
        ct_raw = proj["crown_tower_damage_percent_raw"]
        ct = proj["crown_tower_damage_percent"]
    elif seq_damage is not None:
        damage, dmg_src = seq_damage, f"{table}.{name}.AttackSequenceList.Damage"
        ct_raw = c["CrownTowerDamagePercent"]
        ct = ct_percent(ct_raw)
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
        # StopMovementAfterMS and then stands still for WaitMS, and the per-tick
        # displacement measured on client 15.535.29 is the FASTER figure (calibration.json
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
        # DeathSpawnProjectile: the NAME of a `projectiles` row the unit leaves where it dies (the
        # Phoenix's PhoenixFireball, whose SpawnCharacter is the egg), read under calibration
        # spawner.DEATH_SPAWN_PROJECTILE. 15.535 rows only (dropped below on 2018 rows).
        "death_spawn_projectile": c.get("DeathSpawnProjectile"),
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
        # SpawnAreaObject: the NAME of an `area_effect_objects` row the unit puts down where it
        # appears (the Battle Healer's BattleHealerSpawnHeal), read under calibration
        # spawner.SPAWN_AREA_OBJECT_SCOPE. SpawnAreaObjectLevelIndex is not carried. 15.535 rows only.
        "spawn_area_object": c.get("SpawnAreaObject"),
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
        # different LifeTime and a spawner the dig row does not have. card.rs runs the
        # walk and the morph for a played card whose spell row sets CanDeployOnEnemySide
        # (`summon_card` writes the flag beside this block, 15.535 only) and refuses every
        # other shape: a spawned unit that tunnels, a row without the flag.
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
        # dash-jump, a different state) and get no block here; on the 15.535 rows their
        # JumpSpeed is the dash block's `speed` (after this literal). 2018 vintage: HogRider only;
        # the 15.535 card data adds Prince, DarkPrince, the Battle Ram's Ram and
        # RoyalHog with the identical 4000 / 160.
        "jump": None
        if not c["JumpEnabled"]
        else {
            "height_raw": c["JumpHeight"],
            "speed": c["JumpSpeed"],
        },
        # Hovering, beside the jump: a ground unit that crosses the river without a leap (the
        # Battle Healer, the Royal Ghost), priced like a jumper by the path search under
        # calibration pathfinding.HOVERING_WATER_RULE. 15.535 rows only (dropped below on 2018
        # rows); null where the table has no such column.
        "hovering": flag(c, "Hovering") if isinstance(c, Row) else None,
        "hides_when_not_attacking": flag(c, "HidesWhenNotAttacking"),
        # HideTimeMs is the hide state machine's (the Tesla's), so a 15.535 row that does not hide
        # carries none: the Royal Ghost ships 400 beside a blank HidesWhenNotAttacking. The 2018
        # rows keep what they wrote, so that file stays byte-identical.
        "hide_time_ms": c["HideTimeMs"] if (not isinstance(c, Row) or flag(c, "HidesWhenNotAttacking")) else None,
        "up_time_ms": c["UpTimeMs"],
        "buff_on_damage": None
        if c["BuffOnDamage"] is None
        else {"buff": norm_buff(t, c["BuffOnDamage"]), "time_ms": c["BuffOnDamageTime"]},
        # IgnoreBuff: the buff rows that never land on this unit (a list column; the VoodooHog lists
        # VoodooCurse and GoblinCurse, so a hog is never cursed into a second hog). 15.535 rows only
        # (dropped below on 2018 rows).
        "ignore_buffs": None
        if c.get("IgnoreBuff") is None
        else list(t[table].arrays.get(name, {}).get("IgnoreBuff") or [c.get("IgnoreBuff")]),
        # THE DAMAGE RAMP (VariableDamage2 / VariableDamage3 / VariableDamageTime1 /
        # VariableDamageTime2): a hit deals Damage, then damage2, then damage3 as the attack
        # progress on one target passes time1_ms and time1_ms + time2_ms (calibration
        # combat.VARIABLE_DAMAGE). Written only when ALL FOUR are set (Inferno Tower, Inferno Dragon,
        # Mighty Miner): the loader refuses a half-blank block, and the Monk and the Mega Monk carry
        # the two damages with no times, which is not this ramp -- their columns stay in `raw`,
        # unread, rather than be given times the table does not have. 15.535 rows only.
        "variable_damage": None
        if c.get("VariableDamage2") is None
        or c.get("VariableDamage3") is None
        or c.get("VariableDamageTime1") is None
        or c.get("VariableDamageTime2") is None
        else {
            "damage2": c.get("VariableDamage2"),
            "damage3": c.get("VariableDamage3"),
            "time1_ms": c.get("VariableDamageTime1"),
            "time2_ms": c.get("VariableDamageTime2"),
        },
        # THE COMBO (`combo`): an AttackSequence that advances by itself, with VariableDamage2 and no
        # VariableDamageTime1 (the Monk, the Mega Monk). Dropped below from every row that has none.
        "combo": combo(t, table, name, c),
        # AttackPushBack: the recoil of each of the unit's own attacks, away from its target (the
        # Sparky 750, the Firecracker 1000), under calibration knockback.ATTACK_PUSHBACK. 15.535
        # rows only.
        "attack_pushback_milli": c.get("AttackPushBack"),
        # DeathPushBack: the push a death's damage gives the units it hits, radially from the death
        # point (the Skeleton Barrel's container 1000, the Golem 1800), under calibration
        # knockback.DEATH_PUSHBACK. 15.535 rows only.
        "death_pushback_milli": c.get("DeathPushBack"),
        # ProjectileYOffset: a shot is born this much further along its attacker's own forward y than
        # ProjectileStartRadius alone puts it (the King Tower 400), under calibration
        # combat.PROJECTILE_Y_OFFSET. 15.535 rows only, and dropped below from every row that does not
        # set it, so no other row and nothing in the 2018 file changes. The column also stays out of
        # `raw` (COSMETIC), which is why it is read here by name.
        "projectile_y_offset_milli": c.get("ProjectileYOffset"),
        # OverrideAttackFinishTime: the row's attack ends with its hit, so a kill costs it no retarget wait
        # (calibration combat.POST_KILL_RETARGET_WAIT's clause (a)). Read by name so an [EXT] row that inherits it (the
        # Hero Valkyrie's) carries it; 15.535 rows that set it true only, so every other row is unchanged.
        "override_attack_finish": c.get("OverrideAttackFinishTime"),
        # THE SPECIAL (SpecialRange / SpecialMinRange / SpecialLoadTime / ProjectileSpecial): the
        # Fisherman's hook, under calibration combat.SPECIAL_HOOK. `projectile` is the
        # ProjectileSpecial row in the shape of every projectile object, and `drag_margin_milli`
        # that row's DragMargin, which the projectile object does not carry. Written when
        # SpecialRange is set (the Firecracker evolution names a ProjectileSpecial with no range,
        # which is not this special). 15.535 rows only.
        "special": None
        if c.get("SpecialRange") is None
        else {
            "range_milli": c.get("SpecialRange"),
            "min_range_milli": c.get("SpecialMinRange"),
            "load_time_ms": c.get("SpecialLoadTime"),
            "projectile": norm_projectile(t, c.get("ProjectileSpecial")),
            "drag_margin_milli": (t["projectiles"].get(c.get("ProjectileSpecial")) or {}).get("DragMargin"),
        },
        # THE REFLECT (ReflectedAttackDamage / ReflectAttackCrownTowerDamage / ReflectedAttackRadius /
        # ReflectedAttackBuff / ReflectedAttackBuffDuration): a melee hit on the unit is answered
        # with damage and a stun on the attacker (calibration.json combat.REFLECT_ATTACK). 15.535
        # only, and set on the Electro Giant's row alone; the key is dropped below from every row
        # that sets none of the five, so no other row and nothing in the 2018 file changes. The
        # ReflectedAttack*Effect columns and ReflectedAttackTargetedEffectSources are cosmetic
        # and not carried.
        "reflected_attack": None
        if c.get("ReflectedAttackDamage") is None
        and c.get("ReflectAttackCrownTowerDamage") is None
        and c.get("ReflectedAttackRadius") is None
        and c.get("ReflectedAttackBuff") is None
        and c.get("ReflectedAttackBuffDuration") is None
        else {
            "damage": c.get("ReflectedAttackDamage"),
            "crown_tower_damage": c.get("ReflectAttackCrownTowerDamage"),
            "radius_milli": c.get("ReflectedAttackRadius"),
            "buff": norm_buff(t, c.get("ReflectedAttackBuff")),
            "buff_duration_ms": c.get("ReflectedAttackBuffDuration"),
        },
        # THE ELIXIR COLUMNS (ManaCollectAmount / ManaGenerateTimeMs / ManaOnDeath /
        # ManaOnDeathForOpponent): the Elixir Collector's payout and its elixir on death, the Elixir
        # Golem's elixir for the opponent (calibration economy.*). 15.535 only, and dropped below from
        # every row that sets none of the four, so no other row and nothing in the 2018 file changes.
        "mana": None
        if c.get("ManaCollectAmount") is None
        and c.get("ManaGenerateTimeMs") is None
        and c.get("ManaOnDeath") is None
        and c.get("ManaOnDeathForOpponent") is None
        else {
            "collect_amount": c.get("ManaCollectAmount"),
            "generate_time_ms": c.get("ManaGenerateTimeMs"),
            "on_death": c.get("ManaOnDeath"),
            "on_death_for_opponent": c.get("ManaOnDeathForOpponent"),
        },
        "attached_character": c["AttachedCharacter"],
        "defaults_applied": defaults,
    }
    if u["reflected_attack"] is None:
        del u["reflected_attack"]
    if u["combo"] is None:
        del u["combo"]
    if u["mana"] is None or not isinstance(c, Row):
        del u["mana"]
    if u["projectile_y_offset_milli"] is None or not isinstance(c, Row):
        del u["projectile_y_offset_milli"]
    if u["override_attack_finish"] is not True or not isinstance(c, Row):
        del u["override_attack_finish"]
    if not isinstance(c, Row):
        # The 2018 file stays byte-identical: it does not grow the keys written for the 15.535
        # rows alone (UNIT_FIELDS_15535).
        for k in UNIT_FIELDS_15535:
            del u[k]
    if isinstance(c, Row):
        # 15.535: the scripted actions the row reaches (None when it names none), with the
        # DoAttackAction roots of its AttackSequenceList (the Three Musketeers' bayonet).
        u["action_graph"] = action_graph(t, c, t[table].arrays.get(name, {}).get("AttackSequenceList"))
        # The Goblin Hut's controller, when the row's graph is one (`life_state_spawner`);
        # written only there, so every other row is unchanged.
        ls = life_state_spawner(t, c)
        if ls is not None:
            u["life_state_spawner"] = ls
        # The Furnace's interval spawner, when the row's graph is one (`interval_spawner`);
        # written only there, so every other row is unchanged.
        iv = interval_spawner(t, c)
        if iv is not None:
            u["interval_spawner"] = iv
        # The Three Musketeers' attack selector (`attack_select`); written only there.
        sel = attack_select(t, table, name, c)
        if sel is not None:
            u["attack_select"] = sel
        # The Rune Giant's enchant, when the row's graph is one (`enchant_friends`); written only
        # there, so every other row is unchanged.
        ef = enchant_friends(t, c)
        if ef is not None:
            u["enchant_friends"] = ef
        # The health-threshold transformation (the Cannon Cart, the Goblin Demolisher), when the
        # row's graph is one (`transform_at_hp`); written only there, so every other row is unchanged.
        tb = transform_at_hp(t, c)
        if tb is not None:
            u["transform_at_hp"] = tb
        # The counter (the Ronin), when the row's graph is one (`parry`); written only there, so
        # every other row is unchanged.
        pr = parry(t, c)
        if pr is not None:
            u["parry"] = pr
        # INVISIBLE WHEN IDLE (the Royal Ghost): BuffWhenNotAttacking names a buff whose own row sets
        # Invisible. Written only then, so the Super Knight's idle buff (not an invisibility) and
        # every other row are unchanged.
        idle = c["BuffWhenNotAttacking"]
        idle_row = t["character_buffs"].get(idle) if isinstance(idle, str) and idle else None
        if idle_row is not None and flag(idle_row, "Invisible"):
            u["idle_invisibility"] = {
                "buff": idle,
                "time_ms": c["BuffWhenNotAttackingTime"],
                "use_attack_range": flag(c, "BuffWhenNotAttackingUseAttackRange"),
                "area_damage_when_invisible": flag(c, "AllowAreaDmgWhenInvisible"),
            }
            # StartWithBuffWhenNotAttacking (the Evo Royal Ghost's pair: false): written only where the column is set.
            if c.get("StartWithBuffWhenNotAttacking") is not None:
                u["idle_invisibility"]["starts_hidden"] = bool(c["StartWithBuffWhenNotAttacking"])
        # ANY OTHER IDLE BUFF (the Super Knight's shield, whose buff starts an interval of own-troop areas; the Evo
        # Knight's own DamageReduction): the buff's row, the idle time, and what its OnStartAction does
        # (`idle_buff_block`). Written only where the column is set and the buff is not an invisibility, so every
        # other row is unchanged; the loader refuses a row whose block says something it does not run.
        elif isinstance(idle, str) and idle:
            u["idle_buff"] = idle_buff_block(t, c, idle, idle_row)
        # THE ATTACHED RIDER (the Ram Rider's rider, the Goblin Giant's two Spear Goblins): a
        # SpawnCharacter block with SpawnAttach is not a periodic spawner but units that ride this
        # row and stand where it stood a tick before (card.rs `AttachDef`, calibration rider.*).
        # Written into the spawner block only where the column is set, so every other row is
        # unchanged.
        if u["spawner"] is not None and flag(c, "SpawnAttach"):
            u["spawner"]["attach"] = True
        # A rider row's own targeting columns, each written only where it is set (the Ram Rider's
        # rider; the Fisherbarrel also sets TargetOnlyTroops and three event rows
        # IgnoreTargetsWithBuff): TargetOnlyTroops, and the buff whose carriers the unit ranks last
        # (IgnoreTargetsWithBuff, read with DeprioritizeTargetsWithBuff; calibration
        # targeting.DEPRIORITIZED_TARGET_BUFF). SpawnMaxAngle on the Spear Goblins is the arc the Goblin
        # Giant's riders spread over (card.rs FormationDef, calibration rider.OFFSET_LAW); the facing
        # clamp SpawnAttachMaxRotation on the Ram Rider's rider is carried and read by nothing.
        if flag(c, "TargetOnlyTroops"):
            u["target_only_troops"] = True
        if c.get("IgnoreTargetsWithBuff") is not None:
            u["ignore_targets_with_buff"] = norm_buff(t, c.get("IgnoreTargetsWithBuff"))
        if flag(c, "DeprioritizeTargetsWithBuff"):
            u["deprioritize_targets_with_buff"] = True
        if c.get("SpawnAttachMaxRotation") is not None:
            u["attach_max_rotation_deg"] = c.get("SpawnAttachMaxRotation")
        if c.get("SpawnMaxAngle") is not None:
            u["spawn_max_angle_deg"] = c.get("SpawnMaxAngle")
        # IgnoreClone: the Clone spell never copies this unit (the Goblin Drill's dig, the chess Recruits).
        # Written only where set, so every other row is unchanged.
        if flag(c, "IgnoreClone"):
            u["ignore_clone"] = True
        # DeathSpawnPushback, beside the death_spawn block it qualifies: whether this row's
        # death spawn starts on a small ring and slides out to DeathSpawnRadius (calibration
        # spawner.DEATH_SPAWN_PUSHBACK; measured on client 16.402 on the Golem and the Lava
        # Hound, which set it, against the Battle Ram, which leaves it blank). Written here,
        # after the literal and on the 15.535 rows only, so the 2018 file stays byte-identical;
        # tools/check_card_reads.py's PROLOGUE names it for that reason.
        u["death_spawn_pushback"] = flag(c, "DeathSpawnPushback")
        # SpawnCharacter2, into the spawner block: a second periodic unit (the Super Witch's Bat), which
        # the loader refuses. Written on the 15.535 rows only, after the literal, for the reason
        # DeathSpawnPushback is; tools/check_card_reads.py's PROLOGUE names it.
        if u["spawner"] is not None:
            u["spawner"]["character2"] = c["SpawnCharacter2"]
        # THE DASH'S MOTION, into the dash block: JumpSpeed (native units per tick while it
        # dashes), DashConstantTime (the Mega Knight's blow lands this long after the dash
        # starts) and DashLandingTime, all read by calibration combat.DASH_ATTACK. On the
        # 15.535 rows only, for the reason DeathSpawnPushback is: the 2018 file stays
        # byte-identical, so its Bandit and Mega Knight carry a dash block with no speed and
        # load no dash (card.rs `convert_dash`).
        #
        # A dash block with no DashMaxRange never starts on its own: the Golden Knight's chain
        # and the event Hog Rider's dash start from an Ability or a scripted action, which the
        # loader does not run. That block moves to `triggered_dash`, which nothing reads, so
        # tools/check_card_reads.py goes on calling those columns unread.
        if u["dash"] is not None and u["dash"]["max_range_milli"] is None:
            u["triggered_dash"], u["dash"] = u["dash"], None
        if u["dash"] is not None:
            u["dash"]["speed"] = c["JumpSpeed"]
            u["dash"]["constant_time_ms"] = c["DashConstantTime"]
            u["dash"]["landing_time_ms"] = c["DashLandingTime"]
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

# Unit fields written on the 15.535 rows only (`norm_unit` drops them from a 2018 row, so the
# 2018 file stays byte-identical), and copied onto the card and tower rows built from those
# units beside UNIT_FIELDS_FOR_CARD. A spell card row carries none of them (absent reads as
# blank in card.rs).
UNIT_FIELDS_15535 = [
    "death_spawn_projectile",
    "spawn_area_object",
    "hovering",
    "variable_damage",
    "attack_pushback_milli",
    "special",
    "ignore_buffs",
    "death_pushback_milli",
]

# A rider row's targeting columns and facing clamps (`norm_unit`, 15.535 rows only, each written
# only where the column is set).
RIDER_FIELDS = [
    "target_only_troops",
    "ignore_targets_with_buff",
    "deprioritize_targets_with_buff",
    "attach_max_rotation_deg",
    "spawn_max_angle_deg",
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
        area = s["AreaEffectObject"]
        found = spawned_characters(t, area)
        row = t["area_effect_objects"].get(area) if isinstance(area, str) and area else None
        own = row["SpawnCharacter"] if row is not None else None
        if isinstance(own, str) and own:
            # THE AREA'S OWN SpawnCharacter IS THE CARD'S UNIT (15.535: TriWizards, whose TriWizardSpawn
            # puts the TriWizard down itself and the Electro Wizard and the Ice Wizard through the two
            # areas its OnStartingAction makes). The area is written on the card as
            # `deploy_spawn_area` (`summon_card`), and the loader reads the whole deploy from it.
            return {
                "character": own,
                "count": 1,
                "source": f"{key}.{name}.AreaEffectObject -> area_effect_objects.{area}.SpawnCharacter",
                "others": [c for c, _ in found if c != own],
                "deploy_spawn_area": area,
            }
        if found:
            first = found[0][0]
            return {
                "character": first,
                "count": sum(1 for c, _ in found if c == first),
                "source": f"{key}.{name}.AreaEffectObject -> {found[0][1]}",
                "others": [c for c, _ in found if c != first],
            }
    raise SystemExit(f"{key}.{name}: no SummonCharacter and no resolvable spawn graph")


def deploy_area_effect(t: dict, s: dict, character: str) -> str | None:
    """The card row's AreaEffectObject (spells_characters / spells_buildings) when that area is
    the deploy of the card's own unit: its OnStartingAction graph spawns exactly one character,
    `character`, straight from the area with no second area in between. The Electro Wizard's
    ElectroWizardZap and the Ice Wizard's IceWizardCold: the zap and the chill land where the
    wizard appears (calibration spells.DEPLOY_AREA_EFFECT). None for every other card, and for
    TriWizards, whose TriWizardSpawn puts its own TriWizard down and two wizards through two
    further areas: that is not one area on one unit, and it is written as `deploy_spawn_area`
    instead (`resolve_summon`)."""
    name = s.get("AreaEffectObject")
    if not isinstance(name, str) or not name or "actions" not in t:
        return None
    found = spawned_characters(t, name)
    if len(found) != 1:
        return None
    unit, path = found[0]
    if unit != character or path.count("area_effect_objects.") != 1:
        return None
    return name


#: The keys `champion_dash_chain` reads off the charge action (ActionRunActionListOnObjectsInShapeWithPrio); any
#: other stops the build.
CHAIN_CHARGE_KEYS = {
    "ClassType", "AbortIfInstigatorDies", "ActionOnSelfWhenTriggered", "Actions", "Delays", "GameTagsToSet",
    "OncePerTarget", "PauseTags", "Shape", "TargetFilter", "TargetSelectionMode", "WaitForTarget",
}


# The Deflect's ability, AEO and tag action: every column the reader takes, or none (cosmetic columns aside).
DEFLECT_ABILITY_READ = {
    "AbilityStateDuration", "AreaEffectObject", "Buff", "BuffTime", "CastTime", "MaxCharges", "ManaCost", "Name",
    "OnActivationAction", "TriggerDelay", "GameTagsWhileAbilityActive", "StatsTags", "Stats",
}
DEFLECT_ABILITY_COSMETIC = {
    "KeepIconEvenWhenOutOfCharges", "HideChargesTextField", "DeployedClip", "DeployedEffect", "IconExportName",
    "IconSWF", "TID", "TID_INFO",
}
DEFLECT_AEO_READ = {
    "DeflectProjectilesEnabled", "FollowBehaviour", "HitsAir", "HitsGround", "IgnoreBuildings", "LifeDuration", "Name",
    "OnlyEnemies", "Radius", "Rarity",
}
DEFLECT_AEO_COSMETIC = {"DeflectedProjectileEffect", "DeflectionFBEffect", "SpawnDeployBaseAnim"}
# The tags the Deflect holds its champion with while it is active: he stands (no move; an attract still moves him).
DEFLECT_HOLD_TAGS = "AVOIDANCE_AS_OBSTACLE,NO_MOVE_ALLOW_ATTRACT"


def champion_deflect(t, unit: str) -> dict | None:
    """THE BUTTON OF A CHAMPION WHOSE PRESS DEFLECTS (15.535: the Monk's Deflect), or None (the card loads as a plain
    troop). The ability row hangs a buff on the champion for BuffTime, puts down an area effect that follows him and
    deflects projectiles (DeflectProjectilesEnabled) for its LifeDuration, holds him while it is active
    (GameTagsWhileAbilityActive AVOIDANCE_AS_OBSTACLE, NO_MOVE_ALLOW_ATTRACT for AbilityStateDuration), and its
    OnActivationAction only sets tags for a duration. Any other column, or any other shape, gives None."""
    row = t["characters"].get(unit)
    name = row["Ability"] if isinstance(row, Row) and "Ability" in row.columns else None
    a = t.abilities.get(name) if isinstance(name, str) else None
    if a is None or set(a) - DEFLECT_ABILITY_READ - DEFLECT_ABILITY_COSMETIC:
        return None
    if a.get("GameTagsWhileAbilityActive") != DEFLECT_HOLD_TAGS or a.get("MaxCharges") != 1:
        return None
    aeo_name = a.get("AreaEffectObject")
    aeo_tb = t["area_effect_objects"]
    aeo = aeo_tb.get(aeo_name) if isinstance(aeo_name, str) else None
    if aeo is None or aeo_tb.set_fields.get(aeo_name, set()) - DEFLECT_AEO_READ - DEFLECT_AEO_COSMETIC:
        return None
    if not aeo["DeflectProjectilesEnabled"] or aeo["FollowBehaviour"] != "FollowParent":
        return None
    on = a.get("OnActivationAction")
    act = t["actions"].get(on) if isinstance(on, str) else None
    if act is None or act["ClassType"] != "ActionWithDuration":
        return None
    if t["actions"].set_fields.get(on, set()) != {"ActionDuration", "ClassType", "GameTagsToSet"}:
        return None
    buff = norm_buff(t, a.get("Buff"))
    ints = [a.get(k) for k in ("AbilityStateDuration", "BuffTime", "CastTime", "TriggerDelay", "ManaCost")]
    if buff is None or not all(isinstance(v, int) and not isinstance(v, bool) and v >= 0 for v in ints):
        return None
    state_ms, buff_ms, cast_ms, trigger_ms, mana = ints
    if not (aeo["LifeDuration"] == state_ms == act["ActionDuration"]):
        return None
    return {
        "name": a["Name"],
        "mana_cost": mana,
        "max_charges": 1,
        "cooldown_ms": None,
        "cast_ms": cast_ms,
        "trigger_delay_ms": trigger_ms,
        "keep_current_target": False,
        "is_champion": True,
        "effect": {
            "kind": "deflect",
            "buff": buff,
            "time_ms": buff_ms,
            "active_ms": state_ms,
            "radius_milli": aeo["Radius"],
        },
    }


# The self-buff ability: every column the reader takes, or none (the Deflect's cosmetic columns aside).
SELF_BUFF_ABILITY_READ = {
    "Buff", "BuffTime", "CastTime", "MaxCharges", "ManaCost", "Name", "TriggerDelay", "StatsTags", "Stats",
}


def champion_self_buff(t, unit: str) -> dict | None:
    """THE BUTTON OF A CHAMPION WHOSE PRESS HANGS A BUFF ON HERSELF (15.535: the Archer Queen's ArcherQueenRapid),
    or None (the card loads as a plain troop). The ability row names a Buff and its BuffTime, a CastTime and a
    TriggerDelay, one charge, and nothing else: no OnActivationAction, no area, no tags held. Any other column gives
    None."""
    row = t["characters"].get(unit)
    name = row["Ability"] if isinstance(row, Row) and "Ability" in row.columns else None
    a = t.abilities.get(name) if isinstance(name, str) else None
    if a is None or set(a) - SELF_BUFF_ABILITY_READ - DEFLECT_ABILITY_COSMETIC:
        return None
    if a.get("MaxCharges") != 1:
        return None
    buff = norm_buff(t, a.get("Buff"))
    ints = [a.get(k) for k in ("BuffTime", "CastTime", "TriggerDelay", "ManaCost")]
    if buff is None or not all(isinstance(v, int) and not isinstance(v, bool) and v >= 0 for v in ints):
        return None
    buff_ms, cast_ms, trigger_ms, mana = ints
    if buff_ms <= 0:
        return None
    return {
        "name": a["Name"],
        "mana_cost": mana,
        "max_charges": 1,
        "cooldown_ms": None,
        "cast_ms": cast_ms,
        "trigger_delay_ms": trigger_ms,
        "keep_current_target": False,
        "is_champion": True,
        "effect": {"kind": "self_buff", "buff": buff, "time_ms": buff_ms},
    }


def champion_dash_chain(t, unit: str) -> dict | None:
    """THE BUTTON OF A CHAMPION WHOSE PRESS RUNS A DASH CHAIN (15.535: the Golden Knight's GoldenKnightChain), or
    None for a unit with no [ABILITY] or another one (it loads as a plain troop). The ability's OnActivationAction is
    an ActionGroup whose first action is an ActionRunActionListOnObjectsInShapeWithPrio (the charge's target: a
    Circle shape, a target filter, a selection mode, OncePerTarget, WaitForTarget) whose ActionOnSelfWhenTriggered is
    an ActionDashingAttackChain; the rest of the group is a pending buff (an ActionSpawn of a BuffType, its
    SpeedMultiplier carried) and the button's UI state, which is not read. The chain's own numbers are the
    character's columns: DashCount, JumpSpeed, DashDamage, DashSecondaryRange, DashLandingTime,
    DashImmuneToDamageTime, DashPushBack. The Cooldown is the ability's (15.535.29 names it only as a stats tag, so
    it is carried as the table gives it, a string or null)."""
    kind, row = None, None
    for k in ("characters", "buildings"):
        r = t[k].get(unit)
        if r is not None:
            kind, row = k, r
            break
    # A 2018 row is a plain dict with no [ABILITY] section behind it: no button.
    name = row["Ability"] if isinstance(row, Row) and "Ability" in row.columns else None
    a = t.abilities.get(name) if isinstance(name, str) else None
    if a is None or kind != "characters":
        return None
    acts = t["actions"]
    on = a.get("OnActivationAction")
    # An inline action table (another champion's) is not a chain's group.
    got = _group_leaves(acts, on) if isinstance(on, str) else None
    if got is None:
        return None
    subs, _delays = got
    first = acts.get(subs[0]) if subs else None
    if first is None or first["ClassType"] != "ActionRunActionListOnObjectsInShapeWithPrio":
        return None
    # A seeker whose trigger runs no dash chain (the Hero Giant's slap, read by `slap_effect`) is not this reader's.
    trig = first.get("ActionOnSelfWhenTriggered")
    if not isinstance(trig, str) or acts.get(trig) is None or acts.get(trig)["ClassType"] != "ActionDashingAttackChain":
        return None
    charge = _one_action(acts, subs[0], "ActionRunActionListOnObjectsInShapeWithPrio", CHAIN_CHARGE_KEYS)
    execute = acts.get(charge["ActionOnSelfWhenTriggered"])
    if execute is None or execute["ClassType"] != "ActionDashingAttackChain":
        return None
    shape = t.shapes.get(charge["Shape"])
    if shape is None or shape.get("ClassType") != "Circle" or not isinstance(shape.get("Radius"), int):
        raise SystemExit(f"champion ability {name}: the charge's shape {charge['Shape']!r} is not a circle")
    filt = t.filters.get(charge["TargetFilter"])
    if filt is None:
        raise SystemExit(f"champion ability {name}: no filter {charge['TargetFilter']!r}")
    speed_multiplier = None
    for s in subs[1:]:
        sp = acts.get(s)
        if sp["ClassType"] == "ActionSpawn" and sp["SpawnType"] == "BuffType":
            buff = t["character_buffs"].get(sp["SpawnData"])
            if buff is None:
                raise SystemExit(f"champion ability {name}: no buff {sp['SpawnData']!r}")
            speed_multiplier = buff["SpeedMultiplier"]
    return {
        "name": name,
        "mana_cost": a["ManaCost"],
        "max_charges": a.get("MaxCharges"),
        "cooldown_ms": a.get("Cooldown"),
        "cast_ms": a.get("CastTime") or 0,
        "trigger_delay_ms": a.get("TriggerDelay") or 0,
        "keep_current_target": a.get("KeepCurrentTarget") is True,
        "is_champion": True,
        "effect": {
            "kind": "dash_chain",
            "radius_milli": shape["Radius"],
            "target_filter": charge["TargetFilter"],
            "filter": {k: v for k, v in sorted(filt.items())},
            "selection": charge["TargetSelectionMode"],
            "once_per_target": charge["OncePerTarget"] is True,
            "wait_for_target": charge["WaitForTarget"] is True,
            "count": row["DashCount"],
            "speed": row["JumpSpeed"],
            "damage": row["DashDamage"],
            "secondary_range_milli": row["DashSecondaryRange"],
            "landing_time_ms": row["DashLandingTime"],
            "immune_ms": row["DashImmuneToDamageTime"],
            "pushback_milli": row["DashPushBack"],
            "pending_speed_multiplier": speed_multiplier,
        },
    }


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
    for f in UNIT_FIELDS_15535:
        if f in u:
            card[f] = u[f]
    if "action_graph" in u:
        card["action_graph"] = u["action_graph"]
    if "life_state_spawner" in u:
        card["life_state_spawner"] = u["life_state_spawner"]
    if "interval_spawner" in u:
        card["interval_spawner"] = u["interval_spawner"]
    # Only on a row that sets a Mana column (norm_unit), so every other card row is unchanged.
    if "mana" in u:
        card["mana"] = u["mana"]
    # Only on a row that sets ProjectileYOffset (norm_unit), so every other card row is unchanged.
    if "projectile_y_offset_milli" in u:
        card["projectile_y_offset_milli"] = u["projectile_y_offset_milli"]
    # Only on a row whose OverrideAttackFinishTime is true (norm_unit).
    if "override_attack_finish" in u:
        card["override_attack_finish"] = u["override_attack_finish"]
    if "attack_select" in u:
        card["attack_select"] = u["attack_select"]
    if "enchant_friends" in u:
        card["enchant_friends"] = u["enchant_friends"]
    if "transform_at_hp" in u:
        card["transform_at_hp"] = u["transform_at_hp"]
    if "parry" in u:
        card["parry"] = u["parry"]
    if "idle_invisibility" in u:
        card["idle_invisibility"] = u["idle_invisibility"]
    if "idle_buff" in u:
        card["idle_buff"] = u["idle_buff"]
    # 15.535 only and only where set (norm_unit): a card row whose own unit is a rider row carries
    # its targeting columns, as the unit row does. No card's own unit is one today.
    for f in RIDER_FIELDS:
        if f in u:
            card[f] = u[f]
    # 15.535 only and only where set (norm_unit): the card row whose own unit the Clone never copies.
    if "ignore_clone" in u:
        card["ignore_clone"] = u["ignore_clone"]
    # 15.535 only, like action_graph: the card row carries its unit's death_spawn block, so it
    # carries the flag that qualifies it (the loader reads both off the same row).
    if "death_spawn_pushback" in u:
        card["death_spawn_pushback"] = u["death_spawn_pushback"]
    # 15.535 only: the dash block that starts from an Ability or a scripted action, carried
    # beside the card's `dash` (null on that row) so the card shows what it does not run.
    if "triggered_dash" in u:
        card["triggered_dash"] = u["triggered_dash"]
    # 15.535 only: a champion whose button runs a dash chain (the Golden Knight), read whole
    # (`champion_dash_chain`). Every other champion's button is not read, and its card loads as
    # a plain troop, as before.
    chain = (
        champion_dash_chain(t, res["character"])
        or champion_deflect(t, res["character"])
        or champion_self_buff(t, res["character"])
    )
    if chain is not None:
        card["ability"] = chain
    # Only on a row that reflects (norm_unit), so every other card row is unchanged.
    if "reflected_attack" in u:
        card["reflected_attack"] = u["reflected_attack"]
    # Only on a combo row (norm_unit `combo`), so every other card row is unchanged.
    if "combo" in u:
        card["combo"] = u["combo"]
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
    if not t.vintage.is_2018:
        # 15.535 only, so the 2018 file stays byte-identical: a SummonCharactersList card's members at
        # their explicit offsets (`summon_members`; the Three Musketeers alone), and whether the
        # table marks their x as mirrored. Written only on such a card, so every other row is unchanged.
        members = summon_members(t, key, s, res)
        if members is not None:
            card["summon_members"] = members
            card["summon_offsets_x_mirrored"] = flag(s, "CharactersOffsetsXMirrored")
    if not t.vintage.is_2018:
        # 15.535 only, so the 2018 file stays byte-identical: the card row's own AreaEffectObject
        # column when that area IS the deploy of the card's unit (`deploy_area_effect`).
        card["deploy_area_effect"] = deploy_area_effect(t, s, res["character"])
        # 15.535 only, and only on a card whose AreaEffectObject puts the card's own unit down through
        # its own SpawnCharacter (`resolve_summon`; TriWizards alone), so every other row is unchanged:
        # the area's name, which card.rs reads as the whole deploy (`CardDef::deploy_spawn_area`).
        if res.get("deploy_spawn_area"):
            card["deploy_spawn_area"] = res["deploy_spawn_area"]
        # 15.535 only, and only on a card whose unit travels underground (`spawn_pathfind`: the Miner,
        # the Goblin Drill), so every other row and the 2018 file stay as they were: the spell row's two
        # placement flags. CanDeployOnEnemySide is read by card.rs with the walk (the territory,
        # placement.SPAWN_PATHFIND_TERRITORY); a tunnelling card without it is refused. TouchdownLimitedDeploy
        # is carried for the record: what it limits is established neither by the tables nor by any
        # measurement, and the engine does not read it.
        if u.get("spawn_pathfind") is not None:
            card["can_deploy_on_enemy_side"] = flag(s, "CanDeployOnEnemySide")
            card["touchdown_limited_deploy"] = flag(s, "TouchdownLimitedDeploy")
    card["deploy_projectile"] = norm_projectile(t, s["Projectile"])
    if not t.vintage.is_2018:
        # THE CARD'S KIND IS WHAT IT PUTS ON THE BOARD, not the table it is listed in. The
        # Furnace is a spells_buildings card whose SummonCharacter, Furnace_rework, is a
        # characters row that walks and shoots: a troop (measured on client 16.402: a troop on
        # every frame of both Furnaces). A row that travels underground first (SpawnPathfindSpeed:
        # the Goblin Drill, whose destination is its building's footprint) keeps its table's
        # kind. Written only where the two differ, with the table's kind kept beside it, so no
        # other row and nothing in the 2018 file changes.
        unit_kind = "building" if u["source_table"] == "buildings" else "troop"
        if unit_kind != kind and u.get("spawn_pathfind") is None:
            card["kind"] = unit_kind
            card["card_table_kind"] = kind
        # OmitFromStartingHand (the Elixir Collector; Mirror, a spell): the deal keeps the card
        # out of the first four (calibration economy.OMIT_FROM_STARTING_HAND). Written only where
        # it is set.
        if flag(s, "OmitFromStartingHand"):
            card["omit_from_starting_hand"] = True
    # The ladder is the UNIT row's Rarity (15.535: Common on every base card, so a
    # Rare card scales on the Common ladder from unified level 1; module doc).
    card["level_scaling"] = level_scaling(t, rarities, s["Rarity"], u["rarity"])
    return card


def summon_carrier(t: dict[str, Table], unit: str) -> list[tuple[str, dict]]:
    """The objects a SPELL SUMMON's damage and ladder come from, in order (spells_other
    SummonCharacter; the engine's SpellShape::Fuse / Summon). A hitpoint-less building with a
    DeathAreaEffect is a bottle (Rage's RageBottle): the child area its area makes, then the area
    itself. A character (the Heal Spirit): its projectile. The unit row is not a candidate: the
    card block below reads a projectile's or an area's columns."""
    table, rec = unit_record(t, unit)
    out: list[tuple[str, dict]] = []
    if table == "buildings" and rec["Hitpoints"] is None and rec["DeathAreaEffect"]:
        area = norm_aeo(t, rec["DeathAreaEffect"])
        if area:
            child = norm_aeo(t, area.get("spawn_area_effect_object"))
            if child:
                out.append(("area_effect_objects", child))
            out.append(("area_effect_objects", area))
    else:
        p = norm_projectile(t, rec["Projectile"])
        if p:
            out.append(("projectiles", p))
    return out


VARIANT_OPTION_READ = {"AvailableManaTrigger", "PrecastPendingTime", "SpellData"}
VARIANT_OPTION_COSMETIC = {"VariantSwitchClipName", "VariantFrameClipName"}


def variant_block(t: dict, s: dict) -> dict | None:
    """A LogicBattleSpellVariantData row (15.535.29: the Spirit Empress alone): one card whose FORM is
    chosen by the elixir available at play. Its Options in table order, each {trigger_milli (the
    AvailableManaTrigger, thousandths of an elixir), precast_pending_ms, card (the form: a card row,
    SpellData)}, and the card's UseProjectedTimeSummon and MirrorUsesRootSpell. None on every other
    row. Fail closed: Options on a row without the class, an option key this builder does not know
    (the two *ClipName keys are the card frame's art and are not exported), or a form that is no card
    row stops the build."""
    name = s["Name"]
    opts = t["spells_other"].arrays.get(name, {}).get("Options")
    if opts is None and isinstance(s["Options"], dict):
        opts = [s["Options"]]  # a one-option list lands in the record only
    if s["CustomClassType"] != "LogicBattleSpellVariantData":
        if opts:
            raise SystemExit(f"spells_other.{name}: Options without LogicBattleSpellVariantData")
        return None
    out = []
    for k, o in enumerate(opts or []):
        if not isinstance(o, dict):
            raise SystemExit(f"spells_other.{name}.Options[{k}] is not a table: {o!r}")
        extra = set(o) - VARIANT_OPTION_READ - VARIANT_OPTION_COSMETIC
        if extra:
            raise SystemExit(f"spells_other.{name}.Options[{k}]: unknown keys {sorted(extra)}")
        form = o.get("SpellData")
        if form not in t["spells_characters"].records and form not in t["spells_buildings"].records:
            raise SystemExit(f"spells_other.{name}.Options[{k}].SpellData {form!r} is no card row")
        trigger, precast = o.get("AvailableManaTrigger"), o.get("PrecastPendingTime")
        out.append({"trigger_milli": trigger, "precast_pending_ms": precast, "card": form})
    return {
        "options": out,
        "use_projected_time_summon": s["UseProjectedTimeSummon"],
        "mirror_uses_root_spell": s["MirrorUsesRootSpell"],
        "source": f"spells_other.{name}.Options ({len(out)} entries)",
    }


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
    # A SPELL THAT SUMMONS (15.535 only: Rage's bottle, the Heal Spirit): no projectile and no
    # area of its own, so the damage and the ladder come through the summoned row.
    summon = s["SummonCharacter"] if isinstance(s, Row) and not proj and not aeo and not first else None
    if summon:
        candidates.extend(summon_carrier(t, summon))
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
    if summon:
        card["spell"]["summon"] = {
            "character": summon,
            "count": s["SummonNumber"] if s["SummonNumber"] is not None else 1,
            "source": f"spells_other.{s['Name']}.SummonCharacter",
        }
    # OmitFromStartingHand (Mirror): as on a troop or building card (`summon_card`), 15.535 only
    # and only where it is set.
    if isinstance(s, Row) and flag(s, "OmitFromStartingHand"):
        card["omit_from_starting_hand"] = True
    if not t.vintage.is_2018:
        # 15.535 only, so the 2018 file stays byte-identical (its Mirror row stays refused), and
        # written only on a row that sets it: the MIRROR (spells_other Mirror, the Mirror card alone),
        # which replays its side's last play (calibration match.MIRROR_*), and the VARIANT card
        # (`variant_block`; the Spirit Empress alone), whose form the elixir at play chooses.
        if flag(s, "Mirror"):
            card["spell"]["mirror"] = True
        vb = variant_block(t, s)
        if vb is not None:
            card["spell"]["variant"] = vb
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


# THE 15.535 GLOBALS THE LOADER READS (card.rs `CardDb::globals`): named rows of the vintage's own
# globals.csv, typed -- NumberValue as an integer, BooleanValue as a boolean. Only the rows a loaded
# mechanic reads are carried: the Mirror's MIRROR_LEVEL_OFFSET (read), with its two neighbours
# MIRROR_CAP_TO_MAX_LEVEL and MIRROR_IGNORE_CHAMPIONS carried for the record (read by nothing), and the
# Clone's eleven CLONE_* rules (card.rs `clone_shape` reads every one). The engine's other globals still
# come from the embedded 2018 globals.csv (state.rs `globals_number`). A named row the file lacks, or one
# whose value is neither a number nor a boolean, stops the build.
GLOBALS_READ = (
    "MIRROR_LEVEL_OFFSET",
    "MIRROR_CAP_TO_MAX_LEVEL",
    "MIRROR_IGNORE_CHAMPIONS",
    "CLONE_LEVEL_OFFSET",
    "CLONE_DISTANCE_X",
    "CLONE_DISTANCE_Y",
    "CLONE_PRESERVE_SHIELD",
    "CLONE_CLONED_UNITS",
    "CLONE_MOVE_PARENT",
    "CLONE_DEATH_SPAWN_UNITS",
    "CLONE_DEATH_SPAWN_BUILDINGS",
    "CLONE_RESET_TARGET",
    "CLONE_RESET_CHARGE",
    "CLONE_INHERIT_CHARGE",
)


def globals_block(v: Vintage) -> dict:
    path = v.raw / "globals.csv"
    with path.open(encoding="utf-8", newline="") as fh:
        rd = csv.reader(fh)
        header = next(rd)
        next(rd, None)  # the types row
        rows = {r[0]: dict(zip(header, r, strict=False)) for r in rd if r and r[0]}
    out: dict[str, int | bool] = {}
    for n in GLOBALS_READ:
        r = rows.get(n)
        if r is None:
            raise SystemExit(f"{path.name}: no row {n}")
        num, boo = (r.get("NumberValue") or "").strip(), (r.get("BooleanValue") or "").strip().upper()
        if num and not boo:
            out[n] = int(num)
        elif boo in ("TRUE", "FALSE") and not num:
            out[n] = boo == "TRUE"
        else:
            raise SystemExit(f"{path.name}: {n} has NumberValue {num!r} and BooleanValue {boo!r}; not one typed value")
    return out


# THE EVOLVED FORMS THIS BUILD LOADS (15.535 only), each a spells_evolved row: the evolved form of the base card
# whose EvolvedSpells names it. Only these three: the engine runs their mechanics (card.rs `EvoDef`), and a form
# nobody asked for is not extracted. Written under the top-level list `evolutions`, never in `cards`.
EVOLUTIONS = (
    "Skeletons_EV1", "Cannon_EV1", "Musketeer_EV1", "AngryBarbarians_EV1", "Zap_EV1", "BattleRam_EV1",
    "InfernoDragon_EV1", "BabyDragon_EV1", "Ghost_EV1", "SkeletonArmy_EV1", "Snowball_EV1", "SkeletonBalloon_EV1",
    "Mortar_EV1", "RoyalHogs_EV1", "MinionHorde_EV1", "Tesla_EV1", "RoyalRecruits_EV1", "Wizard_EV1", "Knight_EV1",
)
# THE EVO SKELETON BARREL'S DROPS (`barrel_block`): the keys its pop action may set (read, or display only), the keys
# its health trigger may set, and the columns a drop's area may set; any other stops the build.
BARREL_POP_READ = {"ClassType", "DropBalloonAtHpList", "ContainerAeoList", "OffsetXList", "OffsetYList",
                   "TotalBalloons", "Singleton", "OverrideKamikazeDoubleContainer"}
BARREL_POP_DISPLAY = {"TransitionTime", "BalloonFlyStartFrameList", "BalloonFlyEndFrameList",
                      "BalloonPopStartFrameList", "BalloonPopEndFrameList", "UseSpecialKamikaze",
                      "SpecialKamikazeStartFrameList", "SpecialKamikazeEndFrameList", "SpecialDeployStartFrameLabel",
                      "SpecialDeployEndFrameLabel"}
BARREL_AT_HEALTH_KEYS = {"ClassType", "HealthPercentages", "Actions", "ForceStopIfTrue", "StatsTags"}
BARREL_DROP_READ = {"Radius", "LifeDuration", "Damage", "HitSpeed", "Pushback", "HitsAir", "HitsGround", "OnlyEnemies",
                    "OnLifeTimeEndAction", "OnStartingAction", "Base", "Rarity"}
BARREL_DROP_SPAWN_KEYS = {"ClassType", "SpawnType", "SpawnData", "SpawnRadius", "IsSpawnConstPriority", "Count",
                          "DeployTime", "SpawnPushback", "StatsTags"}
# THE EVO GIANT SNOWBALL'S ROLL (`capture_block`): the keys its capture and its roll may set, and the two filters it
# implements; any other stops the build.
CAPTURE_KEYS = {
    "ClassType", "CapturePriority", "CaptureRadius", "HitFrequency", "DamagePerHit", "NumberOfUnitsToCapture",
    "TargetFilter", "CaptureDragTime", "HideDistance", "PullFileName", "HideAction", "OnFirstCaptureAction",
    "ActionOnCapturedObject", "BuffDuringCapture",
    # display only
    "PullEndClipExportName", "PullStartEffect", "StretchingClipExportName",
}
CAPTURE_ROLL_KEYS = {"ClassType", "Speed", "DistanceY", "DistanceX", "Radius", "TargetFilter", "BuffOnHit", "BuffTime",
                     "StatsTags"}
CAPTURE_FILTER = "characters_no_buildings_no_deflecting_including_invisible"
CAPTURE_ROLL_FILTER = "characters_no_deflecting_including_invisible"
# Evolved rows the 15.535.29 spells_evolved.csv marks NotInUse that the client puts down all the same, each with the
# measurement that shows it (the oracle's scenes of client 15.535.29).
# The card tables an evolved row's base card may come from, and the kind each gives the form.
EVOLVED_BASE_TABLES = (("spells_characters", "troop"), ("spells_buildings", "building"), ("spells_other", "spell"))
EVOLUTIONS_PLAYED_NOT_IN_USE = {
    "AngryBarbarians_EV1": "sp-form-AngryBarbarians-evo-s0 and sp-ec-AngryBarbarians: the second play puts it down",
}
# THE ELITE BARBARIANS' SPEAR (angry_barbarian_evo.toml), read row by row: every action the unit's graph runs, with the
# value this reader implements. A row whose value differs is a mechanic nobody has read, and the build stops.
SPEAR_MIN_RANGE_VAR = "AngryBarbarian_EV1_min_range"
SPEAR_CONDITION = "!target_in_range({min}) && AngryBarbarian_EV_has_projectile > 0 && target_in_range({max})"
# THE EVO BATTLE RAM'S CHARGE ACTION (characters_evo.toml [BattleRam_EV1] OnStartChargingAction, an actions.toml row),
# read column by column: the columns pinned to the value this reader implements, the numbers it reads, and the
# effect-only columns. Any other column, or a pinned value that differs, is a mechanic nobody has read, and the build
# stops. AffectInvisible and FullPushBackCollisionCheck are pinned unmeasured: no scene has an invisible victim or a
# push into a building.
RAM_PUSH_PINNED = {
    "ClassType": "ActionDamagingPushBack",
    "PushToSide": True,
    "DistanceProportinalPush": False,
    "ContinuosPushBack": True,
    "AffectInvisible": True,
    "FullPushBackCollisionCheck": True,
    "ForceStopIfTrue": "CHARGING() == false",
    "GameObjectFilter": "passive_hit_ground_characters_not_same",
    "PushFilter": "normal_pushback_ground_characters_not_same",
}
RAM_PUSH_READ = ("ActionDelay", "PushBackRadius", "PushRadiusDirectionalOffset", "PushBackStrength", "PushBackDamage")
RAM_PUSH_COSMETIC = {"OnPushEffect", "OnPushEffectMinInterval"}
# THE EVO INFERNO DRAGON'S STAGES (characters/inferno_dragon_ev1.toml): the unit's three variables, and the rows its
# graph runs, each read against the strings this reader implements (`stages_block`).
STAGE_COUNT = "InfernoDragon_EV1_AttackCount"
STAGE_DECAY = "InfernoDragon_EV1_AttackDecayCounter"
STAGE_DECAY_TIME = "InfernoDragon_EV1_DecayTime"
# THE EVO BABY DRAGON'S WIND (characters/baby_dragon_ev1.toml): its OnAttackAction, its area, the area's shape and the
# team split, each column pinned to the value this reader implements or read as a number; anything else stops the build.
WIND_ACTION_PINNED = {
    "ClassType": "ActionSpawnResetableAeO", "Singleton": True, "StopAeoIfParentHasCombatDisabled": False,
}
WIND_ACTION_READ = ("OffsetX", "OffsetY", "StayAliveAfterParentDiesDuration")
WIND_AEO_PINNED = {
    "FollowBehaviour": "FollowParent", "StayAfterParentDies": True, "Filter": "all_characters_from_both_teams",
    "HitsAir": True, "HitsGround": True,
}
WIND_AEO_READ = ("LifeDuration", "HitSpeed")
WIND_AEO_COSMETIC = {
    "Rarity", "LoopingEffect", "ScaledEffect", "OneShotEffect", "OnStartingAction", "DamageType", "Name",
}
# THE EVO ROYAL GHOST'S PAIR (characters/ghost_ev1.toml): its pair action, the action that makes the two units, its
# damage area and the two spawn areas, each column pinned to the value this reader implements or read as a number;
# anything else stops the build. The damage area's life and pulse are pinned to the row the one hit was measured on.
GHOST_ACTION_PINNED = {"ClassType": "ActionGhostEvoAction", "SummonSpawnDelay": 0}
GHOST_ACTION_READ = ("SummonDistance", "DamageAEOSpawnDelay")
GHOST_ACTION_NAMES = ("DamageAEO", "LeftSummonAreaType", "RightSummonAreaType", "SummonActionData")
GHOST_SPAWN_PINNED = {
    "ClassType": "ActionGhostEvoSpawnSummon", "UseDeployForSummons": True, "InstantHitForSummons": False,
}
GHOST_AREA_PINNED = {"HitsAir": False, "HitsGround": True, "OnlyEnemies": True, "LifeDuration": 250, "HitSpeed": 150}
GHOST_AREA_READ = ("Radius", "Damage")
GHOST_MARK_PINNED = {"HitsAir": False, "HitsGround": False, "Damage": 0, "HitSpeed": 0}
GHOST_AREA_COSMETIC = {"Rarity", "OneShotEffect", "StatsTags", "Name", "LifeDuration", "Radius"}


def col_list(tb, name: str, col: str) -> list:
    """A column of one row as a list: the overlay's list when it wrote one, else the single value (or [])."""
    arr = tb.arrays.get(name, {}).get(col)
    if arr is not None:
        return list(arr)
    row = tb.get(name)
    if row is None or row[col] is None:
        return []
    return [row[col]]


def group_subactions(t: dict, group: str | None, what: str) -> list[tuple[str, int]]:
    """An ActionGroup's (sub action, delay ms) pairs, in order. Refused unless the row is an ActionGroup whose two
    lists agree in length."""
    acts = t["actions"]
    g = acts.get(group) if group else None
    if g is None or g["ClassType"] != "ActionGroup":
        raise SystemExit(f"{what}: {group!r} is not an ActionGroup")
    subs = col_list(acts, group, "SubActions")
    delays = col_list(acts, group, "SubActionsDelay")
    if len(subs) != len(delays) or not subs:
        raise SystemExit(f"{what}: {group} SubActions {subs} and SubActionsDelay {delays} do not pair up")
    return list(zip(subs, delays, strict=True))


def barrage_block(t: dict, unit: str) -> dict:
    """Cannon_EV1's barrage: OnStartingAction -> an ActionGroup -> one ActionCannonBarrage, whose three lists name
    the bombs (offsets in the table's half-tile unit, and the area object whose LifeDuration times the bomb), and the
    projectile each bomb drops (its radius, push and filters) with the one-hit buff it carries (its damage)."""
    acts = t["actions"]
    _, row = unit_record(t, unit)
    subs = group_subactions(t, row["OnStartingAction"], f"{unit} barrage")
    if len(subs) != 1 or subs[0][1] != 0:
        raise SystemExit(f"{unit}: expected one barrage action at delay 0, got {subs}")
    name = subs[0][0]
    a = acts.get(name)
    if a is None or a["ClassType"] != "ActionCannonBarrage":
        raise SystemExit(f"{unit}: {name} is not an ActionCannonBarrage")
    hs = col_list(acts, name, "BombHorizontalOffsets")
    habs = col_list(acts, name, "BombAbsoluteHorizontalOffsets")
    vs = col_list(acts, name, "BombVerticalOffsets")
    aeos = col_list(acts, name, "BombAreaEffectObjects")
    if not (len(hs) == len(vs) == len(aeos) == len(habs)) or hs != habs:
        raise SystemExit(f"{unit}: barrage lists do not agree ({hs}, {habs}, {vs}, {aeos})")
    bombs, shot = [], None
    for h, v, aeo in zip(habs, vs, aeos, strict=True):
        area = t["area_effect_objects"].get(aeo)
        if area is None or area["LifeDuration"] is None:
            raise SystemExit(f"{unit}: barrage area {aeo} has no LifeDuration")
        spawn = acts.get(area["OnStartingAction"])
        if spawn is None or spawn["ClassType"] != "ActionCannonProjectileSpawn":
            raise SystemExit(f"{unit}: barrage area {aeo} drops no ActionCannonProjectileSpawn")
        if shot not in (None, spawn["BombProjectile"]):
            raise SystemExit(f"{unit}: barrage bombs drop different projectiles")
        shot = spawn["BombProjectile"]
        bombs.append({"h": h, "v": v, "life_ms": area["LifeDuration"], "area": aeo})
    p = t["projectiles"].get(shot)
    b = t["character_buffs"].get(p["TargetBuff"]) if p is not None else None
    if p is None or b is None:
        raise SystemExit(f"{unit}: barrage projectile {shot} or its TargetBuff does not resolve")
    return {
        "bombs": bombs,
        "projectile": shot,
        "radius_milli": p["Radius"],
        "pushback_milli": p["Pushback"],
        "always_apply_pushback": flag(p, "AlwaysApplyPushback"),
        "hits_air": flag(p, "AoeToAir"),
        "hits_ground": flag(p, "AoeToGround"),
        "only_enemies": flag(p, "OnlyEnemies"),
        "projectile_damage": p["Damage"],
        "buff": p["TargetBuff"],
        "buff_time_ms": p["BuffTime"],
        "buff_damage_per_second": b["DamagePerSecond"],
        "buff_crown_tower_damage_per_hit": b["CrownTowerDamagePerHit"],
        "buff_hit_frequency_ms": b["HitFrequency"],
    }


def duplication_block(t: dict, unit: str) -> dict:
    """Skeleton_EV1's duplication: BuffAfterHits* (single entries) naming a buff whose Spawn* columns spawn the unit's
    own row, and the unit's GroupMaxSize."""
    tb, row = unit_record(t, unit)
    counts = col_list(t[tb], unit, "BuffAfterHitsCount")
    times = col_list(t[tb], unit, "BuffAfterHitsTime")
    names = col_list(t[tb], unit, "BuffAfterHits")
    if not (len(counts) == len(times) == len(names) == 1):
        raise SystemExit(f"{unit}: BuffAfterHits* are not single entries ({counts}, {times}, {names})")
    b = t["character_buffs"].get(names[0])
    if b is None or b["SpawnObject"] != unit:
        raise SystemExit(f"{unit}: its BuffAfterHits {names[0]} does not spawn the unit's own row")
    return {
        "hits": counts[0],
        "buff_ms": times[0],
        "buff": names[0],
        "spawn_number": b["SpawnNumber"],
        "spawn_interval_ms": b["SpawnInterval"],
        "spawn_limit": b["SpawnLimit"],
        "spawner_alive_required": bool(b["SpawnerAliveRequired"]),
        "group_max": row["GroupMaxSize"],
    }


def snipe_block(t: dict, unit: str, u: dict) -> dict:
    """Musketeer_EV1's snipe: OnStartingAction -> an ActionGroup whose ActionMusketeerSnipe holds the ammo, the
    reach and the target filter, and the unit's AttackSequenceList entry 1, the snipe shot with its CustomRange."""
    acts = t["actions"]
    _, row = unit_record(t, unit)
    subs = group_subactions(t, row["OnStartingAction"], f"{unit} snipe")
    kinds = [(n, d, (acts.get(n) or {}).get("ClassType")) for n, d in subs]
    snipes = [(n, d) for n, d, k in kinds if k == "ActionMusketeerSnipe"]
    others = [n for n, _, k in kinds if k not in ("ActionMusketeerSnipe", "ActionPlayEffect")]
    if len(snipes) != 1 or others:
        raise SystemExit(f"{unit}: expected one ActionMusketeerSnipe beside effects, got {subs}")
    name, delay = snipes[0]
    s = acts.get(name)
    seq = (u.get("list_columns") or {}).get("AttackSequenceList") or []
    if len(seq) != 2 or seq[0].get("Projectile") is None or seq[1].get("Projectile") is None:
        raise SystemExit(f"{unit}: AttackSequenceList is not the two entries a snipe needs ({seq})")
    base = norm_projectile(t, seq[0]["Projectile"])
    own = u["projectile"]
    if own is None or base is None or base["damage"] != own["damage"] or base["speed"] != own["speed"]:
        raise SystemExit(f"{unit}: AttackSequenceList entry 0 is not the row's own shot")
    filt = t.filters.get(s["SnipeTargetFilter"])
    if filt is None:
        raise SystemExit(f"{unit}: snipe filter {s['SnipeTargetFilter']!r} not in game_object_filters.toml")
    return {
        "delay_ms": delay,
        "ammo": s["AmmoCount"],
        "min_range_milli": s["SnipeMinRange"],
        "max_range_milli": s["SnipeMaxRange"],
        "side_clip_milli": s["SnipeSideClip"],
        "locked_side_clip_milli": s["LockedTargetSnipeSideClip"],
        "skip_pending_damage": bool(s["IgnorePendingDamageTargets"]),
        "filter": s["SnipeTargetFilter"],
        "filter_towers": bool(filt.get("FilterTowers")),
        "shot_range_milli": seq[1].get("CustomRange"),
        "shot": norm_projectile(t, seq[1]["Projectile"]),
    }


def overlay_evolved_rows(t: Tables, ev) -> None:
    """The [SPELL_EVOLVED.<name>] sections the characters' own files write for the rows in EVOLUTIONS, laid over their
    spells_evolved.csv rows (the table loader skips the section everywhere else, SKIP_SECTIONS): the Elite Barbarians'
    SummonCharactersList and its offsets live there alone. A section for a row outside EVOLUTIONS is not read, so no
    other row changes."""
    folder = t.vintage.raw / "characters"
    for p in sorted(folder.glob("*.toml")):
        doc = tomllib.load(p.open("rb"))
        body = doc.get("SPELL_EVOLVED")
        if not isinstance(body, dict):
            continue
        mine = {n: f for n, f in body.items() if n in EVOLUTIONS and isinstance(f, dict)}
        if mine:
            ev.overlay(p, mine, f"characters/{p.name} [SPELL_EVOLVED]")


def spear_block(t: Tables, card: dict) -> dict:
    """AngryBarbarians_EV1's spear (angry_barbarian_evo.toml), read whole or the build stops. Each member unit (the
    card's SummonCharactersList) runs the same graph:
      - OnStartingAction: the ranged state (AngryBarbarian_EV_has_projectile 1) and a check every 50 ms; while the
        variable is 1 and the target is outside `min_range` (the variable's DefaultValue) and inside `max_range` (the
        condition's literal, the AttackSequenceList entry 1's CustomRange), the attack is entry 1: the Projectile row,
        thrown from CustomProjectileStartRadius; otherwise entry 0, the melee Damage;
      - the spear's OnStartingAction: after `trail_first_ms` (SubActionsDelay) an ActionInterval of `trail_every_ms`
        spawning `area` at the spear, and ActionRunActionOnShooter: the melee state (the variable 0, entry 0), an
        ActionInterval of `cooldown_ms` (AffectedByHitSpeed) that puts the ranged state back, and an ActionResetTarget;
      - the spear's OnTargetReachedAction: `area` once more, where it arrives."""
    acts = t["actions"]
    members = [m["character"] for m in card.get("summon_members") or []] or [card["summon_character"]]
    rows = []
    for unit in members:
        _, row = unit_record(t, unit)
        u = norm_unit(t, unit, with_raw=True)
        rows.append((unit, row, u))
    unit, row, u = rows[0]
    seq = (u.get("list_columns") or {}).get("AttackSequenceList") or []
    if len(seq) != 2 or seq[0].get("Projectile") is not None or seq[1].get("Projectile") is None:
        raise SystemExit(f"{unit}: AttackSequenceList is not a melee entry and a thrown one ({seq})")
    for other, orow, ou in rows[1:]:
        other_seq = (ou.get("list_columns") or {}).get("AttackSequenceList")
        if other_seq != seq or orow["OnStartingAction"] != row["OnStartingAction"]:
            raise SystemExit(f"{other}: its attack entries or its graph are not {unit}'s")
    if seq[0].get("Damage") != row["Damage"]:
        raise SystemExit(f"{unit}: the melee entry's Damage {seq[0].get('Damage')} is not the row's {row['Damage']}")
    var = t.variables.get(SPEAR_MIN_RANGE_VAR)
    min_range = (var or {}).get("DefaultValue")
    max_range = seq[1].get("CustomRange")
    if not isinstance(min_range, int) or not isinstance(max_range, int):
        raise SystemExit(f"{unit}: the spear's ranges {min_range!r} / {max_range!r} are not integers")
    cond = SPEAR_CONDITION.format(min=SPEAR_MIN_RANGE_VAR, max=max_range)
    for n in ("AngryBarbarian_EV1_check_can_use_range", "AngryBarbarian_EV1_wait_to_activate_set_range"):
        if (acts.get(n) or {}).get("Condition") != cond:
            raise SystemExit(f"{unit}: {n}'s Condition is not {cond!r}")
    spear = norm_projectile(t, seq[1]["Projectile"])
    if spear is None or spear["name"] != row["Projectile"]:
        raise SystemExit(f"{unit}: the thrown entry's Projectile is not the row's")
    proj_row = t["projectiles"].get(spear["name"])
    start = acts.get(proj_row["OnStartingAction"])
    arr = acts.arrays.get(proj_row["OnStartingAction"], {})
    subs, delays = arr.get("SubActions") or [], arr.get("SubActionsDelay") or []
    if start is None or start["ClassType"] != "ActionGroup" or len(subs) != 2:
        raise SystemExit(f"{spear['name']}: OnStartingAction is not the trail and the shooter's action")
    trail, on_shooter = acts.get(subs[0]), acts.get(subs[1])
    shooter = on_shooter["ClassType"]
    if trail["ClassType"] != "ActionInterval" or shooter != "ActionRunActionOnShooter" or delays[1] != 0:
        raise SystemExit(f"{spear['name']}: {subs} are not an ActionInterval and an ActionRunActionOnShooter at 0")
    spawn = acts.get(trail["ActionToExecute"])
    if spawn["ClassType"] != "ActionSpawn" or spawn["SpawnType"] != "AreaEffectType":
        raise SystemExit(f"{spear['name']}: the trail does not spawn an area")
    reached = proj_row["OnTargetReachedAction"]
    same_area = isinstance(reached, dict) and reached.get("SpawnData") == spawn["SpawnData"]
    if not same_area or reached.get("SpawnType") != "AreaEffectType":
        raise SystemExit(f"{spear['name']}: OnTargetReachedAction does not spawn the trail's area ({reached!r})")
    melee = acts.arrays.get(on_shooter["ActionToExecute"], {}).get("SubActions") or []
    classes = [acts.get(n)["ClassType"] for n in melee]
    if classes != ["ActionSetVariable", "ActionSetAttackSequenceIndex", "ActionInterval", "ActionResetTarget"]:
        raise SystemExit(f"{spear['name']}: the shooter's action runs {classes}, not the melee state this reader reads")
    timer = acts.get(melee[2])
    if timer["ActionToExecute"] != "AngryBarbarian_EV1_as_ranged" or not timer["AffectedByHitSpeed"]:
        raise SystemExit(f"{spear['name']}: the cooldown does not put the ranged state back at the hit speed")
    return {
        "min_range_milli": min_range,
        "max_range_milli": max_range,
        "sight_range_milli": seq[1].get("CustomSightRange"),
        "start_radius_milli": seq[1].get("CustomProjectileStartRadius"),
        "spear": spear,
        "cooldown_ms": timer["Interval"],
        "trail_first_ms": delays[0],
        "trail_action_delay_ms": trail["ActionDelay"],
        "trail_every_ms": trail["Interval"],
        "area": spawn["SpawnData"],
    }


def ram_block(t: Tables, card: dict) -> dict:
    """BattleRam_EV1's charge (characters_evo.toml [BattleRam_EV1]), read whole or the build stops:
      - KeepChargingAfterAttack: a hit neither kills it (its row's Kamikaze is false) nor ends its charge;
      - OnStartChargingAction: an ActionDamagingPushBack, `RAM_PUSH_PINNED`, whose numbers are the `push` block;
      - its DeathSpawnCharacter, Barbarian_EV1: after every BuffAfterHitsCount-th hit it lands its BuffAfterHits for
        BuffAfterHitsTime (single entries), the `spawn_rage` block."""
    unit = card["summon_character"]
    _, row = unit_record(t, unit)
    if row["KeepChargingAfterAttack"] is not True or row["Kamikaze"] is not False or card.get("charge") is None:
        raise SystemExit(f"{unit}: not a charger that keeps its charge and lives through its hit")
    name = row["OnStartChargingAction"]
    a = t["actions"].get(name) if name else None
    if a is None:
        raise SystemExit(f"{unit}: OnStartChargingAction {name!r} is no actions row")
    unread = set(a) - set(RAM_PUSH_PINNED) - set(RAM_PUSH_READ) - RAM_PUSH_COSMETIC
    off = {k: a.get(k) for k, v in RAM_PUSH_PINNED.items() if a.get(k) != v}
    missing = [k for k in RAM_PUSH_READ if not isinstance(a.get(k), int)]
    if unread or off or missing:
        raise SystemExit(f"{unit}: {name} reads {sorted(unread)} unread, {off} off the pinned values, {missing} "
                         "missing")
    ds = card.get("death_spawn") or {}
    spawn = ds.get("character")
    if spawn is None:
        raise SystemExit(f"{unit}: no DeathSpawnCharacter")
    key, _ = unit_record(t, spawn)
    tb = t[key]
    counts = col_list(tb, spawn, "BuffAfterHitsCount")
    times = col_list(tb, spawn, "BuffAfterHitsTime")
    names = col_list(tb, spawn, "BuffAfterHits")
    if len(counts) != 1 or len(times) != 1 or len(names) != 1:
        raise SystemExit(f"{spawn}: BuffAfterHits* are not single entries ({counts}, {times}, {names})")
    buff = norm_buff(t, names[0])
    if buff is None or buff.get("death_spawn") is not None:
        raise SystemExit(f"{spawn}: its BuffAfterHits {names[0]} is no buff, or spawns (a duplication, not a rage)")
    return {
        "keep_charging": True,
        "push": {
            "delay_ms": a["ActionDelay"],
            "radius_milli": a["PushBackRadius"],
            "offset_milli": a["PushRadiusDirectionalOffset"],
            "strength_milli": a["PushBackStrength"],
            "damage": a["PushBackDamage"],
        },
        "spawn_rage": {"unit": spawn, "hits": counts[0], "time_ms": times[0], "buff": buff},
    }


def stages_block(t: Tables, card: dict) -> dict:
    """InfernoDragon_EV1's damage stages (characters/inferno_dragon_ev1.toml), read whole or the build stops:
      - OnAttackAction: each attack adds one to the count, capped at `cap` (`min(cap, count + 1)`), then restarts the
        decay counter at DecayTime (`decay_ms`); OnStartingAttackAction: an attack's start restarts it too;
      - OnStartingAction: every 50 ms the decay counter runs down 50, and at 0 the count is 0 (every branch of the
        decay) and the counter restarts; the attack entry is the first whose `below` the count is under (the last
        entry past them all); while COMBAT_DISABLED the count is 0;
      - AttackSequenceList (Manual): the entries' Damage, `damages`.
    The effect-only rows (the damage-state VFX, the fizzle, the animator layers, a 50 ms tag) are not read."""
    unit = card["summon_character"]
    _, row = unit_record(t, unit)
    u = norm_unit(t, unit, with_raw=True)
    acts = t["actions"]
    seq = (u.get("list_columns") or {}).get("AttackSequenceList") or []
    damages = [e.get("Damage") for e in seq]
    if row["AttackSequenceMode"] != "Manual" or len(damages) < 2 or not all(isinstance(d, int) for d in damages):
        raise SystemExit(f"{unit}: not a Manual AttackSequenceList of damages ({damages})")
    names = {
        "OnAttackAction": "InfernoDragon_EV1_IncrementAttackCount",
        "OnStartingAttackAction": "InfernoDragon_EV1_ResetDecayCounter",
        "OnStartingAction": "InfernoDragon_EV1_ConstantTicker",
    }
    for col, want in names.items():
        if row[col] != want:
            raise SystemExit(f"{unit}: {col} is {row[col]!r}, not {want}")

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"{unit}: {what}")

    inc = acts.get(names["OnAttackAction"])
    m = re.fullmatch(rf"min\((\d+), {STAGE_COUNT} \+ 1\)", str(inc["Value"]))
    step_ok = inc["ClassType"] == "ActionSetVariable" and inc["Variable"] == STAGE_COUNT and m is not None
    need(step_ok, "the count's step")
    need(inc["NextAction"] == names["OnStartingAttackAction"], "the count's step does not restart the decay")
    reset = acts.get(names["OnStartingAttackAction"])
    need(reset["Variable"] == STAGE_DECAY and reset["Value"] == STAGE_DECAY_TIME, "the decay's restart")
    tick = acts.get(names["OnStartingAction"])
    group = tick["ActionToExecute"]
    subs = ["InfernoDragon_EV1_UpdateDecayCounter", "InfernoDragon_EV1_UpdateAttackSequence",
            "InfernoDragon_EV1_CheckCombatDisabled"]
    need(tick["ClassType"] == "ActionInterval" and tick["Interval"] == 50, "the ticker is not every 50 ms")
    need(isinstance(group, dict) and group.get("SubActions") == subs, f"the ticker runs {group!r}")
    dec = acts.get(subs[0])
    then = dec["NextAction"]
    need(dec["Variable"] == STAGE_DECAY and dec["Value"] == f"max(0, {STAGE_DECAY} - 50)", "the decay's step")
    need(isinstance(then, dict) and then.get("ExecuteIfTrue") == f"{STAGE_DECAY} == 0", "the decay's end")
    need((then.get("SubActions") or [])[:2] == ["InfernoDragon_EV1_DoAttackDecay", names["OnStartingAttackAction"]],
         "the decay's end does not zero the count and restart")
    zero = acts.arrays.get("InfernoDragon_EV1_DoAttackDecay", {}).get("SubActions") or []
    need(len(zero) >= 1 and all(z.get("Variable") == STAGE_COUNT and z.get("Value") == "0" for z in zero),
         "a decay branch that does not zero the count")
    sel = acts.arrays.get(subs[1], {})
    conds = sel.get("PerActionConditions") or []
    below = [int(c.rsplit("<", 1)[1]) for c in conds if re.fullmatch(rf"{STAGE_COUNT} < \d+", c)]
    picks = [s.get("AttackIndex") for s in sel.get("SubActions") or []]
    need(len(below) == len(conds) == len(damages) - 1 and below == sorted(below), f"the entry thresholds {conds}")
    need(picks == list(range(len(damages))), f"the entries picked {picks}")
    off = acts.get(subs[2])
    need(off["ExecuteIfTrue"] == "COMBAT_DISABLED" and off["Variable"] == STAGE_COUNT and off["Value"] == "0",
         "COMBAT_DISABLED does not zero the count")
    decay = (t.variables.get(STAGE_DECAY_TIME) or {}).get("DefaultValue")
    need(isinstance(decay, int) and decay > 0, f"DecayTime {decay!r}")
    return {"damages": damages, "below": below, "cap": int(m.group(1)), "decay_ms": decay}


def wind_block(t: Tables, card: dict) -> dict:
    """BabyDragon_EV1's wind (characters/baby_dragon_ev1.toml), read whole or the build stops:
      - OnAttackAction: an ActionSpawnResetableAeO, one at a time (Singleton: each attack makes it anew), `offset` ahead
        of the dragon, which outlives the dragon by StayAliveAfterParentDiesDuration (`stay_ms`);
      - the area: FollowParent, `life_ms`, a pulse every `hit_speed_ms`, on every character of both sides (air and
        ground) inside its Rectangle shape (`width` x `height`), no damage;
      - its OnHitAction: ActionFilterByEnemy, an enemy takes `enemy` for its SpawnTime, a unit of the dragon's side
        `ally` for its SpawnTime; the dragon's IgnoreBuff names the ally buff.
    The area's OnStartingAction plays effects only (an effect and a timed end effect)."""
    unit = card["summon_character"]
    _, row = unit_record(t, unit)
    acts = t["actions"]

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"{unit}: {what}")

    a = acts.get(row["OnAttackAction"] or "")
    need(a is not None, f"OnAttackAction {row['OnAttackAction']!r} is no action")
    off = {k: a.get(k) for k, v in WIND_ACTION_PINNED.items() if a.get(k) != v}
    unread = set(a) - set(WIND_ACTION_PINNED) - set(WIND_ACTION_READ) - {"Aeo"}
    need(not off and not unread and all(isinstance(a.get(k), int) for k in WIND_ACTION_READ),
         f"the wind action reads {off} off, {sorted(unread)} unread")
    aeo = t["area_effect_objects"].get(a["Aeo"])
    need(aeo is not None, f"the wind area {a['Aeo']!r} is no area row")
    set_cols = {k for k in aeo if aeo[k] is not None}
    off = {k: aeo.get(k) for k, v in WIND_AEO_PINNED.items() if aeo.get(k) != v}
    unread = set_cols - set(WIND_AEO_PINNED) - set(WIND_AEO_READ) - WIND_AEO_COSMETIC - {"Shape", "OnHitAction"}
    need(not off and not unread and all(isinstance(aeo.get(k), int) for k in WIND_AEO_READ),
         f"the wind area reads {off} off, {sorted(unread)} unread")
    shape = t.shapes.get(aeo["Shape"]) or {}
    need(shape.get("ClassType") == "Rectangle" and isinstance(shape.get("Width"), int)
         and isinstance(shape.get("Height"), int), f"the wind's shape {shape!r}")
    split = acts.get(aeo["OnHitAction"] or "")
    need(split is not None and split["ClassType"] == "ActionFilterByEnemy", "the wind's OnHitAction is no team split")

    def buff_of(name: str) -> tuple[dict, int]:
        g = acts.get(name or "")
        need(g is not None and g["ClassType"] == "ActionSpawn" and g["SpawnType"] == "BuffType"
             and isinstance(g["SpawnTime"], int), f"{name} does not hang a buff for a time")
        b = norm_buff(t, g["SpawnData"])
        need(b is not None, f"{g['SpawnData']} is no buff")
        return b, g["SpawnTime"]

    enemy, enemy_ms = buff_of(split["IsEnemyAction"])
    ally, ally_ms = buff_of(split["IsSameTeamAction"])
    ignores = col_list(t["characters"], unit, "IgnoreBuff")
    need(ignores == [ally["name"]], f"IgnoreBuff {ignores} is not the ally buff alone")
    return {
        "offset_x_milli": a["OffsetX"],
        "offset_y_milli": a["OffsetY"],
        "stay_ms": a["StayAliveAfterParentDiesDuration"],
        "life_ms": aeo["LifeDuration"],
        "hit_speed_ms": aeo["HitSpeed"],
        "width_milli": shape["Width"],
        "height_milli": shape["Height"],
        "enemy": enemy,
        "enemy_ms": enemy_ms,
        "ally": ally,
        "ally_ms": ally_ms,
    }


def ghost_block(t: Tables, card: dict) -> dict:
    """Ghost_EV1's pair (characters/ghost_ev1.toml), read whole or the build stops:
      - OnStartingAction: an ActionGroup of the pair action and an effect, both at delay 0;
      - the pair action (ActionGhostEvoAction): its two units `distance_milli` either side of what the ghost hits,
        made with no delay (SummonSpawnDelay 0) by its ActionGhostEvoSpawnSummon (`left`, `right`: deployed,
        UseDeployForSummons, and no hit on arrival, InstantHitForSummons false), and its damage area `area_delay_ms`
        after them. Its two spawn areas hit nothing: a mark on the ground for the eye;
      - the damage area: ground units of the other side within `area_radius_milli`, `area_damage` (level-scaled);
      - each unit a Ghost row of its own (Name Ghost) that starts visible (StartWithBuffWhenNotAttacking false) and
        runs no graph of consequence;
      - ClonedVersion (`clone`): a Ghost row whose OnStartingAction hangs the plain Invisibility for good, 50 ms in."""
    unit = card["summon_character"]
    _, row = unit_record(t, unit)
    acts = t["actions"]
    aeos = t["area_effect_objects"]

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"{unit}: {what}")

    subs = group_subactions(t, row["OnStartingAction"], f"{unit} pair")
    classes = [(acts.get(n) or {}).get("ClassType") for n, _ in subs]
    need(sorted(classes) == ["ActionGhostEvoAction", "ActionPlayEffect"] and all(d == 0 for _, d in subs),
         f"OnStartingAction runs {subs} ({classes}), not the pair and an effect at 0")
    name = subs[classes.index("ActionGhostEvoAction")][0]
    a = acts.get(name)
    off = {k: a.get(k) for k, v in GHOST_ACTION_PINNED.items() if a.get(k) != v}
    unread = {k for k in a if a[k] is not None} - set(GHOST_ACTION_PINNED) - set(GHOST_ACTION_READ)
    unread -= set(GHOST_ACTION_NAMES)
    need(not off and not unread and all(isinstance(a.get(k), int) for k in GHOST_ACTION_READ),
         f"the pair action reads {off} off, {sorted(unread)} unread")
    sp = acts.get(a["SummonActionData"] or "")
    need(sp is not None, f"SummonActionData {a['SummonActionData']!r} is no action")
    off = {k: sp.get(k) for k, v in GHOST_SPAWN_PINNED.items() if sp.get(k) != v}
    unread = {k for k in sp if sp[k] is not None} - set(GHOST_SPAWN_PINNED) - {"LeftSummonType", "RightSummonType"}
    need(not off and not unread, f"the pair's spawn reads {off} off, {sorted(unread)} unread")

    def area(n: str, pinned: dict, read: tuple) -> dict:
        r = aeos.get(n or "")
        need(r is not None, f"{n!r} is no area row")
        off = {k: r.get(k) for k, v in pinned.items() if r.get(k) != v}
        unread = {k for k in r if r[k] is not None} - set(pinned) - set(read) - GHOST_AREA_COSMETIC
        need(not off and not unread and all(isinstance(r.get(k), int) and r[k] > 0 for k in read),
             f"area {n} reads {off} off, {sorted(unread)} unread")
        return r

    hit = area(a["DamageAEO"], GHOST_AREA_PINNED, GHOST_AREA_READ)
    for side in ("LeftSummonAreaType", "RightSummonAreaType"):
        area(a[side], GHOST_MARK_PINNED, ())
    for side in ("LeftSummonType", "RightSummonType"):
        _, u = unit_record(t, sp[side])
        need(u["Name"] == "Ghost" and u["StartWithBuffWhenNotAttacking"] is False,
             f"{sp[side]} is not a Ghost row that starts visible")
    clone = row["ClonedVersion"]
    _, cr = unit_record(t, clone)
    csubs = group_subactions(t, cr["OnStartingAction"], f"{unit} clone {clone}")
    inv = acts.get(csubs[0][0]) if len(csubs) == 1 else None
    need(cr["Name"] == "Ghost" and inv is not None and inv["ClassType"] == "ActionSpawn"
         and inv["SpawnType"] == "BuffType" and inv["SpawnData"] == "Invisibility" and inv["SpawnTime"] == 999999,
         f"ClonedVersion {clone} is not a Ghost row that hangs the Invisibility for good")
    return {
        "distance_milli": a["SummonDistance"],
        "left": sp["LeftSummonType"],
        "right": sp["RightSummonType"],
        "area_delay_ms": a["DamageAEOSpawnDelay"],
        "area_radius_milli": hit["Radius"],
        "area_damage": hit["Damage"],
        "clone": clone,
    }


def army_block(t: Tables, card: dict, s: dict) -> dict:
    """SkeletonArmy_EV1's General and Spectrals (characters/skeleton_army_ev1.toml, spells_evolved.toml), read whole or
    the build stops:
      - the play's SummonCharactersList: ONE General (`general`), at SummonCharactersOffsetsX / Y (`general_offset_*`,
        millitiles, the owner's frame as the Three Musketeers' members), beside the row's own soldiers;
      - each soldier's OnDeathAction: while its group holds the General (ActionRunActionIfUnitGroupContains over a
        filter naming the General alone) one `spectral` where it died (ActionSpawnToLocation, AddToSourceGroup); else
        an effect;
      - the General's OnDeathAction: every Spectral of its group killed (ActionRunOnMatchingUnitsInGroup over a
        filter naming the Spectral alone, ActionKill);
      - the Spectral: a soldier row with NO_DAMAGE (it takes none) whose OnStartingAction hangs an Invisible buff for
        good (SpawnTime 999999) beside an effect; its own OnDeathAction is the soldier's."""
    unit = card["summon_character"]
    acts = t["actions"]

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"{unit}: {what}")

    # The play's list lives in csv_logic/spells_evolved.toml's flat [SkeletonArmy_EV1] section, which the table loader
    # does not lay over the csv: read here, every key pinned or read.
    sec = tomllib.load((t.vintage.raw / "spells_evolved.toml").open("rb")).get(s["Name"]) or {}
    need(set(sec) <= {"SummonCharactersList", "SummonCharactersOffsetsX", "SummonCharactersOffsetsY", "Stats"},
         f"spells_evolved.toml [{s['Name']}] sets {sorted(sec)}")
    lst = sec.get("SummonCharactersList")
    xs, ys = sec.get("SummonCharactersOffsetsX"), sec.get("SummonCharactersOffsetsY")
    need(isinstance(lst, list) and len(lst) == 1 and xs is not None and ys is not None and len(xs) == len(ys) == 1
         and all(isinstance(v, int) for v in [*xs, *ys]), f"the play's summon list {lst} {xs} {ys}")
    members = [{"character": lst[0], "offset_x_milli": xs[0], "offset_y_milli": ys[0]}]
    general = members[0]["character"]

    def names(filt: str) -> list:
        f = t.filters.get(filt) or {}
        need(f.get("MatchTeamOwn") is True and f.get("MatchTeamEnemy") is False
             and f.get("MatchTypeCharacters") is True, f"filter {filt} {f}")
        return list(f.get("IncludeCharactersWithData") or [])

    _, srow = unit_record(t, unit)
    check = acts.get(srow["OnDeathAction"] or "")
    need(check is not None and check["ClassType"] == "ActionRunActionIfUnitGroupContains"
         and names(check["ObjectFilter"]) == [general] and _cosmetic_action(acts, check["ActionIfNoMatch"]),
         "the soldier's death does not check its group for the General")
    keys = {"ClassType", "SpawnType", "SpawnData", "AddToSourceGroup"}
    spawn = _one_action(acts, check["Action"], "ActionSpawnToLocation", keys)
    need(spawn["SpawnType"] == "CharacterType" and spawn["AddToSourceGroup"] is True, "the soldier's death spawn")
    spectral = spawn["SpawnData"]
    _, grow = unit_record(t, general)
    kill = _one_action(acts, grow["OnDeathAction"], "ActionRunOnMatchingUnitsInGroup",
                       {"ClassType", "ObjectFilter", "ActionToRun"})
    need(names(kill["ObjectFilter"]) == [spectral], "the General's death does not name the Spectral")
    _one_action(acts, kill["ActionToRun"], "ActionKill", {"ClassType"})
    _, prow = unit_record(t, spectral)
    need(prow["GameTagsToSet"] == "NO_DAMAGE" and prow["OnDeathAction"] == srow["OnDeathAction"],
         "the Spectral is not a NO_DAMAGE soldier")
    subs = group_subactions(t, prow["OnStartingAction"], f"{spectral} start")
    buffs = [acts.get(n) for n, d in subs if (acts.get(n) or {}).get("ClassType") == "ActionSpawn"]
    need(len(subs) == 2 and all(d == 0 for _, d in subs) and len(buffs) == 1
         and all(_cosmetic_action(acts, n) for n, _ in subs if acts.get(n)["ClassType"] != "ActionSpawn"),
         f"the Spectral's start {subs}")
    b = buffs[0]
    brow = t["character_buffs"].get(b["SpawnData"] or "")
    need(b["SpawnType"] == "BuffType" and b["SpawnTime"] == 999999 and brow is not None and flag(brow, "Invisible"),
         "the Spectral's start does not hang an Invisible buff for good")
    return {
        "general": general,
        "general_offset_x_milli": members[0]["offset_x_milli"],
        "general_offset_y_milli": members[0]["offset_y_milli"],
        "spectral": spectral,
        "spectral_buff": norm_buff(t, b["SpawnData"]),
        "spectral_buff_ms": b["SpawnTime"],
    }


# THE EVO ROYAL HOGS' FALL (`fall_block`): the columns a grounded row may set beside its flying row's (the two hooks
# cleared, the display, and JumpHeight, which its flying row inherits at the same value), and the display-only keys of
# the fall's group and of the air-to-ground action.
FALL_GROUNDED_SET = {
    "Base", "ClonedVersion", "VisualActions", "HideHealthbar", "FileName", "BlueExportName", "RedExportName", "Scale",
    "DeathEffect", "MoveEffect", "DamageEffect", "AttackStartEffect", "OnStartingAction", "OnAttackAction",
    "JumpHeight",
}


# A FORM WHOSE MECHANIC IS DATA THE CARD RECORD READS (`data_only_block`): per form, the columns its own row may set
# beside display ones, and the card blocks they make (each must be on the record).
DATA_ONLY_DISPLAY = {"Base", "DeathEffect", "SpawnEffect", "CustomSpawnFilter", "ClonedVersion"}
DATA_ONLY_KNIGHT = ({"BuffWhenNotAttacking", "BuffWhenNotAttackingTime", "BuffWhenNotAttackingUseAttackRange"}, ["idle_buff"])


def data_only_block(t: Tables, card: dict, spec: tuple[set, list]) -> dict:
    """A FORM WHOSE MECHANIC IS DATA (the Evo Knight's BuffWhenNotAttacking: `idle_buff_block` reads it for every row),
    read whole or the build stops: its unit row sets nothing but display columns and the spec's columns, names no
    action, and its record carries each of the spec's blocks (`reads`)."""
    cols, reads = spec
    unit = card["summon_character"]
    table, _ = unit_record(t, unit)
    own = t[table].set_fields.get(unit, set())
    extra = {c for c in own - cols - DATA_ONLY_DISPLAY if not COSMETIC.search(c) and not c.startswith("Prestige")}
    if extra:
        raise SystemExit(f"{card['name']}: its row sets {sorted(extra)}, which is not data this form's card reads")
    missing = [r for r in reads if not card.get(r)]
    if missing or card.get("action_graph"):
        raise SystemExit(f"{card['name']}: its record lacks {missing} or names an action")
    return {"reads": reads}


def shield_blast_block(t: Tables, card: dict) -> dict:
    """THE EVO WIZARD'S BLAST (spells_evolved Wizard_EV1; characters_evo Wizard_EV1), read whole or the build stops. The
    unit row sets ShieldHitpoints; its ShieldLostAction is a group of one ActionSpawn, at delay 0, of an AreaEffectType
    on the Wizard (`area`: the blast, a row of `area_effect_objects`); its OnStartingAction is a group of one ActionSpawn
    of a buff that only shows while the shield holds (`shown_buff`: no column but AliveIfTrue HAS_SHIELD()) and an
    effect."""
    acts = t["actions"]
    unit = card["summon_character"]
    _, urow = unit_record(t, unit)

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"Wizard_EV1: {what}")

    need(isinstance(urow["ShieldHitpoints"], int) and urow["ShieldHitpoints"] > 0, "no shield")
    got = _group_leaves(acts, urow["ShieldLostAction"]) if isinstance(urow["ShieldLostAction"], str) else None
    need(got is not None and len(got[0]) == 1 and got[1] == [0], "ShieldLostAction is not one step at delay 0")
    sp = _one_action(acts, got[0][0], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "AbortIfInstigatorDies", "ParentGOAsSource"})
    need(sp["SpawnType"] == "AreaEffectType" and sp["ParentGOAsSource"] is True, "the blast is not an area on the Wizard")
    area = sp["SpawnData"]
    need(t["area_effect_objects"].get(area) is not None, f"no area {area}")
    # The start group lists one step and two delays ([100, 100]): its SubActions are read directly.
    g = acts.get(urow["OnStartingAction"]) if isinstance(urow["OnStartingAction"], str) else None
    need(g is not None and g["ClassType"] == "ActionGroup", "OnStartingAction is not a group")
    ssubs = _action_list(acts, urow["OnStartingAction"], "SubActions")
    need(len(ssubs) == 1, "OnStartingAction is not one step")
    vfx = _one_action(acts, ssubs[0], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "AbortIfInstigatorDies", "SpawnTime", "NextAction"})
    shown = t["character_buffs"].get(vfx["SpawnData"])
    need(vfx["SpawnType"] == "BuffType" and shown is not None
         and t["character_buffs"].set_fields.get(vfx["SpawnData"], set()) <= {"Rarity", "AliveIfTrue"}
         and shown["AliveIfTrue"] == "HAS_SHIELD()", "the start's buff does more than show")
    need(vfx["NextAction"] is None or _cosmetic_action(acts, vfx["NextAction"]) or _cosmetic_inline(vfx["NextAction"]),
         "the start's buff's NextAction")
    return {"area": area, "shown_buff": vfx["SpawnData"]}


def charge_after_shield_block(t: Tables, card: dict) -> dict:
    """THE EVO ROYAL RECRUITS' CHARGE (spells_evolved RoyalRecruits_EV1; characters_evo Recruit_EV1), read whole or the
    build stops. The unit row sets ChargeSpeedMultiplier and DamageSpecial and no ChargeRange; its ShieldLostAction is an
    ActionSpawn on itself of a buff for good (SpawnTime past any battle) whose one column is OverrideChargeRange: the
    charge's range from the shield's loss (`range_raw`). The card's charge block is completed with it (its `charge`),
    and the block says the run-up waits for the shield's loss."""
    acts = t["actions"]
    unit = card["summon_character"]
    _, urow = unit_record(t, unit)

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"RoyalRecruits_EV1: {what}")

    need(urow["ChargeRange"] in (None, 0) and isinstance(urow["ChargeSpeedMultiplier"], int)
         and isinstance(urow["DamageSpecial"], int), "the row's charge columns")
    sp = acts.get(urow["ShieldLostAction"]) if isinstance(urow["ShieldLostAction"], str) else None
    need(sp is not None and sp["ClassType"] == "ActionSpawn" and sp["SpawnType"] == "BuffType"
         and isinstance(sp["SpawnTime"], int) and sp["SpawnTime"] >= 99999, "ShieldLostAction is not a buff for good")
    need(_present(sp) <= {"ClassType", "SpawnType", "SpawnData", "SpawnTime", "ParentGOAsSource"}, "ShieldLostAction's keys")
    bt = t["character_buffs"]
    b = bt.get(sp["SpawnData"])
    need(b is not None and bt.set_fields.get(sp["SpawnData"], set()) <= {"Rarity", "OverrideChargeRange"}
         and isinstance(b["OverrideChargeRange"], int) and b["OverrideChargeRange"] > 0, f"the buff {sp['SpawnData']}")
    card["charge"] = {
        "charge_range_raw": b["OverrideChargeRange"],
        "damage_special": urow["DamageSpecial"],
        "charge_speed_multiplier_percent": urow["ChargeSpeedMultiplier"],
    }
    return {"range_raw": b["OverrideChargeRange"], "buff": sp["SpawnData"]}


# THE EVO TESLA'S RING (`ring_block`): the columns its area may set beside the cosmetic ones.
RING_AREA_READ = {
    "Rarity", "OnlyEnemies", "HitsGround", "HitsAir", "Radius", "MaxRadius", "LifeDuration", "HitSpeed", "Damage",
    "OnHitAction", "OneHitPerTarget",
}


def ring_block(t: Tables, card: dict) -> dict:
    """THE EVO TESLA'S RING (spells_evolved Tesla_EV1; buildings_evo Tesla_EV1), read whole or the build stops. The
    building's OnStartingAction and OnAppearAction name one action: an ActionSpawn of an AreaEffectType on the building
    (ParentGOAsSource), then an effect (`on_start`, `on_appear`); its OnDisappearAction only plays an effect. The area:
    enemies only, air and ground as set, no damage, OneHitPerTarget, HitSpeed (`hit_speed_ms`), a Radius growing to its
    MaxRadius over its LifeDuration (`min_radius_milli`, `max_radius_milli`, `life_ms`); its OnHitAction an ActionSpawn of
    a BuffType for its SpawnTime (`buff`, `buff_ms`), not stopped by the building's death. The buff: a full stop
    (Speed, HitSpeed and SpawnSpeed multipliers -100) and ONE hit of its DamagePerSecond as it lands (HitFrequency -1;
    `damage`, `crown_hit`: CrownTowerDamagePerHit), written apart from the stop (`stop`)."""
    acts = t["actions"]
    unit = card["summon_character"]
    _, urow = unit_record(t, unit)

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"Tesla_EV1: {what}")

    start, appear = urow["OnStartingAction"], urow["OnAppearAction"]
    need(isinstance(appear, str) and start in (None, appear), "OnStartingAction and OnAppearAction name one action")
    gone = urow["OnDisappearAction"]
    need(gone is None or _cosmetic_action(acts, gone), "OnDisappearAction plays more than an effect")
    sp = acts.get(appear)
    need(sp is not None and sp["ClassType"] == "ActionSpawn" and sp["SpawnType"] == "AreaEffectType"
         and sp["ParentGOAsSource"] is True, f"{appear} is not an area on the building")
    need(_present(sp) - {"ClassType", "SpawnType", "SpawnData", "ParentGOAsSource", "NextAction"} == set(), f"{appear}'s keys")
    nxt = sp["NextAction"]
    need(nxt is None or _cosmetic_action(acts, nxt) or _cosmetic_inline(nxt), f"{appear}'s NextAction")
    tb = t["area_effect_objects"]
    name = sp["SpawnData"]
    r = tb.get(name)
    need(r is not None, f"no area {name}")
    unread = tb.set_fields.get(name, set()) - RING_AREA_READ - HERO_AREA_COSMETIC
    need(not unread, f"area {name} sets {sorted(unread)}")
    need(r["OnlyEnemies"] is True and r["OneHitPerTarget"] is True and not r["Damage"], f"area {name} is not a one-hit ring")
    for col in ("Radius", "MaxRadius", "LifeDuration", "HitSpeed"):
        need(isinstance(r[col], int) and r[col] > 0, f"area {name}'s {col} {r[col]!r}")
    need(r["MaxRadius"] > r["Radius"], f"area {name} does not grow")
    hit = acts.get(r["OnHitAction"]) if isinstance(r["OnHitAction"], str) else None
    need(hit is not None and hit["ClassType"] == "ActionSpawn" and hit["SpawnType"] == "BuffType"
         and isinstance(hit["SpawnTime"], int) and hit["SpawnTime"] > 0, f"area {name}'s OnHitAction")
    need(_present(hit) - {"ClassType", "SpawnType", "SpawnData", "SpawnTime", "AbortIfInstigatorDies"} == set()
         and hit["AbortIfInstigatorDies"] is not True, f"{r['OnHitAction']}'s keys")
    buff = norm_buff(t, hit["SpawnData"])
    brow = t["character_buffs"].get(hit["SpawnData"])
    need(buff is not None and brow["HitFrequency"] == -1 and isinstance(buff["damage_per_second"], int)
         and buff["damage_per_second"] > 0, f"buff {hit['SpawnData']} is not one hit of its DamagePerSecond")
    need(all(buff[k] == -100 for k in ("speed_multiplier_raw", "hit_speed_multiplier_raw", "spawn_speed_multiplier_raw")),
         f"buff {hit['SpawnData']} is not a full stop")
    stop = dict(buff)
    for k in ("damage_per_second", "hit_frequency_ms", "crown_tower_damage_per_hit"):
        if k in stop:
            stop[k] = None
    return {
        "area": name,
        "on_start": start == appear,
        "on_appear": True,
        "min_radius_milli": r["Radius"],
        "max_radius_milli": r["MaxRadius"],
        "life_ms": r["LifeDuration"],
        "hit_speed_ms": r["HitSpeed"],
        "hits_air": r["HitsAir"] is True,
        "hits_ground": r["HitsGround"] is True,
        "damage": buff["damage_per_second"],
        "crown_hit": buff.get("crown_tower_damage_per_hit"),
        "stop": stop,
        "buff_ms": hit["SpawnTime"],
    }


def first_hit_block(t: Tables, card: dict) -> dict:
    """THE EVO MINION HORDE'S GHOST (spells_evolved MinionHorde_EV1; characters/minion_horde_ev1.toml), read whole or the
    build stops. The unit's OnDamageTakenAction (OnDamageTakenActionInstigatorAsSelf) is an ActionGroup run once (its
    ExecuteIfTrue `<var> == 0`, and an ActionSetVariable of <var> to 1 at delay 0) of an ActionSpawn of a BuffType at
    delay 0 (`buff`, for its SpawnTime: `time_ms`) and effects. The buff hides the unit (Invisible) and keeps every hit
    off it (NO_DAMAGE); its OnRemoveAction only puts a display buff back and plays an effect. The unit's OnStartingAction
    only plays an effect."""
    acts = t["actions"]
    unit = card["summon_character"]
    _, urow = unit_record(t, unit)

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"MinionHorde_EV1: {what}")

    start = acts.get(urow["OnStartingAction"]) if isinstance(urow["OnStartingAction"], str) else None
    need(start is None or start["ClassType"] == "ActionPlayEffect", "OnStartingAction plays more than an effect")
    need(urow["OnDamageTakenActionInstigatorAsSelf"] is True, "OnDamageTakenActionInstigatorAsSelf")
    grp = urow["OnDamageTakenAction"]
    g = acts.get(grp) if isinstance(grp, str) else None
    need(g is not None and g["ClassType"] == "ActionGroup", "OnDamageTakenAction is not a group")
    m = re.fullmatch(r"(\w+) == 0", str(g["ExecuteIfTrue"]))
    need(m is not None and m.group(1) in t.variables, f"the group's once-only test {g['ExecuteIfTrue']}")
    var = m.group(1)
    subs, delays = _action_list(acts, grp, "SubActions"), _action_list(acts, grp, "SubActionsDelay")
    need(len(subs) == len(delays), "the group's delays")
    buff_steps = [(s, d) for s, d in zip(subs, delays, strict=True) if acts.get(s)["ClassType"] == "ActionSpawn"]
    sets = [(s, d) for s, d in zip(subs, delays, strict=True) if acts.get(s)["ClassType"] == "ActionSetVariable"]
    need(len(buff_steps) == 1 and buff_steps[0][1] == 0 and len(sets) == 1 and sets[0][1] == 0, "the group's steps")
    for s in subs:
        need(acts.get(s)["ClassType"] in ("ActionSpawn", "ActionSetVariable", "ActionPlayEffect"), f"step {s}")
    sv = acts.get(sets[0][0])
    need(sv["Variable"] == var and sv["Value"] == "1", "the once-only flag")
    sp = acts.get(buff_steps[0][0])
    need(sp["SpawnType"] == "BuffType" and isinstance(sp["SpawnTime"], int) and sp["SpawnTime"] > 0, "the ghost buff")
    brow = t["character_buffs"].get(sp["SpawnData"])
    need(brow is not None and flag(brow, "Invisible"), "the ghost buff is not Invisible")
    rem = brow["OnRemoveAction"]
    if rem:
        rg = _group_leaves(acts, rem)
        need(rg is not None, "the ghost's OnRemoveAction")
        for s in rg[0]:
            a = acts.get(s)
            if a["ClassType"] == "ActionSpawn":
                db = t["character_buffs"].get(a["SpawnData"])
                shown = {k for k, v in db.items() if v is not None} if db is not None else {"?"}
                need(a["SpawnType"] == "BuffType" and shown <= {"Rarity", "ShadowAlpha"}, f"remove step {s}")
            else:
                need(a["ClassType"] == "ActionPlayEffect", f"remove step {s}")
    buff = norm_buff(t, sp["SpawnData"])
    need(buff.get("no_damage") is True, "the ghost buff does not set NO_DAMAGE")
    return {"buff": buff, "time_ms": sp["SpawnTime"]}


def fall_block(t: Tables, card: dict) -> dict:
    """THE EVO ROYAL HOGS' FALL (spells_evolved RoyalHogs_EV1; characters/royal_hog_ev1.toml), read whole or the build
    stops. The unit (RoyalHog_EV1) flies (its FlyingHeight); it falls once:
      - OnStartingAction: an ActionRunActionAtHealth of one HealthPercentages line (`at_hp_pct`) running one group, the
        same group as OnAttackAction (`on_attack`);
      - the group: an ActionGroup, every step at delay 0 (a short SubActionsDelay list pads with 0), run only while its
        ExecuteIfTrue tag is unset: one ActionAirToGround and animations or effects;
      - the ActionAirToGround: TransitionDuration (`transition_ms`), a TotalDuration past any battle, Singleton, the
        tag set (the fall runs once), CloneTriggersLandingActions false, and ActionOnGround a group of an
        ActionChangeGameObjectData to the grounded row (`grounded`, delay 0), an ActionSpawn of an area whose source is
        the hog (`landing_area`, delay 0) and an ActionResetPath (`reset_path_ms`, its delay);
      - the grounded row: the flying row but for the two hooks cleared and display columns (FALL_GROUNDED_SET): the
        same stats on the ground. Its inherited FlyingHeight is the flying row's; the fall, not the row, grounds it."""
    acts = t["actions"]
    unit = card["summon_character"]
    _, urow = unit_record(t, unit)

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"RoyalHogs_EV1: {what}")

    hp = acts.get(urow["OnStartingAction"]) if isinstance(urow["OnStartingAction"], str) else None
    need(hp is not None and hp["ClassType"] == "ActionRunActionAtHealth", f"{unit}'s OnStartingAction")
    need(_present(hp) <= {"ClassType", "HealthPercentages", "Actions"}, "the health trigger's keys")
    pcts = _action_list(acts, urow["OnStartingAction"], "HealthPercentages")
    runs = _action_list(acts, urow["OnStartingAction"], "Actions")
    need(len(pcts) == 1 and len(runs) == 1 and isinstance(pcts[0], int) and 0 < pcts[0] < 100, "one health line")
    grp = runs[0]
    need(urow["OnAttackAction"] == grp, "OnAttackAction runs another group")
    g = acts.get(grp)
    group_keys = {"ClassType", "SubActions", "SubActionsDelay", "ExecuteIfTrue"}
    need(g is not None and g["ClassType"] == "ActionGroup" and _present(g) <= group_keys, f"group {grp}")
    subs = _action_list(acts, grp, "SubActions")
    delays = _action_list(acts, grp, "SubActionsDelay")
    need(len(delays) <= len(subs) and all(d == 0 for d in delays), f"{grp}'s delays")
    fall = [s for s in subs if acts.get(s)["ClassType"] == "ActionAirToGround"]
    need(len(fall) == 1, f"{grp}: one ActionAirToGround")
    for s in subs:
        need(s in fall or acts.get(s)["ClassType"] in ("ActionRunForcedAnimationOnce", "ActionPlayEffect"), f"step {s}")
    a = acts.get(fall[0])
    fall_keys = {"ClassType", "TransitionDuration", "TotalDuration", "Singleton", "ActionOnGround", "GameTagsToSet",
                 "CloneTriggersLandingActions"}
    need(_present(a) == fall_keys, f"{fall[0]}'s keys")
    tag = a["GameTagsToSet"]
    need(g["ExecuteIfTrue"] == f"!{tag}" and a["Singleton"] is True and a["CloneTriggersLandingActions"] is False,
         "the fall's once-only tag")
    need(isinstance(a["TransitionDuration"], int) and a["TransitionDuration"] > 0 and a["TotalDuration"] >= 999999,
         "the fall's clock")
    landed = a["ActionOnGround"]
    ls, ld = _action_list(acts, landed, "SubActions"), _action_list(acts, landed, "SubActionsDelay")
    need(acts.get(landed) is not None and acts.get(landed)["ClassType"] == "ActionGroup" and len(ls) == 3
         and len(ld) == 3, f"landing group {landed}")
    by = {acts.get(s)["ClassType"]: (s, d) for s, d in zip(ls, ld, strict=True)}
    need(sorted(by) == ["ActionChangeGameObjectData", "ActionResetPath", "ActionSpawn"], f"{landed}'s steps")
    ch, sp, rp = (acts.get(by[k][0]) for k in ("ActionChangeGameObjectData", "ActionSpawn", "ActionResetPath"))
    need(_present(ch) == {"ClassType", "NewCharacterData"} and by["ActionChangeGameObjectData"][1] == 0,
         "the row change")
    need(_present(sp) == {"ClassType", "SpawnType", "SpawnData", "ParentGOAsSource"}
         and sp["SpawnType"] == "AreaEffectType" and sp["ParentGOAsSource"] is True and by["ActionSpawn"][1] == 0,
         "the landing area")
    need(_present(rp) == {"ClassType"}, "the path reset")
    grounded = ch["NewCharacterData"]
    _, grow = unit_record(t, grounded)
    extra = t["characters"].set_fields.get(grounded, set()) - FALL_GROUNDED_SET
    need(not extra and grow["Base"] == f"CHARACTER.{unit}" and not grow["OnStartingAction"]
         and not grow["OnAttackAction"] and grow["JumpHeight"] == urow["JumpHeight"],
         f"grounded row {grounded} ({sorted(extra)})")
    need(t["area_effect_objects"].get(sp["SpawnData"]) is not None, f"no area {sp['SpawnData']}")
    return {
        "at_hp_pct": pcts[0],
        "on_attack": True,
        "transition_ms": a["TransitionDuration"],
        "grounded": norm_unit(t, grounded, with_raw=True),
        "landing_area": sp["SpawnData"],
        "reset_path_ms": by["ActionResetPath"][1],
    }


def shot_spawn_block(t: Tables, card: dict) -> dict:
    """Mortar_EV1's shot (projectiles_evo.toml MortarProjectile_EV1), read whole or the build stops: its SpawnCharacter
    (`unit`), one of it (SpawnCharacterCount blank or 1, its level the card's: no SpawnCharacterLevelIndex), deploying
    SpawnCharacterDeployTime (`deploy_ms`), put down where the shot lands; the shot releases nothing else (no
    SpawnProjectile, no SpawnAreaEffectObject, no action graph)."""
    p = card["projectile"]
    unit = card["summon_character"]
    if p is None or not p["spawn_character"]:
        raise SystemExit(f"{unit}: its shot puts no unit down")
    if p["spawn_character_count"] not in (None, 1) or p["spawn_character_level_index"] is not None:
        raise SystemExit(f"{unit}: its shot's unit count or level index")
    if p["spawn_projectile"] is not None or p["spawn_area_effect_object"] is not None or p.get("action_graph"):
        raise SystemExit(f"{unit}: its shot releases more than its unit")
    if not isinstance(p["spawn_character_deploy_time_ms"], int):
        raise SystemExit(f"{unit}: its shot's unit has no deploy time")
    if unit_record(t, p["spawn_character"])[1] is None:
        raise SystemExit(f"{unit}: its shot's unit {p['spawn_character']} is no row")
    return {"unit": p["spawn_character"], "deploy_ms": p["spawn_character_deploy_time_ms"]}


def barrel_block(t: Tables, card: dict) -> dict:
    """SkeletonBalloon_EV1's two drops (characters/skeleton_balloon_ev1.toml), read whole or the build stops:
      - the row's OnStartingAction: an ActionGroup, at 0, of an ActionRunActionAtHealth (HealthPercentages [p]:
        `at_hp_pct`) that runs the pop, and the pop itself (display: the balloons' animation); its OnDeathAction the
        pop; no DeathSpawnCharacter of its own;
      - the pop (ActionSkeletonBarrelPopBalloon): DropBalloonAtHpList [p], two containers (ContainerAeoList: the one the
        health trigger drops, then the one the death drops), each at OffsetXList / OffsetYList from the barrel (the
        owner's frame, native: `extra_offset`, `death_offset`); TotalBalloons 2, Singleton, no kamikaze override; the
        rest display only;
      - each container's area: the base barrel's container (SkeletonContainerNew) to the column, but for its Damage: its
        LifeDuration the container's DeployTime (its fuse), its Radius and Pushback the container's DeathDamageRadius
        and DeathPushBack, one hit (HitSpeed = LifeDuration), and its OnLifeTimeEndAction an ActionSpawn of the
        container's death spawn (character, Count, SpawnRadius, DeployTime, SpawnPushback). Each is written as the
        container's own record with the area's name and Damage (`extra`, `death`): the engine's death bomb with a death
        spawn."""
    acts = t["actions"]
    unit = card["summon_character"]
    _, row = unit_record(t, unit)

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"{unit}: {what}")

    need(not row["DeathSpawnCharacter"], "a DeathSpawnCharacter beside its drops")
    group = group_subactions(t, row["OnStartingAction"], f"{unit} start")
    need([acts.get(n)["ClassType"] for n, _ in group] == ["ActionRunActionAtHealth", "ActionSkeletonBarrelPopBalloon"]
         and all(d == 0 for _, d in group), f"its start group {group}")
    trig = _one_action(acts, group[0][0], "ActionRunActionAtHealth", BARREL_AT_HEALTH_KEYS)
    pct = col_list(acts, group[0][0], "HealthPercentages")
    need(len(pct) == 1 and col_list(acts, group[0][0], "Actions") == [group[1][0]]
         and row["OnDeathAction"] == group[1][0], "the health trigger and the death do not both run the pop")
    need(trig["ForceStopIfTrue"] in (None, "is_kamikazing"), f"the trigger's stop {trig['ForceStopIfTrue']!r}")
    pop = acts.get(group[1][0])
    unread = _present(pop) - BARREL_POP_READ - BARREL_POP_DISPLAY
    need(not unread, f"the pop sets {sorted(unread)}")
    aeos = col_list(acts, group[1][0], "ContainerAeoList")
    xs, ys = col_list(acts, group[1][0], "OffsetXList"), col_list(acts, group[1][0], "OffsetYList")
    need(col_list(acts, group[1][0], "DropBalloonAtHpList") == pct and len(aeos) == 2 and len(xs) == 2 and len(ys) == 2
         and pop["TotalBalloons"] == 2 and not pop["OverrideKamikazeDoubleContainer"], "the pop's lists")
    base = norm_unit(t, "SkeletonContainerNew")
    at = t["area_effect_objects"]
    drops = []
    for name in aeos:
        a = at.get(name)
        need(a is not None, f"no area {name}")
        own = at.set_fields.get(name, set())
        base_row = at.set_fields.get(a["Base"].split(".")[-1], set()) if isinstance(a["Base"], str) else set()
        unread = (own | base_row) - BARREL_DROP_READ - {"ScaledEffect", "StatsTags", "HitEffect"}
        need(not unread, f"{name} sets {sorted(unread)}")
        need(a["LifeDuration"] == base["deploy_time_ms"] and a["HitSpeed"] == a["LifeDuration"]
             and a["Radius"] == base["death_damage_radius_milli"] and a["Pushback"] == base["death_pushback_milli"]
             and flag(a, "OnlyEnemies") is True, f"{name} is not the container's fuse, radius and push")
        need(_cosmetic_action(acts, a["OnStartingAction"]) or acts.get(a["OnStartingAction"]) is not None,
             f"{name} start")
        sp = _one_action(acts, a["OnLifeTimeEndAction"], "ActionSpawn", BARREL_DROP_SPAWN_KEYS)
        ds = base["death_spawn"]
        need(sp["SpawnType"] == "CharacterType" and sp["SpawnData"] == ds["character"] and sp["Count"] == ds["count"]
             and sp["SpawnRadius"] == ds["radius_milli"] and sp["DeployTime"] == ds["deploy_time_ms"]
             and sp["SpawnPushback"] is True and base["death_spawn_pushback"] is True,
             f"{name}'s spawn is not the container's death spawn")
        drops.append({**base, "name": name, "death_damage": a["Damage"]})
    return {
        "at_hp_pct": pct[0],
        "extra": drops[0],
        "extra_offset_x_milli": xs[0],
        "extra_offset_y_milli": ys[0],
        "death": drops[1],
        "death_offset_x_milli": xs[1],
        "death_offset_y_milli": ys[1],
    }


def capture_block(t: Tables, card: dict) -> dict:
    """THE EVO GIANT SNOWBALL (spells_evolved Snowball_EV1; characters/snowball_ev1.toml), read whole or the build
    stops:
      - the flight (the card's projectile, SnowballSpell_EV1): no damage, no push, no buff; it lands a rolling
        projectile (SpawnProjectile, SpawnChain 1, SpawnAxisY) whose MinDistance is the roll's DistanceY;
      - the roll (SnowballSpell_EV1_Rolling): its Damage and crown-tower percent are the hit; its OnStartingAction an
        ActionGroup of the capture and the ActionRollingProjectile, both at 0;
      - the ActionRollingProjectile: `roll_speed` (Speed, native a tick) along the owner's forward for `roll_len_milli`
        (DistanceY; DistanceX 0) through CAPTURE_ROLL_FILTER, its Radius the capture's CaptureRadius, hanging BuffOnHit
        for BuffTime (`hit_buff`): a slow whose DamagePerSecond is written as none (its HitFrequency -1 never pulses:
        measured on client 15.535.29, one hit of the Damage per unit and no other);
      - the ActionCaptureCharacter: every character but a building (CAPTURE_FILTER) within CaptureRadius, up to
        NumberOfUnitsToCapture, dragged over CaptureDragTime (`drag_ms`) and joined to the roll within HideDistance
        (`hide_distance_milli`), held by BuffDuringCapture (`hold_buff`: -100 speed, hit speed and spawn speed), and at
        the roll's end given its ActionOnCapturedObject's buff (an ActionRunActionOnInstigatorDeath of an ActionSpawn of
        a BuffType: `release_buff` for `release_ms`); DamagePerHit 0 and a HitFrequency past the roll's life (one
        capture). OnFirstCaptureAction swaps the projectile's display row and HideAction the unit's: display only."""
    acts = t["actions"]
    pt = t["projectiles"]

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"Snowball_EV1: {what}")

    fl = card["projectile"]
    need(fl is not None and fl["damage"] == 0 and fl["pushback_milli"] == 0 and fl["target_buff"] is None,
         "the flight is not a bare carrier")
    rp = fl["spawn_projectile"]
    need(rp is not None and rp["spawn_projectile"] is None, "the flight lands no single rolling projectile")
    flight_row = pt.get(fl["name"] + "_EV1") or pt.get("SnowballSpell_EV1")
    need(flight_row is not None and flight_row["SpawnChain"] == 1 and flight_row["SpawnAxisY"] is True,
         "the flight's SpawnChain / SpawnAxisY")
    roll_row = pt.get(flight_row["SpawnProjectile"])
    need(roll_row is not None, f"no rolling row {flight_row['SpawnProjectile']}")
    group = group_subactions(t, roll_row["OnStartingAction"], "Snowball_EV1 roll")
    classes = [acts.get(n)["ClassType"] for n, _ in group]
    need(classes == ["ActionCaptureCharacter", "ActionRollingProjectile"] and all(d == 0 for _, d in group),
         f"the roll's group runs {classes}")
    cap = _one_action(acts, group[0][0], "ActionCaptureCharacter", CAPTURE_KEYS)
    roll = _one_action(acts, group[1][0], "ActionRollingProjectile", CAPTURE_ROLL_KEYS)
    need(cap["TargetFilter"] == CAPTURE_FILTER and roll["TargetFilter"] == CAPTURE_ROLL_FILTER, "the filters")
    need(roll["DistanceX"] == 0 and roll["DistanceY"] == flight_row["MinDistance"]
         and roll["Radius"] == cap["CaptureRadius"] and roll["Radius"] == rp["radius_milli"],
         "the roll's distances and radii")
    need(cap["DamagePerHit"] == 0 and isinstance(cap["HitFrequency"], int)
         and cap["HitFrequency"] > roll["DistanceY"] * 1000 // max(roll["Speed"] * 20, 1),
         "the capture hits more than once")
    hold = norm_buff(t, cap["BuffDuringCapture"])
    need(hold is not None and hold["speed_multiplier_raw"] == -100 and hold["hit_speed_multiplier_raw"] == -100
         and hold["spawn_speed_multiplier_raw"] == -100, "the capture's hold is not a full stop")
    run = _one_action(acts, cap["ActionOnCapturedObject"], "ActionRunActionOnInstigatorDeath",
                      {"ClassType", "ActionToRun", "AbortIfInstigatorDies"})
    need(run["AbortIfInstigatorDies"] is False, "the release's AbortIfInstigatorDies")
    sp = _one_action(acts, run["ActionToRun"], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "SpawnTime"})
    release = norm_buff(t, sp["SpawnData"])
    need(sp["SpawnType"] == "BuffType" and release is not None and isinstance(sp["SpawnTime"], int),
         "the release's buff")
    need(_one_action(acts, cap["OnFirstCaptureAction"], "ActionChangeGameObjectData",
                     {"ClassType", "NewProjectileData"}) is not None, "the first capture's swap")
    need(acts.get(cap["HideAction"])["ClassType"] == "ActionHide", "the capture's HideAction")
    hit_buff = norm_buff(t, roll["BuffOnHit"])
    brow = t["character_buffs"].get(roll["BuffOnHit"])
    need(hit_buff is not None and brow["HitFrequency"] == -1, f"the roll's buff {roll['BuffOnHit']}")
    hit_buff = {**hit_buff, "damage_per_second": None, "hit_frequency_ms": None, "crown_tower_damage_per_hit": None}
    return {
        "roll_speed": roll["Speed"],
        "roll_len_milli": roll["DistanceY"],
        "radius_milli": roll["Radius"],
        "damage": rp["damage"],
        "crown_tower_damage_percent": rp["crown_tower_damage_percent"],
        "hits_air": rp["aoe_to_air"] is True,
        "hits_ground": rp["aoe_to_ground"] is True,
        "hit_buff": hit_buff,
        "hit_buff_ms": roll["BuffTime"],
        "hold_buff": hold,
        "release_buff": release,
        "release_ms": sp["SpawnTime"],
        "drag_ms": cap["CaptureDragTime"],
        "hide_distance_milli": cap["HideDistance"],
        "max_units": cap["NumberOfUnitsToCapture"],
    }


def hero_file_links(v: Vintage) -> dict[str, list]:
    """Each base card's EvolvedSpells as the hero form files set it ([SPELL_CHARACTER.<base>] or [SPELL_BUILDING.<base>]
    in characters/hero_form/*_spell.toml), by base name."""
    out: dict[str, list] = {}
    for p in sorted((v.raw / "characters" / "hero_form").glob("*_spell.toml")):
        doc = tomllib.loads(p.read_text(encoding="utf-8"))
        for section in ("SPELL_CHARACTER", "SPELL_BUILDING"):
            for base, row in (doc.get(section) or {}).items():
                if isinstance(row, dict) and isinstance(row.get("EvolvedSpells"), list):
                    out.setdefault(base, []).extend(row["EvolvedSpells"])
    return out


def evolution_records(t: Tables, rarities: dict) -> list[dict]:
    """The `evolutions` list (15.535 only): one card record per EVOLUTIONS row, built as its base card's record is
    (`summon_card`), with `form_of` naming the base card and the block of the mechanic the form runs."""
    out = []
    ev = t["spells_evolved"]
    overlay_evolved_rows(t, ev)
    for name in EVOLUTIONS:
        s = ev.get(name)
        if s is None or (s["NotInUse"] and name not in EVOLUTIONS_PLAYED_NOT_IN_USE):
            raise SystemExit(f"spells_evolved.{name}: absent or NotInUse")
        bases = [
            (key, kind, b)
            for key, kind in EVOLVED_BASE_TABLES
            for b in t[key].records.values()
            if b["EvolvedSpells"] == name and not b["NotInUse"]
        ]
        # A base row whose EvolvedSpells lists this form only in a hero form's file (the Knight's ["Knight_EV1",
        # "Knight_hero"], in knight_hero_spell.toml, which this pass does not lay): that row.
        if not bases:
            linked = hero_file_links(t.vintage)
            bases = [
                (key, kind, b)
                for key, kind in EVOLVED_BASE_TABLES
                for b in t[key].records.values()
                if name in linked.get(b["Name"], []) and not b["NotInUse"]
            ]
        if len(bases) != 1:
            raise SystemExit(f"spells_evolved.{name}: {len(bases)} base cards name it in EvolvedSpells")
        _, kind, b = bases[0]
        if kind == "spell":
            # AN EVOLVED SPELL (the Evo Zap): its row is a spell row, built as its base's (`spell_card`); its mechanic
            # is its area's own actions (card.rs reads them), so it carries no block of its own.
            card = spell_card(t, rarities, s)
            card["form_of"] = b["Name"]
            card["spells_evolved_row"] = list(ev.records).index(name)
            card["evo_cycles"] = s["DarkElixirCost"]
            if name == "Snowball_EV1":
                card["evo_capture"] = capture_block(t, card)
            out.append(card)
            continue
        card = summon_card(t, rarities, kind, "spells_evolved", s)
        # The form's kind is its base card's: Cannon_EV1 is an [EXT] of CHARACTER.Cannon, filed under characters,
        # with IsBuilding inherited true.
        _, urow = unit_record(t, card["summon_character"])
        card["kind"] = "building" if urow["IsBuilding"] else kind
        card.pop("card_table_kind", None)
        card["form_of"] = b["Name"]
        card["spells_evolved_row"] = list(ev.records).index(name)
        unit = card["summon_character"]
        u = norm_unit(t, unit, with_raw=True)
        # The evolved cycle (spells_evolved DarkElixirCost): the base card's plays before each evolved one. Measured
        # on client 15.535.29 over the oracle's 42 sp-ec-* runs, 42 of 42.
        card["evo_cycles"] = s["DarkElixirCost"]
        if name == "Cannon_EV1":
            card["evo_barrage"] = barrage_block(t, unit)
        elif name == "Skeletons_EV1":
            card["evo_duplication"] = duplication_block(t, unit)
            card["is_a_group"] = bool(s["IsAGroup"])
        elif name == "Musketeer_EV1":
            card["evo_snipe"] = snipe_block(t, unit, u)
        elif name == "BattleRam_EV1":
            card["evo_ram"] = ram_block(t, card)
        elif name == "InfernoDragon_EV1":
            card["evo_stages"] = stages_block(t, card)
        elif name == "BabyDragon_EV1":
            card["evo_wind"] = wind_block(t, card)
        elif name == "Ghost_EV1":
            card["evo_ghost"] = ghost_block(t, card)
        elif name == "SkeletonArmy_EV1":
            card["evo_army"] = army_block(t, card, s)
        elif name == "SkeletonBalloon_EV1":
            card["evo_barrel"] = barrel_block(t, card)
        elif name == "Mortar_EV1":
            card["evo_shot_spawn"] = shot_spawn_block(t, card)
        elif name == "RoyalHogs_EV1":
            card["evo_fall"] = fall_block(t, card)
        elif name == "MinionHorde_EV1":
            card["evo_first_hit"] = first_hit_block(t, card)
        elif name == "Tesla_EV1":
            card["evo_ring"] = ring_block(t, card)
        elif name == "RoyalRecruits_EV1":
            card["evo_charge_after_shield"] = charge_after_shield_block(t, card)
        elif name == "Wizard_EV1":
            card["evo_shield_blast"] = shield_blast_block(t, card)
        elif name == "Knight_EV1":
            card["evo_data_only"] = data_only_block(t, card, DATA_ONLY_KNIGHT)
        elif name == "AngryBarbarians_EV1":
            card["evo_spear"] = spear_block(t, card)
            # SummonSpawnDelay (its [SPELL_EVOLVED] section's; the base card's row says SummonDeployDelay): member k
            # leaves its deploy k x 100 ms after the first. Measured on client 15.535.29
            # (sp-form-AngryBarbarians-evo-s0: both members first seen on t752, their deploys ending on t771 and t773).
            if card["summon_deploy_delay_ms"] is None:
                card["summon_deploy_delay_ms"] = s["SummonSpawnDelay"]
        card["cloned_version"] = urow["ClonedVersion"]
        out.append(card)
    return out


# --- hero forms (15.535 only) ---------------------------------------------------------------------
#
# A HERO FORM is a card of its own: characters/hero_form/<stem>_spell.toml [SPELL_HERO.<form>] (CardForm
# "HeroForm", the card row) and <stem>.toml (the [EXT.*] unit that extends the base character, its
# [ABILITY.*] and every object the ability makes). spells_hero_form.csv is a stale copy of the base rows
# and is not read. The build writes EXACTLY the forms named here, under the top-level `hero_forms` list,
# never in `cards`. Each record is a card record (`summon_card`) plus `form_of` (the base card), `ability`
# (the button: `ability_block`) and `tables` (the unit and area rows it names that the top-level maps do
# not carry, so those maps and every record in them stay as they are).
HERO_FORMS = {
    "Musketeer_hero": ("Musketeer", "musketeer_hero"),
    "IceGolemite_hero": ("IceGolemite", "ice_golemite_hero"),
    "Berserker_hero": ("Berserker", "berserker_hero"),
    "Balloon_hero": ("Balloon", "balloon_hero"),
    "Valkyrie_hero": ("Valkyrie", "valkyrie_hero"),
    "Wizard_hero": ("Wizard", "wizard_hero"),
    "MiniPekka_hero": ("MiniPekka", "mini_pekka_hero"),
    "Knight_hero": ("Knight", "knight_hero"),
    "MegaMinion_hero": ("MegaMinion", "mega_minion_hero"),
    "Giant_hero": ("Giant", "giant_hero"),
}
# The keys of an [ABILITY.*] row: the ones read, and the ones only the UI reads. Any other key stops the build.
ABILITY_READ_KEYS = {
    "ManaCost", "MaxCharges", "Cooldown", "CastTime", "TriggerDelay", "IsChampion", "KeepCurrentTarget",
    "OnActivationAction",
}
ABILITY_UI_KEYS = {
    "TID", "TID_INFO", "IconSWF", "IconExportName", "KeepIconEvenWhenOutOfCharges", "HideChargesTextField",
    "DeployedEffect", "DeployedClip", "PopoverIconFileName", "PopoverIconExportName", "Stats", "StatsTags",
    "OutOfChargesTID",
}
# The columns a hero ability's area may set: the ones `hero_area` reads, and the display ones.
HERO_AREA_READ = {
    "Rarity", "LifeDuration", "HitSpeed", "HitSpeedOffset", "Damage", "CrownTowerDamagePercent", "Radius",
    "HitsGround", "HitsAir", "AffectsHidden", "Pushback", "FollowBehaviour", "StayAfterParentDies", "Shape",
    "DamageType", "Filter", "OnHitAction", "OnStartingAction", "OnLifeTimeEndAction", "Base",
}
# The keys the two spawns of a `spawn_ahead` effect may set (the inner one also IgnoreEffects), and the columns its
# placeholder building may set.
SPAWN_AHEAD_KEYS = {
    "ClassType", "SpawnType", "SpawnData", "ValidatePlacementAsBuilding", "RelativeX", "RelativeY", "UseDeploy",
}
PLACEHOLDER_SET = {"Rarity", "IsBuilding", "DeployTime", "DeployDelay", "LifeTime", "OnStartingAction"}
HERO_AREA_COSMETIC = {"ParentLoopingEffectToSelf", "ScaledEffect", "StatsTags", "OneShotEffect"}
# THE HERO BALLOON'S THROW (`throw_effect`): the keys its seeker may set, the filter and the resolver order it
# implements, the ramp's two strings (its variable's name in place of {v}), the projectile's pinned columns and the
# landing area's; any other stops the build. THROW_SPEED_BLOCKS: the speed table's length, 3 ticks a block.
THROW_SEEKER_KEYS = {
    "ClassType", "GameTagsToSet", "OncePerTarget", "TargetSelectionMode", "TargetFilter", "Actions", "Delays", "Shape",
    "PauseTags", "AbortIfInstigatorDies", "ActionOnSelfWhenTriggered", "ParentAsInstigatorForSelfActions",
    "OnFinishedAction",
}
THROW_FILTER = "default_targets_no_towers_no_flying"
THROW_STRATEGIES = ["RESOLVER_STRATEGY_CLOSEST_TARGET", "RESOLVER_STRATEGY_HIGHEST_CURR_HP"]
THROW_RAMP_VALUE = "{v} + 2"
THROW_RAMP_SPEED = "logX10000(max(5, {v} - 1)) / 80"
THROW_PROJECTILE_PINNED = {"Speed": 1, "Homing": True, "Damage": 0, "Gravity": 0, "DeflectBehaviour": "NoDeflect"}
THROW_PROJECTILE_COSMETIC = {"Rarity", "HitEffect", "TrailEffect", "PrefabAsset", "Name"}
THROW_LANDING_PINNED = {"OnlyEnemies": True, "HitsGround": True, "HitsAir": False, "LifeDuration": 50, "HitSpeed": 50}
THROW_SPEED_BLOCKS = 40
# THE HERO VALKYRIE'S SPIN (`spin_chain_effect`): the keys its seeker, its chain and its blow's interval may set, the
# filter both its picks read, and the columns its blow's area may set (read, or display only); any other stops the
# build.
SPIN_SEEKER_KEYS = {
    "ClassType", "GameTagsToSet", "OncePerTarget", "WaitForTarget", "TargetSelectionMode", "TargetFilter", "Actions",
    "Delays", "ActionOnSelfWhenTriggered", "Shape", "PauseTags", "AbortIfInstigatorDies",
}
SPIN_CHAIN_KEYS = {
    "ClassType", "TargetResolver", "GameTagsToSet", "ChainCount", "ChainCompleteIfTrue", "OnChainBegan",
    "OnFinishedAction", "ChainPhaseBuff", "PerformAttackOnReach", "ResetTargetAfterReach", "PauseIfAttackSpeedZero",
    "StopMovementWhenAtTarget",
}
SPIN_INTERVAL_KEYS = {"ClassType", "Interval", "ActionToExecute", "GameTagsToSet", "ForceStopIfTrue", "StatsTags"}
SPIN_FILTER = "GroundCharacterTargetsNoInactive"
SPIN_AREA_READ = {"LifeDuration", "Radius", "HitsGround", "HitsAir", "OnlyEnemies", "Damage", "CrownTowerDamagePercent"}
SPIN_AREA_COSMETIC = {"Rarity", "ScaledEffect", "ExtraHitEffect", "StatsTags"}
# THE HERO WIZARD'S LIFT (`ground_to_air_effect`): the keys its lift and its shot's area spawns may set, and the columns
# its air form may set of its own; any other stops the build.
GROUND_TO_AIR_KEYS = {
    "ClassType", "FlyingHeight", "TransitionDuration", "TotalDuration", "ResetPathInAir", "ResetPathWhenBackToGround",
    "ActionOnFlyHeightReached", "ActionOnStartDescending", "GameTagsToSetOnToAirState", "GameTagsToSetOnToGroundState",
}
REACH_SPAWN_KEYS = {"ClassType", "SpawnType", "SpawnData", "OffsetY"}
AIR_FORM_COLUMNS = {"Base", "Projectile", "ClonedVersion", "FlyingHeight", "PrefabAsset"}


def hero_files(v: Vintage, stem: str) -> list[Path]:
    """A hero form's two files: its card row, then its unit and ability."""
    folder = v.raw / "characters" / "hero_form"
    return [folder / f"{stem}_spell.toml", folder / f"{stem}.toml"]


def overlay_hero_files(t: Tables, v: Vintage, forms: dict) -> None:
    """Lay the hero files of `forms` over `t` (the hero pass's own load): [SPELL_HERO.*] into a new
    `spells_hero` table, [ABILITY.*] into `t.abilities`, the base row's EvolvedSpells into `t.hero_links`
    (recorded, never applied: the base row stays as it is), [STATS.*] dropped, [SHAPE.*], [VARIABLE.*],
    [TARGET_RESOLVER.*] and [DAMAGE_TYPE.*] by name, every other section routed as the base pass routes it. A
    name a table already holds stops the build."""
    t["spells_hero"] = OverlayTable("spells_hero", None)
    for _form, (_base, stem) in forms.items():
        for p in hero_files(v, stem):
            label = f"characters/hero_form/{p.name}"
            for section, body in tomllib.load(p.open("rb")).items():
                if not isinstance(body, dict):
                    raise SystemExit(f"{label}: [{section}] is not a table")
                if section in ("SPELL_CHARACTER", "SPELL_BUILDING", "SPELL_OTHER"):
                    for n, f in body.items():
                        t.hero_links[n] = list(f.get("EvolvedSpells") or [])
                    continue
                if section == "ABILITY":
                    t.abilities.update(body)
                    continue
                if section == "STATS":
                    continue
                # [CARD_GROUP.*] (the Hero Wizard's): the cards an ActionActivateOnCardDeploy listens for, which only
                # the button's display reads.
                if section == "CARD_GROUP":
                    continue
                if section == "SHAPE":
                    t.shapes.update(body)
                    continue
                if section == "VARIABLE":
                    t.variables.update(body)
                    continue
                if section == "TARGET_RESOLVER":
                    t.resolvers.update(body)
                    continue
                if section == "DAMAGE_TYPE":
                    t.damage_types.update(body)
                if section == "SPELL_HERO":
                    route = {n: "spells_hero" for n in body}
                elif section == "EXT":
                    route = {}
                    for n, f in body.items():
                        base = f.get("Base", "")
                        key = SECTION_TABLE.get(base.split(".")[0]) if "." in base else None
                        if key is None:
                            raise SystemExit(f"{label}: [EXT.{n}] Base {base!r} names no table")
                        route[n] = key
                else:
                    key = SECTION_TABLE.get(section)
                    if key is None:
                        raise SystemExit(f"{label}: unknown section [{section}]")
                    route = {n: key for n in body}
                for n, key in route.items():
                    if n in t[key].records:
                        raise SystemExit(f"{label}: [{section}.{n}] names a row {key} already holds")
                    t[key].overlay(p, {n: body[n]}, f"{label} [{section}]")


def _one_action(acts, name, cls: str, keys: set[str]) -> dict:
    """Action `name`, which must be of class `cls` and set no key outside `keys`; else the build stops."""
    a = acts.get(name) if isinstance(name, str) else None
    if a is None or a["ClassType"] != cls:
        raise SystemExit(f"hero ability: action {name!r} is not an {cls}")
    extra = _present(a) - keys
    if extra:
        raise SystemExit(f"hero ability: action {name} sets {sorted(extra)}, which this reader does not read")
    return a


def hero_area(h: Tables, name: str) -> dict:
    """One area a hero ability makes (an ActionSpawn of an AreaEffectType whose source is the hero), read
    whole or the build stops: its clock (LifeDuration, HitSpeed, HitSpeedOffset), its hit (Damage and whether
    its damage type scales with level, the crown-tower percent, the radius of its Shape, air and ground, the
    enemy side from its Filter, Pushback), whether it rides on the hero and stays after it, the buff its
    OnHitAction hangs (an ActionSelect whose options are one buff: `hero_select_buff`) and the area its
    OnLifeTimeEndAction makes. An OnStartingAction must only play an effect."""
    tb = h["area_effect_objects"]
    r = tb.get(name)
    if r is None:
        raise SystemExit(f"hero ability: no area {name}")
    unread = tb.set_fields.get(name, set()) - HERO_AREA_READ - HERO_AREA_COSMETIC
    if unread:
        raise SystemExit(f"hero ability: area {name} sets {sorted(unread)}, which this reader does not read")
    acts = h["actions"]
    if r["OnStartingAction"] is not None:
        start = acts.get(r["OnStartingAction"])
        # ActionInterval of an effect (the Ice Golem storm's blizzard): cosmetic.
        cosmetic = start is not None and (
            _cosmetic_action(acts, r["OnStartingAction"])
            or (start["ClassType"] == "ActionInterval" and _cosmetic_action(acts, start["ActionToExecute"]))
        )
        if not cosmetic:
            raise SystemExit(f"hero ability: area {name}'s OnStartingAction {r['OnStartingAction']} is not an effect")
    shape = h.shapes.get(r["Shape"]) if isinstance(r["Shape"], str) else None
    if shape is None or shape.get("ClassType") != "Circle" or not isinstance(shape.get("Radius"), int):
        raise SystemExit(f"hero ability: area {name}'s Shape {r['Shape']!r} is not a circle")
    if r["Radius"] is not None and r["Radius"] != shape["Radius"]:
        raise SystemExit(f"hero ability: area {name}'s Radius {r['Radius']} is not its Shape's {shape['Radius']}")
    dt = h.damage_types.get(r["DamageType"]) if isinstance(r["DamageType"], str) else None
    if r["DamageType"] is not None and dt is None:
        raise SystemExit(f"hero ability: area {name}'s DamageType {r['DamageType']} is not a row")
    filt = filter_block(h, r["Filter"]) if r["Filter"] is not None else None
    if r["Filter"] is not None and filt is None:
        raise SystemExit(f"hero ability: area {name}'s Filter {r['Filter']} does not read")
    buff, buff_ms = hero_select_buff(h, r["OnHitAction"]) if r["OnHitAction"] is not None else (None, None)
    end = None
    if r["OnLifeTimeEndAction"] is not None:
        keys = {"ClassType", "SpawnType", "SpawnData", "ParentGOAsSource"}
        a = _one_action(acts, r["OnLifeTimeEndAction"], "ActionSpawn", keys)
        if a["SpawnType"] != "AreaEffectType" or a["ParentGOAsSource"] is not True:
            raise SystemExit(f"hero ability: area {name}'s OnLifeTimeEndAction does not make an area on the hero")
        end = hero_area(h, a["SpawnData"])
    return {
        "name": name,
        "life_ms": r["LifeDuration"],
        "hit_speed_ms": r["HitSpeed"],
        "hit_speed_offset_ms": r["HitSpeedOffset"],
        "damage": r["Damage"],
        # DAMAGE_TYPE EnableLevelScaling: false keeps the Damage at every level (a blank type scales).
        "damage_level_scaling": True if dt is None else dt.get("EnableLevelScaling", True) is not False,
        "crown_tower_damage_percent": ct_percent(r["CrownTowerDamagePercent"]),
        "radius_milli": shape["Radius"],
        "hits_ground": flag(r, "HitsGround"),
        "hits_air": flag(r, "HitsAir"),
        "only_enemies": bool(filt and filt.get("match_team_enemy")),
        "affects_hidden": flag(r, "AffectsHidden"),
        "pushback_milli": r["Pushback"],
        "follow_parent": r["FollowBehaviour"] == "FollowParent",
        "stay_after_parent_dies": flag(r, "StayAfterParentDies"),
        "filter": filt,
        "buff": buff,
        "buff_time_ms": buff_ms,
        "end_area": end,
    }


def hero_select_buff(h: Tables, name: str) -> tuple[dict, int]:
    """An OnHitAction that is an ActionSelect of BuffType ActionSpawns (by tower, by radius): its buff and
    SpawnTime when every option is ONE buff (the same columns but the name, the same time), which is the
    select's whole effect whatever its conditions pick. Any other select stops the build."""
    acts = h["actions"]
    _one_action(acts, name, "ActionSelect", {"ClassType", "SubActions", "PerActionConditions"})
    options = _list_col(acts, name, "SubActions")
    seen: list[tuple[dict, int]] = []
    for o in options:
        s = o if isinstance(o, dict) else acts.get(o)
        if s is None or s.get("ClassType") != "ActionSpawn" or s.get("SpawnType") != "BuffType":
            raise SystemExit(f"hero ability: select {name} has an option that is not a buff")
        if _present(s) - {"ClassType", "SpawnType", "SpawnTime", "SpawnData", "StatsTags"}:
            raise SystemExit(f"hero ability: select {name} has an option this reader does not read")
        b = norm_buff(h, s.get("SpawnData"))
        if b is None:
            raise SystemExit(f"hero ability: select {name} names no buff row {s.get('SpawnData')!r}")
        seen.append((b, s.get("SpawnTime")))
    if not seen:
        raise SystemExit(f"hero ability: select {name} has no option")
    first, ms = seen[0]
    for b, t_ms in seen[1:]:
        if {**b, "name": None} != {**first, "name": None} or t_ms != ms:
            raise SystemExit(f"hero ability: select {name}'s options are not one buff")
    return first, ms


# An `action_group` ability's actions: the cosmetic effect it plays, a buff it spawns on the hero, a character swap.
SELF_BUFF_EFFECT_KEYS = {"ClassType", "Effect", "EffectFlags", "OverrideDuration", "OverrideScale"}
SELF_BUFF_SPAWN_KEYS = {"ClassType", "SpawnType", "SpawnData", "SpawnTime", "StatsTags"}
SELF_BUFF_SWAP_KEYS = {"ClassType", "NewCharacterData"}
# The columns an action group's swapped form may set of its own (the [EXT.*] that extends the hero): display ones,
# and the attack finish pair, which the record carries as `override_attack_finish` and `attack_finish_time_ms`.
SELF_BUFF_FORM_COSMETIC = {"Base", "ClonedVersion", "PrefabAsset", "DamageEffect"}
SELF_BUFF_FORM_COLUMNS = {"OverrideAttackFinishTime", "AttackFinishTime"}


# THE HERO MEGA MINION'S WARP (`warp_effect`, `warp_start`): the keys each of its actions may set (read, or display
# only), the resolver it reads (its filter: enemy characters, visible, above ground; its order), and the start's steps.
WARP_LOCK_KEYS = {
    "ClassType", "WarpDelay", "LockDelay", "WarpAction", "ReleaseLockDelay", "AllowWarpWhenMovementSpeedZero",
    "AllowWarpWhenAttackSpeedZero",
}
WARP_HERO_KEYS = {
    "ClassType", "ActionToGetTargetFrom", "ActionToExecute", "SpellTargetIndicatorFilename", "SpellTargetIndicatorClipName",
    "HasTargetOnDeployAction", "NoTargetOnDeployAction", "Singleton", "ForceStopIfTrue",
}
WARP_KEYS = {
    "ClassType", "WarpMode", "Speed", "Acceleration", "TargetResolver", "OnWarpEndAction",
    "OffsetToTargetConsideringDirectionToTower", "ResetPendingDamageAtWarp", "OffsetX", "OffsetY",
    "ForceKeepTargetAfterWarp", "Singleton", "WarpTargetEffect",
}
WARP_MARK_KEYS = {
    "ClassType", "TargetResolver", "RadiusListForEffectSelection", "PlayerTargettedEffectList", "EnemyTargettedEffectList",
    "PlayerCircleTargetIndicatorList", "EnemyCircleTargetIndicatorList", "OnPickNewTargetAction", "OnTargetDiedAction",
    "GameTagsToSetWhileHasNotTarget", "PauseIfInCooldown", "DelayBeforeSearchForNextTarget", "Singleton", "NextAction",
    "ForceStopIfTrue", "EnemyTargetterEffect", "PlayerTargetterEffect",
}
WARP_FILTER = "default_targets_no_towers"
WARP_STRATEGIES = ["RESOLVER_STRATEGY_LOWEST_MAX_HP", "RESOLVER_STRATEGY_FURTHEST_TARGET"]
WARP_STRIKE_SHOWN = {
    "Rarity", "FilterFile", "FilterExportName", "ContinuousEffect", "Invisible", "RemoveOnAttack", "OverrideProjectile",
    "OnRemoveAction", "NotCloned",
}


# THE HERO GIANT'S SLAP (`slap_effect`): the keys its seeker and its push may set, the two filters it implements (its
# pick: enemy troops, air or ground; its landing blow: enemy ground troops) and the tags that refuse the push.
SLAP_SEEKER_KEYS = {
    "ClassType", "GameTagsToSet", "OncePerTarget", "WaitForTarget", "TargetSelectionMode", "TargetFilter", "Actions",
    "Delays", "Shape", "ActionOnSelfWhenTriggeredLeft", "ActionOnSelfWhenTriggeredRight", "PauseTags",
    "AbortIfInstigatorDies",
}
SLAP_PUSH_KEYS = {
    "ClassType", "DirectionMode", "PushbackStrength", "GameTagsToDisallowPush", "SuccessAction",
    "SuccessActionOnInstigator", "FailureActionOnInstigator", "PushbackDelay", "UpdatePhase", "StatsTags",
}
SLAP_FILTER = "Enemy_Characters_No_Buildings_Or_Towers"
SLAP_LANDING_FILTER = "GroundCharacterTargets"
SLAP_REFUSING_TAGS = {"NO_PUSHBACK", "UNTARGETABLE", "DASHING", "DISABLE_PHYSICAL_INTERACTIONS_WITH_OBJECTS"}


def slap_effect(h: Tables, name: str, subs: list[str], delays: list[int]) -> dict:
    """THE HERO GIANT'S BUTTON ([ABILITY.GiantHero_Ability]), read whole or the build stops. OnActivationAction is a group
    of the seeker at delay 0 and the button's UI state and effects. The seeker waits for (WaitForTarget), once per
    target, the enemy troop with the highest current hitpoints and shield (SLAP_FILTER) in its circle (`radius_milli`);
    on the Giant it runs, left or right, an animation and a hold (an ActionWithDuration of NO_MOVE and NO_ATTACK:
    `hold_ms`); on the target, the push: `push_delay_ms` on, toward the arena's horizontal centre, refused by
    SLAP_REFUSING_TAGS. On success the target is knocked up for `flight_ms` and lands with the landing blow (an area of
    one hit: `landing_damage`, its level scaling, `landing_radius_milli`, enemy ground troops), and takes the stun (a full
    stop, `buff` for `time_ms`); on failure the seeker runs again `retry_ms` on."""
    acts = h["actions"]

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"hero ability {name}: {what}")

    seek = [(s, d) for s, d in zip(subs, delays, strict=True) if acts.get(s)["ClassType"] == "ActionRunActionListOnObjectsInShapeWithPrio"]
    need(len(seek) == 1 and seek[0][1] == 0, "one seeker at delay 0")
    for s in subs:
        need(s == seek[0][0] or acts.get(s)["ClassType"] in ("ActionOverrideAbilityButtonState", "ActionPlayEffect"), f"step {s}")
    sk = _one_action(acts, seek[0][0], "ActionRunActionListOnObjectsInShapeWithPrio", SLAP_SEEKER_KEYS)
    need(sk["WaitForTarget"] is True and sk["OncePerTarget"] is True and sk["TargetFilter"] == SLAP_FILTER
         and sk["TargetSelectionMode"] == "HighestCurrentHpIncludeShields", "the seeker's pick")
    shape = h.shapes.get(sk["Shape"]) if isinstance(sk["Shape"], str) else None
    need(shape is not None and shape.get("ClassType") == "Circle" and isinstance(shape.get("Radius"), int), "the seeker's circle")
    per, per_d = _action_list(acts, seek[0][0], "Actions"), _action_list(acts, seek[0][0], "Delays")
    need(len(per) == 1 and list(per_d) in ([0], []), "the seeker's one action at delay 0")
    holds = set()
    for side in ("ActionOnSelfWhenTriggeredLeft", "ActionOnSelfWhenTriggeredRight"):
        got = _group_leaves(acts, sk[side])
        need(got is not None and all(d == 0 for d in got[1]), f"the {side} group")
        for s in got[0]:
            c = acts.get(s)["ClassType"]
            if c == "ActionWithDuration":
                w = _one_action(acts, s, c, {"ClassType", "ActionDuration", "GameTagsToSet"})
                tags = {x.strip() for x in str(w["GameTagsToSet"]).split(",")}
                need({"NO_MOVE", "NO_ATTACK"} <= tags and isinstance(w["ActionDuration"], int), f"the hold {s}")
                holds.add(w["ActionDuration"])
            else:
                need(c == "ActionRunForcedAnimationOnce", f"the {side} group runs {c}")
    need(len(holds) == 1, f"the two sides' holds {sorted(holds)}")
    push = _one_action(acts, per[0], "ActionDoPushbackFromInstigator", SLAP_PUSH_KEYS)
    need(push["DirectionMode"] == "ToHorizontalCenterFromInstigator" and push["UpdatePhase"] == "PostGameObjectTick"
         and isinstance(push["PushbackDelay"], int) and push["PushbackDelay"] >= 0, "the push")
    need({x.strip() for x in str(push["GameTagsToDisallowPush"]).split(",")} <= SLAP_REFUSING_TAGS, "the push's refusing tags")
    need(_cosmetic_action(acts, push["SuccessActionOnInstigator"]), "the push's success on the Giant")
    ok = _group_leaves(acts, push["SuccessAction"])
    need(ok is not None and all(d == 0 for d in ok[1]), "the push's success group")
    by = {}
    for s in ok[0]:
        by.setdefault(acts.get(s)["ClassType"], []).append(s)
    need(sorted(by) == ["ActionKnockback", "ActionPlayEffect", "ActionSpawn"] and len(by["ActionKnockback"]) == 1
         and len(by["ActionSpawn"]) == 1, f"the success group runs {sorted(by)}")
    kb = _one_action(acts, by["ActionKnockback"][0], "ActionKnockback",
                     {"ClassType", "Duration", "Height", "AbortIfInstigatorDies", "ActionOnLanding", "PassInstigatorToLandingAction",
                      "StatsTags"})
    need(isinstance(kb["Duration"], int) and kb["Duration"] > 0, "the knock's Duration")
    st = _one_action(acts, by["ActionSpawn"][0], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "SpawnTime", "StatsTags"})
    stun = norm_buff(h, st["SpawnData"])
    need(st["SpawnType"] == "BuffType" and stun is not None and isinstance(st["SpawnTime"], int)
         and all(stun[k] == -100 for k in ("speed_multiplier_raw", "hit_speed_multiplier_raw", "spawn_speed_multiplier_raw"))
         and not stun["damage_per_second"], "the stun")
    land = _group_leaves(acts, kb["ActionOnLanding"])
    need(land is not None and len(land[0]) == 1 and land[1] == [0], "the landing group")
    sp = _one_action(acts, land[0][0], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "StatsTags"})
    tb = h["area_effect_objects"]
    r = tb.get(sp["SpawnData"])
    need(sp["SpawnType"] == "AreaEffectType" and r is not None, "the landing area")
    unread = tb.set_fields.get(sp["SpawnData"], set()) - {"Rarity", "LifeDuration", "HitSpeed", "Damage", "DamageType", "Shape", "Filter", "StatsTags"}
    need(not unread and not r["HitSpeed"] and r["Filter"] == SLAP_LANDING_FILTER and isinstance(r["Damage"], int),
         f"the landing area sets {sorted(unread)} or is not one hit on enemy ground troops")
    lshape = h.shapes.get(r["Shape"]) if isinstance(r["Shape"], str) else None
    need(lshape is not None and lshape.get("ClassType") == "Circle" and isinstance(lshape.get("Radius"), int), "the landing's circle")
    dt = h.damage_types.get(r["DamageType"]) if isinstance(r["DamageType"], str) else None
    fail = _group_leaves(acts, push["FailureActionOnInstigator"])
    need(fail is not None and seek[0][0] in fail[0], "the push's failure does not seek again")
    for s in fail[0]:
        need(s == seek[0][0] or _cosmetic_action(acts, s), f"the failure group runs {s}")
    return {
        "kind": "slap",
        "radius_milli": shape["Radius"],
        "hold_ms": holds.pop(),
        "push_delay_ms": push["PushbackDelay"],
        "flight_ms": kb["Duration"],
        "buff": stun,
        "time_ms": st["SpawnTime"],
        "landing_damage": r["Damage"],
        "landing_radius_milli": lshape["Radius"],
        "landing_level_scaled": not (dt is not None and dt.get("EnableLevelScaling") is False),
        "retry_ms": fail[1][fail[0].index(seek[0][0])],
    }


def warp_effect(h: Tables, name: str, lock: str) -> dict:
    """THE HERO MEGA MINION'S WARP ([ABILITY] OnActivationAction an ActionBossBanditAbility), read whole or the build
    stops. The lock (no warp or lock delay) runs an ActionMegaMinionHeroAbility, which takes its target from the mark
    (an ActionSetIndicatorOnTarget) and runs the warp (an ActionWarpCharacter, InjectedCharacter: `speed`, `accel`, no
    offset, its target kept). Mark and warp read one resolver: a Global shape, WARP_FILTER, WARP_STRATEGIES (the lowest
    max hitpoints, then the furthest). The warp's end is a group of the strike buff (`strike_ms`) and the instant hit
    (an ActionSetInstantHit on `target_in_range(N) && is_combat_enabled`: `instant_range_milli`). The strike buff hides
    the hero (Invisible) until its attack (RemoveOnAttack); its OverrideProjectile is the strike's shot (`strike_damage`,
    `strike_crown_pct`), and its OnRemoveAction lands, `after_delay_ms` on, a buff whose OverrideProjectile gives every
    later shot `after_crown_pct` on a crown tower. The mark's re-pick after a death (DelayBeforeSearchForNextTarget, the
    forced cooldown) and its bots' buff are read and not run."""
    acts = h["actions"]

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"hero ability {name}: {what}")

    lk = _one_action(acts, lock, "ActionBossBanditAbility", WARP_LOCK_KEYS)
    need(not lk["WarpDelay"] and not lk["LockDelay"], "a delayed warp or lock")
    x = _one_action(acts, lk["WarpAction"], "ActionMegaMinionHeroAbility", WARP_HERO_KEYS)
    mark = _one_action(acts, x["ActionToGetTargetFrom"], "ActionSetIndicatorOnTarget", WARP_MARK_KEYS)
    w = _one_action(acts, x["ActionToExecute"], "ActionWarpCharacter", WARP_KEYS)
    need(w["WarpMode"] == "InjectedCharacter" and isinstance(w["Speed"], int) and w["Speed"] > 0
         and isinstance(w["Acceleration"], int) and w["Acceleration"] > 0, "the warp's motion")
    need(not w["OffsetX"] and not w["OffsetY"] and not w["OffsetToTargetConsideringDirectionToTower"]
         and w["ForceKeepTargetAfterWarp"] is True, "the warp's offsets or its target")
    need(w["TargetResolver"] == mark["TargetResolver"], "the warp and the mark read two resolvers")
    res = h.resolvers.get(w["TargetResolver"])
    shape = h.shapes.get(res.get("Shape")) if res else None
    need(res is not None and shape is not None and shape.get("ClassType") == "Global" and res.get("Filter") == WARP_FILTER
         and list(res.get("StrategyList") or []) == WARP_STRATEGIES, f"the resolver {w['TargetResolver']!r}")
    end = _group_leaves(acts, w["OnWarpEndAction"])
    need(end is not None and end[1] == [0] * len(end[0]), "the warp's end is not a group at delay 0")
    by = {acts.get(s)["ClassType"]: s for s in end[0]}
    need(sorted(by) == ["ActionSetInstantHit", "ActionSpawn"] and len(end[0]) == 2, f"the warp's end runs {sorted(by)}")
    sp = _one_action(acts, by["ActionSpawn"], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "SpawnTime"})
    need(sp["SpawnType"] == "BuffType" and isinstance(sp["SpawnTime"], int) and sp["SpawnTime"] > 0, "the strike buff")
    ih = _one_action(acts, by["ActionSetInstantHit"], "ActionSetInstantHit", {"ClassType", "ExecuteIfTrue"})
    m = re.fullmatch(r"target_in_range\((\d+)\) && is_combat_enabled", str(ih["ExecuteIfTrue"]))
    need(m is not None, f"the instant hit's test {ih['ExecuteIfTrue']!r}")
    bt = h["character_buffs"]
    strike = bt.get(sp["SpawnData"])
    need(strike is not None and bt.set_fields.get(sp["SpawnData"], set()) <= WARP_STRIKE_SHOWN
         and flag(strike, "Invisible") and flag(strike, "RemoveOnAttack"), f"the strike buff {sp['SpawnData']}")
    rm = _one_action(acts, strike["OnRemoveAction"], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "SpawnTime", "ActionDelay"})
    need(rm["SpawnType"] == "BuffType" and isinstance(rm["SpawnTime"], int) and rm["SpawnTime"] >= 99999, "the after buff")
    after = bt.get(rm["SpawnData"])
    need(after is not None and bt.set_fields.get(rm["SpawnData"], set()) <= {"Rarity", "OverrideProjectile"},
         f"the after buff {rm['SpawnData']}")
    shot1, shot2 = norm_projectile(h, strike["OverrideProjectile"]), norm_projectile(h, after["OverrideProjectile"])
    need(shot1 is not None and shot2 is not None, "the override projectiles")
    return {
        "kind": "warp",
        "speed": w["Speed"],
        "accel": w["Acceleration"],
        "instant_range_milli": int(m.group(1)),
        "strike_ms": sp["SpawnTime"],
        "strike_damage": shot1["damage"],
        "strike_crown_pct": shot1["crown_tower_damage_percent"],
        "after_crown_pct": shot2["crown_tower_damage_percent"],
        "after_delay_ms": rm["ActionDelay"] or 0,
        "mark": x["ActionToGetTargetFrom"],
    }


def warp_start(h: Tables, form: str, start, mark: str) -> int:
    """THE WARP'S START (the hero row's OnStartingAction, an ActionGroup): the button hidden at once until its variable is
    set (an ActionOverrideAbilityButtonState), and at one delay the variable set to 1 and the warp's mark begun. That
    delay, the ms after the hero's creation from which a press is taken (`available_after_ms`); else the build stops."""
    acts = h["actions"]
    got = _group_leaves(acts, start) if isinstance(start, str) else None
    if got is None:
        raise SystemExit(f"hero form {form}: its start is not a group")
    by = {acts.get(s)["ClassType"]: (s, d) for s, d in zip(*got, strict=True)}
    if sorted(by) != ["ActionOverrideAbilityButtonState", "ActionSetIndicatorOnTarget", "ActionSetVariable"]:
        raise SystemExit(f"hero form {form}: its start runs {sorted(by)}")
    (sv, dv), (mk, dm), (_, dh) = by["ActionSetVariable"], by["ActionSetIndicatorOnTarget"], by["ActionOverrideAbilityButtonState"]
    if mk != mark or dv != dm or dh != 0 or not isinstance(dm, int) or dm <= 0 or acts.get(sv)["Value"] != "1":
        raise SystemExit(f"hero form {form}: its start's mark, variable or delays")
    return dm


def shows_only(h: Tables, buff: str) -> bool:
    """A buff row that only shows: it sets nothing but display columns and GameTagsToSet, and each tag it sets appears
    once in the hero's own files (`h.hero_text`: its setting here), so no action of the hero's reads it."""
    tb = h["character_buffs"]
    if tb.get(buff) is None or tb.set_fields.get(buff, set()) - {"Rarity", "ContinuousEffect", "GameTagsToSet"}:
        return False
    tags = [x.strip() for x in str(tb.get(buff)["GameTagsToSet"] or "").split(",") if x.strip()]
    text = getattr(h, "hero_text", "")
    return bool(text) and all(text.count(tag) == 1 for tag in tags)


def shield_start(h: Tables, form: str, start) -> int | None:
    """A hero row's OnStartingAction that only sets its shield (the Hero Knight's Knight_hero_OnStartingGroup): an
    ActionGroup of one ActionSetShield at delay 0. Its ShieldPercent (0 to 100), or None for any other start, which the
    row's action graph then carries as before (and the loader refuses a mechanic there)."""
    if not isinstance(start, str) or not start:
        return None
    acts = h["actions"]
    got = _group_leaves(acts, start)
    if got is None or len(got[0]) != 1 or got[1] != [0] or acts.get(got[0][0])["ClassType"] != "ActionSetShield":
        return None
    if _present(acts.get(start)) - {"ClassType", "SubActions", "SubActionsDelay"}:
        raise SystemExit(f"hero form {form}: its start {start} sets {sorted(_present(acts.get(start)))}")
    pct = _one_action(acts, got[0][0], "ActionSetShield", {"ClassType", "ShieldPercent"})["ShieldPercent"]
    if not isinstance(pct, int) or not 0 <= pct <= 100:
        raise SystemExit(f"hero form {form}: its start's ShieldPercent {pct!r}")
    return pct


# THE HERO KNIGHT'S TAUNT (`taunt_area`): the columns its area may set beside the cosmetic ones, the keys of its
# ActionTaunt, and the columns its two taunted buffs may set (they only show: no LockTarget).
TAUNT_AREA_READ = {
    "Rarity", "LifeDuration", "Radius", "Damage", "OneHitPerTarget", "HitSpeed", "OnlyEnemies", "HitsGround", "HitsAir",
    "FollowBehaviour", "OnStartingAction", "OnHitAction",
}
TAUNT_KEYS = {
    "ClassType", "Singleton", "ResetsOnDistance", "AllowBuildingRetargeting", "ValidDuration", "ValidTargetBuff",
    "CrownTowerDuration", "CrownTowerBuff", "ForceStopIfTrue", "StatsTags",
}
TAUNT_BUFF_SHOWN = {"Name", "Rarity", "LockTarget", "FilterFile", "FilterExportName", "ContinuousEffect"}


def taunt_area(h: Tables, ability: str, name: str) -> dict:
    """THE HERO KNIGHT'S TAUNT (an ActionSpawn of an AreaEffectType in its button's group), read whole or the build
    stops. The area rides on the hero (FollowParent) for LifeDuration: a Radius, no damage, OneHitPerTarget, HitSpeed 0,
    enemies only, air and ground (`radius_milli`, `life_ms`, `hits_air`, `hits_ground`); its OnStartingAction only plays
    an effect. Its OnHitAction is an ActionGroup of one ActionTaunt at delay 0, run on a unit that is not warping
    (ExecuteIfTrue `!WARP`, read as: not under ground). The taunt: Singleton, not reset by distance, a building-only
    attacker taken too (AllowBuildingRetargeting), for ValidDuration on a unit and CrownTowerDuration on a crown tower
    (`troop_ms`, `tower_ms`), each with a buff that only shows, and stopped when its unit's row is the one
    ForceStopIfTrue's `has_data(ROW)` names (`stop_if_row`: the Goblin Demolisher's kamikaze form)."""
    tb = h["area_effect_objects"]
    r = tb.get(name)
    if r is None:
        raise SystemExit(f"hero ability {ability}: no area {name}")
    unread = tb.set_fields.get(name, set()) - TAUNT_AREA_READ - HERO_AREA_COSMETIC
    if unread:
        raise SystemExit(f"hero ability {ability}: area {name} sets {sorted(unread)}, which the taunt reader does not read")
    acts = h["actions"]
    ok = (
        isinstance(r["LifeDuration"], int) and r["LifeDuration"] > 0 and isinstance(r["Radius"], int) and r["Radius"] > 0
        and not r["Damage"] and r["OneHitPerTarget"] is True and not r["HitSpeed"] and r["OnlyEnemies"] is True
        and r["FollowBehaviour"] == "FollowParent"
    )
    if not ok:
        raise SystemExit(f"hero ability {ability}: area {name} is not a taunt that rides on the hero")
    if r["OnStartingAction"] is not None and not _cosmetic_action(acts, r["OnStartingAction"]):
        raise SystemExit(f"hero ability {ability}: area {name}'s OnStartingAction is not an effect")
    got = _group_leaves(acts, r["OnHitAction"]) if isinstance(r["OnHitAction"], str) else None
    g = acts.get(r["OnHitAction"]) if got is not None else None
    if got is None or len(got[0]) != 1 or got[1] != [0] or g["ExecuteIfTrue"] != "!WARP":
        raise SystemExit(f"hero ability {ability}: area {name}'s OnHitAction is not one taunt on a unit not warping")
    if _present(g) - {"ClassType", "SubActions", "SubActionsDelay", "ExecuteIfTrue"}:
        raise SystemExit(f"hero ability {ability}: group {r['OnHitAction']} sets {sorted(_present(g))}")
    t = _one_action(acts, got[0][0], "ActionTaunt", TAUNT_KEYS)
    if t["Singleton"] is not True or t["ResetsOnDistance"] is not False or t["AllowBuildingRetargeting"] is not True:
        raise SystemExit(f"hero ability {ability}: taunt {got[0][0]} is not a single taunt held at any distance")
    for col in ("ValidDuration", "CrownTowerDuration"):
        if not isinstance(t[col], int) or t[col] <= 0:
            raise SystemExit(f"hero ability {ability}: taunt {got[0][0]}'s {col} {t[col]!r}")
    for col in ("ValidTargetBuff", "CrownTowerBuff"):
        b = h["character_buffs"].get(t[col]) if isinstance(t[col], str) else None
        shown = {k for k, v in b.items() if v is not None} if b is not None else {"?"}
        if not shown <= TAUNT_BUFF_SHOWN or b["LockTarget"] not in (None, False):
            raise SystemExit(f"hero ability {ability}: taunt buff {t[col]!r} sets {sorted(shown - TAUNT_BUFF_SHOWN)}")
    m = re.fullmatch(r"has_data\((\w+)\)", str(t["ForceStopIfTrue"] or ""))
    if t["ForceStopIfTrue"] is not None and m is None:
        raise SystemExit(f"hero ability {ability}: taunt stop {t['ForceStopIfTrue']!r}")
    return {
        "radius_milli": r["Radius"],
        "life_ms": r["LifeDuration"],
        "hits_air": r["HitsAir"] is True,
        "hits_ground": r["HitsGround"] is True,
        "troop_ms": t["ValidDuration"],
        "tower_ms": t["CrownTowerDuration"],
        "stop_if_row": m.group(1) if m else None,
    }


def action_group_effect(h: Tables, name: str, subs: list[str], delays: list[int], hero_unit: str) -> dict:
    """`action_group` (the Hero Berserker's rage): the ActionGroup's sub-actions as STEPS, each at its SubActionsDelay
    from the trigger, in SubActions order. The steps read:
      - ActionPlayEffect: cosmetic, no step;
      - ActionSpawn of a BuffType with a SpawnTime: `buff`, the buff on the hero for SpawnTime ms;
      - ActionChangeGameObjectData: `form`, the hero wears another character (or its own again). A form must differ
        from the hero only in display columns and the attack finish pair, whose values the step carries as
        `override_attack_finish` and `attack_finish_time_ms` (null where the form leaves them to the hero, and on the
        hero's own row);
      - ActionSetShield: `shield`, the hero's shield set to ShieldPercent of its ShieldHitpoints (the Hero Knight's);
      - ActionSpawn of an AreaEffectType: `taunt`, a taunt area (`taunt_area`, the Hero Knight's).
    Any other sub-action, or a form that sets any other column, stops the build."""
    acts = h["actions"]
    steps: list[dict] = []
    for sub, d in zip(subs, delays, strict=True):
        cls = acts.get(sub)["ClassType"]
        if cls == "ActionPlayEffect":
            _one_action(acts, sub, cls, SELF_BUFF_EFFECT_KEYS)
        elif cls == "ActionSetShield":
            pct = _one_action(acts, sub, cls, {"ClassType", "ShieldPercent"})["ShieldPercent"]
            if not isinstance(pct, int) or not 0 <= pct <= 100:
                raise SystemExit(f"hero ability {name}: {sub}'s ShieldPercent {pct!r}")
            steps.append({"delay_ms": d, "do": "shield", "pct": pct})
        elif cls == "ActionSpawn" and acts.get(sub)["SpawnType"] == "AreaEffectType":
            area = _one_action(acts, sub, cls, {"ClassType", "SpawnType", "SpawnData"})["SpawnData"]
            steps.append({"delay_ms": d, "do": "taunt", **taunt_area(h, name, area)})
        elif cls == "ActionSpawn":
            sp = _one_action(acts, sub, cls, SELF_BUFF_SPAWN_KEYS)
            buff, time = norm_buff(h, sp["SpawnData"]), sp["SpawnTime"]
            if sp["SpawnType"] != "BuffType" or buff is None or not isinstance(time, int) or time <= 0:
                raise SystemExit(f"hero ability {name}: {sub} is not a buff row spawned with a SpawnTime")
            # A BUFF THAT ONLY SHOWS (the Hero Knight's self buff: a ContinuousEffect and a tag nothing else in the
            # hero's own files reads): no step.
            if shows_only(h, sp["SpawnData"]):
                continue
            steps.append({"delay_ms": d, "do": "buff", "buff": buff, "time_ms": time})
        elif cls == "ActionChangeGameObjectData":
            form = _one_action(acts, sub, cls, SELF_BUFF_SWAP_KEYS)["NewCharacterData"]
            override, finish = None, None
            if form != hero_unit:
                own = h["characters"].set_fields.get(form)
                if own is None:
                    raise SystemExit(f"hero ability {name}: form {form} is not a character row")
                extra = set(own) - SELF_BUFF_FORM_COSMETIC - SELF_BUFF_FORM_COLUMNS
                if extra:
                    raise SystemExit(
                        f"hero ability {name}: form {form} sets {sorted(extra)}, which this reader does not read"
                    )
                row = h["characters"].get(form)
                override = row["OverrideAttackFinishTime"] if "OverrideAttackFinishTime" in own else None
                finish = row["AttackFinishTime"] if "AttackFinishTime" in own else None
            steps.append(
                {
                    "delay_ms": d,
                    "do": "form",
                    "form": form,
                    "override_attack_finish": override,
                    "attack_finish_time_ms": finish,
                }
            )
        else:
            raise SystemExit(f"hero ability {name}: {sub} is an {cls}, which an action group does not read")
    if not any(st["do"] in ("buff", "shield", "taunt") for st in steps):
        raise SystemExit(f"hero ability {name}: an action group with no buff, shield or taunt is not read")
    return {"kind": "action_group", "steps": steps}


def ability_block(h: Tables, name: str, units: dict, hero_unit: str | None = None) -> dict:
    """THE BUTTON of a hero form ([ABILITY.<name>]), read whole or the build stops: its cost, charges,
    cooldown, cast and trigger times, KeepCurrentTarget, and what OnActivationAction does, one of two
    shapes -- `spawn_ahead` (an ActionGroup of one ActionSpawnToLocation of a placeholder building whose only
    job is its OnStartingAction, one ActionSpawnToLocation of the unit on the placeholder's point: the Hero
    Musketeer's turret) or `parent_areas` (an ActionGroup of ActionSpawns of areas whose source is the hero:
    the Hero Ice Golem's storm). A unit the effect names is added to `units`."""
    a = h.abilities.get(name) if isinstance(name, str) else None
    if a is None:
        raise SystemExit(f"hero ability {name!r}: no [ABILITY] row")
    # A blank PendingBuff (the Hero Valkyrie's "") names no buff.
    unread = set(a) - ABILITY_READ_KEYS - ABILITY_UI_KEYS - ({"PendingBuff"} if a.get("PendingBuff") == "" else set())
    if unread:
        raise SystemExit(f"hero ability {name}: sets {sorted(unread)}, which this reader does not read")
    acts = h["actions"]
    first = acts.get(a["OnActivationAction"]) if isinstance(a["OnActivationAction"], str) else None
    if first is not None and first["ClassType"] == "ActionRunActionListOnObjectsInShapeWithPrio":
        effect = throw_effect(h, name, a["OnActivationAction"], units)
        return {
            "name": name,
            "mana_cost": a["ManaCost"],
            "max_charges": a.get("MaxCharges"),
            "cooldown_ms": a.get("Cooldown"),
            "cast_ms": a.get("CastTime") or 0,
            "trigger_delay_ms": a.get("TriggerDelay") or 0,
            "keep_current_target": a.get("KeepCurrentTarget") is True,
            "is_champion": a.get("IsChampion") is True,
            "effect": effect,
        }
    if first is not None and first["ClassType"] == "ActionBossBanditAbility":
        return {
            "name": name,
            "mana_cost": a["ManaCost"],
            "max_charges": a.get("MaxCharges"),
            "cooldown_ms": a.get("Cooldown"),
            "cast_ms": a.get("CastTime") or 0,
            "trigger_delay_ms": a.get("TriggerDelay") or 0,
            "keep_current_target": a.get("KeepCurrentTarget") is True,
            "is_champion": a.get("IsChampion") is True,
            "effect": warp_effect(h, name, a["OnActivationAction"]),
        }
    got = _group_leaves(acts, a["OnActivationAction"])
    if got is None:
        raise SystemExit(f"hero ability {name}: OnActivationAction is not an ActionGroup")
    subs, delays = got
    classes = [acts.get(s)["ClassType"] for s in subs]
    group_classes = {"ActionSpawn", "ActionPlayEffect", "ActionChangeGameObjectData", "ActionSetShield"}
    seekers = [s for s in subs if acts.get(s)["ClassType"] == "ActionRunActionListOnObjectsInShapeWithPrio"]
    per_target = [acts.get(x)["ClassType"] for s in seekers for x in _action_list(acts, s, "Actions") if acts.get(x)]
    if "ActionDoPushbackFromInstigator" in per_target:
        effect = slap_effect(h, name, subs, delays)
        classes = []
    elif "ActionRunActionListOnObjectsInShapeWithPrio" in classes:
        effect = spin_chain_effect(h, name, subs, delays)
        classes = []
    elif "ActionGroundToAir" in classes:
        effect = ground_to_air_effect(h, name, subs, delays, units, hero_unit or "")
        classes = []
    elif "ActionSelect" in classes and "ActionHeal" in classes:
        effect = level_up_effect(h, name, subs, delays, hero_unit or "")
        classes = []
    elif (
        "ActionSpawn" in classes
        and set(classes) <= group_classes
        and any(acts.get(s)["SpawnType"] == "BuffType" for s in subs if acts.get(s)["ClassType"] == "ActionSpawn")
    ):
        effect = action_group_effect(h, name, subs, delays, hero_unit or "")
        classes = []
    elif any(d != 0 for d in delays):
        raise SystemExit(f"hero ability {name}: a delayed sub-action is not read")
    if not classes:
        pass
    elif classes == ["ActionSpawnToLocation"]:
        outer = _one_action(acts, subs[0], "ActionSpawnToLocation", SPAWN_AHEAD_KEYS)
        if outer["SpawnType"] != "CharacterType" or outer["UseDeploy"] is not False:
            raise SystemExit(f"hero ability {name}: the placeholder is not a character spawned without its deploy")
        dummy_name = outer["SpawnData"]
        _, dummy = unit_record(h, dummy_name)
        dummy_set = h["buildings"].set_fields.get(dummy_name, set())
        if dummy["Hitpoints"] is not None or dummy_set - PLACEHOLDER_SET:
            raise SystemExit(f"hero ability {name}: placeholder {dummy_name} is more than a timed spawn")
        inner_keys = SPAWN_AHEAD_KEYS | {"IgnoreEffects"}
        inner = _one_action(acts, dummy["OnStartingAction"], "ActionSpawnToLocation", inner_keys)
        if inner["SpawnType"] != "CharacterType" or (inner["RelativeX"] or 0) != 0 or (inner["RelativeY"] or 0) != 0:
            raise SystemExit(f"hero ability {name}: the placeholder's spawn is not a character on its own point")
        unit = inner["SpawnData"]
        _, urow = unit_record(h, unit)
        rec = norm_unit(h, unit, with_raw=True)
        # THE UNIT'S OnStartingAction: one ActionSpawnToLocation of a projectile row on the unit's own point, carried
        # as the unit's deploy projectile (the blow the loader lands where the unit appears).
        if urow["OnStartingAction"] is not None:
            keys = {"ClassType", "SpawnType", "SpawnData"}
            sa = _one_action(acts, urow["OnStartingAction"], "ActionSpawnToLocation", keys)
            if sa["SpawnType"] != "ProjectileType":
                raise SystemExit(f"hero ability {name}: {unit}'s OnStartingAction is not a projectile")
            rec["deploy_projectile"] = norm_projectile(h, sa["SpawnData"])
            rec["action_graph"] = None
        # Its ProjectileYOffset (the turret's 300) is in `rec` already: norm_unit writes it on every 15.535 row that
        # sets it.
        units[unit] = rec
        effect = {
            "kind": "spawn_ahead",
            "relative_x": outer["RelativeX"] or 0,
            "relative_y": outer["RelativeY"] or 0,
            "validate_as_building": outer["ValidatePlacementAsBuilding"] is True,
            "unit": unit,
            "use_deploy": inner["UseDeploy"] is True,
            "via": {"name": dummy_name, "life_ms": dummy["LifeTime"], "deploy_ms": dummy["DeployTime"]},
        }
    elif classes and all(c == "ActionSpawn" for c in classes):
        areas = []
        for s in subs:
            sp = _one_action(acts, s, "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "ParentGOAsSource"})
            if sp["SpawnType"] != "AreaEffectType" or sp["ParentGOAsSource"] is not True:
                raise SystemExit(f"hero ability {name}: {s} is not an area on the hero")
            areas.append(hero_area(h, sp["SpawnData"]))
        effect = {"kind": "parent_areas", "areas": areas}
    elif classes:
        raise SystemExit(f"hero ability {name}: OnActivationAction runs {classes}, which this reader does not read")
    return {
        "name": name,
        "mana_cost": a["ManaCost"],
        "max_charges": a.get("MaxCharges"),
        "cooldown_ms": a.get("Cooldown"),
        "cast_ms": a.get("CastTime") or 0,
        "trigger_delay_ms": a.get("TriggerDelay") or 0,
        "keep_current_target": a.get("KeepCurrentTarget") is True,
        "is_champion": a.get("IsChampion") is True,
        "effect": effect,
    }


def throw_effect(h: Tables, name: str, seeker: str, units: dict) -> dict:
    """THE HERO BALLOON'S BUTTON ([ABILITY.BalloonHero_Ability]), read whole or the build stops:
      - OnActivationAction: an ActionRunActionListOnObjectsInShapeWithPrio (the seeker) that takes, after its one Delay,
        the closest enemy ground troop in its circle Shape (`radius_milli`; filter THROW_FILTER: characters, no towers,
        no fliers, nothing hidden or invisible), once, and runs its activation group on the hero; the per-target action
        is an effect. OnFinishedAction, with no pick, is the failsafe;
      - the activation group: a 500 ms tag that stops the failsafe, an animation, the target finder (a resolver over
        the same Shape and filter, closest then highest current hp: THROW_STRATEGIES) and, `spawn_delay_ms` later, the
        spawner: an ActionSpawn of the throw's projectile from `start_offset_milli` ahead of the hero at that target;
      - the projectile: Speed 1, homing, no damage, released on the hit as `unit` deploying `unit_deploy_ms`; its
        OnStartingAction an ActionInterval (`ramp_start_ms`, then every `ramp_every_ms`) of the ramp: its variable
        up by 2, then its speed logX10000(max(5, v - 1)) / 80. `speeds` is that speed, block by block of the interval,
        with logX10000 read as 10000 ln (measured: 201, 243, 274, 299, 320 native a tick) and the k-th block reading
        the variable at 2k (measured: the first four blocks move at 5's speed);
      - the failsafe: an ActionInterval (`failsafe_start_ms`, then every `failsafe_every_ms`) of the same spawner with
        no target, stopped by the tag;
      - the unit's SpawnAreaObject, its landing: one hit (life = pulse) on enemy ground units in `landing.radius_milli`,
        `landing.damage` (level-scaled), `landing.crown_tower_damage_percent`.
    The unit is added to `units`."""
    import math

    acts = h["actions"]

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"hero ability {name}: {what}")

    sk = _one_action(acts, seeker, "ActionRunActionListOnObjectsInShapeWithPrio", THROW_SEEKER_KEYS)
    need(sk["OncePerTarget"] is True and sk["TargetSelectionMode"] == "Closest" and sk["TargetFilter"] == THROW_FILTER
         and sk["AbortIfInstigatorDies"] is False and sk["ParentAsInstigatorForSelfActions"] is True
         and sk["PauseTags"] == "CAPTURED", "the seeker's flags are not the ones read")
    seek_delays = col_list(acts, seeker, "Delays")
    need(len(seek_delays) == 1 and isinstance(seek_delays[0], int), f"the seeker's Delays {seek_delays}")
    shape = h.shapes.get(sk["Shape"]) if isinstance(sk["Shape"], str) else None
    need(shape is not None and shape.get("ClassType") == "Circle" and isinstance(shape.get("Radius"), int),
         f"the seeker's Shape {sk['Shape']!r} is not a circle")
    need(_cosmetic_action(acts, sk["Actions"]), "the seeker's per-target action is not an effect")
    group = group_subactions(h, sk["ActionOnSelfWhenTriggered"], f"hero ability {name} activation")
    classes = [acts.get(n)["ClassType"] for n, _ in group]
    need(classes == ["ActionWithDuration", "ActionRunForcedAnimationOnce", "ActionWriteResolverResultToContext",
                     "ActionSpawn", "ActionPlayEffect"], f"the activation group runs {classes}")
    stop = _one_action(acts, group[0][0], "ActionWithDuration", {"ClassType", "ActionDuration", "GameTagsToSet"})
    need(stop["GameTagsToSet"] == "UNIT_CUSTOM_TAG_1", "the activation's tag is not the failsafe's stop")
    finder_keys = {"ClassType", "Resolver", "ResultName"}
    finder = _one_action(acts, group[2][0], "ActionWriteResolverResultToContext", finder_keys)
    res = h.resolvers.get(finder["Resolver"])
    need(res is not None and res.get("Shape") == sk["Shape"] and res.get("Filter") == THROW_FILTER
         and list(res.get("StrategyList") or []) == THROW_STRATEGIES, f"the resolver {finder['Resolver']!r}")
    keys = {
        "ClassType", "TargetFromContextName", "SpawnType", "SpawnData", "ProjectileStartOffset", "StartPositionZOffset",
    }
    spawner = _one_action(acts, group[3][0], "ActionSpawn", keys)
    need(spawner["SpawnType"] == "ProjectileType" and spawner["TargetFromContextName"] == finder["ResultName"]
         and isinstance(spawner["ProjectileStartOffset"], int), "the spawner does not throw at the finder's pick")
    pt = h["projectiles"]
    pname = spawner["SpawnData"]
    pr = pt.get(pname)
    need(pr is not None, f"no projectile row {pname}")
    off = {k: pr.get(k) for k, v in THROW_PROJECTILE_PINNED.items() if pr.get(k) != v}
    unread = pt.set_fields.get(pname, set()) - set(THROW_PROJECTILE_PINNED) - THROW_PROJECTILE_COSMETIC - {
        "SpawnCharacter", "SpawnCharacterDeployTime", "OnStartingAction", "OnHitTargetAction"}
    need(not off and not unread, f"the projectile reads {off} off, {sorted(unread)} unread")
    tag_keys = {"ClassType", "ActionDuration", "GameTagsToSet"}
    need(_one_action(acts, pr["OnHitTargetAction"], "ActionWithDuration", tag_keys) is not None,
         "the projectile's OnHitTargetAction is not a tag")
    ramp = _one_action(acts, pr["OnStartingAction"], "ActionInterval",
                       {"ClassType", "StartCounterAt", "Interval", "ActionToExecute"})
    rsubs = group_subactions(h, ramp["ActionToExecute"], f"hero ability {name} ramp")
    need(len(rsubs) == 2 and all(d == 0 for _, d in rsubs), f"the ramp group {rsubs}")
    inc = _one_action(acts, rsubs[0][0], "ActionSetVariable", {"ClassType", "Variable", "Value"})
    var = inc["Variable"]
    up = _one_action(acts, rsubs[1][0], "ActionOverrideProjectileSpeed", {"ClassType", "SpeedOverride"})
    need(inc["Value"] == THROW_RAMP_VALUE.format(v=var) and up["SpeedOverride"] == THROW_RAMP_SPEED.format(v=var)
         and (h.variables.get(var) or {}).get("DefaultValue", 0) == 0, "the ramp's strings or its variable's start")
    speeds = [int(10000 * math.log(max(5, 2 * k - 1))) // 80 for k in range(THROW_SPEED_BLOCKS)]
    keys = {"ClassType", "StartCounterAt", "Interval", "ActionToExecute", "ForceStopIfTrue", "ExecuteIfTrue"}
    fs = _one_action(acts, sk["OnFinishedAction"], "ActionInterval", keys)
    need(fs["ForceStopIfTrue"] == "UNIT_CUSTOM_TAG_1" and fs["ExecuteIfTrue"] == "!UNIT_CUSTOM_TAG_1",
         "the failsafe is not stopped by the activation's tag")
    fsubs = group_subactions(h, fs["ActionToExecute"], f"hero ability {name} failsafe")
    need([n for n, _ in fsubs] == [group[1][0], group[3][0], group[4][0], group[0][0]]
         and all(d == 0 for _, d in fsubs), "the failsafe does not throw the same projectile")
    unit = pr["SpawnCharacter"]
    _, urow = unit_record(h, unit)
    rec = norm_unit(h, unit, with_raw=True)
    rec["spawn_area_object"] = None
    units[unit] = rec
    land = h["area_effect_objects"].get(urow["SpawnAreaObject"] or "")
    need(land is not None, f"{unit}'s SpawnAreaObject is no area row")
    off = {k: land.get(k) for k, v in THROW_LANDING_PINNED.items() if land.get(k) != v}
    unread = h["area_effect_objects"].set_fields.get(urow["SpawnAreaObject"], set()) - set(THROW_LANDING_PINNED) - {
        "Rarity", "Radius", "Damage", "CrownTowerDamagePercent", "StatsTags"}
    need(not off and not unread and isinstance(land["Radius"], int) and isinstance(land["Damage"], int),
         f"the landing area reads {off} off, {sorted(unread)} unread")
    return {
        "kind": "throw",
        "radius_milli": shape["Radius"],
        "seek_delay_ms": seek_delays[0],
        "spawn_delay_ms": group[3][1],
        "start_offset_milli": spawner["ProjectileStartOffset"],
        "unit": unit,
        "unit_deploy_ms": pr["SpawnCharacterDeployTime"],
        "ramp_start_ms": ramp["StartCounterAt"],
        "ramp_every_ms": ramp["Interval"],
        "speeds": speeds,
        "failsafe_start_ms": fs["StartCounterAt"],
        "failsafe_every_ms": fs["Interval"],
        "landing": {
            "radius_milli": land["Radius"],
            "damage": land["Damage"],
            "crown_tower_damage_percent": ct_percent(land["CrownTowerDamagePercent"]),
        },
    }


def ground_to_air_effect(h: Tables, name: str, subs: list[str], delays: list[int], units: dict, hero_unit: str) -> dict:
    """THE HERO WIZARD'S BUTTON ([ABILITY.WizardHeroAbility]), read whole or the build stops:
      - OnActivationAction: an ActionGroup, all at 0, of the lift (ActionGroundToAir), the hero's own buff (an
        ActionSpawn of a BuffType for its SpawnTime: `buff`, `buff_ms`) and an ActionSetInstantHit if its target is in
        range (`instant_range_milli`, the number in its target_in_range);
      - the lift: FlyingHeight (`flying_height_milli`), TransitionDuration (`transition_ms`), TotalDuration
        (`total_ms`), NO_ATTACK on both transitions; at its height (ActionOnFlyHeightReached) the hero becomes
        `air_unit` (an ActionChangeGameObjectData) beside a display variable, a listener that only resets it and the
        button's display; at its descent's start (ActionOnStartDescending) it becomes its own row again beside the
        variable and an animation;
      - the air form: the hero's row but for FlyingHeight and its Projectile (AIR_FORM_COLUMNS), whose
        OnTargetReachedAction is an ActionGroup, at 0, of two ActionSpawns of areas OffsetY (`reach.offset_y_milli`,
        the owner's forward) from the hit: a pulling area (`reach.tornado`) and a one-hit damage area (`reach.damage`).
    The air form is added to `units`, its projectile's reach group carried here, not on the projectile."""
    import re

    acts = h["actions"]

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"hero ability {name}: {what}")

    classes = [acts.get(s)["ClassType"] for s in subs]
    need(classes == ["ActionGroundToAir", "ActionSpawn", "ActionSetInstantHit"] and all(d == 0 for d in delays),
         f"the activation group runs {classes} at {delays}")
    lift = _one_action(acts, subs[0], "ActionGroundToAir", GROUND_TO_AIR_KEYS)
    need(lift["GameTagsToSetOnToAirState"] == "NO_ATTACK" and lift["GameTagsToSetOnToGroundState"] == "NO_ATTACK",
         "the lift's transition tags")
    sp = _one_action(acts, subs[1], "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "SpawnTime", "StatsTags"})
    buff = norm_buff(h, sp["SpawnData"])
    need(sp["SpawnType"] == "BuffType" and buff is not None and isinstance(sp["SpawnTime"], int), "the hero's buff")
    instant = _one_action(acts, subs[2], "ActionSetInstantHit", {"ClassType", "ExecuteIfTrue"})
    m = re.fullmatch(r"target_in_range\((\d+)\)", str(instant["ExecuteIfTrue"]))
    need(m is not None, f"the instant hit's condition {instant['ExecuteIfTrue']!r}")
    up = group_subactions(h, lift["ActionOnFlyHeightReached"], f"hero ability {name} height")
    down = group_subactions(h, lift["ActionOnStartDescending"], f"hero ability {name} descent")
    need(all(d == 0 for _, d in up + down), "a delayed step at the height or the descent")
    ui = {"ActionSetVariable", "ActionActivateOnCardDeploy", "ActionOverrideAbilityButtonState",
          "ActionRunForcedAnimationOnce"}
    swaps = []
    for part, steps in (("height", up), ("descent", down)):
        forms = [n for n, _ in steps if acts.get(n)["ClassType"] == "ActionChangeGameObjectData"]
        rest = {acts.get(n)["ClassType"] for n, _ in steps} - {"ActionChangeGameObjectData"}
        need(len(forms) == 1 and rest <= ui, f"the {part}'s group")
        swaps.append(_one_action(acts, forms[0], "ActionChangeGameObjectData",
                                 {"ClassType", "NewCharacterData"})["NewCharacterData"])
    air, back = swaps
    need(back == hero_unit, f"the descent's form {back} is not the hero's own row")
    own = h["characters"].set_fields.get(air, set())
    need(bool(own) and own <= AIR_FORM_COLUMNS, f"the air form {air} sets {sorted(own - AIR_FORM_COLUMNS)}")
    _, arow = unit_record(h, air)
    need(arow["FlyingHeight"] == lift["FlyingHeight"], "the air form's FlyingHeight is not the lift's")
    rec = norm_unit(h, air, with_raw=True)
    pr = h["projectiles"].get(arow["Projectile"])
    need(pr is not None and pr["OnTargetReachedAction"] is not None, f"the air form's shot {arow['Projectile']}")
    reach = group_subactions(h, pr["OnTargetReachedAction"], f"hero ability {name} shot's reach")
    need(len(reach) == 2 and all(d == 0 for _, d in reach), f"the shot's reach group {reach}")
    spawns = [_one_action(acts, n, "ActionSpawn", REACH_SPAWN_KEYS) for n, _ in reach]
    need(all(s["SpawnType"] in (None, "AreaEffectType") for s in spawns)
         and spawns[0]["OffsetY"] == spawns[1]["OffsetY"], "the shot's reach spawns")
    rows = [h["area_effect_objects"].get(s["SpawnData"]) for s in spawns]
    need(all(r is not None for r in rows), "a reach spawn names no area row")
    pulls = [s["SpawnData"] for s, r in zip(spawns, rows, strict=True) if r["Buff"] is not None]
    hits = [s["SpawnData"] for s, r in zip(spawns, rows, strict=True) if r["Buff"] is None]
    need(len(pulls) == 1 and len(hits) == 1, "the reach is not one pulling area and one damage area")
    units[air] = rec
    return {
        "kind": "ground_to_air",
        "flying_height_milli": lift["FlyingHeight"],
        "transition_ms": lift["TransitionDuration"],
        "total_ms": lift["TotalDuration"],
        "air_unit": air,
        "buff": buff,
        "buff_ms": sp["SpawnTime"],
        "instant_range_milli": int(m.group(1)),
        "reach": {
            "offset_y_milli": spawns[0]["OffsetY"],
            "tornado": norm_aeo(h, pulls[0]),
            "damage": norm_aeo(h, hits[0]),
        },
    }


def level_up_effect(h: Tables, name: str, subs: list[str], delays: list[int], hero_unit: str) -> dict:
    """THE HERO MINI PEKKA'S BUTTON ([ABILITY.MiniPekkHeroAbility]), read whole or the build stops. Its
    OnActivationAction is an ActionGroup, every step at delay 0, of:
      - an ActionSelect whose PerActionConditions are `<stack> == 0`, `== 1`, ... and a last `>= k` on one
        [VARIABLE] (the level stack), each option an inline ActionSetCharacterLevel of RelativeLevelAdjustment n whose
        NextAction only plays an effect -> `levels` (the levels gained, by the stack);
      - an ActionHeal whose Value is `(max_hp - hp) * <VAR> / 100`, VAR a [VARIABLE] with an int DefaultValue ->
        `heal_missing_pct` (after the level set: the group's order);
      - an ActionSetVariable of another [VARIABLE] to 1, which only the quest's ForceStopIfTrue and the hit effect's
        cosmetic select read (the press stops the quest as ABILITY_CASTED does);
      - an ActionFilter on the stack whose OnTrueAction is an ActionRunForcedAnimationOnce (an animation: the cast
        holds the hero for CastTime).
    The stack is filled by the hero unit's quest (`hero_quest`) -> `quest`."""

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"hero ability {name}: {what}")

    acts = h["actions"]
    need(all(d == 0 for d in delays), "a delayed step")
    by = {acts.get(s)["ClassType"]: s for s in subs}
    need(sorted(by) == ["ActionFilter", "ActionHeal", "ActionSelect", "ActionSetVariable"] and len(subs) == 4,
         f"the group is {[acts.get(s)['ClassType'] for s in subs]}")
    need(subs.index(by["ActionSelect"]) < subs.index(by["ActionHeal"]), "the heal before the level set")
    sel = by["ActionSelect"]
    need(_present(acts.get(sel)) <= {"ClassType", "PerActionConditions", "SubActions"}, f"{sel} sets more")
    conds = _action_list(acts, sel, "PerActionConditions")
    opts = _action_list(acts, sel, "SubActions")
    need(len(conds) == len(opts) >= 2, f"{sel}'s conditions and options")
    var = conds[0].split(" ")[0]
    want = [f"{var} == {k}" for k in range(len(conds) - 1)] + [f"{var} >= {len(conds) - 1}"]
    need(conds == want and var in h.variables and not (h.variables.get(var) or {}).get("DefaultValue"),
         f"{sel}'s conditions {conds}")
    levels = []
    for o in opts:
        need(isinstance(o, dict) and o.get("ClassType") == "ActionSetCharacterLevel"
             and set(o) <= {"ClassType", "RelativeLevelAdjustment", "NextAction"}
             and isinstance(o.get("RelativeLevelAdjustment"), int) and o["RelativeLevelAdjustment"] > 0, f"option {o}")
        nxt = o.get("NextAction")
        need(nxt is None or (acts.get(nxt) is not None and acts.get(nxt)["ClassType"] == "ActionPlayEffect"),
             f"option {o}'s NextAction")
        levels.append(o["RelativeLevelAdjustment"])
    heal = acts.get(by["ActionHeal"])
    need(_present(heal) == {"ClassType", "Value"}, f"{by['ActionHeal']} sets more")
    m = re.fullmatch(r"\(max_hp - hp\) \* (\w+) / 100", heal["Value"])
    pct = (h.variables.get(m.group(1)) or {}).get("DefaultValue") if m else None
    need(isinstance(pct, int) and 0 < pct <= 100, f"the heal {heal['Value']}")
    played = acts.get(by["ActionSetVariable"])
    need(_present(played) == {"ClassType", "Variable", "Value"} and played["Value"] == "1"
         and played["Variable"] in h.variables and played["Variable"] != var, f"{by['ActionSetVariable']}")
    filt = acts.get(by["ActionFilter"])
    anim = acts.get(filt["OnTrueAction"]) if isinstance(filt["OnTrueAction"], str) else None
    need(_present(filt) == {"ClassType", "Condition", "OnTrueAction"} and filt["Condition"].startswith(var)
         and anim is not None and anim["ClassType"] == "ActionRunForcedAnimationOnce"
         and _present(anim) <= {"ClassType", "PlaybackDuration"}, f"{by['ActionFilter']}")
    quest = hero_quest(h, name, hero_unit, var, played["Variable"], len(levels) - 1)
    return {"kind": "level_up", "levels": levels, "heal_missing_pct": pct, "quest": quest}


# The keys of the Hero Mini PEKKA's quest action (ActionMiniPekkaHeroQuest) that only the bar's display reads.
HERO_QUEST_UI_KEYS = {
    "BarIndicatorName", "ContainerName", "BarNamesList", "AbilityButtonStartLabel", "AbilityButtonEndLabel",
    "AbilityButtonTextFieldValues", "StatsTags",
}


def hero_quest(h: Tables, name: str, unit: str, var: str, played: str, top: int) -> dict:
    """THE QUEST THAT FILLS A LEVEL STACK: the hero unit's OnStartingAction, an ActionMiniPekkaHeroQuest, read whole or
    the build stops. Its bar fills from StartTimerDelay (`start_delay_ms`) after it starts; every hit that sets its
    UpgradeBarIfTrue tag adds AmountToIncreaseOnUpgradeBarList (`per_hit_ms`); full at Intervals (`interval_ms`), it
    runs OnIntervalReachedAction (an ActionGroup of one ActionSetVariable: `var` = min(top, var + 1)) and starts again,
    MaxResets times (`max_resets`), and stops for good on ForceStopIfTrue (the press: ABILITY_CASTED, or `played` set).
    The tag: the unit's OnHitTargetAction is an ActionGroup, every step at delay 0, of an ActionSpawn of a BuffType
    (SpawnTime 50) whose buff sets that tag and nothing else, and an ActionSelect on `var` that only plays effects
    (ActionRunOnInstigator of an ActionPlayEffect). Every list column holds one value."""

    def need(ok: bool, what: str) -> None:
        if not ok:
            raise SystemExit(f"hero ability {name}: its quest's {what}")

    acts = h["actions"]
    _, urow = unit_record(h, unit)
    q = acts.get(urow["OnStartingAction"]) if isinstance(urow["OnStartingAction"], str) else None
    need(q is not None and q["ClassType"] == "ActionMiniPekkaHeroQuest", f"OnStartingAction {urow['OnStartingAction']}")
    qn = urow["OnStartingAction"]
    read = {"ClassType", "Intervals", "AmountToIncreaseOnUpgradeBarList", "StartTimerDelay", "MaxResets",
            "OnIntervalReachedAction", "OnMaxResetsReachedAction", "ForceStopIfTrue", "UpgradeBarIfTrue"}
    need(not (_present(q) - read - HERO_QUEST_UI_KEYS), f"keys {sorted(_present(q) - read - HERO_QUEST_UI_KEYS)}")
    iv = _action_list(acts, qn, "Intervals")
    per = _action_list(acts, qn, "AmountToIncreaseOnUpgradeBarList")
    need(len(iv) == 1 and len(per) == 1 and all(isinstance(x, int) and x > 0 for x in iv + per), "lists")
    need(q["OnMaxResetsReachedAction"] in (None, ""), "OnMaxResetsReachedAction")
    stop = f"ABILITY_CASTED || !is_active_or_secondary_champion || {played} > 0"
    need(q["ForceStopIfTrue"] == stop, "ForceStopIfTrue")
    got = _group_leaves(acts, q["OnIntervalReachedAction"])
    need(got is not None and len(got[0]) == 1 and got[1] == [0], "OnIntervalReachedAction")
    up = acts.get(got[0][0])
    need(up["ClassType"] == "ActionSetVariable" and up["Variable"] == var
         and up["Value"] == f"min({top}, {var} + 1)", f"step {got[0][0]}")
    tag = q["UpgradeBarIfTrue"]
    hit = _group_leaves(acts, urow["OnHitTargetAction"]) if isinstance(urow["OnHitTargetAction"], str) else None
    need(hit is not None and all(d == 0 for d in hit[1]), "OnHitTargetAction")
    spawns = [s for s in hit[0] if acts.get(s)["ClassType"] == "ActionSpawn"]
    need(len(spawns) == 1, "OnHitTargetAction's buff")
    sp = acts.get(spawns[0])
    buff = h["character_buffs"].get(sp["SpawnData"]) if sp["SpawnType"] == "BuffType" else None
    need(buff is not None and _present(sp) == {"ClassType", "SpawnType", "SpawnData", "SpawnTime"}
         and sp["SpawnTime"] == 50 and {k for k, v in buff.items() if v is not None} == {"Rarity", "GameTagsToSet"}
         and buff["GameTagsToSet"] == tag,
         f"tag buff {sp['SpawnData']}")
    for s in hit[0]:
        if s in spawns:
            continue
        a = acts.get(s)
        need(a["ClassType"] == "ActionSelect", f"hit step {s}")
        for o in _action_list(acts, s, "SubActions"):
            r = acts.get(o) if isinstance(o, str) else None
            on_instigator = r is not None and r["ClassType"] == "ActionRunOnInstigator"
            eff = acts.get(r["ActionToExecute"]) if on_instigator else None
            need(eff is not None and eff["ClassType"] == "ActionPlayEffect", f"hit effect {o}")
    return {
        "start_delay_ms": q["StartTimerDelay"] or 0,
        "interval_ms": iv[0],
        "per_hit_ms": per[0],
        "max_resets": q["MaxResets"],
    }


def spin_chain_effect(h: Tables, name: str, subs: list[str], delays: list[int]) -> dict:
    """THE HERO VALKYRIE'S BUTTON ([ABILITY.ValkyrieHero_Ability]), read whole or the build stops:
      - OnActivationAction: an ActionGroup of the spin's clock reset (an ActionSetVariable of its variable to 0), the
        seeker, the pending buff `pending_delay_ms` later (an ActionSpawn of a BuffType, alive while the clock is
        short of its maximum: `pending_buff`) and the button's UI state (ActionOverrideAbilityButtonState, not read);
      - the seeker: an ActionRunActionListOnObjectsInShapeWithPrio that waits for (WaitForTarget) the closest enemy in
        its circle Shape (`radius_milli`) through SPIN_FILTER, once, and then runs the chain group on the hero; its
        per-target action is a 50 ms duration with no effect;
      - the chain group: an animation, the chain, and the guard buff (an ActionSpawn of a BuffType alive while the clock
        is short: `guard_buff`, a DamageReduction);
      - the chain: an ActionAttackChain whose resolver takes the closest in the same circle through the same filter,
        up to `count` targets, with no attack on reaching one (PerformAttackOnReach false, ResetTargetAfterReach,
        StopMovementWhenAtTarget), its phase buff (`chain_buff`) while it runs, and NO_ATTACK; complete when the clock
        reaches its maximum (`spin_ms`, the maximum variable's DefaultValue); its OnChainBegan an ActionInterval every
        `every_ms` (NO_ATTACK, stopped at the same maximum) of the blow (an ActionSpawnToLocation of `area` at the
        hero's x, y) and the clock's step (the variable plus the interval); its OnFinishedAction an animation's stop
        and the rest buff (`rest_buff`, SpawnTime `rest_ms`);
      - the blow's area: one hit (no HitSpeed) on `area.radius_milli`, `area.damage` level-scaled, the crown-tower
        percent, ground and air as its columns say, enemies only."""
    acts = h["actions"]

    def need(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"hero ability {name}: {what}")

    def spawn_buff(action: str, what: str) -> tuple[dict, int, dict]:
        sp = _one_action(acts, action, "ActionSpawn", {"ClassType", "SpawnType", "SpawnData", "SpawnTime"})
        buff = norm_buff(h, sp["SpawnData"])
        need(sp["SpawnType"] == "BuffType" and buff is not None and isinstance(sp["SpawnTime"], int),
             f"{what} {action}")
        return buff, sp["SpawnTime"], h["character_buffs"].get(sp["SpawnData"])

    classes = [acts.get(s)["ClassType"] for s in subs]
    need(classes[:3] == ["ActionSetVariable", "ActionRunActionListOnObjectsInShapeWithPrio", "ActionSpawn"]
         and all(c == "ActionOverrideAbilityButtonState" for c in classes[3:]) and delays[:2] == [0, 0],
         f"the activation group runs {classes} at {delays}")
    reset = _one_action(acts, subs[0], "ActionSetVariable", {"ClassType", "Variable", "Value"})
    var = reset["Variable"]
    need(reset["Value"] == "0" and (h.variables.get(var) or {}).get("DefaultValue", 0) == 0, f"the clock reset {reset}")
    sk = _one_action(acts, subs[1], "ActionRunActionListOnObjectsInShapeWithPrio", SPIN_SEEKER_KEYS)
    need(sk["OncePerTarget"] is True and sk["WaitForTarget"] is True and sk["TargetSelectionMode"] == "Closest"
         and sk["TargetFilter"] == SPIN_FILTER and sk["AbortIfInstigatorDies"] is False, "the seeker's flags")
    need(col_list(acts, subs[1], "Delays") == [0], "the seeker's Delays")
    dummy = _one_action(acts, sk["Actions"], "ActionWithDuration", {"ClassType", "ActionDuration"})
    need(dummy["ActionDuration"] == 50, "the seeker's per-target action")
    shape = h.shapes.get(sk["Shape"]) if isinstance(sk["Shape"], str) else None
    need(shape is not None and shape.get("ClassType") == "Circle" and isinstance(shape.get("Radius"), int)
         and shape.get("CollectionMethod") == "ContainsOrigin", f"the seeker's Shape {sk['Shape']!r}")
    pending, pending_ms, pending_row = spawn_buff(subs[2], "the pending buff")
    group = group_subactions(h, sk["ActionOnSelfWhenTriggered"], f"hero ability {name} chain group")
    gclasses = [acts.get(n)["ClassType"] for n, _ in group]
    need(gclasses == ["ActionRunForcedAnimationOnce", "ActionAttackChain", "ActionSpawn"]
         and all(d == 0 for _, d in group), f"the chain group runs {gclasses}")
    guard, guard_ms, guard_row = spawn_buff(group[2][0], "the guard buff")
    ch = _one_action(acts, group[1][0], "ActionAttackChain", SPIN_CHAIN_KEYS)
    res = h.resolvers.get(ch["TargetResolver"])
    need(res is not None and res.get("Shape") == sk["Shape"] and res.get("Filter") == SPIN_FILTER
         and list(res.get("StrategyList") or []) == ["RESOLVER_STRATEGY_CLOSEST_TARGET"], "the chain's resolver")
    need(ch["PerformAttackOnReach"] is False and ch["ResetTargetAfterReach"] is True
         and ch["StopMovementWhenAtTarget"] is True and ch["GameTagsToSet"] == "NO_ATTACK"
         and isinstance(ch["ChainCount"], int) and ch["ChainCount"] > 0, "the chain's flags")
    mx = next((k for k in h.variables if ch["ChainCompleteIfTrue"] == f"{var} >= {k}"), None)
    need(mx is not None and isinstance((h.variables.get(mx) or {}).get("DefaultValue"), int),
         f"the chain's end {ch['ChainCompleteIfTrue']!r}")
    spin_ms = h.variables[mx]["DefaultValue"]
    alive = f"{var} < {mx}"
    need(pending_row["AliveIfTrue"] == alive and guard_row["AliveIfTrue"] == alive, "the buffs' lives")
    chain_buff = norm_buff(h, ch["ChainPhaseBuff"])
    need(chain_buff is not None, f"the chain's phase buff {ch['ChainPhaseBuff']!r}")
    iv = _one_action(acts, ch["OnChainBegan"], "ActionInterval", SPIN_INTERVAL_KEYS)
    every = iv["Interval"]
    need(isinstance(every, int) and every > 0 and iv["GameTagsToSet"] == "NO_ATTACK"
         and iv["ForceStopIfTrue"] == ch["ChainCompleteIfTrue"], "the blow's interval")
    step = group_subactions(h, iv["ActionToExecute"], f"hero ability {name} blow")
    need(len(step) == 2 and all(d == 0 for _, d in step), f"the blow's group {step}")
    inner = group_subactions(h, step[0][0], f"hero ability {name} blow's spawn")
    need(len(inner) == 1 and inner[0][1] == 0, f"the blow's spawn group {inner}")
    keys = {"ClassType", "SpawnType", "SpawnData", "TargetExprX", "TargetExprY"}
    sp = _one_action(acts, inner[0][0], "ActionSpawnToLocation", keys)
    need(sp["SpawnType"] == "AreaEffectType" and sp["TargetExprX"] == "x" and sp["TargetExprY"] == "y",
         "the blow is not an area on the hero's point")
    clock = _one_action(acts, step[1][0], "ActionSetVariable", {"ClassType", "Variable", "Value"})
    need(clock["Variable"] == var and clock["Value"] == f"{var} + {every}", f"the clock's step {clock}")
    stop = group_subactions(h, ch["OnFinishedAction"], f"hero ability {name} end")
    need([acts.get(n)["ClassType"] for n, _ in stop] == ["ActionStopForcedAnimation", "ActionSpawn"]
         and all(d == 0 for _, d in stop), f"the chain's end {stop}")
    rest, rest_ms, _ = spawn_buff(stop[1][0], "the rest buff")
    at = h["area_effect_objects"]
    area = at.get(sp["SpawnData"])
    need(area is not None, f"no area row {sp['SpawnData']}")
    unread = at.set_fields.get(sp["SpawnData"], set()) - SPIN_AREA_READ - SPIN_AREA_COSMETIC
    need(not unread and isinstance(area["Radius"], int) and isinstance(area["Damage"], int)
         and isinstance(area["LifeDuration"], int), f"the blow's area sets {sorted(unread)}")
    return {
        "kind": "spin_chain",
        "radius_milli": shape["Radius"],
        "count": ch["ChainCount"],
        "pending_delay_ms": delays[2],
        "every_ms": every,
        "spin_ms": spin_ms,
        "pending_buff": pending,
        "pending_ms": pending_ms,
        "chain_buff": chain_buff,
        "guard_buff": guard,
        "guard_ms": guard_ms,
        "rest_buff": rest,
        "rest_ms": rest_ms,
        "area": {
            "name": sp["SpawnData"],
            "life_ms": area["LifeDuration"],
            "radius_milli": area["Radius"],
            "damage": area["Damage"],
            "crown_tower_damage_percent": ct_percent(area["CrownTowerDamagePercent"]),
            "hits_ground": flag(area, "HitsGround") is True,
            "hits_air": flag(area, "HitsAir") is True,
            "only_enemies": flag(area, "OnlyEnemies") is True,
        },
    }


def hero_form_records(v: Vintage, rarities: dict, level_base: str) -> tuple[list[dict], list[Path]]:
    """The `hero_forms` records (HERO_FORMS, in that order) from a load of their own, and the files read."""
    h = load_tables(v, hero=HERO_FORMS)
    h.level_base = level_base
    out: list[dict] = []
    files: list[Path] = []
    for form, (base, stem) in HERO_FORMS.items():
        files += hero_files(v, stem)
        s = h["spells_hero"].get(form)
        if s is None or s["CardForm"] != "HeroForm":
            raise SystemExit(f"hero form {form}: no [SPELL_HERO] row with CardForm HeroForm")
        # A row built from an overlay alone has no Name column of its own.
        s = Row(s.columns, {**s, "Name": form})
        if form not in h.hero_links.get(base, []):
            raise SystemExit(f"hero form {form}: {base}'s EvolvedSpells does not list it")
        key = next((k for k in ("spells_characters", "spells_buildings") if h[k].get(base) is not None), None)
        if key is None:
            raise SystemExit(f"hero form {form}: base card {base} is not a troop or building card")
        card = summon_card(h, rarities, "troop" if key == "spells_characters" else "building", "spells_hero", s)
        card["display_name"] = f"Hero {display_name(base)}"
        card["form_of"] = base
        unit = card["summon_character"]
        _, urow = unit_record(h, unit)
        units = {unit: norm_unit(h, unit, with_raw=True)}
        # A START THAT ONLY SETS THE SHIELD (the Hero Knight's, `shield_start`): the hero comes with that share of its
        # ShieldHitpoints (`spawn_shield_pct`), and the action graph that start alone makes is read whole here.
        pct = shield_start(h, form, urow["OnStartingAction"])
        if pct is not None:
            # Every other root must name no action row (the VisualActions health bar, as the Ronin's).
            roots = (units[unit].get("action_graph") or {}).get("roots", {})
            others = sorted(k for k, v in roots.items() if k != "OnStartingAction" and h["actions"].get(v) is not None)
            if others:
                raise SystemExit(f"hero form {form}: a shield start beside other actions {others}")
            card["spawn_shield_pct"] = pct
            card["action_graph"] = None
            units[unit]["action_graph"] = None
        # Its ProjectileYOffset (the Hero Musketeer's 300) is on `card` already: summon_card copies what norm_unit
        # writes on every 15.535 row that sets it (COSMETIC keeps it out of `raw`).
        # The hero's own files, where `shows_only` looks for a reader of a tag its button's buff sets.
        h.hero_text = "".join(p.read_text(encoding="utf-8") for p in hero_files(v, stem))
        card["ability"] = ability_block(h, urow["Ability"], units, unit)
        # THE WARP'S START (the Hero Mega Minion's, `warp_start`): its mark from 1500 ms after the hero's creation; the
        # action graph that start alone makes is read whole here.
        if card["ability"]["effect"]["kind"] == "warp":
            card["ability"]["effect"]["available_after_ms"] = warp_start(h, form, urow["OnStartingAction"],
                                                                         card["ability"]["effect"]["mark"])
            roots = (units[unit].get("action_graph") or {}).get("roots", {})
            others = sorted(k for k, v in roots.items() if k != "OnStartingAction" and h["actions"].get(v) is not None)
            if others:
                raise SystemExit(f"hero form {form}: a warp start beside other actions {others}")
            card["action_graph"] = None
            units[unit]["action_graph"] = None
        # ITS DEATH SPAWN (the Hero Balloon's BalloonHero_Bomb, a BalloonBomb [EXT] that changes display columns only):
        # its own row, in the form's units, which the loader loads for it.
        if urow["DeathSpawnCharacter"] is not None:
            units[urow["DeathSpawnCharacter"]] = norm_unit(h, urow["DeathSpawnCharacter"], with_raw=True)
        aeos = {}
        dae = card.get("death_area_effect")
        if dae:
            aeos[dae] = norm_aeo(h, dae)
        card["tables"] = {"units": units, "area_effect_objects": aeos}
        out.append(card)
    return out, files


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
        for f in UNIT_FIELDS_15535:
            if f in u:
                rec[f] = u[f]
        if "reflected_attack" in u:
            rec["reflected_attack"] = u["reflected_attack"]
        # The King Tower's ProjectileYOffset (norm_unit writes it only where set).
        if "projectile_y_offset_milli" in u:
            rec["projectile_y_offset_milli"] = u["projectile_y_offset_milli"]
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
    if not v.is_2018:
        # the file the `globals` block comes from (`globals_block`)
        files["globals.csv"] = {"sha256": sha256_of(v.raw / "globals.csv")}
        # the target filters a striking area names (`strike_area_block`)
        if (v.raw / "game_object_filters.toml").is_file():
            files["game_object_filters.toml"] = {"sha256": sha256_of(v.raw / "game_object_filters.toml")}
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
    else:
        # The globals the loader reads, by name (`globals_block`). 15.535 only: the 2018 Mirror row
        # carries no `spell.mirror` and stays refused, so that file needs none.
        doc["globals"] = globals_block(v)
        # The evolved forms the engine loads (`evolution_records`), after every other list, 15.535 only.
        doc["evolutions"] = evolution_records(t, rarities)
        # THE HERO FORMS (`hero_form_records`), last, from a load of their own: no other record changes.
        doc["hero_forms"], hero_files = hero_form_records(v, rarities, t.level_base)
        read = {p.relative_to(v.raw).as_posix(): {"sha256": sha256_of(p)} for p in hero_files}
        provenance["files"] = dict(sorted({**provenance["files"], **read}.items()))
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
