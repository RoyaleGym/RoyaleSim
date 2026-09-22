# RoyaleSim

A Clash Royale battle engine in Rust. It runs the whole match — pathfinding, targeting, collision
and separation, combat, spells, elixir and the crown/overtime win conditions — in integer
arithmetic with no floats anywhere, deterministic to the bit, at tens of thousands of ticks a
second. It ships as one PyO3 extension module, `royalesim`, so a Python process drives battles
directly, with no game client anywhere in the loop.

What makes it more than another fan simulator is that its rules were **measured off recordings of
the real client** rather than guessed: the 16.402 pathfinder and its cost model, the contact and
avoidance law, the deploy rules, the arena. Every constant it runs on names the client version it
was measured on and the recording it was measured in (`data/calibration.json`, the ledger), and
where a rule is still unknown this README says so.

---

## Run a battle from Python

```python
import json, royalesim

deck = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Goblins"]
b = royalesim.Battle(card_names=deck, slot_of_k=[[0, 1, 2], [0, 1, 2]])
b.reset(seed=1, decks=[list(range(8))] * 2, shuffle=0, start_tick=0,
        elixir_milli=[10_000, 10_000], tower_hp=None, spawns=[])

T = royalesim.SUBTILE                          # 18000 subtiles to a tile
b.step([(0, 0, 5 * T, 10 * T)], 0)             # blue plays hand slot 0 (Giant) at tile (5, 10)

for _ in range(12):
    b.step([], 40)                             # 40 ticks = two seconds
    s = json.loads(bytes(b.state_json()))
    giant = next(e for e in s["entities"] if e[3] == 0)
    print(f"t={s['tick']:4d}  giant=({giant[5]/T:5.2f}, {giant[6]/T:5.2f})"
          f"  hp={giant[7]:4d}  red left tower={s['players'][1]['tower_hp'][1]}")
```

```
t=  40  giant=( 4.61, 10.83)  hp=3344  red left tower=2968
t=  80  giant=( 4.34, 12.56)  hp=3344  red left tower=2968
t= 120  giant=( 4.21, 14.45)  hp=3344  red left tower=2968
t= 160  giant=( 3.86, 16.15)  hp=3344  red left tower=2968
t= 200  giant=( 3.79, 17.94)  hp=3344  red left tower=2968
t= 240  giant=( 3.77, 19.82)  hp=3026  red left tower=2968
t= 280  giant=( 3.77, 21.64)  hp=2708  red left tower=2968
t= 320  giant=( 3.76, 23.30)  hp=2390  red left tower=2968
t= 360  giant=( 3.76, 23.30)  hp=2178  red left tower=2757
t= 400  giant=( 3.76, 23.30)  hp=1860  red left tower=2335
t= 440  giant=( 3.76, 23.30)  hp=1648  red left tower=2124
t= 480  giant=( 3.76, 23.30)  hp=1330  red left tower=1913
```

Nobody told the Giant where to walk. It planned its own route on the measured 16.402 search:
it slides left onto the bridge column, crosses the river around t=160, is under princess-tower
fire by t=240, halts at the tower's edge-to-edge range at t=320 and is taking the tower down by
t=360. Run it again with the same seed and you get the same numbers, on any machine, forever.

Deploy legality is a pure query, so a policy can build its action mask from the engine's own
answer instead of reimplementing the rules:

```python
import royalesim

deck = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Goblins"]
b = royalesim.Battle(card_names=deck, slot_of_k=[[0, 1, 2], [0, 1, 2]])
b.reset(seed=1, decks=[list(range(8))] * 2, shuffle=0, start_tick=0,
        elixir_milli=[10_000, 10_000], tower_hp=None, spawns=[])

T, R = royalesim.SUBTILE, royalesim.DEPLOY_REASONS
for label, (x, y) in [("own half", (5, 10)), ("the river", (9, 16)),
                      ("enemy half", (9, 25)), ("off the map", (40, 10))]:
    print(f"{label:12s} {R[b.check_deploy(0, 0, x * T, y * T)]}")   # team 0, hand slot 0
```

```
own half     OK
the river    WATER
enemy half   OUT_OF_TERRITORY
off the map  OUT_OF_ARENA
```

## Watch one

