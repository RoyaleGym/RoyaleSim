# Changelog

The battle logic changes often, as it is measured against the real game. This file lists changes to how you install
and call the engine. Logic changes are listed by release in the [GitHub Releases](https://github.com/RoyaleGym/RoyaleSim/releases).

## 0.1.49 (2026-11-07)

- 2 more settings default to the current client's behaviour, each measured against its recorded battles:
  - A troop chasing a target that has left its sight keeps it only while its path to it goes backwards (round by a
    bridge, for one), whether the target is a troop or a building. Otherwise it turns to the lane's tower. A flying
    troop lets such a target go (`targeting.CHASE_HOLD_BACKWARDS`).
  - A shot launched while the Little Prince's guard is charging no longer lands on the last tick of the guard's
    immunity. A shot that was already flying at it when the charge began still does (`combat.DASH_IMMUNE_HITS`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` keep the older client's values for its battles.

## 0.1.48 (2026-11-06)

- `reset` takes two more optional arguments for copying a battle under way. Leaving them out keeps the old
  behaviour:
  - `unit_deploy_ticks` gives each `unit_spawns` entry its deploy ticks still to run, one per entry (a unit row's
    `deploy_ticks`), 0 for a deployed unit. A unit still deploying stands until its timer ends, then deploys. A
    negative count, and a count above 0 for a unit that has no deploy time, are refused.
  - `projectiles_in_flight` puts one shot of a unit or crown tower on its way:
    `(firer, team, x, y, aim_x, aim_y, target)`. `firer` and `target` point at the copy's own board: 0 and up is
    that `unit_spawns` entry, and -1, -2, -3 are crown towers 0, 1, 2 in `reset`'s `tower_hp` order. `target=None`
    is a shot whose target is gone: it flies on to its aim. The shot is the firer's own, at its level, so the firer
    must be on the board.
- New `projectile_in_flight_kind(name, form=0)` says first whether a unit's or crown tower's shot can be put in
  flight. Only `"flies"` can. The others are `"straight"` (the Hunter's shots), `"hops"` (a chain or bounce),
  `"form_shot"` (an evolution's special shot), `"warp_shot"` (a hero whose warp remakes its shots), `"kamikaze"`
  and `"no_projectile"` (a melee hit). A crown tower is `"KingTower"` or `"PrincessTower"`.
- `state_json` is unchanged, so royalegym needs no upgrade.

## 0.1.47 (2026-11-05)

- 2 more settings default to the current client's behaviour, each measured against its recorded battles:
  - The Evo Mega Knight no longer starts a jump while he is still standing after an uppercut. If a target is in
    reach when the stand ends, he jumps then (`combat.UPPERCUT_STAND_DASH`).
  - The Goblin Machine fires its rocket. It marks where the closest enemy 2.5 to 5 tiles away stands and, a second
    later, fires a rocket that damages every enemy near that spot, once every 5 seconds while targets are in reach.
    The setting is new (`combat.GOBLIN_MACHINE_ROCKET`); its old value leaves the rocket out.
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles.
- New setting `targeting.CHASE_HOLD_BACKWARDS`: how long a troop keeps chasing a target that has left its sight when
  nothing else is in sight. The default is unchanged, and `client_arms("15.535.29")` lists it.

## 0.1.46 (2026-11-04)

- 4 more settings default to the current client's behaviour, each measured against its recorded battles:
  - A unit being healed (an Evo Bat after each of its hits, a Battle Healer, a unit in a Heal Spirit's area) is not
    written off as doomed until the homing shots in flight at it add up to its full hitpoints
    (`targeting.DOOMED_HEAL_BUFF`).
  - The Evo Skeleton Army's General stays inside the arena when the army is played on the back row
    (`formation.ARMY_GENERAL_CLAMP`).
  - The Little Prince's guard's charge passes over a troop leaping the river. The troop takes the hit, and the push,
    on its first tick out of the leap if the charge still reaches it (`combat.GUARD_CHARGE_LEAP`).
  - An Executioner knocked back while his axe is out keeps his attack and waits for the axe, instead of starting a
    fresh throw (`knockback.PINGPONG_WAIT_RESET`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles.
- New settings `combat.HOOK_DRAG_ROUNDING` (how the Fisherman's drag step is rounded) and
  `combat.UPPERCUT_STAND_VICTIM` (when the Evo Mega Knight may attack a unit he has just thrown). The defaults are
  unchanged, and `client_arms("15.535.29")` lists both.

## 0.1.45 (2026-11-04)

- Upgrade royalegym to 0.1.27 with this release. Its state reader carries the new columns below; an older royalegym
  refuses this engine's state rows.
- `reset` can start from a battle under way. Each new argument is optional, and leaving it out keeps a fresh battle's
  state:
  - `unit_spawns` puts down any unit by its name, a summoned one too (a Golemite, a Witch's Skeleton), at its own
    level: `(name, team, x, y, hp, level, form)`. With `form` 0 you get the unit you name, an evolved or hero one
    too (`"Musketeer_EV1"`), and a unit row's `unit_type` name works as well, so a unit row puts it back. With 1 or 2
    you get a card's evolution or hero form (`"Musketeer"`, 1). An evolved or hero unit starts as if just played (an
    Evo Musketeer has all her snipes, a hero has its charge), but its play's own effects are not cast (an Evo Cannon
    fires no barrage). `unit_spawn_kind(name, form)` says first whether a unit can be put down: a rider, a unit a
    hero's button makes, the Hero Tombstone's waiting monster and the Evo Lumberjack's ghost cannot.
    `snipe_states()` gives each Evo Musketeer's snipes left.
  - `king_wake_ms` sets each king tower's wake clock, `evo_counts` each evolved card's plays since its last evolved
    one, and `last_played` the card each side's Mirror would copy.
  - `spells_in_flight` puts a flying spell object on its way, one Arrows wave per entry. Ask
    `spell_in_flight_kind(card_id, form)` first: only `"flies"` can be put in flight.
- `state_json` gains columns, each after the existing ones: a player's `king_wake_ms`, and a spell row's `level` and
  `form` (0 the card, 1 its evolution, 2 its hero form, 3 an evolution's decoy).
- A battle saved against different card data (a release that changed the cards, or another card table) is refused
  with a message that says so.
- 7 more settings default to the current client's behaviour, each measured against its recorded battles:
  - A hero's attack keeps loading while it uses its ability (the Skeleton King, Archer Queen, Monk, Boss Bandit,
    Mighty Miner and the hero forms), unless it is stunned (`combat.PRESS_LOAD_TIMER`).
  - A unit the Evo Mega Knight throws can be targeted only by air attackers while it flies
    (`targeting.UPPERCUT_FLIGHT_TARGETABILITY`).
  - The Little Prince's guard's charge also hits enemy buildings in its path (`combat.GUARD_CHARGE_BUILDINGS`).
  - Units knocked back by the Mega Knight's jump start moving a tick earlier when they were placed after him
    (`combat.DASH_PUSHBACK`).
  - The Giant Skeleton's death bomb pushes units back (`knockback.DEATH_PUSHBACK`).
  - A unit the Fisherman drags onto the river is set down on the nearest land (`combat.HOOK_WATER_LANDING`).
  - A Bandit, Boss Bandit or Mega Knight stops pushing units the moment its dash starts (`combat.DASH_START_CONTACT`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles.

## 0.1.44 (2026-11-04)

- The Evo Giant runs on the 16.635 table (`cards.CARD_TABLE` `"client160635007_20261103"`). Below half its
  hitpoints it puts a Rage area down behind it at once and then every 1500 ms, held while it is stunned or frozen.
  Each area is wider the more hits the giant has made since (2 to 3.5 tiles). The areas rage its own side only and
  deal no damage. A Clone of it makes a plain Giant. This is read off the game's table and not yet measured. The
  Evo Mirror is still not supported. On that table `unit_types()` gains `Giant_EV1`, so every unit type after it in
  the sorted list moves up one (`unit_types_digest` names the list). The other tables' lists are unchanged.
- 10 more settings default to the current client's behaviour, each measured against its recorded battles:
  - A Poison does not touch a unit exactly on its edge (`spells.PULSE_EDGE`).
  - The Inferno Tower, Inferno Dragon and Mighty Miner wait one tick more after a guard's charge before they target
    it (`targeting.VARIABLE_DAMAGE_IMMUNE_TARGET`).
  - A dashing Bandit, Boss Bandit or Mega Knight keeps its target (`targeting.DASH_TARGET_SCAN`).
  - The Dark Witch's Bats come out at her facing's eighth of a turn (`spawner.SPAWN_POINT`).
  - The Little Prince: a charging guard is not slowed (`status.DASH_IMMUNE_BUFFS`), an area hit passes over a guard
    on its first landing ticks (`combat.GUARD_LANDING_AREA`), and his load timer runs through his cast unless he is
    stunned (`combat.PRESS_LOAD_TIMER`).
  - A unit the Evo Mega Knight throws takes one free step, then is rooted (`knockback.UPPERCUT_ROOT_WINDOW`).
  - A walker does not count a dasher's attack stand as a walk stand (`movement.DASH_STAND_HEADING`).
  - The Hero Mega Minion marks its warp target before the press (`combat.WARP_PICK`).
- Replay fixtures (`tools/make_replay_fixture.py`) carry a shielded unit's shield, and `replay_parity` scores it on
  its own, apart from hitpoints. A recording without shields gives the same fixture as before.
- `replay_parity` follows the 16.635 client's random number generator, the same as 16.402's. The fixture maker drops
  the generator's states from a 16.635 recording that read them at the old place in memory, and says so in the
  fixture's `rng_unread`. A 16.635 fixture's generator column is read only when it names the place its states were
  read at (`rng.place`), so one made by an earlier maker plays on the engine's own draws.
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles.
- Battles saved by an earlier release do not load: a saved battle is checked against the card data it ran on, and
  the Evo Giant's support changed that data on every table (`SNAPSHOT_FORMAT` is still 20).

## 0.1.43 (2026-11-03)

- A new card table: the 16.635 client's, from the game update of 2026-11-03 (content 16.635.8). Pick it with
  `calibration_overrides={"cards.CARD_TABLE": '"client160635007_20261103"'}`. The update changes no card's numbers,
  so the table carries the Season 88 values. The default table is unchanged.
  - On it the Evo Electro Giant's pulse takes the Clone spell's copies again.
  - The Evo Giant and the Evo Mirror are not supported yet: a deck may play the Giant and the Mirror, not their
    evolutions.
  - Some new details of the client are recorded in the table and not yet simulated: hero abilities and the Golden
    Knight's dash are refunded until they land, the Evo Mega Knight stands while it uppercuts, the Little Prince's
    guard charge passes over units that are immune while they dash, and the Evo Cannon's and Evo Firecracker's
    damage buffs stack once per cast. In Rust, a loaded table's `CardDb.unmodelled` lists them.
- `tools/make_replay_fixture.py --table 160635007-20261103` makes fixtures from recordings of the new client
  (16.635.8), and no 16.402 table takes such a recording.
- `Battle.unit_mana_json()` gives the elixir each unit of the table produces, carries or pays out on its death.
- 28 more settings default to the current client's behaviour, each measured against its recorded battles:
  - Dashes and charges: a timed dasher whose target dies takes its next one after the dash
    (`combat.DASH_KILL_RETARGET`); a standing dasher keeps a new target and dashes at it
    (`combat.DASH_STAND_RETARGET`); a shot already flying at a unit that starts a dash lands on the unit's last
    immune tick (`combat.DASH_IMMUNE_HITS`).
  - The Little Prince: his guard is born heading for the nearest enemy in sight (`combat.GUARD_BIRTH_GOAL`), walks its
    first landed steps without avoiding (`combat.GUARD_LANDING_AVOIDANCE`), takes no stun or freeze during its charge
    (`status.DASH_IMMUNE_BUFFS`), and is passed over then by Lightning (`spells.STRIKE_DASH_IMMUNE`) and by the
    Inferno Tower, Inferno Dragon and Mighty Miner (`targeting.VARIABLE_DAMAGE_IMMUNE_TARGET`); a knockback resets
    his ramp (`combat.RAMP_KNOCK_RESET`).
  - The Evo Mega Knight: his deploy blow counts toward his uppercut and his dash blows do not
    (`combat.UPPERCUT_COUNT`), and he stands a moment after a throw (`combat.UPPERCUT_STAND`).
  - The Hero Mega Minion: it hides from its first warp step both ways (`combat.WARP_HIDING`), walks during its warp
    (`combat.WARP_WALK_STEP`), and stays hidden one tick longer after its strike (`combat.WARP_STRIKE_REVEAL`).
  - Targeting: a troop leaping the river takes no target in the air (`targeting.LEAP_TARGET_SCAN`); a unit in a
    knockback decides its target on the ladder (`targeting.KNOCKED_TARGET_HOLD`); a building without a projectile
    keeps a target only within its range (`targeting.BUILDING_KEEP_REACH`); a stun or freeze clears its victim's
    target a tick later (`status.STUN_CLEARS_TARGET`); the Inferno and Mighty Miner rows walk into reach as the
    client does (`targeting.VARIABLE_DAMAGE_WALK_REACH`, `targeting.VARIABLE_DAMAGE_LADDER_WALK`).
  - Shots: a straight shot tests its aim point (`combat.STRAIGHT_SHOT_RANGE_END`) and hits a building by its square
    (`combat.STRAIGHT_SHOT_BUILDING_REACH`).
  - Others: a stun or freeze resets a Sparky's charge (`combat.LOAD_FIRST_HIT_HOLD`); the Rune Giant walks on during
    his launch's hold (`enchant.ON_BUFF_HOLD_WALK`); the Clone's slide crosses the river (`spells.CLONE_SLIDE_WATER`);
    a buff's death spawn takes its victim's facing and moves on its first tick (`status.BUFF_DEATH_SPAWN_FACING`,
    `status.BUFF_DEATH_SPAWN_FIRST_UPDATE`); a unit that transforms stands on its switch tick
    (`transform.SWITCH_TICK_WALK`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles. `replay_parity` plays a tick's plays in the
  client's order. Saved battles from earlier releases load unchanged (`SNAPSHOT_FORMAT` is still 20).

## 0.1.42 (2026-10-22)

- One more setting defaults to the current client's behaviour, measured against its recorded battles: a charging
  unit (the Prince, the Dark Prince) lands its charge hit only after a step at full charge, so a run-up that fills on
  the tick before its swing gives a plain hit (`charge.CHARGED_HIT_TIMING`).
- The older behaviour stays available as the setting's old value, and `client_arms("15.535.29")` and `replay_parity`
  use it for the older client's battles. Saved battles from earlier releases load unchanged (`SNAPSHOT_FORMAT` is
  still 20).

## 0.1.41 (2026-10-21)

- Two more settings default to the current client's behaviour, each measured against its recorded battles:
  - A Royal Ghost that hides again stays hidden through the start of its next swing and is shown by the hit
    (`targeting.INVISIBILITY`).
  - A unit that takes no damage while it dashes, such as the Little Prince's guard, is not pushed then either
    (`knockback.DASH_IMMUNE_PUSH`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles. Saved battles from earlier releases load
  unchanged (`SNAPSHOT_FORMAT` is still 20).

## 0.1.40 (2026-10-20)

- A new card table: the current client's after its Season 88 mid-season balance update of 2026-10-19. Pick it with
  `calibration_overrides={"cards.CARD_TABLE": '"client160402017_20261019"'}`. It changes the Minion Giant, the Rune
  Giant (his enchant now ends 0.5 s after his death), the Cannon Cart, the Dart Goblin, the Evo Skeleton Barrel, the
  Electro Dragon, the Flying Machine, the Void, the Skeleton Barrel and the Skeletons' spawn ring. The default table
  is unchanged.
- On that table the Evo Electro Giant's pulse passes over the Clone spell's copies, as the client has done since its
  2026-10-08 content update.
- `Battle.card_table_fnv1a64()` gives the hash of the card table a Battle read, whichever table it runs.
- `tools/make_replay_fixture.py --table 160402017-20261019` makes fixtures from recordings of the new content
  (16.402.22).
- Saved battles from earlier releases load unchanged (`SNAPSHOT_FORMAT` is still 20).

## 0.1.39 (2026-10-18)

- One more setting defaults to the current client's behaviour, measured against its recorded battles: while a Little
  Prince's guard is being born, rolling spells, shots that pass through units and Lightning's picks pass over it, and
  area hits still land (`combat.GUARD_BIRTH_PASS_THROUGH`).
- The older behaviour stays available as the setting's old value, and `client_arms("15.535.29")` and `replay_parity`
  use it for the older client's battles. Saved battles from earlier releases load unchanged (`SNAPSHOT_FORMAT` is
  still 20).
- `tools/make_replay_fixture.py` refuses a recording whose content stamp the 2026-10-06 card table is not known to fit
  (16.402.19 and 16.402.21 are), since a later content update may change cards.

## 0.1.38 (2026-10-17)

- Four more settings default to the current client's behaviour, each measured against its recorded battles:
  - A ground hero whose button holds it plans a fresh route once the hold ends (`pathfinding.HELD_PRESS_ROUTE`), and
    one whose button does not hold it, a dash included, plans a fresh route on the press (`pathfinding.PRESS_ROUTE`).
  - A princess tower keeps a target past its reach only once its attack has started (`targeting.TOWER_HOLD_HIT_STARTED`).
  - A unit's buff for not attacking is on from its deploy (`status.IDLE_BUFF`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles. Saved battles from earlier releases load
  unchanged (`SNAPSHOT_FORMAT` is still 20).

## 0.1.37 (2026-10-16)

- Thirteen more settings default to the current client's behaviour, each measured against its recorded battles:
  - A dying unit's death blow hits what is in its circle when the blow lands (`combat.DEATH_BLOW_VICTIM_POINT`).
  - A Tesla that goes underground still takes hits for its hide time (`hide.HIDING_DAMAGE_WINDOW`).
  - The Evo Giant Snowball's rolling ball keeps hitting what it rolls onto (`spells.CAPTURE_ROLL_STRIKES`).
  - The Evo Mega Knight's uppercut throws toward the thrown unit's nearest standing crown tower
    (`knockback.UPPERCUT_THROW_AIM`).
  - The Evo Bats' heal after hits first pulses a full period after the hit (`status.AFTER_HITS_HEAL_PULSE`).
  - An Electro Giant's reflected stun holds a tick longer too (`status.HIT_STUN_SPAN`).
  - A unit released onto the river stands on the centre of the nearest dry half-cell
    (`spells.ROLL_RELEASE_WATER_POINT`), and a Little Prince's guard that would land in the river lands on the
    nearest land (`combat.GUARD_WATER_LANDING`).
  - The Little Prince's ramp keeps its level for the attack on the tick it resets (`combat.RAMP_LEVEL_DROP`), and a
    Fisherman's hook resets it (`combat.RAMP_HOOK_RESET`).
  - A unit standing before its dash is not pushed, and a timed dash's unit joins the contact pass the tick after it
    ends (`combat.DASH_STAND_CONTACT`, `combat.DASH_END_CONTACT`).
  - The unit a card's deploy area puts down (the Electro Wizard's, the Ice Wizard's) is not pushed on its first tick
    (`spells.DEPLOY_AREA_BIRTH_CONTACT`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles. Saved battles from earlier releases load
  unchanged (`SNAPSHOT_FORMAT` is still 20).

## 0.1.36 (2026-10-16)

- Twenty-two more settings default to the current client's behaviour, each measured against its recorded battles:
  - The Battle Healer's hits heal her friends around her (`combat.AREA_EFFECT_ON_HIT`), and the Royal Chef cooks
    pancakes that level up a friendly troop (`combat.TOWER_TROOP_COOKING`).
  - A stun from an Electro Wizard's bolt or an Electro Giant's reflect holds one tick longer (`status.HIT_STUN_SPAN`).
  - A Royal Delivery's crate lands a tick later (`spells.STRIKE_DUE`).
  - A shield takes a tick's hits one by one, and the hit that breaks it is absorbed whole (`combat.SHIELD_TICK_SUM`).
  - A tower judging whether its target is already doomed counts each shot's own damage, not its enchant bonus
    (`targeting.DOOMED_ENCHANT_BONUS`). A ranged unit whose target dies during its first attack still fires
    (`combat.LOST_TARGET_SHOT`). A snipe never picks a crown tower (`targeting.SNIPE_TOWER_FILTER`).
  - A Miner who comes up is no body to his neighbours for two ticks (`movement.SPAWN_PATHFIND_SURFACE_CONTACT`). A
    rolling Log does not hit a unit a spawner makes on that tick (`spells.LIVE_SPAWN_ROLL_BIRTH`). A pull starts a
    tick later (`status.ATTRACT_ONSET`).
  - The Little Prince's guard stays hit-immune a tick longer and keeps pushing on its landing ticks
    (`combat.GUARD_CHARGE_IMMUNITY`, `combat.GUARD_LANDING_PUSH`), and the Little Prince is not pushed while he
    attacks in his press hold (`collision.GUARD_PRESS_CONTACT`).
  - The Rune Giant is not pushed in his enchant hold (`enchant.ON_BUFF_HOLD_CONTACT`), and the Golden Knight is no
    contact body between chain hops (`movement.CHAIN_LANDED_BODY`).
  - The Evo P.E.K.K.A.'s kill heal flies to her and can heal past her maximum (`combat.EVO_KILL_HEAL`).
  - The Mother Witch's hog appears on the first dry point near where the cursed unit died
    (`status.BUFF_DEATH_SPAWN_POINT`).
  - The Evo Goblin Cage takes and holds its captive a tick or two later (`combat.CAGE_DEPLOY_TAKE`,
    `combat.CAGE_HOLD_START`), and the Evo Goblin Barrel's decoy goblins land on the real goblins' mirror points
    (`spells.EVO_BARREL_DECOY_POINTS`).
  - The Hero Tombstone's button can be pressed while its tomb fades, like the Hero Goblins' flag
    (`combat.FLAG_PRESS_WINDOW`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles. Saved battles from earlier releases load
  unchanged (`SNAPSHOT_FORMAT` is still 20).
- The card tables carry two more blocks: the area a unit puts down on each hit (`area_effect_on_hit`) and the Royal
  Chef's cooking timings (`chef_cooking`, on the Chef's king tower).

## 0.1.35 (2026-10-16)

- Eleven more settings default to the current client's behaviour, each measured against its recorded battles:
  - A unit a Witch, Tombstone or Goblin Hut makes on a Poison pulse's tick is not hit by that pulse
    (`spells.LIVE_SPAWN_AREA_BIRTH`).
  - A walker steering round a Little Prince who stands after his button press turns whatever way he faces
    (`movement.CAST_HOLD_HEADING`).
  - Lightning reaches a troop only within its radius plus the troop's own (`spells.STRIKE_REACH`). On a tie in
    hitpoints it strikes the one nearest its centre (`spells.STRIKE_TIE_BREAK`).
  - An own-side Rage reaches a rider on its mount (`rider.OWN_AREA_BUFFS_WHILE_ATTACHED`).
  - Goblinstein's tether keeps running from where its Monster died (`combat.TETHER_MONSTER_GONE`).
  - A Tornado does not pull the Little Prince while his press holds him (`status.GUARD_PRESS_ATTRACT`).
  - A ground troop laid on the river stays on the water (`formation.MEMBER_WATER_POINT`).
  - The Hero Balloon's throw can pick a building and counts the target's size (`targeting.THROW_PICK`).
  - A rolling log hits only what is under it now (`spells.ROLLING_HIT_SWEEP`), and passes over a unit mid-jump or
    mid-dash (`spells.ROLLING_SPARES_LEAPERS`).
- Each older behaviour stays available as that setting's old value, and `client_arms("15.535.29")` and
  `replay_parity` use the old values for the older client's battles. Saved battles from earlier releases load
  unchanged (`SNAPSHOT_FORMAT` is still 20).

## 0.1.34 (2026-10-13)

- One more setting defaults to the current client's behaviour, measured against its recorded battles: the Little
  Prince's guard waits one tick longer after its charge lands before it picks a target and moves
  (`combat.GUARD_FREE_TICK`). The older behaviour stays available as that setting's old value, and `replay_parity`
  uses it for the older client's battles.

## 0.1.33 (2026-10-11)

- The Hero Ice Wizard plays on the current client's card table (`calibration_overrides={"cards.CARD_TABLE":
  '"client160402017_20261006"'}`, the Ice Wizard marked as form 2 in `reset(..., forms=)`). Its button waits for the
  closest enemy its deploy chill or its shots slowed, then puts an ice cube down behind that enemy. Every enemy around
  the cube is frozen and takes a hit, and stays frozen while the cube stands: 5 seconds at most, and 2 seconds after
  nothing is left frozen. When the cube is gone they move again, slowed for 2 seconds. The timings are measured
  against 34 recorded cubes. What the button does when the hero's shot kills a unit, or the hero dies, while it waits
  is read off the game's data, not measured.
- Four more settings default to the current client's behaviour, each measured against its recorded battles. Only a
  Tesla acts on the last tick of its deploy; every other building waits for its countdown to end
  (`combat.BUILDING_DEPLOY_LAST_TICK`). A Royal Recruit or Royal Hog keeps the lane of the point it stands on
  (`targeting.SUMMON_LANE_FLIP_SCOPE`), and Royal Hogs and Royal Recruits tapped into an opened pocket stand there
  (`formation.LINE_TERRITORY`). A Poison cloud applies once more as it ends, so a unit still in it takes one more pulse
  (`spells.PULSING_AREA_LIFE`). Each older behaviour stays available as that setting's old value, and `replay_parity`
  uses the old values for the older client's battles.
- New: `royalesim.client_arms(client)`. `client_arms("15.535.29")` gives the `calibration_overrides` that run the older
  client's measured mechanics and the card tables' own values, the setup `replay_parity` uses for that client's
  battles. Any 15.535 build gets the same; `client_arms("16.402")` is `{}`.
- Each value in `calibration_overrides` is JSON text: `'"none"'` or `json.dumps("none")`, not `"none"`. The 0.1.28 and
  0.1.30 entries below now say so.

## 0.1.32 (2026-10-11)

- Two settings now default to the current client's behaviour, each measured against its recorded battles. A building
  lets go of a target that has moved out of its reach after it fires, as a crown tower does
  (`targeting.LAUNCH_BEYOND_POSITION`). The Little Prince's guard appears 2000 behind him and takes its first step on
  the tick it appears, and after its charge it finds a fresh path (`combat.GUARD_BIRTH_POINT`). Each older behaviour
  stays available as that setting's old value, and `replay_parity` uses the old values for the older client's
  battles.

## 0.1.31 (2026-10-11)

- Four settings now default to the current client's behaviour, each measured against its recorded battles. A flying
  unit that is still deploying can be pushed over the river; a ground one still stops at the bank
  (`movement.DEPLOY_WATER_EDGE`). The Little Prince is not pushed by other units while he holds his press
  (`collision.GUARD_PRESS_CONTACT`); a knockback, a hook or a pull still moves him. The Evo Electro Dragon's chain hops
  are timed as on the current client: every hop waits, the first one too (`combat.EVO_CHAIN_HOP_WAIT` and
  `combat.EVO_CHAIN_HOP_FIRST_STEP`). Each older behaviour stays available as that setting's old value, and
  `replay_parity` uses the old values for the older client's battles.
- New setting `combat.GUARD_BIRTH_POINT`: where the Little Prince's guard appears. The default is unchanged.

## 0.1.30 (2026-10-10)

- 114 more settings now default to the current client's behaviour. These are the settings where the current client
  behaves as the older client (15.535.29) does, so battles recorded on the current client replay more closely. Each
  older behaviour stays available as that setting's old value, and `replay_parity` uses the old values for the older
  client's battles.
- More settings default to the current client's behaviour, each measured against its recorded battles. The Dagger
  Duchess throws her daggers in bursts and recharges when she is not attacking (the engine now reads the card tables'
  `burst_attack`). The Golden Knight's dash chain can pick any enemy ground unit near him, buildings and towers
  included. A unit attacking a Tesla keeps it as its target while the Tesla hides. An Electro Spirit's chain goes on
  when its target dies before the hop lands. The Little Prince's guard also hits units that walk into it just after it
  lands, and his press holds him 3 ticks longer. A building can attack on the last tick of its deploy. A unit stunned
  or frozen by a hit keeps its reload timer still from one tick later. The Evo Witch's waves are made in the game's
  order, and the Evo Battle Ram's charge skips a unit that is still waiting to appear.
- Illegal taps are still refused by default: `placement.ILLEGAL_TROOP_TAP` and `spells.ILLEGAL_SPELL_TAP` ship
  `refuse`, and will keep doing so. A tap outside the legal area is usually a bug in the caller, and a refusal shows
  it. The game's clamp, which moves the tap to the edge of the legal area, is a setting you can turn on with
  `calibration_overrides`: `{"placement.ILLEGAL_TROOP_TAP": '"client15535_clamp_to_legal_edge"'}` or
  `{"spells.ILLEGAL_SPELL_TAP": '"client16402_clamp_to_legal_edge"'}` (each value is JSON).
- Six settings treat the two sides differently, as the game does. A seat-symmetry check can switch each to a symmetric
  version with `calibration_overrides`: `targeting.EQUAL_DISTANCE_TIE` to `own_frame_high_x`, `formation.LINE_FRAME`
  to `rotation`, `formation.LINE_CENTRE_SEARCH` to `ring_corner_walk`, `placement.RELOCATION_TIE_ORDER` and
  `placement.TROOP_RELOCATION_TIE_ORDER` to `placer_frame_first_found`, and `placement.ENEMY_BUILDING_TAPS` to
  `ignored`.
- The state's building footprint (the 15th element of each row) is the box a tap is checked against. A building on the
  tile grid is unchanged; a building off the grid (a hero's turret) is reported where it blocks taps.
- Tools: `tools/make_replay_fixture.py` reads more of a live capture. A unit first seen after missing frames, already
  walking, is dated so its deploy ends where it was first seen. A Goblinstein is placed from its card's own layout. An
  Evo Goblin Barrel's decoy and the Hero Goblins' flag spawns are no longer read as plays. `replay_parity` plays a
  group row given only as its centre point from its tile.

## 0.1.29 (2026-10-10)

- Six settings now default to the current client's behaviour, each measured against its recorded battles: a Rune
  Giant holds his attack for 14 ticks after each enchant; a buildings-only troop ignores a building farther across the
  board than its sight allows (`targeting.BUILDING_SCAN_X_CUT` now reads the card tables' `sight_clip_side_milli`, on
  both clients); the Little Prince's ramp holds through his own press; his guard takes no hit while it charges, and
  its push resets the pushed troop's swing; and a formation member waiting to step in takes no hit and no area. Each
  older behaviour stays available as that setting's old value, and `replay_parity` uses the old values for the older
  client's battles.
- The card tables carry the Dagger Duchess's burst attack (`burst_attack` on her unit: her charges, their recharge
  and the order of her attacks). The engine does not read it yet. The tables' hashes change with it.

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
  with `calibration_overrides={"spells.RELEASE_GROUND_POINT": '"none"'}` (the value is JSON).
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
