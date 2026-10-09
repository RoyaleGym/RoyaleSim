# The engine's Python API

Most people never call RoyaleSim directly. RoyaleGym wraps it as an environment, and that is the place to start.
This page is for people who want the engine itself.

## Install

```bash
pip install royalesim
```

Use Python 3.12. The wheels need no Rust, and there are wheels for Windows, Linux and macOS. Each release's wheels
are also on the project's GitHub Releases page.

## A battle in a few lines

```python
import json
import royalesim

b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
ids = {row[0]: i for i, row in enumerate(json.loads(b.catalogue_json()))}   # card name -> id
deck = ["Knight", "Archer", "Goblins", "Giant", "Musketeer", "Fireball", "Arrows", "Skeletons"]
b.reset(seed=0, decks=[[ids[n] for n in deck], [ids[n] for n in deck]], shuffle=1,
        start_tick=0, elixir_milli=[5000, 5000], tower_hp=None, spawns=[])
SUB = royalesim.SUBTILE_PER_MILLITILE
b.step([], 100)                                        # nobody can play in the opening seconds
r = b.step([(0, 0, 9000 * SUB, 10000 * SUB)], 20)     # blue plays hand slot 0, then 20 ticks pass
state = json.loads(b.state_json())
print(state["tick"], royalesim.DEPLOY_REASONS[r[0][1]])
```

```text
120 OK
```

Look card ids up by name, as above. An id is a card's position in the catalogue, and positions move when cards are
added.

## The pieces

**Time.** One tick is 50 ms, so 20 ticks make a second. `step(commands, ticks)` applies the commands, then runs that
many ticks. It stops early when the battle ends.

**Positions.** Integers in sub-tiles. A tile is 1000 milli-tiles, and a milli-tile is `SUBTILE_PER_MILLITILE`
sub-tiles. The arena is 18 tiles wide and 32 tiles long. Blue (team 0) plays from the bottom.

**Cards.** `card_names=None` loads the default catalogue. Pass a list of names to choose your own. A card's id is its
position in that list. `catalogue_json()` lists each card with its cost, kind and deploy rule.

**Levels.** Both sides play at one card level and one tower level unless you say otherwise. To give each side its
own, pass `levels=[blue, red]` to `reset`, each a list with one level per deck card (or empty), and
`tower_levels=[blue, red]`. A Mirror's copy is one level above the Mirror.

**Commands.** A command is `(team, hand_slot, x, y)`. `step` returns one row per command:
`(card_id, reason, tick, x, y)`, where `reason` indexes `DEPLOY_REASONS` (`0` is accepted) and `x, y` is where
the card actually went down. Ask first with
`check_deploy(team, slot, x, y)`, which returns the same reason without playing.

**State.** `state_json()` returns the whole battle as JSON bytes: towers, units, spells, hands, elixir and the tick.
`spells` lists every live spell object, not only spell cards. An evolution's effects, a death bomb, a deploy blow and a
hero's ability are spells too, each under the card that was played.
`state_hash()` is one number for the whole state. Two runs with the same seed and commands give the same hash.

**Units.** Each unit is a list of values in `ENTITY_FIELDS` order. `card_id` is the card that put the unit down, so a
Witch's Skeletons report the Witch. `unit_type` is the unit itself: an index into `unit_types_json()`, a sorted list of
unit names. Every Skeleton has the same `unit_type`, whichever card made it. The list is the same for every Battle
built on one card table. `unit_types_digest()` is a short hash of it, so you can tell when a new table changes the
numbering.

**Saving.** `save()` returns bytes, and `load(blob)` restores them into a Battle with the same cards.

**Play delay.** `set_command_delay_ticks(blue, red)` makes a play land that many ticks after it is sent, like the
real client. `pending_commands()` lists what is waiting.

**Card table.** A Battle runs the 15.535.29 client's card table unless you pass
`calibration_overrides={"cards.CARD_TABLE": '"client160402017_20261006"'}`, which runs the current client's table
instead. `card_table()` names the table a Battle runs. Card ids are the same on both tables.

## Module-level helpers

| Name | What it gives you |
|---|---|
| `royalesim.data_dir()` | The folder with the engine's data files: calibration, arena and card table. |
| `royalesim.card_table_source()` | `"embedded"` for an installed wheel, or `"file:<path>"` in a source checkout. |
| `Battle.provenance()` | The commit the engine was built from, and whether that tree was clean. |
| `DEPLOY_REASONS` | Names for the reason codes a command returns. |
| `ENTITY_FIELDS` | Names for the values in each unit's row of `state_json()`, in order. |
| `HAND_SIZE`, `ABILITY_BUTTONS` | Hand slots per player, and champion/hero ability buttons per player. |

Methods whose names start with `debug_`, and the `*_states` readers, are for testing the engine. They may change
without notice.

## What stays stable

The battle logic changes as it is measured against the real game. The calls on this page are meant to stay put. When
one has to change, the CHANGELOG says so.
