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
    it (CREATION POINT below). Without a tap the centroid is used, except where
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

CREATION POINT
    A single unit is played where it appeared, and its first frame's x, y already carry
    that tick's contact push: a Knight deployed onto a unit stands up to 150 off the point
    it was created on. So a single unit the capture shows deploying (behavior_state 4 or
    11) on its creation tick is played at its creation point, the first frame's x2, y2,
    when the two differ (`source` `creation_point`). Its creation tick is its first frame
    when `first_seen` = `tick` and either no frame was missed before it
    (`first_seen_gap` 1) or its deploy end pins the spawn there (`tick_evidence` exact).
    Otherwise the unit keeps its first frame: its x2, y2 may be a tick after its creation,
    already pushed. A unit that travels under ground keeps its row (it is played at its
    `destination`).

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
    entities already on the board; a capture with no frames.

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
from capture_names import SEAT_TAG, distinct_captures, folder_seats  # noqa: E402

LIVE = os.environ.get("ROYALELIVE_REPORTS")
RAW = os.path.join(ROOT, "data", "raw", "cr-15.535.29", "csv_logic")
CARDS = os.path.join(ROOT, "data", "derived", "cards.json")
REGISTER = os.path.join(ROOT, "data", "derived", "mechanic_register.json")
#: The committed measurement of the client's summon formations (tools/make_formation_fixture.py; tests/formations.rs
#: holds the engine to it). This tool reads it only for the nominal offsets of a recovered tile (RECOVERED TILE).
FORMATIONS = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "formations", "measured.json")
OUT_DEFAULT = os.path.join(ROOT, "data", "derived", "replay")
CENSUS = "card_census.json"

