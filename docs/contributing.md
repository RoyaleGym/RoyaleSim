# Working on the engine

This page is for anyone changing the engine. It is the development loop: how to build, every gate
and how to run it, how the test plants work, and the conventions the code follows. `README.md` has
the one-time workspace setup.

The commands here are written for Windows PowerShell, which is the default shell on the platform
most of this was built on. On macOS or Linux, swap the backslashes for forward slashes and
`..\.venv\Scripts\` for `../.venv/bin/`.

## Build loop

The crate builds alone with cargo. Python callers need the extension module installed into the
workspace venv:

```
cd RoyaleSim
..\.venv\Scripts\maturin develop --release
```

Fat LTO, so it is not quick. Measured from a fresh clone with no `target/`: 160 seconds cold, 88
with a warm cargo registry cache, and the build processes peaked under a gigabyte. A rebuild after
touching one file takes longer than either, because that command also runs the Rust suite.

`maturin develop` installs into the active venv. With no venv active, it installs into a `.venv`
found in the current or a parent directory. A venv under any other name needs `VIRTUAL_ENV`
pointing at it.

**Rebuild after data changes.** `data/calibration.json` and `data/derived/arena.json` are
compiled in with `include_str!`, and the env layer's `RustEngine` refuses a build whose
compiled-in **values** differ from the files on disk. After changing either file, run
`maturin develop --release` again. Prose fields of the ledger (provenance, notes, promotion
rules) are not compared, so a prose-only edit needs no rebuild.

Building needs about 1.5 GB of RAM; on a small machine run one release build at a time and
nothing else heavy beside it.

This repo has no Python dependency on any sibling: the crate builds alone, and the calibration
tooling in `tools/` and `oracle/` needs only numpy and msgspec. The two exceptions are
`tools/watch_battle.py` and `tools/oracle_diff.py`, which drive the engine through the env layer's
`RustEngine` and so need `royalegym` installed (`pip install -e ../RoyaleGym`).

### Data

`data/derived/` (`arena.json`, `cards.json`, `globals.json`) is gitignored and generated, and it
must exist **before** the build: the crate `include_str!`s `arena.json`, and `royalegym` reads
`cards.json` and `globals.json`. `tools/extract_arena.py` and `extract_globals.py` read the tracked
`data/raw/retroroyale-2018/` and need nothing beyond the standard library.
`tools/extract_cards.py` defaults to the 15.535.29 card table, which needs
`data/raw/cr-15.535.29/`. That directory comes from `tools/decode_sc_assets.py` run on a verified
asset pack (Supercell's files, not redistributed). A checkout without that pack generates the card
table from the tracked 2018 files instead:

```
python tools\extract_cards.py --vintage 2018
python tools\extract_cards.py --vintage 2018 --out data\derived\cards.json
```

The first writes `data\derived\cards-2018.json`; the second writes the same table over
`data\derived\cards.json`, which is the file the engine loads.

Both runs are needed: the engine reads `cards.json`, and `tests/stacked_tie.rs` loads the same
table again by its vintage name, refusing (never skipping) when it is absent. A 2018-only
checkout cannot score the three checks that are about the 15.535 table itself:
`tests/levels.rs` (the level ladder against recorded `max_hp`), `tests/jump16402.rs` (the jump
blocks of the Hog Rider, Prince and Dark Prince, which the 2018 columns give to the Hog alone) and
`tools/check_data.py`'s live-level rows, which report themselves vacuous. Those three want the
15.535 pack and `extract_cards.py` with no `--vintage`; the rest of `cargo test --release` does not
care which vintage is loaded.

The recorded traces in `data/oracle-native/` are not distributed; without them
`tests/test_oracle_native_diff.py` skips and says so.

The env layer finds the data directory through `royalegym.protocol.data_dir()`
(`../RoyaleSim/data` from a sibling checkout, overridable with `ROYALESIM_DATA_DIR`).
`tools/make_client16402_paths_fixture.py`, `make_client16402_jump_fixture.py`,
`make_live_levels_fixture.py` and `make_replay_fixture.py` read the recordings from the folder
named by `ROYALELIVE_REPORTS`; without it they exit and say so.

## Gates

```
cd crates\royalesim
cargo test --release
cargo clippy --all-targets -- -D warnings
cd ..\..
..\.venv\Scripts\python -m pytest -q
..\.venv\Scripts\ruff check tools oracle tests
..\.venv\Scripts\python tools\check_data.py
..\.venv\Scripts\python tools\check_card_reads.py
cd ..\RoyaleGym
..\.venv\Scripts\python -m pytest -q
```

In order: the Rust suite, its lint, the Python suite from the repo root, the Python lint, whether
the card data is what the client ships, whether the engine reads what the cards carry, and the env
layer driving the engine. No counts are quoted here, because a number typed into prose is stale the
moment the suite moves; each command prints its own.

The integer-only rule used to be a `grep` on this page, which is not a gate, because nobody runs a
page. `tests/test_no_floats.py` holds it now, over `src/`, `tests/`, `examples/` and `build.rs`.

`cargo test` also runs in debug; release is the one that matters, because release keeps overflow
checks on. After a comment-only edit to Rust, `cargo check --release` is enough; after a code
edit, run `cargo test --release` and then `maturin develop --release` so the venv's module matches
the source.

The env layer's suite is the second gate on this crate: it drives the engine through `RustEngine`
and holds the rotation and self-play checks.

Two more, neither of them a pass/fail gate:

```
cd crates\royalesim
cargo test --release --test throughput -- --ignored --nocapture
cd ..\..
..\.venv\Scripts\python tools\watch_battle.py --open
```

The first is the timing run. The second opens a battle you can watch.

### `tools/check_card_reads.py`

The card gate asks one question the data gate does not: of the columns that reach `cards.json`,
which ones does `card.rs` read? A column the loader has no field for is dropped in silence, so the
card plays as a plainer card with the same name.

It works out the answer rather than being told it. It reads the `Raw*` structs out of `card.rs` to
get the keys the loader takes; it walks `norm_unit` in `extract_cards.py` with `ast` to get which
card-table column becomes which key; and it reads `units[*].raw` in `cards.json` to get the columns
each row ships. A column is unread when that chain ends nowhere.

It **fails** when a thin-slice card carries an unread mechanic, and **reports** per card for the
rest of the catalogue. Thin-slice gaps that are open today are listed in the tool by name, with
what the engine does instead; the gate also fails when one of those entries goes stale.

It needs `cards.json` and `card.rs` and nothing else. `--cards data/derived/cards-2018.json` scores
the 2018 table. Two passes are optional and each says out loud when it is skipped: the engine's own
catalogue needs the built extension module, and the per-object pass needs
`data/derived/mechanic_register.json`. A skip is not a pass.

`--all-plants` runs four plants and reports whether each one still reddens the gate.
`tests/test_card_reads.py` drives all of it.

### Traces and generated fixtures

- `tools/oracle_diff.py` runs the engine beside a recorded trace, tick for tick. The walk family
  is bit-exact (6/6); see `pathfinding.md`.
- `tests/test_oracle_native_diff.py` **skips loudly** when `data/oracle-native/` is absent. A skip
  is not a pass.
- `tools/make_oracle2026_fixture.py --check` and `tools/make_client16402_paths_fixture.py --check`
  re-derive the generated path fixtures and report whether they are in sync. Regenerating either
  re-scores the gate that reads it, so it is a deliberate step and not a side effect of a build.
- The other four makers need inputs a plain checkout does not have, and `--check` reports a missing
  input rather than a stale fixture: `make_client16402_jump_fixture.py`, `make_replay_fixture.py`
  and `make_formation_fixture.py` need the recordings under `ROYALELIVE_REPORTS`, and
  `make_live_levels_fixture.py` needs those **and** the 15.535 pack, because it resolves card ids
  against that pack's `spells_*.csv` row order. Their committed fixtures stand on their own; only
  re-deriving them needs the inputs.
- The replay sample `crates/royalesim/tests/fixtures/replay/sample.json` is the recording
  `20260920-003751-B`, cut at tick 1440. This line checks that it is current:

  ```
  ROYALELIVE_REPORTS=<the recordings folder> python tools/make_replay_fixture.py 20260920-003751-B --until-tick 1440 --check crates/royalesim/tests/fixtures/replay/sample.json
  ```

  It prints `is current` or `STALE`. To rebuild the sample, put `--out <dir>` in place of
  `--check ...`, then copy `<dir>/20260920-003751-B.replay.json` over `sample.json`.
  `tests/test_replay_fixture.py` runs the same check. Without `ROYALELIVE_REPORTS` it skips, and
  a skip is not a pass.

### `tools/watch_battle.py`

Plays a whole battle on `RustEngine` through the env layer, scores five gates over it, and writes
a self-contained `battle.html`. A 3-minute battle (`--seed 7 --steps 400 --noop-prob 0.2`) took
1.4 s end to end on 2026-09-21 including the re-simulation and the 2.8 MB, 4001-frame page (about
5 s on the 2026-09-13 machine). `--trace-out battle.msgpack` also saves the trace, which
RoyaleViser plays (`python -m royaleviser battle.msgpack`). The five gates:

| Gate | What it checks |
|---|---|
| `determinism` | re-simulates the recorded trace on a fresh engine, hash for hash |
| `vacuity` | frames exist, both seats landed accepted deploys, troops existed, something moved, something lost hp |
| `arena` | the trace header's grid against the current `data/derived/arena.json` |
| `dry` | no non-flying entity centre is ever on a water half-cell (see the tolerance note below) |
| `render` | the page is written, self-contained, and its frame count and first/last tick match the trace |

It exits non-zero if any gate is red, so a green page is evidence rather than decoration.
`--plant desync` (and five other plants: `--all-plants` runs them all) deliberately breaks the
battle to prove each gate can still go red. With no usable extension module it prints `SKIPPED`
and exits 2. It never silently falls back to the mock engine, which is a different simulator;
`--engine mock` is explicit and opt-in.

Its policy is uniform-over-legal-actions: it picks each legal action as often as any other. So it
says **nothing** about balance. What it cannot see is anything the trace does not record: troop
projectiles, targets, attack timers and paths.

Under `collision.CONTACT_LAW = client16402` the `dry` gate runs against
`tests/common/mod.rs CLIENT16402_TOLERANCE`, because live units do stand on water cells. See
"Invariants the game does not have" in `mechanics.md`.

## Plants

A *plant* is a deliberate defect, compiled in behind a `cfg`, used to prove that a specific gate
can actually fail. They are compile-time `cfg`s rather than runtime flags, so a plant cannot leak
into a shipped build:

```
RUSTFLAGS='--cfg clash_plant="id_tiebreak"' CARGO_TARGET_DIR=target/plant cargo test --test mirror
```

Use a separate `CARGO_TARGET_DIR`, or the plant build poisons the normal one.

There are 155 of them, each declared at the site it corrupts and named in the header of the test
it is aimed at. To list them:

```
grep -rho 'clash_plant *= *"[a-z_0-9]*"' src tests | sed 's/.*= *//' | tr -d '"' | sort -u
```

Two rules go with them, and both were learned the hard way:

- **A plant that lands on nothing is not a resting state.** If a plant stops failing its gate,
  retire it out loud in the test header, naming what killed it, and replace it. Do not leave it
  in place looking like proof.
- **A checker that cannot fail is worse than no checker.** When you add a new gate, show the
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
- The docs and the env layer reference module and tool names, so renaming one is a cross-repo
  change, not a local tidy-up.
- `data/raw/cr-*` (the decoded modern asset pack), `data/derived/` (generated) and
  `data/oracle-native/` (large traces) are gitignored and never committed.
  `data/raw/retroroyale-2018/` is tracked.

## Repository layout

| Path | What it holds |
|---|---|
| `crates/royalesim/` | the Rust crate: lib + PyO3 module, both named `royalesim`. `src/` is mapped in `architecture.md`; `tests/` are the cargo integration tests, with `fixtures/oracle2026/` (first paths, generated) |
| `data/calibration.json` | the ledger: every constant with value, status, confidence, provenance (`calibration.md`) |
| `data/raw/retroroyale-2018/` | vendored ~2018 csv_logic and tilemaps (Supercell's content, not MIT; tracked) |
| `data/raw/cr-15.535.29/` | decoded modern csv_logic / tilemaps (gitignored; `tools/decode_sc_assets.py`) |
| `data/derived/` | `arena.json`, `cards.json`, `globals.json` (gitignored; `tools/extract_*.py`) |
| `data/oracle-native/` | the recorded 15.535 traces (gitignored, not distributed) |
| `oracle/` | the trace format and the calibration protocol: `scenarios.json` (the discriminating scenarios), `calibrate.py`, `synth.py`, `extract_tracks.py` (video tracks; cv2 optional) |
| `tools/` | `extract_*.py` (data/raw -> data/derived), `check_data.py`, `check_card_reads.py`, `oracle_diff.py` (the engine beside a trace, tick for tick), `diff_harness.py`, `watch_battle.py`, `throughput.py`, `make_*_fixture.py`, `mechanic_register.py`, `decode_sc_assets.py` |
| `tests/` | pytest for the tooling (10 files); `test_oracle_native_diff.py` skips loudly without `data/oracle-native` |
| `docs/` | this documentation; `docs/media/` holds the README's graphics |

## Test layout

Everything under `crates/royalesim/` runs with `cargo test --release`; everything under the
repository's own `tests/` runs with pytest. Any count quoted below is the Rust suite unless
the row says otherwise -- RoyaleSim's Rust suite was red for four hours on 2026-09-22 while
four sessions quoted correct Python-suite counts at each other, and none of them said which.

| Path | Covers |
|---|---|
| `crates/royalesim/src/**` unit tests | phase order, math, loaders |
| `crates/royalesim/tests/battle.rs` | scripted battles and the every-tick invariants |
| `tests/mirror.rs`, `setup_spawn_order.rs`, `stacked_tie.rs` | seat symmetry and tie-breaks |
| `tests/mechanics.rs`, `territory.rs`, `spells.rs`, `knockback.rs` | behaviour per mechanic |
| `tests/tiebreak.rs`, `hide.rs`, `spawner.rs`, `charge.rs`, `reach.rs`, `lifetime.rs`, `status.rs` | the mechanics measured on the 16.402 corpus |
| `tests/tick_order.rs`, `knockback16402.rs`, `jump16402.rs` | the measured tick order, knockback ladder and river hop |
| `tests/formations.rs` | 12 `cargo test` cases over the summon layouts, against `tests/fixtures/formations/measured.json`, 79 groups (`tools/make_formation_fixture.py --check`). The fixture buckets by tap row as well as card, side and lane half, because a row-blind bucket threw away the corpus' only evidence for one of the clamp's bounds. |
| `tests/levels.rs` | level scaling and the tower ladder, against `tests/fixtures/live_levels.json` (`tools/make_live_levels_fixture.py --check`, needs `ROYALELIVE_REPORTS`) |
| `tests/replay_parity.rs`, `examples/replay_parity.rs` | a whole recorded battle replayed and scored, against `tests/fixtures/replay/sample.json` (`tools/make_replay_fixture.py ... --check`) |
| `tests/oracle2026.rs` | the path gates against recorded first paths (G6), against `tests/fixtures/oracle2026/client16402_first_paths.json` (`tools/make_client16402_paths_fixture.py --check`) |
| `tests/loadable_census.rs` | which rows of `cards.json` and `cards-2018.json` load, and the first clause of each refusal, pinned row by row. A change that makes a card loadable or unloadable edits the list; on a mismatch the test prints the new list to paste |
| `tests/hash_continuity.rs` | five short scripted battles on both card tables, hashed on every tick, against `tests/data/hash_continuity.json`. The file is recorded at the parent commit (`ROYALESIM_RECORD_HASH_CONTINUITY=1`), so a change that plays every battle as before passes. A change that makes a card load lists it in `LOADED_SINCE_PARENT` |
| `tests/save_load.rs`, `api.rs` | snapshots and the Python-facing API |
| `tests/throughput.rs` | timing; `#[ignore]`d, not a gate |
| `tests/` (pytest, repo root) | the Python tooling in `tools/` and `oracle/`; `test_capture_names.py` holds the one seat-naming rule the fixture makers share |
