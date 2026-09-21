#!/usr/bin/env python3
"""Play a whole battle on the real engine, gate it, and write an HTML you can watch.

WHY THIS EXISTS
    The pieces needed to produce a watchable battle live in three different places
    (``ClashParallelEnv``, ``replay.ReplayRecorder``, ``render.write_html``), and
    wiring them together by hand every time is how a simulator ends up with nobody
    ever looking at it.  This is the single command that plays a battle end to end
    and hands back a page.

    It is deliberately BOTH a demo and a gate.  A demo nobody checks is a battle
    that can be quietly wrong on screen; a gate with no picture is a number
    nobody can judge.  Every run therefore scores five gates over the battle it
    just played, and exits non-zero if any is red -- so a green HTML is evidence,
    not decoration.

THE GATES (each has a plant; see PLANTS)
    determinism   the recorded trace re-simulates hash-for-hash on a FRESH engine
                  (replay.verify_trace).  This is the strongest statement this
                  repo can make about itself: same seed, same battle, bit for bit.
    vacuity       the battle actually happened -- enough frames, BOTH seats landed
                  at least one accepted deploy, troops existed, something moved,
                  something took damage.  Without this the other four gates are
                  green over an empty board.
    arena         the trace header's grid equals the CURRENT data/derived/arena.json
                  read through protocol.Arena.load (the one Python arena loader).
                  A stale trace drawn on a regenerated arena puts units in the river.
    dry           no non-flying entity centre is ever on a WATER half-cell: pushes
                  and formations must resolve onto land, and a unit that ends up in
                  the river is a real break.
                  UNDER THE SHIPPED CONTACT LAW (calibration
                  pathfinding.PATH_SEARCH = client16402) it is a bounded
                  count instead: the live 16.402 captures show ground troops on
                  water cells for up to 1070 consecutive ticks (a unit attacking
                  from the bank, measured over every capture) and the game never
                  ejects them, so the gate only fails when more than 5 % of ground
                  positions are wet.
    render        write_html produced a page, it is self-contained, and the frames
                  embedded in it are exactly the frames of the trace after --stride.

WHAT IT CANNOT CATCH
    Whether the battle looks like CLASH ROYALE.  Nothing here compares anything to
    the real game -- that is the oracle's job -- and no
    recording exists yet.  The default policy is uniform-over-legal-actions with a
    no-op probability; it is a SMOKE POLICY and says nothing whatever about card
    balance, deck strength or who should win.  It also cannot see anything the
    trace does not record: troop projectiles, targets, attack timers and paths are
    not in EntityState, and the building-footprint invariant needs footprints the
    trace has no column for (that one is gated in Rust, tests/common).

USAGE
    python tools/watch_battle.py                       # play, gate, write battle.html
    python tools/watch_battle.py --open                # ... and open it
    python tools/watch_battle.py --seed 7 --steps 400
    python tools/watch_battle.py --engine mock         # no Rust extension needed
    python tools/watch_battle.py --noop-prob 0.2       # busier battle, towers fall
    python tools/watch_battle.py --plant desync        # prove a gate can fail
    python tools/watch_battle.py --all-plants          # exit 0 only if all land

    exit codes: 0 every gate green (or every plant landed) -- 1 a gate red, a plant
    that did not land, or an unrenderable trace -- 2 usage, or a component this run
    needs is missing.  A SKIP IS NOT A PASS: if the Rust
    extension is stale or absent this prints SKIPPED and exits 2 rather than
    silently falling back to the MockEngine, which is a different simulator.

PLANTS
    Each breaks the loaded battle (never the file on disk), asserts its edit changed the
    bytes, and requires the gate it AIMS AT to go red while the baseline was green.
    desync       corrupt one recorded frame hash          -> determinism
    frozen       replay frame 0's entities in every frame -> vacuity (movement)
    no_deploys   force both seats to no-op every step     -> vacuity (deploys)
    stale_arena  flip one half-cell in the trace header   -> arena
    wet_troop    move one ground troop onto the river     -> dry
    torn_render  drop a frame between the trace and page  -> render
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import msgspec
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / "RoyaleGym"))

from royalegym.env import ClashParallelEnv  # noqa: E402
from royalegym.mock_engine import MockEngine  # noqa: E402
from royalegym.protocol import (  # noqa: E402
    BIT_WATER,
    Arena,
    Calibration,
    DeployStatus,
    EntityKind,
    Winner,
)
from royalegym.render import RenderError, extract_view, write_html  # noqa: E402
from royalegym.replay import ReplayRecorder, Trace, save_trace, verify_trace  # noqa: E402
from royalegym.selfplay import RandomLegalOpponent  # noqa: E402

# A battle short enough to run in seconds and long enough that troops cross the
# river: 500 ms per decision, so 360 steps is 3 minutes of game time.
DEFAULT_STEPS = 360
# Floors for the vacuity gate.  Each is far below what a working battle produces
# (measured on the baseline below) and far above what a broken one does.
MIN_FRAMES = 50
MIN_TROOP_UIDS = 4
MIN_MOVED_SUBTILES = 1
PLANTS = ("desync", "frozen", "no_deploys", "stale_arena", "wet_troop", "torn_render")


class Skip(Exception):
    """A component this run needs is missing.  Never silently downgraded."""


@dataclass
class Gate:
    name: str
    ok: bool
    detail: str


@dataclass
class Report:
    gates: list[Gate] = field(default_factory=list)
    summary: dict[str, Any] = field(default_factory=dict)
    out: Path | None = None

    def add(self, name: str, ok: bool, detail: str) -> None:
        self.gates.append(Gate(name, ok, detail))

    @property
    def red(self) -> list[str]:
        return [g.name for g in self.gates if not g.ok]


def make_engine(kind: str) -> Any:
    if kind == "mock":
        return MockEngine()
    try:
        from royalegym.rust_engine import RustEngine
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise Skip(
            f"the Rust extension is not importable ({exc}). Build it from the repo root:\n"
            "  VIRTUAL_ENV=.venv maturin develop --release\n"
            "or run with --engine mock, which is a DIFFERENT simulator."
        ) from exc
    try:
        return RustEngine()
    except Exception as exc:  # pragma: no cover - environment dependent
        raise Skip(
            f"the Rust extension refused to start ({exc}). It refuses a build that is "
            "stale against calibration.json / arena.json / cards.json; rebuild it from "
            "the repo root with `VIRTUAL_ENV=.venv maturin develop --release`."
        ) from exc


def play(
    engine_kind: str, seed: int, steps: int, decision_ms: int, noop_prob: float
) -> tuple[Trace, dict[str, Any]]:
    """Play one battle and return its trace plus what the env saw happen."""
    engine = make_engine(engine_kind)
    rec = ReplayRecorder(frame_every_tick=True)
    env = ClashParallelEnv(engine=engine, decision_ms=decision_ms, recorder=rec)
    policy = RandomLegalOpponent(noop_prob=noop_prob)
    rng = np.random.default_rng(seed)
    obs, _ = env.reset(seed=seed)
    accepted = {"blue": 0, "red": 0}
    rejected = 0
    used = 0
    for _ in range(steps):
        if not env.agents:
            break
        acts = {a: policy.act(obs[a], obs[a]["action_mask"], rng) for a in env.agents}
        obs, _rew, term, trunc, infos = env.step(acts)
        used += 1
        for a, info in infos.items():
            st = info.get("deploy_status")
            if st is None or st < 0:
                continue
            if st == DeployStatus.OK:
                accepted[a] += 1
            else:
                rejected += 1
        if all(term.values()) or all(trunc.values()):
            break
    trace = rec.trace
    if trace is None:  # pragma: no cover - the recorder is always attached above
        raise Skip("the recorder produced no trace")
    if trace.result is None:
        # The battle ran out of steps before the match ended; close the trace so
        # verify_trace has a final hash to check.  This is the normal case.
        rec.end(env.engine)
    return trace, {
        "engine": engine_kind,
        "seed": seed,
        "steps": used,
        "decision_ms": decision_ms,
        "noop_prob": noop_prob,
        "accepted": accepted,
        "rejected": rejected,
    }


# --------------------------------------------------------------------------
# Plants.  Each mutates the loaded copy and asserts the mutation landed.
# --------------------------------------------------------------------------
def apply_plant(name: str, trace: Trace) -> Trace:
    before = msgspec.msgpack.encode(trace)
    t = msgspec.msgpack.decode(before, type=Trace)
    if name == "desync":
        i = len(t.frames) // 2
        h = t.frames[i].state_hash
        t.frames[i].state_hash = f"{(int(h, 16) ^ 1):016x}"
    elif name == "frozen":
        first = t.frames[0].entities
        for f in t.frames:
            f.entities = list(first)
    elif name == "stale_arena":
        g = t.header.grid
        hy = len(g) // 2
        hx = len(g[hy]) // 2
        g[hy][hx] ^= BIT_WATER
    elif name == "wet_troop":
        wet = _a_water_half_cell(t)
        if wet is None:
            raise Skip("plant wet_troop: the trace header has no water half-cell")
        hs = t.header.subtile // t.header.half
        x, y = wet[0] * hs + hs // 2, wet[1] * hs + hs // 2
        placed = False
        # under the shipped-law arm the gate is a 5 % bound, so the plant has to wet
        # EVERY ground troop on every frame to land; the old arm's gate trips on one
        every = _client16402_arm(t)
        for f in t.frames:
            for j, e in enumerate(f.entities):
                if e.kind == EntityKind.TROOP and not e.flying:
                    f.entities[j] = msgspec.structs.replace(e, x=x, y=y)
                    placed = True
                    if not every:
                        break
            if placed and not every:
                break
        if not placed:
            raise Skip("plant wet_troop: no ground troop in the trace to move")
    elif name in ("no_deploys", "torn_render"):
        return t  # applied elsewhere: at the policy, and between trace and page
    else:
        raise Skip(f"unknown plant {name!r}; known: {', '.join(PLANTS)}")
    after = msgspec.msgpack.encode(t)
    if after == before:
        raise Skip(f"plant {name} PATCHED NOTHING -- it would have graded a clean trace")
    return t


def _a_water_half_cell(t: Trace) -> tuple[int, int] | None:
    for hy, row in enumerate(t.header.grid):
        for hx, bits in enumerate(row):
            if bits & BIT_WATER:
                return hx, hy
    return None


# --------------------------------------------------------------------------
# Gates
# --------------------------------------------------------------------------
def gate_determinism(rep: Report, trace: Trace, engine_kind: str) -> None:
    problems = verify_trace(trace, make_engine(engine_kind))
    rep.add(
        "determinism",
        not problems,
        "re-simulated hash-for-hash on a fresh engine"
        if not problems
        else f"{len(problems)} divergence(s); first: {problems[0]}",
    )


def gate_vacuity(rep: Report, trace: Trace, played: dict[str, Any]) -> None:
    frames = trace.frames
    troops = {e.uid for f in frames for e in f.entities if e.kind == EntityKind.TROOP}
    first_pos: dict[int, tuple[int, int]] = {}
    moved = 0
    hp_drops = 0
    last_hp: dict[int, int] = {}
    for f in frames:
        for e in f.entities:
            p = first_pos.setdefault(e.uid, (e.x, e.y))
            moved = max(moved, abs(e.x - p[0]) + abs(e.y - p[1]))
            prev = last_hp.get(e.uid)
            if prev is not None and e.hp < prev:
                hp_drops += 1
            last_hp[e.uid] = e.hp
    checks = [
        (len(frames) >= MIN_FRAMES, f"{len(frames)} frames (floor {MIN_FRAMES})"),
        (played["accepted"]["blue"] > 0, f"blue accepted {played['accepted']['blue']} deploys"),
        (played["accepted"]["red"] > 0, f"red accepted {played['accepted']['red']} deploys"),
        (len(troops) >= MIN_TROOP_UIDS, f"{len(troops)} troops existed (floor {MIN_TROOP_UIDS})"),
        (moved >= MIN_MOVED_SUBTILES, f"largest displacement {moved} subtiles"),
        (hp_drops > 0, f"{hp_drops} hp drops"),
    ]
    rep.summary["troops"] = len(troops)
    rep.summary["hp_drops"] = hp_drops
    rep.summary["max_displacement_subtiles"] = moved
    bad = [d for ok, d in checks if not ok]
    rep.add(
        "vacuity",
        not bad,
        "; ".join(d for _, d in checks) if not bad else "EMPTY BATTLE: " + "; ".join(bad),
    )


def gate_arena(rep: Report, trace: Trace) -> None:
    live = Arena.load(Calibration.load())
    h = trace.header
    diffs = []
    for name, got, want in (
        ("subtile", h.subtile, live.subtile),
        ("half", h.half, live.half),
        ("tiles_x", h.tiles_x, live.tiles_x),
        ("tiles_y", h.tiles_y, live.tiles_y),
        ("water_half_rows", tuple(h.water_half_rows), tuple(live.water_half_rows)),
        (
            "bridges_half_cols",
            [tuple(b) for b in h.bridges_half_cols],
            [tuple(b) for b in live.bridges_half_cols],
        ),
    ):
        if got != want:
            diffs.append(f"{name}: trace {got} != arena.json {want}")
    cells = sum(
        1
        for hy, row in enumerate(h.grid)
        for hx, bits in enumerate(row)
        if hy < len(live.grid) and hx < len(live.grid[hy]) and bits != live.grid[hy][hx]
    )
    if len(h.grid) != len(live.grid) or any(
        len(a) != len(b) for a, b in zip(h.grid, live.grid, strict=False)
    ):
        diffs.append("grid shape differs")
    if cells:
        diffs.append(f"{cells} half-cell(s) differ")
    rep.add(
        "arena",
        not diffs,
        f"trace grid == data/derived/arena.json ({len(h.grid)}x{len(h.grid[0])} half-cells)"
        if not diffs
        else "; ".join(diffs),
    )


def gate_dry(rep: Report, trace: Trace) -> None:
    h = trace.header
    hs = h.subtile // h.half
    wet: list[str] = []
    for f in trace.frames:
        for e in f.entities:
            if e.flying:
                continue
            hx, hy = e.x // hs, e.y // hs
            if 0 <= hy < len(h.grid) and 0 <= hx < len(h.grid[hy]) and h.grid[hy][hx] & BIT_WATER:
                wet.append(f"tick {f.tick}: uid {e.uid} (card {e.card_id}) at ({e.x}, {e.y})")
                if len(wet) > 5:
                    break
        if len(wet) > 5:
            break
    checked = sum(1 for f in trace.frames for e in f.entities if not e.flying)
    rep.summary["ground_entity_positions_checked"] = checked
    if not checked:
        rep.add("dry", False, "VACUOUS: no ground entity position was checked")
        return
    if _client16402_arm(trace):
        wet_all = sum(
            1
            for f in trace.frames
            for e in f.entities
            if not e.flying and 0 <= e.y // hs < len(h.grid) and 0 <= e.x // hs < len(h.grid[e.y // hs]) and h.grid[e.y // hs][e.x // hs] & BIT_WATER
        )
        rep.summary["ground_entity_positions_wet"] = wet_all
        rep.add(
            "dry",
            wet_all * 20 <= checked,
            f"{wet_all} of {checked} ground entity positions on water (the live game allows it; bound 5 %)",
        )
        return
    rep.add(
        "dry",
        not wet,
        f"{checked} ground entity positions, none on water"
        if not wet
        else f"{len(wet)}+ on water; first: {wet[0]}",
    )


def _client16402_arm(trace: Trace) -> bool:
    """Is the trace from a battle under the shipped search? The ledger the engine
    was built with decides (calibration.json pathfinding.PATH_SEARCH)."""
    try:
        cal = json.loads((ROOT / "data" / "calibration.json").read_text(encoding="utf-8"))
        return cal["pathfinding"]["PATH_SEARCH"]["value"] == "client16402"
    except Exception:  # noqa: BLE001
        return False


def gate_render(rep: Report, trace: Trace, out: Path, stride: int, torn: bool) -> None:
    to_draw = trace
    if torn:
        before = msgspec.msgpack.encode(trace)
        to_draw = msgspec.msgpack.decode(before, type=Trace)
        to_draw.frames = to_draw.frames[:-1]
        if msgspec.msgpack.encode(to_draw) == before:
            raise Skip("plant torn_render PATCHED NOTHING")
    try:
        write_html(to_draw, out, title=f"clash-simulator seed {trace.header.seed}", stride=stride)
    except RenderError as exc:
        rep.add("render", False, f"the page could not be written: {exc}")
        return
    page = out.read_text(encoding="utf-8")
    view = extract_view(page)
    # build_view's own stride rule: every stride-th frame, and ALWAYS the last one.
    # Transcribed, not copied -- and then checked against the trace's own first and
    # last ticks, which is what a dropped frame moves.
    keep = list(range(0, len(trace.frames), stride))
    if keep[-1] != len(trace.frames) - 1:
        keep.append(len(trace.frames) - 1)
    frames = view["frames"]  # TraceFrame is array_like: frame[0] is the tick
    problems = []
    if len(frames) != len(keep):
        problems.append(f"page has {len(frames)} frames, the trace strides to {len(keep)}")
    if view["source"]["frames_recorded"] != len(trace.frames):
        problems.append(
            f"page says {view['source']['frames_recorded']} frames recorded, "
            f"the trace has {len(trace.frames)}"
        )
    if frames and frames[0][0] != trace.frames[0].tick:
        problems.append(f"page starts at tick {frames[0][0]}, the trace at {trace.frames[0].tick}")
    if frames and frames[-1][0] != trace.frames[-1].tick:
        problems.append(f"page ends at tick {frames[-1][0]}, the trace at {trace.frames[-1].tick}")
    external = [m for m in ("http://", "https://", "//cdn") if m in page]
    if external:
        problems.append(f"the page is not self-contained: {external}")
    rep.add(
        "render",
        not problems,
        f"{out.name}, {out.stat().st_size} bytes, {len(frames)} frames, self-contained"
        if not problems
        else "; ".join(problems),
    )
    rep.out = out


def run_once(args: argparse.Namespace, plant: str | None) -> Report:
    noop_prob = 1.0 if plant == "no_deploys" else args.noop_prob
    trace, played = play(args.engine, args.seed, args.steps, args.decision_ms, noop_prob)
    if plant in ("desync", "frozen", "stale_arena", "wet_troop"):
        trace = apply_plant(plant, trace)
    rep = Report()
    rep.summary.update(played)
    if trace.result is not None:
        rep.summary["winner"] = Winner(trace.result.winner).name
        rep.summary["crowns"] = trace.result.crowns
        rep.summary["final_tick"] = trace.result.final_tick
    gate_determinism(rep, trace, args.engine)
    gate_vacuity(rep, trace, {**played, "accepted": played["accepted"]})
    gate_arena(rep, trace)
    gate_dry(rep, trace)
    gate_render(rep, trace, args.out, args.stride, torn=plant == "torn_render")
    if args.trace_out:
        save_trace(trace, args.trace_out)
        rep.summary["trace"] = str(args.trace_out)
    return rep


AIMED_AT = {
    "desync": "determinism",
    "frozen": "vacuity",
    "no_deploys": "vacuity",
    "stale_arena": "arena",
    "wet_troop": "dry",
    "torn_render": "render",
}


def print_report(rep: Report, header: str) -> None:
    print(header)
    for k, v in rep.summary.items():
        print(f"    {k}: {v}")
    for g in rep.gates:
        print(f"  [{'OK ' if g.ok else 'RED'}] {g.name}: {g.detail}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        prog="python tools/watch_battle.py",
        description=__doc__.splitlines()[0],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("--engine", choices=("rust", "mock"), default="rust")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--steps", type=int, default=DEFAULT_STEPS)
    ap.add_argument("--decision-ms", type=int, default=500)
    ap.add_argument(
        "--noop-prob",
        type=float,
        default=0.55,
        help="per-step probability each seat plays nothing. NOT a policy: a smoke "
        "generator that says nothing about balance (default 0.55)",
    )
    ap.add_argument("--out", type=Path, default=ROOT / "battle.html")
    ap.add_argument("--trace-out", type=Path, default=None, help="also save the .msgpack trace")
    ap.add_argument("--stride", type=int, default=1, help="keep every Nth frame in the page")
    ap.add_argument("--open", action="store_true", help="open the page when every gate is green")
    ap.add_argument("--plant", choices=PLANTS)
    ap.add_argument("--all-plants", action="store_true")
    args = ap.parse_args(argv)
    if args.stride < 1 or args.steps < 1 or not 0.0 <= args.noop_prob <= 1.0:
        print("usage: --stride and --steps are >= 1, --noop-prob is in [0, 1]", file=sys.stderr)
        return 2

    try:
        base = run_once(args, None)
    except Skip as exc:
        print(f"SKIPPED: {exc}", file=sys.stderr)
        return 2
    print_report(base, f"BASELINE ({args.engine} engine)")
    if base.red:
        print(f"\nRED: {', '.join(base.red)}")
        return 1

    plants = list(PLANTS) if args.all_plants else ([args.plant] if args.plant else [])
    if plants and base.red:  # pragma: no cover - unreachable, kept as the stated rule
        print("a plant is only evidence on a green baseline", file=sys.stderr)
        return 1
    landed = 0
    for name in plants:
        try:
            rep = run_once(args, name)
        except Skip as exc:
            print(f"\nplant {name}: DID NOT LAND ({exc})")
            continue
        aim = AIMED_AT[name]
        hit = aim in rep.red
        # A plant that turns some OTHER gate red certifies nothing about the gate
        # it aims at (see the extract_arena.py first draft).
        stray = [g for g in rep.red if g != aim]
        why = next(g.detail for g in rep.gates if g.name == aim)
        if hit and not stray:
            landed += 1
            print(f"\nplant {name}: LANDED on {aim} -- {why}")
        elif hit:
            landed += 1
            print(f"\nplant {name}: landed on {aim} ({why});")
            print(f"  ALSO turned {stray} red -- consequences, check them")
        else:
            print(f"\nplant {name}: PLANT DID NOT LAND -- {aim} stayed green")
            print(f"  (red: {rep.red or 'none'})")
    if plants:
        print(f"\nplants: {landed}/{len(plants)} landed")
        return 0 if landed == len(plants) else 1

    print(f"\nEVERY GATE GREEN. Open {base.out} in a browser to watch it.")
    if args.open and base.out is not None:
        # A local file the caller explicitly asked to open; no shell, no argument.
        os.startfile(base.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
