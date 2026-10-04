# Changelog

The battle logic changes often, as it is measured against the real game. This file lists changes to how you install
and call the engine. Logic changes are listed by release in the [GitHub Releases](https://github.com/RoyaleGym/RoyaleSim/releases).

## 0.1.9 (2026-10-03)

- `state_json()`: each player gains `deck`, the side's eight card ids as set up, and `forms`, parallel to it (0 the
  card itself, 1 its evolution, 2 its hero form). A battle restored with `load` reports them too.
- Battle logic: troops now see a crown tower from their full sight range (their own size was left out before), and
  units made by the Evo Goblin Drill and the Hero Goblins' flag wait before they can be targeted. Also champion
  dashes, hero buttons, Hunter pellets on buildings, kamikaze launches, line formations, troop placement ties and Evo
  Skeleton Army spectrals, as measured in the game.

## 0.1.8 (2026-10-03)

- `state_json()`: each entity row gains four values after `mount_uid` (27 in all): `charge`, the build-up a player
  watches in permille (a Prince's run-up, an Inferno's ramp; 0 when none); `dest_x` and `dest_y`, where a unit under
  ground will come up (-1 when it is not tunnelling); and `ability_ticks`, the ticks left in a hero's or champion's
  ability where that has a fixed end (0 otherwise).
- `status_flags` gains bit 5 (a Clone's copy), bit 6 (an ability winding up), bit 7 (an ability active: the Archer
  Queen's cloak, the Golden Knight's dash chain, the Monk's deflect and the rest) and bit 8 (fully charged).
- `catalogue_json()`: each card row gains a 13th value, `evo_cycle`, the plays before each evolved play of the card's
  evolution (0 for a card with none), so the opponent's evolution charge can be counted from the plays you see.
- RoyaleGym reads the new values from commit e1fca68 on. An older RoyaleGym refuses this engine, because its rows are
  longer than it knows.

## 0.1.7 (2026-10-03)

- Each side, and each card in a deck, can have its own level, as in a real match. `Battle.reset` takes
  `levels=[blue, red]`, each empty or one unified level per deck card, and `tower_levels=[blue, red]`. A play is at its
  card's level: an evolution or a hero form at its base card's, and a Mirror's copy at the Mirror's level plus one.
  Leaving both out plays both sides at the battle's single level, as before.
- `Battle.step_commands_run()` lists every delayed command that ran or was dropped during the last `step`, over all
  its ticks: `(tick, team, kind, what, reason)`, where `what` is the card id of a play or the command slot of an
  ability press. A command still waiting when a level overtime ends is now reported dropped, with reason `GAME_OVER`.

## 0.1.6 (2026-10-03)

- No change to installing or calling the engine. Battle logic: crown-tower target ties, jump landings, Evo Musketeer
  snipes, a hooked unit's release, chase limits, death bombs and headings while casting, as measured in the game.

## 0.1.5 (2026-10-03)

- `state_json()`: an entity's `card_id` is now the card whose play put the unit on the board, all the way down its
  chain. A Tombstone's Skeletons report the Tombstone, a Barbarian Hut's Barbarians the hut, the Goblin Drill's
  Goblins the Drill, and the Tri-Wizards' Electro and Ice Wizards the Tri-Wizards. Before, a unit several cards can
  make reported the first such card in the catalogue (every Skeleton reported the Witch). A projectile's
  `firer_card_id` is unchanged: the unit that fired it.
- Battle logic: knockback and death pushback, units that strike and move in the same tick, kamikaze contact, cage
  shots, barrages, dash and chain-hop timing, ghost pairs, hidden buildings, equal-distance targets and pulls at the
  water's edge, as measured in the game.

## 0.1.4 (2026-10-02)

- A level overtime now ends as in the game. From tick 6067 every crown tower loses the same hit points each tick, and
  the first tower to fall decides the match (1-0, or 2-1 from 1-1). If both sides' weakest towers are exactly level,
  the match is a draw at tick 6147. At the end of a level overtime no more cards can be played, and every troop, building
  and spell leaves the board, so only the towers and the drain remain.
- `state_json()`: each spell row gains a 12th value, `ticks_flown`, the ticks a flying spell has moved.
- `catalogue_json()`: each card row gains a 12th value, `champion`, true for a champion card.
- Wheels are tested on CPython 3.10, 3.11, 3.12, 3.13 and 3.14.
- Battle logic: illegal troop taps, Furnace spirits, first hits, random delays, death rings and chases after a kill,
  as measured in the game.

## 0.1.3 (2026-10-02)

- A played card's slot can now stay empty for a moment, as in the game. Each player has one refill timer: a new card
  enters the hand once every 1 second (0.5 s in double elixir, 0.35 s in triple). Play two cards quickly and the
  second slot is empty until the timer lets the next card in. An empty slot reads `-1` in `state_json()`'s `"hand"`,
  and playing it is refused with `EMPTY_SLOT`.
- Battle logic changes: shots, Skeleton King copies, placement ties, targets of knocked-back and slapped units, death
  spawns, containers and building scans, as measured in the game.

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
