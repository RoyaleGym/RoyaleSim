#!/usr/bin/env python3
r"""A live capture -> a whole-battle REPLAY FIXTURE (script + truth) for replay_parity.rs.

    python tools/make_replay_fixture.py <capture.native.oracle.jsonl.gz>
                    [--placements <p.jsonl> ...] [--out <dir>] [--truth-stride N] [--until-tick T]
    python tools/make_replay_fixture.py --all [--reports <dir>] [--out <dir>] [--truth-stride N]
    python tools/make_replay_fixture.py <capture> --check <fixture.json>   # STALE or current

    <capture> is a *.native.oracle.jsonl.gz file, or its fixture name (NAMES below), which
    the tool looks up in --reports. `--check` ignores the `census` note, which records the
    state of the run rather than anything about the battle.

THE COMMITTED SAMPLE
    crates/royalesim/tests/fixtures/replay/sample.json is this tool run on the capture
    20260920-003751-B, cut at tick 1440. With ROYALELIVE_REPORTS set to the captures folder,
    this says whether it is still current:

      python tools/make_replay_fixture.py 20260920-003751-B --until-tick 1440 \
          --check crates/royalesim/tests/fixtures/replay/sample.json

    It prints "is current" and exits 0, or prints STALE and exits 1. To rebuild the sample,
    run the same command with `--out <dir>` in place of `--check ...`, then copy
    <dir>/20260920-003751-B.replay.json over sample.json. tests/test_replay_fixture.py runs
    the check. Without ROYALELIVE_REPORTS that test skips, and a skip there is not a pass.

    ROYALELIVE_REPORTS  the captures folder (the default for --reports; required for
                        --all when --reports is not given)

    Default output dir: data/derived/replay/ (gitignored). `--all` walks every
    *.native.oracle.jsonl.gz in --reports and writes the MANIFEST (manifest.json)
    beside the fixtures: every capture, playable or not, with why.

WHAT A FIXTURE IS
    The capture is a per-tick record of the real game's entity table (the ground-truth
    captures of the live client, CR 16.402): both sides, whole battle. This tool splits
    it into

      script   what a player DID: the decks, the tower level and starting hp, each
               side's card levels, and the DEPLOY LIST -- one deploy per group of the
               card's OWN units that first appears on one tick with one (side, card),
               at that tick, plus the spell casts the capture's `effects` stream shows.
      truth    what the game then SHOWED: per frame tick, per entity, the columns the
               harness scores (side, card, x, y, hp, alive, target, path-node count,
               behavior_state), the path's cells and the attack timers (ATTACK TIMERS
               below), run-length encoded per entity column; and per frame, each side's
               elixir when the capture carries it (ELIXIR below).

    replay_parity.rs plays the script through the engine and scores the engine's
    entities against the truth, entity by entity, tick by tick.

THE SIDE CONVENTION (written into every fixture as `frame`)
    Native side 0 = engine Blue, native side 1 = engine Red, and native millitiles map
    to engine subtiles by the identity scale (x18): in every capture native side 0
    defends LOW y (its king at y 3000, the header's `towers`), which is exactly what
    Blue does in the engine (lib.rs `Team`: "Blue defends the low-y side"). So nothing
    is rotated. If a capture ever recorded side 0 at the top, the tool takes it that the
    capture's coordinates were turned on the way (the game always puts side 0 at the
    bottom), turns every position of the capture back by (W - x, H - y) in native units
    (18000 x 32000) -- entities, path cells, spell objects -- KEEPING the sides, and says
    so in `frame.transform`. Turning AND swapping the sides would not do it: that pair is
    the game's seat symmetry (tests/common/mod.rs `mirror()`), which maps a battle to an
    equivalent one with side 0 still at the top (until 2026-09-22 this branch did both,
    and put Blue's king at y 29000). The placements log's taps are not turned: the log's
    own rule (below) already gives the game's frame. The absolute frame is kept on
    purpose: the shipped pathfinder is the game's own absolute-grid search (not
    seat-symmetric, tests/mirror.rs), so a rotated replay would score a different
    tie-break than the game ran.

A DEPLOY'S FORM (`form`, `form_row`, and the fixture's `forms_read`)
    A play's units carry its form's own card id: class 13 for an evolved play (the
    spells_evolved.csv row: Skeletons_EV1 13000010, Musketeer_EV1 13000014, Cannon_EV1
    13000096, measured on client 15.535.29 in scripted scenes), class 203 for a hero play
    (spells_hero_form.csv: 203000014 the hero Musketeer on every play of a hero-form slot in
    five battles on client 16.402), and the plain card id for a base play. So each deploy is
    published with `form` "ev1", "hero" or "base" and `form_row` the form's row
    (Skeletons_EV1, Musketeer_hero, Skeletons), and the id table names a class-13 unit by its
    base card, as it does a class-203 one. `forms_read` says how each form was read. No
    evolved play is recorded on client 16.402 yet. OPEN: the copies an evolved unit makes
    (Skeleton_EV1's SkeletonDuplication) carry the evolved id and the card's own hp, so they
    would be taken for untapped deploys; no 16.402 capture holds one to measure the rule.

DEPLOYS VERSUS SPAWNS
    A truth entity carries its CARD's id even when it was not deployed: Tombstone
    Skeletons carry 27000009, Golemites the Golem's id, a Battle Ram's Barbarians
    26000036, a Witch's Skeletons the Witch's (spawner troops carry the spawner's card
    id in every capture). Each entity is classed by its
    `max_hp` at its `level` against the hitpoints of every object its card can put on
    the board (cards.json: the card's own summon, its second summon, its spawner /
    death-spawn / spell-released units, the building a tunnel leaves (`spawn_pathfind`
    `morph`: the Goblin Drill's) and an interval spawner's unit (`interval_spawner`: the
    Furnace's Fire Spirits), each unit's own spawners and death spawn below it, on the
    ladder cards.json gives each one -- the same resolution
    tools/make_live_levels_fixture.py uses): an entity whose hp is
    the card's OWN unit's (or its second summon's) is a deploy summon; any other is a
    SPAWNED unit and is truth only, never a deploy. An hp that matches no object (a
    16.402 balance delta) takes the nearest object within NEAREST_MAX_ERROR_PERCENT
    and is flagged `hp_match: nearest`; beyond that it is an UNKNOWN OBJECT (role
    `unknown_object`, truth only, never a deploy and never paired: the game put
    something on the board under the card's id that cards.json does not derive from
    the card). Until 2026-09-27 the Goblin Drill's building and its Goblins and the
    Furnace's Fire Spirits were unknown objects: the resolution did not follow a
    tunnel's morph or an interval spawner, so they were never paired. An
    unknown-object group that coincides with a deploy TAP of its card makes the fixture
    unplayable from that tick (the engine cannot reproduce the deploy). A cross-check
    is recorded per group: a deploy group of a card with a spawner or death_spawn block
    (mechanic_register.json families) is expected to be that card's own unit; a spawned
    group's tick is expected NOT to coincide with a hand card leaving (a tap for that
    card in the placements log within the tap window). The fixture records the FNV-1a
    64 hash of the cards.json it was classified against (`cards_json_fnv1a64`); the
    harness notes a mismatch with the engine's, the manifest says when the census is
    stale.

DEPLOY POSITION AND TICK
    The tick is the tick the entities came to exist with their deploy timer running
    (kind 14, behavior_state 4); the harness issues the deploy so that the engine's
    units materialise on that tick (spawn_unit the tick before). The captures miss
    frames (below), so the first frame a group is SEEN in can be later than its spawn.
    Two constraints pin it: the spawn lies in (previous frame's tick, first-seen tick],
    and the deploy-end transition (the first frame with behavior_state != 4, at tick
    t1, after a state-4 frame at t0) lies at spawn + DeployTime / 50 ms - 1 -- measured
    on every gap-free spawn of the corpus (19 ticks for the 1000 ms cards, 23
    for the Princess's 1200, 69 for the X-Bow's 3500; the action-graph huts flip at 10
    and are not simulable anyway). The intersection is the spawn tick; when it still
    holds several ticks and the group is a SINGLE unit, its first step decides
    (`first_step_spawn_tick`: the first frame off the spawn point, when the frame
    before it was seen, is spawn + DeployTime / 50) if that lands inside the range;
    else the EARLIEST is used and `tick_evidence` says "range [a, b]"; when the range is
    empty the first-seen tick is used and `tick_evidence` says so. `first_seen` keeps
    the raw frame tick.

    Why the earliest: a range is left by a missed frame, and the missed frame is where
    the group was created. On the 16.402 corpus, 12 of 12 range rows whose battle the
    other seat recorded without the gap (Tombstone, Tesla x2, Royal Hogs, Goblin Hut x2,
    Bats, Goblin Drill x2, Heal, Skeleton Warriors, Skeleton Army) were created on the
    range's first tick, and 0 on its last; the lifetime drain of the three buildings among
    them with a clean drain dates them the same way, and so do the first steps of the one
    single-seat group checked (20260918-122757.b1 Goblins, 2414 of [2414, 2415]). The
    frame of a creation tick is missed more often than others (26 % of 643 creation ticks
    one seat pins, against 18 % of all ticks). Every range in the corpus is two ticks
    wide; a wider one has no evidence.

    A capture can also SHOW a group late: some ticks after the spawn its own deploy
    timers give, later than any missed frame explains (`shown_late_spawn`). The frame
    gap then bounds the showing, not the spawn, and the group's first deploy end decides
    alone, narrowed by its other members' ends at the card's stagger where they all
    agree. On the 16.402 corpus (73 fixtures) this moves 3 deploy rows: side 1's Goblins
    of 20260920-010218, shown on 572 by seat B and on 577 by seat A, whose deploy ends
    give a spawn on 569 in both seats; and side 0's Knight of 20260919-181741, which
    seat B shows on 1192 and seat A on 1193 (spawn 1192).

    A card that travels UNDER GROUND (`spawn_pathfind`: the Miner, the Goblin Drill) is
    timed by its tunnel instead (`tunnel_spawn_ticks`): its first frame stands two
    SpawnPathfindSpeed steps from its King's centre and each tick after it one step
    more, so a tunnel first seen n steps out had its first frame n - 2 ticks before (a
    Miner first seen already surfaced shows its last tunnel point as x2, y2). Its deploy
    timer starts when it SURFACES, so the deploy-end rule would time the surfacing. On
    20260920-083112-A this moves three rows by one tick, onto the ticks seat B saw.

    The position is the TAP when the placements log recorded beside the capture (the
    taps a scripted player made: side, card, tick, requested tile) has one for that
    side and card in the window [tap tick + 5, tap tick + 80] (the tap-to-entity
    latency in the captures runs 23-38 ticks) and the group has several members:
    `requested` (screen tiles)
    converted to native by the log's own rule (side 0: (18 - x, y); side 1:
    (x, 32 - y)), i.e. the tile centre the game snapped the tap to -- the centre a
    formation was laid around, which its members' centroid need not be (the game
    displaces a formation off a footprint or an edge, and the centroid then says where
    the units went, not where the player tapped; the engine is charged with that
    difference). A single unit is placed where it appeared (its centroid, which is the
    tap snapped to the game's grid), or where it was created when its first tick pushed
    it (CREATION POINT below), or on its tile's laid point when the capture missed its
    creation tick and a push moved it off that point (LATE SINGLE below). Without a tap
    the centroid is used, except where
    the group's members agree on the tile centre it was laid around (RECOVERED TILE
    below). `source` says which. A matched tap is kept on the row as `tap` (`tick`,
    `native`, `cycled`): the harness resolves a tapped troop's point on the board of
    `tap.tick`, where the client resolved it (replay_parity/harness.rs `resolve_tick`).
    A log names a card as the client showed it; `canon_name` matches that to cards.json
    by name or display name. A spell cast comes from the `effects` stream (`spell_casts`: the stream
    lists every projectile object on every frame it exists, so ONE cast is the run of
    class-28 objects of one (side, card) with no gap over CAST_GAP_TICKS between
    sightings -- a Fireball seen on 15 frames, a Log's airborne object then its rolling
    object, an Arrows volley of 9-30 objects on one tick are one cast each; its tick
    is the first frame and its aim point the mean of the first frame's projectile
    targets: a point spell's exact tap, a Log's landing = the roll's start, a volley's
    centre; `cast_objects` / `cast_frames` / `aim` record the evidence) or, for a
    spell the effects stream does not show (no projectile: Zap, Rage, Freeze, ...),
    from the tap plus this capture's median tap latency, flagged `timing: estimated`; a spell tap
    with no latency measurement is listed under `unresolved` instead of guessed.

PAIR DATING
    A battle recorded from both seats shows each deploy twice. In each capture a row could have
    appeared on any tick of its WINDOW, (the frame before its first sighting, its first
    sighting]; a cast dated by its elixir drop is sighted on the drop's frame, and a row the
    rules above dated outside its window (a group shown late, a tunnel's count, a Barbarian
    Barrel's flight) has the one tick they gave it. Where the other seat's capture holds the
    same deploy (exactly one row of the same card and side, dated within PAIR_MATCH_TICKS) and
    the two windows share exactly one tick, that tick is the row's, whatever the rules above
    chose: each of them chooses inside the one window its own capture shows. `tick_evidence`
    then reads "exact (pair-dated with <partner>: ...; this capture alone gave <tick>: <its
    evidence>)". A row whose windows share two ticks or none, or with no partner row, keeps its
    tick. Both seats' fixtures carry the game's sides and positions in one frame (THE SIDE
    CONVENTION), so the same deploy has the same card and side in both; its position is not
    compared (083112's Miner of t1203, first seen under ground by seat B and already surfaced by
    seat A, is published 1,277 apart on one axis).

    The partner is the other seat's capture of the same battle (`battle_partners`): another
    seat tag, a stamp within BATTLE_STAMP_SECONDS (164951 and 164953 are one battle), and
    every part of a capture split into parts (.b1, .b2). A capture without a seat tag has
    none. Wherever a capture is built -- alone, with --all or for --check -- the maker also
    builds its partners from their own captures and logs, as each is dated alone (no pair
    dating of its own), and takes only their rows' windows (`seat_offer`). So a fixture does
    not depend on which captures a run was handed, and --check of one capture is complete.

    On the 16.402 corpus (73 fixtures, 2026-09-28), 428 rows have a window of two or more
    ticks. 314 are pinned by a partner: 288 were already on the shared tick and 26 move by one
    tick (20 casts dated by their elixir drop, 2 Arrows volleys, 4 unit groups, among them
    005517-A's Ice Wizard 641 -> 640, which its deploy-end transition had dated). 82 rows share
    two ticks, 3 share none, 8 have no partner row and 21 no partner capture.

RECOVERED TILE
    A group of several members with no tap is played at its centroid, and the centroid is
    not where the game put the group when a member was clamped. The game snaps a troop tap
    to a tile centre, lays the formation around it, then clamps each member into its tile
    column's deploy range and into the arena's bounds, each along one axis. A clamped
    member pulls the centroid a few hundred off the tile, and the engine then lays the
    whole formation that far off. Side 1's Skeletons laid on the king's back row are the
    plainest case: their two back members are clamped to y 31000, and their centroid sits
    600 below the tile they were laid on.

    So such a group is played at the tile centre its members agree on, when they agree on
    exactly one (`source` `recovered_tile`). The members are read at their CREATION POINT:
    the capture's x2, y2 on the member's first frame, where it stood before that tick's
    movement (the frame's x, y already carry the first tick's contact push). They agree
    through NOMINAL OFFSETS: the member offsets from the tap that the committed formation
    measurement shows for the card (FORMATIONS, tools/make_formation_fixture.py), on its
    groups with a logged tap whose members stand centred on it, both lanes, and each side
    also taking the other side's offsets turned a half-turn. A candidate is a tile centre
    that some member's creation point minus a nominal offset lands on, to within
    RECOVER_TOLERANCE on both axes. Nothing in the offsets puts a candidate on a tile
    centre, so landing on one is the check. The candidate taken is the only one with the
    most members on a nominal offset from it, and either two or more members are, or one
    is and every other member matches a nominal offset on one axis (a clamped member).
    Otherwise the centroid stays. `recovery` says what decided, on every row that tried.
    The group's `centroid` is kept as it was.

    The recovery reads the client's measured offsets, not the engine's formation law, so
    a group it places is still evidence against that law. tools/replay_formations.py
    reports such a group at its centroid, so the formation measurement never reads a tile
    this recovery chose.

    A group the nominal offsets place nowhere (no measured group of the card, as for every
    Archer pair in the corpus) is read off its RING'S LAID POINT instead: a ring is
    symmetric about the point the game lays it on, so the exact mean of the members'
    creation points is that point, and a ground ring is laid one native unit lower in x
    on the arena's left half and one lower in y for side 1 than the tile centre it was
    tapped on (calibration formation.GROUND_DEPLOY_POINT; a flying ring on the tile
    centre itself). The group is played on the tile centre whose laid point that mean
    sits within RECOVER_TOLERANCE of, on both axes, when there is one (`laid_tile`; the
    source is recovered_tile). The first frame cannot say this: two Archers are created
    exactly touching, 1000 apart at radius 500 each, and the first tick's contact push
    moves the first one a unit away, so the first-frame mean is half a unit off and the
    floored centroid a whole unit off the laid point. 20260918-112751 t354: created on
    (8999, 500) and (7999, 500), the laid point (8499, 500) of the tile (8500, 500), first
    seen on (9000, 500) and (7999, 500), centroid (8499, 500), and the engine laid the
    pair one unit left of both.

CREATION POINT
    A single unit is played where it appeared, and its first frame's x, y already carry
    that tick's contact push: a Knight deployed onto a unit stands up to 150 off the point
    it was created on. So a single unit the capture shows deploying (behavior_state 4 or
    11) on its creation tick is played at its creation point, the first frame's x2, y2,
    when the two differ (`source` `creation_point`). Its creation tick is its first frame
    when `first_seen` = `tick` and either no frame was missed before it
    (`first_seen_gap` 1) or its deploy end pins the spawn there (`tick_evidence` exact).
    Otherwise the unit keeps its first frame (but see LATE SINGLE): its x2, y2 may be a tick
    after its creation, already pushed. A unit that travels under ground keeps its row (it
    is played at its `destination`).

AN UNLOGGED SIDE'S TROOP
    A side no placement log covers (the opponent of a live capture: only our own taps are
    logged) has no tap to play a troop on. A troop play of such a side that RECOVERED TILE,
    CREATION POINT and LATE SINGLE all leave on its members' centroid is played on the
    centre of the tile that centroid lies in (`source` `tile_centre`, x // 1000 * 1000 +
    500, the same in y): the game puts a troop down around a tile centre, and the
    centroid of members already pushed, or of a formation clamped at a river bank, is
    off it. Only a play the capture shows deploying (every member's first frame in
    behavior_state 4 or 11): a unit first seen walking is not at its creation point. And
    not a centroid on a tile line (x or y a whole thousand): that is a point the game
    relocated a tap to, on no tile's centre (a tower tap's troop on the king's back row
    at y 31000, a line of three there at c and c +- 549).
    Live (593 fixtures, the 10-06 table, measured before those two exclusions): 8,517
    such plays moved, troops within 250 64.07 % -> 66.72 % of both-alive unit-ticks, 175
    fixtures up and 47 down, the first divergence later in 59 and earlier in 5 (a
    Skeleton Army tapped on the river edge at (3500, 14500) lies 330 low on its centroid,
    and on its tile it is laid exactly). A logged side keeps what its units show: our
    own plays on their logged taps instead scored 4,675 fewer within 250 (the game
    relocates some taps, 1,164 of 8,996 by more than 250).

THE TOWER TROOP
    A side's princess towers may hold a tower troop other than the Princess (the Dagger
    Duchess, the Cannoneer, the Royal Chef: rows DaggerDuchess, Cannoneer, ChefTower), and
    the capture names none. Each has its own hitpoints (1270, 1200, 1240 against the
    PrincessTower's 1400), on the princess tower's ladder (`princess_tower_percent`, as
    the engine scales it: globals HITPOINT_INCREASE_PERCENT_PER_TOWER_LEVEL to
    combat.TOWER_HITPOINT_LADDER's cap level, _AFTER_TOURNAMENTCAP above it, compounded
    and floored). So `tower_troops` names, per side, the one row whose hitpoints at the
    side's tower level are its princess towers' max_hp, or null where none (or two)
    fit or the towers disagree. Live (602 fixtures): 1152 sides the Princess, 22 the
    Cannoneer, 18 the Dagger Duchess, 10 the Royal Chef, every side read.

A SCHEDULED SPELL'S CAST
    The 16.402 Graveyard puts its skeletons down on its area's schedule (its action
    graph's ActionSpawnToLocation entries: the first 2200 ms after the cast, then 500 or
    600 ms apart, each at a fixed offset from the cast point), and a capture shows no
    object for the cast itself, so the opponent's Graveyard has no cast row. Its
    skeletons are its spawn (truth only; read as no object, each was a Graveyard cast of
    its own: 126 of 168 rows in 8 live battles, 1.33M engine unit-ticks with no
    counterpart). The cast is read off them (`schedule_casts`, `source` `schedule`): a
    side's skeletons of the card that fall within the area's life of the first are one
    cast; the k-th to appear is the schedule's k-th entry, so the cast's tick is the
    earliest of each one's first tick less its entry's delay, its x the creation point of
    the ones whose entry has no x offset, and its y that of the ones with no y offset
    (each the median). A cast row of the card on that side already within
    SCHEDULE_CAST_SLACK ticks (one from a tap) is kept instead.

A SPELL'S LEVEL FROM ITS SPAWN
    A spell that puts units down (the Goblin Barrel's goblins, the Barbarian Log's
    Barbarian, the Graveyard's skeletons) records its level on them, so a cast whose
    spawn is seen takes its units' level (`level_source` `spawn`), over the side mode,
    the card level and a damage read alike: live 20261007-011702-A's Barbarian Log was
    played at the side mode 11, where its Barbarian stood at 12. The spawn is the
    earliest group of that side and card first seen within SPELL_SPAWN_WINDOW ticks after
    the cast, each claimed once.

EVO COPIES
    An Evo Skeleton's hit makes a copy of it (its evolution's `evo_duplication`: up to the
    group's GroupMaxSize living members, spawner.EVO_COPY_POINT for where). The capture shows
    each copy as a new unit of the card, under the evolution's id, so a group of them read
    as a deploy was played AGAIN: the engine makes its own copies too, and the harness
    paired neither. Live 20261002-152325-A: side 0's Evo Skeletons of t1362 (count 3) were
    followed by rows of 2, 1, 1, 1 on t1392 to t1419 (keys 55-59) and one on t2818 (key
    122); with those rows out, unmatched engine units fell 55 -> 1 and the troops within
    250 rose 3,871 -> 11,176 of 15,905 unit-ticks. A summon group under an evolution that
    duplicates is its living group's copies, not a play, when no tap answers it, it has
    fewer members than a play puts down (the evolution's `count`), and every member first
    stands within EVO_COPY_REACH of a living member, on the frame before, of an earlier
    group of the same side and id (a play's or a copy's) that has finished its deploy (a
    hitter; a play whose members first show on two frames is not its own copies). Its entities are spawned (truth
    only), listed in `spawned_groups` with `copy_of`, which says how near each stood.

LATE SINGLE
    A single troop whose creation tick comes before its first frame (`tick` <
    `first_seen`: a missed frame, or a unit shown late) was pushed on the ticks the capture
    did not show, so its first frame's x2, y2 is already off the point it was created on,
    and playing it there moves it twice (the engine pushes it again from there). Such a
    unit is played on the LAID POINT of the tile its first frame's x2, y2 lies in (`source`
    `laid_point`, and `recovery` says so): the tile centre one native unit lower in x on the
    arena's left half and in y for side 1 for a ground unit (formation.GROUND_DEPLOY_POINT,
    as `laid_tile` reads a ring), the tile centre for a flyer. Only when that point is off
    the first frame's centroid (a unit that stood still is left as it is), the capture shows
    it deploying (behavior_state 4 or 11) on its first frame, and x2, y2 lies within
    LATE_SINGLE_PUSH_PER_TICK per missed tick of the laid point on each axis. That box holds
    every push the contact law can add on the missed ticks (at most 150 on either axis a
    tick) and more: one tick's push is at most about 151 long ((150, 15), for one: the
    engine's own push lays 20260918-134739-B's Ice Spirit there from its laid point on
    t1935), while the box's corners lie 212 off a tick. A unit a tower tap put on the king's
    back row at y 31000 is on no tile's laid point, 499 in y from its tile's: the box leaves
    it on its centroid when it is seen 1 to 3 ticks late and takes it from 4 ticks late
    (4 x 150 = 600); none of the rows the maker lays over the 16.402 captures is such a unit.
    Measured on the 16.402 corpus: a Bomber of 20260918-122757.b1 deployed on t588 and first
    seen on t589 at (5660, 25656), its x2, y2 (5610, 25601), 111 and 102 off the laid point
    (5499, 25499) of its tile; played there, the engine puts it on the client's point on
    t589 and on every frame after, where from its first frame it stood 97 off. The same holds
    for the seven such units of the corpus's playable fixtures whose first frame is off their
    laid point (20260918-134739-B t1935, 20260918-164953-A t2288, 20260919-145440-A t202,
    20260919-181741-A t262, 20260919-183504-B t1253, 20260920-005517-A t2084 and the Bomber
    above): each first frame lands on the client's to the unit.

SPELL OBJECTS
    A cast from the effects stream also publishes every object of the run, in the fixture's
    frame (`objects`, one record per object, in order of first sighting):

      first      the tick of the object's first sighting
      launch     where the object stood on the tick BEFORE that sighting (the stream's
                 previous position on the first sighting): its launch point when the first
                 sighting was its first tick (compare `first` with the frame before it)
      depart     the first sighting on which it is off `launch`; null if never seen moving
      target     the point the object flies to, as the stream gives it on the first sighting
      last_seen  the last frame the object is on, and `end` its position there
      arrival    the next frame of the capture (the object is gone from it): the tick it
                 arrived, exactly when arrival = last_seen + 1; null when the capture ends first

    The positions turn with the arena like every other position (`arena_point`; no capture
    of the corpus is turned, so tests/test_replay_fixture.py TestSpellPointRotation and the
    both-ways-up battle test are that branch's only checks). `departures` groups the
    objects by `depart`: [[tick, objects], ...].
    What the 73 distinct captures show (2026-09-22): each of the 23 Arrows casts has all its
    objects first seen on one tick; 21 are 30 objects, of which 19 depart exactly 10 + 10 + 10
    on ticks t, t + 4, t + 8 (the later waves sitting at their launch points until then) and
    two split 10 + 10 + 10 with one gap of 3 or 5; the other two are 23 objects in broken-up
    departures. Within a wave the arrivals spread over a few ticks with the flight distance,
    so a wave is a departure tick, not an arrival tick. A Fireball, Rocket or Snowball is one
    object launched from the caster's king tower centre (27 of 32, 5 of 6, 17 of 18); each of
    the other 7 follows a frame gap and sits one or two flight steps from that centre, i.e.
    was first seen a tick or two into its flight. The Log and the Barbarian Barrel (BarbLog)
    are two objects (7 of 7, 8 of 8): the airborne one, then the rolling one, launched from
    the airborne one's target. No object's target changes during its flight (0 of 770).
    Lightning's objects are never seen off their launch point (4 of 4). arrival - last_seen is
    1 for 544 of the 768 objects with an arrival, 2 for 214, 3 for 10. The one Fireball whose
    damage can be told apart (20260920-072148-A, cast tick 1334) was last seen on 1367 and its
    two victims, a princess tower and a Tesla, lost hp on 1368: its arrival.

FRAMES
    A capture holds one frame per 50 ms tick; frames are missed now and then (tick
    deltas of 2-12) and, in the earliest captures, REPEATED while the game was frozen
    (the ledger names them). Frames are de-duplicated by tick (first wins;
    `frames_duplicate` counts the rest) and a group whose first frame follows a gap
    carries `first_seen_gap` > 1. That gap bounds how much earlier the spawn is only
    for a group the capture showed on time. A group it showed LATE (`shown_late_spawn`)
    can have spawned more ticks before its first frame than the gap: on the 16.402
    corpus 3, 8 and 1 ticks before, with gaps of 1, 2 and 1. For every group, its
    deploy row's `first_seen` minus `tick` says how many ticks before its first frame
    it spawned.

ATTACK TIMERS
    Four more truth columns, as the capture records them per entity per frame, in ms; they
    need no transform when the arena rotates:

      attack_progress_ms      the attack's own clock. It is already above zero on the frame
                              the entity enters behavior_state 2 (2,655 of 2,729 entries),
                              grows 50 a tick (186,023 of 190,963 one-tick steps while above
                              zero; the rest are 0, 35, 65, 70 or 100 -- the clock stopped,
                              slowed or sped up -- and one reset), and does NOT fall back when
                              a hit lands: inside state 2 it fell 765 times against 5,252
                              hits. So > 0 means the entity is inside an attack, windup AND
                              cooldown together, not the windup alone.
      attack_load_timer_ms    splits that cycle: it jumps up on the tick of each hit (to a
                              per-card value, 300 to 1,600; 500 is the commonest) and on
                              2,037 of the 2,729 entries into the attack, then counts down 50
                              a tick to 0 and waits there for the next hit. On 2,258 of the
                              5,252 hit ticks a new projectile from the attacker appears in the
                              effects stream on the same tick.
      event_timer_ms          a further per-entity countdown (50 a tick on 43,528 of 43,550
                              one-tick decreases), re-armed to 150 to 650 at intervals while
                              the entity walks or attacks
      attack_component_valid  1 when the entity has an attack, else 0 (published as 0/1 so
                              every column is an integer); 1 on all 1,713,889 entity rows of
                              the corpus

    Measured 2026-09-22 over the 73 distinct captures, reading only frame pairs one tick
    apart; a hit here is such a pair, both frames in state 2, on which attack_load_timer_ms
    went up. Plain run-length encoding was chosen over a (value, slope, run) encoding after
    measuring both: 24 KB against 5 KB on the 110 KB 20260920-003751-B, 62 KB against 25 KB
    on the 1.4 MB 20260920-010218-B, where path_cells alone is 960 KB; the plain runs keep
    ONE decoder for every column (harness.rs decode_rle).

ELIXIR
    `truth.elixir_raw`, when the capture carries the per-frame `elixir_raw` pair: per side
    ("0", "1"; the pair is indexed by the capture's sides, which the fixture keeps), that
    side's elixir on each frame of `ticks`, run-length encoded like an entity column, null
    on a frame the capture has no value for. 10,000 is one elixir: each of the 8 deploys of
    20260920-003751-B takes its cost times 10,000 off its own side, less the regeneration over
    the frame gap. A capture without the pair gives a fixture without the key. Measured on
    20260920-003751-B: 14 KB run-length, 21 KB as two plain lists; a (value, slope, run)
    encoding would be 203 bytes, and was not taken for the same one-decoder reason as the
    timers.

AN ABILITY PRESS
    A press of a champion's or hero's button is a row of kind "ability" (the harness's
    KIND_ABILITY: `card` the base card, `form` "hero" for a hero form, issued at tick - 1
    as a deploy), read off the capture for both sides (`ability_presses`): an elixir drop
    no deploy or cast row explains, of the ability's `mana_cost`, with a unit of that
    side's champion or hero alive; an ability with a cast hold must also show it (its unit
    enters behaviour state 10, CAST_STATE, from one frame before the drop to
    PRESS_ONSET_TICKS after it, or its own unit appears). Measured on the live replay set
    (2026-10-09, 603 fixtures): 902 state-10 onsets of champions and heroes, 896 on the
    frame their side's pool fell by exactly the ability's cost; the Little Prince's
    ChampionGuard 17 ticks after the drop on 579 of 604; our own press receipts at the
    drop's tick for 795 of 797.

PLAYABILITY
    Unplayable, with the reason in the fixture and the manifest: a deploy card whose id
    the id table (Supercell's global ids: class x 1_000_000 + the row index of the
    card's spells_*.csv in the 15.535 files -- 26 characters, 27 buildings, 28 spells,
    203 hero forms whose rows are the base cards' with a `_hero` suffix) does not
    resolve; a deploy card the ENGINE cannot load, when a card
    census is present (data/derived/replay/card_census.json, written by
    `cargo run --example replay_parity -- --census`; the engine's loader is the only
    authority on that list, so it is not re-derived here); a tapped deploy whose entity
    is an unknown object (above); a capture that starts mid-battle with non-tower
    entities already on the board; a capture with no frames; a battle of a game mode that
    buffs every unit (GAME MODE).

GAME MODE
    The capture's header names no game mode, so the fixture's `battle_rates` measures what a
    mode changes, on the frame pairs one tick apart. `unit_rate_percent`: the modal step of
    `attack_progress_ms` of entities attacking on both frames, over 50, in percent (null under
    20 steps); `attack_steps` the counts behind it. 100 on a ladder battle. 130 under the Rage
    modes' global buff RageModeRage (game_modes.csv GlobalBuff; HitSpeed, Speed and SpawnSpeed
    multipliers 130), which the engine does not apply: 20261006-172724, -173137 and -173334
    step 65 on 7,943 of 9,345, 2,381 of 2,493 and 8,927 of 9,695 one-tick steps and 50 on
    none (every ladder battle's modal step is 50), walk 1.29-1.32x the same unit's ladder
    step (a Hog Rider 155 a tick, 119 on ladder), and space a Witch's Skeletons 107-109
    ticks, not 140; deploys still take 19-20 ticks and the load and event timers still count
    down 50 a tick. Such a battle is unplayable. `global_buff` names the
    buff when the rate is a known one. `elixir_per_tick`: [first tick, elixir_raw a tick] per
    regeneration section, i.e. floor(5,000,000 / the battle timeline's ElixirFullBarMS):
    178, 357, 537 on Default (28000, 14000, 9300 ms), 240, 473, 714 on RageMode (20750,
    10550, 7000), 537 throughout on TripleElixir, 1,250 on 7xElixir. The engine plays no
    elixir (deploys are spawned), so the elixir sections are recorded, not refused.

    Needs data/derived/cards.json; data/derived/mechanic_register.json
    (tools/mechanic_register.py) is read for the family labels and only warned about
    when absent.

NAMES
    The fixture names the seats A, B, ... in sort order over the whole run
    (tools/capture_names.py) and drops the file prefix and suffix, so `capture` is
    "20260920-003751-B" and the `placements` list the same stamps; the fixture file is
    <capture>.replay.json. A folder carrying one capture under several names contributes
    it once, and a run refuses to write two fixtures to one path.
"""

