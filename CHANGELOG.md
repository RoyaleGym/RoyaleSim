# Changelog

The battle logic changes often, as it is measured against the real game. This file lists changes to how you install
and call the engine. Logic changes are listed by release in the [GitHub Releases](https://github.com/RoyaleGym/RoyaleSim/releases).

## 0.1.21 (2026-10-07)

- Placement, as recorded in ladder battles and measured in the game: once a lane's enemy princess tower has fallen, a
  troop (and the Log) may be placed on that lane's bridge. The other lane's bridge stays closed while its princess
  stands, and the river off the bridges stays water. `Battle.territory_model()` now returns
  `"enemy_tower_no_deploy_rects_open_bridge"`.
- RoyaleGym 0.1.19 or later is needed with this engine: older RoyaleGym versions refuse the new territory model.

## 0.1.20 (2026-10-07)

- `status_flags` bit 3 (evolved) and bit 4 (hero) now follow the unit, not the row it is on right now. A landed Evo
  Royal Hog, a lifted Hero Wizard and a Hero Bowler in its siege keep their bit, and an evolution's own summons (the
  Evo Royal Ghost's pair) now carry bit 3. A champion no longer carries bit 4, and neither does anything a hero puts
  down beside itself (a turret, a mount, a flag, a tomb's monster).
- `status_flags` gains bit 9 (512): a flier that Vines holds to the ground. `flying` keeps its meaning; a unit is in the
  air for targeting when it is flying and bit 9 is not set.
- New: `royalesim.STATUS_BITS`, the names of the status bits in bit order (name k is bit k). A bit missing from the list
  is one this engine does not report.
- Battle logic: the Evo Goblin Cage's Brawler plays with the plain Brawler's hitpoints (1121 at level 11, not 1080).
- This release also adds settings used to measure upcoming fixes against the game (a combo hit's step, a target lost
  past its reach, a net cast between shots, a pull among touching units, the Hero Tombstone's monster and its
  Skeletons, a dash's range check); every one defaults to the engine's current behaviour.

## 0.1.19 (2026-10-06)

- No change to installing or calling the engine.
- No change to battle logic. This release adds settings used to measure upcoming fixes against the game (a unit
  that loses its target while knocked back, a thrown target out of sight, units a death blow kills); every one
  defaults to the engine's current behaviour.

## 0.1.18 (2026-10-06)

- No change to installing or calling the engine.
- No change to battle logic. This release adds settings used to measure upcoming fixes against the game (a death
  bomb's body, the Little Prince's ramp, the Hero Dark Prince's mount, a death ring's direction); every one defaults
  to the engine's current behaviour.

## 0.1.17 (2026-10-06)

- Each unit in `state_json()` has a new last value, `unit_type`: what the unit is, while `card_id` stays the card that
  put it down. A Witch's Skeletons and a Tombstone's report two card ids and one `unit_type`. It is an index into
  `Battle.unit_types_json()`, a sorted list of unit names, the same for every Battle on one card table.
  `Battle.unit_types_digest()` is a short hash of that list. `ENTITY_FIELDS` names the new value.
- An ability button's `available` value now matches whether a press would be taken (apart from elixir): a Hero Mega
  Minion's button reads not ready until it has waited and has a target, and a Hero Tombstone's or Hero Goblins' button
  reads not ready once its time runs out.
- No change to battle logic. This release also adds settings used to measure upcoming fixes; every one defaults to
  the engine's current behaviour.

## 0.1.16 (2026-10-05)

- No change to installing or calling the engine.
- No change to battle logic. This release adds settings used to measure upcoming fixes against the game; every one
  defaults to the engine's current behaviour.

## 0.1.15 (2026-10-05)

- No change to installing or calling the engine.
- Battle logic, as measured in both clients: a Golemite (or any unit a death spawns) keeps the first leg of the route
  it was born with while it slides. If it slides past the next point of that route, it goes on to the point after,
  as the game does, instead of turning back for it.

## 0.1.14 (2026-10-04)

- No change to installing or calling the engine.
- Battle logic, as measured in both clients: the units a death spawns (a Golem's Golemites, for example) plan their
  route from where they appear, not from where their slide ends.

## 0.1.13 (2026-10-04)

- No change to installing or calling the engine.
- Battle logic, as measured in both clients: a unit that is already dead to the shots in flight is judged by homing
  shots only, and a ranged unit does not launch a shot at a target that has moved well beyond its reach.

## 0.1.12 (2026-10-04)

- No change to installing or calling the engine.
- Battle logic: a Hunter's pellets now reach a building's square, not only the circle inside it, and the engine
  draws its random delays (the Hunter's pellets) the way the game does, as measured in both clients.
- New per-client rules for replays of the 15.535.29 client: death bombs and paths, line formations, Evo Hunter nets,
  Evo Electro Dragon hops, the Evo Archer's far shot, Skeleton King souls, knockback ladders and recoil, heal pulses,
  Evo Skeleton Army spectrals, thrown units, kamikaze launches, held presses and the Monk's deflect.

## 0.1.11 (2026-10-04)

- The source package (sdist) now carries LICENSE, so PyPI accepts it.
- Battle logic: no change to how a battle plays by default. New per-client rules for replays of the 15.535.29 client
  (chases, target keeping, load timers, Evo Electro Dragon and Evo Hunter timings, warps, Skeleton King souls, freeze
  lengths), as measured in that client.
- A MIRRORED shuffle with per-card `levels` now keeps each card at its own level. Before, the levels stayed in the
  setup's order while the shuffle moved the cards, so a card could play at another card's level.
- `state_json()` players' `deck` and `forms` are the order the battle deals from: the setup's order, except under a
  MIRRORED shuffle, which permutes both decks the same way (each card's form and level go with it).

## 0.1.10 (2026-10-03)

- No change to installing or calling the engine. Battle logic: the Golden Knight's ability is one use per deploy, like
  every champion's except the Boss Bandit's. It used to come back 11 seconds after his dash chain, which was never
  measured; the game gives it no recharge.

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
