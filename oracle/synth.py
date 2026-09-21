#!/usr/bin/env python3
"""Synthetic "recordings": a small kinematic model plus a model of the camera.

WHY THIS EXISTS
    tools/diff_harness.py is the acceptance gate for every movement constant, and a
    gate nobody has watched decide is a gate nobody should trust.  This file
    manufactures truth from a KNOWN configuration, degrades it the way a phone
    screen recording would, and lets the harness prove it picks the right config
    back out -- and, under --plant, that it notices when it should not.

    It is also the harness's first simulator BACKEND.  The Rust engine replaces it
    later behind the same `simulate(scenario, config) -> trace` call; what stays
    Python forever is the MEASUREMENT model (`render_recording`, `capture_events`),
    because the camera is not part of the game.

TWO LAYERS, KEPT APART ON PURPOSE
    1. LOGIC (`simulate`).  Integer subtiles, integer ticks, integer square roots.
       Floats are allowed outside crates/royalesim, but the logic layer mirrors the
       engine's arithmetic anyway so a later tick-for-tick comparison against the
       Rust backend is a diff of integers, not a tolerance argument.
    2. MEASUREMENT (`render_recording`, `capture_events`).  Floats.  This is the
       phone: display interpolation between ticks, capture frame rate, frame drops,
       tracker noise, homography scale/offset error, sprite foot-point bias.

WHAT THE LOGIC MODEL IS AND IS NOT
    It is the smallest model FAMILY that contains every candidate in
    oracle/scenarios.json, and nothing more: straight/lane-snap bridge routing,
    collision-only vs look-ahead building avoidance, three push laws, circle vs box
    footprints, three retarget rules, deploy snapping.  There is no combat, no
    damage, no projectiles, no enemy troops.  It is NOT a second engine and must
    never become one: there is one authority on battle behaviour.
    Every candidate here is a hypothesis about the real game; none is a claim.

WHERE NUMBERS COME FROM (no constant from calibration.json
written into code)
    * SUBTILE_PER_TILE, MILLITILE_PER_TILE, TICK_MS, SPEED_TO_SUBTILES_PER_TICK,
      REPATH_INTERVAL_TICKS, PUSH_MODEL, BUILDING_FOOTPRINT_MODEL, ALGORITHM
      -> data/calibration.json via `base_config()`.
    * river rows, bridges -> data/derived/arena.json.
    * tower positions -> the `Layout` section of the vendored tilemap.csv, because
      arena.json does not carry them yet (interface demand on extract_arena.py).
    * card stats (Speed, CollisionRadius, Mass, SightRange, Range, DeployTime,
      JumpEnabled) -> raw characters.csv / buildings.csv, ~2018 vintage, because
      data/derived/cards.json does not exist yet.  `card_stats` is the ONE function
      to re-point when it lands.
    * Candidate-only numbers that live nowhere else (look-ahead distance, a 3x3
      princess box) are passed in by oracle/scenarios.json, never defaulted here.

CANNOT
    Say anything about the real game.  A synthetic trace is never evidence; the
    promotion tool refuses one.
"""

from __future__ import annotations

import csv
import json
import math
import random
from dataclasses import dataclass, field, replace
from functools import lru_cache
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
CALIBRATION = ROOT / "data" / "calibration.json"
ARENA = ROOT / "data" / "derived" / "arena.json"
RAW = ROOT / "data" / "raw" / "retroroyale-2018"
CSV_DIR = RAW / "csv_logic"
TILEMAP = RAW / "tilemaps" / "tilemap.csv"

TRACE_FORMAT = "oracle-trace/1"
DATA_VINTAGE = "retroroyale-2018 csv_logic (~2018 client data; EVIDENCE, NOT SPEC)"


# --------------------------------------------------------------------------- #
# loaders                                                                     #
# --------------------------------------------------------------------------- #


@lru_cache(maxsize=4)
def load_calibration(path: str = str(CALIBRATION)) -> dict:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def cal_value(dotted: str, cal: dict | None = None) -> Any:
    node = cal if cal is not None else load_calibration()
    for part in dotted.split("."):
        node = node[part]
    return node["value"]


@lru_cache(maxsize=1)
def subtile() -> int:
    return int(cal_value("representation.SUBTILE_PER_TILE"))


@lru_cache(maxsize=1)
def millitile() -> int:
    return int(cal_value("representation.MILLITILE_PER_TILE"))


def sub_per_milli() -> int:
    s, m = subtile(), millitile()
    if s % m:
        raise ValueError(
            f"SUBTILE {s} is not a multiple of MILLITILE {m}; shipped distances would need rounding"
        )
    return s // m


def t100(v100: int) -> int:
    """Tile coordinate in hundredths -> subtiles, exact."""
    s = subtile()
    if (v100 * s) % 100:
        raise ValueError(f"{v100}/100 tile is not an exact subtile count")
    return v100 * s // 100


@lru_cache(maxsize=1)
def load_arena() -> dict:
    if not ARENA.exists():
        raise FileNotFoundError(
            f"{ARENA} missing -- run tools/extract_arena.py (data/derived is generated, "
            "never hand-edited)"
        )
    return json.loads(ARENA.read_text(encoding="utf-8"))