from __future__ import annotations

import argparse
import csv
import glob
import gzip
import json
import math
import os
import re
import statistics
import sys
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capture_names import SEAT_TAG, folder_captures, folder_seats  # noqa: E402

LIVE = os.environ.get("ROYALELIVE_REPORTS")
RAW = os.path.join(ROOT, "data", "raw", "cr-15.535.29", "csv_logic")
CARDS = os.path.join(ROOT, "data", "derived", "cards.json")
REGISTER = os.path.join(ROOT, "data", "derived", "mechanic_register.json")
#: The committed measurement of the client's summon formations (tools/make_formation_fixture.py; tests/formations.rs
#: holds the engine to it). This tool reads it only for the nominal offsets of a recovered tile (RECOVERED TILE).
FORMATIONS = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "formations", "measured.json")
OUT_DEFAULT = os.path.join(ROOT, "data", "derived", "replay")
CENSUS = "card_census.json"
#: The `--table` CARDS, RAW and CENSUS are set for (`select_table`).
TABLE = "15.535.29"

#: THE CARD TABLES A FIXTURE CAN BE MADE AGAINST (calibration cards.CARD_TABLE), by `--table`: the derived card file,
#: the raw pack whose csv names every client card id (the 160402017 base pack: the 2026-10-06 update folder is a toml
#: overlay without the spells csv), and the census file `replay_parity --census` writes under that table's arm
#: (harness.rs `census_file`). One switch sets all three, so they cannot disagree.
TABLES = {
    "15.535.29": ("cards.json", "cr-15.535.29", "card_census.json"),
    "160402017-20261006": ("cards-160402017-20261006.json", "cr-160402017", "card_census-160402017-20261006.json"),
    # The same client before its 2026-10-06 update (content 16.402.2): a capture recorded before the update.
    "160402017": ("cards-160402017.json", "cr-160402017", "card_census-160402017.json"),
}


def select_table(name: str) -> None:
    """Point CARDS, RAW and CENSUS at table `name` (a TABLES key)."""
    global CARDS, RAW, CENSUS, TABLE
    cards, raw, census = TABLES[name]
    TABLE = name
    CARDS = os.path.join(ROOT, "data", "derived", cards)
    RAW = os.path.join(ROOT, "data", "raw", raw, "csv_logic")
    CENSUS = census

#: THE 2026-10-06 UPDATE'S CONTENT (a capture header's `client.content_version`, which the live reader writes from that
#: update on): 16.402.19, and the later stamps of the same build that changed no card (16.402.21: build 160402020 is
#: 160402017's tables).
UPDATE_20261006_CONTENT = (16, 402, 19)


def content_refusal(table: str, content: str | None) -> str | None:
    """Why a capture recorded on client content `content` (its header's stamp; None for a capture made before the
    stamp, i.e. before the 2026-10-06 update) does not fit card table `table`, or None when it does. The table must be
    the game the capture was played on: a stamp is per capture, since a device takes a content update when its game
    starts, so no date decides it. The 15.535.29 table takes any capture (the 16.402 corpus is measured on it)."""
    if table not in ("160402017", "160402017-20261006"):
        return None
    if content is None:
        if table == "160402017-20261006":
            return ("the capture has no content stamp, so it was recorded before the 2026-10-06 update: "
                    "make it with --table 160402017")
        return None
    try:
        v = tuple(int(x) for x in content.split("."))
    except ValueError:
        return f"the capture's content stamp {content!r} is not a version"
    if table == "160402017":
        return (f"the capture's content {content} is the 2026-10-06 update's or later: "
                "make it with --table 160402017-20261006")
    if v[:2] != UPDATE_20261006_CONTENT[:2] or v < UPDATE_20261006_CONTENT:
        return f"the capture's content {content} is not the 2026-10-06 update's (16.402.19 on)"
    return None


FORMAT = "replay-fixture-1"
#: The recording's five-digit tag at the end of a capture or placement-log file name
#: (`...-NNNNN.jsonl`): one tag per seat of a battle, the same on that seat's capture and log.
SEAT_FILE_TAG = re.compile(r"-(\d{5})(?=\.)")
#: One elixir in the capture's `elixir_raw` units (module doc, ELIXIR).
ELIXIR_UNIT = 10_000
#: The most elixir a side regenerates in one tick (triple elixir: one elixir per ~0.93 s), so a
#: drop measured across a frame gap is still recognised as a card's cost.
ELIXIR_REGEN_MAX_PER_TICK = 540
#: THE BATTLE'S RATES (module doc, GAME MODE). An attacking unit's `attack_progress_ms` grows this much on a one-tick
#: step at 100 % (one tick is 50 ms; module doc, ATTACK TIMERS).
ATTACK_STEP_AT_100 = 50
#: The fewest one-tick attack steps that measure a battle's unit rate; with fewer, `unit_rate_percent` is null. A
#: ladder battle's steps are 50 on 83 % or more of them (20261007-130902 the least), a Rage battle's 65 on 85 % or more.
UNIT_RATE_MIN_STEPS = 20
#: The global buff a game mode lays on every unit, by the unit rate it gives: game_modes.csv's GlobalBuff column names
#: RageModeRage on the Rage modes (Rage_Ladder, Rage_Friendly, the RampUpElixirRageJacks modes, ...), and its
#: character_buffs row is HitSpeedMultiplier, SpeedMultiplier and SpawnSpeedMultiplier 130.
GLOBAL_BUFF_BY_UNIT_RATE = {130: "RageModeRage"}
#: An elixir section is at least this many one-tick rises of one size, give or take ELIXIR_STEP_SLACK.
ELIXIR_SECTION_MIN_STEPS = 20
ELIXIR_STEP_SLACK = 2
#: A one-tick rise above this is a grant (an Elixir Collector's, an Elixir Golem's), not regeneration: the largest
#: regeneration step is the 7xElixir timeline's 1,250.
ELIXIR_REGEN_STEP_MAX = 2000
#: How far after its tap a cast's elixir drop is looked for, ticks: a stale log tick put one
#: 167 ticks before its cast (181741).
CAST_DROP_WINDOW = 400
#: The one convention the harness plays (replay_parity/harness.rs DEPLOY_TICK_CONVENTION).
DEPLOY_TICK_CONVENTION = "first_effect_frame"
CAPTURE_SUFFIX = ".native.oracle.jsonl.gz"
# Native arena size in millitiles (18 x 32 tiles); only used to rotate a capture whose
# side 0 sits at the top, which no capture of the corpus does.
NATIVE_W, NATIVE_H = 18_000, 32_000
#: The grid the game publishes `path_nodes` on: half-tile cells, 500 native on a side.
#: Derived rather than written down so a change to the arena cannot leave these stale.
CELL_NATIVE = 500
CELL_COLS, CELL_ROWS = NATIVE_W // CELL_NATIVE, NATIVE_H // CELL_NATIVE
# Entity kinds in the captures: 12 building deploying/inactive, 13 building up, 14 troop
# deploying, 15 troop active. Towers carry card_id -1.
KIND_TROOP_DEPLOYING = 14
# Spell cards are class 28 of Supercell's global ids (class x 1_000_000 + row); the id
# table is the row order of the 15.535 spells_*.csv files per class.
SPELL_CLASS = 28
EVO_CLASS = 13
HERO_CLASS = 203
ID_CLASSES = {  # base classes first: a reader that keeps the first id per name keeps the base card's
    26: "spells_characters.csv",
    27: "spells_buildings.csv",
    28: "spells_other.csv",
    EVO_CLASS: "spells_evolved.csv",
    HERO_CLASS: "spells_hero_form.csv",
}
# the suffix a form's row adds to its base card's name, per form class
FORM_SUFFIX = {EVO_CLASS: "_EV1", HERO_CLASS: "_hero"}
# A DEPLOY'S FORM. A play's units carry its form's own card id: an evolved play's are of class 13
# (the spells_evolved.csv row: Skeletons_EV1 13000010, Musketeer_EV1 13000014, Cannon_EV1 13000096,
# measured on client 15.535.29 in scripted scenes), a hero play's of class 203 (the
# spells_hero_form.csv row: 203000014 on every play of a hero-form slot in five battles on client
# 16.402, and 203000038 the hero Ice Golem on 15.535.29), and a base play's the plain card id. So a
# deploy is published with `form` "ev1", "hero" or "base" read off its units' class, and
# `form_row` the form's row. No evolved play is recorded on client 16.402 yet; the fixture's
# `forms_read` says how each form is read.
FORMS_READ = {
    "ev1": "the units' card id is of class 13 (measured on client 15.535.29)",
    "hero": "the units' card id is of class 203",
    "base": "the units carry the plain card id (class 26, 27 or 28)",
}
# Tap window: a group first seen in [tap + TAP_MIN, tap + TAP_MAX] belongs to that tap
# (measured latency 23-38 ticks over the corpus; the game also refuses the
# odd tap right after tick 150, which then matches nothing).
TAP_MIN, TAP_MAX = 5, 80
# An hp that matches no object of the card exactly takes the nearest object only within
# this relative error: the 16.402 balance deltas the corpus carries against the 15.535
# cards.json are 1.2 % (Ice Spirit 84 vs 85) and 6.6 % (Ice Golem 480 vs 514); the
# smallest hp that is a DIFFERENT object is 49 % off (the Goblin Drill's surfaced
# building 1313 against its dig troop 2560). Beyond it the entity is `unknown_object`.
NEAREST_MAX_ERROR_PERCENT = 10
#: A tile's side in native units: a troop tap snaps to the centre of one (calibration placement.TAP_SNAP).
TILE_NATIVE = 1000
#: The arena's width, native: a ground ring tapped left of its middle is laid a unit lower in x (`laid_tile`).
ARENA_W_NATIVE = 18000
#: A measured formation group gives nominal offsets only when its members' mean offset from its tap is within this on
#: both axes. That leaves out a group laid a tile away from its logged tap (a tap the game moved off a tower) and a
#: group with a clamped member: neither is a formation around its logged tile.
NOMINAL_CENTRED_NATIVE = 60
#: How far a creation point minus a nominal offset may sit from a tile centre, and a member from a nominal offset,
#: native per axis. The measured offsets are first-frame positions after the first tick's contact push, so two siblings
#: that overlap at creation stand a few units off their ring (the Minions' 577 ring, 2 apart).
RECOVER_TOLERANCE = 3
#: How far one tick's contact push can move a unit on one axis, native: move16402.rs `collision_mean` scales a push
#: longer than 150 to length 150 through a truncating isqrt (so up to about 151 long), and neither axis of it passes
#: 150. A single first seen late is played on its tile's laid point only when its first frame's creation point lies
#: within this many units per missed tick of it on each axis, a box that also admits points no push reaches (its
#: corners lie 212 off a tick; module doc, LATE SINGLE).
LATE_SINGLE_PUSH_PER_TICK = 150
# Spell casts: the effects stream lists every projectile OBJECT on every frame it
# exists (a Fireball 15 frames, a Log's airborne object then its rolling object, Arrows
# 9-30 objects on one tick). A cast is the run of objects of one (side, card) with no
# gap longer than this many ticks between sightings: the Log's airborne -> rolling
# handoff is 2 ticks, the captures' frame gaps run to 12, and a second cast of the same
# card by the same side needs the card cycled back (seconds).
CAST_GAP_TICKS = 20
#: How far from its tunnel's last recorded position a morphed spawn-pathfind deploy may surface. The tunnel ends
#: within one step of its goal cell (300 for the Goblin Drill), whose cell holds the destination, but a missed frame
#: can leave the last recorded position a step or two further back. The bound only keeps another spawn of that side
#: from being paired.
TUNNEL_SURFACE_MAX = 2000
#: How far an effect cast's first frame may sit from its caster's elixir drop, ticks: the drop
#: lands on the first projectile frame, and a frame gap can put the sighting a tick or two off.
EFFECT_DROP_SLACK = 3
#: Milliseconds per game tick (cards.json times / TICK_MS = ticks).
TICK_MS = 50
#: THE BATTLE'S OWN ELIXIR RATE (`battle_regen`): the one-tick rise below the cap that a stretch of this many ticks
#: shows most often, on at least this many frames.
REGEN_STRETCH_TICKS, REGEN_MIN_FRAMES = 600, 5
#: A UNIT'S RELEASE (`released_by_unit`): how far from a unit of its side that carries its card id a flying effect
#: object may leave, native. Measured on the 2026-10 live set: a Clone copy's shot leaves 400-1,200 from the copy's
#: centre, the Heal Spirit's jump and the Barbarian Barrel barbarian's roll from the unit's own point.
RELEASE_REACH = 1500
#: The Barbarian Barrel's airborne object (BarbLogProjectile): created MinDistance 3000 short of
#: the landing on the caster's forward axis, moving 360 a tick. Where a capture missed its first
#: frame it is first seen a step or more in, and the cast tick is that many steps earlier.
BARBLOG_AIRBORNE_START, BARBLOG_AIRBORNE_STEP = 3000, 360
#: Two seats' captures of one battle carry stamps at most this many seconds apart (placement_files_for's rule for a
#: battle's logs): 20260918-164951/164953 and 20260920-004657/004658 are one battle each (PAIR DATING).
BATTLE_STAMP_SECONDS = 10
#: How far apart two seats may date the same deploy for their rows to be matched, ticks (PAIR DATING). On the
#: 16.402 corpus no row with a window of two or more ticks matches two partner rows within this.
PAIR_MATCH_TICKS = 3
#: The per-entity truth columns, in the order `truth.columns` lists them and every row holds
#: them. The attack timers (module doc, ATTACK TIMERS) follow the seven the harness reads.
TRUTH_COLUMNS = (
    "x",
    "y",
    "hp",
    "target",
    "path_n",
    "state",
    "path_cells",
    "attack_progress_ms",
    "attack_load_timer_ms",
    "event_timer_ms",
    "attack_component_valid",
)


# ---------------------------------------------------------------------------
# id table


def missing_id_files() -> list[str]:
    """The ID_CLASSES files absent from RAW. The 15.535.29 pack is not committed, so a checkout
    or worktree without it has none of them."""
    return [os.path.join(RAW, f) for f in ID_CLASSES.values() if not os.path.exists(os.path.join(RAW, f))]


