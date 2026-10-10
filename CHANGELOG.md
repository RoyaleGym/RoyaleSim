# Changelog

The battle logic changes often, as it is measured against the real game. This file lists changes to how you install
and call the engine. Logic changes are listed by release in the [GitHub Releases](https://github.com/RoyaleGym/RoyaleSim/releases).

## 0.1.28 (2026-10-10)

- Two new `BattleConfig` fields. `tower_troops` names each side's crown tower troop: `"DaggerDuchess"`, `"Cannoneer"`
  or `"ChefTower"`. The default, `[None, None]`, keeps the Princess. `king_level` sets each King Tower's own level; the
  default uses `tower_level`, as before. Both are read when the battle is made. The tower troops load as the last three
  card slots, so no deck, catalogue or existing index moves.
- Twenty-five settings now default to the current client's behaviour, each measured against its recorded battles.
  Among them: a dying troop's death push, the ring a death spawn is laid on, a Witch's ring point at the arena's edge,
  the Hero Mega Minion's warp and return, the Little Prince's guard, the Mighty Miner's bomb, the tower troops' damage,
  an Evo Skeletons copy (now on the tick after its hit), and a Poison's first pulse (now 25 ticks after its cast). Each
  older behaviour stays available as that setting's old value, and `replay_parity` uses the old values for the older
  client's battles. The full list is in the release notes on GitHub.
- `spells.RELEASE_GROUND_POINT` treats the two sides differently, as the game does. A seat-symmetry check can turn it off
  with `calibration_overrides={"spells.RELEASE_GROUND_POINT": "none"}`.
- On the current client's card tables, a Skeleton King counts a soul once it lands, 1.45 s after the death, not at the
  death.
- A hero's level-up can take it past level 17 along its rarity's level ladder (the client's Hero Mini P.E.K.K.A went
  from 16 to 18 in one level-up). A card is still played at 17 at most.
- The card tables carry a new field, `sight_clip_side_milli`: how far a unit's sight is cut at its sides (the Hog
  Rider's 4000, the Giant's 2000). The engine does not read it yet. The tables' hashes change with it.
