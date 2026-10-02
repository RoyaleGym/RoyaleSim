# AGENTS.md: RoyaleSim for AI coding agents

This file is for AI agents working in this repository. People should start at the [README](README.md).

## What this is

RoyaleSim is a deterministic Clash Royale battle engine. The core is Rust, using whole numbers only. It is exposed to
Python as the `royalesim` package through PyO3. RoyaleGym, the environment layer, is built on it.

## Layout

| Path | What it holds |
|---|---|
| `crates/royalesim/src/` | The engine. `state.rs` is the battle, `card.rs` the card table loader, `py.rs` the Python binding. |
| `crates/royalesim/tests/` | Rust integration tests, one file per mechanic. |
| `crates/royalesim/examples/replay_parity.rs` | Replays recorded battles and scores the engine against them. |
| `python/royalesim/` | The Python package: re-exports the compiled `royalesim.royalesim` and adds `data_dir()`. |
| `data/calibration.json` | Every engine constant, with its status (guess to measured) and its evidence. Read at build time. |
| `data/derived/` | Generated tables. `cards-15.535.json` is committed. The rest are made by stage 3 (below). |
| `data/raw/retroroyale-2018/` | The 2018 game tables the 2018 card table is built from. |
| `tools/` | Data extraction (`extract_*.py`), the wheel data stager, the cheatsheet generator, viewers. |
| `tests/` | The Python suite: binding tests, doc gates and data gates. |
| `docs/` | The deep docs. `engine.md` is the full engine page, `api.md` the Python API. |

## Build and test

These are CI's own commands (`.github/workflows/suite.yml`). Run them from the repository root, in a venv holding the
packages of pyproject's `dev` extra.

```bash
python tools/extract_arena.py
python tools/extract_cards.py --vintage 2018
cp data/derived/cards-15.535.json data/derived/cards.json
python tools/extract_globals.py
maturin develop --release
cd crates/royalesim
cargo test --profile gate --no-fail-fast
cargo clippy --release --all-targets --keep-going -- -D warnings
cd ../..
python -m pytest -q
python -m ruff check .
```

Wheels are built by `.github/workflows/wheels.yml`. It runs stage 3, then `python tools/stage_wheel_data.py`, then
`maturin build --release`. Each wheel is then installed into a clean venv and checked with `tools/wheel_smoke.py`.

## Rules that the tests enforce

- **Integers only in the engine.** No floats in battle logic. A test scans for them.
- **Determinism.** The same seed and commands give the same `state_hash()` on every machine. `hash_continuity.rs`
  pins the hash across changes, and a deliberate change re-records it.
- **Every constant lives in `data/calibration.json`** with a status and evidence. Do not hard-code a game value.
- **Test plants.** A test for a rule names a `clash_plant` that switches the rule off. The test must fail with the plant
  on (`RUSTFLAGS='--cfg clash_plant="NAME"'`). `docs/contributing.md` lists them and their count.
- **Generated pages stay fresh.** `docs/game-values.md` must equal what `tools/game_values.py` prints.
- **Docs gates.** Every relative link resolves, every shell block runs in the shells it claims, and every figure names
  what produced it.
- **Card ids are positions** in the catalogue. Tests and callers name cards, never bare ids. New cards are appended.

## Public text

This repository is public. Its text describes the engine and the game's mechanics only. Do not add anything about
training bots, results from bots, or private tooling and paths.

## Deeper docs

- `docs/engine.md`: building from source, what is modelled, speed and accuracy.
- `docs/contributing.md`: the build loop, every gate and the plant list.
- `docs/calibration.md`: the constants file and its status words.
- `docs/mechanics.md`: what is modelled, what is not, and the known defects.
- `docs/architecture.md`: how the engine is built.
