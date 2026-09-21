# Working on the engine

This file is the development loop: how to build, every gate and how to run it, how the test
plants work, and the conventions the code follows. `README.md` has the one-time workspace setup.

## Build loop

The crate builds alone with cargo. Python callers need the extension module installed into the
workspace venv:

```
cd RoyaleSim
..\.venv\Scripts\maturin develop --release     # ~1 min, fat LTO, ~1.5 GB RAM
```

`maturin develop` installs into the active venv or, with none active, into a `.venv` found in the
current or a parent directory. A venv under any other name needs `VIRTUAL_ENV` pointing at it.

**Rebuild after data changes.** `data/calibration.json` and `data/derived/arena.json` are
compiled in with `include_str!`, and the env layer's `RustEngine` refuses a build whose
compiled-in **values** differ from the files on disk. After changing either file, run
`maturin develop --release` again. Prose fields of the ledger (provenance, notes, promotion
rules) are not compared, so a prose-only edit needs no rebuild.

Building needs about 1.5 GB of RAM; on a small machine run one release build at a time and
nothing else heavy beside it.

## Gates

```
cd crates\royalesim && cargo test --release             # 176 passed + 3 ignored (2026-09-21)
cd crates\royalesim && cargo clippy --all-targets -- -D warnings
cd crates\royalesim && grep -rn 'f32\|f64' src/ tests/  # must print nothing
..\.venv\Scripts\python -m pytest -q                     # 88 passed (2026-09-21), from the repo root
..\.venv\Scripts\ruff check tools oracle tests
cd ..\RoyaleGym && ..\.venv\Scripts\python -m pytest -q  # 235 (2026-09-21): the env layer drives the engine
```

`cargo test` also runs in debug; release is the one that matters, because release keeps overflow
checks on. After a comment-only edit to Rust, `cargo check --release` is enough; after a code
edit, run `cargo test --release` and then `maturin develop --release` so the venv's module matches
the source.

The env layer's suite is the second gate on this crate: it drives the engine through `RustEngine`
and holds the rotation and self-play checks.

Two more, neither of them a pass/fail gate:

```
cd crates\royalesim && cargo test --release --test throughput -- --ignored --nocapture   # timing
..\.venv\Scripts\python tools\watch_battle.py --open                                     # a battle you can watch
```

### Traces and generated fixtures

- `tools/oracle_diff.py` runs the engine beside a recorded trace, tick for tick. The walk family
  is bit-exact (6/6); see `pathfinding.md`.
- `tests/test_oracle_native_diff.py` **skips loudly** when `data/oracle-native/` is absent. A skip
  is not a pass.
- `tools/make_oracle2026_fixture.py --check` and `tools/make_client16402_paths_fixture.py --check`
  re-derive the generated path fixtures and report whether they are in sync. Regenerating either
  re-scores the gate that reads it, so it is a deliberate step and not a side effect of a build.

### `tools/watch_battle.py`

Plays a whole battle on `RustEngine` through the env layer, scores five gates over it, and writes
a self-contained `battle.html`. A 3-minute battle takes about 5 s end to end including the
re-simulation and the page. The five gates:

| Gate | What it checks |
|---|---|
| `determinism` | re-simulates the recorded trace on a fresh engine, hash for hash |
| `vacuity` | frames exist, both seats landed accepted deploys, troops existed, something moved, something lost hp |
| `arena` | the trace header's grid against the current `data/derived/arena.json` |
| `dry` | no non-flying entity centre is ever on a water half-cell (see the tolerance note below) |
| `render` | the page is written, self-contained, and its frame count and first/last tick match the trace |

It exits non-zero if any gate is red, so a green page is evidence rather than decoration. With no
usable extension module it prints `SKIPPED` and exits 2 — it never silently falls back to the
mock engine, which is a different simulator; `--engine mock` is explicit and opt-in.

Its policy is uniform-over-legal-actions, so it says **nothing** about balance. What it cannot
see is anything the trace does not record: troop projectiles, targets, attack timers and paths.

Under `collision.CONTACT_LAW = client16402` the `dry` gate runs against
`tests/common/mod.rs CLIENT16402_TOLERANCE`, because live units do stand on water cells — see
"Invariants the game does not have" in `mechanics.md`.

## Plants

A *plant* is a deliberate defect, compiled in behind a `cfg`, used to prove that a specific gate
can actually fail. They are compile-time `cfg`s rather than runtime flags, so a plant cannot leak
into a shipped build:

```
RUSTFLAGS='--cfg clash_plant="id_tiebreak"' CARGO_TARGET_DIR=target/plant cargo test --test mirror
```

Use a separate `CARGO_TARGET_DIR`, or the plant build poisons the normal one.

There are 86 of them as of 2026-09-21, each declared at the site it corrupts and named in the
header of the test it is aimed at. To list them:

```
grep -rho 'clash_plant *= *"[a-z_0-9]*"' src tests | sed 's/.*= *//' | tr -d '"' | sort -u
```

Two rules go with them, and both were learned the hard way:

- **A plant that lands on nothing is not a resting state.** If a plant stops failing its gate,
  retire it out loud in the test header, naming what killed it, and replace it — do not leave it
  in place looking like proof.
- **A checker that cannot fail is worse than no checker.** When a new gate is added, show the
  plant that reddens it *and* show that the existing gates stay green under that same plant;
  otherwise the new gate is certifying nothing the old ones did not already cover.

## Conventions

- **No floats** in `src/`. Integer arithmetic only.
- **Every constant the engine runs on is an entry in the ledger**, with the evidence that pins it.
  Adding a number to the code that belongs in `data/calibration.json` is the failure mode the
  ledger exists to prevent; `calibration.md` explains the format.
- **State rules as observable behaviour.** A claim in a comment, a doc or the ledger should be
  something a recording of the real game could show. Say what was measured, on which client
  version, and in which trace.
- **Comments carry facts and dates.** Match the surrounding style: the long-form `WHY / WHAT`
  headers on the Rust modules, sentence case in the ledger.
- Module and tool names are referenced from the docs and from the env layer; renaming one is a
  cross-repo change, not a local tidy-up.
- `data/raw/cr-*` (the decoded modern asset pack), `data/derived/` (generated) and
  `data/oracle-native/` (large traces) are gitignored and never committed.
  `data/raw/retroroyale-2018/` is tracked.

## Test layout

| Path | Covers |
|---|---|
| `crates/royalesim/src/**` unit tests | phase order, math, loaders |
| `crates/royalesim/tests/battle.rs` | scripted battles and the every-tick invariants |
| `tests/mirror.rs`, `setup_spawn_order.rs`, `stacked_tie.rs` | seat symmetry and tie-breaks |
| `tests/mechanics.rs`, `territory.rs`, `spells.rs`, `knockback.rs` | behaviour per mechanic |
| `tests/oracle2026.rs` | the path gates against recorded first paths (G6) |
| `tests/save_load.rs`, `api.rs` | snapshots and the Python-facing API |
| `tests/throughput.rs` | timing; `#[ignore]`d, not a gate |
| `tests/` (pytest, repo root) | the Python tooling in `tools/` and `oracle/` |
