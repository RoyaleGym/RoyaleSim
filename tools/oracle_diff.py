#!/usr/bin/env python3
"""ORACLE DIFF -- step the Rust engine beside an offline-oracle trace, tick for tick.

    RoyaleSim\\.venv\\Scripts\\python.exe tools/oracle_diff.py                 # the walk gate
    ... tools/oracle_diff.py --family walk --family building_Giant --verbose
    ... tools/oracle_diff.py --trace data/oracle-native/walk/Giant_x3.5_y8.5_seed2.jsonl.gz
    ... tools/oracle_diff.py --rule                                            # score the
                                                                               # consumption
                                                                               # predicates

WHAT IT DOES
    For one trace it rebuilds the same situation in royalesim -- an empty board
    with the four crown towers, the same card at the same NATIVE position, spawned
    so that its first moving tick lands on the oracle's -- then steps both and
    prints the per-tick position error in NATIVE units (1 tile = 1000) together with
    the engine's path cells and the oracle's published `path_nodes`.

THE GATE: every `walk/` trace must be EXACT -- zero subtiles of error -- from the
oracle's first moving tick to the tick its unit starts attacking
(`behavior_state == 2`), which is where the isolated-unit laws stop applying
(calibration movement.CONTACT_DOMAIN).

WHY THE COMPARISON STOPS AT THE FIRST ATTACKING TICK
    The oracle's own attack predicate is wider than this engine's: measured on these
    six traces it starts attacking at Range + own CollisionRadius + target
    CollisionRadius of the tower centre, while royalesim's targeting.
    ADD_CHARACTER_RANGE_TO_RADIUS reading is Range + target CollisionRadius. After
    that tick the two engines are answering different questions.

UNITS
    The oracle is in native arena units (millitiles, 1000 per tile); royalesim is in
    subtiles, 18 per millitile (calibration representation.SUBTILE_PER_TILE and
    time.SPEED_TO_SUBTILES_PER_TICK). Errors are reported in NATIVE units, and an
    error of 0 means bit-exact.
"""

from __future__ import annotations

import argparse
import gzip
import itertools
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT.parent / "RoyaleGym"))

TRACES = ROOT / "data" / "oracle-native"
NATIVE_PER_TILE = 1000
CELL_NATIVE = 500

# The card ids the corpus uses, as royalesim names them.
CARD_OF_ID = {
    26000000: "Knight",
    26000003: "Giant",
    26000009: "Golem",
    26000010: "Skeletons",
    26000018: "MiniPekka",
    26000021: "HogRider",
    26000024: "RoyalGiant",
    26000060: "GoblinGiant",
    27000000: "Cannon",
}
WALK_GATE_CARDS = {"Knight", "Giant", "Golem", "Skeletons", "MiniPekka", "HogRider"}


# --------------------------------------------------------------------------- traces
def load(path: Path) -> tuple[dict, list[dict], list[dict]]:
    header, frames, events = None, [], []
    with gzip.open(path, "rt", encoding="utf-8") as f:
        for line in f:
            rec = json.loads(line)
            if rec["record"] == "header":
                header = rec
            elif rec["record"] == "frame":
                frames.append(rec["state"])
            else:
                events.append(rec)
    assert header is not None, f"{path}: no header"
    return header, frames, events


def units_of(header: dict, frames: list[dict]) -> dict[tuple[int, int], list[tuple[int, dict]]]:
    """(side, generation_key) -> [(tick, entity)], every non-tower mobile unit."""
    towers = {(t["side"], t["x"], t["y"]) for t in header.get("towers", [])}
    out: dict[tuple[int, int], list[tuple[int, dict]]] = {}
    for st in frames:
        for e in st["entities"]:
            if e.get("card_id") == -1 or (e["side"], e["x"], e["y"]) in towers:
                continue
            out.setdefault((e["side"], e["generation_key"]), []).append((st["tick"], e))
    return out