def load_id_table() -> dict[int, str]:
    """Supercell global id -> card name: class x 1_000_000 + the row index of the card in
    its 15.535 spells_*.csv (ID_CLASSES); a hero-form row names its base card.

    Refuses when a class file is absent. An absent file would read as a class with no ids, so
    every tap of that class would go unresolved and the output would silently lose them."""
    missing = missing_id_files()
    if missing:
        raise SystemExit(
            "the 15.535.29 pack is absent or incomplete, so card ids cannot be resolved: missing "
            + ", ".join(missing)
        )
    table: dict[int, str] = {}
    for cls, f in ID_CLASSES.items():
        path = os.path.join(RAW, f)
        with open(path, encoding="utf-8-sig") as fh:
            rows = [r[0].strip() for r in list(csv.reader(fh))[2:] if r and r[0].strip()]
        for ix, name in enumerate(rows):
            suffix = FORM_SUFFIX.get(cls)
            base = name[: -len(suffix)] if suffix and name.endswith(suffix) else name
            table.setdefault(cls * 1_000_000 + ix, base)
    return table


def load_form_rows() -> dict[int, str]:
    """The form row's name (spells_evolved.csv Musketeer_EV1, spells_hero_form.csv Musketeer_hero) by its
    class-13 or class-203 card id, counting rows as `load_id_table` does. Refuses when a file is absent."""
    rows: dict[int, str] = {}
    for cls in FORM_SUFFIX:
        path = os.path.join(RAW, ID_CLASSES[cls])
        if not os.path.exists(path):
            raise SystemExit(f"the 15.535.29 pack lacks {path}: deploy forms cannot be read")
        with open(path, encoding="utf-8-sig") as fh:
            names = [r[0].strip() for r in list(csv.reader(fh))[2:] if r and r[0].strip()]
        rows.update({cls * 1_000_000 + ix: name for ix, name in enumerate(names)})
    return rows


def deploy_form(card_id: int, form_rows: dict[int, str], base_name: str | None = None) -> dict:
    """The `form` and `form_row` of a deploy whose units carry `card_id` (A DEPLOY'S FORM): "ev1" for
    class 13, "hero" for class 203, "base" for a plain card id (its row is the card itself); {} for a
    form id with no row, which is not guessed."""
    cls = card_id // 1_000_000
    if cls in FORM_SUFFIX:
        if card_id not in form_rows:
            return {}
        return {"form": "ev1" if cls == EVO_CLASS else "hero", "form_row": form_rows[card_id]}
    return {"form": "base", "form_row": base_name} if base_name else {}


def base_ids(id_table: dict[int, str], card_names: set[str]) -> dict[str, int]:
    """Card name -> its base card id (class 26, 27 or 28). A form's id (class 13 evolved, 203 hero) names the
    same card and is never its base id: a tap log records the base id, and a play of any form answers it."""
    return {
        name: cid
        for cid, name in sorted(id_table.items(), reverse=True)
        if name in card_names and cid // 1_000_000 not in FORM_SUFFIX
    }


def display_names(cards: list[dict]) -> dict[str, str]:
    """cards.json's names and display names, squashed (no spaces) and lower-cased -> the card's name. A key two
    cards share is left out, so it can only fail to match, never match the wrong card."""
    seen: dict[str, set[str]] = {}
    for c in cards:
        for label in (c["name"], c.get("display_name")):
            if label:
                seen.setdefault(label.replace(" ", "").lower(), set()).add(c["name"])
    return {k: next(iter(v)) for k, v in seen.items() if len(v) == 1}


def canon_name(name: str, card_names: set[str], display: dict[str, str] | None = None) -> str:
    """A placements-log card name (display or internal) -> the cards.json name.

    The logs write the name the client showed. cards.json keys the internal name and keeps a display name
    beside it (`display`, from `display_names`). Taken in order: the name itself, the name without spaces, then
    a display or internal name that differs only in spaces and case, then the same with a plural "s" added.
    Without the last two, 48 records of the 96 placement logs of 2026-09-18/20 matched no card, so their taps
    carried no tick or point into a fixture: "Ice Spirit" (IceSpirits, display "Ice Spirits"; 28), "Ice Golem"
    (IceGolemite; 9), "Wall Breakers" (Wallbreakers; 2), "Executioner" (AxeMan; 2), "Furnace" (FirespiritHut;
    2), "ElixirCollector" ("Elixir Collector"; 2), "Guards" (SkeletonWarriors), "Night Witch" (DarkWitch) and
    "The Log" (Log). Still unmatched, because cards.json names them otherwise: "Giant Snowball" (19),
    "Barbarian Barrel" (2), "HealSpirit" (2), "Magic Archer" (1). A name that matches nothing is returned as
    it is."""
    if name in card_names:
        return name
    squashed = name.replace(" ", "")
    if squashed in card_names:
        return squashed
    for key in (squashed.lower(), squashed.lower() + "s"):
        if display and key in display:
            return display[key]
    return name


def public_name(raw: str, seats: dict[str, str]) -> str:
    """A capture or placements-log file name as the fixture records it (module doc, NAMES)."""
    name = SEAT_TAG.sub(lambda m: "-" + seats[m.group(1)], os.path.basename(raw))
    for suffix in (CAPTURE_SUFFIX, ".jsonl"):
        name = name.removesuffix(suffix)
    for prefix in ("frames-auto-", "frames-", "placements-"):
        name = name.removeprefix(prefix)
    return name


def capture_named(name: str, reports: str | None) -> str | None:
    """The capture in `reports` whose fixture name is `name` (`20260920-003751-B`; module
    doc, NAMES), or None when the folder has none. The seat letters are the whole folder's,
    as in every run. Two captures under one name is an error, not a choice."""
    if not reports or not os.path.isdir(reports):
        return None
    paths = folder_captures(reports, CAPTURE_SUFFIX)
    seats = folder_seats(reports, CAPTURE_SUFFIX, paths)
    hits = [p for p in paths if public_name(p, seats) == name]
    if len(hits) > 1:
        raise SystemExit(f"{reports} holds {len(hits)} captures named {name}: {hits}")
    return hits[0] if hits else None


# ---------------------------------------------------------------------------
# card objects and ladders (tools/make_live_levels_fixture.py's resolution)


def reachable_units(doc: dict, card: dict) -> dict[str, int]:
    """name -> base hitpoints of every object of `card` with hitpoints."""
    units = doc["units"]
    out: dict[str, int] = {}
    if card.get("hitpoints") is not None:
        out[card.get("summon_character") or card["name"]] = card["hitpoints"]

    def add(name, depth=0):
        u = units.get(name) if name else None
        if u is None:
            return
        if u.get("hitpoints") is not None:
            out.setdefault(name, u["hitpoints"])
        if depth < 2:
            add((u.get("death_spawn") or {}).get("character"), depth + 1)
            add((u.get("spawner") or {}).get("character"), depth + 1)
            add((u.get("interval_spawner") or {}).get("character"), depth + 1)

    spell = card.get("spell") or {}
    proj = card.get("projectile") or {}
    # a spell's area that puts units down: its own spawn, and its action graph's (the 16.402 Graveyard's skeletons are
    # its schedule's ActionSpawnToLocation entries; read as no object, each was a Graveyard cast of its own)
    area = spell.get("area_effect_object") or {}
    area_units = [area.get("spawn_character")]
    schedule = (area.get("schedule") or {}).get("entries") or []
    area_units += [e.get("spawn") for e in schedule if e.get("spawn_type") == "CharacterType"]
    for ref in (
        card.get("summon_character"),
        (card.get("second_summon") or {}).get("character"),
        (card.get("spawner") or {}).get("character"),
        (card.get("death_spawn") or {}).get("character"),
        (spell.get("spawn") or {}).get("character"),
        proj.get("spawn_character"),
        (proj.get("spawn_projectile") or {}).get("spawn_character"),
        # the building a tunnel leaves (the Goblin Drill's), with its own spawner and death spawn below it
        (card.get("spawn_pathfind") or {}).get("morph"),
        # the unit the card's ability button puts down (the Little Prince's ChampionGuard, 625 on the Common ladder):
        # a spawned unit, never a deploy summon. Unread, each guard was an unknown object (56% off the Prince's own
        # hp), never paired: 631 groups in the live set (2026-10-09)
        ((card.get("ability") or {}).get("effect") or {}).get("unit"),
        *area_units,
    ):
        add(ref)
    return out


def own_units(card: dict) -> set[str]:
    """The objects a DEPLOY of `card` puts on the board: its summon and second summon."""
    out = {card.get("summon_character") or card["name"]}
    second = (card.get("second_summon") or {}).get("character")
    if second:
        out.add(second)
    return out


def ladder_percent(doc: dict, card: dict, unit: str, level: int):
    ls = card["level_scaling"]
    if unit == (card.get("summon_character") or card["name"]) and card.get("hitpoints") is not None:
        base_level = ls.get("base_level", doc["rarities"][ls["rarity"]]["relative_level"] + 1)
        table = ls["multiplier_percent_by_level"]
    else:
        r = doc["rarities"][doc["units"][unit]["rarity"]]
        base_level = r["relative_level"] + 1
        table = r["multiplier_percent_by_level"]
    ix = level - base_level
    return table[ix] if 0 <= ix < len(table) else None


def classify_unit(
    doc: dict, card: dict | None, level: int, max_hp: int
) -> tuple[str | None, bool, str]:
    """(object name, is a deploy summon, how the hp matched: exact | nearest | no_card |
    no_object | unknown_object). `unknown_object`: no object of the card comes within
    NEAREST_MAX_ERROR_PERCENT of the hp (the nearest and its error are in the string):
    the game put something on the board under this card's id that cards.json does not
    derive from the card (an action-graph spawner's emission, a form the loader has no
    object for) -- truth only, never a deploy. `no_object` (no object of the card has
    hitpoints) is a deploy summon for a troop or a building, and truth only for a spell."""
    if card is None:
        return None, True, "no_card"
    # The card and its forms (an evolution's or a hero form's record, `form_of` the card): a deploy of a form is
    # its own record's objects (the Evo Bats' Bat_EV1, 48 base where the Bat is 32), each on its record's ladder.
    forms = [r for k in ("evolutions", "hero_forms") for r in doc.get(k) or [] if r.get("form_of") == card["name"]]
    rows = [card, *forms]
    own = set().union(*(own_units(r) for r in rows))
    best = None
    candidates = [(row, unit, base) for row in rows for unit, base in reachable_units(doc, row).items()]
    for row, unit, base in candidates:
        pct = ladder_percent(doc, row, unit, level)
        if pct is None:
            continue
        hp = base * pct // 100
        if hp == max_hp:
            return unit, unit in own, "exact"
        err = abs(hp - max_hp) * 100 // max(max_hp, 1)
        if best is None or err < best[0]:
            best = (err, unit, hp)
    if best is None:
        # a spell is cast, never summoned: an object of its id that no record derives is truth only
        return None, card.get("kind") != "spell", "no_object"
    err, unit, hp = best
    if err > NEAREST_MAX_ERROR_PERCENT:
        return None, False, f"unknown_object (nearest {unit} {hp} at level {level}, {err}% off)"
    return unit, unit in own, "nearest"


def fnv1a64(data: bytes) -> str:
    """FNV-1a 64 as 16 hex digits (the harness computes the same, `harness.rs fnv1a64`)."""
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


# ---------------------------------------------------------------------------
# capture reading


def read_capture(path: str):
    header = None
    frames = []
    with gzip.open(path, "rt", encoding="utf-8") as fh:
        for line in fh:
            d = json.loads(line)
            if d.get("record") == "header":
                header = d
            elif d.get("record") == "frame":
                frames.append(d["state"])
    return header, frames


#: `relabel_late_reads`: a walking unit's step into a frame this many times its step out of it votes that the frame was
#: read a tick late; units that step less than LATE_READ_MIN_STEP on either leg do not vote. A frame is relabelled when
#: at least LATE_READ_MIN_VOTES units, and LATE_READ_SHARE of the voters, vote so.
LATE_READ_RATIO = 1.6
LATE_READ_MIN_STEP = 20
LATE_READ_MIN_VOTES = 3
LATE_READ_SHARE = 0.6


def relabel_late_reads(frames: list[dict]) -> list[list[int]]:
    """A FRAME READ A TICK LATE is relabelled to the tick its contents show; returns [[label, new tick], ...].

    A capture can read a frame after the next tick's update has begun, so the frame carries tick t's label and tick
    t + 1's positions (and any unit that tick creates). It shows as a frame one tick after the one before it whose
    walking units have moved two ticks' worth, followed by a frame two ticks later whose units have moved one tick's
    worth; a correctly labelled frame there gives the reverse (about 1 : 2). 20260918-134739-B, frame 253 of frames
    252, 253, 255: its Skeletons and Goblins stepped 178-363 into it and 89-161 out of it, and its Ice Golem, first
    seen on it, is created on 254 by the battle's other seat and by its own deploy clock. Over the 16.402 corpus 12 of
    the 5,776 frames with that spacing are flagged. The frame keeps its contents and takes tick + 1, which the next
    frame (two ticks on) leaves free."""
    out: list[list[int]] = []
    for i in range(1, len(frames) - 1):
        t0, t1, t2 = frames[i - 1]["tick"], frames[i]["tick"], frames[i + 1]["tick"]
        if t1 - t0 != 1 or t2 - t1 != 2:
            continue
        pts = [{e.get("id"): (e["x"], e["y"]) for e in (frames[j].get("entities") or [])} for j in (i - 1, i, i + 1)]
        votes = late = 0
        for k in set(pts[0]) & set(pts[1]) & set(pts[2]):
            s_in, s_out = math.dist(pts[0][k], pts[1][k]), math.dist(pts[1][k], pts[2][k])
            if s_in < LATE_READ_MIN_STEP or s_out < LATE_READ_MIN_STEP:
                continue
            votes += 1
            late += s_in >= LATE_READ_RATIO * s_out
        if late >= LATE_READ_MIN_VOTES and late >= LATE_READ_SHARE * votes:
            frames[i]["tick"] = t1 + 1
            out.append([t1, t1 + 1])
    return out


def dedupe(frames):
    """First frame per tick wins; frames must come in non-decreasing tick order."""
    out, seen, dup, back = [], set(), 0, 0
    last = -1
    for f in frames:
        t = f["tick"]
        if t in seen:
            dup += 1
            continue
        if t < last:
            back += 1
            continue
        seen.add(t)
        out.append(f)
        last = t
    return out, dup, back


#: A key that starts at most this many ticks after another of the same side, card and
#: kind ended is the same unit re-keyed (merge_rekeyed), if the rest also agrees.
REKEY_MAX_TICKS = 2
#: ... and within this distance PER ELAPSED TICK, native: one unit step (a Skeleton moves 90
#: a tick) with room; a dropped frame between the two keys doubles it (010218-A.b1: 178 over
#: 2 ticks), still far short of where a spawner puts a new unit.
REKEY_STEP_PER_TICK = 100


#: A truth unit seen again after a gap farther than this (native per elapsed tick) from where it was last seen is
#: another unit under the same key: no walk, charge, dash, knockback or hook-drag step comes near it (the Fisherman's
#: drag, the fastest, takes 510 a tick; the reused key of 005517-A jumped 1,307 a tick). A gap is a tick the key is not
#: seen on (a frame without it, or a frame the capture missed).
REUSE_REACH_PER_TICK = 1000


def split_reused_keys(ents: dict, per_tick_rows: list, ticks: list[int]) -> list[list[int]]:
    """Split a truth key that names TWO units. The capture can hand a new unit the key of one that is gone:
    20260920-005517-A's key 22, a Tombstone's Skeleton last seen on 938 at (13690, 8449), is seen again on 953 at
    (12457, 28019), 19,609 away, a fresh Skeleton beside a Tombstone at the other end of the arena. One entity then
    reads as a unit that died in the engine and lives on in the truth. A key seen again after a gap farther than
    REUSE_REACH_PER_TICK per elapsed tick from its last point is split there: the later rows move to a new key (above
    every key in use), the new entity takes the old one's side, card, kind, level and max hp with no creation point,
    and every target naming the old key from that frame on names the new one. Returns [old key, new key, tick] per
    split; the fixture records them."""
    splits: list[list[int]] = []
    next_key = max(ents) + 1 if ents else 1
    todo = sorted(ents.values(), key=lambda e: e["key"])
    while todo:
        e = todo.pop(0)
        last = None
        for fi in range(e["first_index"], e["last_index"] + 1):
            r = per_tick_rows[fi].get(e["key"])
            if r is None:
                continue
            if last is not None and ticks[fi] - ticks[last] > 1:
                ra = per_tick_rows[last][e["key"]]
                if math.hypot(r[0] - ra[0], r[1] - ra[1]) > REUSE_REACH_PER_TICK * (ticks[fi] - ticks[last]):
                    nk = next_key
                    next_key += 1
                    frames = 0
                    for fj in range(fi, e["last_index"] + 1):
                        row = per_tick_rows[fj].pop(e["key"], None)
                        if row is not None:
                            per_tick_rows[fj][nk] = row
                            frames += 1
                    new = dict(e, key=nk, first_index=fi, frames=frames, x0=r[0], y0=r[1], c0=None,
                               states=[x for x in e.get("states", []) if x[0] >= fi],
                               positions=[x for x in e.get("positions", []) if x[0] >= fi])
                    e["last_index"] = last
                    e["frames"] -= frames
                    e["states"] = [x for x in e.get("states", []) if x[0] < fi]
                    e["positions"] = [x for x in e.get("positions", []) if x[0] < fi]
                    ents[nk] = new
                    for fj in range(fi, len(per_tick_rows)):
                        for k2, row in per_tick_rows[fj].items():
                            if row[3] == e["key"]:
                                per_tick_rows[fj][k2] = (*row[:3], nk, *row[4:])
                    splits.append([e["key"], nk, ticks[fi]])
                    todo.append(new)
                    break
            last = fi
    return splits


def merge_rekeyed(ents: dict, per_tick_rows: list, ticks: list[int]) -> list[list[int]]:
    """Fold a truth unit the capture gave a NEW KEY mid-life back into one entity.

    Key B is key A re-keyed when B starts 1..REKEY_MAX_TICKS ticks after A's last frame,
    on the same side with the same card, kind and max hp, within REKEY_STEP_PER_TICK per
    elapsed tick of A's
    last position, with A's last hp and A's last behaviour state -- the last is what a
    fresh emission beside a dying unit cannot match -- and A is the ONLY such key. B's
    rows move under A's key, every target that named B names A, and B is gone. Returns
    [kept key, merged key, tick] per merge; the fixture records them.
    """
    merges: list[list[int]] = []
    for b in sorted(list(ents.values()), key=lambda e: (e["first_index"], e["key"])):
        if b["key"] not in ents or b["first_index"] == 0:
            continue
        rb = per_tick_rows[b["first_index"]].get(b["key"])
        if rb is None:
            continue
        found = []
        for a in ents.values():
            if a is b or a["last_index"] >= b["first_index"]:
                continue
            same = ("side", "card_id", "kind_first", "max_hp")
            if any(a[f] != b[f] for f in same):
                continue
            gap = ticks[b["first_index"]] - ticks[a["last_index"]]
            if not 1 <= gap <= REKEY_MAX_TICKS:
                continue
            ra = per_tick_rows[a["last_index"]].get(a["key"])
            if ra is None or ra[2] != rb[2] or ra[5] != rb[5]:
                continue
            if math.hypot(ra[0] - rb[0], ra[1] - rb[1]) > REKEY_STEP_PER_TICK * gap:
                continue
            found.append(a)
        if len(found) != 1:
            continue
        a = found[0]
        for fi in range(b["first_index"], b["last_index"] + 1):
            row = per_tick_rows[fi].pop(b["key"], None)
            if row is not None:
                per_tick_rows[fi][a["key"]] = row
        a["last_index"] = b["last_index"]
        a["frames"] += b["frames"]
        a.setdefault("states", []).extend(b.get("states", []))
        a.setdefault("positions", []).extend(b.get("positions", []))
        del ents[b["key"]]
        merges.append([a["key"], b["key"], ticks[b["first_index"]]])
    if merges:
        remap = {bk: ak for ak, bk, _ in merges}
        for rows in per_tick_rows:
            for k, row in rows.items():
                if row[3] in remap:
                    rows[k] = (*row[:3], remap[row[3]], *row[4:])
    return merges


def spawn_tick_bounds(ticks: list[int], first_index: int) -> tuple[int, int]:
    """(previous frame tick + 1, first-seen tick): where the spawn can lie."""
    lo = ticks[first_index - 1] + 1 if first_index > 0 else ticks[first_index]
    return lo, ticks[first_index]


def first_step_spawn_tick(
    ticks: list[int], positions: list[tuple[int, int, int]], d: int
) -> int | None:
    """The spawn tick the FIRST STEP pins, or None: a single unit steps first on spawn
    + DeployTime / 50 (movement.DEPLOY_TIMING; 432 of 432 gap-free single-unit spawns
    of the corpus that stepped at all), so when the first frame off the spawn
    point (t_m) follows a seen frame still on it (t_m - 1), the spawn is t_m - d. When
    frames were missed between the last frame on the point and t_m, the steps taken
    by t_m are counted from the unit's own per-tick displacement on the NEXT seen
    frame (a fresh walk is straight and uniform): k steps within a quarter step of
    k x that displacement, 1 <= k <= the gap, put the first step at t_m - (k - 1). Not
    read for formation members: the game's separation pushes them off their point
    while they still deploy. `positions` = [(frame index, x, y), ...] for the member."""
    if d < 1 or not positions:
        return None
    _, x0, y0 = positions[0]
    for k in range(1, len(positions)):
        fi, x, y = positions[k]
        if (x, y) == (x0, y0):
            continue
        pfi = positions[k - 1][0]
        gap = ticks[fi] - ticks[pfi]
        if gap == 1:
            return ticks[fi] - d
        if k + 1 >= len(positions):
            return None
        nfi, nx, ny = positions[k + 1]
        dt = ticks[nfi] - ticks[fi]
        if not 1 <= dt <= 3:
            return None
        per_tick = math.hypot(nx - x, ny - y) / dt
        d0 = math.hypot(x - x0, y - y0)
        if per_tick <= 0:
            return None
        steps = round(d0 / per_tick)
        if 1 <= steps <= gap and abs(d0 - steps * per_tick) <= per_tick / 4:
            return ticks[fi] - (steps - 1) - d
        return None
    return None


def refine_spawn_tick(
    ticks: list[int],
    first_index: int,
    states: list[tuple[int, int]],
    deploy_ms: int | None,
    tick_ms: int = 50,
    positions: list[tuple[int, int, int]] | None = None,
) -> tuple[int, int, str]:
    """(spawn tick, first-seen tick, evidence) from the frame gap, the deploy-end
    transition and -- for a single unit, `positions` given -- the first step.
    `states` = [(frame index, behavior_state), ...] for one member."""
    lo, hi = spawn_tick_bounds(ticks, first_index)
    if lo == hi:
        return hi, hi, "exact"
    d = (deploy_ms or 0) // tick_ms
    if d >= 1 and states and states[0][1] == 4:
        tr = next((k for k, (_, st) in enumerate(states) if st != 4), None)
        if tr is not None and tr > 0:
            t0, t1 = ticks[states[tr - 1][0]], ticks[states[tr][0]]
            a, b = t0 + 1 - (d - 1), t1 - (d - 1)
            lo2, hi2 = max(lo, a), min(hi, b)
            if lo2 <= hi2:
                if lo2 == hi2:
                    return hi2, hi, "exact (deploy-end transition)"
                step = first_step_spawn_tick(ticks, positions or [], d)
                if step is not None and lo2 <= step <= hi2:
                    return step, hi, f"exact (first step, in the transition range [{lo2}, {hi2}])"
                return lo2, hi, f"range [{lo2}, {hi2}] (deploy-end transition), earliest used"
            return (
                hi,
                hi,
                f"first-seen tick: the deploy-end transition [{a}, {b}] contradicts the frame gap"
                f" [{lo}, {hi}]",
            )
    return lo, hi, f"range [{lo}, {hi}] (frame gap, no transition seen), earliest used"


