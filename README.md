# RoyaleSim

The RocketSim analog of the Royale family: a deterministic, integer-only Clash Royale battle
engine in Rust (pathfinding, targeting, collision, combat, spells, elixir, win conditions),
bit-exact against the real client where it has been measured, exposed to Python as one PyO3
extension module, `royalesim`. It knows nothing about rewards, observations or training: those
are RoyaleGym's and RoyaleLearn's. What it does know is the game, and every constant it runs on
carries the client version it was measured on and the trace it was measured in
(`data/calibration.json`, the ledger).

## Where it sits in the family

Five sibling repos, one workspace folder, one venv (Setup below):

| Repo | Analog | What it is to the engine |
|---|---|---|
| **RoyaleSim** | RocketSim | this repo |
| [RoyaleGym](https://github.com/RoyaleGym/RoyaleGym) | RLGym | the env API. Its `royalegym.rust_engine.RustEngine` wraps the `royalesim` module; its `royalegym.protocol` reads `data/` (arena, cards, calibration); its tests are the second gate on this crate. The family's front door: the overview lives in its README |
| [RoyaleLearn](https://github.com/RoyaleGym/RoyaleLearn) | RLGym-PPO | the learner; reaches the engine only through RoyaleGym |
| [RoyaleViser](https://github.com/RoyaleGym/RoyaleViser) | rlviser | the out-of-process viewer; engine traces and streams reach it through RoyaleGym |
| RoyaleLive (private, not published) | (the client instrument) | the ground truth: the client instrument that records ground-truth traces from the real game. It reads this repo's `data/`; nothing here imports it |

Dependency direction: `RoyaleLearn -> RoyaleGym -> RoyaleSim`; `RoyaleLive -> RoyaleSim data`.
This repo has no Python dependency on any sibling: the crate builds alone, and the calibration
tooling in `tools/` and `oracle/` needs only numpy/msgspec (plus `royalegym` for
`tools/watch_battle.py` and `tools/oracle_diff.py`, which drive the engine through the env
layer's `RustEngine`).

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
  oracle-native/         the offline 15.535 traces (gitignored; RoyaleLive's recordings)
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
scripts use the same variable. `tools/make_client16402_paths_fixture.py` imports RoyaleLive's sampler
from `ROYALELIVE_DIR` (default `../RoyaleLive`) and reads captures from `ROYALELIVE_REPORTS`
(default `<ROYALELIVE_DIR>/reports`).

## Setup: the shared workspace

Clone the five siblings into one folder and build one venv at that folder's root. Python 3.12;
Rust 1.80+ with cargo. The engine is built first because everything else imports it.

```
mkdir Royale && cd Royale
git clone https://github.com/RoyaleGym/RoyaleSim.git
git clone https://github.com/RoyaleGym/RoyaleGym.git
git clone https://github.com/RoyaleGym/RoyaleViser.git
# RoyaleLive (private) is not needed for anything in this recipe.
git clone https://github.com/RoyaleGym/RoyaleLearn.git
python -m venv .venv
.venv\Scripts\python -m pip install maturin pytest hypothesis ruff
cd RoyaleSim && ..\.venv\Scripts\python tools\extract_arena.py && ..\.venv\Scripts\python tools\extract_cards.py && ..\.venv\Scripts\python tools\extract_globals.py && cd ..   # 0. data/derived/ (gitignored; the crate compiles arena.json in)
cd RoyaleSim && ..\.venv\Scripts\maturin develop --release && cd ..   # 1. this repo: builds royalesim into the venv (~1 min, fat LTO, ~1.5 GB RAM)
.venv\Scripts\python -m pip install -e RoyaleGym                       # 2. the env layer: pulls numpy, gymnasium, pettingzoo, msgspec
.venv\Scripts\python -m pip install -e RoyaleViser                     # 3. the viewer: pulls pygame
                                                                       # 4. RoyaleLive (private): scripts run from its folder, no package yet
.venv\Scripts\python -m pip install -e RoyaleLearn                     # 5. the learner
```

`data/derived/` is gitignored and generated (step 0, before the build: the crate `include_str!`s
`arena.json`, and `royalegym` reads `cards.json` and `globals.json`): `tools/extract_arena.py`,
`extract_cards.py`, `extract_globals.py` read the tracked `data/raw/retroroyale-2018/` and need
nothing beyond the standard library (verified byte-identical from a fresh venv, 2026-09-21).
`data/raw/cr-15.535.29/` comes from `tools/decode_sc_assets.py` on a verified asset pack
(Supercell's files, not redistributed). The offline traces in `data/oracle-native/` are
RoyaleLive's; without them `tests/test_oracle_native_diff.py` skips and says so.

`maturin develop` installs into the active venv or, with none active, into a `.venv` folder found
in the current or a parent directory: that is how `..\.venv\Scripts\maturin` from this folder
lands in the workspace venv. A venv under any other name needs `VIRTUAL_ENV` set to it (measured
2026-09-21: from `.venv2\Scripts\maturin` the build went into the sibling `.venv`).

**Rebuild after data changes.** The crate compiles `data/calibration.json` and
`data/derived/arena.json` in (`include_str!`). RoyaleGym's `RustEngine` compares the compiled-in
values with the files on disk and refuses a stale build, so after touching either file run
`maturin develop --release` again. Prose fields of the ledger (provenance, notes) are not
compared; only values are. The build needs ~1.5 GB RAM.

## Tests

```
cd RoyaleSim\crates\royalesim && cargo test --release     # 176 passed + 3 ignored (2026-09-21, from a fresh venv)
cd RoyaleSim && ..\.venv\Scripts\python -m pytest -q       # 88 passed (2026-09-21)
```

The second gate on this crate is RoyaleGym's suite (`cd RoyaleGym && ..\.venv\Scripts\python -m
pytest -q`, 235 on 2026-09-21), which drives the engine through `RustEngine`, and the rotation /
self-play gates there. `tools/oracle_diff.py` diffs the engine against an offline trace
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
  242 232 live unit-tick positions exact (RoyaleLive's verifier).
- Open: the A* expansion order among equally cheap routes (see the named G6 divergence below),
  knockback (RoyaleLive's finding, not yet modelled), the buff tags, air units, the JumpEnabled water hop,
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