- Tools: `tools/make_replay_fixture.py` reads more of a live capture. It recovers casts the elixir check had dropped,
  Poison and Earthquake levels, a hero's or a champion's ability presses and the game mode. It also reads a Zap that
  leaves no object (from the capture's header), a Rage whose area shows after its elixir drop, a Lightning or a Royal
  Delivery from its area (dated on its cast, its level from its bolt or crate), and a spell's level from what it took
  off a crown tower. A Poison or an Earthquake first seen after missing frames is dated by its pulses, and a Hero
  Musketeer's turret is read as its card's.

## 0.1.27 (2026-10-09)

- The 2026-10-06 card table now has that update's Evo Skeletons summon ring (400, was 700). The extractor read the
  update's evolution overrides for no other row, and this is the only value that moves.
- A third card table: `cards.CARD_TABLE = "client160402017"` is the same client before its 2026-10-06 update (content
  16.402.2). It is read from a checkout (`data/derived/cards-160402017.json`); a wheel carries no copy.
- A card can be played one level past its rarity's level count: level 17, which max-level players reach in the live
  game, on the next multiplier of its level ladder (a Common's 450 %). Levels 1 to 16 are unchanged, and so are the
  card tables. A Mirror of a level-16 card now plays at 17.
- A new setting, `match.BATTLE_END`. The default, `rules`, is the game's end. `overtime_end` lets nothing end a battle
  before overtime's end, for scoring a replay past the point where the engine's battle would have ended and the
  recorded one did not.
- `replay_parity` puts each deck card at the level its plays recorded, not at the side's most common level. A
  battle whose most common level was below one card's lowest level was refused whole. It also reads the random
  generator that the current client's recorded battles carry; it used to read only the older client's.
- Tools: `tools/make_replay_fixture.py` takes `--table 160402017`, and refuses a capture whose content stamp is not
  its table's. It reads an Evo Skeletons copy as that group's spawn, not as a new play, and a Graveyard's cast off its
  skeletons. It takes a spell's level from the units it puts down where it can, and only ever a level the card can
  be played at. An opponent's troop seen only as its members' centroid is played on that point's tile centre. It
  names each side's tower troop (`tower_troops`: the Princess, the Dagger Duchess, the Cannoneer or the Royal Chef).

## 0.1.26 (2026-10-09)

- This release adds settings used to measure upcoming fixes against the game (a struck troop's last step, a Log tapped
  on the water, a doomed target and its attackers' wait, a curse's first step, a Sparky's windup after its target dies,
  a launcher and a kamikaze, a troop tapped on an enemy building, the Evo Skeleton Army's General); every one defaults
  to the engine's current behaviour.
- Tools: `tools/make_replay_fixture.py` reads an evolution's or a hero form's units as its card's (an Evo Bats deploy was
  read as an unknown object), reads the time stamp of a live capture's name, and gives a live capture no partner seat.
  A `replay_parity` report names the card table it loaded, not the checkout's `cards.json`.

## 0.1.25 (2026-10-09)

- New: `cards.CLIENT16402_VALUES` can set a unit's `HitSpeed` and `LoadTime`, as it already sets hitpoints, damage and a
  few area timings. Its `table` may name either card table, so one card's value can be moved on the current client's
  table too. Defaults are unchanged.
- Tools: `replay_parity --census` stamps the card table it loaded and, on the current client's table, writes its own
  census file. `tools/make_replay_fixture.py --table 160402017-20261006` builds fixtures against that table, its card
  ids and its census together.

## 0.1.24 (2026-10-09)

- New: the event cards load. Global Clone, Super Witch, Goblin Party Rocket, Global Lightning, Super Mini P.E.K.K.A.,
  Super Elite Archer, Super Lava Hound, Super Hog Rider and the Goblin Rocket Silo now run, as recorded in the game.
  Every card of the 15.535.29 table loads.
- Battle logic: the Evo Hunter never casts his net at a building. A Golden Knight waiting to dash goes the tick after
  his target comes in reach. A hero's turret pushes no one on the tick it is made.
- This release also adds settings used to measure upcoming fixes against the game (a Clone's hold and slide, a death
  spawn's deploy, death projectile copies, an Evo Skeleton Barrel's drop point, a chaser and a sliding target, a
  launch from beyond reach, the Evo Mega Knight's uppercut); every one defaults to the engine's current behaviour.

## 0.1.23 (2026-10-08)

- New: the current client's card table. `Battle(..., calibration_overrides={"cards.CARD_TABLE":
  json.dumps("client160402017_20261006")})` runs that Battle on the 160402017 client's table, with its 2026-10-06
  update: its own hitpoints, damage and timings, the 2026-10-06 balance changes included. `Battle.card_table()` names
  the table a Battle runs. The default is still the 15.535.29 table, and card ids are the same on both. The wheel
  carries both tables.
- On the current client's table the Evo Electro Giant runs too: every 6 seconds a ring grows out of him, and each enemy
  it reaches drops a level. This is read off the table, not measured yet.
- A few newer keys still play as on the older table, among them the Little Prince's ability and the Rune Giant's
  enchant hold. `examples/table_census` lists them.
- Battle logic: a strike due exactly at the end of an area's life now lands, as measured in the game. No area on the
  default table has one.

## 0.1.22 (2026-10-07)

- Battle logic: when the Evo Goblin Drill's building dies, its two Goblins come out to its left and right, 500 either
  side, as the plain Goblin Drill's do. They came out above and below it.
- Battle logic: the Evo Lumberjack's ghost is no longer sped up by his death Rage. Its row says it ignores Rage, and the
  engine now reads that.
- This release also adds settings used to measure upcoming fixes against the game (a sniper letting go of its target,
  a captive's route and facing, spear throwers and doomed targets, an impact area's spot, a chain's first hop, a Goblin
  Drill building going under and rising, a tomb on the path grid, a new unit's first target, a poison area's pulses, a
  ring member on water, an uppercut's flight, routes during a knockback, a chase past a kill, a caged unit's avoidance,
  an evolved copy's spot); every one defaults to the engine's current behaviour.

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