@lru_cache(maxsize=1)
def tower_layout() -> list[dict]:
    """Tower centres from the tilemap `Layout` section, in half-tiles.

    Parsed here only because data/derived/arena.json has no towers key yet.  The
    coordinates are half-tile integers; (7,13) is tile (3.5, 6.5).  Team is decided
    by which side of the river the tower sits on.
    """
    rows = list(csv.reader(TILEMAP.open(encoding="utf-8-sig")))
    out: list[dict] = []
    in_layout = False
    kind = None
    for r in rows:
        if r and r[0].strip():
            in_layout = r[0].strip() == "Layout"
            kind = None
            continue
        if not in_layout or len(r) < 4:
            continue
        if r[1].strip():
            kind = r[1].strip()
            continue
        if kind and r[2].strip().lstrip("-").isdigit() and r[3].strip().lstrip("-").isdigit():
            out.append({"name": kind, "hx": int(r[2]), "hy": int(r[3])})
    if len(out) != 6:
        # vacuity guard: an empty or partial parse reads exactly like an arena
        # with no towers in it, which every downstream scenario would accept.
        raise ValueError(f"expected 6 towers in tilemap Layout, parsed {len(out)}: {out}")
    arena = load_arena()
    half = int(arena["half_tiles_per_tile"])
    river_mid_half = (arena["water_half_rows"][0] + arena["water_half_rows"][1] + 1) / 2
    for t in out:
        t["team"] = "blue" if t["hy"] < river_mid_half else "red"
        t["x"] = t["hx"] * subtile() // half
        t["y"] = t["hy"] * subtile() // half
    return out


_TRUEISH = {"true", "TRUE", "True", "1"}


@lru_cache(maxsize=64)
def card_stats(name: str) -> dict:
    """Movement-relevant stats for one character or building, ~2018 vintage."""
    for fname in ("characters.csv", "buildings.csv"):
        rows = list(csv.reader((CSV_DIR / fname).open(encoding="utf-8-sig")))
        head = rows[0]
        for row in rows[2:]:
            if row and row[0] == name:

                def g(col: str, row: list[str] = row, head: list[str] = head) -> str:
                    return row[head.index(col)].strip() if col in head else ""

                def gi(col: str) -> int:
                    v = g(col)
                    return int(v) if v else 0

                return {
                    "name": name,
                    "file": fname,
                    "vintage": DATA_VINTAGE,
                    "speed": gi("Speed"),
                    "collision_radius_milli": gi("CollisionRadius"),
                    "mass": gi("Mass"),
                    "sight_milli": gi("SightRange"),
                    "range_milli": gi("Range"),
                    "deploy_ms": gi("DeployTime"),
                    "buildings_only": g("TargetOnlyBuildings") in _TRUEISH,
                    "jump_enabled": g("JumpEnabled") in _TRUEISH,
                    "is_building": fname == "buildings.csv",
                }
    raise KeyError(f"card {name!r} not in characters.csv or buildings.csv ({DATA_VINTAGE})")


# --------------------------------------------------------------------------- #
# integer helpers -- the same semantics as crates/royalesim/src/fixed.rs     #
# --------------------------------------------------------------------------- #


def tdiv(a: int, b: int) -> int:
    """Truncating division, like Rust's i64 `/`.  Python's `//` floors, which
    would make a unit walking left move differently from its mirror walking right."""
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


def isqrt(n: int) -> int:
    return math.isqrt(n) if n > 0 else 0


def step_toward(px: int, py: int, tx: int, ty: int, amount: int) -> tuple[int, int]:
    dx, dy = tx - px, ty - py
    ln = isqrt(dx * dx + dy * dy)
    if ln == 0 or amount >= ln:
        return tx, ty
    return px + tdiv(dx * amount, ln), py + tdiv(dy * amount, ln)


# --------------------------------------------------------------------------- #
# configuration                                                               #
# --------------------------------------------------------------------------- #

SPEED_UNITS = ("tiles_per_minute", "millitiles_per_50ms")
PATH_MODELS = ("lane_snap", "diagonal")
AVOIDANCE = ("collision_only", "lookahead")
PUSH_MODELS = ("mass_weighted", "speed_weighted", "equal_split")
FOOTPRINTS = ("circle", "box")
RETARGET = ("nearest_edge_in_sight", "nearest_centre_in_sight", "nearest_edge_global", "locked")
DEPLOY_SNAP = ("tile_centre", "half_tile_centre", "half_tile_vertex", "none")


@dataclass(frozen=True)
class ModelConfig:
    """One point in the candidate space.  No defaults: every field is either read
    from calibration.json (`base_config`) or set by a scenario, so nothing here can
    silently pick a side in an open question."""

    tps: int
    speed_unit: str
    path_model: str
    avoidance: str
    lookahead_tiles_100: int
    lookahead_margin_tiles_100: int
    repath_interval_ticks: int
    repath_phase_ticks: int
    push_model: str
    footprint: str
    princess_box_half_tiles_100: int | None
    retarget: str
    deploy_snap: str
    extra: dict = field(default_factory=dict, compare=False, hash=False)

    def validate(self) -> None:
        for name, allowed in (
            ("speed_unit", SPEED_UNITS),
            ("path_model", PATH_MODELS),
            ("avoidance", AVOIDANCE),
            ("push_model", PUSH_MODELS),
            ("footprint", FOOTPRINTS),
            ("retarget", RETARGET),
            ("deploy_snap", DEPLOY_SNAP),
        ):
            v = getattr(self, name)
            if v not in allowed:
                raise ValueError(f"{name}={v!r} not one of {allowed}")
        if self.tps <= 0 or self.repath_interval_ticks <= 0:
            raise ValueError("tps and repath_interval_ticks must be positive")

    def with_overrides(self, over: dict) -> ModelConfig:
        unknown = set(over) - set(self.__dataclass_fields__)
        if unknown:
            raise ValueError(f"unknown config field(s) {sorted(unknown)}")
        c = replace(self, **over)
        c.validate()
        return c

    def as_dict(self) -> dict:
        return {k: getattr(self, k) for k in self.__dataclass_fields__ if k != "extra"}