`tools/watch_battle.py` plays a full battle, scores five gates over it and writes a self-contained
HTML page you open in a browser. It is deliberately both the demo and a gate: a green page is
evidence, and the script exits non-zero if any gate is red.

```
$ ..\.venv\Scripts\python tools\watch_battle.py --seed 7 --steps 400 --noop-prob 0.2
BASELINE (rust engine)
    engine: rust
    seed: 7
    ...
    accepted: {'blue': 27, 'red': 32}
    rejected: 0
    winner: NONE
    crowns: [1, 1]
    final_tick: 4000
    troops: 119
    hp_drops: 335
  [OK ] determinism: re-simulated hash-for-hash on a fresh engine
  [OK ] vacuity: 4001 frames (floor 50); blue accepted 27 deploys; red accepted 32 deploys; 119 troops existed (floor 4); largest displacement 469674 subtiles; 335 hp drops
  [OK ] arena: trace grid == data/derived/arena.json (64x36 half-cells)
  [OK ] dry: 79 of 33504 ground entity positions on water (the live game allows it; bound 5 %)
  [OK ] render: battle.html, 2810551 bytes, 4001 frames, self-contained

EVERY GATE GREEN. Open battle.html in a browser to watch it.
```

That run took **2.5 s** end to end, including the full re-simulation and the 2.8 MB page
(2026-09-21, one core). The page shows both hands, both elixir bars, every unit, every tower's hp
and the per-tick state hash, and scrubs tick by tick. `--plant desync` (and five other plants)
deliberately breaks the battle to prove each gate can still go red.

## What it gets right, measured

| Rule | Score | Where |
|---|---|---|
| The 16.402 path search (one goal cell, x10/x14 integer costs, f-only heap, 8 directions, water priced) | corpus of **755 first paths** — 627 from recorded 16.402 battles, 128 from recorded walks. Three (units chasing a moving troop) are not comparable, because a recording cannot recover the goal cell; of the remaining 752 the engine reproduces **751 node for node** | gate G6, `crates/royalesim/tests/oracle2026.rs` |
| The contact law: separation, avoidance, the step, the Mass rule | **240,389 of 242,232 unit-tick positions exact (0.9924)**, replayed over every frame pair of all 31 recorded 16.402 battles. By situation: isolated 0.9937, separation active 0.9832, against buildings 0.9983, attacking 0.9928 | `docs/pathfinding.md` |
| Whole recorded walks, tick for tick | **6 of 6 bit-exact** (one of the six over 105 of its 107 ticks; `oracle_diff.py` names the shortfall) | `tools/oracle_diff.py` |
| Determinism | same seed, same battle, hash for hash; a recorded trace re-simulates on a fresh engine, frame hash for frame hash | `royalegym.replay.verify_trace`, the determinism gate above |
| Seat symmetry | 180-degree rotation checked every tick, under the frame-planned path arm | `crates/royalesim/tests/mirror.rs` |

The fit is not a free parameter, and `docs/pathfinding.md` keeps the controls that prove it:
take one rule out and the score collapses. Water made impassable scores 306 of 388 cases;
friendly-only occlusion 336 of 388; the earlier trace-fitted arm of 2026-09-18, 168 of 345 —
scored on the earlier 388- and 345-case corpora those controls were run against
(`docs/pathfinding.md` has both halves of each).
That is what makes the near-perfect score mean something.

**Speed.** `tools/throughput.py` runs five three-minute battles with both seats deploying at
random — 18,000 ticks, mean ~10 live entities — and prints the rate. On one core through the
Python surface it lands in the range **10,000-30,000 ticks/s**, so a 3-minute match costs a few
hundred milliseconds. The spread across runs on the same machine is wider than the difference
between decoding state every step and never (`--read-every 0`), so treat the figure your own run
prints as the one that matters. `docs/performance.md` has the older per-component table.
`docs/performance.md` has the full table, measured 2026-09-13 on the earlier path arms: up to
112,000 ticks/s on light boards, ~1 us per entity-tick, and save/load in ~140/~150 us.

## What is in the box

- **65 cards load** — 52 troops, 6 buildings, 7 spells (Fireball, Arrows, Zap, The Log and the
  Goblin Barrel are modelled and tested; Rocket and Freeze load because their data has an
  implemented shape, but no test covers them).