#: The behavior_state of a summon member still waiting out its deploy stagger (entity kind 12); it
#: turns 4 when its own deploy starts.
STATE_STAGGER_WAIT = 11


def deploy_end(ticks: list[int], states: list[tuple[int, int]]) -> tuple[int, int] | None:
    """(t0, t1) of one member's deploy-end transition, or None when the capture does not show it:
    t0 the last frame of its first behavior_state-4 run, t1 the frame after it."""
    for k in range(1, len(states)):
        if states[k - 1][1] == 4 and states[k][1] != 4:
            return ticks[states[k - 1][0]], ticks[states[k][0]]
    return None


def shown_late_spawn(
    ticks: list[int],
    first_index: int,
    member_states: list[list[tuple[int, int]]],
    deploy_ms: int | None,
    stagger_ms: int | None = None,
    tick_ms: int = 50,
) -> tuple[int, str] | None:
    """(spawn tick, evidence) when the capture showed a deploy group LATE, else None.

    A capture can show a group some ticks after the tick its own deploy timers say it was
    spawned. The frame gap then bounds when the capture first SHOWED the group, not when it
    was spawned, and the deploy-end transition is the only clock left. Client 16.402, both
    seats of 20260920-010218: side 1's Goblins (tapped on 545) are first shown on 572 by
    seat B, with no frame missed, and on 577 by seat A (576 missed). Seat A shows their
    deploys end on 588, 592, 596 and 600: 19 ticks after a spawn on 569, plus the card's
    200 ms stagger per member. Seat B shows the same ends, the first across a missed frame
    (587, then 589).

    The group's first deploy end (a member first seen in state 4) gives the spawn range
    [t0 + 1 - (d - 1), t1 - (d - 1)], as in refine_spawn_tick. Only when that range lies
    wholly BEFORE the frame gap's range was the group shown late. The other members narrow
    a range left by a missed frame: in the order their deploys end, member r ends
    r x stagger ticks after the first. They are used only where every one of them agrees.
    """
    d = (deploy_ms or 0) // tick_ms
    if d < 1:
        return None
    lo, hi = spawn_tick_bounds(ticks, first_index)
    ends = []
    for states in member_states:
        if not states or states[0][1] not in (4, STATE_STAGGER_WAIT):
            continue
        end = deploy_end(ticks, states)
        if end is not None:
            ends.append((end[1], end[0], states[0][1]))
    if not ends:
        return None
    ends.sort()
    t1, t0, first_state = ends[0]
    if first_state != 4:
        return None
    a, b = t0 + 1 - (d - 1), t1 - (d - 1)
    if b >= lo:
        return None
    s = (stagger_ms or 0) // tick_ms
    na, nb = a, b
    for r, (u1, u0, _) in enumerate(ends):
        na, nb = max(na, u0 + 1 - (d - 1) - r * s), min(nb, u1 - (d - 1) - r * s)
    if na <= nb:
        a, b = na, nb
    shown = f"the capture shows the group from {hi}, {hi - b} tick(s) after it"
    if a == b:
        return b, f"exact (deploy-end transition; {shown})"
    return b, f"range [{a}, {b}] (deploy-end transition; {shown}), latest used"


def path_cell(index: int, rotate: bool) -> list:
    """One published `path_nodes` index as a `[col, row]` cell, rotated with the arena.

    Module level and not a closure because NO CAPTURE IN THE CORPUS ROTATES -- 0 of 75 on
    2026-09-22 -- so this branch cannot be validated against data and a unit test is the only
    instrument there is. The property the test pins is the one that makes the cells agree with
    the positions: a cell centre is at `500c + 250`, and `pos_of` maps it to
    `NATIVE_W - 500c - 250 = 500(CELL_COLS-1-c) + 250`, which is exactly the centre of the
    mirrored cell. The identity is therefore exact for centres, with no boundary case.
    """
    col, row = index % CELL_COLS, index // CELL_COLS
    if rotate:
        col, row = CELL_COLS - 1 - col, CELL_ROWS - 1 - row
    return [col, row]


def arena_point(x: int, y: int, rotate: bool) -> list[int]:
    """A native point in the fixture's frame: itself, or turned 180 degrees about the arena's
    centre when the capture is rotated (module doc, THE SIDE CONVENTION). Every published
    position goes through here, entity and spell alike, so the two cannot disagree."""
    return [NATIVE_W - x, NATIVE_H - y] if rotate else [x, y]


def attack_timers(e: dict) -> tuple:
    """The entity's four attack-timer columns (module doc, ATTACK TIMERS); a field the
    capture lacks is null. The validity flag is published as 0/1."""
    valid = e.get("attack_component_valid")
    return (
        e.get("attack_progress_ms"),
        e.get("attack_load_timer_ms"),
        e.get("event_timer_ms"),
        None if valid is None else int(bool(valid)),
    )


def elixir_columns(frames: list[dict]) -> dict | None:
    """`truth.elixir_raw` over these frames (module doc, ELIXIR), or None when no frame
    carries `elixir_raw`. The capture's pair is indexed by its own sides, and the fixture
    keeps the capture's sides even when it turns the arena (module doc, THE SIDE
    CONVENTION), so it needs no transform."""
    if not any("elixir_raw" in f for f in frames):
        return None
    out = {}
    for s in (0, 1):
        vals = []
        for f in frames:
            pair = f.get("elixir_raw")
            v = pair[s] if isinstance(pair, list) and len(pair) == 2 else None
            vals.append(v if isinstance(v, int) and not isinstance(v, bool) else None)
        out[str(s)] = rle(vals)
    return out


def battle_rates(frames: list[dict]) -> dict:
    """THE BATTLE'S RATES (module doc, GAME MODE), measured on the pairs of frames one tick apart: the units' rate
    (`attack_progress_ms` steps of entities in behavior_state 2 on both frames, buildings and troops) and the elixir
    regeneration per tick by section (one-tick rises below the cap, of either side, that are not grants)."""
    steps: Counter = Counter()
    runs: list[list[int]] = []  # [first tick, rise, count]
    for i in range(1, len(frames)):
        a, b = frames[i - 1], frames[i]
        if b["tick"] - a["tick"] != 1:
            continue
        before = {e["generation_key"]: e for e in a.get("entities") or []}
        for e in b.get("entities") or []:
            p = before.get(e["generation_key"])
            if p is None or p.get("behavior_state") != 2 or e.get("behavior_state") != 2:
                continue
            p0, p1 = p.get("attack_progress_ms"), e.get("attack_progress_ms")
            if isinstance(p0, int) and isinstance(p1, int) and 0 < p1 - p0 <= 4 * ATTACK_STEP_AT_100:
                steps[p1 - p0] += 1
        pa, pb = a.get("elixir_raw"), b.get("elixir_raw")
        if not (isinstance(pa, list) and isinstance(pb, list) and len(pa) == len(pb) == 2):
            continue
        for s in (0, 1):
            if not (isinstance(pa[s], int) and isinstance(pb[s], int)):
                continue
            d = pb[s] - pa[s]
            if not 0 < d <= ELIXIR_REGEN_STEP_MAX or pb[s] >= 10 * ELIXIR_UNIT:
                continue
            if runs and abs(runs[-1][1] - d) <= ELIXIR_STEP_SLACK:
                runs[-1][2] += 1
            else:
                runs.append([b["tick"], d, 1])
    sections: list[list[int]] = []
    for tick, d, n in runs:
        if n >= ELIXIR_SECTION_MIN_STEPS and not (sections and abs(sections[-1][1] - d) <= ELIXIR_STEP_SLACK):
            sections.append([tick, d])
    n = sum(steps.values())
    modal = steps.most_common(1)[0][0] if steps else None
    rate = round(100 * modal / ATTACK_STEP_AT_100) if modal is not None and n >= UNIT_RATE_MIN_STEPS else None
    return {
        "unit_rate_percent": rate,
        "attack_steps": {"modal": modal, "modal_count": steps[modal] if modal is not None else 0,
                         f"at_{ATTACK_STEP_AT_100}": steps[ATTACK_STEP_AT_100], "all": n},
        "global_buff": GLOBAL_BUFF_BY_UNIT_RATE.get(rate) if rate not in (None, 100) else None,
        "elixir_per_tick": sections,
    }


def snap_troop_tap(native: list) -> list:
    """A troop tap's native point as the game places it. A log's `requested` tile is almost
    always a tile centre (x.5 -> ...500); one on a tile BOUNDARY on x (9.0 -> 9000, the centre
    line) goes to the tile on its right, +x (a troop tapped at x = 9000 stands at 9500 in the
    recordings; a spell breaks the same tie the other way, which is why this is for troops).
    Measured on 240 troop taps of the corpus, one of which is on the boundary: 002736's Royal
    Hogs at (9000, 12500), which started ~500 left of where the game put them."""
    x, y = int(native[0]), int(native[1])
    if x % 1000 == 0:
        x += 500
    return [x, y]


def nominal_offsets(doc: dict) -> dict[tuple[str, int], list[tuple[int, int]]]:
    """(card, side) -> the member offsets from the tap that a formation measurement shows (FORMATIONS' format).

    Read from its `tap_tile` groups whose members stand centred on the tap (NOMINAL_CENTRED_NATIVE), both lanes pooled.
    Each side also takes the other side's offsets turned a half-turn: the ring is one ring in its owner's frame, less
    the one-unit ground deploy point (formation.GROUND_DEPLOY_POINT). In the committed measurement side 0's Skeletons
    stand at (-1, 807) and (+-699, -403) from their tap and side 1's at (-1, -808) and (+-699, 402)."""
    out: dict[tuple[str, int], set[tuple[int, int]]] = defaultdict(set)
    for g in doc.get("groups", []):
        offs = [tuple(m["offset"]) for m in g.get("members", [])]
        if g.get("source") != "tap_tile" or not offs:
            continue
        reach = NOMINAL_CENTRED_NATIVE * len(offs)
        if abs(sum(o[0] for o in offs)) > reach or abs(sum(o[1] for o in offs)) > reach:
            continue
        for ox, oy in offs:
            out[(g["card"], g["side"])].add((ox, oy))
            out[(g["card"], 1 - g["side"])].add((-ox, -oy))
    return {k: sorted(v) for k, v in out.items()}


def load_nominal_offsets(path: str = FORMATIONS) -> dict[tuple[str, int], list[tuple[int, int]]]:
    """nominal_offsets() of the committed formation measurement. Refuses when it is absent: without it every group with
    no tap would silently stay at its centroid."""
    if not os.path.exists(path):
        raise SystemExit(
            f"{path} is absent, so no deploy group can be placed on its recovered tile"
            " (python tools/make_formation_fixture.py writes it)"
        )
    with open(path, encoding="utf-8") as fh:
        return nominal_offsets(json.load(fh))


