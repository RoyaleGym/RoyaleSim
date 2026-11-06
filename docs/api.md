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

**A battle under way.** `reset` can also start from the middle of a battle:

- `unit_spawns` puts down a unit by its name, a summoned one too (a Golemite, a Witch's Skeleton), each at its own
  level: `(name, team, x, y, hp, level, form)`, with -1 for the default hp and level. The unit must come from a card in
  the Battle's card list (a Golemite needs the Golem). With `form` 0 you get the unit you name, an evolved one too
  (`"Musketeer_EV1"`). A unit's `unit_type` name from `unit_types_json()` works as well, so a unit row can be put
  back as it is. With 1 or 2 you get the card's evolution or hero form (`"Musketeer"`, 1). An evolved or hero
  unit starts as if just played: an Evo Musketeer has all her snipes and a hero has its charge. Its play's own
  effects are not cast, so an Evo Cannon fires no barrage. Ask `unit_spawn_kind(name, form)` first. `"stands"` and
  `"fresh"` can be put down. A rider, a spell, a unit a hero's button makes (the Hero Musketeer's turret), the Hero
  Tombstone's waiting monster and the Evo Lumberjack's ghost cannot. `snipe_states()` gives each Evo Musketeer's
  snipes left.
- `unit_deploy_ticks` gives each `unit_spawns` entry its deploy ticks still to run, one per entry (a unit row's
  `deploy_ticks`), 0 for a deployed unit. A unit still deploying stands until its timer ends, then deploys: a Tesla
  hides then and a Furnace starts its spirits. A member of a group (a Royal Recruit) keeps the wait the group's
  stagger gives it. An evolved or hero unit's own clocks start at the copy. Leave out a unit that is under ground (a
  Miner on its way).
- `king_wake_ms=[blue, red]` sets each king tower's wake clock: the milliseconds since it was first hit or lost a
  princess tower, or -1. A player row's `king_wake_ms` in `state_json` reads it back.
- `evo_counts=[blue, red]` gives each evolved deck card's plays since its last evolved one, one count per deck card.
- `last_played=[blue, red]` is the card id each side's Mirror would copy, or -1.
- `spells_in_flight` puts one flying spell object on its way:
  `(card_id, form, team, x, y, aim_x, aim_y, level, delay_ms)`. One Arrows wave is one entry. `form` is 0 for the
  card, 1 for its evolution, 2 for its hero form and 3 for an evolution's decoy. Ask
  `spell_in_flight_kind(card_id, form)` first: only `"flies"` can be put in flight.
- `projectiles_in_flight` puts one shot of a unit or crown tower on its way:
  `(firer, team, x, y, aim_x, aim_y, target)`. `firer` and `target` point at the copy's own board: 0 and up is that
  `unit_spawns` entry, and -1, -2, -3 are crown towers 0, 1, 2 in the order `reset`'s `tower_hp` takes them, 0 the
  king (the firer's of `team`, the target's of the other side). A row's `tower_slot` is not that order, and a row's
  `target_uid` -1 is `None` here, not -1. `target=None` is a shot whose target is gone: it flies on to its aim. The
  shot is the firer's own, at its level, so the firer must be on the board, and the target must be an enemy. Ask
  `projectile_in_flight_kind(name, form)` first: only `"flies"` can be put in flight. A straight shot (the Hunter's),
  a chain or bounce (the Electro Dragon's), an evolution's special shot (the Evo Musketeer's snipe) and a melee hit
  cannot.

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
`calibration_overrides={"cards.CARD_TABLE": '"client160635007_20261103"'}`, which runs the current client's table
(16.635, from its game update of 2026-11-03) instead. `"client160402017_20261019"` runs the 16.402 client's table
after its Season 88 mid-season update of 2026-10-19, and `"client160402017_20261006"` the same client's table from
2026-10-06 to that update. `card_table()` names the table a Battle runs, and `card_table_fnv1a64()` gives the hash
of the file it read. Card ids are the same on every table. A build from a checkout also takes `"client160402017"`, the
current client's table from before its 2026-10-06 update; a wheel has no copy.

**Overrides are JSON.** Each value in `calibration_overrides` is JSON text: pass `json.dumps("none")`, which is
`'"none"'`. A bare `"none"` raises ValueError.

**The older client.** The engine runs the current client's (16.402) mechanics and card values. For a battle on the
older client's rules, pass `calibration_overrides=royalesim.client_arms("15.535.29")`. That is the 15.535.29 client's
measured mechanics and the card tables' own values: the same setup `replay_parity` runs for a battle recorded on that
client. Any `"15.535.<build>"` gets the same setup; it was measured on 15.535.29. `client_arms("16.402")` is `{}`.

## Module-level helpers

| Name | What it gives you |
|---|---|
| `royalesim.data_dir()` | The folder with the engine's data files: calibration, arena and card table. |
| `royalesim.card_table_source()` | `"embedded"` for an installed wheel, or `"file:<path>"` in a source checkout. |
| `royalesim.client_arms(client)` | The `calibration_overrides` that run one client's measured setup: `"15.535.29"` or `"16.402"`. |
| `Battle.provenance()` | The commit the engine was built from, and whether that tree was clean. |
| `DEPLOY_REASONS` | Names for the reason codes a command returns. |
| `ENTITY_FIELDS` | Names for the values in each unit's row of `state_json()`, in order. |
| `HAND_SIZE`, `ABILITY_BUTTONS` | Hand slots per player, and champion/hero ability buttons per player. |

Methods whose names start with `debug_`, and the `*_states` readers, are for testing the engine. They may change
without notice.

## What stays stable

The battle logic changes as it is measured against the real game. The calls on this page are meant to stay put. When
one has to change, the CHANGELOG says so.