def base_config(cal: dict | None = None, **candidate_only: Any) -> ModelConfig:
    """The model as calibration.json currently believes it, plus candidate-only
    fields a scenario must supply (look-ahead distance/margin have no registry key).

    Mappings, each derived rather than assumed:
      TICK_MS -> tps (must divide 1000 exactly; a 16.67 ms tick cannot be stored
                 as an int and so would have to be registered as a rate)
      SPEED_TO_SUBTILES_PER_TICK * tps * 60 == SUBTILE    -> tiles_per_minute
      SPEED_TO_SUBTILES_PER_TICK * tps * 50 == SUBTILE*1000/MILLITILE... per 50 ms
    """
    cal = cal if cal is not None else load_calibration()
    tick_ms = int(cal_value("time.TICK_MS", cal))
    if 1000 % tick_ms:
        raise ValueError(f"TICK_MS={tick_ms} does not divide 1000")
    tps = 1000 // tick_ms
    mult = int(cal_value("time.SPEED_TO_SUBTILES_PER_TICK", cal))
    s = int(cal_value("representation.SUBTILE_PER_TILE", cal))
    m = int(cal_value("representation.MILLITILE_PER_TILE", cal))
    if mult * tps * 60 == s:
        speed_unit = "tiles_per_minute"
    elif mult * tps * 50 == (s // m) * 1000:
        speed_unit = "millitiles_per_50ms"
    else:
        raise ValueError(f"SPEED_TO_SUBTILES_PER_TICK={mult} at {tps} TPS matches no known unit")

    algo = cal_value("pathfinding.ALGORITHM", cal)
    algo_map = {
        "lane_flow_with_local_avoidance": ("lane_snap", "collision_only"),
        "post2025_diagonal_with_lookahead": ("diagonal", "lookahead"),
        # THE SHIPPED VALUE SINCE 2026-09-18, and this generator does NOT implement
        # it. `weighted_grid_astar` is the measured 2026 weighted grid A* over the
        # 36x64 half-tile cells (crates/royalesim/src/path2026.rs); reproducing it
        # here would mean a second implementation of the thing the offline oracle
        # already settles, and a synthetic trace from it would be a copy of the
        # engine, not evidence about the game.
        #
        # WHAT THIS MAPPING IS FOR, and what it is not: synth exists to SELF-TEST the
        # video-oracle harness's statistics (do the plants land, does the uncertainty
        # gate fire) on traces whose truth is known by construction. For that job the
        # generator only has to move plausibly. It is mapped to the nearest family
        # member, `diagonal`+`lookahead`, and `algorithm_not_modelled` below records
        # that the trace does NOT model the shipped algorithm -- so no synth result
        # may ever be read as evidence about the 2026 pathfinder. Use
        # tools/oracle_diff.py against data/oracle-native for that.
        "weighted_grid_astar": ("diagonal", "lookahead"),
    }
    if algo not in algo_map:
        raise NotImplementedError(
            f"pathfinding.ALGORITHM={algo!r} is not in the synth model family "
            f"({sorted(algo_map)}); weighted_grid_astar and authored_waypoints need the "
            "Rust backend"
        )
    path_model, avoidance = algo_map[algo]

    push = cal_value("collision.PUSH_MODEL", cal)
    if push not in PUSH_MODELS:
        raise NotImplementedError(f"collision.PUSH_MODEL={push!r} not modelled by synth")
    fp = cal_value("collision.BUILDING_FOOTPRINT_MODEL", cal)
    fp_map = {"collision_radius_circle": "circle", "tile_size_override_box": "box"}
    if fp not in fp_map:
        raise NotImplementedError(f"BUILDING_FOOTPRINT_MODEL={fp!r} not modelled by synth")

    fields = dict(
        tps=tps,
        speed_unit=speed_unit,
        path_model=path_model,
        avoidance=avoidance,
        # RETIRED IN THE LEDGER 2026-09-18: the offline oracle measured that there is
        # no periodic replan timer at all (150 structural recomputes over 31 859
        # path-ticks, no common period), so the key is null and the real triggers are
        # pathfinding.REPLAN_TRIGGERS. This generator's model still has a cadence
        # parameter, and a null one would mean "never replan"; it keeps the FORMER
        # folklore value so synth traces stay what they were -- a self-test of the
        # harness's statistics, not a claim about the game (see the algo_map note).
        repath_interval_ticks=(
            int(cal_value("pathfinding.REPATH_INTERVAL_TICKS", cal))
            if cal_value("pathfinding.REPATH_INTERVAL_TICKS", cal) is not None
            else 10
        ),
        # which tick of the repath cycle a unit's clock starts on; unknowable per
        # trial in the real game, so scenarios profile over it
        repath_phase_ticks=0,
        push_model=push,
        footprint=fp_map[fp],
        princess_box_half_tiles_100=None,
        # no registry key for these two; neutral placeholders that scenarios override
        retarget="nearest_edge_in_sight",
        deploy_snap="none",
        lookahead_tiles_100=0,
        lookahead_margin_tiles_100=0,
    )
    fields.update(candidate_only)
    c = ModelConfig(**fields)
    c.validate()
    return c


def speed_subtiles_per_tick(speed: int, cfg: ModelConfig) -> int:
    """Exact under every candidate, or raise.  A remainder here would mean the
    synthetic truth was rounded differently from the thing it is scored against."""
    s = subtile()
    if cfg.speed_unit == "tiles_per_minute":
        num, den = speed * s, 60 * cfg.tps
    else:
        num, den = speed * sub_per_milli() * 1000, 50 * cfg.tps
    if num % den:
        raise ValueError(
            f"speed {speed} under {cfg.speed_unit} at {cfg.tps} TPS is not "
            f"an integer subtile step ({num}/{den})"
        )
    return num // den


# --------------------------------------------------------------------------- #
# logic simulation                                                            #
# --------------------------------------------------------------------------- #


@dataclass
class Body:
    label: str
    card: str
    team: str
    x: int
    y: int
    r: int
    mass: int
    speed_step: int
    raw_speed: int
    sight: int
    rng: int
    spawn_tick: int
    active_tick: int
    buildings_only: bool
    jump: bool
    is_building: bool
    half: int | None = None  # box half-extent in subtiles when footprint == box
    target: str | None = None
    state: str = "deploying"
    detour: tuple[int, int] | None = None
    detour_obstacle: str | None = None
    detour_tick: int = -1
    xs: list = field(default_factory=list)
    ys: list = field(default_factory=list)
    ticks: list = field(default_factory=list)


def _ceil_div(a: int, b: int) -> int:
    return -((-a) // b)


def _river_bounds() -> tuple[int, int, list[tuple[int, int, int]]]:
    a = load_arena()
    half = int(a["half_tiles_per_tile"])
    lo = a["water_half_rows"][0] * subtile() // half
    hi = (a["water_half_rows"][1] + 1) * subtile() // half
    bridges = []
    for b in a["bridges"]:
        x0 = b["half_cols"][0] * subtile() // half
        x1 = (b["half_cols"][1] + 1) * subtile() // half
        bridges.append((x0, x1, (x0 + x1) // 2))
    return lo, hi, bridges


def snap_deploy(x: int, y: int, cfg: ModelConfig) -> tuple[int, int]:
    """Tap position -> spawn position under each deploy-snap candidate."""
    s = subtile()
    if cfg.deploy_snap == "none":
        return x, y
    if cfg.deploy_snap == "tile_centre":
        return (x // s) * s + s // 2, (y // s) * s + s // 2
    h = s // 2
    if cfg.deploy_snap == "half_tile_centre":
        return (x // h) * h + h // 2, (y // h) * h + h // 2
    # half_tile_vertex: nearest multiple of half a tile
    return ((x + h // 2) // h) * h, ((y + h // 2) // h) * h


def snap_lattice(cfg: ModelConfig) -> tuple[float, list[float]] | None:
    """(period_tiles, offsets_tiles) of spawn positions, or None for no snapping.
    Floats: this feeds the measurement-side likelihood in the harness, not logic."""
    if cfg.deploy_snap == "none":
        return None
    if cfg.deploy_snap == "tile_centre":
        return 1.0, [0.5]
    if cfg.deploy_snap == "half_tile_centre":
        return 0.5, [0.25]
    return 0.5, [0.0]


class Sim:
    def __init__(self, scenario: dict, cfg: ModelConfig):
        cfg.validate()
        self.cfg = cfg
        self.scn = scenario
        setup = scenario["setup"]
        self.ticks = _ceil_div(int(setup["duration_ms"]) * cfg.tps, 1000)
        self.river_lo, self.river_hi, self.bridges = _river_bounds()
        self.width = int(load_arena()["tiles"][0]) * subtile()
        self.bodies: dict[str, Body] = {}
        self.buildings: dict[str, Body] = {}
        self.events: list[dict] = []
        self.pending = sorted(setup.get("actions", []), key=lambda a: (a["t_ms"], a["label"]))
        for _i, t in enumerate(tower_layout()):
            st = card_stats(t["name"])
            lbl = f"{t['team']}_{t['name']}_{'L' if t['x'] * 2 < self.width else 'R'}"
            if t["name"] == "KingTower":
                lbl = f"{t['team']}_KingTower"
            self.buildings[lbl] = self._make(lbl, t["name"], t["team"], t["x"], t["y"], st, 0, 0)

    def _make(
        self, label: str, card: str, team: str, x: int, y: int, st: dict, spawn: int, active: int
    ) -> Body:
        r = st["collision_radius_milli"] * sub_per_milli()
        half = None
        if st["is_building"] and self.cfg.footprint == "box":
            half = r
            if card == "PrincessTower" and self.cfg.princess_box_half_tiles_100 is not None:
                half = t100(self.cfg.princess_box_half_tiles_100)
        return Body(
            label=label,
            card=card,
            team=team,
            x=x,
            y=y,
            r=r,
            mass=st["mass"],
            speed_step=speed_subtiles_per_tick(st["speed"], self.cfg) if st["speed"] else 0,
            raw_speed=st["speed"],
            sight=st["sight_milli"] * sub_per_milli(),
            rng=st["range_milli"] * sub_per_milli(),
            spawn_tick=spawn,
            active_tick=active,
            buildings_only=st["buildings_only"],
            jump=st["jump_enabled"],
            is_building=st["is_building"],
            half=half,
        )

    # -- geometry ----------------------------------------------------------- #
    @staticmethod
    def _closest_on_shape(b: Body, px: int, py: int) -> tuple[int, int]:
        if b.half is None:
            return b.x, b.y
        return (min(max(px, b.x - b.half), b.x + b.half), min(max(py, b.y - b.half), b.y + b.half))

    def _edge_dist(self, u: Body, b: Body) -> int:
        """Distance from a unit centre to a building's footprint EDGE (range is
        edge-to-edge: ADD_CHARACTER_RANGE_TO_RADIUS)."""
        if b.half is None:
            d = isqrt((u.x - b.x) ** 2 + (u.y - b.y) ** 2) - b.r
        else:
            qx, qy = self._closest_on_shape(b, u.x, u.y)
            d = isqrt((u.x - qx) ** 2 + (u.y - qy) ** 2)
        return max(d, 0)

    def _extent(self, b: Body) -> int:
        return b.r if b.half is None else b.half

    def _project_out(self, u: Body, b: Body) -> None:
        if b.half is None:
            R = b.r + u.r
            dx, dy = u.x - b.x, u.y - b.y
            d2 = dx * dx + dy * dy
            if d2 >= R * R:
                return
            d = isqrt(d2)
            if d == 0:
                # Deterministic and mirror-symmetric: push away from the river,
                # i.e. back toward the unit's own side.
                u.y = b.y + (-R if u.team == "blue" else R)
                return
            u.x = b.x + tdiv(dx * R, d)
            u.y = b.y + tdiv(dy * R, d)
            return
        qx, qy = self._closest_on_shape(b, u.x, u.y)
        dx, dy = u.x - qx, u.y - qy
        d2 = dx * dx + dy * dy
        if d2 >= u.r * u.r:
            return
        if d2 == 0:
            # centre inside the box: leave by the shallowest face
            pen = [
                (b.x + b.half - u.x, 1, 0),
                (u.x - (b.x - b.half), -1, 0),
                (b.y + b.half - u.y, 0, 1),
                (u.y - (b.y - b.half), 0, -1),
            ]
            p, sx, sy = min(pen)
            u.x += sx * (p + u.r)
            u.y += sy * (p + u.r)
            return
        d = isqrt(d2)
        u.x = qx + tdiv(dx * u.r, d)
        u.y = qy + tdiv(dy * u.r, d)

    # -- phases ------------------------------------------------------------- #
    def _spawn(self, tick: int) -> None:
        while self.pending and _ceil_div(int(self.pending[0]["t_ms"]) * self.cfg.tps, 1000) <= tick:
            a = self.pending.pop(0)
            st = card_stats(a["card"])
            x, y = t100(a["tile_100"][0]), t100(a["tile_100"][1])
            if not st["is_building"]:
                x, y = snap_deploy(x, y, self.cfg)
            active = tick + _ceil_div(st["deploy_ms"] * self.cfg.tps, 1000)
            b = self._make(a["label"], a["card"], a["team"], x, y, st, tick, active)
            (self.buildings if st["is_building"] else self.bodies)[a["label"]] = b
            self.events.append({"name": "spawn", "unit": a["label"], "tick": tick})

    def _enemy_buildings(self, u: Body) -> list[Body]:
        return [b for b in self.buildings.values() if b.team != u.team]

    def _default_tower(self, u: Body) -> Body:
        # LOGIC_XPOS_BASED_TOWER_TARGETING: lane by x, not nearest.
        side = "L" if u.x * 2 < self.width else "R"
        other = "red" if u.team == "blue" else "blue"
        return self.buildings[f"{other}_PrincessTower_{side}"]

    def _pick_target(self, u: Body) -> str:
        if self.cfg.retarget == "locked" and u.target is not None:
            return u.target
        best, best_key = self._default_tower(u), None
        for b in self._enemy_buildings(u):
            if b.card == "KingTower":
                continue
            edge = self._edge_dist(u, b)
            if edge > u.sight and self.cfg.retarget != "nearest_edge_global":
                continue
            if self.cfg.retarget == "nearest_centre_in_sight":
                key = (u.x - b.x) ** 2 + (u.y - b.y) ** 2
            else:
                key = edge * edge
            if best_key is None or key < best_key:
                best, best_key = b, key
        if best_key is None:
            best = self._default_tower(u)
        return best.label

    def _forward(self, u: Body) -> int:
        return 1 if u.team == "blue" else -1

    def _waypoint(self, u: Body, tgt: Body) -> tuple[int, int]:
        f = self._forward(u)
        own_side = (u.y < self.river_lo) if f > 0 else (u.y >= self.river_hi)
        tgt_across = (tgt.y >= self.river_hi) if f > 0 else (tgt.y < self.river_lo)
        on_bridge = self.river_lo <= u.y < self.river_hi
        if u.jump or not tgt_across:
            return tgt.x, tgt.y
        near = min(self.bridges, key=lambda br: (abs(br[2] - u.x), abs(br[2] - tgt.x)))
        cx = near[2]
        entry_y = self.river_lo if f > 0 else self.river_hi - 1
        exit_y = self.river_hi if f > 0 else self.river_lo - 1
        if own_side:
            if self.cfg.path_model == "lane_snap" and abs(u.x - cx) > u.speed_step:
                return cx, u.y
            return cx, entry_y
        if on_bridge:
            return cx, exit_y
        return tgt.x, tgt.y

    def _lookahead(self, u: Body, wx: int, wy: int, tick: int, tgt_label: str) -> tuple[int, int]:
        """Steer to a tangent point around the first obstacle whose expanded
        footprint the straight segment to the waypoint would clip, if that
        obstacle is within the look-ahead distance.  Re-decided only every
        repath_interval_ticks, which is the whole content of REPATH_INTERVAL."""
        due = (
            u.detour_tick < 0
            or (tick + self.cfg.repath_phase_ticks) % self.cfg.repath_interval_ticks == 0
        )
        if not due:
            if u.detour is None:
                return wx, wy
            ob = self.buildings.get(u.detour_obstacle or "")
            if ob is not None and not self._past(u, ob, wx, wy):
                return u.detour
            u.detour = None
            return wx, wy
        u.detour_tick = tick
        u.detour, u.detour_obstacle = None, None
        L = t100(self.cfg.lookahead_tiles_100)
        margin = t100(self.cfg.lookahead_margin_tiles_100)
        dx, dy = wx - u.x, wy - u.y
        ln = isqrt(dx * dx + dy * dy)
        if ln == 0:
            return wx, wy
        best = None
        for b in self.buildings.values():
            if b.label == tgt_label or b.spawn_tick > tick:
                continue
            R = self._extent(b) + u.r
            cx, cy = b.x - u.x, b.y - u.y
            along = tdiv(cx * dx + cy * dy, ln)  # projection length, subtiles
            if along <= 0 or along > min(L + R, ln + R):
                continue
            perp = tdiv(cx * dy - cy * dx, ln)  # signed lateral offset of the centre
            if abs(perp) >= R:
                continue
            if best is None or along < best[0]:
                best = (along, b, perp, R)
        if best is None:
            return wx, wy
        _, b, perp, R = best
        # go round on the side the unit already leans toward; ties break toward
        # the arena centre line so the rule is mirror-symmetric in x.
        side = -1 if perp > 0 else 1
        if perp == 0:
            side = 1 if (b.x * 2 < self.width) == (dy * self._forward(u) > 0) else -1
        # unit perpendicular (to the left of travel is (-dy, dx))
        px, py = tdiv(-dy * (R + margin), ln), tdiv(dx * (R + margin), ln)
        u.detour = (b.x - side * px, b.y - side * py)
        u.detour_obstacle = b.label
        return u.detour

    def _past(self, u: Body, b: Body, wx: int, wy: int) -> bool:
        """Abeam or beyond the obstacle along the direction to the real waypoint.
        Strictly-beyond would park a unit that lands exactly on its detour point
        until the next repath."""
        return (u.x - b.x) * (wx - u.x) + (u.y - b.y) * (wy - u.y) >= 0

    def _push_share_num_den(self, a: Body, b: Body) -> tuple[int, int]:
        """Fraction of the overlap that `a` absorbs, as an integer ratio.
        speed_weighted convention: a unit yields in proportion to the OTHER unit's
        speed (a fast pusher displaces a slow blocker more).  The Rust engine's
        PushModel::SpeedWeighted must use the same convention or the harness is
        scoring two different hypotheses under one name."""
        m = self.cfg.push_model
        if m == "mass_weighted":
            num, den = b.mass, a.mass + b.mass
        elif m == "speed_weighted":
            num, den = b.raw_speed, a.raw_speed + b.raw_speed
        else:
            num, den = 1, 2
        if den == 0:
            return 1, 2
        return num, den

    def run(self) -> dict:
        for tick in range(self.ticks + 1):
            self._spawn(tick)
            movers = [u for u in self.bodies.values() if tick >= u.active_tick]
            # Target + Path + propose, read-only against start-of-tick state.
            proposals: dict[str, tuple[int, int]] = {}
            for u in sorted(movers, key=lambda b: b.label):
                prev = u.target
                u.target = self._pick_target(u)
                tgt = self.buildings[u.target]
                if prev is not None and prev != u.target:
                    self.events.append(
                        {
                            "name": "retarget",
                            "unit": u.label,
                            "tick": tick,
                            "from": prev,
                            "to": u.target,
                        }
                    )
                if self._edge_dist(u, tgt) <= u.rng:
                    if u.state != "attacking":
                        self.events.append(
                            {
                                "name": "attack_start",
                                "unit": u.label,
                                "tick": tick,
                                "target": u.target,
                            }
                        )
                    u.state = "attacking"
                    proposals[u.label] = (u.x, u.y)
                    continue
                u.state = "walking"
                wx, wy = self._waypoint(u, tgt)
                if self.cfg.avoidance == "lookahead":
                    wx, wy = self._lookahead(u, wx, wy, tick, u.target)
                proposals[u.label] = step_toward(u.x, u.y, wx, wy, u.speed_step)
            # Move: apply all proposals at once, then static obstacles.
            for lbl, (nx, ny) in proposals.items():
                u = self.bodies[lbl]
                u.x, u.y = nx, ny
                self._river_clamp(u)
                for b in self.buildings.values():
                    if b.spawn_tick <= tick:
                        self._project_out(u, b)
            # Push: accumulate into a buffer, apply in one pass (invariant 2).
            live = [u for u in self.bodies.values() if tick >= u.spawn_tick]
            dxs: dict[str, int] = {u.label: 0 for u in live}
            dys: dict[str, int] = {u.label: 0 for u in live}
            for i, a in enumerate(live):
                for b in live[i + 1 :]:
                    R = a.r + b.r
                    ddx, ddy = b.x - a.x, b.y - a.y
                    d2 = ddx * ddx + ddy * ddy
                    if d2 >= R * R:
                        continue
                    d = isqrt(d2)
                    if d == 0:
                        ddx, ddy, d = subtile(), 0, subtile()
                    overlap = R - d
                    na, da = self._push_share_num_den(a, b)
                    push_a = tdiv(overlap * na, da)
                    push_b = overlap - push_a
                    dxs[a.label] -= tdiv(ddx * push_a, d)
                    dys[a.label] -= tdiv(ddy * push_a, d)
                    dxs[b.label] += tdiv(ddx * push_b, d)
                    dys[b.label] += tdiv(ddy * push_b, d)
            for u in live:
                u.x += dxs[u.label]
                u.y += dys[u.label]
                if dxs[u.label] or dys[u.label]:
                    self._river_clamp(u)
                    for b in self.buildings.values():
                        if b.spawn_tick <= tick:
                            self._project_out(u, b)
            for u in live:
                u.ticks.append(tick)
                u.xs.append(u.x)
                u.ys.append(u.y)
        return self._trace()

    def _river_clamp(self, u: Body) -> None:
        if u.jump:
            return
        if not (self.river_lo - u.r < u.y < self.river_hi + u.r):
            return
        for x0, x1, _ in self.bridges:
            if x0 <= u.x < x1:
                return
        # off-bridge inside the river band: back out to the nearer bank
        u.y = (
            self.river_lo - u.r
            if u.y < (self.river_lo + self.river_hi) // 2
            else self.river_hi + u.r
        )

    def _trace(self) -> dict:
        s = subtile()
        tps = self.cfg.tps
        units = {}
        for u in self.bodies.values():
            units[u.label] = {
                "card": u.card,
                "team": u.team,
                "t": [k / tps for k in u.ticks],
                "x": [x / s for x in u.xs],
                "y": [y / s for y in u.ys],
                "logic": {"tick": u.ticks, "x_sub": u.xs, "y_sub": u.ys},
            }
        return {
            "format": TRACE_FORMAT,
            "scenario_id": self.scn.get("id"),
            "source": {
                "kind": "sim",
                "backend": "oracle.synth",
                "config": self.cfg.as_dict(),
                "tps": tps,
                "data_vintage": DATA_VINTAGE,
            },
            "time_unit": "s",
            "space_unit": "tile",
            "units": units,
            "events": [dict(e, t=e["tick"] / tps) for e in self.events],
        }


def simulate(scenario: dict, cfg: ModelConfig) -> dict:
    return Sim(scenario, cfg).run()


class SynthBackend:
    """The harness's pluggable simulator interface, implemented by this model.
    A Rust backend must provide the same three callables."""

    name = "oracle.synth"

    def simulate(self, scenario: dict, config: ModelConfig) -> dict:
        return simulate(scenario, config)

    def snap_lattice(self, config: ModelConfig) -> tuple[float, list[float]] | None:
        return snap_lattice(config)

    def base_config(self, **candidate_only: Any) -> ModelConfig:
        return base_config(**candidate_only)


# --------------------------------------------------------------------------- #
# measurement model -- the phone, not the game                                #
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class CaptureModel:
    """Defaults are the error budget argued in oracle/extract_tracks.py's
    docstring for ~1080p/60fps, NOT measured on a real recording.  Change them
    when the first real recording's residuals say otherwise."""

    fps: float = 60.0
    interpolate: bool = True  # client draws positions between ticks
    render_delay_ticks: int = 1  # interpolation shows one tick behind
    pos_sigma_tiles: float = 0.06  # per-frame tracker jitter
    scale_sigma: float = 0.01  # homography scale error (fraction)
    offset_sigma_tiles: float = 0.10  # homography translation error
    foot_bias_sigma_tiles: float = 0.08  # per-unit sprite foot-point bias
    time_jitter_s: float = 0.002  # capture timestamp jitter
    drop_prob: float = 0.02  # dropped/unusable frames
    start_offset_s: float = 0.0  # recording clock vs battle clock

    def scaled(self, k: float) -> CaptureModel:
        return replace(
            self,
            pos_sigma_tiles=self.pos_sigma_tiles * k,
            scale_sigma=self.scale_sigma * k,
            offset_sigma_tiles=self.offset_sigma_tiles * k,
            foot_bias_sigma_tiles=self.foot_bias_sigma_tiles * k,
            time_jitter_s=self.time_jitter_s * k,
        )


def _interp_series(ts: list[float], vs: list[float], t: float, hold: bool) -> float | None:
    if not ts or t < ts[0] or t > ts[-1]:
        return None
    lo, hi = 0, len(ts) - 1
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if ts[mid] <= t:
            lo = mid
        else:
            hi = mid
    if hold or ts[hi] == ts[lo]:
        return vs[lo] if t < ts[hi] else vs[hi]
    w = (t - ts[lo]) / (ts[hi] - ts[lo])
    return vs[lo] + (vs[hi] - vs[lo]) * w


def render_recording(logic: dict, cap: CaptureModel, seed: int) -> dict:
    """Logic trace -> what extract_tracks.py would hand the harness."""
    rng = random.Random(seed)
    cx, cy = 9.0, 16.0
    scale = 1.0 + rng.gauss(0.0, cap.scale_sigma)
    ox, oy = rng.gauss(0.0, cap.offset_sigma_tiles), rng.gauss(0.0, cap.offset_sigma_tiles)
    tps = logic["source"]["tps"]
    delay = cap.render_delay_ticks / tps
    phase = rng.random() / cap.fps
    units = {}
    for lbl in sorted(logic["units"]):
        u = logic["units"][lbl]
        bx, by = 0.0, rng.gauss(0.0, cap.foot_bias_sigma_tiles)
        if not u["t"]:
            continue
        t0, t1 = u["t"][0] + delay, u["t"][-1] + delay
        f0 = math.ceil((t0 - phase) * cap.fps)
        f1 = math.floor((t1 - phase) * cap.fps)
        T, X, Y = [], [], []
        for f in range(f0, f1 + 1):
            if rng.random() < cap.drop_prob:
                continue
            tf = f / cap.fps + phase
            lx = _interp_series(u["t"], u["x"], tf - delay, not cap.interpolate)
            ly = _interp_series(u["t"], u["y"], tf - delay, not cap.interpolate)
            if lx is None or ly is None:
                continue
            mx = cx + (lx - cx) * scale + ox + bx + rng.gauss(0.0, cap.pos_sigma_tiles)
            my = cy + (ly - cy) * scale + oy + by + rng.gauss(0.0, cap.pos_sigma_tiles)
            T.append(tf + cap.start_offset_s + rng.gauss(0.0, cap.time_jitter_s))
            X.append(mx)
            Y.append(my)
        units[lbl] = {"card": u["card"], "team": u["team"], "t": T, "x": X, "y": Y}
    # Spawns are what a person can timestamp by eye on video (a building popping
    # into existence); they are shown on the first display frame after the tick.
    events = []
    for e in logic.get("events", []):
        if e["name"] != "spawn":
            continue
        f = math.ceil((e["t"] + delay - phase) * cap.fps - 1e-9)
        events.append(
            {
                "name": "spawn",
                "unit": e["unit"],
                "t": f / cap.fps + phase + cap.start_offset_s + rng.gauss(0.0, cap.time_jitter_s),
            }
        )
    return {
        "format": TRACE_FORMAT,
        "scenario_id": logic.get("scenario_id"),
        "source": {
            "kind": "synthetic_recording",
            "generator": "oracle.synth.render_recording",
            "truth_config": logic["source"].get("config"),
            "seed": seed,
            "capture": cap.__dict__.copy(),
            "capture_fps": cap.fps,
        },
        "time_unit": "s",
        "space_unit": "tile",
        "units": units,
        "events": events,
    }


def event_phase_logic(scenario: dict, cfg: ModelConfig, seed: int) -> dict:
    """Asynchronous inputs (taps at uncorrelated real times) become visible on the
    next logic tick.  Only the tick grid matters here, so no kinematics run."""
    setup = scenario["setup"]
    rng = random.Random(seed)
    n = int(setup["n_events"])
    lo_ms, hi_ms = setup["spacing_ms"]
    lat = int(setup.get("latency_ticks", 0))
    t_ms = 0.0
    ticks = []
    for _ in range(n):
        t_ms += rng.uniform(lo_ms, hi_ms)
        ticks.append(math.ceil(t_ms * cfg.tps / 1000.0) + lat)
    return {
        "format": TRACE_FORMAT,
        "scenario_id": scenario.get("id"),
        "source": {
            "kind": "sim",
            "backend": "oracle.synth",
            "config": cfg.as_dict(),
            "tps": cfg.tps,
        },
        "time_unit": "s",
        "space_unit": "tile",
        "units": {},
        "events": [
            {"name": setup.get("event_name", "appear"), "tick": k, "t": k / cfg.tps} for k in ticks
        ],
    }


def capture_events(logic: dict, cap: CaptureModel, seed: int) -> dict:
    """Event times as a capture device sees them: shown on the first display
    frame at or after the tick, timestamped by the capture clock."""
    rng = random.Random(seed)
    phase = rng.random() / cap.fps
    ev = []
    for e in logic["events"]:
        if rng.random() < cap.drop_prob:
            continue
        f = math.ceil((e["t"] - phase) * cap.fps - 1e-9)
        ev.append(
            dict(
                name=e["name"],
                t=f / cap.fps + phase + cap.start_offset_s + rng.gauss(0.0, cap.time_jitter_s),
            )
        )
    return {
        "format": TRACE_FORMAT,
        "scenario_id": logic.get("scenario_id"),
        "source": {
            "kind": "synthetic_recording",
            "generator": "oracle.synth.capture_events",
            "truth_config": logic["source"].get("config"),
            "seed": seed,
            "capture": cap.__dict__.copy(),
            "capture_fps": cap.fps,
        },
        "time_unit": "s",
        "space_unit": "tile",
        "units": {},
        "events": ev,
    }


def deploy_positions(scenario: dict, cfg: ModelConfig, cap: CaptureModel, seed: int) -> dict:
    """Random taps -> snapped spawns -> measured spawn positions (tiles)."""
    setup = scenario["setup"]
    rng = random.Random(seed)
    s = subtile()
    x0, x1 = setup["tap_x_100"]
    y0, y1 = setup["tap_y_100"]
    ox, oy = rng.gauss(0.0, cap.offset_sigma_tiles), rng.gauss(0.0, cap.offset_sigma_tiles)
    pts = []
    for _ in range(int(setup["n_taps"])):
        tx = t100(rng.randint(x0, x1))
        ty = t100(rng.randint(y0, y1))
        sx, sy = snap_deploy(tx, ty, cfg)
        pts.append(
            [
                sx / s + ox + rng.gauss(0.0, cap.pos_sigma_tiles),
                sy / s + oy + rng.gauss(0.0, cap.pos_sigma_tiles),
            ]
        )
    return {
        "format": TRACE_FORMAT,
        "scenario_id": scenario.get("id"),
        "source": {
            "kind": "synthetic_recording",
            "generator": "oracle.synth.deploy_positions",
            "truth_config": cfg.as_dict(),
            "seed": seed,
            "capture": cap.__dict__.copy(),
            "capture_fps": cap.fps,
        },
        "time_unit": "s",
        "space_unit": "tile",
        "units": {},
        "events": [],
        "spawn_positions": pts,
    }


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--scenario", required=True, help="scenario id from oracle/scenarios.json")
    ap.add_argument("--candidate", required=True, help="candidate name within that scenario")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--clean", action="store_true", help="emit the logic trace, no camera model")
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    scen = {
        s["id"]: s
        for s in json.loads((ROOT / "oracle" / "scenarios.json").read_text(encoding="utf-8"))[
            "scenarios"
        ]
    }
    if args.scenario not in scen:
        print(f"unknown scenario {args.scenario}")
        return 2
    s = scen[args.scenario]
    cfg = base_config(**s.get("candidate_only", {})).with_overrides(
        {**s.get("base_overrides", {}), **s["candidates"][args.candidate]}
    )
    cap = CaptureModel()
    if s["kind"] == "trajectory":
        tr = simulate(s, cfg)
        if not args.clean:
            tr = render_recording(tr, cap, args.seed)
    elif s["kind"] == "event_phase":
        tr = event_phase_logic(s, cfg, args.seed)
        if not args.clean:
            tr = capture_events(tr, cap, args.seed + 1)
    else:
        tr = deploy_positions(s, cfg, cap, args.seed)
    text = json.dumps(tr)
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