- **Towers and match flow**: king and princess towers, king activation, regular time, 60 s sudden
  death overtime, the 3-crown instant win, double elixir, hand and cycle.
- **Targeting** with edge-to-edge range, target lock once windup starts, keep-target hysteresis,
  building-only targeters and sight range; projectiles, splash, shields, death damage, lifetime.
- **Deploy rules as data**: the alive-enemy-tower no-deploy rects, water, the arena bitmask and
  occupancy, all queryable (`check_deploy`, `tower_no_deploy_rects`, `passable_half_cells`) so a
  learner's action mask is the engine's own answer.
- **Snapshots**: `save()` / `load()` round-trip a battle to a byte string and back to the
  identical state hash — roughly 7-11 KB for a mid-game board, depending on what is on it.
- **One Rust call per env step.** `step(commands, ticks)` validates, applies and ticks without
  returning to Python, with the GIL released, so a threaded vectorised env is not serialised by
  it. Bulk state crosses as one JSON byte string that msgspec decodes with a typed C decoder.

## What it does not do yet

A simulator is only useful if it is honest about its coverage, so:

- **Card numbers are ~2018 vintage by design** (`data/raw/retroroyale-2018/`). Movement and
  pathfinding are measured against the 2026 client; card stats are not.
- **The checked set is an 18-card thin slice** (`cards.json`, key `thin_slice`). The whole 65-card
  catalogue loads, but **19 of those 65 carry a mechanic the card loader never parses** — draw
  decks from `thin_slice`, not from the catalogue.
- **Not modelled**: charge (a Prince plays as a plain melee unit), hide (a Tesla plays as an
  always-up building), death spawn and periodic spawners, dash/morph/jump, air units' full
  behaviour, status effects other than Zap's stun, evolutions, champions and tower troops.
- **The post-overtime tiebreak is missing**: a match that survives overtime is scored a Draw
  rather than by lowest tower hp.
- **Two known collide-layer defects**, both measured and reproducible in ordinary play: a unit can
  sit inside a building footprint for up to 47 ticks (142 of 146 measured violations begin on the
  spawn tick), and push-outs from several obstacles are summed rather than selected.
- **The target is behavioural fidelity plus bit-identical determinism within a build**, not a
  tick-for-tick reproduction of a real match. The real intra-tick order and the real PRNG are out
  of reach, and are not a goal.

`docs/mechanics.md` is the full inventory, with the measurement behind each row and the repros for
both defects.

---

## Where it sits

RoyaleSim is the engine at the bottom of a five-repo stack. It knows nothing about rewards,
observations or training; those belong to the layers above.

