# The calibration ledger

`data/calibration.json` holds every physics constant the engine runs on, and each one carries how
well it is known. Nothing in the Rust core or in the Python layer may hardcode a number that
appears in this file.

## Why it exists

This project's predecessor spent weeks trying to make pathfinding match the real game. The cause
was not difficulty: plausible numbers and measured numbers were stored the same way, so a wrong
guess was indistinguishable from a fact, and later tuning absorbed the error instead of revealing
it. A constant fitted to the one case anybody checked makes a wrong **law** read as right, on
every board at once.

The ledger's job is to keep those two kinds of number apart, permanently and visibly.

## What an entry looks like

```jsonc
"PATH_SEARCH": {
  "value": "client16402",
  "status": "measured",
  "confidence": "HIGH",
  "candidates": ["client16402", "trace_fitted_astar"],
  "provenance": "...the client version, the captures, and what was compared...",
  "promotion_rules": "...the observation that would settle or overturn it...",
  "engine_contract": "...which code reads it, and what it changes..."
}
```

| Field | Meaning |
|---|---|
| `value` | what the engine runs on. Compiled into the crate; changing it needs a rebuild |
| `status` | how the value is known — the vocabulary below |
| `confidence` | HIGH / MEDIUM / LOW, and which *part* of a key is which when a source settles only part of it |
| `candidates` | the other values the engine implements. `state.rs::pick` refuses a candidate string with no implementation, so a candidate is always runnable |
| `provenance` | the evidence: the client version, the trace or capture, the counts |
| `promotion_rules` | the observation that would raise the status, written before it is made |
| `engine_contract` | which code reads the key and what changes when it changes |

### Status vocabulary, in increasing order of trust

| Status | Meaning |
|---|---|
| `guess` | nobody has evidence; a placeholder so the engine runs |
| `hypothesis` | an argument from the shape of the data, not an observation |
| `community` | multiple independent third parties agree, with no primary source |
| `datamined` | taken from shipped game data — state the file and the vintage |
| `measured` | observed in the real client by this project's own instruments |
| `owner_ruling` | a maintainer's direct observation of the live client, quoted verbatim and dated |

`owner_ruling` ranks **with** `measured`, not below it: a direct observation of the live client is
a primary source, and the live client is the target. A ruling may settle only part of a key — the
sign of a push but not its vector, say — in which case `confidence` names which half is which and
a `promotion_rule` stays open for the rest.

## Changing a value

Change a value only with new evidence, and write the evidence into the entry: what was measured,
on which client version, and in which trace or capture.

`oracle/calibrate.py` enforces the ordering. A value at `measured` or `owner_ruling` cannot be
overwritten with a different value without `--supersede`, so a later result that disagrees with an
earlier measurement is refused rather than applied quietly. Superseded readings stay in the
entry's history instead of being deleted — an argument that loses to a measurement belongs beside
what overturned it.

After changing any `value`, rebuild: `..\.venv\Scripts\maturin develop --release`. Prose-only
edits need no rebuild (see `contributing.md`).

## Ground truth, and which source wins

Three bodies of evidence sit behind the `measured` entries:

| Source | Client | Where |
|---|---|---|
| Offline traces | 15.535.29 | `data/oracle-native/` (gitignored, large) |
| Live captures | 16.402 | recorded by the client instrument in the private RoyaleLive repo |
| Shipped game data | 2016-2018 vendored, 2023 cross-reference | `data/raw/` |

Where the two clients disagree, **the live 16.402 client wins** and the entry says so: the target
is the live game, not a frozen build. Shipped data from 2016-2018 is evidence, not spec — ground
movement was rewritten on 2025-03-31, so anything pre-2025 about movement is archaeology until it
is re-measured.

`tools/oracle_diff.py` diffs the engine against a trace tick by tick;
`crates/royalesim/tests/oracle2026.rs` gates the recorded first paths.

## Open keys and what would settle them

Everything below is at `guess`, `hypothesis` or `datamined`-but-unverified as of 2026-09-21, which
means the engine runs on a placeholder and the behaviour it produces is not evidence about the
real game. Each row names what a recording would have to show.

| Key | Value today | Status | Settled by |
|---|---|---|---|
| `collision.PUSH_MODEL` | `mass_weighted` | guess, LOW | a mass-ladder recording: units of known Mass pushing each other |
| `collision.BUILDING_FOOTPRINT_MODEL` | `collision_radius_circle` | guess, LOW | a walk past a building; note that circle and 2x2 box differ by 0.044 tile at best, so only 3x3-vs-not is separable |
| `collision.SEPARATION_ITERATIONS` | 1 | guess, LOW | a crowd recording with per-tick positions |
| `pathfinding.TIE_BREAK` | `ortho_first_placeholder` | guess, LOW | read only by the trace-fitted arm (`path2026.rs`); the selected arm reproduces the published node lists outright, so the key no longer gates it |
| `combat.DAMAGE_ARITHMETIC` | `integer` | guess, LOW | hit counts to kill a tower at known levels |
| `combat.CROWN_TOWER_DAMAGE_ROUNDING` | `ceil_kept_share` | community, MEDIUM | a crown tower's displayed hp before and after a spell at two card levels |
| `time.PROJECTILE_SPEED_TO_SUBTILES_PER_TICK` | 15 | hypothesis, LOW | a projectile's flight time over a known distance at 60 fps |
| `targeting.LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET` | 25 | datamined, LOW | vendor the modern data, or measure a target held past its range |
| `targeting.LOGIC_XPOS_BASED_TOWER_TARGETING` | true | datamined, LOW | a centre-column deploy: which tower it walks at |
| `match.LOGIC_BATTLE_START_COOLDOWN_MS` | 4500 | datamined, LOW | any recording of a match start |
| `match.KING_ACTIVATE_TIME_MS` | 3300 | datamined, MEDIUM | a recording of what the delay actually delays |
| `arena.ARENA_SOURCE_VINTAGE` | ~2018 tilemap | datamined, MEDIUM | a calibrated screenshot of a live arena; bridge width varied by arena even in 2018 |
| `knockback.*` (8 keys) | fixed-distance slide | guess / community, LOW | the live ladder is characterised (`mechanics.md`, "Knockback"); porting it closes these |
| `spells.*` (8 keys) | see `spell-spec.md` | guess / hypothesis, LOW | each key in `spell-spec.md` carries its own deciding observation |
| `status.*` (stun and buff timing) | see `spell-spec.md` | community / hypothesis | likewise |
| `rng.GENERATOR` | `pcg32` | guess, LOW | not settleable, and not a goal — see `architecture.md`, Determinism |

The keys that carry the measured 2026 movement and pathfinding model — `time.TICK_MS`,
`time.SPEED_TO_SUBTILES_PER_TICK`, `pathfinding.PATH_SEARCH`, `collision.CONTACT_LAW`, the
`movement.*` section, and the cost, goal and replan keys — are at `measured`/HIGH. Their evidence
is in `pathfinding.md` and `movement-measurements.md`.
