//! ONE CLIENT'S MEASURED SETUP, as calibration overrides (`client_overrides`; Python `royalesim.client_arms`).
//!
//! The ledger (data/calibration.json) ships the current client's mechanics (16.402) and its card values
//! (cards.CLIENT16402_VALUES at arm client16402). The older client, 15.535.29, runs some mechanics apart from it
//! (`CLIENT15535_ARMS`) and the card tables' own values. A replay of a capture that client recorded runs exactly that
//! setup (examples/replay_parity/harness.rs `config_for_with`), and a caller who wants a battle on 15.535 rules asks for
//! the same overrides here.
#![allow(unexpected_cfgs)]

use std::collections::BTreeMap;

/// The ledger key whose arm picks the card values a battle runs (state.rs `CardValuesArm`).
pub const CARD_VALUES_KEY: &str = "cards.CLIENT16402_VALUES";

/// THE MECHANICS CLIENT 15.535.29 RUNS APART FROM THE LEDGER'S (the 16.402 client's), as (`section.KEY`, JSON value): a
/// capture that client recorded (card_table.game_version) runs each, unless the run overrides its key.
///   movement.DYING_UNIT_VISIBILITY = client_doomed_static: a troop whose death is settled before the move pass is a
///   static obstacle to every avoidance scan. Measured on client 15.535.29: Oracle's sp-order-{kvm,vmk}-{1500,2500}-s0
///   scenes (the killer created before and after the mover) exact to their end under it and departing at the first kill
///   under whole_tick; sp-f4-furnace-s0 t267, sp-hogs-cannon-s0 t285, sp-ram-kill-s0 t309 exact. The 16.402 corpus keeps
///   whole_tick (20260918-122757.b2 t1067; 31 of 31 first effects of the arm on 9 battles nearer under whole_tick).
///   match.TICK_ORDER = client_sequential_strike: the Target and Attack phases one pass in creation order, a direct strike
///   landing at once, so a walker created after a melee striker turns for its next goal on the kill frame. Measured on
///   client 15.535.29 over every death with one landed blow: walkers created after the striker 114 of 124 (unit deaths)
///   and 8 of 8 (towers near) on the kill frame, before it 72 of 75 and 2 of 2 a frame later; an attacker's post-kill
///   wait ends on the sixth frame either way (187 and 376). The 16.402 corpus keeps client16402 (the ledger's
///   measured_16402_2026_09_26: 104 of 112 a frame later where the arm puts all 112 on the kill frame).
///   combat.DEATH_DAMAGE_TICK = client15535_death_tick: a dying unit's death blow lands on its death tick. Measured on
///   client 15.535.29: an enemy within a dying Golem's or Ice Golemite's death radius loses the damage on the death
///   frame, 122 of 122. The 16.402 corpus keeps next_tick (9 of 10 on the frame after).
///   knockback.LADDER_END_ROUTE = client15535_kept: a knockback ladder's end leaves the unit's route as it was (135 of 154
///   ladders on client 15.535.29). The 16.402 corpus's one ladder with a route does not separate the arms.
///   movement.HELD_FACING = client15535_toward_waypoint: a held unit with a route turns its facing toward its next
///   waypoint (143 of 163 held facing turns on client 15.535.29). The 16.402 corpus is not measured for it.
///   collision.HELD_UNIT_AVOIDANCE = scanned: a held unit starts avoidance offsets at the walking rate on client 15.535.29
///   (41 of 5,442 held ticks against 0.90% walking); the 16.402 corpus keeps masked (0 of 2,024).
///   movement.DOOMED_OWN_UPDATE = client15535_struck_stays: a kamikaze doomed before the move pass stays where the tick
///   found it (a Battle Ram killed by its own hit lays its Barbarians on its last point, 29 of 29 on client 15.535.29), and
///   so does a troop a troop struck down in the pass, or an older striker (9 of 9 melee kills, 6 of 6 Inferno kills).
///   movement.KAMIKAZE_DEATH_CONTACT = client15535_avoided_not_pushed: a dying kamikaze pushes nobody on its death tick and
///   is still steered round (18 of 18 overlapping neighbours exact on the death tick under it on client 15.535.29).
///   targeting.CHASE_HOLD_PAST_LIMIT = client15535_troops_kept: a troop keeps a target past its round sight past the
///   chase-drop limit when the pair was never inside it (135 of 139 on client 15.535.29; the 16.402 corpus let 3 of 4 go).
///   combat.LAUNCH_PAST_TARGET = client15535_homing_unclamped: a homing shot starts its whole ProjectileStartRadius out,
///   past a nearer target (the Hero Musketeer's near shots 6 of 6 on client 15.535.29).
///   placement.RELOCATION_TIE_ORDER = client15535_interleaved_ring: equally near relocations of a building tap go to the
///   first in the client's ring order, in arena coordinates, for both seats (4 of 4 Elixir Collector taps, a Cannon
///   tapped on a tile edge, 10 of 10, and a Tesla tapped on a Tesla, on client 15.535.29).
///   targeting.KNOCKED_TARGET_HOLD = client15535_sight_keep: a knocked unit keeps a target that is not a crown tower only
///   within its sight + both radii + 25 (9 of 9 let go past it on client 15.535.29, none within it).
///   targeting.SLAP_FLIGHT_TARGETABILITY = client15535_airborne: a unit in a Hero Giant's slap flight is a target only for
///   attackers that attack air (5 of 5 ground-only holders let it go, 6 of 6 air ones kept it on client 15.535.29).
///   spawner.EVO_COPY_COUNT = client15535_at_hit: an Evo Skeletons group's room for a copy is read at the hit, before the
///   tick's deaths (6 of 6 hits on a group of 8 with a member dying that tick made none on client 15.535.29).
///   spawner.CONTAINER_BURST_PUSH = client15535_contact_push: a container's members are laid around the point the contact
///   law pushes it to (a Skeleton 217 off, (-130, +73), exact on client 15.535.29); their slides end around its own.
///   movement.AVOIDANCE_OBSTACLE_TAG = client15535_unpushed_obstacle: a row's AVOIDANCE_AS_OBSTACLE tag (the Evo Skeleton
///   Army's General) makes its carrier unpushed, still pushing, and a static avoidance obstacle (93 of 93 deploy ticks
///   unmoved).
///   combat.LOAD_FIRST_HIT_LEAVE = client15535_refund_at_death: a Sparky leaving its attack before it fires gets the
///   entry's windup back (its load timer LoadTime less its progress; every Sparky leave on client 15.535.29), on the death
///   tick of a target that died after its turn (2 of 2: the engine's Sparky ran a tick late after them).
///   placement.ILLEGAL_TROOP_TAP = client15535_clamp_to_legal_edge: a troop tapped outside its territory goes down on the
///   first legal tile back along its column (4 of 4 bridge taps, 3 of 3 enemy-half singles on client 15.535.29).
///   spawner.DEATH_RING_AXIS = client15535_unit_heading: a death ring's degree is read off its members' heading, the
///   direction normalized to 256 (7 of 7 Battle Ram deaths where it rounds apart from the raw direction).
///   combat.PASS_KILL_CHASE = client15535_chaser_past_keep_reads_pass: an attacker chasing its target past its keep
///   reach reads the pass's kills and takes its next target at once (19 of 25 after the striker); inside the keep
///   reach it waits as one in reach (3 of 3).
///   movement.KNOCKED_DOOMED_AVOIDANCE = client15535_knocked_mover: a doomed troop mid-knockback (an Evo Cannon bomb's
///   victim) is no static obstacle to the avoidance scan (8 of 8 scanners kept their offset).
///   knockback.TROOP_DEATH_PUSHBACK = client15535_ladder: a dying Golem or Golemite pushes the units its death blow hits
///   on the knockback ladder of its DeathPushBack (43 of 43 enemies in reach).
///   movement.STRUCK_CONTACT_ORDER = client15535_before_pass: a troop struck down in the sequential pass is a body to no
///   mover's separation in the move pass (102 of 102 overlapping movers took no push from it), and stays when its striker
///   came before it.
///   transform.FALL_GROUNDING = client15535_late_kept_walk: a falling Evo Royal Hog leaves the air on the tick after its
///   landing tick, after the Target phase, keeping its walk (ground-only enemies took it two ticks after, 4 of 4).
///   combat.CAGE_CAPTIVE_SHOTS = client15535_before_shots_hidden_after_snap: an Evo Goblin Cage drags its captive before
///   the tick's shots step and hides it the tick after the snap (a tower's arrow landed on every dragged captive).
///   combat.LOAD_FIRST_HIT_KILL_WAIT = client15535_skipped: a Sparky whose target dies mid-swing takes its next target on
///   the tick after (4 of 4), serving no post-kill wait.
///   targeting.EQUAL_DISTANCE_TIE = client15535_later_created: of two enemies at one distance the later created is taken
///   (every exact troop tie, 5 of 5).
///   hide.SHOT_AT_HIDING_BUILDING = client15535_lands: a shot fired at a Tesla while it was up lands on it after it goes
///   under (2 of 2).
///   combat.DASH_CHAIN_IMMUNITY = client15535_whole_chain: a Golden Knight in his chain takes no hit (7 of 7 dropped).
///   targeting.GHOST_PAIR_FIRST_FRAME = client15535_untargetable: an Evo Ghost's pair is no one's target on its first frame.
///   combat.EVO_CHAIN_HOP_WAIT = client15535_two_ticks: an Evo Electro Dragon's hops after the first wait 2 ticks (67 of 77).
///   status.ATTRACT_WATER_EDGE = client15535_pull_stops: a ground unit's pulled step stops at a water cell's edge.
///   status.ATTRACT_ONSET = client_next_tick: a pulling area moves its victims on the tick after each of its own ticks
///   (the Tornado's D + 1..D + 21, the Evo Valkyrie's tornado H + 2..H + 11, every tick of both).
///   movement.CAST_HOLD_HEADING = client15535_not_counted: a unit in its own ability's cast steers no neighbour by its
///   heading (a walker passing the Skeleton King turned on his cast's first frame; read, LOW).
///   spawner.DEATH_BOMB_TIMING_SCOPE = client15535_every_bomb: a plain death bomb lands on its fuse's last tick (5 of 5).
///   knockback.DEATH_PUSHBACK = every_death_bomb_ladder: a Giant Skeleton's bomb pushes the troops it hits (1 of 1).
///   targeting.CHASE_DROP_MEASURE = client15535_lane_dy: the chase-drop limit is measured on |dy| (41 of 42 drops; 19
///   of 19 targets past it across alone kept).
///   targeting.CHASE_DROP_WALKING_AWAY = client15535_growing_away: the edge lets go of a troop that walked away as the
///   Target phase began with |dy| growing; only the drop tick's rescan passes over another one walking away.
///   targeting.SNIPE_REPICK = client15535_last_target_first: an Evo Musketeer's next snipe takes her last snipe's target
///   first (1 decisive re-pick).
///   movement.JUMP_LANDING_SCOPE = client15535_whole_tick: a river jump's lander is no body for its landing tick's first
///   updates either (1 of 1).
///   movement.HELD_WAYPOINT_TEST = client15535_run: a held unit runs its waypoint's reached test (120 of 120 pops, 3,111
///   of 3,111 keeps).
///   spawner.SPECTRAL_FIRST_UPDATE = client15535_same_tick_apart: an Evo Skeleton Army Spectral takes its first update on
///   the tick it is made (43 of 43 hold a target on their first frame), the tick's other Spectrals unseen (16 of 17
///   first-frame displacements beside another).
///   spawner.SPECTRAL_BIRTH_POINT = client15535_water_nudge: a Spectral whose soldier died over water is made 1 to its
///   right (8 of 8; 33 of 33 over land on the point).
///   transform.DISMOUNT_LEAP_STEP = client15535_leap_lands_first: a Hero Dark Prince freed on its leap's last tick takes
///   that leap step before it dismounts (1 of 1).
///   transform.DISMOUNT_HOP_WATER = client15535_land_row_centre: its hop onto the river lands on the nearest land row's
///   centre (5 of 5).
///   transform.DISMOUNT_MOUNT_BIRTH = client15535_hero_point: its mount is born on its point before its first hop and
///   takes its first update that tick, meeting its body (6 of 6 mounts exact).
///   spells.CLONE_OFFSET = client15535_column_edge_slide: a Clone's pair each walk 125 a tick toward their own column's
///   edge cell (692 of 692 slide steps).
///   spells.CLONE_HOLD_DEPLOY = client15535_covers_deploy_late_walk: a Clone holds a pair whose original is still deploying
///   for its deploy left when longer than the hold, a tick more, and the pair walks a tick after it takes its targets (6 of
///   6 units of the one such cast).
///   combat.TIMED_JUMP_BLOW_STOP = client15535_stops_at_blow: a Mega Knight's jump stops at its blow, put on land then
///   when over water (6 of 6 jumps never move past the blow).
///   status.FIRST_HIT_BUFF_COUNTDOWN = client15535_from_next_tick: an Evo Minion Horde minion's ghost is counted down from
///   the tick after its first damage (slowed through H + 60, 6 of 6).
///   combat.RETARGET_WAIT_REACH_LOSS = client15535_after_reach_loss_variable_rows: the Monk waits five ticks after his
///   knocked target leaves his reach (7 of 7 knocks), an enemy in his reach or not (2 of 2), and so does the Inferno Dragon
///   (4 of 4, 2 with an enemy in its reach, 2 re-picking the lost target).
///   formation.LINE_CENTRE_SEARCH = client15535_interleaved_first_ring: a line's place is looked for as the client walks
///   it (245 of 246 Royal Recruits taps).
///   combat.CAGE_RELEASE_SCAN = client15535_scans_while_held: an Evo Goblin Cage's freed captive takes its target on the
///   tick after the cage's death and walks the tick after (4 of 4 captives).
///   combat.GUARD_CHARGE_STEP = client15535_substeps_to_aim: the Little Prince's guard charges in pieces of 250 and 150
///   re-aimed at his point + (250, 3250) (78 of 78 steps).
///   combat.SIEGE_RESET_HOLD = client15535_two_tick_hold: the Hero Bowler swings two ticks after its siege's reset
///   (2 of 2 sieges, its shells 2 ticks later).
///   spawner.DEATH_RING_DIRECTION = client15535_facing: a unit dying on its walk lays its death ring along its heading
///   after the death tick, not its step (13 of 13 Evo Battle Rams, one sliding along the bridge's edge).
///   transform.DISMOUNT_MOUNT_ACQUIRE = client15535_8th_frame: no enemy takes the Hero Dark Prince's mount before its
///   F + 7 (every first take measured).
///   movement.DEATH_BOMB_AVOIDANCE = client15535_static_blocker: a death bomb on its fuse is a static blocker to the
///   avoidance scan (4 of 4 turns where a look circle met one).
///   combat.RAMP_GRACE_MOVE = client15535_own_walk: the Little Prince's ramp grace runs down on his own walk alone (3 of 3
///   ramps kept through pushes while he attacked, 6 of 6 reset by a walk).
///   combat.RAMP_STUN_RESTART = client15535_restart: a stun restarts the Little Prince's attack cycle (his one recorded
///   stun: progress 0 on the Zap's tick, a fresh start on the resume).
///   transform.DISMOUNT_MOUNT_HOLD_BODY = client15535_no_body: the Hero Dark Prince's held mount meets no body (the one
///   dismount that left the pair touching: neither moved until the hold ended).
///   targeting.KNOCKED_LOST_TARGET = client15535_rescans: a unit on a knockback ladder whose target is gone takes its next
///   one before the ladder ends (28 of 28 ladders).
///   targeting.SLAP_FLIGHT_SIGHT_HOLD = client15535_let_go: a target in a slap flight is let go past the chaser's round
///   sight (4 of 4 Bats).
///   combat.DEATH_BLOW_VICTIM_REAP = client15535_same_reap: a unit a death blow kills leaves the board on the death tick
///   (15 of 16).
///   knockback.COMBO_STEP_OFFSET = client15535_held_offset: a combo hit's first ladder step turns by the victim's
///   start-of-tick avoidance offset (20 of 20 held-offset ladders; the Monk's 2 on deploying victims).
///   combat.DOOMED_READ_IN_PASS = client15535_at_turn: an attacker's doomed test reads its target's hitpoints after the
///   strikes earlier in the pass (freed 4 of 4; 82 of 85 held waited).
///   combat.NET_CAST_WINDOW = client15535_shot_window: an Evo Hunter's net is cast outside his shot window, with no
///   hold (19 of 19 nets; the hold 15).
///   collision.ATTRACT_CONTACT_MEAN = client15535_in_mean: a pulled unit with a contact takes the pull as one entry of
///   its contact mean (32 of 32 pulled steps with a neighbour).
///   transform.TOMB_MONSTER_STEP_SCOPE = client15535_tomb_standing: a pressed Hero Tombstone monster steps off its tomb
///   only beside a standing tomb (6 of 6; 1 of 1 after the tomb's death stood).
///   spawner.TOMB_PRESS_SPAWN_FIRST_UPDATE = client15535_start_of_tick: a Hero Tombstone's press-kill death spawn takes
///   its first update on the start-of-tick board (16 of 16 pushes, 4 of 4 goals).
///   combat.DASH_RANGE_TARGET_POINT = client15535_pass_point: a dash's Range test reads its target's point in the move
///   pass (14 of 14 dash ends; one discriminating).
///   spawner.HERO_TOMB_EMISSION_NUDGE = client15535_right_one: a Hero Tombstone's emitted Skeleton touching its tomb
///   alone lands 1 further to its right (15 of 15; fitted).
///   collision.CAGE_CAPTIVE_AVOIDANCE = client15535_masked: an Evo Goblin Cage's captive runs no avoidance scan from
///   its grab through its release hold (761 captive frames at 0; 3 of 3 releases start -190).
///   movement.CAPTURE_DRAG_FACING = client15535_faces_ball: an Evo Giant Snowball's dragged captive faces the ball, and
///   scans along that facing (52 of 52 drag frames).
///   pathfinding.LADDER_PLAN_SEGMENT = client15535_plan_tick: a route planned on a knockback-ladder tick freezes its
///   segment on that tick from the start-of-tick point (133 of 133; 21 of 21 kept at the walk).
///   knockback.UPPERCUT_FLIGHT_CONTACT = client15535_out_of_pass: a unit in an Evo Mega Knight uppercut's flight is out
///   of the contact pass, as a slap's (12 of 12 overlap ticks).
///   spawner.CONTAINER_RING_WATER = client15535_clamp_from_burst_point: a container ring's member is born where the
///   water clamp from the burst point takes it, not ejected to land (14 of 14 on water bursts).
///   combat.DART_POISON_PULSE = client15535_pulses_singleton: an Evo Dart Goblin's poison area pulses every 5 ticks
///   through its life, one pending damage per unit, the level read at the take (47 of 52 losses; the single take 39).
///   targeting.NEWBORN_FIRST_TARGET = client15535_tick_survivors: an emitted unit's first target skips an enemy the
///   tick's landing shots will kill (19 of 19 newborns passed over an enemy gone on their birth tick).
///   pathfinding.HERO_TOMB_DUMMY_OBSTACLE = client15535_dummy_box: a dead Hero Tombstone's box stays in the path grid
///   until its visual dummy leaves (6 of 6 plans boxed; 7 of 7 switched at its departure).
///   hide.DRILL_UNDER_SHOT = client15535_dropped: a shot flying at an Evo Goblin Drill building that goes under lands
///   on nothing (1 of 1 hide with a shot in flight).
///   collision.DRILL_RISE_BODY = client15535_hide_time_plus_3: an Evo Goblin Drill building gone under meets no body
///   until T + 23 (2 of 2 overlapping units pushed first on T + 23).
///   targeting.DRILL_RISE_TARGETABLE = client15535_targetable_on_rise: an Evo Goblin Drill building is a target again
///   on its rise (2 of 2 picks with it nearest took it).
///   combat.EVO_IMPACT_AREA_ANCHOR = client15535_follows_target: the Evo Ice Spirits' impact area rides the unit its
///   shot landed on (sp-f2-ice-s0's Hog hit 4,200 off the landing point).
///   targeting.DOOMED_DROP_SPEAR_MEMBERS = client15535_projectile: an Evo Elite Barbarian member is a projectile
///   attacker for doomed targets and its spear dooms (6 of 6 post-throw rescans passed the doomed victim).
///   spells.CAPTURE_ROUTE = client15535_dropped_at_join: an Evo Giant Snowball's captive drops its route when it joins
///   the ball (0 of 60 ride frames held one; 9 of 9 planned afresh).
///   targeting.SNIPE_LOCK_RELEASE = client15535_min_plus_radius_keep_windup: an Evo Musketeer lets her snipe target go
///   below SnipeMinRange + its radius ahead and shoots it plain, her windup running on (2 of 2; 0 of 610 held steps).
///   status.BUFF_DEATH_SPAWN_UNDELAYED_DEPLOY = client15535_unit_deploy_time: a buff death spawn whose row leaves
///   DeathSpawnDeployDelay blank deploys its unit's DeployTime (the Goblin Party Rocket's goblins: 1000 ms).
///   spawner.DEATH_PROJECTILE_COPIES = client15535_per_death_spawn_member: a death leaves one death projectile for each
///   member of its own death spawn (the Super Lava Hound's two fire walls).
///   spawner.DEATH_SPAWN_PROJECTILE = client_projectile: a death whose row carries a DeathSpawnProjectile leaves it on the
///   death point, acting on the second tick after the unit's last (the Phoenix in 3 scenarios, the Goblin Demolisher in
///   5); none fired on this lane before, the Super Lava Hound's fire walls included.
///   spawner.BARREL_DROP_POINT = client15535_after_move: the Evo Skeleton Barrel's health-line drop falls from its point
///   after that tick's step (7 of 7 Skeletons of the one such drop).
///   targeting.CHASE_DROP_KNOCKED_TARGET = client15535_holds_unless_nearer: a troop holding a sliding troop target still
///   takes an enemy its rescan ranks nearer (1 of 1, a Skeleton after a Hog Rider pushed by the Log).
///   targeting.LAUNCH_BEYOND_KEEP = client15535_plain_keep: a projectile troop that launched from beyond its reach still keeps
///   its target within Range + both radii + 25 (8 of 8 within, 5 of 5 past it let go).
///   targeting.UPPERCUT_FLIGHT_TARGETABILITY = client15535_airborne: a unit in the Evo Mega Knight's uppercut flight is a target
///   only for an attacker that attacks air (1 of 1, the Mega Knight letting its flying Knight go).
///   spells.CLONE_SLIDE_ROUTE = client15535_dropped: a Clone's slide drops the unit's route on its last tick, and a fresh
///   one is planned on the next (every Clone slide read in the battery and sp-event-GlobalClone-s0).
///   combat.UPPERCUT_STAND = client15535_until_load_100: an Evo Mega Knight out of his attack after his uppercut stands until
///   his load timer reads 100 (2 of 2).
///   spawner.EVO_COPY_POINT = client15535_ahead_outward_behind_clamped: an Evo Skeletons copy point past the last
///   row's centre is clamped to it (4 of 4).
///   knockback.COMBO_PUSHBACK = client15535_ladder_armed_at_hit: the Monk's third hit pushes a walking victim from its
///   start-of-tick point with no walk on the hit's tick (2 of 2).
///   combat.DASH_CHAIN_AIM = client15535_goal_cell: a dash chain's dash steps toward its target's goal cell centre (43 of
///   45 steps exact).
///   combat.KAMIKAZE_LAUNCH_PASS = client15535_launcher_reads_start_undoomed (a launched kamikaze the homing shots in flight
///   doom stays gone to a launcher too, 4 of 4 mutual launches; otherwise client15535_launcher_reads_start): a kamikaze's
///   launch removes it for every later unit of
///   the sequential pass (10 of 10 pickers, 7 of 7 walking holders) but a kamikaze whose own launch is due, which launches
///   at it too (3 of 3 pairs of spirits launching at each other).
///   pathfinding.PRESS_ROUTE = client15535_replanned_with_dash: a ground hero's press with CastTime 0 drops its route (22 of
///   22), a Golden Knight's too (15 of 15).
///   combat.SPIN_BEGIN = client15535_next_tick: a Hero Valkyrie's spin begins the tick after her button fires (1 press).
///   formation.LINE_FRAME = client15535_y_reflection: side 1's line is placed as side 0's at the same arena x (2 of 2).
///   placement.TROOP_RELOCATION_TIE_ORDER = client15535_arena_clockwise: a troop tap's equally near relocations go to the
///   arena order -y, -x, +y, +x seen from the tap, both seats (10 of 10).
///   spawner.FIRST_STEP_DYING_CONTACT = client15535_avoidance_only: a first update meets the units dying on its tick in
///   the avoidance scan alone (1 emission).
///   movement.CHAIN_LANDED_BODY = client15535_no_body_to_end: a dash chain's champion is no contact body from a landing to
///   his chain's end (3 of 3 last landings).
///   targeting.CHASE_RESCAN_PASS_OVER = client15535_receding_or_behind: a unit walking for its tower passes over every
///   troop past the chase-drop limit whose |dy| grew (about 600 rescans), and every rescan passes over a troop past it
///   behind the unit (0 of 1,326 taken).
///   combat.DIRECT_HIT_BUFF_COUNTDOWN = client15535_landing_tick: an instant hit's buff holds one tick fewer (17 of 17).
///   spawner.SOUL_POINT_BASE = client15535_post_move: a Skeleton King's copies are drawn around his post-move point (28 of
///   34).
///   targeting.WALKING_KEEP_REACH = client15535_walking_reach: a walking Inferno Dragon keeps its target only within the
///   reach it walks to (1 of 1; every other walking holder kept, 258 of 258).
///   combat.NET_INITIAL_COOLDOWN = client15535_from_creation: an Evo Hunter's net is first ready InitialCooldown after his
///   creation (7 of 7 first nets; the old arm refuted by 4).
///   targeting.CHASE_HOLD_SCOPE = client15535_walkers_only: a holder still in its attack lets a target past round sight go
///   (16 of 19).
///   combat.WARP_HIDING = client15535_first_step: a warping Hero Mega Minion is hidden from its first warp step (1 holder).
///   combat.LOST_TARGET_SWING = client15535_runs_on_in_reach: a cancelled lock's swing runs on onto a target in reach (198 of
///   199 in-attack switches).
///   combat.EVO_CHAIN_SHOT_LAUNCH = client15535_next_tick: an Evo Electro Dragon's shot leaves a tick after his fire (21 of
///   21).
///   spawner.ACTION_GROUP_SPAWN_ORDER = client15535_reversed: an action group's same-tick spawns are created last listed
///   first (the Evo Goblin Drill's hide pair, 6 of 6).
///   combat.EVO_CHAIN_HOP_REACH = client15535_strict: an Evo Electro Dragon's hop passes over a unit at exactly its
///   radius (one unit at the boundary).
///   movement.DEFLECT_CONTACT = client15535_pushes: a deflecting Monk pushes his neighbours (a Giant sliding round him
///   took his push on 34 of 34 frames inside both radii).
///   combat.HELD_SHOT_TEST = client15535_untested: a Hunter's held pellet tests nothing while it stands (647 of 647
///   pellets; the engine's tested rule hit newborn Golemites the client's never touched).
///   movement.DEFLECT_STAY_TIME = client15535_tick_short: the Deflect keeps the Monk unpushed a tick less than its active
///   time (a Giant on him moved him on P + 97, the tick after his deflect state ended).
///   pathfinding.HELD_PRESS_ROUTE = client15535_replanned: a ground hero's press that holds it drops its route too; it
///   plans afresh when it walks again (56 of 61 held presses).
///   spawner.THROWN_UNIT_DEATH_BLOW = client15535_unreached: no death blow of a throw's landing tick reaches the unit it
///   puts down (the Hero Balloon's trooper on the golem its blow killed, 2 of 2).
///   spawner.SPECTRAL_PARENT_BLOCKER = client15535_static_walker_group: an Evo Skeleton Army Spectral's first update
///   meets its tick's dying soldiers as static blockers grouped as the walkers they were (36 of 37 first-frame signs).
///   status.HEAL_PULSE_LANDING = client15535_before_buffered_hits: a buff's heal pulse lands before the tick's buffered
///   hits (a full Recruit's Heal Spirit pulse lost before a tower's arrow; a melee strike, landed at once, still before it).
///   knockback.RECOIL_ROUTE = client15535_replanned: an Evo Battle Ram's recoil drops its route and its ladder's first
///   step plans a fresh one (16 of 16 impacts).
///   knockback.LADDER_PATH_REQUEST = client15535_on_ladder_goal_held: a knocked unit with no route plans on its ladder as a
///   walking unit would (87 of 111), and a route's goal moved for the same target waits for the ladder's end (0 of 91
///   mid-ladder). client15535_on_release, which places 165 of 166 first routes, scores lower (the r45 iso nlad, il-925e)
///   and stays a candidate.
///   spawner.SOUL_OFFSET_ROUNDING = client15535_toward_zero: a Skeleton King copy's offset divides toward zero (26 copies
///   1 nearer on their negative axes).
///   combat.FAR_SHOT_SELECT_MOMENT = client15535_at_load_start: the Evo Archer picks her arrow when her swing's load
///   starts (14 of 14 arrows).
///   combat.EVO_CHAIN_HOP_FIRST_STEP = client15535_shot_plus_two: an Evo Electro Dragon's hop first steps in the pass that
///   released it, but a first hop first steps on its shot's appearance + 2 (11 of 11 first hops; a one-step shot's lands
///   a tick after it).
///   targeting.LAUNCH_BEYOND_POSITION = client15535_after_move: a crown tower's launch is judged on its target's point after
///   the launch tick's move (59 of 59 let-gos).
///   combat.NET_LAUNCH = client15535_edge_next_tick: an Evo Hunter's net starts at his collision radius plus its
///   extra (800) and first steps on the next tick (three nets' tracks).
///   formation.LINE_LANE = client15535_line_centre: a line's members take the lane of its centre, not the tap's (2 of 2
///   lines moved across the middle).
///   pathfinding.DEATH_BOMB_OBSTACLE = client15535_fused_building: a death bomb on its fuse is a building to route
///   planning (3 of 3 routes round it).
///   combat.LOAD_TIMER_TARGET_LOSS = client15535_stands_while_held: a unit stunned, frozen or knocked back holds its load
///   timer through the hold (10-tick stuns 33 of 33, 22-tick freezes 39 of 39); every unheld loss runs it on.
///   spells.ILLEGAL_SPELL_TAP = client16402_clamp_to_legal_edge: a Log tapped beyond its legal area (the enemy half, the
///   river, the water) is cast from the first legal tile back along its column, both seats (10 of 10 casts).
///   combat.POST_KILL_DOOM_ETA = client_within_600: a victim is doomed to its attackers' post-kill wait only when the last
///   homing shot at it lands within 600 ms (a tower's arrow launched on its last tick leaves every attacker waiting; 35 of
///   35 crown-tower and 6 of 7 projectile-unit waits, 4,610 of 4,693 and 675 of 695 frees).
///   status.BUFF_DEATH_SPAWN_FIRST_UPDATE = client15535_doomed_parent: a buff's death spawn whose parent the move pass's
///   doomed mask held takes its first update on its creation tick, the parent a static blocker (the Super Witch's hog).
///   placement.ENEMY_BUILDING_TAPS = client15535_tile_box_ring: a troop tap on an enemy building's box (a Hero Musketeer's
///   turret) is moved off it by the ring search, the box's low corner rounded down to the tile grid (Oracle's turret
///   battery, 72 of 72 taps).
///   formation.ARMY_GENERAL_POINT = client15535_ground_point: the Evo Skeleton Army's General is laid off the soldiers'
///   ring centre, the play's point moved by formation.GROUND_DEPLOY_POINT (4 of 4 plays).
///   spawner.EVO_COPY_TICK = hit_tick: an Evo Skeletons copy is made on its hit's tick.
///   combat.WARP_LAW = client15535_ladder: the Hero Mega Minion's warp steps on its ladder (343, then + / - its
///   acceleration about its speed), with no return (the 15.535.29 row has none).
///   spells.ATTACHED_AREA_TIMING = first_ms_offset: a hero area hits HitSpeed - HitSpeedOffset of its age away.
///   spawner.EVO_BARREL_DROP_TICK = client15535_fuse: the Evo Skeleton Barrel's drops land on their fuse.
///   spawner.DEATH_BOMB_SPAWN_TIMING = at_fuse_end: a container's units and its hit both on T + 12 (18 of 18, 7 of 7).
///   movement.SPAWN_PATHFIND_MORPH_AREA = at_surface: the Goblin Drill's surfacing hit on the building's first frame.
///   combat.RETARGET_WAIT_WHILE_HELD = client_paused: every held Target phase pauses the post-kill wait (67 stuns).
///   spells.PULSING_AREA_EFFECT = hit_speed_offset: a listed HitSpeedOffset is waited a tick short (the engine's pulses).
///   combat.GUARD_CHARGE_IMMUNITY = none: the Little Prince's guard takes hits in its charge (unread there).
///   knockback.GUARD_PUSH_ATTACK_RESET = frozen: the guard's push freezes the pushed troop's swing (unread there).
///   formation.STAGGER_WAIT = client16402_untargetable_immovable: a waiting member takes hits (unread there).
///   combat.DASH_CHAIN_TARGETS = troops: the Golden Knight's later hops take troops alone (its battery's targets).
///   combat.DASH_TRIGGER_BUILDING = centre_plus_radius: his first dash reads every kind's circle (buildings unread).
///   hide.HELD_THROUGH_HIDE = dropped: a held Tesla is let go when it goes under (unread there).
///   combat.CHAIN_HOP_DEAD_TARGET = ends: a chain hop whose target died before it landed ends the chain (unread there).
///   combat.GUARD_LANDING_HITS = charge_steps_only: the Little Prince's guard hits on its charge's steps alone (unread there).
///   combat.GUARD_PRESS_HOLD = table_time: the Little Prince's press holds him for the hold's table time (unread there).
///   combat.BUILDING_DEPLOY_LAST_TICK = after_countdown: a building acts from the tick after its deploy ends (unread there).
///   movement.DEPLOY_WATER_EDGE = every_deploying_unit: a deploying flyer stops at the water's edge (unread there).
///   collision.GUARD_PRESS_CONTACT = pushed: the Little Prince is pushed in his press (unread there).
///   combat.GUARD_BIRTH_POINT = client15535_fixed_offset: his guard is born at (18, -1944), no step (six runs).
///   combat.GUARD_FREE_TICK = free_on_arrival_plus_1: his guard targets and steps on its arrival + 1 (unread there).
///   targeting.SUMMON_LANE_FLIP_SCOPE = every_summon: a line member's lane flipped as a ring member's (unread there).
///   spells.PULSING_AREA_LIFE = life_exclusive: a pulsing area ends on its last whole update (unread there).
///   formation.LINE_TERRITORY = own_half: a line's place on its owner's half (unread there).
///   status.WARP_BACK_CLOAK = with_the_trigger: the Boss Bandit cloaked on her trigger (its holders unread there).
///   combat.DASH_IMMUNE_HITS = discarded: a shot on a dash-immune unit dropped (combat.DASH_ATTACK's measurement there).
///   spells.LIVE_SPAWN_AREA_BIRTH = seen_on_birth_tick: a live spawn in its birth tick's area scans (unread there).
///   combat.DASH_END_CONTACT = end_tick_body: a timed dash's unit a body on its end tick (unread there).
///   spells.STRIKE_TIE_BREAK = earliest_created: a Lightning hp tie to the earliest created (its 28 casts).
///   spells.STRIKE_REACH = edge_within_radius_plus_170_now_and_next: a Lightning's reach, its 35 runs' bracket.
///   rider.OWN_AREA_BUFFS_WHILE_ATTACHED = immune: no own-side area on an attached rider (unread there).
///   combat.TETHER_MONSTER_GONE = run_ends: the tether ends with its Monster (unread there).
///   status.GUARD_PRESS_ATTRACT = pulled: a pull moves a Little Prince in his press (unread there).
///   formation.MEMBER_WATER_POINT = nearest_passable_ground: a ground member laid on water put on the bank (unread there).
///   targeting.THROW_PICK = ground_troops_centre_in_radius: the Hero Balloon throws at troops by centre (its golem scenes).
///   spells.ROLLING_HIT_SWEEP = swept_prev_to_cur: a roll's box swept from its previous centre (unread there).
///   spells.ROLLING_SPARES_LEAPERS = hits: a ground roll hits a leaping unit (unread there).
///   targeting.DOOMED_ENCHANT_BONUS = counted: the doomed sums add a shot's enchant bonus (unread there).
///   targeting.SNIPE_TOWER_FILTER = filter_towers_column: the snipe's FilterTowers alone (its row sets it there).
///   combat.FLAG_PRESS_WINDOW = window_only: the Hero Goblins' flag pressed through its window (sp-form-Goblins-hero-s0).
///   combat.DASH_STAND_CONTACT = pushed: a unit standing before its dash is pushed (unread there).
///   combat.DASH_STAND_RETARGET = cancelled: a switch mid-stand ends the stand (unread there).
///   spells.ROLL_RELEASE_WATER_POINT = nearest_passable_ground: a single release on water to the nearest land on an axis (unread there).
///   movement.SPAWN_PATHFIND_SURFACE_CONTACT = body_on_surface: a tunneller a body from its surfacing (unread there).
///   spells.LIVE_SPAWN_ROLL_BIRTH = hit_on_birth_tick: a live spawn in its birth tick's roll test (unread there).
///   status.WARP_BACK_LANDING = nominal_point: the Boss Bandit's warp on its point (unread there).
///   combat.SHIELD_TICK_SUM = tick_sum: a shield takes the tick's summed hits (unread there).
///   enchant.ON_BUFF_HOLD_CONTACT = pushed: the holding Rune Giant pushed (unread there).
///   combat.GUARD_LANDING_PUSH = stops_at_arrival: the guard's push stops at its arrival (sp-lp-live-s0).
///   spells.EVO_BARREL_DECOY_POINTS = mirrored_cast_own_formation: the decoy's own ring (sp-form-GoblinBarrel-evo-s0).
///   status.HIT_STUN_SPAN = from_landing_tick: a hit's and a reflect's stun hold their BuffTime from the landing (unread there).
///   combat.CAGE_DEPLOY_TAKE = on_deploy_end: the cage takes on its deploy's end tick (15.535.29 measured reach-case takes).
///   combat.CAGE_HOLD_START = at_grab: the captive walks on through its grab (15.535.29: 'walks on through G').
///   combat.EVO_KILL_HEAL = buff_at_kill: one pulse on the kill + 10, no overheal (sp-form-Pekka-evo-s0).
///   combat.LOST_TARGET_SHOT = cancel: a lost swing is cancelled (unread there).
///   combat.TOWER_TROOP_COOKING = not_cooked: no Chef tower in a 15.535.29 capture (unread there).
///   combat.AREA_EFFECT_ON_HIT = not_read: AreaEffectOnHit unread there (the Evo Royal Giant's Freeze measured inert).
///   spells.SPAWN_AREA_FIRST_PULSE = frequency_less_tick: the spawn heal on her first frame + 4, 9, 14, 19 (measured there).
///   spells.STRIKE_DUE = clock_at_or_below_zero: the Royal Delivery's crate on C + 39 (7 casts); 16.402's on C + 40 (ob13).
///   combat.GUARD_WATER_LANDING = in_water: the guard stands where its charge ends (unread on 15.535.29).
///   knockback.UPPERCUT_THROW_AIM = own_king: sp-form-MegaKnight-evo-s0 threw its Knight at its king.
///   combat.RAMP_HOOK_RESET = kept: unread on client 15.535.29.
///   combat.RAMP_LEVEL_DROP = with_the_count: unread on client 15.535.29.
///   spells.DEPLOY_AREA_BIRTH_CONTACT = pushed: unread on client 15.535.29.
///   spells.CAPTURE_ROLL_STRIKES = capture_tick_only: client 15.535.29's scenes struck every unit on R + 2 (no late reach).
///   combat.DEATH_BLOW_VICTIM_POINT = death_tick_reap: unread there (client15535_death_tick lands the blow in the Reap).
///   status.AFTER_HITS_HEAL_PULSE = frequency_less_tick: sp-form-Bats-evo-s0 healed on the hit + 9 and + 19.
///   hide.HIDING_DAMAGE_WINDOW = shut_at_hide: unread on client 15.535.29.
///   status.IDLE_BUFF = off_after_hit_while_attacking: sweep-SuperKnight (the area shape; the own shape's deploy unread there).
///   targeting.TOWER_HOLD_HIT_STARTED = every_tick: unread on client 15.535.29.
///   combat.GUARD_BIRTH_PASS_THROUGH = hit: unread on client 15.535.29.
///   targeting.INVISIBILITY = client_until_hit: sp-ghost-ab-s0 and the corpus battle (the hide-again on the hit + 46).
///   knockback.DASH_IMMUNE_PUSH = pushed: unread on client 15.535.29.
pub const CLIENT15535_ARMS: &[(&str, &str)] = &[
    ("movement.DYING_UNIT_VISIBILITY", "\"client_doomed_static\""),
    ("match.TICK_ORDER", "\"client_sequential_strike\""),
    ("combat.DEATH_DAMAGE_TICK", "\"client15535_death_tick\""),
    ("knockback.LADDER_END_ROUTE", "\"client15535_kept\""),
    ("movement.HELD_FACING", "\"client15535_toward_waypoint\""),
    ("collision.HELD_UNIT_AVOIDANCE", "\"scanned\""),
    ("movement.DOOMED_OWN_UPDATE", "\"client15535_struck_stays\""),
    ("movement.KAMIKAZE_DEATH_CONTACT", "\"client15535_avoided_not_pushed\""),
    ("targeting.CHASE_HOLD_PAST_LIMIT", "\"client15535_troops_kept\""),
    ("combat.LAUNCH_PAST_TARGET", "\"client15535_homing_unclamped\""),
    ("placement.RELOCATION_TIE_ORDER", "\"client15535_interleaved_ring\""),
    ("targeting.KNOCKED_TARGET_HOLD", "\"client15535_sight_keep\""),
    ("targeting.SLAP_FLIGHT_TARGETABILITY", "\"client15535_airborne\""),
    ("spawner.EVO_COPY_COUNT", "\"client15535_at_hit\""),
    ("spawner.CONTAINER_BURST_PUSH", "\"client15535_contact_push\""),
    ("movement.AVOIDANCE_OBSTACLE_TAG", "\"client15535_unpushed_obstacle\""),
    ("placement.ILLEGAL_TROOP_TAP", "\"client15535_clamp_to_legal_edge\""),
    ("combat.LOAD_FIRST_HIT_LEAVE", "\"client15535_refund_at_death\""),
    ("spawner.DEATH_RING_AXIS", "\"client15535_unit_heading\""),
    ("combat.PASS_KILL_CHASE", "\"client15535_chaser_past_keep_reads_pass\""),
    ("movement.KNOCKED_DOOMED_AVOIDANCE", "\"client15535_knocked_mover\""),
    ("knockback.TROOP_DEATH_PUSHBACK", "\"client15535_ladder\""),
    ("movement.STRUCK_CONTACT_ORDER", "\"client15535_before_pass\""),
    ("transform.FALL_GROUNDING", "\"client15535_late_kept_walk\""),
    ("combat.CAGE_CAPTIVE_SHOTS", "\"client15535_before_shots_hidden_after_snap\""),
    ("combat.LOAD_FIRST_HIT_KILL_WAIT", "\"client15535_skipped\""),
    ("targeting.EQUAL_DISTANCE_TIE", "\"client15535_later_created\""),
    ("hide.SHOT_AT_HIDING_BUILDING", "\"client15535_lands\""),
    ("combat.DASH_CHAIN_IMMUNITY", "\"client15535_whole_chain\""),
    ("targeting.GHOST_PAIR_FIRST_FRAME", "\"client15535_untargetable\""),
    ("combat.EVO_CHAIN_HOP_WAIT", "\"client15535_two_ticks\""),
    ("status.ATTRACT_WATER_EDGE", "\"client15535_pull_stops\""),
    ("status.ATTRACT_ONSET", "\"client_next_tick\""),
    ("movement.CAST_HOLD_HEADING", "\"client15535_not_counted\""),
    ("spawner.DEATH_BOMB_TIMING_SCOPE", "\"client15535_every_bomb\""),
    ("knockback.DEATH_PUSHBACK", "\"every_death_bomb_ladder\""),
    ("targeting.CHASE_DROP_MEASURE", "\"client15535_lane_dy\""),
    ("targeting.CHASE_DROP_WALKING_AWAY", "\"client15535_growing_away\""),
    ("targeting.SNIPE_REPICK", "\"client15535_last_target_first\""),
    ("movement.JUMP_LANDING_SCOPE", "\"client15535_whole_tick\""),
    ("movement.HELD_WAYPOINT_TEST", "\"client15535_run\""),
    ("spawner.SPECTRAL_FIRST_UPDATE", "\"client15535_same_tick_apart\""),
    ("spawner.SPECTRAL_BIRTH_POINT", "\"client15535_water_nudge\""),
    ("transform.DISMOUNT_LEAP_STEP", "\"client15535_leap_lands_first\""),
    ("transform.DISMOUNT_HOP_WATER", "\"client15535_land_row_centre\""),
    ("transform.DISMOUNT_MOUNT_BIRTH", "\"client15535_hero_point\""),
    ("spells.CLONE_OFFSET", "\"client15535_column_edge_slide\""),
    ("spells.CLONE_HOLD_DEPLOY", "\"client15535_covers_deploy_late_walk\""),
    ("combat.TIMED_JUMP_BLOW_STOP", "\"client15535_stops_at_blow\""),
    ("status.FIRST_HIT_BUFF_COUNTDOWN", "\"client15535_from_next_tick\""),
    ("combat.RETARGET_WAIT_REACH_LOSS", "\"client15535_after_reach_loss_variable_rows\""),
    ("formation.LINE_CENTRE_SEARCH", "\"client15535_interleaved_first_ring\""),
    ("combat.CAGE_RELEASE_SCAN", "\"client15535_scans_while_held\""),
    ("combat.GUARD_CHARGE_STEP", "\"client15535_substeps_to_aim\""),
    ("combat.SIEGE_RESET_HOLD", "\"client15535_two_tick_hold\""),
    ("spawner.DEATH_RING_DIRECTION", "\"client15535_facing\""),
    ("transform.DISMOUNT_MOUNT_ACQUIRE", "\"client15535_8th_frame\""),
    ("transform.DISMOUNT_MOUNT_HOLD_BODY", "\"client15535_no_body\""),
    ("targeting.KNOCKED_LOST_TARGET", "\"client15535_rescans\""),
    ("targeting.SLAP_FLIGHT_SIGHT_HOLD", "\"client15535_let_go\""),
    ("combat.DEATH_BLOW_VICTIM_REAP", "\"client15535_same_reap\""),
    ("knockback.COMBO_STEP_OFFSET", "\"client15535_held_offset\""),
    ("combat.DOOMED_READ_IN_PASS", "\"client15535_at_turn\""),
    ("combat.NET_CAST_WINDOW", "\"client15535_shot_window\""),
    ("collision.ATTRACT_CONTACT_MEAN", "\"client15535_in_mean\""),
    ("transform.TOMB_MONSTER_STEP_SCOPE", "\"client15535_tomb_standing\""),
    ("spawner.TOMB_PRESS_SPAWN_FIRST_UPDATE", "\"client15535_start_of_tick\""),
    ("combat.DASH_RANGE_TARGET_POINT", "\"client15535_pass_point\""),
    ("spawner.HERO_TOMB_EMISSION_NUDGE", "\"client15535_right_one\""),
    ("collision.CAGE_CAPTIVE_AVOIDANCE", "\"client15535_masked\""),
    ("movement.CAPTURE_DRAG_FACING", "\"client15535_faces_ball\""),
    ("pathfinding.LADDER_PLAN_SEGMENT", "\"client15535_plan_tick\""),
    ("knockback.UPPERCUT_FLIGHT_CONTACT", "\"client15535_out_of_pass\""),
    ("spawner.CONTAINER_RING_WATER", "\"client15535_clamp_from_burst_point\""),
    ("combat.DART_POISON_PULSE", "\"client15535_pulses_singleton\""),
    ("targeting.NEWBORN_FIRST_TARGET", "\"client15535_tick_survivors\""),
    ("pathfinding.HERO_TOMB_DUMMY_OBSTACLE", "\"client15535_dummy_box\""),
    ("hide.DRILL_UNDER_SHOT", "\"client15535_dropped\""),
    ("collision.DRILL_RISE_BODY", "\"client15535_hide_time_plus_3\""),
    ("targeting.DRILL_RISE_TARGETABLE", "\"client15535_targetable_on_rise\""),
    ("combat.EVO_IMPACT_AREA_ANCHOR", "\"client15535_follows_target\""),
    ("targeting.DOOMED_DROP_SPEAR_MEMBERS", "\"client15535_projectile\""),
    ("spells.CAPTURE_ROUTE", "\"client15535_dropped_at_join\""),
    ("targeting.SNIPE_LOCK_RELEASE", "\"client15535_min_plus_radius_keep_windup\""),
    ("status.BUFF_DEATH_SPAWN_UNDELAYED_DEPLOY", "\"client15535_unit_deploy_time\""),
    ("spawner.DEATH_PROJECTILE_COPIES", "\"client15535_per_death_spawn_member\""),
    ("spawner.DEATH_SPAWN_PROJECTILE", "\"client_projectile\""),
    ("spawner.BARREL_DROP_POINT", "\"client15535_after_move\""),
    ("targeting.CHASE_DROP_KNOCKED_TARGET", "\"client15535_holds_unless_nearer\""),
    ("targeting.LAUNCH_BEYOND_KEEP", "\"client15535_plain_keep\""),
    ("targeting.UPPERCUT_FLIGHT_TARGETABILITY", "\"client15535_airborne\""),
    ("spells.CLONE_SLIDE_ROUTE", "\"client15535_dropped\""),
    ("combat.UPPERCUT_STAND", "\"client15535_until_load_100\""),
    ("spawner.EVO_COPY_POINT", "\"client15535_ahead_outward_behind_clamped\""),
    ("movement.DEATH_BOMB_AVOIDANCE", "\"client15535_static_blocker\""),
    ("combat.RAMP_GRACE_MOVE", "\"client15535_own_walk\""),
    ("combat.RAMP_STUN_RESTART", "\"client15535_restart\""),
    ("knockback.COMBO_PUSHBACK", "\"client15535_ladder_armed_at_hit\""),
    ("combat.DASH_CHAIN_AIM", "\"client15535_goal_cell\""),
    ("combat.KAMIKAZE_LAUNCH_PASS", "\"client15535_launcher_reads_start_undoomed\""),
    ("pathfinding.PRESS_ROUTE", "\"client15535_replanned_with_dash\""),
    ("combat.SPIN_BEGIN", "\"client15535_next_tick\""),
    ("formation.LINE_FRAME", "\"client15535_y_reflection\""),
    ("placement.TROOP_RELOCATION_TIE_ORDER", "\"client15535_arena_clockwise\""),
    ("spawner.FIRST_STEP_DYING_CONTACT", "\"client15535_avoidance_only\""),
    ("movement.CHAIN_LANDED_BODY", "\"client15535_no_body_to_end\""),
    ("targeting.CHASE_RESCAN_PASS_OVER", "\"client15535_receding_or_behind\""),
    ("combat.DIRECT_HIT_BUFF_COUNTDOWN", "\"client15535_landing_tick\""),
    ("spawner.SOUL_POINT_BASE", "\"client15535_post_move\""),
    ("targeting.WALKING_KEEP_REACH", "\"client15535_walking_reach\""),
    ("combat.NET_INITIAL_COOLDOWN", "\"client15535_from_creation\""),
    ("targeting.CHASE_HOLD_SCOPE", "\"client15535_walkers_only\""),
    ("combat.WARP_HIDING", "\"client15535_first_step\""),
    ("combat.LOST_TARGET_SWING", "\"client15535_runs_on_in_reach\""),
    ("combat.EVO_CHAIN_SHOT_LAUNCH", "\"client15535_next_tick\""),
    ("spawner.ACTION_GROUP_SPAWN_ORDER", "\"client15535_reversed\""),
    ("combat.EVO_CHAIN_HOP_REACH", "\"client15535_strict\""),
    ("movement.DEFLECT_CONTACT", "\"client15535_pushes\""),
    ("movement.DEFLECT_STAY_TIME", "\"client15535_tick_short\""),
    ("combat.HELD_SHOT_TEST", "\"client15535_untested\""),
    ("pathfinding.HELD_PRESS_ROUTE", "\"client15535_replanned\""),
    ("spawner.THROWN_UNIT_DEATH_BLOW", "\"client15535_unreached\""),
    ("spawner.SPECTRAL_PARENT_BLOCKER", "\"client15535_static_walker_group\""),
    ("status.HEAL_PULSE_LANDING", "\"client15535_before_buffered_hits\""),
    ("knockback.RECOIL_ROUTE", "\"client15535_replanned\""),
    ("knockback.LADDER_PATH_REQUEST", "\"client15535_on_ladder_goal_held\""),
    ("spawner.SOUL_OFFSET_ROUNDING", "\"client15535_toward_zero\""),
    ("combat.FAR_SHOT_SELECT_MOMENT", "\"client15535_at_load_start\""),
    ("combat.EVO_CHAIN_HOP_FIRST_STEP", "\"client15535_shot_plus_two\""),
    ("targeting.LAUNCH_BEYOND_POSITION", "\"client15535_after_move\""),
    ("combat.NET_LAUNCH", "\"client15535_edge_next_tick\""),
    ("formation.LINE_LANE", "\"client15535_line_centre\""),
    ("pathfinding.DEATH_BOMB_OBSTACLE", "\"client15535_fused_building\""),
    ("combat.LOAD_TIMER_TARGET_LOSS", "\"client15535_stands_while_held\""),
    ("spells.ILLEGAL_SPELL_TAP", "\"client16402_clamp_to_legal_edge\""),
    ("combat.POST_KILL_DOOM_ETA", "\"client_within_600\""),
    ("status.BUFF_DEATH_SPAWN_FIRST_UPDATE", "\"client15535_doomed_parent\""),
    ("placement.ENEMY_BUILDING_TAPS", "\"client15535_tile_box_ring\""),
    ("formation.ARMY_GENERAL_POINT", "\"client15535_ground_point\""),
    ("spawner.EVO_COPY_TICK", "\"hit_tick\""),
    ("combat.WARP_LAW", "\"client15535_ladder\""),
    ("spells.ATTACHED_AREA_TIMING", "\"first_ms_offset\""),
    ("spawner.EVO_BARREL_DROP_TICK", "\"client15535_fuse\""),
    ("spawner.DEATH_BOMB_SPAWN_TIMING", "\"at_fuse_end\""),
    ("movement.SPAWN_PATHFIND_MORPH_AREA", "\"at_surface\""),
    ("combat.RETARGET_WAIT_WHILE_HELD", "\"client_paused\""),
    ("spells.PULSING_AREA_EFFECT", "\"hit_speed_offset\""),
    ("combat.GUARD_CHARGE_IMMUNITY", "\"none\""),
    ("knockback.GUARD_PUSH_ATTACK_RESET", "\"frozen\""),
    ("formation.STAGGER_WAIT", "\"client16402_untargetable_immovable\""),
    ("combat.DASH_CHAIN_TARGETS", "\"troops\""),
    ("combat.DASH_TRIGGER_BUILDING", "\"centre_plus_radius\""),
    ("hide.HELD_THROUGH_HIDE", "\"dropped\""),
    ("combat.CHAIN_HOP_DEAD_TARGET", "\"ends\""),
    ("combat.GUARD_LANDING_HITS", "\"charge_steps_only\""),
    ("combat.GUARD_PRESS_HOLD", "\"table_time\""),
    ("combat.BUILDING_DEPLOY_LAST_TICK", "\"after_countdown\""),
    ("movement.DEPLOY_WATER_EDGE", "\"every_deploying_unit\""),
    ("collision.GUARD_PRESS_CONTACT", "\"pushed\""),
    ("combat.GUARD_BIRTH_POINT", "\"client15535_fixed_offset\""),
    ("combat.GUARD_FREE_TICK", "\"free_on_arrival_plus_1\""),
    ("targeting.SUMMON_LANE_FLIP_SCOPE", "\"every_summon\""),
    ("spells.PULSING_AREA_LIFE", "\"life_exclusive\""),
    ("formation.LINE_TERRITORY", "\"own_half\""),
    ("status.WARP_BACK_CLOAK", "\"with_the_trigger\""),
    ("combat.DASH_IMMUNE_HITS", "\"discarded\""),
    ("spells.LIVE_SPAWN_AREA_BIRTH", "\"seen_on_birth_tick\""),
    ("combat.DASH_END_CONTACT", "\"end_tick_body\""),
    ("spells.STRIKE_TIE_BREAK", "\"earliest_created\""),
    ("spells.STRIKE_REACH", "\"edge_within_radius_plus_170_now_and_next\""),
    ("rider.OWN_AREA_BUFFS_WHILE_ATTACHED", "\"immune\""),
    ("combat.TETHER_MONSTER_GONE", "\"run_ends\""),
    ("status.GUARD_PRESS_ATTRACT", "\"pulled\""),
    ("formation.MEMBER_WATER_POINT", "\"nearest_passable_ground\""),
    ("targeting.THROW_PICK", "\"ground_troops_centre_in_radius\""),
    ("spells.ROLLING_HIT_SWEEP", "\"swept_prev_to_cur\""),
    ("spells.ROLLING_SPARES_LEAPERS", "\"hits\""),
    ("targeting.DOOMED_ENCHANT_BONUS", "\"counted\""),
    ("targeting.SNIPE_TOWER_FILTER", "\"filter_towers_column\""),
    ("combat.FLAG_PRESS_WINDOW", "\"window_only\""),
    ("combat.DASH_STAND_CONTACT", "\"pushed\""),
    ("combat.DASH_STAND_RETARGET", "\"cancelled\""),
    ("spells.ROLL_RELEASE_WATER_POINT", "\"nearest_passable_ground\""),
    ("movement.SPAWN_PATHFIND_SURFACE_CONTACT", "\"body_on_surface\""),
    ("spells.LIVE_SPAWN_ROLL_BIRTH", "\"hit_on_birth_tick\""),
    ("status.WARP_BACK_LANDING", "\"nominal_point\""),
    ("combat.SHIELD_TICK_SUM", "\"tick_sum\""),
    ("enchant.ON_BUFF_HOLD_CONTACT", "\"pushed\""),
    ("combat.GUARD_LANDING_PUSH", "\"stops_at_arrival\""),
    ("spells.EVO_BARREL_DECOY_POINTS", "\"mirrored_cast_own_formation\""),
    ("status.HIT_STUN_SPAN", "\"from_landing_tick\""),
    ("combat.CAGE_DEPLOY_TAKE", "\"on_deploy_end\""),
    ("combat.CAGE_HOLD_START", "\"at_grab\""),
    ("combat.EVO_KILL_HEAL", "\"buff_at_kill\""),
    ("combat.LOST_TARGET_SHOT", "\"cancel\""),
    ("combat.TOWER_TROOP_COOKING", "\"not_cooked\""),
    ("combat.AREA_EFFECT_ON_HIT", "\"not_read\""),
    ("spells.SPAWN_AREA_FIRST_PULSE", "\"frequency_less_tick\""),
    ("spells.STRIKE_DUE", "\"clock_at_or_below_zero\""),
    ("combat.GUARD_WATER_LANDING", "\"in_water\""),
    ("knockback.UPPERCUT_THROW_AIM", "\"own_king\""),
    ("combat.RAMP_HOOK_RESET", "\"kept\""),
    ("combat.RAMP_LEVEL_DROP", "\"with_the_count\""),
    ("spells.DEPLOY_AREA_BIRTH_CONTACT", "\"pushed\""),
    ("spells.CAPTURE_ROLL_STRIKES", "\"capture_tick_only\""),
    ("combat.DEATH_BLOW_VICTIM_POINT", "\"death_tick_reap\""),
    ("status.AFTER_HITS_HEAL_PULSE", "\"frequency_less_tick\""),
    ("hide.HIDING_DAMAGE_WINDOW", "\"shut_at_hide\""),
    ("status.IDLE_BUFF", "\"off_after_hit_while_attacking\""),
    ("targeting.TOWER_HOLD_HIT_STARTED", "\"every_tick\""),
    ("combat.GUARD_BIRTH_PASS_THROUGH", "\"hit\""),
    ("targeting.INVISIBILITY", "\"client_until_hit\""),
    ("knockback.DASH_IMMUNE_PUSH", "\"pushed\""),
];