FORMAT = "replay-fixture-1"
#: The recording's five-digit tag at the end of a capture or placement-log file name
#: (`...-NNNNN.jsonl`): one tag per seat of a battle, the same on that seat's capture and log.
SEAT_FILE_TAG = re.compile(r"-(\d{5})(?=\.)")
#: One elixir in the capture's `elixir_raw` units (module doc, ELIXIR).
ELIXIR_UNIT = 10_000
#: The most elixir a side regenerates in one tick (triple elixir: one elixir per ~0.93 s), so a
#: drop measured across a frame gap is still recognised as a card's cost.
ELIXIR_REGEN_MAX_PER_TICK = 540
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
#: A measured formation group gives nominal offsets only when its members' mean offset from its tap is within this on
#: both axes. That leaves out a group laid a tile away from its logged tap (a tap the game moved off a tower) and a
#: group with a clamped member: neither is a formation around its logged tile.
NOMINAL_CENTRED_NATIVE = 60
#: How far a creation point minus a nominal offset may sit from a tile centre, and a member from a nominal offset,
#: native per axis. The measured offsets are first-frame positions after the first tick's contact push, so two siblings
#: that overlap at creation stand a few units off their ring (the Minions' 577 ring, 2 apart).
RECOVER_TOLERANCE = 3
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
#: The Barbarian Barrel's airborne object (BarbLogProjectile): created MinDistance 3000 short of
#: the landing on the caster's forward axis, moving 360 a tick. Where a capture missed its first
#: frame it is first seen a step or more in, and the cast tick is that many steps earlier.
BARBLOG_AIRBORNE_START, BARBLOG_AIRBORNE_STEP = 3000, 360
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
    paths = distinct_captures(glob.glob(os.path.join(reports, "*" + CAPTURE_SUFFIX)))
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
    object for) -- truth only, never a deploy."""
    if card is None:
        return None, True, "no_card"
    own = own_units(card)
    best = None
    for unit, base in reachable_units(doc, card).items():
        pct = ladder_percent(doc, card, unit, level)
        if pct is None:
            continue
        hp = base * pct // 100
        if hp == max_hp:
            return unit, unit in own, "exact"
        err = abs(hp - max_hp) * 100 // max(max_hp, 1)
        if best is None or err < best[0]:
            best = (err, unit, hp)
    if best is None:
        return None, True, "no_object"
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


def first_cast_drop(frame_ticks: list, elixir: list, tap_tick: int, cost: int, skip: set) -> int | None:
    """The first frame tick at or after `tap_tick`, inside CAST_DROP_WINDOW ticks, on which one
    side's `elixir` (per frame, aligned with `frame_ticks`; None where the capture has none) falls
    by `cost` elixir -- within what the frame gap could regenerate -- on a tick not in `skip`
    (frames a matched deploy already explains, drops an earlier cast claimed); None if none."""
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
        if abs((a - b) - cost * ELIXIR_UNIT) <= ELIXIR_REGEN_MAX_PER_TICK * gap + ELIXIR_UNIT // 10:
            return tk
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


#: How far past a damaging spell's radius a victim's centre may stand and still be read as hit (a unit's own radius).
SPELL_VICTIM_MARGIN = 1500
#: How many frames after a spell row's tick its first hit is looked for.
SPELL_HIT_WINDOW = 120
#: How much more than its damage a victim may lose on the hit frame (a building's decay tick, a concurrent chip).
SPELL_HIT_SLACK = 2


def spell_damage_at(doc: dict, card: dict, level: int) -> int | None:
    """A damaging spell's hit on a troop or a building at a unified level, scaled as the engine scales it (card.rs
    `level_multiplier`: the ladder entered at level_scaling.base_level, truncating division). None for a card with no
    damage, and for a level its ladder does not have."""
    base = card.get("damage")
    ls = card.get("level_scaling")
    if not base or not ls:
        return None
    first = ls.get("base_level", doc["rarities"][ls["rarity"]]["relative_level"] + 1)
    table = ls["multiplier_percent_by_level"]
    step = level - first
    return base * table[step] // 100 if 0 <= step < len(table) else None


def spell_levels_from_damage(
    deploys: list[dict], doc: dict, cards_by_name: dict, ents: dict, per_tick_rows: list, ticks: list
) -> None:
    """A damaging spell's LEVEL, read off what its first hit took. A capture records no level for a cast, so a spell
    row takes its side's mode (`level_source` "side mode"). That is wrong where a side levels its spells apart from its
    troops: 20260918-112751's side-0 Fireball of tick 583 took 357 from a Goblin Hut on 607 (358 with the hut's decay
    tick), its ladder's level 4, where the side mode 3 plays 325. The spell's HIT is the first frame, from its row's
    tick on, on which an enemy that is not a tower (card id >= 0) and stood within the spell's radius +
    SPELL_VICTIM_MARGIN of its point loses at least the spell's least damage and lives. A level fits when every such
    drop is its damage at that level up to SPELL_HIT_SLACK more. One fitting level replaces the side mode
    (`level_source` "damage"); several are settled by the one nearest the side mode; none keep the side mode. The drops
    are in `level_evidence` either way."""
    ix, iy, ihp = (TRUTH_COLUMNS.index(c) for c in ("x", "y", "hp"))
    index_of = {t: i for i, t in enumerate(ticks)}
    for d in deploys:
        if d["kind"] != "spell" or d.get("level_source") != "side mode" or d.get("level") is None:
            continue
        card = cards_by_name.get(d["card"]) or {}
        if spell_damage_at(doc, card, d["level"]) is None:
            continue
        reach = (card.get("area_damage_radius_milli") or (card.get("projectile") or {}).get("radius_milli") or 0)
        reach += SPELL_VICTIM_MARGIN
        i0 = index_of.get(d["tick"])
        if i0 is None:
            continue
        foes = [e for e in ents.values() if e["side"] != d["side"] and e["card_id"] >= 0]
        # a drop below the spell's least damage is not its hit (a building's decay tick, a troop's chip)
        least = min(x for lv in range(1, 17) if (x := spell_damage_at(doc, card, lv)) is not None)
        hit = None
        for i in range(max(i0, 1), min(i0 + SPELL_HIT_WINDOW, len(per_tick_rows))):
            drops = []
            for e in foes:
                a, b = per_tick_rows[i - 1].get(e["key"]), per_tick_rows[i].get(e["key"])
                if a is None or b is None or a[ihp] - b[ihp] < least or b[ihp] <= 0:
                    continue
                if math.dist((a[ix], a[iy]), d["pos"]) <= reach:
                    drops.append(a[ihp] - b[ihp])
            if drops:
                hit = (ticks[i], sorted(drops))
                break
        if hit is None:
            d["level_evidence"] = "no enemy within its reach lost hp inside the window"
            continue
        fits = [
            lv for lv in range(1, 17)
            if (dmg := spell_damage_at(doc, card, lv)) is not None
            and all(dmg <= x <= dmg + SPELL_HIT_SLACK for x in hit[1])
        ]
        d["level_evidence"] = f"hit on {hit[0]}: drops {hit[1]}; fitting levels {fits}"
        if fits:
            best = min(fits, key=lambda lv: (abs(lv - d["level"]), lv))
            if best != d["level"]:
                d["level"] = best
                d["level_source"] = "damage"


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
# the fixture


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
) -> dict:
    # the nominal offsets of a recovered tile (RECOVERED TILE): the committed measurement unless the caller hands some
    if nominal is None:
        nominal = load_nominal_offsets()
    # the evolved and hero rows (A DEPLOY'S FORM); without the 15.535 pack no deploy's form is read
    if form_rows is None and not missing_id_files():
        form_rows = load_form_rows()
    header, raw_frames = read_capture(capture)
    frames, dup, back = dedupe(raw_frames)
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
    if not frames:
        fx["playable"] = False
        fx["unplayable_reasons"] = ["no frames"]
        return fx

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
            **(deploy_form(cid, form_rows, name) if form_rows is not None else {}),
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
    # a unit deploy's cost leaves the pool on the frame the capture first SHOWS the group: `first_seen`, later
    # than `tick` after a frame gap or where the group was shown late (shown_late_spawn)
    explained = {(d["side"], t) for d in deploys for t in (d["tick"], d["first_seen"])}
    claimed: set[tuple[int, int]] = set()
    unresolved = []

    def cast_drop(side: int, tap_tick: int, cost: int, latest: int | None = None, seen: bool = False) -> int | None:
        # `seen`: the cast's own objects are on the frame, so a unit deploy on that tick does not explain the
        # drop away; only another cast's claim does
        skip = {tk for (s2, tk) in (claimed if seen else explained | claimed) if s2 == side}
        tk = first_cast_drop(frame_ticks, elixir_by_side.get(side) or [], tap_tick, cost, skip)
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
        tap_ix = None
        for ix, t in enumerate(taps):
            if ix in used_taps or t["kind"] != "cast" or t["side"] != side or t["id"] != cid:
                continue
            if t["tick"] + TAP_MIN <= tick <= t["tick"] + TAP_MAX:
                tap_ix = ix
                break
        d = {
            "tick": tick,
            "first_seen": tick,
            "tick_evidence": "first frame of the projectile"
            + (
                f" (frame gap {ticks[fi] - ticks[fi - 1]})"
                if fi > 0 and ticks[fi] - ticks[fi - 1] > 1
                else ""
            ),
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
        if has_elixir and isinstance(cost, int):
            drop = cast_drop(side, tick - EFFECT_DROP_SLACK, cost, tick + EFFECT_DROP_SLACK, seen=True)
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
                        "why": f"a {name} object with no elixir drop of its cost ({cost}) on its side:"
                        " released by a unit, not cast",
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

    tunnel_destinations(deploys, ents, per_tick_rows, cards_by_name)
    tunnel_spawn_ticks(deploys, ents, ticks, towers, cards_by_name)

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
        else distinct_captures(glob.glob(os.path.join(args.reports, "*" + CAPTURE_SUFFIX)))
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
    # ONE seat map for the run, the captures folder's (tools/capture_names.py folder_seats),
    # so a capture's letter is the same whether it is built alone, with --all, or by another
    # maker. The jobs' own names go in too, in case a capture was handed in from elsewhere.
    pool = [c for c, _ in jobs] + [p for _, ps in jobs for p in ps]
    seats = folder_seats(args.reports, CAPTURE_SUFFIX, pool)
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