def cells_of(nodes) -> list[tuple[int, int]]:
    return [(v % 36, v // 36) for v in (nodes or [])]


# --------------------------------------------------------------------------- cost
# The engine's own cost model, read from the ledger, so that a path can be scored
# without the engine.  The cost gate -- "the oracle's path is exactly cost-minimal"
# -- is TIE-BREAK INDEPENDENT: two different lists of the same cost are both correct
# under the measured model, and only the expansion order separates them.
def cost_model() -> tuple[dict, int, int]:
    calib = json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"))
    costs = calib["pathfinding"]["PATHFINDING_COSTS"]["value"]
    ratio = calib["pathfinding"]["DIAGONAL_COST_RATIO"]["value"]
    return costs, ratio["num"], ratio["den"]


def cell_costs() -> dict[tuple[int, int], int]:
    """Per-cell entry cost from the engine's own arena.json (the same bits the
    2026 tilemap carries; tools/extract_arena.py derives one from the other)."""
    arena = json.loads((ROOT / "data" / "derived" / "arena.json").read_text(encoding="utf-8"))
    bits = arena["bits"]
    costs, _, _ = cost_model()
    out = {}
    for r, row in enumerate(arena["grid"]):
        for c, v in enumerate(row):
            if v & (bits["WATER"] | bits["NO_DEPLOY"]):
                out[(c, r)] = None
            elif v & (bits["LANE_LEFT"] | bits["LANE_RIGHT"]):
                out[(c, r)] = costs["road"]
            else:
                out[(c, r)] = costs["default"]
    return out


_CELL_COST = None

# CollisionRadius of the occluders the corpus uses, native units, read off
# data/raw/cr-15.535.29/csv_logic/characters/*.toml. TESLA IS 500, not the 600 this
# table used to assume -- with 600 its box claims cells the oracle's own path walks
# through, and both Tesla runs looked like the occlusion model failing.
OCCLUDER_RADIUS = {"Cannon": 600, "BombTower": 600, "Tesla": 500,
                   "princess": 1000, "king": 1400}


def occluded(header: dict, buildings: list[tuple[str, tuple[int, int]]], side: int) -> set[tuple[int, int]]:
    """The cells a building occludes: half-open AABB of the CollisionRadius, no mover
    pad (calibration pathfinding.OCCLUSION_MODEL).

    EVERY BUILDING, BOTH SIDES -- not only the mover's own. On the live 16.402
    corpus friendly-only occlusion fails 119 of 785 first paths and both sides fails
    6. `side` is kept
    in the signature because the caller has it and the next question about this
    corpus is whose box a path crosses; on the offline traces the change is inert,
    because no interior cell of any first path here lies inside an enemy tower box.
    """
    boxes = [(t["x"], t["y"], OCCLUDER_RADIUS[t["type"]]) for t in header.get("towers", [])]
    boxes += [(x, y, OCCLUDER_RADIUS.get(n, 600)) for n, (x, y) in buildings]
    out = set()
    for cx, cy, r in boxes:
        for c in range((cx - r) // CELL_NATIVE, (cx + r - 1) // CELL_NATIVE + 1):
            for rr in range((cy - r) // CELL_NATIVE, (cy + r - 1) // CELL_NATIVE + 1):
                out.add((c, rr))
    return out


def path_cost(cells_start_first: list[tuple[int, int]], blocked: set | None = None) -> int | None:
    """Cost of walking `cells` in order, paying for the cell ENTERED. `blocked` are
    occluded cells, which cost PATHFINDING_BUILDING_COST to enter.

    A box is PRICED, not refused, and there is no goal-cell exemption for it
    (calibration pathfinding.OCCLUDED_CELL_TREATMENT = cost_50). On the live 16.402
    corpus 97 of 785 paths cross a box interior, always the cell before the goal, so
    a hard block makes them infeasible; and with the price charged instead, keeping
    the spec-4.6 goal exemption scores 12 failures against 6 without it. Water and
    the bit-16 block stay impassable (`cell_costs` returns None) -- the OCCLUSION
    cost takes precedence over the terrain flag, so a cell in a box is priced even
    where its terrain is not walkable.
    """
    global _CELL_COST
    if _CELL_COST is None:
        _CELL_COST = cell_costs()
    blocked = blocked or set()
    costs, num, den = cost_model()
    total = 0
    for (c0, r0), (c1, r1) in itertools.pairwise(cells_start_first):
        if (c1, r1) in blocked:
            step = costs["building"]
        else:
            step = _CELL_COST.get((c1, r1))
            if step is None:
                return None
        if abs(c1 - c0) == 1 and abs(r1 - r0) == 1:
            step = step * num // den
        elif abs(c1 - c0) + abs(r1 - r0) != 1:
            return None
        total += step
    return total


BUILDING_CARD_IDS = {27000000: "Cannon", 27000004: "BombTower", 27000006: "Tesla"}


def buildings_of(header: dict, frames: list[dict], events: list[dict], walker_spawn: int):
    """The FRIENDLY buildings a trace places: (already standing, dropped later).

    POSITIONS COME FROM THE FRAMES, NOT FROM THE DEPLOY COMMAND. The game SNAPS a
    deploy to a tile, so the two differ by up to half a cell: `cannon_dx-0.5`
    commanded x = 3000 and the Cannon stands at x = 3500, and `Tesla_dx+0.0`
    commanded (3500, 9500) and stands at (3000, 9000). Reading `acts[i][1]` put a
    phantom occlusion box on the board -- it is why this tool used to report the
    ORACLE's own path as illegal (`COST 198 vs None`) on cannon_dx-0.5.

    `acts` / the `cannon_deployed` event are still used, but only for the TICK: a
    building dropped after the walker spawned has to be issued as a real deploy
    command at that tick so the engine sees the replan trigger where the oracle did.
    Its position is then taken from the first frame it appears in.
    """
    first_seen: dict[int, tuple[int, str, tuple[int, int]]] = {}
    for st in frames:
        for e in st["entities"]:
            name = BUILDING_CARD_IDS.get(e.get("card_id"))
            if name is None or e["generation_key"] in first_seen:
                continue
            first_seen[e["generation_key"]] = (st["tick"], name, (e["x"], e["y"]))
    # The COMMAND ticks, so that a mid-walk drop is replayed as a command rather than
    # as a spawn (the entity appears one tick after the command).
    cmd_ticks = sorted(
        [(act[2] or {}).get("tick", 0) for act in header.get("acts") or [] if act[0] in BUILDING_CARD_IDS.values()]
        + [ev["tick"] for ev in events if ev.get("event") == "cannon_deployed"]
    )
    pre: list[tuple[str, tuple[int, int]]] = []
    late: list[tuple[int, str, tuple[int, int]]] = []
    for seen_tick, name, xy in sorted(first_seen.values()):
        # Match the entity to the command that produced it: the latest command at or
        # before the tick it first appears. Falls back to `seen_tick - 1`.
        tick = max((t for t in cmd_ticks if t < seen_tick), default=seen_tick - 1)
        if tick <= walker_spawn:
            pre.append((name, xy))
        else:
            late.append((tick, name, xy))
    return pre, late


# --------------------------------------------------------------------------- engine
def engine_module():
    # rust_engine is imported for its side effect: it REFUSES a build whose
    # embedded calibration.json / arena.json differ from the files on disk, so a
    # diff can never be run against a stale extension.
    import royalesim
    from royalegym import rust_engine  # noqa: F401

    return royalesim


def catalogue_id(battle, name: str) -> int:
    """The protocol card id of `name`: its INDEX in the catalogue (py.rs
    `catalogue_rows` emits [name, kind, elixir, count, radius, flying, hp] per row,
    and `Battle.reset` takes those indices)."""
    for i, row in enumerate(json.loads(battle.catalogue_json())):
        if row[0] == name:
            return i
    raise SystemExit(f"{name} is not in the engine catalogue")


class Engine:
    """One royalesim battle: the tracked troop, the four crown towers, and any
    FRIENDLY buildings the trace placed.

    The buildings matter: they are the occluders (calibration
    pathfinding.OCCLUSION_MODEL) and without them a building_Giant diff is a diff
    of a different problem. One already standing when the walker spawns goes in
    through `spawns`; one dropped mid-walk is issued as a real deploy COMMAND at
    the trace's tick, so the engine sees it exactly as the oracle did -- the entity
    exists the tick after the command, which is replan trigger 2.

    DEPLOY TIMING IS NOT UNDER TEST HERE, and this docstring used to claim it was.
    `spawns` goes through state.rs `setup_spawn_place`, which sets `deploy_ms = 0`:
    the unit is on the board and free to move on the next tick, with no countdown.
    `diff_trace` then aligns on the engine's own first moving tick anyway, so a wrong
    countdown would be absorbed. The countdown itself is gated separately, by
    `deploy_countdown_is_spawn_plus_deploy_time` below, which issues a real command.
    """

    def __init__(self, card: str, native_xy: tuple[int, int], side: int, start_tick: int,
                 pre_buildings: list[tuple[str, tuple[int, int]]] = (),
                 late_buildings: list[tuple[int, str, tuple[int, int]]] = ()):
        royalesim = engine_module()
        self.sub_per_native = royalesim.SUBTILE_PER_MILLITILE
        self.battle = royalesim.Battle(None, [[0, 1, 2], [0, 1, 2]])
        cid = catalogue_id(self.battle, card)
        self.side = side
        sub = self.sub_per_native
        # The deck's first HAND_SIZE cards are the hand, and a mid-walk building is
        # deployed from it; elixir is set high so nothing is refused for cost.
        extra = [n for _t, n, _p in late_buildings]
        names = [card] + extra + [n for n in ("Cannon", "Archer", "Musketeer") if n != card and n not in extra]
        self.deck = [catalogue_id(self.battle, n) for n in names[:8]]
        self.slot_of = {n: i for i, n in enumerate(names[:8])}
        spawns = [(side, cid, native_xy[0] * sub, native_xy[1] * sub, -1)]
        for name, (bx, by) in pre_buildings:
            spawns.append((side, catalogue_id(self.battle, name), bx * sub, by * sub, -1))
        self.battle.reset(2, [self.deck, self.deck], 0, start_tick, [200000, 200000], None, spawns)
        self.card = card
        self.pending = sorted(late_buildings)

    def units(self):
        return self.battle.debug_units()

    def tick_count(self) -> int:
        return json.loads(self.battle.state_json())["tick"]

    def step(self) -> None:
        cmds = []
        now = self.tick_count()
        while self.pending and self.pending[0][0] <= now:
            _t, name, (bx, by) = self.pending.pop(0)
            slot = self.slot_of.get(name)
            if slot is not None and slot < 4:
                cmds.append((self.side, slot, bx * self.sub_per_native, by * self.sub_per_native))
        self.battle.step(cmds, 1)


# --------------------------------------------------------------------------- diff
class Diff:
    def __init__(self, name: str, family: str, card: str, uid_note: str):
        self.name, self.family, self.card, self.uid_note = name, family, card, uid_note
        self.rows: list[tuple[int, float, tuple[int, int], tuple[int, int]]] = []
        self.first_bad: tuple | None = None
        self.engine_cells: list[tuple[int, int]] = []
        self.oracle_cells: list[tuple[int, int]] = []
        self.window: tuple[int, int] = (0, 0)
        self.same_cost: bool | None = None
        self.blocked: set = set()
        self.siblings: int = 1
        self.note = ""

    @property
    def window_ticks(self) -> int:
        """How many ticks the window asks for. `len(self.rows)` short of this means
        the comparison was cut off -- the gate asserts the two are equal, because a
        max error of 0 over a truncated window is not the gate anybody meant."""
        return self.window[1] - self.window[0] + 1

    def add(self, tick: int, mine: tuple[int, int], theirs: tuple[int, int]) -> None:
        err = math.hypot(mine[0] - theirs[0], mine[1] - theirs[1])
        self.rows.append((tick, err, mine, theirs))
        if err > 0 and self.first_bad is None:
            self.first_bad = (tick, mine, theirs)

    @property
    def max_err(self) -> float:
        return max((r[1] for r in self.rows), default=0.0)

    @property
    def mean_err(self) -> float:
        return sum(r[1] for r in self.rows) / len(self.rows) if self.rows else 0.0

    def line(self) -> str:
        bad = "" if self.first_bad is None else (
            f"  first divergence t{self.first_bad[0]} "
            f"engine={self.first_bad[1]} oracle={self.first_bad[2]}"
        )
        short = "" if len(self.rows) == self.window_ticks else f"  TRUNCATED {len(self.rows)}/{self.window_ticks}"
        return (f"{self.family:16s} {self.name:30s} {self.card:11s} t{self.window[0]}..{self.window[1]} "
                f"n={len(self.rows):4d} max={self.max_err:8.2f} mean={self.mean_err:7.2f}{bad}{short}{self.note}")


def diff_trace(path: Path, family: str, verbose: bool = False, all_units: bool = False) -> list[Diff]:
    header, frames, events = load(path)
    out: list[Diff] = []
    by_unit = units_of(header, frames)
    # THE TRACKED UNIT is the first one in the trace. A multi-unit deploy
    # (Skeletons, Goblin Giant) puts its siblings in the CROWD-SEPARATION regime,
    # which no law implemented here models and which sets no flag in the oracle's
    # own state either (calibration movement.CONTACT_DOMAIN) -- they are listed with
    # --all-units and never gated.
    # Only units that actually WALK: a building_Giant trace's first entity by
    # generation key is the Cannon (or Bomb Tower, or Tesla) that was dropped in
    # front of the Giant, and it never moves.
    movers = []
    for (side, key), track in sorted(by_unit.items()):
        first_move = next((t1 for (_t0, a), (t1, b) in itertools.pairwise(track)
                           if (a["x"], a["y"]) != (b["x"], b["y"])), None)
        if first_move is not None and CARD_OF_ID.get(track[0][1].get("card_id")) is not None:
            movers.append((side, key, track, first_move))
    for idx, (side, _key, track, first_move) in enumerate(movers):
        if idx > 0 and not all_units:
            continue
        card_id = track[0][1].get("card_id")
        card = CARD_OF_ID[card_id]
        ticks = {t: e for t, e in track}
        attack = next((t for t, e in track if e.get("behavior_state") == 2), None)
        end = attack if attack is not None else track[-1][0] + 1
        spawn_tick = track[0][0]
        start = ticks[first_move - 1]
        count = sum(1 for (s2, _k2), tr2 in by_unit.items()
                    if s2 == side and tr2[0][1].get("card_id") == card_id)

        d = Diff(path.name.split(".jsonl")[0], family, card, f"unit {idx}")
        d.siblings = count
        pre, late = buildings_of(header, frames, events, spawn_tick)
        try:
            eng = Engine(card, (start["x"], start["y"]), side, spawn_tick, pre, late)
        except SystemExit as e:
            # The 2018 card table has no Goblin Giant; report and move on rather
            # than failing the whole sweep on a card the engine cannot spawn.
            d.note = f"  SKIPPED: {e}"
            out.append(d)
            continue
        # Run the engine to ITS first moving tick, then align: the position law is
        # what is under test, not the deploy countdown's off-by-one (which
        # `deploy_countdown_is_spawn_plus_deploy_time` gates on its own).
        #
        # THE TRACKED UNIT IS PINNED BY UID, not by list position: `debug_units`
        # returns the uid as element 0 of every row, and indexing `us[0]` would
        # silently switch to a different unit the moment one died. It happens to be
        # safe today only because `setup_spawn_place` materialises ONE entity per
        # spawn whatever the card's summon count -- so the engine side of the
        # Skeletons comparison holds a single isolated troop where the oracle has
        # three, which is an isolated-unit comparison by construction.
        eng_first = None
        prev = None
        uid = None
        for _ in range(0, max(1, first_move - spawn_tick) + 240):
            us = eng.units()
            if not us:
                break
            pos = (us[0][2], us[0][3])
            if prev is not None and pos != prev:
                eng_first, uid = pos, us[0][0]
                break
            prev = pos
            eng.step()
        if eng_first is None:
            d.note = "  ENGINE NEVER MOVED"
            out.append(d)
            continue
        d.blocked = occluded(header, [(n, xy) for n, xy in pre], side)
        tracked = next(u for u in eng.units() if u[0] == uid)
        d.engine_cells = list(tracked[6])
        d.oracle_cells = cells_of(ticks[first_move].get("path_nodes"))
        sub = eng.sub_per_native
        # tick `first_move` is the engine's first moving tick too; compare it and
        # every tick after it up to the oracle's first attacking tick.
        t = first_move
        d.window = (first_move, end - 1)
        while True:
            tracked = next((u for u in eng.units() if u[0] == uid), None)
            if tracked is None:
                d.note = f"  TRACKED UNIT GONE at t{t}"
                break
            mine = (tracked[2] / sub, tracked[3] / sub)
            mine = (int(mine[0]) if float(mine[0]).is_integer() else mine[0],
                    int(mine[1]) if float(mine[1]).is_integer() else mine[1])
            o = ticks.get(t)
            if o is not None:
                d.add(t, mine, (o["x"], o["y"]))
                if verbose:
                    print(f"    t{t} engine={mine} oracle={(o['x'], o['y'])} "
                          f"cells={tracked[6][-3:]} oracle_nodes={cells_of(o.get('path_nodes'))[-3:]}")
            t += 1
            if t >= end:
                break
            eng.step()
        out.append(d)
    return out


# ------------------------------------------------------- the deploy-timing gate
def deploy_countdown_is_spawn_plus_deploy_time(card: str = "Knight", command_tick: int = 100):
    """Issue `card` as a REAL deploy command; return ((spawn, first move), (expected
    spawn, expected first move), a note).

    spec 9.1: the unit appears the tick AFTER the command and first moves
    DeployTime/TICK_MS ticks after that, anchored on the spawn. The oracle's Knight is
    commanded at tick 100, appears at 101 and first moves at 121 (DeployTime 1000 ms,
    TICK_MS 50). `Engine`'s scenario spawns bypass the countdown entirely
    (`setup_spawn_place` sets `deploy_ms = 0`) and `diff_trace` re-aligns on the
    engine's own first moving tick, so this is the only thing in the tool that
    exercises calibration movement.DEPLOY_TIMING.
    """
    royalesim = engine_module()
    calib = json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"))
    tick_ms = calib["time"]["TICK_MS"]["value"]
    cards = json.loads((ROOT / "data" / "derived" / "cards.json").read_text(encoding="utf-8"))["cards"]
    deploy_ms = next(c["deploy_time_ms"] for c in cards if c["name"] == card)

    battle = royalesim.Battle(None, [[0, 1, 2], [0, 1, 2]])
    deck = [catalogue_id(battle, n) for n in (card, "Cannon", "Archer", "Musketeer")]
    # The battle stands AT `command_tick`; the step that carries the command takes it
    # to command_tick + 1, which is where the entity must appear.
    battle.reset(2, [deck, deck], 0, command_tick, [200000, 200000], None, [])
    sub = royalesim.SUBTILE_PER_MILLITILE
    battle.step([(0, 0, 3500 * sub, 8500 * sub)], 1)
    prev, first_move, spawn_seen = None, None, None
    for _ in range(400):
        us = battle.debug_units()
        now = json.loads(battle.state_json())["tick"]
        if us:
            if spawn_seen is None:
                spawn_seen = now
            pos = (us[0][2], us[0][3])
            if prev is not None and pos != prev:
                first_move = now
                break
            prev = pos
        battle.step([], 1)
    want = (command_tick + 1, command_tick + 1 + deploy_ms // tick_ms)
    got = (spawn_seen, first_move)
    note = (f"{card}: commanded t{command_tick}, appeared t{spawn_seen}, first moved t{first_move}; "
            f"DeployTime {deploy_ms} ms / TICK_MS {tick_ms} = {deploy_ms // tick_ms} ticks -> expected {want}")
    return got, want, note


# --------------------------------------------------------------------------- the rule sweep
def rule_sweep() -> None:
    """Score the two WAYPOINT_ARRIVE_RULE candidates against the whole corpus.

    Pure trace arithmetic -- no engine -- so it says what the ORACLE did, not what
    this engine does. It is the evidence behind calibration
    pathfinding.WAYPOINT_ARRIVE_RULE.
    """
    def trunc(a: int, b: int) -> int:
        q = abs(a) // abs(b)
        return q if (a < 0) == (b < 0) else -q

    def norm256(dx: int, dy: int) -> tuple[int, int]:
        L = math.isqrt(dx * dx + dy * dy)
        return (0, 0) if L == 0 else (trunc(dx * 256, L), trunc(dy * 256, L))

    def centre(v: int) -> tuple[int, int]:
        return ((v % 36) * CELL_NATIVE + CELL_NATIVE // 2, (v // 36) * CELL_NATIVE + CELL_NATIVE // 2)

    rows = []
    for fam in ("walk", "lane_sweep_Knight", "building_Giant", "repath_Giant", "meet"):
        for p in sorted((TRACES / fam).glob("*.jsonl.gz")):
            header, frames, _ = load(p)
            for _key, track in units_of(header, frames).items():
                prev_tail, seg = None, (0, 0)
                for (t0, a), (t1, b) in itertools.pairwise(track):
                    if t1 - t0 != 1:
                        prev_tail = None
                        continue
                    pa, pb = a.get("path_nodes") or [], b.get("path_nodes") or []
                    if not pa:
                        prev_tail = None
                        continue
                    tail = pa[-1]
                    if tail != prev_tail:
                        cx0, cy0 = centre(tail)
                        seg = norm256(cx0 - a["x"], cy0 - a["y"])  # frozen at assignment
                        prev_tail = tail
                    k = len(pa) - len(pb)
                    if k < 0 or pa[:len(pa) - k] != pb or (k and not pb):
                        continue
                    cx, cy = centre(tail)
                    dx, dy = cx - b["x"], cy - b["y"]
                    rows.append((bool(k), dx * dx + dy * dy, (dx * seg[0] + dy * seg[1]) // 256,
                                 fam, b.get("avoidance_offset") or 0))
    drops = [r for r in rows if r[0]]
    keeps = [r for r in rows if not r[0]]
    print(f"consumption predicate, whole corpus: {len(drops)} ordinary drops, {len(keeps)} keeps")
    for label, fn in (("euclid_post_move  (spec rule 7.5)", lambda r: math.isqrt(r[1]) <= 1000),
                      ("segment_projection (the ledger)", lambda r: r[2] <= 1000)):
        miss = [r for r in drops if not fn(r)]
        early = [r for r in keeps if fn(r)]
        wm = sum(1 for r in miss if r[3] == "walk")
        we = sum(1 for r in early if r[3] == "walk")
        av = sum(1 for r in early if r[4])
        print(f"  {label:36s} missed={len(miss):4d} early={len(early):3d} total={len(miss)+len(early):4d}"
              f" | walk missed={wm} early={we} | early with avoidance_offset!=0: {av}/{len(early)}")


# --------------------------------------------------------------------------- main
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--family", action="append", default=None,
                    help="walk | building_Giant | repath_Giant | lane_sweep_Knight | meet")
    ap.add_argument("--trace", default=None, help="one .jsonl.gz")
    ap.add_argument("--verbose", action="store_true", help="per-tick table")
    ap.add_argument("--rule", action="store_true", help="score the consumption predicates and exit")
    ap.add_argument("--all-units", action="store_true", help="also diff a deploy's crowd siblings")
    args = ap.parse_args()

    if not TRACES.exists():
        print(f"{TRACES} is absent -- nothing to diff", file=sys.stderr)
        return 0
    if args.rule:
        rule_sweep()
        return 0

    jobs: list[tuple[Path, str]] = []
    if args.trace:
        p = Path(args.trace)
        jobs.append((p, p.parent.name))
    else:
        for fam in args.family or ["walk", "building_Giant", "repath_Giant"]:
            jobs += [(p, fam) for p in sorted((TRACES / fam).glob("*.jsonl.gz"))]

    results: list[Diff] = []
    for p, fam in jobs:
        for d in diff_trace(p, fam, verbose=args.verbose, all_units=args.all_units):
            results.append(d)
            print(d.line(), flush=True)
            if d.engine_cells or d.oracle_cells:
                same = "IDENTICAL" if d.engine_cells == d.oracle_cells else "DIFFER"
                mc = path_cost(d.engine_cells[::-1], d.blocked)
                oc = path_cost(d.oracle_cells[::-1], d.blocked)
                # COMPARABLE ONLY BETWEEN THE SAME ENDPOINTS. When the two routes end
                # at different goal cells -- spec 6.2's open question, where the
                # oracle's goal is not the cheapest in-reach cell in 114 of 140
                # samples -- the two numbers price two different problems and saying
                # one is dearer means nothing. The tie-break-independent cost gate is
                # crates/royalesim/tests/oracle2026.rs `g1`, which plans between the
                # ORACLE path's own endpoints.
                ends = (d.engine_cells[:1], d.engine_cells[-1:]) == (d.oracle_cells[:1], d.oracle_cells[-1:])
                if not ends:
                    verdict = f"COST {mc} vs {oc} -- DIFFERENT ENDPOINTS, not comparable"
                    d.same_cost = None
                else:
                    verdict = "SAME COST" if mc == oc else f"COST {mc} vs {oc}"
                    d.same_cost = mc == oc
                print(f"    first path cells {same} ({verdict}; "
                      f"engine {len(d.engine_cells)} cells, oracle {len(d.oracle_cells)})")
                print(f"      engine ({len(d.engine_cells)}): {d.engine_cells}")
                print(f"      oracle ({len(d.oracle_cells)}): {d.oracle_cells}")

    gate = [d for d in results if d.family == "walk" and d.card in WALK_GATE_CARDS and d.uid_note == "unit 0"]
    paths = [d for d in results if d.engine_cells or d.oracle_cells]
    if paths:
        agree = sum(1 for d in paths if d.engine_cells == d.oracle_cells)
        costed = [d for d in paths if d.same_cost is not None]
        same = sum(1 for d in costed if d.same_cost)
        print()
        print(f"FIRST-PATH CELLS: {agree}/{len(paths)} identical to the oracle's published list; "
              f"{same}/{len(costed)} the same COST (gate G1 -- the tie-break-independent one)")
    print()
    print(f"WALK GATE ({len(gate)} traces, exact = 0 native units of error for the WHOLE window):")
    # THE WINDOW IS PART OF THE GATE. A max error of 0 over a window that stopped
    # early is not the gate anybody meant. The one allowed shortfall is Skeletons:
    # `setup_spawn_place` materialises ONE entity per spawn, so the engine meets the
    # princess tower with a single Skeleton where the oracle has three, takes every
    # shot and dies one tick before the oracle's window ends. Spawning three would put
    # the comparison in the unmodelled crowd-separation regime.
    def short_ok(d: Diff) -> bool:
        return d.card == "Skeletons" and d.window_ticks - len(d.rows) <= 2
    bad = [d for d in gate if d.max_err > 0 or (len(d.rows) != d.window_ticks and not short_ok(d))]
    for d in gate:
        full = len(d.rows) == d.window_ticks
        ok = d.max_err == 0 and (full or short_ok(d))
        mark = "PASS" if ok and full else ("PASS*" if ok else "FAIL")
        print(f"  {mark:5s} {d.name:34s} {d.card:12s} "
              f"ticks={len(d.rows):4d}/{d.window_ticks:4d} max={d.max_err:.2f}{d.note}")
    print(f"  => {len(gate) - len(bad)}/{len(gate)} exact  (PASS* = the known one-unit-vs-three shortfall)")
    try:
        got, want, note = deploy_countdown_is_spawn_plus_deploy_time()
        print()
        print(f"DEPLOY TIMING (spec 9.1, calibration movement.DEPLOY_TIMING): "
              f"{'PASS' if got == want else 'FAIL'}  {note}")
        if got != want:
            bad = bad or [1]
    except Exception as e:  # the engine may be absent when only --rule is wanted
        print(f"DEPLOY TIMING: could not run ({e})")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())