/// THE CALIBRATION OVERRIDES THAT RUN ONE CLIENT'S MEASURED SETUP, `section.KEY` -> JSON value, ready for a battle's
/// `calibration_overrides` (state.rs `Calib::shipped_with_overrides`):
///   - "16.402" or "16.402.<build>": none. The ledger ships the current client's setup.
///   - "15.535" or "15.535.<build>": every `CLIENT15535_ARMS` pair, and cards.CLIENT16402_VALUES at arm none, so the
///     card tables' own values run (the Fire Spirits' Hitpoints 85, not the current client's 84). This is what a replay
///     of a 15.535.29 capture runs. It was measured on 15.535.29; another 15.535 build is given the same.
///
/// Any other client is an error that names the two.
pub fn client_overrides(client: &str) -> Result<BTreeMap<String, String>, String> {
    let is = |v: &str| {
        client == v
            || client
                .strip_prefix(v)
                .and_then(|rest| rest.strip_prefix('.'))
                .is_some_and(|b| !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()))
    };
    if is("16.402") {
        return Ok(BTreeMap::new());
    }
    if !is("15.535") {
        return Err(format!("client {client:?}: the known clients are \"15.535.29\" and \"16.402\""));
    }
    let mut out: BTreeMap<String, String> = CLIENT15535_ARMS.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    // PLANT client_arms_keep_card_values (tests/client_arms.rs): the 15.535 setup leaves the current client's values on.
    #[cfg(not(clash_plant = "client_arms_keep_card_values"))]
    {
        let doc: serde_json::Value =
            serde_json::from_str(crate::py::EMBEDDED_CALIBRATION_JSON).map_err(|e| format!("the compiled-in ledger: {e}"))?;
        let (section, key) = CARD_VALUES_KEY.split_once('.').ok_or_else(|| format!("{CARD_VALUES_KEY} is not section.KEY"))?;
        let mut value = doc
            .get(section)
            .and_then(|s| s.get(key))
            .and_then(|e| e.get("value"))
            .cloned()
            .ok_or_else(|| format!("{CARD_VALUES_KEY} has no value in the ledger"))?;
        let obj = value.as_object_mut().ok_or_else(|| format!("{CARD_VALUES_KEY}'s value is not an object"))?;
        obj.insert("arm".to_string(), serde_json::Value::String("none".to_string()));
        out.insert(CARD_VALUES_KEY.to_string(), value.to_string());
    }
    Ok(out)
}
