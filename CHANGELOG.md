# Changelog

The battle logic changes often, as it is measured against the real game. This file lists changes to how you install
and call the engine. Logic changes are listed by release in the [GitHub Releases](https://github.com/RoyaleGym/RoyaleSim/releases).

## 0.1.2 (2026-10-02)

- No change to installing or calling the engine. Battle logic changes only: the Skeleton King's copies are placed,
  and take their first step, as in the game.

## 0.1.1 (2026-10-02)

- No change to installing or calling the engine. Battle logic changes only: a unit keeps chasing a target just past
  its sight range, and the Skeleton Barrel flies straight at its target, as in the game.

## 0.1.0 (2026-10-01)

- Prebuilt wheels for Windows, Linux and macOS. One wheel per platform runs on CPython 3.10 and later. You no longer
  need Rust to install the engine.
- The engine carries its card table inside the wheel. An installed engine no longer reads files from the machine it
  was built on.
- New: `royalesim.data_dir()`, the folder holding the engine's data files.
- New: `royalesim.card_table_source()`, which card table a battle loads.
- `state_json()`: each row of a player's `"evo"` list gains a fourth value, the plays the evolution needs. Plays
  divided by it is the progress to the next evolved play. Readers of the first three values are unaffected.
- `royalesim` is now a package. The compiled module is still `royalesim.royalesim`, and everything in it is
  available as `royalesim.<name>`, as before.