def recovered_tile(
    points: list[tuple[int, int]], offsets: list[tuple[int, int]]
) -> tuple[list[int] | None, str]:
    """The tile centre a deploy group's members agree it was laid around, and why; or None and why not (module doc,
    RECOVERED TILE).

    `points` are the members' creation points, `offsets` the card's nominal offsets for the side. A candidate is a tile
    centre that a point minus an offset lands on, within RECOVER_TOLERANCE on both axes. Per candidate, a member is ON
    it when its offset from the centre matches a nominal offset on both axes, and CLAMPED when on one axis only (the
    game's clamps each move a member along one axis). The candidate with the most members on it is taken when no other
    candidate has as many, and either two or more members are on it, or one is and every other member is clamped."""
    n = len(points)
    if not offsets:
        return None, "no nominal offsets for this card and side"
    tol = RECOVER_TOLERANCE

    def centre(v: int) -> int | None:
        c = (v // TILE_NATIVE) * TILE_NATIVE + TILE_NATIVE // 2
        return c if abs(v - c) <= tol else None

    candidates = set()
    for x, y in points:
        for ox, oy in offsets:
            cx, cy = centre(x - ox), centre(y - oy)
            if cx is not None and cy is not None:
                candidates.add((cx, cy))
    if not candidates:
        return None, "no member's creation point is a nominal offset from a tile centre"
    scored = []
    for cx, cy in candidates:
        on = clamped = 0
        for x, y in points:
            dx, dy = x - cx, y - cy
            if any(abs(dx - ox) <= tol and abs(dy - oy) <= tol for ox, oy in offsets):
                on += 1
            elif any(abs(dx - ox) <= tol or abs(dy - oy) <= tol for ox, oy in offsets):
                clamped += 1
        scored.append((on, clamped, (cx, cy)))
    scored.sort(key=lambda s: (-s[0], -s[1], s[2]))
    on, clamped, (cx, cy) = scored[0]
    tied = [s[2] for s in scored if s[0] == on]
    if len(tied) > 1:
        return None, f"{len(tied)} tile centres tie with {on} of {n} members on a nominal offset: {sorted(tied)}"
    if on >= 2 or (on == 1 and clamped == n - 1):
        return [cx, cy], f"{on} of {n} members on a nominal offset from it, {clamped} clamped on one axis"
    return None, f"the best tile centre ({cx}, {cy}) has {on} of {n} members on a nominal offset, {clamped} on one axis"


def laid_tile(points: list[tuple[int, int]], side: int, flying: bool) -> tuple[list[int] | None, str]:
    """The tile centre a group's ring was laid around, read from its members' creation points, and why; or None and
    why not (module doc, RECOVERED TILE, its last paragraph). The exact mean of `points` is the ring's laid point; a
    ground ring's laid point is its tile centre one native unit lower in x on the arena's left half and in y for side 1
    (formation.GROUND_DEPLOY_POINT), a flying ring's the tile centre. The tile is taken when the mean sits within
    RECOVER_TOLERANCE of its laid point on both axes."""
    n = len(points)
    mx, my = sum(p[0] for p in points) / n, sum(p[1] for p in points) / n
    half = ARENA_W_NATIVE // 2
    for tx in {int(mx // TILE_NATIVE) * TILE_NATIVE + TILE_NATIVE // 2 + d for d in (-TILE_NATIVE, 0, TILE_NATIVE)}:
        for ty in {int(my // TILE_NATIVE) * TILE_NATIVE + TILE_NATIVE // 2 + d for d in (-TILE_NATIVE, 0, TILE_NATIVE)}:
            lx = tx - (1 if tx < half and not flying else 0)
            ly = ty - (1 if side == 1 and not flying else 0)
            if abs(mx - lx) <= RECOVER_TOLERANCE and abs(my - ly) <= RECOVER_TOLERANCE:
                return [tx, ty], f"the members' mean ({mx:g}, {my:g}) is the laid point ({lx}, {ly}) of this tile"
    return None, f"the members' mean ({mx:g}, {my:g}) is no tile centre's laid point"


def late_single_point(c0: tuple[int, int], side: int, flying: bool, late_ticks: int) -> tuple[list[int] | None, str]:
    """The laid point a single unit first seen `late_ticks` after its creation tick was created on, read from its first
    frame's creation point `c0`, and why; or None and why not (module doc, LATE SINGLE). The point is the laid point of
    the tile `c0` lies in: the tile centre one native unit lower in x on the arena's left half and in y for side 1 for a
    ground unit (formation.GROUND_DEPLOY_POINT), the tile centre for a flyer. It is taken when `c0` lies within
    LATE_SINGLE_PUSH_PER_TICK x `late_ticks` of it on both axes."""
    tx = int(c0[0]) // TILE_NATIVE * TILE_NATIVE + TILE_NATIVE // 2
    ty = int(c0[1]) // TILE_NATIVE * TILE_NATIVE + TILE_NATIVE // 2
    lx = tx - (1 if tx < ARENA_W_NATIVE // 2 and not flying else 0)
    ly = ty - (1 if side == 1 and not flying else 0)
    reach = LATE_SINGLE_PUSH_PER_TICK * max(1, late_ticks)
    dx, dy = int(c0[0]) - lx, int(c0[1]) - ly
    where = f"({c0[0]}, {c0[1]}) is ({dx}, {dy}) off"
    if abs(dx) <= reach and abs(dy) <= reach:
        why = f"created {late_ticks} tick(s) before its first frame; its creation point there {where} the laid point"
        return [lx, ly], f"{why} ({lx}, {ly}) of its tile"
    return None, f"its first frame's creation point {where} its tile's laid point ({lx}, {ly}), beyond {reach}"


def first_cast_drop(
    frame_ticks: list, elixir: list, tap_tick: int, cost: int, skip: set, regen: int = ELIXIR_REGEN_MAX_PER_TICK
) -> int | None:
    """The first frame tick at or after `tap_tick`, inside CAST_DROP_WINDOW ticks, on which one
    side's `elixir` (per frame, aligned with `frame_ticks`; None where the capture has none) falls
    by `cost` elixir -- within what the frame gap could regenerate at `regen` a tick (`battle_regen`) -- on a tick
    not in `skip` (frames a matched deploy already explains, drops an earlier cast claimed); None if none."""
    for i in range(1, len(frame_ticks)):
        tk = frame_ticks[i]
        if tk < tap_tick:
            continue
        if tk > tap_tick + CAST_DROP_WINDOW:
            break
        a, b = elixir[i - 1], elixir[i]
        if a is None or b is None or tk in skip:
            continue
        gap = tk - frame_ticks[i - 1]
        if abs((a - b) - cost * ELIXIR_UNIT) <= regen * gap + ELIXIR_UNIT // 10:
            return tk
    return None


def battle_regen(frame_ticks: list, elixir_by_side: dict) -> int:
    """THE BATTLE'S OWN ELIXIR RATE, raw per tick: the largest of the one-tick rises below the cap that each
    REGEN_STRETCH_TICKS stretch shows most often (on at least REGEN_MIN_FRAMES frames), never below
    ELIXIR_REGEN_MAX_PER_TICK. A special mode regenerates faster than triple elixir: 20261002-172509-A rises 1,250
    a tick from its first frame (an elixir every 8 ticks), so a Log cast across a two-tick frame gap falls by 17,500,
    outside the triple-elixir tolerance (2 x 540 + 1,000), and 8 of the 2026-10 live battles lost their casts so."""
    by: dict[int, Counter] = defaultdict(Counter)
    for col in elixir_by_side.values():
        for i in range(1, len(frame_ticks)):
            a, b = col[i - 1], col[i]
            if frame_ticks[i] - frame_ticks[i - 1] != 1 or a is None or b is None:
                continue
            if a < b < 10 * ELIXIR_UNIT:
                by[frame_ticks[i] // REGEN_STRETCH_TICKS][b - a] += 1
    modal = [c.most_common(1)[0] for c in by.values()]
    return max([ELIXIR_REGEN_MAX_PER_TICK] + [r for r, n in modal if n >= REGEN_MIN_FRAMES])


def effect_leads(card: dict | None) -> list[int]:
    """AN EFFECT THAT SHOWS ITS CAST LATE: the ticks from a cast to the first object the effects stream can show of
    it. [0] for a spell whose objects are its own. A spell whose area effect spawns the projectile the stream lists
    (cards.json `spell.area_effect_object.projectile`: Lightning, Royal Delivery) shows it on the effect's first hit,
    `hit_speed_offset_ms` after the cast, or one `hit_speed_ms` later for each hit that found no target, while the
    effect lives. Measured on the 2026-10 live set: of 65 Lightning casts with no tap the caster's elixir drops 10
    ticks before the first strike on 62 and 20 on 3 (each with no strike on the first hit); of 85 Royal Deliveries,
    40 ticks before the box on all 85."""
    ae = ((card or {}).get("spell") or {}).get("area_effect_object") or {}
    if not ae.get("projectile") or ae.get("hit_speed_offset_ms") is None:
        return [0]
    step, life = ae.get("hit_speed_ms") or 0, ae.get("life_duration_ms") or 0
    leads = [ae["hit_speed_offset_ms"]]
    while step > 0 and leads[-1] + step <= life:
        leads.append(leads[-1] + step)
    return [ms // TICK_MS for ms in leads]


def released_by_unit(frames: list[dict], fi: int, side: int, cid: int) -> str | None:
    """How an effect object first seen on frames[fi] is A UNIT'S RELEASE, not a cast; None when it is not. A unit
    carries the card id of the play that made it, and what it launches carries that id too: the Heal Spirit (card Heal)
    jumps as a Heal object, a Clone copy shoots Clone objects, the Barbarian Barrel's barbarian rolls a BarbLog. So an
    object that flies (its launch point is not its target) from within RELEASE_REACH of a unit of its side carrying its
    card id, on the frame before or its own, is that unit's. A cast's object leaves the caster's king, rolls from its
    landing, or stands where it strikes (a Lightning bolt, a Royal Delivery's box). 2026-10 live set: 59 such objects
    (Heal 30, Clone 26, BarbLog 3), each with no drop of its card's cost; none of the 189 other dropped objects is
    one."""
    if fi == 0:
        return None
    units = [
        e for j in (fi - 1, fi) for e in frames[j].get("entities") or []
        if e.get("side") == side and e.get("card_id") == cid
    ]
    for o in frames[fi].get("effects") or []:
        if o.get("card_id") != cid or o.get("side") != side:
            continue
        launch = (o.get("x2", o["x"]), o.get("y2", o["y"]))
        if launch == projectile_target(o):
            continue
        for u in units:
            off = math.dist((u["x"], u["y"]), launch)
            if off <= RELEASE_REACH:
                return f"launched {off:.0f} from its side's unit at ({u['x']}, {u['y']}) that carries its card id"
    return None


def rle(values) -> list:
    """[v0, run0, v1, run1, ...]."""
    out: list = []
    for v in values:
        if out and out[-2] == v:
            out[-1] += 1
        else:
            out.extend([v, 1])
    return out


# ---------------------------------------------------------------------------
# placements


def placement_files_for(capture: str, reports_dir: str) -> list[str]:
    """Both sides' placement logs of the battle this capture recorded: same stamp, either
    seat; a twin recorded a couple of seconds later has a stamp within 10 s."""
    base = os.path.basename(capture)
    if not base.startswith("frames-"):
        return []
    stamp = SEAT_TAG.sub("", base)[len("frames-") :].split(".")[0]
    stamp = stamp[len("auto-") :] if stamp.startswith("auto-") else stamp
    if len(stamp) < 15:
        return []
    day, hms = stamp[:8], stamp[9:15]
    want = int(hms[:2]) * 3600 + int(hms[2:4]) * 60 + int(hms[4:6])
    out = []
    for f in sorted(glob.glob(os.path.join(reports_dir, f"placements-{day}-*.jsonl"))):
        s = os.path.basename(f)[len("placements-") + 9 :][:6]
        if not s.isdigit():
            continue
        have = int(s[:2]) * 3600 + int(s[2:4]) * 60 + int(s[4:6])
        if abs(have - want) <= 10:
            out.append(f)
    return out


def read_placements(
    paths: list[str],
    card_names: set[str],
    name_to_id: dict[str, int],
    spell_names: set[str] | None = None,
    default_sides: dict[str, int] | None = None,
    display: dict[str, str] | None = None,
):
    """-> (taps, decks): taps = [{side, card, id, tick, native, kind, cycled}],
    decks = {side: [ids]}.

    A tap is a CAST when its card is a spell (`spell_names`), whatever the record says: the
    log's `actual` field reads "cast" on three records in the whole corpus, and a scripted
    cycle play (`{"cycled": "Rage", ...}`) carries none, so classing by `actual` alone made
    every cycled spell a troop deploy that matched no unit group and was dropped. Without
    `spell_names` the record's own `actual` decides, as before (make_spell_impact_fixture).

    A log with no `local_side_native` record takes `default_sides[its file-name tag]` (the
    capture's side for its own tag, the other side for the other seat's): half the logs of
    2026-09-18/19 carry none, and without it every cycled play in them had no side and was
    skipped."""
    taps, decks = [], {}
    for p in paths:
        tag = SEAT_FILE_TAG.search(os.path.basename(p))
        side = (default_sides or {}).get(tag.group(1)) if tag else None
        with open(p, encoding="utf-8") as fh:
            recs = [json.loads(line) for line in fh if line.strip()]
        for r in recs:
            if "local_side_native" in r:
                side = r["local_side_native"]
        for r in recs:
            if "deck" in r and side is not None:
                decks[side] = [int(x) for x in r["deck"]]
        for r in recs:
            card = r.get("card") or r.get("cycled")
            if not card:
                continue
            s = r.get("side", side)
            if s is None:
                continue
            tile = r.get("requested") or r.get("tile")
            native = None
            if isinstance(tile, list) and len(tile) == 2:
                sx, sy = float(tile[0]), float(tile[1])
                nx, ny = (18.0 - sx, sy) if s == 0 else (sx, 32.0 - sy)
                native = [round(nx * 1000), round(ny * 1000)]
            name = canon_name(card, card_names, display)
            taps.append(
                {
                    "side": s,
                    "card": name,
                    "id": name_to_id.get(name),
                    "tick": int(r["tick"]),
                    "native": native,
                    "kind": "cast"
                    if r.get("actual") == "cast" or (spell_names is not None and name in spell_names)
                    else "deploy",
                    "cycled": "cycled" in r,
                }
            )
    taps.sort(key=lambda t: (t["tick"], t["side"], t["card"]))
    return taps, decks


# ---------------------------------------------------------------------------
# spell casts


def projectile_target(eff: dict) -> tuple[int, int]:
    """The point an effects-stream object flies to (`projectile_x/y`, else where it is)."""
    return eff.get("projectile_x", eff["x"]), eff.get("projectile_y", eff["y"])


def launch_tick(frame: dict, side: int, cid: int) -> tuple[int, str] | None:
    """(ticks already flown, evidence) for a cast whose first sighting is `frame`, when its object left from its
    caster's king tower (a Fireball, a Rocket, a Snowball: every such cast the corpus sees on its launch tick stands one
    step from the king, its previous point on the king's centre) and is already past its first step: its point is a
    whole number k of steps (point minus previous point) from the king, on the line from it, so it was launched on the
    first frame's tick minus k plus 1. None for every other object (a volley, a roll, a strike, one seen on its launch
    tick). 20260920-071744-B: a Fireball first seen on 475, after the frames of 473 and 474 were lost, on (9678, 27335)
    with its previous point (9452, 27890): 1798 from the king (9000, 29000), three steps of 599, launched on 473, the
    tick the other seat's capture first shows it."""
    effs = [e for e in frame.get("effects") or [] if e.get("card_id") == cid and e.get("side") == side]
    kings = [(e["x"], e["y"]) for e in frame.get("entities") or [] if e.get("kind") == 12 and e.get("side") == side]
    if len(effs) != 1 or len(kings) != 1 or effs[0].get("x2") is None:
        return None
    e, (kx, ky) = effs[0], kings[0]
    sx, sy = e["x"] - e["x2"], e["y"] - e["y2"]
    step = math.hypot(sx, sy)
    if step == 0 or (e["x2"], e["y2"]) == (kx, ky):
        return None
    k = math.hypot(e["x"] - kx, e["y"] - ky) / step
    # on the line from the king: the previous point is k - 1 steps out along the same heading
    ok = round(k) >= 2 and abs(k - round(k)) < LAUNCH_STEP_TOLERANCE and math.hypot(
        kx + sx * (round(k) - 1) - e["x2"], ky + sy * (round(k) - 1) - e["y2"]) <= LAUNCH_POINT_TOLERANCE * round(k)
    if not ok:
        return None
    return round(k), (f"{round(k)} steps of {step:.0f} from its caster's king ({kx}, {ky}) on its first frame, so"
                      f" launched {round(k) - 1} ticks earlier")


def spell_casts(frames: list[dict], rotate: bool = False) -> list[dict]:
    """The casts in the `effects` stream: one per run of class-28 objects of one (side,
    card) with no gap over CAST_GAP_TICKS between sightings (CAST_GAP_TICKS). Each:
    side, card_id, first_index, last_index, frames (sightings), objects (distinct
    object ids over the run), aim = the mean of the projectile targets
    (`projectile_x/y`, else x/y) of the objects on the FIRST frame -- one object's own
    target for a point spell (Fireball: the tap exactly), the airborne object's
    landing for a rolling one (the Log: where the roll starts, which the game may have
    clamped to the caster's territory), the pattern's centre for a volley (Arrows) --
    and aim_rule saying which; `tracks`, one record per object, and `departures`
    (module doc, SPELL OBJECTS).

    `rotate` puts every point in the fixture's frame (`arena_point`); sides are kept (module
    doc, THE SIDE CONVENTION). The aim is the mean taken IN that frame, so one battle gives
    one fixture whichever way up the capture recorded it (a mean taken before the turn
    would round the other way)."""
    open_casts: dict[tuple, dict] = {}
    out: list[dict] = []
    for fi, f in enumerate(frames):
        tick = f["tick"]
        by_key: dict[tuple, list[dict]] = defaultdict(list)
        for eff in f.get("effects") or []:
            cid = eff.get("card_id", -1)
            if cid < 0 or cid // 1_000_000 != SPELL_CLASS:
                continue
            by_key[(eff["side"], cid)].append(eff)
        for key, effs in by_key.items():
            c = open_casts.get(key)
            if c is not None and tick - c["last_tick"] > CAST_GAP_TICKS:
                out.append(c)
                c = None
            if c is None:
                pts = [arena_point(*projectile_target(e), rotate) for e in effs]
                c = open_casts[key] = {
                    "side": key[0],
                    "card_id": key[1],
                    "first_index": fi,
                    "last_index": fi,
                    "last_tick": tick,
                    "frames": 0,
                    "ids": {},
                    "aim": [
                        sum(x for x, _ in pts) // len(pts),
                        sum(y for _, y in pts) // len(pts),
                    ],
                    "aim_rule": "the object's projectile target"
                    if len(pts) == 1
                    else f"mean of {len(pts)} objects' projectile targets on the first frame",
                }
            c["last_index"] = fi
            c["last_tick"] = tick
            c["frames"] += 1
            for e in effs:
                track = c["ids"].get(str(e.get("id")))
                if track is None:
                    track = c["ids"][str(e.get("id"))] = {
                        "first_index": fi,
                        "launch": (e.get("x2", e["x"]), e.get("y2", e["y"])),
                        "target": projectile_target(e),
                        "depart_index": None,
                    }
                if track["depart_index"] is None and (e["x"], e["y"]) != track["launch"]:
                    track["depart_index"] = fi
                track["last_index"] = fi
                track["end"] = (e["x"], e["y"])
    out.extend(open_casts.values())
    for c in out:
        tracks = [
            object_record(t, frames, rotate)
            for t in sorted(c.pop("ids").values(), key=lambda t: t["first_index"])
        ]
        c["objects"] = len(tracks)
        c["tracks"] = tracks
        c["departures"] = departures(tracks)
        c.pop("last_tick")
    out.sort(key=lambda c: (c["first_index"], c["side"], c["card_id"]))
    return out


#: `launch_tick`: how far from a whole number of steps, and how far per step off the king's line, a first sighting may
#: stand and still be dated by its steps (the corpus's gap-seen king-launched casts stand on whole steps to 0.01).
LAUNCH_STEP_TOLERANCE = 0.05
LAUNCH_POINT_TOLERANCE = 3
#: How far past a damaging spell's radius a victim's centre may stand and still be read as hit (a unit's own radius).
SPELL_VICTIM_MARGIN = 1500
#: How many frames after a spell row's tick its first hit is looked for.
SPELL_HIT_WINDOW = 120
#: How much more than its damage a victim may lose on the hit frame (a building's decay tick, a concurrent chip).
SPELL_HIT_SLACK = 2
#: A PULSING SPELL'S LEVEL (spell_levels_from_damage): how many ticks off its buff's HitFrequency two pulses on one
#: victim may fall and still be one train (live Poison, a victim's consecutive pulses: 3,137 20 ticks apart, 37 at 19
#: and 33 at 21, a frame read a tick late or early).
SPELL_PULSE_TOLERANCE = 1
#: The capture's building kinds (module doc: 12 deploying or inactive, 13 up); a pulse takes the buff's
#: BuildingDamagePercent on one (state.rs, the Earthquake's 350).
BUILDING_KINDS = (12, 13)


#: How far from a living member of its group an Evo Skeletons copy may first stand (module doc, EVO COPIES): the
#: client makes it a tile from its hitter (spawner.EVO_COPY_POINT), and both may move a step before the frame shows it.
EVO_COPY_REACH = 2000


def evo_copy_of(members: list[dict], group_keys: set[int], before: dict[int, tuple], count: int | None) -> str | None:
    """How a summon group with no tap is its living group's copies (module doc, EVO COPIES): fewer members than a play
    puts down (`count`), each first standing within EVO_COPY_REACH of a living member (`before`: the frame before's
    rows, x, y, hp first) of `group_keys`; None when it is not."""
    if count is None or len(members) >= count:
        return None
    near = []
    for e in members:
        best = min(
            (math.dist((e["x0"], e["y0"]), r[:2]) for k in group_keys if (r := before.get(k)) is not None and r[2] > 0),
            default=None,
        )
        if best is None or best > EVO_COPY_REACH:
            return None
        near.append(round(best))
    return f"{len(members)} copies of a living group (each {near} from its nearest living member)"


#: THE TOWER TROOPS a princess tower slot can hold (module doc, THE TOWER TROOP): the table's rows.
TOWER_TROOPS = ("PrincessTower", "DaggerDuchess", "Cannoneer", "ChefTower")
#: The globals the engine reads its tower ladder from (state.rs GLOBALS_CSV) and the ledger that caps it.
GLOBALS_2018 = os.path.join(ROOT, "data", "raw", "retroroyale-2018", "csv_logic", "globals.csv")
CALIBRATION = os.path.join(ROOT, "data", "calibration.json")


def princess_tower_percent(level: int) -> int:
    """The princess tower's hitpoint multiplier at `level`, as the engine computes it (state.rs
    `tower_multiplier_percent`): 100 at level 1, then each level compounded by its percent, floored."""
    with open(GLOBALS_2018, encoding="utf-8", newline="") as fh:
        rows = list(csv.reader(fh))
    head = rows[0]
    number = {r[head.index("Name")]: r[head.index("NumberValue")] for r in rows[2:] if len(r) == len(head)}
    rate = int(number["HITPOINT_INCREASE_PERCENT_PER_TOWER_LEVEL"])
    after = int(number["HITPOINT_INCREASE_PERCENT_PER_TOWER_LEVEL_AFTER_TOURNAMENTCAP"])
    with open(CALIBRATION, encoding="utf-8") as fh:
        cap = json.load(fh)["combat"]["TOWER_HITPOINT_LADDER"]["value"]["cap_level"]
    p = 100
    for lv in range(2, level + 1):
        p = p * (100 + (rate if lv <= cap else after)) // 100
    return p


def tower_troops(towers: list[dict], doc: dict) -> dict[str, str | None]:
    """Per side, the tower troop its princess towers hold (module doc, THE TOWER TROOP), or None."""
    bases = {t["name"]: t["hitpoints"] for t in doc.get("towers") or [] if t["name"] in TOWER_TROOPS}
    bases.update({n: u["hitpoints"] for n in TOWER_TROOPS if (u := doc["units"].get(n)) and u.get("hitpoints")})
    out: dict[str, str | None] = {}
    for side in (0, 1):
        names = set()
        for t in towers:
            if t["side"] != side or t.get("slot") not in (1, 2) or not t.get("level"):
                continue
            pct = princess_tower_percent(t["level"])
            fit = [n for n, b in bases.items() if b * pct // 100 == t["max_hp"]]
            names.add(fit[0] if len(fit) == 1 else None)
        out[str(side)] = names.pop() if len(names) == 1 else None
    return out


#: How many ticks after a cast its spawned units may first be seen (A SPELL'S LEVEL FROM ITS SPAWN): a Goblin
#: Barrel's goblins land about 57 ticks after the cast from across the arena, a Barbarian Log's Barbarian about 31.
SPELL_SPAWN_WINDOW = 200
#: How near a cast row already there must be to a scheduled cast read off its spawns to stand for it (A SCHEDULED
#: SPELL'S CAST).
SCHEDULE_CAST_SLACK = 20


def spawn_schedule(card: dict) -> list[dict]:
    """A spell area's unit spawns, by delay (A SCHEDULED SPELL'S CAST): its schedule's CharacterType entries."""
    area = (card.get("spell") or {}).get("area_effect_object") or {}
    entries = [e for e in (area.get("schedule") or {}).get("entries") or [] if e.get("spawn_type") == "CharacterType"]
    return sorted(entries, key=lambda e: e.get("delay_ms") or 0)


def schedule_casts(
    spawned_groups: list[dict], ents: dict, ticks: list[int], cards_by_name: dict, deploys: list[dict]
) -> list[dict]:
    """The casts a scheduled spell's spawns give away (module doc, A SCHEDULED SPELL'S CAST): one row per cast no cast
    row of the card on its side already stands for."""
    members: dict[tuple, list[dict]] = defaultdict(list)
    for g in spawned_groups:
        card = cards_by_name.get(g["card"]) or {}
        if card.get("kind") == "spell" and spawn_schedule(card):
            members[(g["side"], g["card"])].extend(ents[k] for k in g["keys"] if k in ents)
    def point(e):
        return tuple(e["c0"]) if e.get("c0") is not None else (e["x0"], e["y0"])

    out = []
    for (side, name), es in sorted(members.items()):
        card = cards_by_name[name]
        entries = spawn_schedule(card)
        life = ((card["spell"]["area_effect_object"].get("life_duration_ms") or 0) + 49) // 50
        es.sort(key=lambda e: (e["first_index"], e["key"]))
        while es:
            start = ticks[es[0]["first_index"]]
            cast = [e for e in es if ticks[e["first_index"]] <= start + life]
            es = es[len(cast):]
            pairs = list(zip(cast, entries, strict=False))  # a cast cut short by the battle's end shows fewer
            tick = min(ticks[e["first_index"]] - (en.get("delay_ms") or 0) // 50 for e, en in pairs)
            xs = [point(e)[0] for e, en in pairs if (en.get("x") or {}).get("offset_milli") == 0]
            ys = [point(e)[1] for e, en in pairs if (en.get("y") or {}).get("offset_milli") == 0]
            if not xs or not ys:
                continue
            if any(
                d["kind"] == "spell" and d["side"] == side and d["card"] == name
                and abs(d["tick"] - tick) <= SCHEDULE_CAST_SLACK
                for d in deploys
            ):
                continue
            out.append(
                {
                    "tick": tick,
                    "first_seen": ticks[cast[0]["first_index"]],
                    "tick_evidence": f"its first {len(pairs)} spawns less their schedule delays",
                    "side": side,
                    "card": name,
                    "card_id": cast[0]["card_id"],
                    "kind": "spell",
                    "level": None,
                    "count": 0,
                    "keys": [],
                    "pos": [int(statistics.median(xs)), int(statistics.median(ys))],
                    "source": "schedule",
                    "spawn_keys": [e["key"] for e in cast],
                }
            )
    return out


def spell_levels_from_spawn(deploys: list[dict], spawned_groups: list[dict], ents: dict) -> None:
    """A spell's level off the units it put down (module doc, A SPELL'S LEVEL FROM ITS SPAWN)."""
    claimed: set[int] = set()
    for d in sorted((d for d in deploys if d["kind"] == "spell"), key=lambda d: d["tick"]):
        if d.get("spawn_keys"):
            levels = [ents[k]["level"] for k in d.pop("spawn_keys") if k in ents]
        else:
            group = next(
                (
                    g for i, g in enumerate(spawned_groups)
                    if i not in claimed and g["side"] == d["side"] and g["card"] == d["card"]
                    and d["tick"] <= g["tick"] <= d["tick"] + SPELL_SPAWN_WINDOW
                ),
                None,
            )
            if group is None:
                continue
            claimed.add(spawned_groups.index(group))
            levels = [ents[k]["level"] for k in group["keys"] if k in ents]
        if not levels:
            continue
        level = Counter(levels).most_common(1)[0][0]
        if level != d.get("level"):
            note = f"its spawn's level {level} (was {d.get('level')}, {d.get('level_source')})"
            d["level_evidence"] = f"{d['level_evidence']}; {note}" if d.get("level_evidence") else note
            d["level"], d["level_source"] = level, "spawn"


def spell_damage_at(doc: dict, card: dict, level: int) -> int | None:
    """A damaging spell's hit on a troop or a building at a unified level, scaled as the engine scales it (card.rs
    `level_multiplier`: the ladder entered at level_scaling.base_level, truncating division). None for a card with no
    damage, and for a level its ladder does not have."""
    return scaled_at(doc, card, card.get("damage"), level)


def spell_pulse_at(doc: dict, card: dict, level: int, building: bool) -> int | None:
    """One pulse of a spell whose area's buff deals damage over time, at a unified level, as the engine deals it
    (status.rs `pulse_amount`, calibration status.BUFF_PULSE_AMOUNT scaled_per_second_times_frequency: the level-scaled
    DamagePerSecond's HitFrequency share, truncated; state.rs: times the buff's BuildingDamagePercent on a building,
    truncated). Poison, 36 a second at 1000 ms: 83 at level 10, 92 at 11, 111 at 13. The Earthquake, 32 at 1000 ms and
    350 % on a building: 81 on a troop and 283 on a building at 11. None for a card whose area does not pulse twice in
    its life (a train needs two pulses: the Tornado's 550 ms in a 1050 ms area), and for a level its ladder lacks."""
    area = (card.get("spell") or {}).get("area_effect_object") or {}
    buff = area.get("buff") or {}
    dps, every = buff.get("damage_per_second"), buff.get("hit_frequency_ms")
    if not dps or not every or every <= 0 or (area.get("life_duration_ms") or 0) < 2 * every:
        return None
    per_second = scaled_at(doc, card, dps, level)
    if per_second is None:
        return None
    pulse = per_second * every // 1000
    if building and buff.get("building_damage_percent"):
        pulse = pulse * buff["building_damage_percent"] // 100
    return pulse


def scaled_at(doc: dict, card: dict, base: int | None, level: int) -> int | None:
    """`base` at a unified level on `card`'s ladder (`spell_damage_at`); None for no base, or a level it lacks."""
    ls = card.get("level_scaling")
    if not base or not ls:
        return None
    first = ls.get("base_level", doc["rarities"][ls["rarity"]]["relative_level"] + 1)
    # the ladder continued one level past its count (card.rs LEVELS_PAST_COUNT: the rarity's unused_tail)
    ladder = doc["rarities"][ls.get("ladder_rarity") or ls["rarity"]]
    table = ls["multiplier_percent_by_level"] + ladder.get("unused_tail", [])[:LEVELS_PAST_COUNT]
    step = level - first
    return base * table[step] // 100 if 0 <= step < len(table) else None


#: The levels past a rarity's count a card is played at (card.rs LEVELS_PAST_COUNT): the live max-level cards' 17.
LEVELS_PAST_COUNT = 1


def playable_levels(doc: dict, card: dict) -> range:
    """The unified levels a card can be played at: its CARD rarity's ladder (card.rs `level_multiplier`: the local
    level, level - relative_level, in 1..=level_count + LEVELS_PAST_COUNT). A Log is a Legendary, 9 to 17, though its
    damage scales on the Common ladder from 1: a hit that only a level-8 Log could land is not a Log's. The whole
    ladder for a card whose rarity the table does not list."""
    r = doc["rarities"].get(card.get("rarity") or "")
    if not r:
        return range(1, 17 + LEVELS_PAST_COUNT)
    return range(r["relative_level"] + 1, r["relative_level"] + r["level_count"] + LEVELS_PAST_COUNT + 1)


def spell_pulse_votes(d: dict, doc: dict, card: dict, ents: dict, per_tick_rows: list, ticks: list, i0: int) -> Counter:
    """A PULSING SPELL'S LEVEL (spell_levels_from_damage): per playable level, how many drops inside SPELL_HIT_WINDOW
    frames from `i0`, on enemies that are not towers, stood within the area's radius + SPELL_VICTIM_MARGIN of the cast
    point and lived, are that level's pulse (`spell_pulse_at`, up to SPELL_HIT_SLACK more) on a victim that takes the
    same pulse again one HitFrequency (SPELL_PULSE_TOLERANCE) before or after."""
    ix, iy, ihp = (TRUTH_COLUMNS.index(c) for c in ("x", "y", "hp"))
    area = card["spell"]["area_effect_object"]
    every = area["buff"]["hit_frequency_ms"] // 50  # 50 ms a tick
    reach = (area.get("radius_milli") or 0) + SPELL_VICTIM_MARGIN
    foes = [e for e in ents.values() if e["side"] != d["side"] and e["card_id"] >= 0]
    drops: dict[int, list[tuple[int, int]]] = defaultdict(list)
    for i in range(max(i0, 1), min(i0 + SPELL_HIT_WINDOW, len(per_tick_rows))):
        for e in foes:
            a, b = per_tick_rows[i - 1].get(e["key"]), per_tick_rows[i].get(e["key"])
            if a is None or b is None or b[ihp] <= 0 or a[ihp] <= b[ihp]:
                continue
            if math.dist((a[ix], a[iy]), d["pos"]) <= reach:
                drops[e["key"]].append((ticks[i], a[ihp] - b[ihp]))
    votes: Counter = Counter()
    for key, seen in drops.items():
        building = ents[key].get("kind_first") in BUILDING_KINDS
        for lv in playable_levels(doc, card):
            pulse = spell_pulse_at(doc, card, lv, building)
            at = [t for t, x in seen if pulse is not None and pulse <= x <= pulse + SPELL_HIT_SLACK]
            votes[lv] += sum(1 for t in at if any(abs(abs(t - u) - every) <= SPELL_PULSE_TOLERANCE for u in at))
    return +votes


def spell_levels_from_damage(
    deploys: list[dict], doc: dict, cards_by_name: dict, ents: dict, per_tick_rows: list, ticks: list
) -> None:
    """A damaging spell's LEVEL, read off what its first hit took. A capture records no level for a cast, so a spell
    row takes its side's mode (`level_source` "side mode"). That is wrong where a side levels its spells apart from its
    troops: 20260918-112751's side-0 Fireball of tick 583 took 357 from a Goblin Hut on 607 (358 with the hut's decay
    tick), its ladder's level 4, where the side mode 3 plays 325. The spell's HIT is the first frame, from its row's
    tick on, on which an enemy that is not a tower (card id >= 0) and stood within the spell's radius +
    SPELL_VICTIM_MARGIN of its point loses at least the spell's least damage and lives. A level fits when every such
    drop is its damage at that level up to SPELL_HIT_SLACK more, and, where the hit KILLS an enemy in reach (it is gone
    or at 0 on the hit's frame, its last hp at least the least damage), the damage up to SPELL_HIT_SLACK more covers
    that last hp (item 58: 20260918-115249.b1's side-0 Fireballs of t2074 and t3183 kill a 358-hp Goblin Hut, its
    decay tick included, and a 351-hp Musketeer, where the side mode's level 3 lands 326; level 4's 357 covers both,
    and at level 4 the report reaches the bar). A kill another hit shares the tick with reads as the spell's alone.
    Only the levels the card can be played at are tried (`playable_levels`): a Log is a Legendary, so a drop that only
    its level 8 lands (live 20261007-005625-A read Logs at 2, 6 and 8, and the harness dropped each row as a level the
    card does not have) is no Log hit. One fitting level replaces the side mode
    (`level_source` "damage"); several are settled by the one nearest the side mode; none keep the side mode. The drops
    are in `level_evidence` either way.

    A PULSING SPELL'S LEVEL: a spell whose area's buff deals damage over time (Poison, the Earthquake: `spell_pulse_at`)
    has no `damage`, so no hit of it was read and every cast kept the side mode; live, our Poison at the side mode 12
    dealt level 11's 92 a pulse and at 14 level 13's 111. Nor is its first drop its hit: its first pulse falls 25 ticks
    after the elixir drop and other damage lands before it (the first-drop rule fits no level on 243 of the 637 live
    casts). Its PULSE TRAINS vote instead (`spell_pulse_votes`): a drop inside the window on an enemy within the area's
    radius + SPELL_VICTIM_MARGIN that a level's pulse explains (up to SPELL_HIT_SLACK more) votes for that level when
    the same victim takes that pulse again one HitFrequency (SPELL_PULSE_TOLERANCE) before or after; the most votes
    win, a tie the one nearest the side mode, and an unread cast takes its card level (below). Live, 248 fixtures: 584
    of 637 Poison casts read off their trains, no cast's votes split, and 597 change level (12 -> 11 on 285, 12 -> 10
    on 6, 14 -> 13 on 306, by train or card level); the Earthquake reads 9, 11 and 12 in different battles (troops 67,
    81, 89 a pulse; buildings 234, 283, 311 and their decay tick), and 11 of its 27 casts change.

    ONE LEVEL PER CARD PER SIDE: a cast whose hit is not read (no drop inside the window, or none any level fits) takes
    the level its side's other casts of the same card were read at (`level_source` "card level"; the most read, the
    one nearest the side mode on a tie, then the lowest), and the side mode only when none was read. 20260918-112751's
    side 0 cast three Fireballs: the first two read level 4, and the third (tick 3032) fell to the side mode 3; at 4 the
    fixture gains 1,186 unit-ticks within 250 and 1,130 hp exact (parity, round 9 item 39). It is the one card in the 73
    fixtures cast at two levels by one side."""
    ix, iy, ihp = (TRUTH_COLUMNS.index(c) for c in ("x", "y", "hp"))
    index_of = {t: i for i, t in enumerate(ticks)}
    read: dict[tuple, Counter] = defaultdict(Counter)
    read_rows: set[int] = set()
    for d in deploys:
        if d["kind"] != "spell" or d.get("level_source") != "side mode" or d.get("level") is None:
            continue
        card = cards_by_name.get(d["card"]) or {}
        pulsing = spell_pulse_at(doc, card, d["level"], False) is not None
        if not pulsing and spell_damage_at(doc, card, d["level"]) is None:
            continue
        reach = (card.get("area_damage_radius_milli") or (card.get("projectile") or {}).get("radius_milli") or 0)
        reach += SPELL_VICTIM_MARGIN
        i0 = index_of.get(d["tick"])
        if i0 is None and d.get("source") == "effect":
            # a cast dated before its first sighting (`launch_tick`): its hit is looked for from that sighting on
            i0 = index_of.get(d.get("first_seen"))
        if i0 is None:
            continue
        if pulsing:
            # A PULSING SPELL'S LEVEL: its pulse trains vote (`spell_pulse_votes`); the most votes win
            votes = spell_pulse_votes(d, doc, card, ents, per_tick_rows, ticks, i0)
            fits = [lv for lv, n in votes.items() if n == max(votes.values())]
            d["level_evidence"] = (
                f"pulse trains: level votes {dict(sorted(votes.items()))}; fitting levels {sorted(fits)}"
                if votes else "no pulse train on an enemy within its reach inside the window"
            )
            if fits:
                best = min(fits, key=lambda lv: (abs(lv - d["level"]), lv))
                read[(d["side"], d["card"])][best] += 1
                read_rows.add(id(d))
                if best != d["level"]:
                    d["level"], d["level_source"] = best, "damage"
            continue
        foes = [e for e in ents.values() if e["side"] != d["side"] and e["card_id"] >= 0]
        # a drop below the spell's least damage is not its hit (a building's decay tick, a troop's chip); both the least
        # damage and the fitting levels are over the levels the card can be played at (`playable_levels`)
        playable = playable_levels(doc, card)
        least = min((x for lv in playable if (x := spell_damage_at(doc, card, lv)) is not None), default=None)
        if least is None:
            continue
        hit = None
        for i in range(max(i0, 1), min(i0 + SPELL_HIT_WINDOW, len(per_tick_rows))):
            drops, kills = [], []
            for e in foes:
                a, b = per_tick_rows[i - 1].get(e["key"]), per_tick_rows[i].get(e["key"])
                if a is None or a[ihp] <= 0 or math.dist((a[ix], a[iy]), d["pos"]) > reach:
                    continue
                # A KILL BOUNDS THE LEVEL (item 58): an enemy in reach whose last hp is at least the spell's least
                # damage and that is gone (or at 0) on the hit's frame took at least that much.
                if b is None or b[ihp] <= 0:
                    if a[ihp] >= least:
                        kills.append(a[ihp])
                    continue
                if a[ihp] - b[ihp] >= least:
                    drops.append(a[ihp] - b[ihp])
            if drops or kills:
                hit = (ticks[i], sorted(drops), sorted(kills))
                break
        if hit is None:
            d["level_evidence"] = "no enemy within its reach lost hp inside the window"
            continue
        fits = [
            lv for lv in playable
            if (dmg := spell_damage_at(doc, card, lv)) is not None
            and all(dmg <= x <= dmg + SPELL_HIT_SLACK for x in hit[1])
            and all(dmg + SPELL_HIT_SLACK >= k for k in hit[2])
        ]
        kill_note = f"; kills of last hp {hit[2]}" if hit[2] else ""
        d["level_evidence"] = f"hit on {hit[0]}: drops {hit[1]}{kill_note}; fitting levels {fits}"
        if fits:
            best = min(fits, key=lambda lv: (abs(lv - d["level"]), lv))
            read[(d["side"], d["card"])][best] += 1
            read_rows.add(id(d))
            if best != d["level"]:
                d["level"] = best
                d["level_source"] = "damage"
    # one level per card per side: an unread cast takes its side's read level of that card
    for d in deploys:
        if d["kind"] != "spell" or d.get("level_source") != "side mode" or id(d) in read_rows:
            continue
        levels = read.get((d["side"], d["card"]))
        if not levels:
            continue
        top = max(levels.values())
        level = min((lv for lv, n in levels.items() if n == top), key=lambda lv: (abs(lv - d["level"]), lv))
        note = f"the side's read casts of {d['card']}: levels {dict(sorted(levels.items()))}"
        d["level_evidence"] = f"{d['level_evidence']}; {note}" if d.get("level_evidence") else note
        d["level"] = level
        d["level_source"] = "card level"


def tunnel_destinations(deploys: list[dict], ents: dict, per_tick_rows: list, cards_by_name: dict) -> None:
    """Give every deploy of a card that travels underground (`spawn_pathfind` in cards.json: the Miner, the Goblin
    Drill) its DESTINATION, the point it surfaces at, which the harness must play instead of `pos` (the tunnel's
    first frame, next to the owner's King). Measured on the client 16.402 corpus (083112):
    - no morph (the Miner): the SAME entity jumps to the destination and turns state 4 on the tick its tunnel ends,
      so the destination is its first state-4 position after its state-6 frames;
    - a morph (the Goblin Drill): the tunnel unit vanishes and the building appears on the next frame at the
      destination, so the destination is the first position of the entity of its side that first appears on the
      tunnel's last frame + 1 (+ 2 across a frame gap), the nearest to the tunnel's last position within
      TUNNEL_SURFACE_MAX.
    A deploy whose surfacing is not in the frames keeps no destination and says why in `destination_evidence`."""
    ix, iy, ist = (TRUTH_COLUMNS.index(c) for c in ("x", "y", "state"))
    by_key = {e["key"]: e for e in ents.values()}
    for d in deploys:
        sp = (cards_by_name.get(d.get("card")) or {}).get("spawn_pathfind")
        if not sp or not d.get("keys"):
            continue
        e = by_key.get(d["keys"][0])
        if e is None:
            continue
        rows = [(i, per_tick_rows[i].get(e["key"])) for i in range(e["first_index"], e["last_index"] + 1)]
        rows = [(i, r) for i, r in rows if r is not None]
        if not sp.get("morph"):
            dug = False
            for i, r in rows:
                dug = dug or r[ist] == 6
                if dug and r[ist] == 4:
                    d["destination"] = [r[ix], r[iy]]
                    d["destination_evidence"] = f"the unit's first state-4 frame after its tunnel, index {i}"
                    break
            else:
                if rows and rows[0][1][ist] == 4:
                    # the whole tunnel fell inside a frame gap: first seen already surfaced (083112-A, a 1-tick trip)
                    d["destination"] = [rows[0][1][ix], rows[0][1][iy]]
                    d["destination_evidence"] = "first seen already surfaced (state 4): the tunnel fell in a frame gap"
                else:
                    d["destination_evidence"] = "the unit never surfaced (state 6 -> 4) inside the frames"
            continue
        if not rows:
            continue
        last_i, last = rows[-1]
        best = None
        for o in ents.values():
            if o["side"] != e["side"] or o["key"] == e["key"] or o["first_index"] not in (last_i + 1, last_i + 2):
                continue
            r = per_tick_rows[o["first_index"]].get(o["key"])
            if r is None:
                continue
            dist = math.dist((r[ix], r[iy]), (last[ix], last[iy]))
            if dist <= TUNNEL_SURFACE_MAX and (best is None or dist < best[0]):
                best = (dist, o, r)
        if best is None:
            d["destination_evidence"] = "no entity of its side appeared next to the tunnel's last frame"
            continue
        d["destination"] = [best[2][ix], best[2][iy]]
        d["destination_evidence"] = (
            f"the {sp['morph']} entity (key {best[1]['key']}) that appeared on the frame after the tunnel's last,"
            f" {round(best[0])} from its last position"
        )


def tunnel_spawn_ticks(
    deploys: list[dict], ents: dict, ticks: list[int], towers: list[dict], cards_by_name: dict
) -> None:
    """Give every deploy of a card that travels underground the tick of its tunnel's FIRST frame, counted in tunnel
    steps from its owner's King. Measured on the client 16.402 corpus (083112, the seat that saw each first frame):
    a tunnel's first frame stands two SpawnPathfindSpeed steps out from its King's centre (the Goblin Drill 565 and
    596 for 600, the Miner 1245 for 1300), each tick after it one step more, so a first frame seen n steps out
    was the tunnel's first frame n - 2 ticks earlier. A Miner first seen already surfaced (state 4) shows its last
    tunnel point as its x2, y2, one tick before. The deploy-end and frame-gap rules do not reach this: a Miner's
    deploy timer starts when it SURFACES, and a frame gap leaves a range the latest end of which is a step late
    (083112-A: the Drills of 250 and 1487 are 249 and 1486 on the seat that saw them; the Miner of 1204 is 1203).
    The count is used only when the distance is within a quarter step of a whole number of steps (a straight
    start), and only when it moves the tick; `tick_evidence` then says so."""
    kings = {}
    for t in towers:
        if t.get("slot") == 0:
            kings[t["side"]] = (t["x"], t["y"])
    by_key = {e["key"]: e for e in ents.values()}
    for d in deploys:
        sp = (cards_by_name.get(d.get("card")) or {}).get("spawn_pathfind")
        if not sp or not d.get("keys") or not sp.get("speed"):
            continue
        e = by_key.get(d["keys"][0])
        king = kings.get(d.get("side"))
        if e is None or king is None or not e.get("states"):
            continue
        speed = sp["speed"]
        first_state = e["states"][0][1]
        t_first = ticks[e["first_index"]]
        if first_state == 6:
            p, t_p, seen = (e["x0"], e["y0"]), t_first, "first seen under ground"
        elif first_state == 4 and e.get("c0") is not None and tuple(e["c0"]) != (e["x0"], e["y0"]):
            p, t_p, seen = tuple(e["c0"]), t_first - 1, "first seen surfaced, its last tunnel point one tick before"
        else:
            continue
        dist = math.dist(p, king)
        n = round(dist / speed)
        if n < 2 or abs(dist - n * speed) > speed / 4:
            continue
        tick = t_p - (n - 2)
        if tick == d["tick"]:
            continue
        d["tick_evidence"] = (
            f"tunnel count: {seen} at tick {t_p}, {round(dist)} from its King = {n} steps of {speed},"
            f" so its first frame (two steps out) is {tick} (was {d['tick']}: {d['tick_evidence']})"
        )
        d["tick"] = tick


def mirror_plays(deploys: list[dict], decks: dict, id_table: dict) -> None:
    """Publish a MIRROR play as a Mirror. The Mirror card replays its side's last card one level up (for that card's
    cost + 1), and the capture shows only the copy. Measured on the client 16.402 corpus, 090204 t805: an ElixirGolem
    at level 12, right after the side's level-11 ElixirGolem, in a deck holding Mirror. A side's deploy is a Mirror
    play when its deck holds Mirror, it repeats the side's previous play, and its level is one above that card's level
    in the side's other deploys. The row becomes card Mirror, kind "mirror", with `mirrored` naming the copy; its
    keys, position and level (the copy's) are kept."""
    mirror_id = next((cid for cid, n in id_table.items() if n == "Mirror"), None)
    if mirror_id is None:
        return
    for side in (0, 1):
        if mirror_id not in (decks.get(side) or []):
            continue
        mine = [d for d in deploys if d["side"] == side]
        for k in range(1, len(mine)):
            prev, d = mine[k - 1], mine[k]
            if d["kind"] == "spell" or d["card"] != prev["card"] or d["level"] is None:
                continue
            others = [x["level"] for x in mine if x is not d and x["card"] == d["card"] and x["level"] is not None]
            base = Counter(others).most_common(1)[0][0] if others else None
            if base is None or d["level"] != base + 1:
                continue
            d["mirrored"] = {"card": d["card"], "card_id": d["card_id"]}
            d["mirror_evidence"] = (
                f"{d['card']} at level {d['level']}, one above the side's {d['card']} level {base}, right after"
                f" its {prev['card']} at {prev['tick']}, in a deck holding Mirror"
            )
            d["card"], d["card_id"], d["kind"] = "Mirror", mirror_id, "mirror"


def object_record(track: dict, frames: list[dict], rotate: bool) -> dict:
    """One spell object as the fixture publishes it (module doc, SPELL OBJECTS): ticks as
    the capture has them, every point through `arena_point`."""
    last = track["last_index"]
    depart = track["depart_index"]
    return {
        "first": frames[track["first_index"]]["tick"],
        "launch": arena_point(*track["launch"], rotate),
        "depart": frames[depart]["tick"] if depart is not None else None,
        "target": arena_point(*track["target"], rotate),
        "last_seen": frames[last]["tick"],
        "end": arena_point(*track["end"], rotate),
        "arrival": frames[last + 1]["tick"] if last + 1 < len(frames) else None,
    }


def departures(records: list[dict]) -> list[list]:
    """[[depart tick, objects], ...] in tick order; objects never seen moving count under
    null, last."""
    n = Counter(r["depart"] for r in records)
    return [[t, n[t]] for t in sorted(n, key=lambda t: (t is None, t or 0))]


# ---------------------------------------------------------------------------
# pair dating (module doc, PAIR DATING)


def capture_stamp(path: str) -> tuple[str, int] | None:
    """(day, second of the day) a capture's file name was stamped with, or None for a name without a stamp."""
    base = os.path.basename(path)
    if not base.startswith("frames-"):
        return None
    # The stamp follows any name prefix (frames-auto-, frames-liveplay-): its first YYYYMMDD-HHMMSS.
    m = re.search(r"(\d{8})-(\d{6})", SEAT_TAG.sub("", base)[len("frames-") :].split(".")[0])
    if m is None:
        return None
    hms = m.group(2)
    return m.group(1), int(hms[:2]) * 3600 + int(hms[2:4]) * 60 + int(hms[4:6])


def battle_partners(capture: str, pool: list[str]) -> list[str]:
    """The captures in `pool` that recorded the battle of `capture` from the OTHER seat: another seat tag, a stamp
    within BATTLE_STAMP_SECONDS (the rule placement_files_for pairs a battle's logs by), and every part of a capture
    split into parts (.b1, .b2). A capture without a seat tag has no partner."""
    tag, stamp = SEAT_TAG.search(os.path.basename(capture)), capture_stamp(capture)
    if tag is None or stamp is None:
        return []
    # A LIVE capture (frames-liveplay-) is its battle's only seat: our client on the ladder, the opponent a stranger.
    # Another live capture stamped within seconds is another account's battle, never the other seat of this one.
    if os.path.basename(capture).startswith("frames-liveplay-"):
        return []
    out = []
    for p in pool:
        t2, s2 = SEAT_TAG.search(os.path.basename(p)), capture_stamp(p)
        if t2 is None or s2 is None or t2.group(1) == tag.group(1):
            continue
        if s2[0] == stamp[0] and abs(s2[1] - stamp[1]) <= BATTLE_STAMP_SECONDS:
            out.append(p)
    return sorted(out)


def deploy_window(d: dict, index: dict[int, int], ticks: list[int]) -> tuple[int, int] | None:
    """(lo, hi): the ticks a deploy row could have appeared on, given the capture's frames: (the frame before its
    first sighting, its first sighting]. A row with no sighting of its own (a cast dated by its elixir drop) is
    sighted on its tick. A row dated OUTSIDE that window by other evidence (a group shown late, a tunnel's steps, a
    Barbarian Barrel's flight) has the one tick it was dated on. None for a row first seen on the capture's first
    frame, or off its frames. `index` maps each of `ticks` to its position."""
    seen = d["first_seen"] if d.get("first_seen") is not None else d["tick"]
    i = index.get(seen)
    if not i:
        return None
    lo, hi = ticks[i - 1] + 1, ticks[i]
    if not lo <= d["tick"] <= hi:
        return d["tick"], d["tick"]
    return lo, hi


def seat_offer(deploys: list[dict], ticks: list[int]) -> list[dict]:
    """What a capture's rows give the battle's other seat: each row's card, side, tick and window (`deploy_window`),
    for every row that has a window. `ticks` are the capture's frame ticks (a stride-1 fixture's `truth.ticks`)."""
    index = {t: i for i, t in enumerate(ticks)}
    out = []
    for d in deploys:
        w = deploy_window(d, index, ticks)
        if w is not None:
            out.append({"card": d["card"], "side": d["side"], "tick": d["tick"], "window": w})
    return out


def pair_date(deploys: list[dict], ticks: list[int], partners: list[tuple[str, list[dict]]]) -> int:
    """Date a row by the battle's other seat: a row whose window (`deploy_window`) holds several ticks, matched to
    exactly one row of the partners' offers (`seat_offer`: the same card and side, dated within PAIR_MATCH_TICKS),
    takes the one tick the two windows share, and `tick_evidence` becomes "exact (pair-dated with ...)", keeping what
    the capture alone gave. A row whose windows share two ticks or none, or with no partner row, keeps its tick.
    `partners` = [(the partner fixture's name, its offer), ...]. Returns the number of rows that moved."""
    index = {t: i for i, t in enumerate(ticks)}
    moved = 0
    for d in deploys:
        w = deploy_window(d, index, ticks)
        if w is None or w[0] == w[1]:
            continue
        match = [
            (name, r)
            for name, offer in partners
            for r in offer
            if r["card"] == d["card"] and r["side"] == d["side"] and abs(r["tick"] - d["tick"]) <= PAIR_MATCH_TICKS
        ]
        if len(match) != 1:
            continue
        name, r = match[0]
        lo, hi = max(w[0], r["window"][0]), min(w[1], r["window"][1])
        if lo != hi:
            continue
        # "exact" first, as for every other pinned row: readers take a row whose evidence starts so as dated to the tick
        d["tick_evidence"] = (
            f"exact (pair-dated with {name}: its window [{r['window'][0]}, {r['window'][1]}] and this row's"
            f" [{w[0]}, {w[1]}] share only {lo}; this capture alone gave {d['tick']}: {d['tick_evidence']})"
        )
        if lo != d["tick"]:
            d["tick"] = lo
            moved += 1
    return moved


# ---------------------------------------------------------------------------
# the fixture


#: AN ABILITY PRESS (module doc, AN ABILITY PRESS): the behaviour state of a champion or hero in its ability's cast
#: hold.
CAST_STATE = 10
#: How long after its elixir leaves the pool a pressed hero may enter its cast hold, ticks: on the drop's own frame in
#: 896 of the live set's 902 confirmed presses (2026-10-09), 8 and 14 ticks later behind a hit in progress, 90 ticks
#: later under a Freeze (20261008-031035, press 3363, both Princes held until 3453); one frame BEFORE the drop on 4.
PRESS_ONSET_TICKS = 120


def ability_presses(
    deploys: list[dict],
    claimed: set,
    ents: dict,
    per_tick_rows: list,
    ticks: list[int],
    elixir_by_side: dict[int, list],
    doc: dict,
    cards_by_name: dict,
    name_to_id: dict[str, int],
) -> list[dict]:
    """THE ABILITY PRESSES (kind "ability", the harness's KIND_ABILITY), read off the capture for both sides alike: one
    row per elixir drop that no deploy or cast row explains, whose amount is the ability cost (`mana_cost`) of a
    champion or hero of that side alive on the frame before the drop's. An ability with a cast hold (`cast_ms` > 0:
    the Little Prince, the hero Wizard, the hero Bowler) must also show its press: its unit enters CAST_STATE from one
    frame before the drop until PRESS_ONSET_TICKS after it (and before the side's next drop of that cost), or the
    ability's own unit appears within PRESS_ONSET_TICKS (a Prince whose last frame was the press's still put its guard
    down: 20261007-181010, 5193). One without a cast hold (the Golden Knight's dash, held until a target is in reach)
    is read off the drop alone. The row's tick is the drop's frame, as a deploy's is the frame its cost leaves the pool;
    the harness issues it a tick earlier, as a deploy. Our own tap log is not read: its press receipts are these drops
    (795 of 797 at the same tick, 2026-10-09), and the capture also holds the presses no tap of ours made."""
    hero_rows = {r["form_of"]: r for r in doc.get("hero_forms") or [] if r.get("form_of")}
    explained = {(d["side"], t) for d in deploys for t in (d["tick"], d.get("first_seen")) if t is not None}
    explained |= set(claimed)
    buttons = []
    for e in ents.values():
        card = cards_by_name.get(e.get("card") or "")
        if e.get("role") != "summon" or card is None:
            continue
        hero = e["card_id"] // 1_000_000 == HERO_CLASS
        row = hero_rows.get(card["name"]) if hero else card
        ability = (row or {}).get("ability")
        if ability and ability.get("mana_cost"):
            buttons.append((e, card, row if hero else None, ability))

    def state(e, i):
        r = per_tick_rows[i].get(e["key"]) if 0 <= i < len(per_tick_rows) else None
        return r[5] if r is not None else None

    out = []
    for side in (0, 1):
        pool = elixir_by_side.get(side) or []
        # the side's regeneration per tick: ELIXIR_REGEN_MAX_PER_TICK is triple elixir's, and an event's pool gains up
        # to 1,250 a tick (20261008-155251), which put a 1-cost press 2,500 off its cost after a frame gap
        steps = sorted(
            (pool[k] - pool[k - 1]) / (ticks[k] - ticks[k - 1])
            for k in range(1, len(pool))
            if pool[k] is not None and pool[k - 1] is not None and 0 < pool[k] - pool[k - 1] < ELIXIR_UNIT // 2
        )
        regen = max(ELIXIR_REGEN_MAX_PER_TICK, steps[len(steps) * 95 // 100] if steps else 0)
        drops = [
            i
            for i in range(1, len(pool))
            if pool[i] is not None
            and pool[i - 1] is not None
            and pool[i - 1] - pool[i] > ELIXIR_UNIT // 2
            and (side, ticks[i]) not in explained
        ]

        def pays(i, cost, pool=pool, regen=regen):
            tolerance = regen * (ticks[i] - ticks[i - 1]) + ELIXIR_UNIT // 10
            return abs((pool[i - 1] - pool[i]) - cost * ELIXIR_UNIT) <= tolerance

        for n, i in enumerate(drops):
            gap = ticks[i] - ticks[i - 1]
            found = []
            for e, card, hero_row, ab in buttons:
                # pressed on the tick before the drop's frame: alive on the frame before it
                alive = e["first_index"] <= i - 1 <= e["last_index"]
                if e["side"] != side or not alive or not pays(i, ab["mana_cost"]):
                    continue
                if not ab.get("cast_ms"):
                    found.append((PRESS_ONSET_TICKS + 1, -e["first_index"], e, card, hero_row, ab,
                                  f"elixir drop of {ab['mana_cost']}; no cast hold"))
                    continue
                later = [j for j in drops[n + 1:] if pays(j, ab["mana_cost"])]
                until = min(ticks[i] + PRESS_ONSET_TICKS, ticks[later[0]] - 1 if later else ticks[-1])
                onset = next(
                    (j for j in range(i - 1, e["last_index"] + 1)
                     if ticks[j] <= until and state(e, j) == CAST_STATE and state(e, j - 1) != CAST_STATE),
                    None,
                )
                if onset is not None:
                    why = f"elixir drop of {ab['mana_cost']}; cast state {CAST_STATE} at {ticks[onset] - ticks[i]:+d}"
                    found.append((max(ticks[onset] - ticks[i], 0), -e["first_index"], e, card, hero_row, ab, why))
                    continue
                unit = (ab.get("effect") or {}).get("unit")
                put = next(
                    (o for o in ents.values() if unit and o["side"] == side and o.get("unit") == unit
                     and 0 <= ticks[o["first_index"]] - ticks[i] <= PRESS_ONSET_TICKS),
                    None,
                )
                if put is not None:
                    after = ticks[put["first_index"]] - ticks[i]
                    why = f"elixir drop of {ab['mana_cost']}; its {unit} at +{after}"
                    found.append((after, -e["first_index"], e, card, hero_row, ab, why))
            if not found:
                continue
            found.sort(key=lambda c: (c[0], c[1]))
            _, _, e, card, hero_row, ab, why = found[0]
            out.append(
                {
                    "tick": ticks[i],
                    "first_seen": ticks[i],
                    "tick_evidence": "exact" if gap == 1 else "frame_gap",
                    "first_seen_gap": gap,
                    "side": side,
                    "card": card["name"],
                    "card_id": name_to_id.get(card["name"], e["card_id"]),
                    "kind": "ability",
                    "level": e["level"],
                    "count": 0,
                    "keys": [e["key"]],
                    "pos": [0, 0],
                    "source": "ability_press",
                    "form": "hero" if hero_row else "base",
                    "form_row": hero_row["name"] if hero_row else card["name"],
                    "press": {"ability": ab.get("name"), "cost": ab["mana_cost"], "evidence": why},
                }
            )
    return out


def build(
    capture: str,
    placements: list[str],
    stride: int,
    until_tick: int | None,
    census: dict | None,
    id_table: dict[int, str],
    doc: dict,
    register: dict,
    name_to_id: dict[str, int],
    card_names: set[str],
    seats: dict[str, str] | None = None,
    nominal: dict[tuple[str, int], list[tuple[int, int]]] | None = None,
    form_rows: dict[int, str] | None = None,
    partners: list[tuple[str, list[dict]]] | None = None,
) -> dict:
    # the nominal offsets of a recovered tile (RECOVERED TILE): the committed measurement unless the caller hands some
    if nominal is None:
        nominal = load_nominal_offsets()
    # the evolved and hero rows (A DEPLOY'S FORM); without the 15.535 pack no deploy's form is read
    if form_rows is None and not missing_id_files():
        form_rows = load_form_rows()
    header, raw_frames = read_capture(capture)
    frames, dup, back = dedupe(raw_frames)
    # a frame read a tick late takes the tick its contents show (relabel_late_reads)
    late = relabel_late_reads(frames)
    if until_tick is not None:
        frames = [f for f in frames if f["tick"] <= until_tick]
    reasons: list[str] = []
    # The seat map is the captures FOLDER's, so a capture's letter does not depend on which
    # placement logs happen to sit beside it, nor on which captures this run was handed.
    if seats is None:
        seats = folder_seats(
            os.path.dirname(os.path.abspath(capture)), CAPTURE_SUFFIX, [capture, *placements]
        )
    fx: dict = {
        "format": FORMAT,
        # What a deploy's `tick` means (DEPLOY POSITION AND TICK above: the tick the entities
        # came to exist); the replay harness refuses a fixture that does not say.
        "deploy_tick_convention": DEPLOY_TICK_CONVENTION,
        "generated_by": "tools/make_replay_fixture.py",
        "cards_json_fnv1a64": doc.get("_fnv1a64"),
        "capture": public_name(capture, seats),
        "placements": [public_name(p, seats) for p in placements],
        "frame": {
            "blue_native_side": 0,
            "transform": "identity",
            "native_per_tile": 1000,
            "subtiles_per_native": 18,
        },
        "truth_stride": stride,
        "frames_total": len(raw_frames),
        "frames_duplicate": dup,
        "frames_out_of_order": back,
    }
    if late:
        fx["frames_relabelled_late"] = late
    client = (header or {}).get("client")
    why = content_refusal(TABLE, client.get("content_version") if isinstance(client, dict) else None)
    if why:
        fx["playable"] = False
        fx["unplayable_reasons"] = [f"card table {TABLE}: {why}"]
        return fx
    if not frames:
        fx["playable"] = False
        fx["unplayable_reasons"] = ["no frames"]
        return fx
    # -- THE BATTLE'S RATES (module doc, GAME MODE): a mode that buffs every unit is a battle the engine cannot play
    rates = battle_rates(frames)
    fx["battle_rates"] = rates
    if rates["unit_rate_percent"] not in (None, 100):
        att = rates["attack_steps"]
        reasons.append(
            f"game mode: every unit runs at {rates['unit_rate_percent']} % (attack_progress_ms grows {att['modal']} a"
            f" tick on {att['modal_count']} of {att['all']} one-tick steps, {att[f'at_{ATTACK_STEP_AT_100}']} at"
            f" {ATTACK_STEP_AT_100}; global buff {rates['global_buff'] or 'not recognised'}), and the engine has no"
            " game-mode buff"
        )

    # -- side convention: side 0 must defend low y; else turn the capture's positions back,
    # keeping its sides (module doc, THE SIDE CONVENTION)
    towers_hdr = (header or {}).get("towers") or []
    king0 = [t for t in towers_hdr if t["side"] == 0]
    king1 = [t for t in towers_hdr if t["side"] == 1]
    rotate = bool(king0 and king1) and min(t["y"] for t in king0) > min(t["y"] for t in king1)
    if rotate:
        fx["frame"] = {
            "blue_native_side": 0,
            "transform": "rotate180: (W - x, H - y), sides kept",
            "native_per_tile": 1000,
            "subtiles_per_native": 18,
        }

    def pos_of(x, y):
        return tuple(arena_point(x, y, rotate))

    def cells_of(nodes):
        """The published path as [col, row] cells, GOAL-FIRST, in the fixture's frame.

        `path_nodes` is a flat list of indices on the CELL_COLS x CELL_ROWS grid the game
        publishes paths on. It must be rotated with the positions or the two disagree: a
        fixture that flips the arena and not the path would read as a pathfinder defect on
        every rotated battle, which is the most expensive way for this to be wrong.
        """
        return [path_cell(n, rotate) for n in nodes or []]

    # -- entities across frames
    ticks = [f["tick"] for f in frames]
    ents: dict[int, dict] = {}
    per_tick_rows: list[dict[int, tuple]] = []
    for fi, f in enumerate(frames):
        ptr_to_key = {e["id"]: e["generation_key"] for e in f["entities"]}
        rows = {}
        for e in f["entities"]:
            k = e["generation_key"]
            x, y = pos_of(e["x"], e["y"])
            tgt = ptr_to_key.get(e.get("target") or "", -1) if e.get("target") else -1
            nodes = e.get("path_nodes") or []
            # in TRUTH_COLUMNS order
            rows[k] = (
                x,
                y,
                e["hp"],
                tgt,
                len(nodes),
                e["behavior_state"],
                cells_of(nodes),
                *attack_timers(e),
            )
            rec = ents.get(k)
            if rec is None:
                rec = ents[k] = {
                    "key": k,
                    "side": e["side"],
                    "card_id": e["card_id"],
                    "level": e["level"],
                    "max_hp": e["max_hp"],
                    "kind_first": e["kind"],
                    "first_index": fi,
                    "last_index": fi,
                    "frames": 0,
                    "x0": x,
                    "y0": y,
                    # the CREATION POINT (RECOVERED TILE): where the entity stood before its first frame's movement
                    "c0": pos_of(e["x2"], e["y2"]) if e.get("x2") is not None and e.get("y2") is not None else None,
                }
            rec["last_index"] = fi
            rec["frames"] += 1
            if len(rec.setdefault("states", [])) < 400:
                rec["states"].append((fi, e["behavior_state"]))
                rec.setdefault("positions", []).append((fi, x, y))
        per_tick_rows.append(rows)

    # -- a key the capture reused for another unit is two entities (split_reused_keys)
    reused = split_reused_keys(ents, per_tick_rows, ticks)
    if reused:
        fx["truth_split_keys"] = reused
    # -- a unit the capture re-keyed mid-life is one entity (merge_rekeyed)
    rekeyed = merge_rekeyed(ents, per_tick_rows, ticks)
    if rekeyed:
        fx["truth_rekeyed"] = rekeyed

    # -- mid-battle start
    first_non_tower = [e for e in ents.values() if e["first_index"] == 0 and e["card_id"] >= 0]
    if ticks[0] > 0 and first_non_tower:
        reasons.append(
            f"capture starts mid-battle: first frame is tick {ticks[0]} with"
            f" {len(first_non_tower)} non-tower entities on the board"
        )

    # -- towers: the six at the first frame, by (side, position) -> engine slot
    towers = []
    for e in ents.values():
        if e["card_id"] != -1 or e["first_index"] != 0:
            continue
        x0, y0 = e["x0"], e["y0"]
        hp0 = per_tick_rows[0][e["key"]][2]
        towers.append(
            {
                "key": e["key"],
                "side": e["side"],
                "x": x0,
                "y": y0,
                "hp": hp0,
                "max_hp": e["max_hp"],
                "level": e["level"],
                "kind_first": e["kind_first"],
            }
        )
    for t in towers:
        same = [u for u in towers if u["side"] == t["side"]]
        king = max(same, key=lambda u: u["max_hp"])
        if t is king:
            t["slot"] = 0
        else:
            t["slot"] = (
                1 if t["x"] < king["x"] else 2
            )  # engine lanes: 1 = low x (engine-left), 2 = high x
    towers.sort(key=lambda t: (t["side"], t["slot"]))
    if len(towers) != 6:
        reasons.append(f"first frame holds {len(towers)} towers, not 6")
    tower_level = {}
    for s in (0, 1):
        lv = [t["level"] for t in towers if t["side"] == s]
        tower_level[s] = Counter(lv).most_common(1)[0][0] if lv else None

    # -- classification of every non-tower entity
    cards_by_name = {c["name"]: c for c in doc["cards"]}
    unknown_ids: set[int] = set()
    for e in ents.values():
        if e["card_id"] < 0:
            e["role"] = "tower"
            continue
        name = id_table.get(e["card_id"])
        e["card"] = name
        if name is None:
            unknown_ids.add(e["card_id"])
            e["role"] = "unknown"
            e["unit"], e["deploy_summon"], e["hp_match"] = None, True, "no_card"
            continue
        unit, is_own, how = classify_unit(doc, cards_by_name.get(name), e["level"], e["max_hp"])
        e["unit"], e["deploy_summon"], e["hp_match"] = unit, is_own, how
        if how.startswith("unknown_object"):
            e["role"] = "unknown_object"
        else:
            e["role"] = "summon" if is_own else "spawned"
    for cid in sorted(unknown_ids):
        reasons.append(f"card id {cid} is not in the id table")

    # -- deploy groups: (side, card_id, first frame index) of deploy summons
    groups: dict[tuple, list[dict]] = defaultdict(list)
    for e in ents.values():
        if e["role"] == "summon":
            groups[(e["side"], e["card_id"], e["first_index"])].append(e)
    # the taps are already in the game's frame (the log's own rule): not turned with the capture
    spell_names = {n for n, c in cards_by_name.items() if c.get("kind") == "spell"}
    # the side of a log that does not record its own (read_placements): this capture's file-name
    # tag is its header's local side, the other seat's tag the other side
    own_tag = SEAT_FILE_TAG.search(os.path.basename(capture))
    own_side = (header or {}).get("local_side_native")
    default_sides: dict[str, int] = {}
    if own_tag and own_side in (0, 1):
        for pf in placements:
            m = SEAT_FILE_TAG.search(os.path.basename(pf))
            if m:
                default_sides[m.group(1)] = own_side if m.group(1) == own_tag.group(1) else 1 - own_side
    taps, decks = read_placements(
        placements, card_names, name_to_id, spell_names, default_sides, display_names(doc["cards"])
    )
    used_taps: set[int] = set()
    deploys = []
    latencies = []
    # the sides a placement log covers (module doc, AN UNLOGGED SIDE'S TROOP)
    logged_sides = {t["side"] for t in taps}
    # an Evo Skeletons group's own copies (module doc, EVO COPIES): the keys of each (side, id)'s duplicating groups
    evolutions = {r["name"]: r for r in doc.get("evolutions") or []}
    duplicating: dict[tuple, set[int]] = defaultdict(set)
    copies_of: dict[tuple, str] = {}
    for (side, cid, fi), members in sorted(
        groups.items(), key=lambda kv: (kv[0][2], kv[0][0], kv[0][1])
    ):
        members.sort(key=lambda e: e["key"])
        name0 = members[0]["card"]
        card0 = cards_by_name.get(name0) or {}
        late = shown_late_spawn(
            ticks,
            fi,
            [e.get("states", []) for e in members],
            card0.get("deploy_time_ms"),
            card0.get("summon_deploy_delay_ms"),
        )
        if late is not None:
            (tick, evidence), first_seen = late, ticks[fi]
        else:
            tick, first_seen, evidence = refine_spawn_tick(
                ticks,
                fi,
                members[0].get("states", []),
                card0.get("deploy_time_ms"),
                positions=members[0].get("positions", []) if len(members) == 1 else None,
            )
        cx = sum(e["x0"] for e in members) // len(members)
        cy = sum(e["y0"] for e in members) // len(members)
        tap_ix = None
        for ix, t in enumerate(taps):
            if ix in used_taps or t["kind"] != "deploy" or t["side"] != side or t["id"] != name_to_id.get(name0, cid):
                continue
            if t["tick"] + TAP_MIN <= first_seen <= t["tick"] + TAP_MAX:
                tap_ix = ix
                break
        name = members[0]["card"]
        card = cards_by_name.get(name) or {}
        form = deploy_form(cid, form_rows, name) if form_rows is not None else {}
        evo = evolutions.get(form.get("form_row")) if form.get("form") == "ev1" else None
        if evo and evo.get("evo_duplication"):
            why = None
            if tap_ix is None and fi > 0:
                # a hitter is a member that has finished its deploy: a play's members first shown on two frames are
                # not each other's copies
                ready = {
                    k for k in duplicating[(side, cid)]
                    if ticks[ents[k]["first_index"]] + (evo.get("deploy_time_ms") or 0) // 50 <= ticks[fi]
                }
                why = evo_copy_of(members, ready, per_tick_rows[fi - 1], evo.get("count"))
            duplicating[(side, cid)].update(e["key"] for e in members)
            if why:
                for e in members:
                    e["role"] = "spawned"
                copies_of[(side, cid, fi)] = why
                continue
        d = {
            "tick": tick,
            "first_seen": first_seen,
            "tick_evidence": evidence,
            "side": side,
            "card": name,
            "card_id": cid,
            "kind": card.get("kind", "troop"),
            "level": Counter(e["level"] for e in members).most_common(1)[0][0],
            "count": len(members),
            "keys": [e["key"] for e in members],
            "centroid": [cx, cy],
            "first_seen_gap": ticks[fi] - ticks[fi - 1] if fi > 0 else 0,
            "hp_match": Counter(e["hp_match"] for e in members).most_common(1)[0][0],
            "families": sorted(
                (register.get("cards", {}).get(name) or {}).get("families", {}).keys()
            ),
            **form,
        }
        if tap_ix is not None:
            t = taps[tap_ix]
            used_taps.add(tap_ix)
            latencies.append(first_seen - t["tick"])
            d["tap"] = {
                "tick": t["tick"],
                "native": t["native"],
                "cycled": t["cycled"],
            }
            if t["native"] and len(members) > 1:
                d["pos"], d["source"] = snap_troop_tap(t["native"]), "tap_tile"
        if "pos" not in d:
            d["pos"], d["source"] = [cx, cy], "centroid"
            if len(members) > 1:
                # the tile the members agree the game laid them around (module doc, RECOVERED TILE)
                if all(e.get("c0") is not None for e in members):
                    points = [tuple(e["c0"]) for e in members]
                    tile, d["recovery"] = recovered_tile(points, nominal.get((name, side), []))
                    if tile is None and d["recovery"] == "no nominal offsets for this card and side":
                        flying = bool((cards_by_name.get(name) or {}).get("flying_height"))
                        tile, why = laid_tile(points, side, flying)
                        d["recovery"] += "; " + why
                    if tile is not None:
                        d["pos"], d["source"] = tile, "recovered_tile"
                else:
                    d["recovery"] = "the capture gives no creation point for every member"
            elif (
                first_seen == tick
                and (d["first_seen_gap"] == 1 or evidence.startswith("exact"))
                and members[0].get("c0") is not None
                and list(members[0]["c0"]) != [cx, cy]
                and (members[0].get("states") or [(0, None)])[0][1] in (4, STATE_STAGGER_WAIT)
                and not card.get("spawn_pathfind")
            ):
                # a single unit the capture saw deploying on its creation tick and pushed on it: played where it was
                # created (module doc, CREATION POINT)
                d["pos"], d["source"] = list(members[0]["c0"]), "creation_point"
            elif (
                first_seen > tick
                and members[0].get("c0") is not None
                and (members[0].get("states") or [(0, None)])[0][1] in (4, STATE_STAGGER_WAIT)
                and not card.get("spawn_pathfind")
                and card.get("kind", "troop") == "troop"
            ):
                # a single unit created before its first frame, pushed on the ticks the capture missed: played on its
                # tile's laid point (module doc, LATE SINGLE)
                flying = bool(card.get("flying_height"))
                laid, why = late_single_point(tuple(members[0]["c0"]), side, flying, first_seen - tick)
                if laid is not None and laid != [cx, cy]:
                    d["pos"], d["source"], d["recovery"] = laid, "laid_point", why
        if (
            d["source"] == "centroid"
            and d["kind"] == "troop"
            and side not in logged_sides
            and all((e.get("states") or [(0, None)])[0][1] in (4, STATE_STAGGER_WAIT) for e in members)
            and cx % 1000
            and cy % 1000
        ):
            # a side with no tap: the centroid's tile centre (module doc, AN UNLOGGED SIDE'S TROOP)
            d["pos"], d["source"] = [cx // 1000 * 1000 + 500, cy // 1000 * 1000 + 500], "tile_centre"
        deploys.append(d)

    # spawned and unknown-object groups: truth only, cross-checked against the taps.
    # An unknown-object group that COINCIDES with a deploy tap of its card is a deploy
    # the engine cannot reproduce (the game put an object cards.json does not derive
    # from the card): the fixture is unplayable from that tick (a prefix cut).
    spawned_groups = []
    sp: dict[tuple, list[dict]] = defaultdict(list)
    for e in ents.values():
        if e["role"] in ("spawned", "unknown_object"):
            sp[(e["side"], e["card_id"], e["first_index"])].append(e)
    for (side, cid, fi), members in sorted(
        sp.items(), key=lambda kv: (kv[0][2], kv[0][0], kv[0][1])
    ):
        tick = ticks[fi]
        # a tap a deploy group already answered is not this group's (the Goblin
        # Drill's surfaced building and Goblins follow its tap inside the window)
        coincides = [
            t
            for ix, t in enumerate(taps)
            if ix not in used_taps
            and t["kind"] == "deploy"
            and t["side"] == side
            and t["id"] == name_to_id.get(members[0]["card"], cid)
            and t["tick"] + TAP_MIN <= tick <= t["tick"] + TAP_MAX
        ]
        name = members[0]["card"]
        role = members[0]["role"]
        hp_match = Counter(e["hp_match"] for e in members).most_common(1)[0][0]
        spawned_groups.append(
            {
                "tick": tick,
                "side": side,
                "card": name,
                "card_id": cid,
                "role": role,
                "unit": members[0]["unit"],
                "count": len(members),
                "keys": sorted(e["key"] for e in members),
                "hp_match": hp_match,
                "coincides_with_a_tap": bool(coincides),
                **({"copy_of": copies_of[(side, cid, fi)]} if (side, cid, fi) in copies_of else {}),
            }
        )
        if role == "unknown_object" and coincides:
            reasons.append(
                f"{name}: deployed at tick {tick} as an object cards.json does not derive from"
                f" the card ({hp_match}; hp {members[0]['max_hp']})"
            )

    # THE CASTER'S ELIXIR (module doc, ELIXIR), per frame and side: it labels an entity-less cast and backs
    # an effect cast (below).
    elixir_by_side: dict[int, list] = {0: [], 1: []}
    for f in frames:
        pair = f.get("elixir_raw")
        for es in (0, 1):
            v = pair[es] if isinstance(pair, list) and len(pair) == 2 else None
            elixir_by_side[es].append(v if isinstance(v, int) and not isinstance(v, bool) else None)
    has_elixir = any(v is not None for col in elixir_by_side.values() for v in col)
    frame_ticks = [f["tick"] for f in frames]
    regen = battle_regen(frame_ticks, elixir_by_side)
    # a unit deploy's cost leaves the pool on the frame the capture first SHOWS the group: `first_seen`, later
    # than `tick` after a frame gap or where the group was shown late (shown_late_spawn)
    explained = {(d["side"], t) for d in deploys for t in (d["tick"], d["first_seen"])}
    claimed: set[tuple[int, int]] = set()
    unresolved = []

    def cast_drop(side: int, tap_tick: int, cost: int, latest: int | None = None, seen: bool = False) -> int | None:
        # `seen`: the cast's own objects are on the frame, so a unit deploy on that tick does not explain the
        # drop away; only another cast's claim does
        skip = {tk for (s2, tk) in (claimed if seen else explained | claimed) if s2 == side}
        tk = first_cast_drop(frame_ticks, elixir_by_side.get(side) or [], tap_tick, cost, skip, regen)
        if tk is None or (latest is not None and tk > latest):
            return None
        claimed.add((side, tk))
        return tk

    # -- spells: from the effects stream, else from taps with the measured latency
    for cast in spell_casts(frames, rotate):
        cid, fi = cast["card_id"], cast["first_index"]
        name = id_table.get(cid)
        if name is None:
            unknown_ids.add(cid)
            reasons.append(f"spell id {cid} is not in the id table")
            continue
        ax, ay = cast["aim"]  # already in the fixture's frame
        tick = ticks[fi]
        side = cast["side"]
        # A KING-LAUNCHED CAST FIRST SEEN AFTER A FRAME GAP is dated by the steps it has flown (`launch_tick`).
        flown = launch_tick(frames[fi], side, cid) if fi > 0 and ticks[fi] - ticks[fi - 1] > 1 else None
        if flown is not None and ticks[fi - 1] < tick - flown[0] + 1 < tick:
            tick = tick - flown[0] + 1
        else:
            flown = None
        tap_ix = None
        for ix, t in enumerate(taps):
            if ix in used_taps or t["kind"] != "cast" or t["side"] != side or t["id"] != cid:
                continue
            if t["tick"] + TAP_MIN <= tick <= t["tick"] + TAP_MAX:
                tap_ix = ix
                break
        d = {
            "tick": tick,
            "first_seen": ticks[fi],
            "tick_evidence": "first frame of the projectile"
            + (
                f" (frame gap {ticks[fi] - ticks[fi - 1]})"
                if fi > 0 and ticks[fi] - ticks[fi - 1] > 1
                else ""
            )
            + (f"; {flown[1]}" if flown is not None else ""),
            "side": side,
            "card": name,
            "card_id": cid,
            "kind": "spell",
            "level": None,
            "count": 0,
            "keys": [],
            "pos": [ax, ay],
            "source": "effect",
            "aim": cast["aim_rule"],
            "cast_objects": cast["objects"],
            "cast_frames": cast["frames"],
            "cast_last_tick": ticks[cast["last_index"]],
            "objects": cast["tracks"],
            "departures": cast["departures"],
            "first_seen_gap": ticks[fi] - ticks[fi - 1] if fi > 0 else 0,
            "families": sorted(
                (register.get("cards", {}).get(name) or {}).get("families", {}).keys()
            ),
        }
        # AN EFFECT CAST IS BACKED BY ITS CASTER'S ELIXIR. On every corpus cast the cost leaves the pool
        # on the first projectile frame (94 of 98 effect casts, within EFFECT_DROP_SLACK). The other
        # four are two cases: (a) the drop is EARLIER, at the tap -- Lightning is an invisible area whose
        # objects first appear at its strike, 10 ticks after the cast (070448: drop 2982, first object
        # 2992 on the struck target) -- so the cast takes the drop's tick and the tap's point; (b) there is
        # NO drop -- the spell object is a unit's release, not a cast (090204: the Heal Spirit's
        # kamikaze projectile at 1589, card 28000016) -- so it is not played.
        cost = (cards_by_name.get(name) or {}).get("elixir")
        # A UNIT'S RELEASE is not a cast (`released_by_unit`), whatever the elixir did meanwhile.
        released = released_by_unit(frames, fi, side, cid)
        if released:
            unresolved.append(
                {
                    "tick": tick,
                    "side": side,
                    "card": name,
                    "why": f"a {name} object {released}: released by a unit, not cast",
                }
            )
            continue
        # A MIRROR PLAY's objects carry the Mirror's id: it copies its side's last play, for that card's cost plus its
        # own (2026-10 live set: 9 of 9 Mirror objects fall on a drop of the previous card's cost + 1). A copy that is a
        # unit has its own deploy row on the object's frame (061817 t629: a Mega Knight, drop 8): that row is the Mirror
        # play and the object is its own. A copied spell is this row, cast as the card it copies (`mirrored`).
        mirror_of = None
        if ((cards_by_name.get(name) or {}).get("spell") or {}).get("mirror") and isinstance(cost, int):
            lo = ticks[fi - 1] if fi else tick - 1
            shown = {id(x): x["first_seen"] if x.get("first_seen") is not None else x["tick"] for x in deploys}
            mine = sorted((x for x in deploys if x["side"] == side and shown[id(x)] <= lo), key=lambda x: x["tick"])
            if mine and isinstance((cards_by_name.get(mine[-1]["card"]) or {}).get("elixir"), int):
                mirror_of = mine[-1]
                cost += cards_by_name[mirror_of["card"]]["elixir"]
                twin = next(
                    (
                        x for x in deploys
                        if x["side"] == side and x["kind"] not in ("spell", "mirror") and x["card"] == mirror_of["card"]
                        and lo < shown[id(x)] <= ticks[fi]
                    ),
                    None,
                )
                if twin is not None and (
                    not has_elixir
                    or cast_drop(side, lo + 1, cost, ticks[fi] + EFFECT_DROP_SLACK, seen=True) is not None
                ):
                    if tap_ix is not None:
                        used_taps.add(tap_ix)
                    twin["mirrored"] = {"card": twin["card"], "card_id": twin["card_id"]}
                    twin["mirror_evidence"] = (
                        f"a Mirror object on its first frame {ticks[fi]}, and a drop of {cost} (its {twin['card']}"
                        f" + 1): a copy of the side's {mirror_of['card']} at {mirror_of['tick']}"
                    )
                    twin["card"], twin["card_id"], twin["kind"] = name, cid, "mirror"
                    continue
                d["kind"] = "mirror"
                d["mirrored"] = {"card": mirror_of["card"], "card_id": mirror_of["card_id"]}
                d["mirror_evidence"] = (
                    f"copies the side's {mirror_of['card']} at {mirror_of['tick']}, for {cost} elixir"
                )
                d["_mirror_of"] = mirror_of
        played = cards_by_name.get(mirror_of["card"] if mirror_of else name)
        leads = effect_leads(played)
        if has_elixir and isinstance(cost, int):
            if leads == [0]:
                # the slack runs from the first frame that shows the object, which a launch dating (`flown`) may
                # put several ticks after `tick` (080246: a Fireball dated 3513, first seen 3519 after a 7-tick gap)
                drop = cast_drop(side, tick - EFFECT_DROP_SLACK, cost, ticks[fi] + EFFECT_DROP_SLACK, seen=True)
            else:
                # AN EFFECT THAT SHOWS ITS CAST LATE (`effect_leads`): its drop is looked for only where its hits
                # put the cast, on a frame no unit deploy explains (a drop on the first object's frame is another
                # play's: 150503 t3759, a Royal Delivery's box on a Knight's drop, its own 40 ticks before)
                drop = None
                for lead in leads:
                    drop = cast_drop(side, tick - lead - EFFECT_DROP_SLACK, cost, tick - lead + EFFECT_DROP_SLACK)
                    if drop is not None:
                        break
                if drop is not None:
                    d["tick"] = drop
                    d["timing"] = "elixir_drop"
                    d["tick_evidence"] = (
                        f"elixir drop of {cost} on side {side} at {drop}, {tick - drop} ticks before its first object"
                        f" on {tick} (its area effect's hits come {', '.join(map(str, leads))} ticks after the cast)"
                    )
                    ae = ((played or {}).get("spell") or {}).get("area_effect_object") or {}
                    if tap_ix is not None:
                        d["pos"] = list(taps[tap_ix]["native"])
                    elif ae.get("hit_biggest_targets") and cast["tracks"]:
                        # its bolts stand on what they strike, not on the cast point: the mean of every strike's point
                        pts = [t["target"] for t in cast["tracks"]]
                        d["pos"] = [sum(p[0] for p in pts) // len(pts), sum(p[1] for p in pts) // len(pts)]
                        d["source"] = "strike_mean"
                        d["aim"] = f"mean of the {len(pts)} strike points (estimated; no tap)"
            if drop is None and tap_ix is not None:
                drop = cast_drop(side, taps[tap_ix]["tick"], cost, tick)
                if drop is not None:
                    d["tick"] = drop
                    d["pos"] = list(taps[tap_ix]["native"])
                    d["timing"] = "elixir_drop"
                    d["tick_evidence"] = (
                        f"elixir drop of {cost} on side {side} at {drop}; the first object is seen on"
                        f" {tick}, so the cast is played at its tap"
                    )
            if drop is None:
                unresolved.append(
                    {
                        "tick": tick,
                        "side": side,
                        "card": name,
                        "why": f"a {name} object with no elixir drop of its cost ({cost}) on its side where"
                        " its cast would be: not played",
                    }
                )
                continue
        # THE BARBARIAN BARREL'S TICK is its airborne object's creation, which a missed first frame
        # hides: first seen `steps` 360-steps in, it was cast `steps` ticks earlier.
        if name == "BarbLog" and cast["tracks"]:
            launch = cast["tracks"][0]["launch"]
            done = BARBLOG_AIRBORNE_START - round(math.dist(launch, (ax, ay)))
            steps = round(done / BARBLOG_AIRBORNE_STEP)
            if steps > 0 and abs(done - steps * BARBLOG_AIRBORNE_STEP) <= 1:
                d["tick"] = tick - steps
                d["tick_evidence"] = (
                    f"first airborne frame {tick}, its object already {done} in from the start"
                    f" {BARBLOG_AIRBORNE_START} short of the landing: cast {steps} tick(s) earlier"
                )
        if tap_ix is not None:
            used_taps.add(tap_ix)
            latencies.append(tick - taps[tap_ix]["tick"])
            d["tap"] = {
                "tick": taps[tap_ix]["tick"],
                "native": taps[tap_ix]["native"],
            }
        deploys.append(d)
    median_latency = int(statistics.median(latencies)) if latencies else None
    # AN ENTITY-LESS CAST IS LABELLED BY THE CASTER'S ELIXIR, not by tap + latency: a cast's
    # cost leaves the pool on its first effect frame (a Fireball's elixir drops on its first
    # projectile frame; a Rage bottle appears on its elixir drop), and a placement log's tick can
    # be stale (181741: one tick repeated on three lines written seconds apart, a Rage labelled
    # 182 that the elixir puts at 324). The first frame at or after the tap, inside
    # CAST_DROP_WINDOW ticks, on which the caster's elixir falls by the card's cost (less what
    # the frame gap could regenerate), on a frame no matched deploy of that side already
    # explains, each drop claimed once. A capture without elixir keeps tap + median latency.
    explained |= {(d["side"], d["tick"]) for d in deploys}

    for ix, t in enumerate(taps):
        if ix in used_taps or t["kind"] != "cast":
            continue
        if until_tick is not None and t["tick"] > until_tick:
            # past a --until-tick cut: not this fixture's, and not worth an unresolved line
            continue
        deck = decks.get(t["side"])
        if deck and t["id"] is not None and t["id"] not in deck:
            # 005517: a side-1 "Rage" from a placements line whose player's deck has no Rage.
            unresolved.append(
                {
                    "tick": t["tick"],
                    "side": t["side"],
                    "card": t["card"],
                    "why": "the card is not in the caster's recorded deck",
                }
            )
            continue
        if t["id"] is None or (median_latency is None and not has_elixir) or not t["native"]:
            unresolved.append(
                {
                    "tick": t["tick"],
                    "side": t["side"],
                    "card": t["card"],
                    "why": "no projectile in the effects stream and no latency measurement on"
                    " this capture"
                    if median_latency is None
                    else "no projectile in the effects stream and no id / position",
                }
            )
            continue
        if has_elixir and frame_ticks and not frame_ticks[0] <= t["tick"] <= frame_ticks[-1]:
            # a placements log covers the whole battle; a capture split into parts (.b1, .b2)
            # covers one stretch of it, and a tap outside that stretch is not this fixture's
            unresolved.append(
                {
                    "tick": t["tick"],
                    "side": t["side"],
                    "card": t["card"],
                    "why": "the tap is outside this capture's frames",
                }
            )
            continue
        name = id_table.get(t["id"])
        if has_elixir:
            cost = (cards_by_name.get(name) or {}).get("elixir")
            est = cast_drop(t["side"], t["tick"], cost) if isinstance(cost, int) else None
            if est is None:
                unresolved.append(
                    {
                        "tick": t["tick"],
                        "side": t["side"],
                        "card": t["card"],
                        "why": f"no elixir drop of its cost ({cost}) on the caster's side within"
                        f" {CAST_DROP_WINDOW} ticks of the tap",
                    }
                )
                continue
            evidence = f"elixir drop of {cost} on side {t['side']} at {est} (tap tick {t['tick']})"
            timing = "elixir_drop"
        else:
            est = t["tick"] + median_latency
            evidence, timing = f"tap tick {t['tick']} + median latency {median_latency}", "estimated"
        if until_tick is not None and est > until_tick:
            continue
        deploys.append(
            {
                "tick": est,
                "first_seen": None,
                "tick_evidence": evidence,
                "side": t["side"],
                "card": name,
                "card_id": t["id"],
                "kind": "spell",
                "level": None,
                "count": 0,
                "keys": [],
                "pos": list(t["native"]),
                "source": "tap_tile",
                "timing": timing,
                "tap": {
                    "tick": t["tick"],
                    "native": t["native"],
                },
                "families": sorted(
                    (register.get("cards", {}).get(name) or {}).get("families", {}).keys()
                ),
            }
        )
    # a scheduled spell's casts read off its spawns (module doc, A SCHEDULED SPELL'S CAST)
    deploys.extend(schedule_casts(spawned_groups, ents, ticks, cards_by_name, deploys))
    deploys.sort(key=lambda d: (d["tick"], d["side"], d["card_id"]))
    mirror_plays(deploys, decks, id_table)
    # spells at the level of the side's units (a cast carries no level in the captures)
    for s in (0, 1):
        lv = [d["level"] for d in deploys if d["side"] == s and d["level"] is not None]
        side_level = Counter(lv).most_common(1)[0][0] if lv else tower_level.get(s)
        for d in deploys:
            if d["side"] == s and d["level"] is None:
                d["level"] = side_level
                d["level_source"] = "side mode"
    spell_levels_from_damage(deploys, doc, cards_by_name, ents, per_tick_rows, ticks)
    spell_levels_from_spawn(deploys, spawned_groups, ents)
    for d in deploys:
        src = d.pop("_mirror_of", None)
        if src is not None and src.get("level") is not None:
            d["level"], d["level_source"] = src["level"] + 1, "mirrored row + 1"
    tunnel_destinations(deploys, ents, per_tick_rows, cards_by_name)
    tunnel_spawn_ticks(deploys, ents, ticks, towers, cards_by_name)
    # the battle's other seat dates the rows its own frames pin (module doc, PAIR DATING); after every other tick
    # rule, and before the decks read the play order
    if partners and pair_date(deploys, ticks, partners):
        deploys.sort(key=lambda d: (d["tick"], d["side"], d["card_id"]))

    # -- decks and levels per side
    script_decks = {}
    card_levels = {}
    for s in (0, 1):
        used = []
        for d in deploys:
            if d["side"] == s and d["card"] not in used:
                used.append(d["card"])
        recorded = [id_table.get(cid) for cid in decks.get(s, [])]
        recorded = [n for n in recorded if n]
        padding = [n for n in recorded if n not in used]
        script_decks[str(s)] = {"deploy_order": used, "recorded": recorded, "padding": padding}
        lv = Counter()
        per_card: dict[str, Counter] = defaultdict(Counter)
        for d in deploys:
            if d["side"] == s and d["level"] is not None and d["kind"] != "spell":
                lv[d["level"]] += d["count"]
                per_card[d["card"]][d["level"]] += d["count"]
        mode = lv.most_common(1)[0][0] if lv else tower_level.get(s)
        card_levels[str(s)] = {
            "mode": mode,
            "per_card": {c: cnt.most_common(1)[0][0] for c, cnt in sorted(per_card.items())},
        }

    # -- ability presses (module doc, AN ABILITY PRESS)
    if has_elixir:
        presses = ability_presses(
            deploys, claimed, ents, per_tick_rows, ticks, elixir_by_side, doc, cards_by_name, name_to_id
        )
        if presses:
            deploys.extend(presses)
            deploys.sort(key=lambda d: (d["tick"], d["side"], d["card_id"]))

    # -- engine loadability (the census, when present)
    if census is not None:
        loadable = set(census.get("loadable", []))
        rejected = census.get("rejected", {})
        for name in sorted({d["card"] for d in deploys if d["card"]}):
            if name not in loadable:
                reasons.append(f"{name}: {rejected.get(name, 'not in the engine card set')}")
        have = census.get("cards_json_fnv1a64")
        if have != doc.get("_fnv1a64"):
            fx["census"] = (
                f"STALE: built against cards.json {have}, this run reads {doc.get('_fnv1a64')}"
                " (cargo run --example replay_parity -- --census)"
            )
    else:
        fx["census"] = (
            "absent: engine loadability not checked here"
            " (cargo run --example replay_parity -- --census)"
        )

    # -- truth: RLE per entity column over its contiguous frame run
    sel = list(range(0, len(frames), max(stride, 1)))
    sel_set = set(sel)
    truth_ticks = [ticks[i] for i in sel]
    truth_entities = []
    for e in sorted(ents.values(), key=lambda e: e["key"]):
        idx = [i for i in range(e["first_index"], e["last_index"] + 1) if i in sel_set]
        if not idx:
            continue
        cols: list[list] = [[] for _ in TRUTH_COLUMNS]
        gaps = 0
        for i in idx:
            row = per_tick_rows[i].get(e["key"])
            if row is None:
                gaps += 1
                row = (None,) * len(TRUTH_COLUMNS)
            for c, v in enumerate(row):
                cols[c].append(v)
        rec = {
            "key": e["key"],
            "side": e["side"],
            "card_id": e["card_id"],
            "card": e.get("card")
            if e["card_id"] >= 0
            else (
                "KingTower"
                if any(t["key"] == e["key"] and t["slot"] == 0 for t in towers)
                else "PrincessTower"
            ),
            "role": e["role"],
            "unit": e.get("unit"),
            "level": e["level"],
            "max_hp": e["max_hp"],
            "t0": sel.index(idx[0]),
            "n": len(idx),
            "absent_frames": gaps,
        }
        rec.update({name: rle(cols[c]) for c, name in enumerate(TRUTH_COLUMNS)})
        truth_entities.append(rec)
    truth = {
        "columns": list(TRUTH_COLUMNS),
        "encoding": "per entity, each column run-length encoded as [value, run, ...]"
        " over its frames from index t0 for n frames of `ticks`; a null value is a"
        " frame the entity was absent from inside its run; alive = present. `elixir_raw`,"
        " when present, is per side the same encoding over all of `ticks` (10000 = one elixir)",
        "ticks": truth_ticks,
        "entities": truth_entities,
    }
    elixir = elixir_columns([frames[i] for i in sel])
    if elixir is not None:
        truth["elixir_raw"] = elixir

    fx.update(
        {
            "playable": not reasons,
            "unplayable_reasons": reasons,
            "local_side_native": (header or {}).get("local_side_native"),
            "ticks": {"first": ticks[0], "last": ticks[-1], "frames": len(frames)},
            "tap_latency_ticks": {"median": median_latency, "samples": sorted(latencies)},
            "towers": [
                {k: t[k] for k in ("slot", "side", "x", "y", "hp", "max_hp", "level")}
                for t in towers
            ],
            "tower_level": {str(s): tower_level[s] for s in (0, 1)},
            "tower_troops": tower_troops(towers, doc),
            "card_levels": card_levels,
            "decks": script_decks,
            "forms_read": FORMS_READ if form_rows is not None else {},
            "deploys": deploys,
            "spawned_groups": spawned_groups,
            "unresolved": unresolved,
            "truth": truth,
        }
    )
    return fx


def fixture_text(fx: dict) -> str:
    return json.dumps(fx, separators=(",", ":")) + "\n"


def comparable(fx: dict) -> dict:
    """The fixture without the fields that describe the RUN rather than the battle."""
    return {k: v for k, v in fx.items() if k != "census"}


def write_fixture(fx: dict, out_dir: str, written: dict[str, str] | None = None) -> str:
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, fx["capture"] + ".replay.json")
    # Two captures on one path would leave one battle's fixture holding the other's
    # content and list the same file twice in the manifest.
    if written is not None and path in written:
        raise SystemExit(f"{path} was already written from {written[path]}; this run would overwrite it")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(fixture_text(fx))
    if written is not None:
        written[path] = fx["capture"]
    return path


def load_census(out_dir: str) -> dict | None:
    """The engine's census from --out, else from the default output dir."""
    for d in (out_dir, OUT_DEFAULT):
        p = os.path.join(d, CENSUS)
        if os.path.exists(p):
            with open(p, encoding="utf-8") as fh:
                return json.load(fh)
    return None


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument(
        "capture",
        nargs="?",
        help="a *.native.oracle.jsonl.gz capture, or its fixture name (20260920-003751-B)"
        " looked up in --reports",
    )
    ap.add_argument(
        "--placements",
        nargs="*",
        default=None,
        help="placement logs (default: the same-stamp files beside the capture)",
    )
    ap.add_argument("--out", default=OUT_DEFAULT)
    ap.add_argument(
        "--table",
        choices=sorted(TABLES),
        default="15.535.29",
        help="the card table (calibration cards.CARD_TABLE): its card file, id pack and census together",
    )
    ap.add_argument("--truth-stride", type=int, default=1)
    ap.add_argument(
        "--until-tick",
        type=int,
        default=None,
        help="drop frames after this tick (a trimmed sample)",
    )
    ap.add_argument(
        "--all", action="store_true", help="every capture in --reports, plus the manifest"
    )
    ap.add_argument(
        "--reports", default=LIVE, help="the captures folder (default: ROYALELIVE_REPORTS)"
    )
    ap.add_argument(
        "--check",
        metavar="FIXTURE",
        default=None,
        help="compare with this existing fixture instead of writing (exit 1 if it differs)",
    )
    args = ap.parse_args()
    select_table(args.table)
    if not args.capture and not args.all:
        ap.error("a capture or --all")
    if args.check and args.all:
        ap.error("--check takes one capture, not --all")
    if args.all and not args.reports:
        ap.error("--all needs --reports or ROYALELIVE_REPORTS set to the captures folder")
    if args.capture and not os.path.isfile(args.capture):
        found = capture_named(args.capture, args.reports)
        if found is None:
            ap.error(
                f"{args.capture} is neither a capture file nor the name of one in"
                f" {args.reports or '--reports (unset, and ROYALELIVE_REPORTS is not set)'}"
            )
        args.capture = found
    with open(CARDS, "rb") as fh:
        raw = fh.read()
    doc = json.loads(raw.decode("utf-8"))
    doc["_fnv1a64"] = fnv1a64(raw)
    if os.path.exists(REGISTER):
        with open(REGISTER, encoding="utf-8") as fh:
            register = json.load(fh)
    else:
        register = {}
        print(
            f"warning: {REGISTER} missing (python tools/mechanic_register.py):"
            " no mechanic families",
            file=sys.stderr,
        )
    id_table = load_id_table()
    card_names = {c["name"] for c in doc["cards"]}
    # the base class id per card name (a hero-form id, class 203, names the same card)
    name_to_id = base_ids(id_table, card_names)
    census = load_census(args.out)
    nominal = load_nominal_offsets()
    captures = (
        [args.capture]
        if args.capture
        else folder_captures(args.reports, CAPTURE_SUFFIX)
    )
    jobs = [
        (
            cap,
            args.placements
            if args.placements is not None
            else placement_files_for(cap, os.path.dirname(os.path.abspath(cap))),
        )
        for cap in captures
    ]
    # the other seat's captures of each job's battle, from the job's own folder (PAIR DATING)
    folders: dict[str, list[str]] = {}
    partners_of: dict[str, list[str]] = {}
    for cap, _ in jobs:
        folder = os.path.dirname(os.path.abspath(cap))
        if folder not in folders:
            folders[folder] = folder_captures(folder, CAPTURE_SUFFIX)
        partners_of[cap] = battle_partners(cap, folders[folder])
    # ONE seat map for the run, the captures folder's (tools/capture_names.py folder_seats),
    # so a capture's letter is the same whether it is built alone, with --all, or by another
    # maker. The jobs' own names go in too, in case a capture was handed in from elsewhere.
    pool = [c for c, _ in jobs] + [p for _, ps in jobs for p in ps] + [p for ps in partners_of.values() for p in ps]
    seats = folder_seats(args.reports, CAPTURE_SUFFIX, pool)
    offers: dict[str, list[dict]] = {}

    def offer_of(partner: str) -> list[dict]:
        # a partner's rows as it dates them ALONE (no pair dating of its own), from its whole capture and its own
        # logs, at stride 1 so that its truth ticks are its frames; once per capture per run
        if partner not in offers:
            pfx = build(
                partner,
                placement_files_for(partner, os.path.dirname(os.path.abspath(partner))),
                1,
                None,
                census,
                id_table,
                doc,
                register,
                name_to_id,
                card_names,
                seats,
                nominal,
            )
            offers[partner] = seat_offer(pfx.get("deploys", []), pfx["truth"]["ticks"] if "truth" in pfx else [])
        return offers[partner]

    manifest = []
    written: dict[str, str] = {}
    for cap, placements in jobs:
        fx = build(
            cap,
            placements,
            args.truth_stride,
            args.until_tick,
            census,
            id_table,
            doc,
            register,
            name_to_id,
            card_names,
            seats,
            nominal,
            partners=[(public_name(p, seats), offer_of(p)) for p in partners_of[cap]],
        )
        if args.check:
            with open(args.check, encoding="utf-8") as fh:
                have = json.load(fh)
            if comparable(have) != comparable(fx):
                print(f"STALE: {args.check} differs from what this capture builds")
                return 1
            print(f"{args.check} is current")
            return 0
        path = write_fixture(fx, args.out, written)
        size = os.path.getsize(path)
        row = {
            "capture": fx["capture"],
            "fixture": os.path.basename(path),
            "playable": fx["playable"],
            "reasons": fx["unplayable_reasons"],
            "deploys": len(fx.get("deploys", [])),
            "spawned_groups": len(fx.get("spawned_groups", [])),
            "ticks": fx.get("ticks"),
            "frames_duplicate": fx["frames_duplicate"],
            "bytes": size,
        }
        manifest.append(row)
        status = (
            "playable" if fx["playable"] else "UNPLAYABLE " + "; ".join(fx["unplayable_reasons"])
        )
        print(
            f"{row['fixture']}: {row['deploys']} deploys, {row['spawned_groups']} spawned groups,"
            f" {size // 1024} KB -- {status}"
        )
    if args.all:
        mpath = os.path.join(args.out, "manifest.json")
        census_state = "absent"
        if census is not None:
            census_state = (
                "present"
                if census.get("cards_json_fnv1a64") == doc["_fnv1a64"]
                else f"STALE (built against cards.json {census.get('cards_json_fnv1a64')})"
            )
        with open(mpath, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(
                {
                    "generated_by": "tools/make_replay_fixture.py --all",
                    "cards_json_fnv1a64": doc["_fnv1a64"],
                    "census": census_state,
                    "run_captures": [os.path.basename(c) for c, _ in jobs],
                    "playable": sum(1 for r in manifest if r["playable"]),
                    "unplayable": sum(1 for r in manifest if not r["playable"]),
                    "captures": manifest,
                },
                fh,
                indent=1,
            )
            fh.write("\n")
        print(
            f"manifest: {mpath}"
            f" ({sum(1 for r in manifest if r['playable'])} playable / {len(manifest)})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