| Repo | What it is to the engine |
|---|---|
| **RoyaleSim** | this repo |
| [RoyaleGym](https://github.com/RoyaleGym/RoyaleGym) | the environment API bot creators write against. Its `royalegym.rust_engine.RustEngine` wraps the `royalesim` module; its `royalegym.protocol` reads `data/` (arena, cards, calibration); its tests are the second gate on this crate. The family's front door: the overview lives in its README |
| [RoyaleLearn](https://github.com/RoyaleGym/RoyaleLearn) | the learner; reaches the engine only through RoyaleGym |
| [RoyaleViser](https://github.com/RoyaleGym/RoyaleViser) | the out-of-process viewer; engine traces and streams reach it through RoyaleGym |
| RoyaleLive (private, not published) | the client instrument that records the ground-truth traces this engine is calibrated against. It reads this repo's `data/`; nothing here imports it |

Dependency direction: `RoyaleLearn -> RoyaleGym -> RoyaleSim`; `RoyaleLive -> RoyaleSim data`.
This repo has no Python dependency on any sibling: the crate builds alone, and the calibration
tooling in `tools/` and `oracle/` needs only numpy/msgspec (plus `royalegym` for
`tools/watch_battle.py` and `tools/oracle_diff.py`, which drive the engine through the env
layer's `RustEngine`).

If you know RLGym and RocketSim, that layering will look familiar — environment API over engine,
learner on top, viewer out of process — and it is borrowed from that prior art on purpose.

## Setup: the shared workspace

Clone the siblings into one folder and build one venv at that folder's root. Python 3.12;
Rust 1.80+ with cargo. The engine is built first because everything else imports it.

```
mkdir Royale && cd Royale
git clone https://github.com/RoyaleGym/RoyaleSim.git
git clone https://github.com/RoyaleGym/RoyaleGym.git
git clone https://github.com/RoyaleGym/RoyaleViser.git
git clone https://github.com/RoyaleGym/RoyaleLearn.git
python -m venv .venv
.venv\Scripts\python -m pip install maturin pytest hypothesis ruff
cd RoyaleSim && ..\.venv\Scripts\python tools\extract_arena.py && ..\.venv\Scripts\python tools\extract_cards.py && ..\.venv\Scripts\python tools\extract_globals.py && cd ..   # 0. data/derived/ (gitignored; the crate compiles arena.json in)
cd RoyaleSim && ..\.venv\Scripts\maturin develop --release && cd ..   # 1. this repo: builds royalesim into the venv (~1 min, fat LTO, ~1.5 GB RAM)
.venv\Scripts\python -m pip install -e RoyaleGym                       # 2. the env layer: pulls numpy, gymnasium, pettingzoo, msgspec
.venv\Scripts\python -m pip install -e RoyaleViser                     # 3. the viewer: pulls pygame
.venv\Scripts\python -m pip install -e RoyaleLearn                     # 4. the learner
```

Only steps 0 and 1 are needed for the Python examples at the top of this file; `watch_battle.py`
also needs step 2. RoyaleLive is private, has no package yet (its scripts run from its own
folder), and is not needed for anything in this recipe.

`data/derived/` is gitignored and generated (step 0, before the build: the crate `include_str!`s
`arena.json`, and `royalegym` reads `cards.json` and `globals.json`): `tools/extract_arena.py`,
`extract_cards.py`, `extract_globals.py` read the tracked `data/raw/retroroyale-2018/` and need
nothing beyond the standard library (verified byte-identical from a fresh venv, 2026-09-21).
`data/raw/cr-15.535.29/` comes from `tools/decode_sc_assets.py` on a verified asset pack
(Supercell's files, not redistributed). The recorded traces in `data/oracle-native/` are not
distributed; without them `tests/test_oracle_native_diff.py` skips and says so.

`maturin develop` installs into the active venv or, with none active, into a `.venv` folder found
in the current or a parent directory: that is how `..\.venv\Scripts\maturin` from this folder
lands in the workspace venv. A venv under any other name needs `VIRTUAL_ENV` set to it (measured
2026-09-21: from `.venv2\Scripts\maturin` the build went into the sibling `.venv`).

**Rebuild after data changes.** The crate compiles `data/calibration.json` and
`data/derived/arena.json` in (`include_str!`). RoyaleGym's `RustEngine` compares the compiled-in
values with the files on disk and refuses a stale build, so after touching either file run
`maturin develop --release` again. Prose fields of the ledger (provenance, notes) are not
compared; only values are. The build needs ~1.5 GB RAM.

## Layout

```
crates/royalesim/        the Rust crate: lib + PyO3 module, both named royalesim
  src/                   state.rs (the tick), path*.rs (the pathfinders), move16402.rs (the 16.402
                         contact law), collide.rs, combat.rs, spell.rs, target.rs, card.rs, arena.rs,
                         fixed.rs (integer math), py.rs (the Python surface)
  tests/                 cargo integration tests; fixtures/oracle2026/ (first paths, generated)
data/
  calibration.json       THE LEDGER: every constant with value, status, confidence, provenance
  raw/retroroyale-2018/  vendored ~2018 csv_logic and tilemaps (Supercell's content, not MIT)
  raw/cr-15.535.29/      decoded modern csv_logic / tilemaps (gitignored; tools/decode_sc_assets.py)
  derived/               arena.json, cards.json, globals.json, ... (gitignored; tools/extract_*.py)
  oracle-native/         the recorded 15.535 traces (gitignored, not distributed)
oracle/                  the trace format and the calibration protocol: scenarios.json (the
                         discriminating scenarios), calibrate.py, synth.py,
                         extract_tracks.py (video tracks; cv2 optional)
tools/                   extract_*.py (data/raw -> data/derived), check_data.py, oracle_diff.py (the
                         engine beside a trace, tick for tick), diff_harness.py, watch_battle.py,
                         make_*_fixture.py, mechanic_register.py, decode_sc_assets.py
tests/                   pytest for the tooling (6 files); tests/test_oracle_native_diff.py skips
                         loudly without data/oracle-native
docs/                    architecture, contributing, calibration, mechanics, pathfinding,
                         pathfinder-spec, movement-measurements, spell-spec, performance
```

The data directory is found by the env layer through `royalegym.protocol.data_dir()`
(`../RoyaleSim/data` from a sibling checkout, overridable with `ROYALESIM_DATA_DIR`); RoyaleLive's
scripts use the same variable. `tools/make_client16402_paths_fixture.py` imports RoyaleLive's
sampler from `ROYALELIVE_DIR` (default `../RoyaleLive`) and reads its recordings from
`ROYALELIVE_REPORTS` (default `<ROYALELIVE_DIR>/reports`).

## Tests

```
cd RoyaleSim\crates\royalesim && cargo test --release     # 176 passed + 3 ignored (2026-09-21, from a fresh venv)
cd RoyaleSim && ..\.venv\Scripts\python -m pytest -q       # 88 passed (2026-09-21)
```

The second gate on this crate is RoyaleGym's suite (`cd RoyaleGym && ..\.venv\Scripts\python -m
pytest -q`, 235 on 2026-09-21), which drives the engine through `RustEngine`, and the rotation /
self-play gates there. `tools/oracle_diff.py` diffs the engine against a recorded trace
(6/6 walks bit-exact); `tools/watch_battle.py` renders and sanity-checks a battle.

## Status (2026-09-21)

- `path2026.rs` + `path16402.rs`: the measured 16.402 search (one goal cell, x10/x14 integer
  costs, f-only binary heap, N S W E NW SW SE NE, water priced and pushed), selected by the ledger
  key `pathfinding.PATH_SEARCH = client16402`; of the 755-case corpus, every published node
  list is reproduced exactly except one, with three skipped (units chasing a moving troop,
  whose goal cell a trace cannot recover) -- gate G6, `tests/oracle2026.rs`. The trace-fitted
  arm (`trace_fitted_astar`) stays runnable and is what the seat-symmetry gates run under.
- `move16402.rs` + `state.rs phase_path16402`: the measured contact law (separation, avoidance,
  the step, the Mass rule), `collision.CONTACT_LAW = client16402`; 240 389 of
  242 232 live unit-tick positions exact.
- Open: the A* expansion order among equally cheap routes (see the named G6 divergence below),
  knockback (recorded but not yet modelled), the buff tags, air units, the JumpEnabled water hop,
  attack-before-move tick order, the mechanic gap table (charge, spawner, death spawn, hide,
  evolutions), and `data/derived/cards.json` still being
  the 2018 data for Range on some cards. `docs/pathfinding.md` and `docs/mechanics.md` carry
  that list with the measurement behind each item.
- Known defects, both in the collide layer, both measured and reproducible in ordinary play: a
  unit can sit inside a building footprint for up to 47 ticks, and push-outs from several
  obstacles are summed rather than selected. `docs/mechanics.md` has the numbers and the repros.
- One published sequence is not reproduced, and it is named in gate G6 so a second one cannot
  hide behind it: `auto-20260920-072831-A:8:Giant`, where the engine walks the lane straight
  and the client drifts a column sideways. Both reach the goal; which of the lane routes is
  taken is the open A* expansion-order item above.
- The generated fixture `tests/fixtures/oracle2026/client16402_first_paths.json` holds all 755
  cases of the corpus as of 2026-09-21 (`tools/make_client16402_paths_fixture.py --check`
  reports it in sync). Regenerating it re-scores G6 and is a deliberate step, not a side effect.

Read next: [`docs/architecture.md`](docs/architecture.md) (how the engine is built),
[`docs/contributing.md`](docs/contributing.md) (the build loop and every gate),
[`docs/pathfinding.md`](docs/pathfinding.md) (the measured 16.402 pathfinder and contact law),
[`docs/mechanics.md`](docs/mechanics.md) (what is modelled, what is not, and the known defects).
[`docs/README.md`](docs/README.md) indexes the rest.

## Community

Engine questions, calibration evidence and pathfinding work happen in the project's Discord:
[**https://discord.gg/4D2BS5JBHP**](https://discord.gg/4D2BS5JBHP)

Issues and pull requests on this repo are welcome too.
