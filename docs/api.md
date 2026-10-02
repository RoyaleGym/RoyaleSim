# The engine's Python API

Most people never call RoyaleSim directly. RoyaleGym wraps it as an environment, and that is the place to start.
This page is for people who want the engine itself.

## Install

```bash
pip install royalesim
```

The wheel needs no Rust. It runs on CPython 3.10 and later, on Windows, Linux and macOS.

## A battle in ten lines

```python
import json
import royalesim

b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
b.reset(seed=0, decks=[list(range(8)), list(range(8, 16))], shuffle=1,
        start_tick=0, elixir_milli=[5000, 5000], tower_hp=None, spawns=[])
SUB = royalesim.SUBTILE_PER_MILLITILE
b.step([(0, 0, 9000 * SUB, 10000 * SUB)], 20)   # blue plays hand slot 0, then 20 ticks pass
state = json.loads(b.state_json())
```

## The pieces

**Time.** One tick is 50 ms, so 20 ticks make a second. `step(commands, ticks)` applies the commands, then runs that
many ticks. It stops early when the battle ends.

**Positions.** Integers in sub-tiles. A tile is 1000 milli-tiles, and a milli-tile is `SUBTILE_PER_MILLITILE`
sub-tiles. The arena is 18 tiles wide and 32 tiles long. Blue (team 0) plays from the bottom.

**Cards.** `card_names=None` loads the default catalogue. Pass a list of names to choose your own. A card's id is its
position in that list. `catalogue_json()` lists each card with its cost, kind and deploy rule.

**Commands.** A command is `(team, hand_slot, x, y)`. `step` returns one row per command:
`(card_id, reason, ...)`, where `reason` indexes `DEPLOY_REASONS` (`0` is accepted). Ask first with
`check_deploy(team, slot, x, y)`, which returns the same reason without playing.

**State.** `state_json()` returns the whole battle as JSON bytes: towers, units, spells, hands, elixir and the tick.
`state_hash()` is one number for the whole state. Two runs with the same seed and commands give the same hash.

**Saving.** `save()` returns bytes, and `load(blob)` restores them into a Battle with the same cards.

**Play delay.** `set_command_delay_ticks(blue, red)` makes a play land that many ticks after it is sent, like the
real client. `pending_commands()` lists what is waiting.

## Module-level helpers

| Name | What it gives you |
|---|---|
| `royalesim.data_dir()` | The folder with the engine's data files: calibration, arena and card table. |
| `royalesim.card_table_source()` | `"embedded"` for an installed wheel, or `"file:<path>"` in a source checkout. |
| `Battle.provenance()` | The commit the engine was built from, and whether that tree was clean. |
| `DEPLOY_REASONS` | Names for the reason codes a command returns. |
| `HAND_SIZE`, `ABILITY_BUTTONS` | Hand slots per player, and champion/hero ability buttons per player. |

Methods whose names start with `debug_`, and the `*_states` readers, are for testing the engine. They may change
without notice.

## What stays stable

The battle logic changes as it is measured against the real game. The calls on this page are meant to stay put. When
one has to change, the CHANGELOG says so.
