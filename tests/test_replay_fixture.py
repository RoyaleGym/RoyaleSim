"""tools/make_replay_fixture.py: the pure pieces (spawn-tick recovery, the RLE truth
encoding, the deploy-versus-spawn classification, the spell-cast grouping and its
per-object records, the attack-timer and elixir columns, the cards.json hash) on
hand-built inputs, a hand-built battle built both ways up, the committed sample's
script against what its capture is documented to hold, and the maker's own --check that
the sample is still what its capture builds (skips without ROYALELIVE_REPORTS)."""

from __future__ import annotations

import gzip
import importlib.util
import json
import os
import subprocess
import sys

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAMPLE = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "replay", "sample.json")
CARDS = os.path.join(ROOT, "data", "derived", "cards.json")
# SKIPS, LOUDLY, when data/derived/cards.json is absent: it is generated, not tracked, and a
# bare FileNotFoundError would name the missing file instead of the command that makes it.
# A skip here is not a pass.
needs_cards = pytest.mark.skipif(
    not os.path.exists(CARDS),
    reason=f"{CARDS} is absent; run python tools/extract_cards.py --vintage 2018 --out {CARDS} first"
    " -- a skip here is not a pass",
)


def _cards_version():
    """The vintage stamp of whatever cards.json this workspace has, or None."""
    try:
        with open(CARDS, encoding="utf-8") as fh:
            return json.load(fh).get("version")
    except OSError:
        return None


# THE GUARD ABOVE WATCHES PRESENCE, AND PRESENCE WAS THE WRONG QUANTITY. A clone HAS a
# cards.json -- the 2018 one the README's install writes -- so the presence guard passed,
# the tests below ran against a table they were never made against, and the suite failed
# on every clone while passing in a workspace carrying the modern asset pack. Worse, the
# reason string above tells the reader to run the very command that produces that state.
#
# The committed sample records the hash of the table it was classified against, so these
# tests want a VINTAGE and not a file. They say which, and say it is not a pass.
MODERN_CARDS = "cards-15535.1"
needs_modern_cards = pytest.mark.skipif(
    _cards_version() != MODERN_CARDS,
    reason=f"this fixture was made against the {MODERN_CARDS} card table and "
    f"{CARDS} here is {_cards_version()!r} (a clone gets the 2018 build from the README's "
    "install, which carries neither the cards nor the columns these assertions name)"
    " -- a skip here is not a pass",
)


def _load():
    spec = importlib.util.spec_from_file_location(
        "make_replay_fixture", os.path.join(ROOT, "tools", "make_replay_fixture.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


@pytest.fixture(scope="module")
def m():
    return _load()


def test_rle_round_trips(m):
    assert m.rle([]) == []
    assert m.rle([5]) == [5, 1]
    assert m.rle([5, 5, 5, 7, None, None, 7]) == [5, 3, 7, 1, None, 2, 7, 1]


def test_a_tunnelling_deploy_publishes_where_it_surfaces(m):
    """`tunnel_destinations`, measured on the client 16.402 corpus: a Miner is ONE entity that turns state 4 at its
    destination when its tunnel (state 6) ends; a Goblin Drill's tunnel unit vanishes and its building appears at
    the destination on the next frame. A tunnel inside a frame gap leaves the unit first seen already surfaced."""
    ix, iy, ist = (m.TRUTH_COLUMNS.index(c) for c in ("x", "y", "state"))

    def row(x, y, state):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[ix], r[iy], r[ist] = x, y, state
        return tuple(r)

    cards = {"Miner": {"spawn_pathfind": {"speed": 650, "morph": None}},
             "GoblinDrill": {"spawn_pathfind": {"speed": 300, "morph": "GoblinDrill"}}}
    ents = {
        "miner": {"key": 5, "side": 0, "first_index": 0, "last_index": 3},
        "dig": {"key": 7, "side": 0, "first_index": 0, "last_index": 2},
        "building": {"key": 8, "side": 0, "first_index": 3, "last_index": 3},
        "enemy": {"key": 9, "side": 1, "first_index": 3, "last_index": 3},
        "gap": {"key": 10, "side": 0, "first_index": 1, "last_index": 3},
    }
    rows = [
        {5: row(9235, 1777, 6), 7: row(9178, 3569, 6)},
        {5: row(8800, 1200, 6), 7: row(8000, 4000, 6), 10: row(8500, 500, 4)},
        {5: row(3500, 1500, 4), 7: row(3150, 23150, 6), 10: row(8500, 500, 4)},
        {5: row(3500, 1500, 4), 8: row(3000, 23000, 4), 9: row(3100, 23100, 4), 10: row(8500, 500, 4)},
    ]
    deploys = [{"card": "Miner", "keys": [5]}, {"card": "GoblinDrill", "keys": [7]}, {"card": "Miner", "keys": [10]},
               {"card": "Knight", "keys": [9]}]
    m.tunnel_destinations(deploys, ents, rows, cards)
    assert deploys[0]["destination"] == [3500, 1500], deploys[0]
    assert deploys[1]["destination"] == [3000, 23000], deploys[1]  # the own building, not the enemy nearer to it
    assert deploys[2]["destination"] == [8500, 500], deploys[2]
    assert "destination" not in deploys[3]


def test_a_mirror_play_is_published_as_a_mirror(m):
    """`mirror_plays`, measured on the client 16.402 corpus (a level-12 ElixirGolem right after the side's level-11
    one, in a deck holding Mirror): the copy's row becomes the Mirror play. Controls: a deck without Mirror, and a
    repeat at the card's own level, stay as they are."""
    ids = {28000006: "Mirror", 26000067: "ElixirGolem", 26000000: "Knight"}

    def play(tick, card, cid, level):
        return {"tick": tick, "side": 0, "card": card, "card_id": cid, "kind": "troop", "level": level, "keys": [tick]}

    deploys = [play(779, "ElixirGolem", 26000067, 11), play(805, "ElixirGolem", 26000067, 12),
               play(900, "Knight", 26000000, 11), play(950, "Knight", 26000000, 11)]
    m.mirror_plays(deploys, {0: [28000006, 26000067, 26000000]}, ids)
    assert [d["card"] for d in deploys] == ["ElixirGolem", "Mirror", "Knight", "Knight"]
    assert deploys[1]["mirrored"] == {"card": "ElixirGolem", "card_id": 26000067}
    assert (deploys[1]["kind"], deploys[1]["level"], deploys[1]["keys"]) == ("mirror", 12, [805])
    plain = [play(779, "ElixirGolem", 26000067, 11), play(805, "ElixirGolem", 26000067, 12)]
    m.mirror_plays(plain, {0: [26000067]}, ids)
    assert [d["card"] for d in plain] == ["ElixirGolem", "ElixirGolem"]


def test_spawn_tick_is_exact_when_the_spawn_frame_was_seen(m):
    ticks = [170, 171, 172, 173]
    # first seen at index 2 (tick 172), previous frame 171: no gap -> exact
    assert m.refine_spawn_tick(ticks, 2, [(2, 4), (3, 4)], 1000) == (172, 172, "exact")


def test_spawn_tick_is_recovered_from_the_deploy_end_transition(m):
    # the sample's Prince: frames ..., 171, 173, ... (172 missed), deploying until the
    # frame at 190 and walking-state at 191: spawn + 19 = 191 -> spawn 172
    ticks = [171, 173, *range(174, 200)]
    states = [(1, 4)] + [(k, 4) for k in range(2, 19)] + [(19, 1)]
    assert ticks[19] == 191
    assert ticks[18] == 190
    tick, first_seen, why = m.refine_spawn_tick(ticks, 1, states, 1000)
    assert (tick, first_seen) == (172, 173)
    assert why.startswith("exact")


def test_spawn_tick_keeps_a_range_when_both_frames_were_missed(m):
    # frames 170, 173 (171, 172 missed) and the transition seen across a 2-tick gap
    ticks = [170, 173, 174, 175, 176, 190, 192]
    states = [(1, 4), (2, 4), (3, 4), (4, 4), (5, 4), (6, 1)]
    tick, first_seen, why = m.refine_spawn_tick(ticks, 1, states, 1000)
    # transition in (190, 192] -> spawn in [172, 173]; frame gap -> [171, 173]; the earliest is taken: on the 16.402
    # corpus 12 of 12 range rows the other seat pins were created on the range's first tick
    assert (tick, first_seen) == (172, 173)
    assert why.startswith("range [172, 173]")
    assert why.endswith("earliest used")


def test_spawn_tick_is_pinned_by_the_first_step_inside_a_transition_range(m):
    # the sample's Battle Ram: frames ... 935, 937 (936 missed), deploying through 954,
    # walking at 956 (955 missed): the transition puts the spawn in [936, 937]; the
    # unit is ONE step (59 native) off its point at 956 and two more steps (116) by
    # 958, so its first step was at 956 = spawn + 20 -> 936, not the latest 937
    ticks = [935, 937, *range(939, 955), 956, 958, 960]
    fi0 = ticks.index(937)
    states = [(fi0 + k, 4) for k in range(ticks.index(954) - fi0 + 1)] + [(ticks.index(956), 1)]
    positions = [(fi, 3499, 8500) for fi in range(fi0, ticks.index(954) + 1)] + [
        (ticks.index(956), 3481, 8556),
        (ticks.index(958), 3459, 8672),
        (ticks.index(960), 3437, 8788),
    ]
    tick, first_seen, why = m.refine_spawn_tick(ticks, fi0, states, 1000, positions=positions)
    assert (tick, first_seen) == (936, 937)
    assert why.startswith("exact (first step")
    # a formation member (no positions passed) takes the earliest of the range, which the first step confirms here
    tick, _, why = m.refine_spawn_tick(ticks, fi0, states, 1000)
    assert tick == 936
    assert why.startswith("range [936, 937]")
    # the first step alone: gap-free before the first moved frame
    assert m.first_step_spawn_tick([100, 101, 102], [(0, 0, 0), (1, 0, 0), (2, 0, 59)], 20) == 82
    # a walk that does not fit whole steps of its own speed says nothing
    positions[-3] = (ticks.index(956), 3490, 8530)
    assert m.first_step_spawn_tick(ticks, positions, 20) is None
    # a step count outside the gap (three steps across a two-tick gap) says nothing
    positions[-3] = (ticks.index(956), 3437, 8668)
    assert m.first_step_spawn_tick(ticks, positions, 20) is None


def test_spawn_tick_takes_the_gap_s_earliest_tick_without_a_transition(m):
    ticks = [170, 173, 174]
    tick, first_seen, why = m.refine_spawn_tick(ticks, 1, [(1, 4), (2, 4)], 1000)
    assert (tick, first_seen) == (171, 173)
    assert "no transition" in why
    assert why == "range [171, 173] (frame gap, no transition seen), earliest used"
    # a summon-delay card starts in state 11, not 4: no transition read either
    tick, _, why = m.refine_spawn_tick(ticks, 1, [(1, 11), (2, 4)], 1000)
    assert tick == 171
    assert "no transition" in why


def test_a_unit_first_seen_out_of_its_deploy_after_a_long_gap_ends_its_deploy_there(m):
    """A UNIT FIRST SEEN OUT OF ITS DEPLOY (module doc): first seen walking (state 1) on 872 after a gap from 799, a
    1000 ms deploy: played on 852, its deploy ending where it stands (live 20261002-142800.b1's Giant, 72 ticks
    early before). A gap shorter than its deploy keeps the earliest tick, and so does a unit first seen deploying."""
    ticks = [799, 872, 873]
    assert m.refine_spawn_tick(ticks, 1, [(1, 1), (2, 1)], 1000)[:2] == (852, 872)
    assert "first seen out of its deploy" in m.refine_spawn_tick(ticks, 1, [(1, 1), (2, 1)], 1000)[2]
    assert m.refine_spawn_tick([860, 872, 873], 1, [(1, 2), (2, 2)], 1000)[:2] == (861, 872), "a gap under a deploy"
    assert m.refine_spawn_tick(ticks, 1, [(1, 11), (2, 11)], 1000)[0] == 800, "still waiting its stagger"


def test_an_evolved_mirror_cast_is_played_at_its_real_barrel(m):
    """AN EVOLVED MIRROR CAST (module doc): two barrels on the first frame, mirrored about the centre line (14500 and
    3500), of a card whose evolution throws a decoy: one evolved play at the barrel its own goblins land under
    (live 20261008-015445 t2092, read at the mean (9000, 25500) before). With no unit to tell, the first object; a
    plain cast (one object) or two that do not mirror are no mirror cast."""
    mirror = {"projectile": "GoblinBarrelSpell_EV1_Decoy", "unit": "GoblinDummy"}
    evo = {"name": "GoblinBarrel_EV1", "evo_mirror": mirror}
    cast = {"side": 0, "card_id": 28000004, "tracks": [{"first": 2092, "target": [3500, 25500]},
                                                       {"first": 2092, "target": [14500, 25500]}]}
    ticks = list(range(2092, 2200))

    def unit(key, name, x, y, t):
        return {"key": key, "side": 0, "card_id": 28000004, "unit": name, "x0": x, "y0": y, "first_index": t - 2092}

    ents = {1: unit(1, "BarrelGoblin", 14500, 26227, 2149), 2: unit(2, "GoblinDummy", 3500, 26227, 2149)}
    pt, why = m.evo_mirror_cast(cast, ents, ticks, evo)
    assert pt == (14500, 25500), why
    pt, why = m.evo_mirror_cast(cast, {}, ticks, evo)
    assert pt == (3500, 25500)
    assert "listed first" in why
    assert m.evo_mirror_cast({**cast, "tracks": cast["tracks"][:1]}, ents, ticks, evo) is None
    assert m.evo_mirror_cast({**cast, "tracks": [cast["tracks"][0], {"first": 2092, "target": [9500, 25500]}]},
                             ents, ticks, evo) is None
    assert m.evo_mirror_cast(cast, ents, ticks, {"name": "GoblinBarrel_EV1"}) is None, "no mirror in the evolution"


def test_a_heros_flag_spawns_are_no_plays(m):
    """A HERO'S FLAG SPAWNS (module doc): the hero Goblins' flag at (8741, 20967) puts Goblin_dummy units at (+-1000,
    -500) from it; the two of its card id first seen there while it stands (live 20261002-092029 t632, t635) are its
    spawns, and one elsewhere, or after the flag fell, stays a summon."""
    doc = {"hero_forms": [{"name": "Goblins_hero", "ability": {"effect": {
        "kind": "flag_spawns", "unit": "GoblinHero_Flag_Building",
        "spawns": [{"unit": "Goblin_dummy", "dx_milli": 1000, "dy_milli": -500},
                   {"unit": "Goblin_dummy", "dx_milli": -1000, "dy_milli": -500}]}}}]}
    ticks = list(range(546, 700))

    def ent(key, unit, x, y, t0, t1, role="summon"):
        return {"key": key, "side": 0, "card_id": 203000002, "unit": unit, "role": role, "x0": x, "y0": y,
                "first_index": t0 - 546, "last_index": t1 - 546}

    ents = {0: ent(0, "GoblinHero_Flag_Building", 8741, 20967, 546, 639, "spawned"),
            1: ent(1, "Goblin_Stab", 9741, 20467, 632, 672), 2: ent(2, "Goblin_Stab", 7741, 20467, 635, 672),
            3: ent(3, "Goblin_Stab", 5000, 20467, 632, 672), 4: ent(4, "Goblin_Stab", 9741, 20467, 660, 672)}
    assert m.flag_spawns(ents, ticks, doc) == 2
    assert [(e["role"], e["unit"]) for e in (ents[1], ents[2])] == [("spawned", "Goblin_dummy")] * 2
    assert [e["role"] for e in (ents[3], ents[4])] == ["summon", "summon"]


@needs_cards
def test_a_two_part_play_is_recovered_from_its_cards_layout(m):
    """A TWO-PART PLAY (module doc, RECOVERED TILE): the Goblinstein's Doctor stands 3500 behind its Monster in its
    owner's frame and 1000 toward the centre line, so a side-1 play whose Monster stood on (11500, 23499) and Doctor on
    (10500, 26999) (live 20261007-005625 t697, played 1750 off at the centroid before) was laid on the tile (11500,
    23500). A Monster pushed off its point is still placed by its Doctor (t2762: (14650, 17497) and (13500, 20999))."""
    card = {"count": 1, "second_summon": {"character": "goblinstein_doctor", "count": 1},
            "summon_radius_milli": 3500, "summon_width_milli": -1000}
    assert m.layout_offsets(card, 0, 3500) == [(0, 0), (1000, -3500)], "left half: toward the centre is +x"
    assert m.layout_offsets(card, 1, 14500) == [(0, 0), (-1000, 3500)], "right half, side 1: -x and +y"
    assert m.layout_offsets({**card, "summon_width_milli": None}, 0, 3500) == [], "a ring (the Rascals) has no width"
    tile, why = m.recovered_tile([(11500, 23499), (10500, 26999)], m.layout_offsets(card, 1, 11500))
    assert tile == [11500, 23500], why
    tile, why = m.recovered_tile([(14650, 17497), (13500, 20999)], m.layout_offsets(card, 1, 14650))
    assert tile == [14500, 17500], why


def test_a_one_frame_range_is_the_missed_frame(m):
    # 20260919-143305-A's Tombstone: frames 302 and 304 (303 missed); in state 4 through 321, 322 missed, state 0 on
    # 323. The frame gap leaves [303, 304] and the deploy end (spawn + 19 in (321, 323]) the same range. Seat B of the
    # same battle saw 303 and 322: it was created on 303, the missed frame, and so were 11 more such rows the other seat
    # pins, with none on the range's last tick.
    ticks = [300, 302, 304, *range(305, 322), 323, 324]
    fi = ticks.index(304)
    states = [(k, 4) for k in range(fi, ticks.index(321) + 1)] + [(ticks.index(323), 0)]
    tick, first_seen, why = m.refine_spawn_tick(ticks, fi, states, 1000)
    assert (tick, first_seen) == (303, 304)
    assert why == "range [303, 304] (deploy-end transition), earliest used"


@needs_modern_cards
def test_classification_tells_a_deploy_summon_from_a_spawned_unit(m):
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    # a level-11 Tombstone: its own building hp against its Skeletons'
    tomb = cards["Tombstone"]
    own_hp = tomb["hitpoints"] * m.ladder_percent(doc, tomb, tomb["summon_character"], 11) // 100
    unit, is_own, how = m.classify_unit(doc, tomb, 11, own_hp)
    assert (unit, is_own, how) == ("Tombstone", True, "exact")
    skel = (
        doc["units"]["Skeleton"]["hitpoints"] * m.ladder_percent(doc, tomb, "Skeleton", 11) // 100
    )
    unit, is_own, how = m.classify_unit(doc, tomb, 11, skel)
    assert (unit, is_own, how) == ("Skeleton", False, "exact")
    # an hp no object has takes the nearest within NEAREST_MAX_ERROR_PERCENT and says so
    unit, is_own, how = m.classify_unit(doc, tomb, 11, skel + 3)
    assert (unit, is_own, how) == ("Skeleton", False, "nearest")
    # beyond it the entity is an unknown object: truth only, never a deploy
    drill = cards["GoblinDrill"]
    unit, is_own, how = m.classify_unit(doc, drill, 11, 20000)
    assert (unit, is_own) == (None, False)
    assert how.startswith("unknown_object (nearest GoblinDrillDig 2560")
    assert m.classify_unit(doc, drill, 11, 2560) == ("GoblinDrillDig", True, "exact")
    # the balance deltas the corpus carries stay `nearest` (Ice Spirit 84 vs 85: 1 %)
    spirit = cards["IceSpirits"]
    own = spirit["hitpoints"] * m.ladder_percent(doc, spirit, spirit["summon_character"], 11) // 100
    unit, is_own, how = m.classify_unit(doc, spirit, 11, own - 2)
    assert (unit, is_own, how) == ("IceSpirits", True, "nearest")
    # a card the file does not know is a deploy summon by default
    assert m.classify_unit(doc, None, 11, 100) == (None, True, "no_card")


@needs_modern_cards
def test_a_tunnels_building_and_an_interval_spawners_unit_are_the_cards_spawned_units(m):
    """The Goblin Drill's dig leaves a building (`spawn_pathfind` `morph`) that spawns Goblins and leaves two at its
    death, and the Furnace (FirespiritHut) puts down Fire Spirits on an interval (`interval_spawner`). On the client
    16.402 corpus (20260920-083112, 071744, 071056) those are the building's 1313, the Goblins' 202 and the spirits'
    215 at level 11. Until this rule they matched no object of their card and were unknown objects, never paired: the
    Goblin Drill's 3,105 unit-ticks of 083112-A and the Furnace's 1,260 over its four fixtures scored as missing."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    drill = cards["GoblinDrill"]
    assert m.classify_unit(doc, drill, 11, 1313) == ("GoblinDrill", False, "exact")
    assert m.classify_unit(doc, drill, 11, 202) == ("Goblin", False, "exact")
    furnace = cards["FirespiritHut"]
    assert m.classify_unit(doc, furnace, 11, 727) == ("Furnace_rework", True, "exact")
    # the tables' FireSpirits base is 85 (217 at level 11); client 16.402 shows 215 (base 84), within the nearest rule
    assert m.classify_unit(doc, furnace, 11, 217) == ("FireSpirits", False, "exact")
    assert m.classify_unit(doc, furnace, 11, 215) == ("FireSpirits", False, "nearest")
    # the reachable set grows by exactly these objects
    assert set(m.reachable_units(doc, drill)) == {"GoblinDrillDig", "GoblinDrill", "Goblin"}
    assert set(m.reachable_units(doc, furnace)) == {"Furnace_rework", "FireSpirits"}


def test_a_tunnel_is_timed_by_its_steps_from_its_king(m):
    """`tunnel_spawn_ticks`, measured on the client 16.402 corpus (20260920-083112, both seats): a tunnel's first
    frame stands two SpawnPathfindSpeed steps from its King's centre, so one first seen n steps out had its first
    frame n - 2 ticks before. Seat A missed tick 249: it first shows the Goblin Drill of 249 on 250, three steps out
    (854 from its King), and the frame-gap rule kept the latest tick of [249, 250]; it first shows the Miner of 1203
    already surfaced on 1204, and the deploy-end rule timed the surfacing. Seat B saw both first frames. Controls: a
    first frame two steps out, a start that is not whole steps, and a card that does not tunnel stay as they were."""
    cards = {"Miner": {"spawn_pathfind": {"speed": 650, "morph": None}},
             "GoblinDrill": {"spawn_pathfind": {"speed": 300, "morph": "GoblinDrill"}}, "Knight": {}}
    ticks = [247, 248, 250, 251, 1202, 1204, 1205]
    towers = [{"side": 0, "slot": 0, "x": 9000, "y": 3000}, {"side": 1, "slot": 0, "x": 9000, "y": 29000}]

    def ent(key, fi, x0, y0, state, c0=None):
        return {"key": key, "first_index": fi, "x0": x0, "y0": y0, "c0": c0, "states": [(fi, state)]}

    ents = {
        12: ent(12, 2, 8177, 3228, 6, c0=(8477, 3215)),  # 083112-A: 3 steps out on 250
        38: ent(38, 5, 8500, 500, 4, c0=(9235, 1777)),  # 083112-A: surfaced on 1204, last tunnel point 2 steps out
        50: ent(50, 3, 8477, 3215, 6, c0=(8775, 3182)),  # two steps out: a first frame
        51: ent(51, 3, 8330, 3215, 6, c0=(8600, 3200)),  # 684 from the King: not whole steps of 300
        52: ent(52, 2, 8177, 3228, 1, c0=(8200, 3228)),  # a Knight: not a tunnel
    }

    def dep(card, key, tick, why="range [249, 250] (frame gap, no transition seen), latest used"):
        return {"card": card, "keys": [key], "side": 0, "tick": tick, "tick_evidence": why}

    deploys = [dep("GoblinDrill", 12, 250), dep("Miner", 38, 1204, "exact (deploy-end transition)"),
               dep("GoblinDrill", 50, 251, "exact"), dep("GoblinDrill", 51, 251, "exact"), dep("Knight", 52, 250)]
    m.tunnel_spawn_ticks(deploys, ents, ticks, towers, cards)
    assert [d["tick"] for d in deploys] == [249, 1203, 251, 251, 250], deploys
    assert deploys[0]["tick_evidence"].startswith("tunnel count: first seen under ground at tick 250, 854 from")
    assert "first seen surfaced" in deploys[1]["tick_evidence"]
    assert deploys[2]["tick_evidence"] == "exact"


def _eff(side, cid, oid, tx, ty):
    return {
        "side": side,
        "card_id": cid,
        "id": oid,
        "x": 0,
        "y": 0,
        "projectile_x": tx,
        "projectile_y": ty,
    }


def test_one_spell_cast_is_one_run_of_objects_not_one_deploy_per_frame(m):
    fb, log, arrows = 28000000, 28000011, 28000001
    frames = []
    # a Fireball object seen on 15 frames (1040..1066, frames missed) -> ONE cast
    for t in range(1040, 1067, 2):
        frames.append({"tick": t, "effects": [_eff(1, fb, "0xA", 3500, 12500)]})
    # a Log: the airborne object 254..260, then the rolling object from 262 -> ONE cast
    for t in range(254, 261, 2):
        frames.append({"tick": t, "effects": [_eff(0, log, "0xB", 3500, 14500)]})
    for t in range(262, 313, 2):
        frames.append({"tick": t, "effects": [_eff(0, log, "0xC", 3500, 24600)]})
    # an Arrows volley: three objects on one tick, two of them seen again 2 ticks on
    frames.append(
        {
            "tick": 195,
            "effects": [
                _eff(1, arrows, f"0x{k}", 9000 + 300 * k, 30000 + 600 * k) for k in range(3)
            ],
        }
    )
    frames.append(
        {
            "tick": 197,
            "effects": [_eff(1, arrows, "0x1", 9300, 30600), _eff(1, arrows, "0x2", 9600, 31200)],
        }
    )
    # the same Fireball card cast again 500 ticks later -> a second cast
    frames.append({"tick": 1600, "effects": [_eff(1, fb, "0xA", 5000, 20000)]})
    frames.sort(key=lambda f: f["tick"])
    casts = m.spell_casts(frames)
    got = [
        (
            c["side"],
            c["card_id"],
            frames[c["first_index"]]["tick"],
            c["frames"],
            c["objects"],
            c["aim"],
        )
        for c in casts
    ]
    assert got == [
        (1, arrows, 195, 2, 3, [9300, 30600]),
        (0, log, 254, 30, 2, [3500, 14500]),
        (1, fb, 1040, 14, 1, [3500, 12500]),
        (1, fb, 1600, 1, 1, [5000, 20000]),
    ], got
    assert casts[0]["aim_rule"].startswith("mean of 3 objects")
    assert casts[2]["aim_rule"] == "the object's projectile target"
    # a gap of exactly CAST_GAP_TICKS is the same cast, one more is a new one
    frames = [
        {"tick": 100, "effects": [_eff(0, fb, "0x1", 1, 1)]},
        {"tick": 100 + m.CAST_GAP_TICKS, "effects": [_eff(0, fb, "0x1", 1, 1)]},
        {"tick": 101 + 2 * m.CAST_GAP_TICKS, "effects": [_eff(0, fb, "0x1", 1, 1)]},
    ]
    assert [frames[c["first_index"]]["tick"] for c in m.spell_casts(frames)] == [
        100,
        101 + 2 * m.CAST_GAP_TICKS,
    ]
    # non-spell effects are not casts
    assert m.spell_casts([{"tick": 5, "effects": [_eff(0, 26000000, "0x1", 1, 1)]}]) == []
    # a cast whose first sighting is its AREA object (a live capture's Lightning: the area on its drop's frame at the
    # cast point, the bolts 10 ticks on) is marked so the dating takes no strike lead (`effect_leads`)
    lightning = 28000007
    area = {**_eff(0, lightning, "0xA", 0, 0), "x": 15500, "y": 18500, "area": True}
    del area["projectile_x"], area["projectile_y"]
    bolt = _eff(0, lightning, "0xB", 14181, 15215)
    (cast,) = m.spell_casts([{"tick": 562, "effects": [area]}, {"tick": 572, "effects": [area, bolt]}])
    assert (cast["first_area"], cast["aim"]) == (True, [15500, 18500]), cast
    (late,) = m.spell_casts([{"tick": 572, "effects": [bolt]}])
    assert not late["first_area"], "a capture with no area objects: the bolt first, dated by the lead"


def test_a_tap_of_a_spell_card_is_a_cast_whatever_the_log_record_says(m, tmp_path):
    # A scripted cycle play logs {"cycled": "Rage", "tile": ...} and no `actual`; only three
    # records in every placement log carry "actual": "cast". Classed by `actual` alone, every
    # cycled spell became a troop DEPLOY, matched no unit group, and was dropped: 47 of 90 spell
    # taps never reached a fixture, and the battles stayed "playable" without them.
    log = tmp_path / "placements.jsonl"
    rows = [
        {"local_side_native": 0},
        {"tick": 309, "cycled": "Rage", "tile": [3.5, 1.5], "for": "Knight"},
        {"tick": 339, "card": "Knight", "requested": [14.5, 8.5], "side": 0},
        {"tick": 400, "card": "Fireball", "requested": [9.5, 25.5], "side": 0, "actual": "cast"},
    ]
    log.write_text("\n".join(json.dumps(r) for r in rows) + "\n", encoding="utf-8")
    names = {"Rage", "Knight", "Fireball"}
    ids = {"Rage": 28000002, "Knight": 26000000, "Fireball": 28000000}
    taps, _ = m.read_placements([str(log)], names, ids, {"Rage", "Fireball"})
    kinds = {t["card"]: t["kind"] for t in taps}
    assert kinds == {"Rage": "cast", "Knight": "deploy", "Fireball": "cast"}, kinds
    assert next(t for t in taps if t["card"] == "Rage")["cycled"], "the cycled flag survives"
    # without the spell names the record decides, as before (make_spell_impact_fixture's call)
    taps, _ = m.read_placements([str(log)], names, ids)
    assert {t["card"]: t["kind"] for t in taps}["Rage"] == "deploy"


def test_a_cast_is_labelled_by_the_casters_elixir_drop_not_the_log_tick(m):
    # 181741: the log's tick for a Rage repeated on lines written seconds apart, and tap + latency
    # put the cast at 182 where the caster's elixir says 324. The label is the first frame at or
    # after the tap on which the caster's pool falls by the card's cost.
    ticks = [100, 101, 102, 104, 105, 106, 110]
    # regen +200; a 2-elixir drop on 102; regen over a 2-tick gap; 3 elixir on 105; 2 on 106; regen
    elixir = [90000, 90200, 70200, 70600, 40600, 20600, 21400]
    assert m.first_cast_drop(ticks, elixir, 100, 2, set()) == 102
    # the frame a matched deploy explains is skipped, and the next drop OF THE COST is taken
    assert m.first_cast_drop(ticks, elixir, 100, 2, {102}) == 106
    # a 3-elixir drop is not a 2-elixir card's
    assert m.first_cast_drop(ticks, elixir, 103, 2, set()) == 106
    assert m.first_cast_drop(ticks, elixir, 103, 3, set()) == 105
    # nothing before the tap, nothing past the window, nothing where the capture has no value
    assert m.first_cast_drop(ticks, elixir, 107, 2, set()) is None
    assert m.first_cast_drop([0, 1 + m.CAST_DROP_WINDOW], [50000, 30000], 0, 2, set()) is None
    assert m.first_cast_drop(ticks, [None] * len(ticks), 100, 2, set()) is None


def test_a_log_without_its_own_side_takes_the_captures(m, tmp_path):
    # Half the placement logs of 2026-09-18/19 carry no local_side_native record, and a cycled
    # play carries no side of its own: every one of them was skipped (133849's Rages, 134739's).
    # The capture knows its instance port and its side; the log's port says which it is.
    log = tmp_path / "placements-20260918-134739-10001.jsonl"
    tap = {"tick": 177, "cycled": "Rage", "tile": [14.5, 1.5], "for": "Giant"}
    log.write_text(json.dumps(tap) + "\n", encoding="utf-8")
    ids = {"Rage": 28000002}
    taps, _ = m.read_placements([str(log)], {"Rage"}, ids, {"Rage"})
    assert taps == [], "no side at all: skipped, as before"
    taps, _ = m.read_placements([str(log)], {"Rage"}, ids, {"Rage"}, {"10001": 1})
    assert [(t["side"], t["card"], t["kind"]) for t in taps] == [(1, "Rage", "cast")]
    # the log's own record still wins over the default
    log.write_text(json.dumps({"local_side_native": 0}) + "\n" + log.read_text(encoding="utf-8"), encoding="utf-8")
    taps, _ = m.read_placements([str(log)], {"Rage"}, ids, {"Rage"}, {"10001": 1})
    assert [t["side"] for t in taps] == [0]


def test_a_log_name_the_client_showed_matches_its_card_by_display_name(m, tmp_path):
    # The logs write the name the client showed; cards.json keys the internal name and keeps the display name
    # beside it. Unmatched, the tap carried no tick and no point into its fixture: 48 records of the 96 logs of
    # 2026-09-18/20, among them every cycled "Ice Spirit" (28) and "Ice Golem" (9) of the Rage-tile battles.
    cards = [
        {"name": "IceSpirits", "display_name": "Ice Spirits"},
        {"name": "IceGolemite", "display_name": "Ice Golem"},
        {"name": "Wallbreakers", "display_name": "Wallbreakers"},
        {"name": "AxeMan", "display_name": "Executioner"},
        {"name": "Elixir Collector", "display_name": "Elixir Collector"},
        {"name": "Goblins", "display_name": "Goblins"},
        {"name": "TwinA", "display_name": "Twin"},
        {"name": "TwinB", "display_name": "Twin"},
    ]
    names = {c["name"] for c in cards}
    display = m.display_names(cards)
    for log, card in [
        ("Ice Spirit", "IceSpirits"),
        ("Ice Golem", "IceGolemite"),
        ("Wall Breakers", "Wallbreakers"),
        ("Executioner", "AxeMan"),
        ("ElixirCollector", "Elixir Collector"),
        ("Goblins", "Goblins"),
    ]:
        assert m.canon_name(log, names, display) == card, log
    # a display name two cards share matches neither; a name nothing carries is kept as it is
    assert m.canon_name("Twin", names, display) == "Twin"
    assert m.canon_name("Giant Snowball", names, display) == "Giant Snowball"
    # the tap reaches the fixture with its tick and point
    log = tmp_path / "placements.jsonl"
    rows = [{"local_side_native": 0}, {"tick": 1912, "cycled": "Ice Spirit", "tile": [14.5, 1.5], "for": "Knight"}]
    log.write_text("\n".join(json.dumps(r) for r in rows) + "\n", encoding="utf-8")
    ids = {"IceSpirits": 26000030}
    taps, _ = m.read_placements([str(log)], names, ids, set(), None, display)
    got = [(t["card"], t["id"], t["tick"], t["native"]) for t in taps]
    assert got == [("IceSpirits", 26000030, 1912, [3500, 1500])], got
    taps, _ = m.read_placements([str(log)], names, ids, set())
    assert [t["id"] for t in taps] == [None], "without the display names the tap names no card, as before"


def test_a_troop_tap_on_the_centre_line_goes_to_the_tile_on_its_right(m):
    # 002736's Royal Hogs: the log asked for (9000, 12500), the tile boundary on the centre line,
    # and the game put them one tile right; the fixture fed the raw point and the hogs started
    # ~500 left. A tile centre is left alone, and a y boundary too (no recording shows one).
    assert m.snap_troop_tap([9000, 12500]) == [9500, 12500]
    assert m.snap_troop_tap([14500, 8500]) == [14500, 8500]
    assert m.snap_troop_tap([3500, 1000]) == [3500, 1000]


def test_fnv1a64_matches_the_harness_known_answers(m):
    # the same known answers tests/replay_parity.rs pins for harness.rs fnv1a64
    assert m.fnv1a64(b"") == "cbf29ce484222325"
    assert m.fnv1a64(b"a") == "af63dc4c8601ec8c"


@needs_modern_cards
def test_the_committed_sample_is_the_documented_battle(m):
    with open(SAMPLE, encoding="utf-8") as fh:
        fx = json.load(fh)
    assert fx["format"] == m.FORMAT
    assert fx["frame"] == {
        "blue_native_side": 0,
        "transform": "identity",
        "native_per_tile": 1000,
        "subtiles_per_native": 18,
    }
    assert fx["playable"]
    assert fx["unplayable_reasons"] == []
    assert fx["ticks"]["last"] == 1440
    # classified against this tree's cards.json (a mismatch is a NOTE in the harness)
    with open(CARDS, "rb") as fh:
        assert fx["cards_json_fnv1a64"] == m.fnv1a64(fh.read()), "rerun the maker on the sample"
    assert fx["capture"] == "20260920-003751-B"
    assert fx["placements"] == ["20260920-003751-A", "20260920-003751-B"]
    by = {(d["side"], d["card"]): d for d in fx["deploys"]}
    # the scripted taps of capture 20260920-003751: the Prince at t150 -> the entity
    # at 172/173, the Dark Prince t236, the Battle Ram t912, the Giant t1303, the
    # Musketeer t1390
    assert by[(0, "Prince")]["tick"] == 172
    assert by[(0, "Prince")]["first_seen"] == 173
    assert by[(0, "Prince")]["tap"]["tick"] == 150
    assert by[(0, "DarkPrince")]["tap"]["tick"] == 236
    assert by[(0, "BattleRam")]["tap"]["tick"] == 912
    # the Battle Ram's spawn is pinned by its first step (936 of the range [936, 937])
    assert by[(0, "BattleRam")]["tick"] == 936
    assert by[(0, "BattleRam")]["tick_evidence"].startswith("exact (first step")
    assert by[(1, "Giant")]["tap"]["tick"] == 1303
    assert by[(1, "Giant")]["tick"] == 1329
    assert by[(1, "Musketeer")]["tap"]["tick"] == 1390
    # a tap carries its tick and tile only
    assert set(by[(0, "Prince")]["tap"]) == {"tick", "native", "cycled"}
    # the cycled Skeletons are a three-unit group at the tap tile
    assert by[(0, "Skeletons")]["count"] == 3
    assert by[(0, "Skeletons")]["source"] == "tap_tile"
    # the Battle Ram's Barbarians are spawned, not deployed
    spawned = [g for g in fx["spawned_groups"] if g["card"] == "BattleRam"]
    assert len(spawned) == 1
    assert spawned[0]["unit"] == "Barbarian"
    assert spawned[0]["count"] == 2
    # every truth column decodes to the entity's frame count
    for e in fx["truth"]["entities"]:
        for col in fx["truth"]["columns"]:
            runs = e[col]
            assert sum(runs[1::2]) == e["n"], (e["key"], col)


# The committed sample's recipe (the maker's docstring, THE COMMITTED SAMPLE).
SAMPLE_CAPTURE = "20260920-003751-B"
SAMPLE_ARGS = ["--until-tick", "1440"]


@needs_cards
def test_the_committed_sample_is_what_the_maker_builds_from_its_capture(m, tmp_path):
    """The maker's own --check on the sample's recipe: STALE fails this test. The same check
    on a copy with one hp changed must say STALE, or "current" would prove nothing.

    SKIPS, LOUDLY, without ROYALELIVE_REPORTS: the capture is not in a plain checkout, so the
    sample cannot be re-derived and nobody has checked it. A skip here is not a pass."""
    reports = os.environ.get("ROYALELIVE_REPORTS")
    if not reports:
        pytest.skip(
            "SKIPPED, NOT PASSED: ROYALELIVE_REPORTS is not set, so the committed sample"
            " was not rebuilt from its capture and may be stale. That variable names the"
            " folder holding the recorded battles, which are not public and which a CLONE"
            " NEVER HAS -- this is permanently local coverage rather than a setup step"
            " somebody forgot. A green run of this suite has not checked that the sample"
            " still matches the capture it was made from."
        )
    missing = m.missing_id_files()
    if missing:
        pytest.skip(
            "SKIPPED, NOT PASSED: the 15.535.29 pack is absent here (missing "
            + ", ".join(missing)
            + "), so card ids cannot be resolved and the sample was not rebuilt. It is not"
            " committed: a worktree needs data/raw/cr-15.535.29 linked in."
        )
    if m.capture_named(SAMPLE_CAPTURE, reports) is None:
        pytest.skip(
            f"{reports} has no capture named {SAMPLE_CAPTURE}, so the committed sample was not"
            " rebuilt from it and may be stale -- a skip here is not a pass"
        )

    def check(fixture):
        cmd = [
            sys.executable,
            os.path.join(ROOT, "tools", "make_replay_fixture.py"),
            SAMPLE_CAPTURE,
            *SAMPLE_ARGS,
            "--check",
            fixture,
        ]
        return subprocess.run(cmd, capture_output=True, text=True, timeout=600, cwd=ROOT)

    run = check(SAMPLE)
    why = (
        f"{run.stdout}{run.stderr}\nthe committed sample is not what the maker builds: rebuild"
        f" it with `python tools/make_replay_fixture.py {SAMPLE_CAPTURE} {' '.join(SAMPLE_ARGS)}"
        f" --out <dir>` and copy <dir>/{SAMPLE_CAPTURE}.replay.json over sample.json"
    )
    assert run.returncode == 0, why
    assert "is current" in run.stdout, why
    with open(SAMPLE, encoding="utf-8") as fh:
        fx = json.load(fh)
    fx["truth"]["entities"][-1]["hp"][0] += 1
    changed = tmp_path / "changed.json"
    changed.write_text(json.dumps(fx), encoding="utf-8")
    run = check(str(changed))
    assert run.returncode == 1, run.stdout + run.stderr
    assert "STALE" in run.stdout, run.stdout + run.stderr


def test_a_deploy_s_form_is_read_off_its_units_card_class(m):
    """A DEPLOY'S FORM: class 13 names the evolved row, class 203 the hero row, a plain id the base card; a form id
    with no row is not guessed."""
    rows = {13000010: "Skeletons_EV1", 203000014: "Musketeer_hero"}
    assert m.deploy_form(13000010, rows, "Skeletons") == {"form": "ev1", "form_row": "Skeletons_EV1"}
    assert m.deploy_form(203000014, rows, "Musketeer") == {"form": "hero", "form_row": "Musketeer_hero"}
    assert m.deploy_form(26000010, rows, "Skeletons") == {"form": "base", "form_row": "Skeletons"}
    assert m.deploy_form(13000099, rows, "Skeletons") == {}
    assert m.deploy_form(203000099, rows, "Musketeer") == {}
    assert set(m.FORMS_READ) == {"ev1", "hero", "base"}


def test_the_form_rows_are_named_by_their_class_13_and_203_ids(m):
    """The ids oracle's scenes and the live captures saw; the id table names a form unit by its base card."""
    if m.missing_id_files():
        pytest.skip("SKIPPED, NOT PASSED: the 15.535.29 pack is absent, so the form rows cannot be read")
    rows = m.load_form_rows()
    assert rows[13000010] == "Skeletons_EV1"
    assert rows[13000014] == "Musketeer_EV1"
    assert rows[13000096] == "Cannon_EV1"
    assert rows[203000014] == "Musketeer_hero"
    assert rows[203000038] == "IceGolemite_hero"
    table = m.load_id_table()
    assert (table[13000010], table[13000096], table[203000014]) == ("Skeletons", "Cannon", "Musketeer")


def test_the_corpus_hero_musketeer_is_published_as_a_hero(m, tmp_path):
    """20260918-122757.b2: side 0's Musketeer slot is the hero form (the reader's forms[] = 2) and its two plays
    (1348, 3432) put units of 203000014 on the board; side 1's two Musketeers are plain (base).
    SKIPS, LOUDLY, without ROYALELIVE_REPORTS or the 15.535.29 pack: a skip here is not a pass."""
    reports = os.environ.get("ROYALELIVE_REPORTS")
    if not reports or m.missing_id_files() or m.capture_named("20260918-122757.b2", reports) is None:
        pytest.skip("SKIPPED, NOT PASSED: the capture 20260918-122757.b2 or the 15.535.29 pack is not here")
    maker = os.path.join(ROOT, "tools", "make_replay_fixture.py")
    run = subprocess.run(
        [sys.executable, maker, "20260918-122757.b2", "--out", str(tmp_path)],
        capture_output=True,
        text=True,
        timeout=600,
        cwd=ROOT,
    )
    assert run.returncode == 0, run.stdout + run.stderr
    with open(tmp_path / "20260918-122757.b2.replay.json", encoding="utf-8") as fh:
        fx = json.load(fh)
    musketeers = [
        (d["tick"], d["side"], d.get("form"), d.get("form_row")) for d in fx["deploys"] if d["card"] == "Musketeer"
    ]
    hero = [(1348, 0, "hero", "Musketeer_hero"), (3432, 0, "hero", "Musketeer_hero")]
    assert [x for x in musketeers if x[1] == 0] == hero
    assert [x for x in musketeers if x[1] == 1] == [(1518, 1, "base", "Musketeer"), (2732, 1, "base", "Musketeer")]
    assert fx["forms_read"] == m.FORMS_READ


def test_a_capture_is_found_by_its_fixture_name(m, tmp_path):
    def touch(name):
        p = tmp_path / name
        p.write_bytes(b"x")
        return str(p)

    a = touch("frames-auto-20260920-003751-31" + m.CAPTURE_SUFFIX)
    b = touch("frames-auto-20260920-003751-42" + m.CAPTURE_SUFFIX)
    b1 = touch("frames-auto-20260920-010218-31.b1" + m.CAPTURE_SUFFIX)
    folder = str(tmp_path)
    # the letters are the whole folder's, in sort order of the seats
    assert m.capture_named("20260920-003751-A", folder) == a
    assert m.capture_named("20260920-003751-B", folder) == b
    assert m.capture_named("20260920-010218-A.b1", folder) == b1
    assert m.capture_named("20260920-003751-C", folder) is None
    assert m.capture_named("20260920-003751-B", None) is None
    assert m.capture_named("20260920-003751-B", str(tmp_path / "missing")) is None
    # two files that both answer to one name: an error, never a pick
    touch("frames-20260920-003751-31" + m.CAPTURE_SUFFIX)
    with pytest.raises(SystemExit):
        m.capture_named("20260920-003751-A", folder)


def test_replay_formations_reads_the_sample_s_offsets_stagger_and_reach():
    """tools/replay_formations.py: the corpus report's hand-read numbers, off the sample."""
    spec = importlib.util.spec_from_file_location(
        "replay_formations", os.path.join(ROOT, "tools", "replay_formations.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    with open(SAMPLE, encoding="utf-8") as fh:
        fx = json.load(fh)
    rows = {(g["side"], g["card"]): g for g in mod.group_rows(fx)}
    # the three cycled Skeletons behind the king: laid 700 native either side and
    # 1250 below the tap tile (the game displaced the ring off the king footprint),
    # all three leaving deploy state at spawn + 19
    sk = rows[(0, "Skeletons")]
    assert sk["source"] == "tap_tile"
    assert sorted(m["offset"] for m in sk["members"]) == [[-700, -1250], [-43, -336], [698, -1250]]
    assert [m["stagger"] for m in sk["members"]] == [19, 19, 19]
    # the Prince's melee reach against the princess tower: 3135 native centre to
    # centre = Range 1600 + own radius 600 + the tower's 1000 (the reach gap)
    pr = rows[(0, "Prince")]["members"][0]
    assert pr["offset"] == [0, 0]
    assert pr["stagger"] == 19
    assert 3100 <= pr["reach_to_tower"] <= 3200
    # a unit that never attacked a tower has no reach
    assert rows[(1, "Giant")]["members"][0]["reach_to_tower"] is None


class TestPathCellRotation:
    """`path_cell` is the only part of the fixture no capture can exercise.

    Measured 2026-09-22: 0 of 75 captures in the corpus meet the rotate condition (side 0
    already defends low y in every one), so the rotated branch of the maker is dead against
    real data and these properties are the only instrument it has.
    """

    def test_unrotated_is_the_published_index(self, m):
        for index in (0, 1, 35, 36, 92, 2033, 2303):
            assert m.path_cell(index, False) == [index % m.CELL_COLS, index // m.CELL_COLS]

    def test_rotation_is_a_mirror_and_is_its_own_inverse(self, m):
        for index in (0, 1, 35, 36, 92, 2033, 2303):
            col, row = m.path_cell(index, True)
            back = m.path_cell(row * m.CELL_COLS + col, True)
            assert back == m.path_cell(index, False)

    def test_a_rotated_cell_centre_is_where_pos_of_sends_the_centre(self, m):
        """The property that makes the cells agree with the positions in the same fixture.

        `pos_of` maps native (x, y) to (NATIVE_W - x, NATIVE_H - y). A cell centre sits at
        500c + 250, so its image is 500(COLS-1-c) + 250 -- exactly the mirrored cell's centre.
        A fixture that flipped the board and not the path would read as a pathfinder defect on
        every rotated battle, so this is the assertion that has to hold.
        """
        half = m.CELL_NATIVE // 2
        for index in range(0, m.CELL_COLS * m.CELL_ROWS, 67):
            col, row = m.path_cell(index, False)
            rcol, rrow = m.path_cell(index, True)
            assert (m.NATIVE_W - (col * m.CELL_NATIVE + half)) == rcol * m.CELL_NATIVE + half
            assert (m.NATIVE_H - (row * m.CELL_NATIVE + half)) == rrow * m.CELL_NATIVE + half

    def test_the_grid_covers_the_arena_exactly(self, m):
        assert m.CELL_COLS * m.CELL_NATIVE == m.NATIVE_W
        assert m.CELL_ROWS * m.CELL_NATIVE == m.NATIVE_H


def _load_formations():
    spec = importlib.util.spec_from_file_location(
        "replay_formations", os.path.join(ROOT, "tools", "replay_formations.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---------------------------------------------------------------------------
# attack timers and elixir (the maker's module doc: ATTACK TIMERS, ELIXIR)


def test_attack_timers_are_four_integer_columns_after_the_seven(m):
    e = {
        "attack_progress_ms": 1250,
        "attack_load_timer_ms": 650,
        "event_timer_ms": 150,
        "attack_component_valid": True,
    }
    assert m.attack_timers(e) == (1250, 650, 150, 1)
    assert m.attack_timers({**e, "attack_component_valid": False})[3] == 0
    # integers, not booleans (True == 1 in Python, but JSON `true` is not an integer to
    # harness.rs decode_rle)
    assert all(type(v) is int for v in m.attack_timers(e))
    assert type(m.attack_timers({**e, "attack_component_valid": False})[3]) is int
    # a capture without the fields gives nulls, not a crash
    assert m.attack_timers({}) == (None, None, None, None)
    assert m.TRUTH_COLUMNS == (
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


def test_elixir_is_one_run_length_column_per_fixture_side(m):
    frames = [
        {"tick": 0, "elixir_raw": [60000, 50000]},
        {"tick": 1, "elixir_raw": [60178, 50178]},
        {"tick": 2, "elixir_raw": [30356, 50356]},
    ]
    # indexed by the capture's sides, which the fixture keeps (a turned capture included:
    # test_one_battle_is_one_fixture_whichever_way_up_it_was_recorded)
    assert m.elixir_columns(frames) == {
        "0": [60000, 1, 60178, 1, 30356, 1],
        "1": [50000, 1, 50178, 1, 50356, 1],
    }
    # a full bar is one run
    full = [{"tick": t, "elixir_raw": [100000, 100000]} for t in range(5)]
    assert m.elixir_columns(full) == {"0": [100000, 5], "1": [100000, 5]}
    # a capture without the pair has no column at all
    assert m.elixir_columns([{"tick": 0}, {"tick": 1}]) is None
    # a frame without a usable pair is a null in the run, not a crash
    odd = [
        {"tick": 0, "elixir_raw": [1, 2]},
        {"tick": 1},
        {"tick": 2, "elixir_raw": []},
        {"tick": 3, "elixir_raw": [None, 5]},
    ]
    assert m.elixir_columns(odd) == {"0": [1, 1, None, 3], "1": [2, 1, None, 2, 5, 1]}


def test_replay_formations_reads_the_timer_columns_and_tolerates_their_absence():
    rf = _load_formations()
    e = {
        "n": 3,
        "x": [100, 3],
        "y": [200, 2, None, 1],
        "hp": [50, 3],
        "target": [-1, 3],
        "path_n": [0, 3],
        "state": [2, 3],
        "attack_progress_ms": [1100, 1, 1150, 1, 1200, 1],
        "attack_load_timer_ms": [650, 1, 600, 1, 700, 1],
        "event_timer_ms": [0, 3],
        "attack_component_valid": [1, 3],
    }
    rows = rf.rows_of(e)
    assert rows[0] == (100, 200, 50, -1, 0, 2, 1100, 650, 0, 1)
    assert rows[2] == (100, None, 50, -1, 0, 2, 1200, 700, 0, 1)
    # a fixture made before the timers were published: the six, then nulls
    old = {k: v for k, v in e.items() if k in ("n", "x", "y", "hp", "target", "path_n", "state")}
    assert rf.rows_of(old)[1] == (100, 200, 50, -1, 0, 2, None, None, None, None)


# ---------------------------------------------------------------------------
# spell objects (the maker's module doc: SPELL OBJECTS)

ARROWS, LOG, LIGHTNING = 28000001, 28000011, 28000007


def _obj(side, cid, oid, pos, prev, target):
    return {
        "side": side,
        "card_id": cid,
        "id": oid,
        "x": pos[0],
        "y": pos[1],
        "x2": prev[0],
        "y2": prev[1],
        "projectile_x": target[0],
        "projectile_y": target[1],
    }


def _spell_frames():
    """Three casts on frames 100..102, 200..205, 207 (206 missed), 300..302, 304, 305 (303
    missed; the capture ends): a Lightning-like object that never moves, a volley of three
    objects (two fly at once, one waits four ticks), and a Log's airborne then rolling
    object."""
    at: dict[int, list] = {}

    def put(t, eff):
        at.setdefault(t, []).append(eff)

    for k, t in enumerate(range(200, 204)):  # a: flies from 200, last seen 203
        put(t, _obj(1, ARROWS, "a", (9000, 28000 - 1000 * k), (9000, 29000 - 1000 * k), (9000, 23000)))
    for k, t in enumerate(range(200, 202)):  # b: flies from 200, last seen 201
        put(t, _obj(1, ARROWS, "b", (8000, 28000 - 1000 * k), (8000, 29000 - 1000 * k), (8000, 26000)))
    for t in range(200, 204):  # c: waits at its launch point ...
        put(t, _obj(1, ARROWS, "c", (10000, 29000), (10000, 29000), (10000, 24000)))
    put(204, _obj(1, ARROWS, "c", (10000, 28000), (10000, 29000), (10000, 24000)))  # ... departs 204
    put(205, _obj(1, ARROWS, "c", (10000, 27000), (10000, 28000), (10000, 24000)))
    put(300, _obj(0, LOG, "L1", (3500, 5500), (3500, 4500), (3500, 7500)))
    put(301, _obj(0, LOG, "L1", (3500, 6500), (3500, 5500), (3500, 7500)))
    put(302, _obj(0, LOG, "L1", (3500, 7400), (3500, 6500), (3500, 7500)))
    put(304, _obj(0, LOG, "L2", (3500, 7700), (3500, 7500), (3500, 17600)))
    put(305, _obj(0, LOG, "L2", (3500, 7900), (3500, 7700), (3500, 17600)))
    for t in (100, 101):
        put(t, _obj(0, LIGHTNING, "z", (5000, 20000), (5000, 20000), (5000, 20000)))
    ticks = [100, 101, 102, *range(200, 206), 207, 300, 301, 302, 304, 305]
    return [{"tick": t, "effects": at.get(t, [])} for t in ticks]


def test_a_cast_publishes_each_object_s_launch_departure_target_and_arrival(m):
    casts = {c["card_id"]: c for c in m.spell_casts(_spell_frames())}
    volley = casts[ARROWS]
    assert volley["tracks"] == [
        {"first": 200, "launch": [9000, 29000], "depart": 200, "target": [9000, 23000],
         "last_seen": 203, "end": [9000, 25000], "arrival": 204},
        {"first": 200, "launch": [8000, 29000], "depart": 200, "target": [8000, 26000],
         "last_seen": 201, "end": [8000, 27000], "arrival": 202},
        # waited at its launch point until 204; the frame after its last sighting is 207
        {"first": 200, "launch": [10000, 29000], "depart": 204, "target": [10000, 24000],
         "last_seen": 205, "end": [10000, 27000], "arrival": 207},
    ]  # fmt: skip
    assert volley["departures"] == [[200, 2], [204, 1]]
    assert volley["objects"] == 3
    # the Log: two objects, the rolling one launched from the airborne one's target; the
    # capture ends before the roll does, so it has no arrival
    log = casts[LOG]["tracks"]
    assert [(o["first"], o["depart"], o["last_seen"], o["arrival"]) for o in log] == [
        (300, 300, 302, 304),
        (304, 304, 305, None),
    ]
    assert log[1]["launch"] == log[0]["target"] == [3500, 7500]
    assert casts[LOG]["departures"] == [[300, 1], [304, 1]]
    # an object never seen off its launch point has no departure
    assert casts[LIGHTNING]["tracks"][0]["depart"] is None
    assert casts[LIGHTNING]["departures"] == [[None, 1]]
    assert casts[LIGHTNING]["tracks"][0]["arrival"] == 102


class TestSpellPointRotation:
    """Spell points must turn with the arena (and their sides stay), and no capture of the
    corpus is turned (0 of 75 on 2026-09-22), so these and the both-ways-up battle test below
    are the only instruments the turned branch has, as for the path cells above."""

    def test_arena_point_is_a_half_turn_and_its_own_inverse(self, m):
        for p in [(0, 0), (9000, 3000), (3500, 25500), (17999, 1), (9000, 16000)]:
            assert m.arena_point(*p, False) == list(p)
            turned = m.arena_point(*p, True)
            assert turned == [m.NATIVE_W - p[0], m.NATIVE_H - p[1]]
            assert m.arena_point(*turned, True) == list(p)
        # the two king towers trade places; the centre stays put
        assert m.arena_point(9000, 3000, True) == [9000, 29000]
        assert m.arena_point(9000, 16000, True) == [9000, 16000]

    def test_every_spell_point_turns_with_the_arena_and_its_side_stays(self, m):
        frames = _spell_frames()
        plain = {c["card_id"]: c for c in m.spell_casts(frames)}
        turned = {c["card_id"]: c for c in m.spell_casts(frames, True)}
        assert plain.keys() == turned.keys()
        for cid, a in plain.items():
            b = turned[cid]
            assert b["side"] == a["side"]
            assert b["departures"] == a["departures"]
            for oa, ob in zip(a["tracks"], b["tracks"], strict=True):
                for k in ("launch", "target", "end"):
                    assert ob[k] == m.arena_point(*oa[k], True), (cid, k)
                for k in ("first", "depart", "last_seen", "arrival"):
                    assert ob[k] == oa[k], (cid, k)

    def test_the_aim_is_the_mean_in_the_fixture_s_frame(self, m):
        """A mean taken before the turn rounds the other way: (7401 + 9500 + 11000) / 3 is
        not whole, and the same battle recorded the other way up must still give one aim."""
        frame = {
            "tick": 7,
            "effects": [
                _obj(1, ARROWS, "a", (9000, 29000), (9000, 29000), (9500, 23100)),
                _obj(1, ARROWS, "b", (8000, 29000), (8000, 29000), (7401, 23300)),
                _obj(1, ARROWS, "c", (10000, 29000), (10000, 29000), (11000, 22001)),
            ],
        }
        (plain,) = m.spell_casts([frame])
        assert plain["aim"] == [27901 // 3, 68401 // 3]
        turned_frame = {
            "tick": 7,
            "effects": [
                {
                    **e,
                    "projectile_x": m.NATIVE_W - e["projectile_x"],
                    "projectile_y": m.NATIVE_H - e["projectile_y"],
                }
                for e in frame["effects"]
            ],
        }
        (back,) = m.spell_casts([turned_frame], True)
        assert back["aim"] == plain["aim"]


# ---------------------------------------------------------------------------
# a hand-built battle, built as recorded and turned the other way up

#: The published path grid's width and height in cells (make_replay_fixture.CELL_COLS, _ROWS),
#: written out so the turned twin below states the geometry independently of the maker.
GRID_COLS, GRID_ROWS = 36, 64


def _battle():
    """(header towers, frames) of a small native battle, side 0 defending low y: six
    towers, a Knight walking with its timers running, a Fireball from side 0's king, a
    volley from side 1 whose first-frame targets have a mean that is not a whole number,
    and both sides' elixir. Frames 0, 1, 2, 3, 5, 6 (4 missed)."""
    tower_rows = [
        (1, 0, 9000, 3000, 4824),
        (2, 0, 3500, 6500, 3052),
        (3, 0, 14500, 6500, 3052),
        (4, 1, 9000, 29000, 4824),
        (5, 1, 3500, 25500, 3052),
        (6, 1, 14500, 25500, 3052),
    ]
    header = [{"side": s, "x": x, "y": y} for _, s, x, y, _ in tower_rows]
    frames = []
    for i, t in enumerate([0, 1, 2, 3, 5, 6]):
        ents = [
            {
                "id": f"p{k}",
                "generation_key": k,
                "side": s,
                "x": x,
                "y": y,
                "x2": x,
                "y2": y,
                "card_id": -1,
                "level": 11,
                "kind": 13,
                "hp": hp - (7 * i if k == 5 else 0),
                "max_hp": hp,
                "behavior_state": 0,
                "target": None,
                "path_nodes": [],
                "attack_progress_ms": 50 * i if k == 5 else 0,
                "attack_load_timer_ms": 0,
                "event_timer_ms": 0,
                "attack_component_valid": True,
            }
            for k, s, x, y, hp in tower_rows
        ]
        if i >= 1:
            ents.append(
                {
                    "id": "p7",
                    "generation_key": 7,
                    "side": 0,
                    "x": 3400 + 7 * i,
                    "y": 12000 + 60 * i,
                    "x2": 3400 + 7 * (i - 1),
                    "y2": 12000 + 60 * (i - 1),
                    "card_id": 26000000,
                    "level": 11,
                    "kind": 14 if i < 3 else 15,
                    "hp": 1766,
                    "max_hp": 1766,
                    "behavior_state": 4 if i < 3 else 1,
                    "target": "p5",
                    "path_nodes": [GRID_COLS * 50 + 7, GRID_COLS * 30 + 6 + i],
                    "attack_progress_ms": 1150 + 50 * i,
                    "attack_load_timer_ms": 700 - 50 * i,
                    "event_timer_ms": 250 - 50 * (i % 3),
                    "attack_component_valid": i % 2 == 0,
                }
            )
        effects = []
        if 1 <= t <= 3:  # a Fireball from side 0's king, gone at 5
            k = t - 1
            at, prev = (8800 - 200 * k, 3600 + 600 * k), (9000 - 200 * k, 3000 + 600 * k)
            effects.append(_obj(0, 28000000, "f", at, prev, (5000, 20000)))
        if t >= 2:  # side 1's volley: a and c fly from 2 (a is gone at 5), b waits until 5
            k = [2, 3, 5, 6].index(t)
            if t <= 3:
                effects.append(_obj(1, ARROWS, "a", (9100 + 100 * k, 28000 - 1000 * k), (9000, 29000), (9500, 23100)))
            b_at = (8000, 29000) if t < 5 else (7900 - 100 * (t - 5), 28000 - 1000 * (t - 5))
            effects.append(_obj(1, ARROWS, "b", b_at, (8000, 29000), (7401, 23300)))
            effects.append(_obj(1, ARROWS, "c", (10100 + 100 * k, 28000 - 1000 * k), (10000, 29000), (11000, 22000)))
        frames.append(
            {
                "tick": t,
                "entities": ents,
                "effects": effects,
                # each cast takes its cost off its side on its first frame: the Fireball 4 on 1, the Arrows 3 on 2
                "elixir_raw": [60000 + 178 * t - (40000 if t >= 1 else 0), 70000 + 178 * t - (30000 if t >= 2 else 0)],
            }
        )
    return header, frames


def _turned(header, frames):
    """The same battle with its coordinates turned 180 degrees about the arena's centre, as a
    recording made the other way up would hold it: side 0 now at the top, every point and
    path cell turned, the sides (and so the elixir pair) as they were. NOT the seat symmetry
    (turn AND swap the sides), which is an equivalent battle with side 0 still at the bottom."""
    w, h = 18_000, 32_000

    def ent(e):
        e = dict(e)
        e["x"], e["y"], e["x2"], e["y2"] = w - e["x"], h - e["y"], w - e["x2"], h - e["y2"]
        e["path_nodes"] = [
            (GRID_ROWS - 1 - n // GRID_COLS) * GRID_COLS + (GRID_COLS - 1 - n % GRID_COLS) for n in e["path_nodes"]
        ]
        return e

    def eff(fx):
        fx = dict(fx)
        for a, b in (("x", "y"), ("x2", "y2"), ("projectile_x", "projectile_y")):
            fx[a], fx[b] = w - fx[a], h - fx[b]
        return fx

    towers = [{"side": t["side"], "x": w - t["x"], "y": h - t["y"]} for t in header]
    out = [
        {**f, "entities": [ent(e) for e in f["entities"]], "effects": [eff(x) for x in f["effects"]]} for f in frames
    ]
    return towers, out


def _write_capture(path, header, frames, drop_elixir=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    with gzip.open(path, "wt", encoding="utf-8") as fh:
        fh.write(json.dumps({"record": "header", "towers": header, "local_side_native": 0}) + "\n")
        for f in frames:
            state = {k: v for k, v in f.items() if not (drop_elixir and k == "elixir_raw")}
            fh.write(json.dumps({"record": "frame", "state": state}) + "\n")


@pytest.fixture(scope="module")
def maker_inputs(m):
    with open(CARDS, "rb") as fh:
        raw = fh.read()
    doc = json.loads(raw.decode("utf-8"))
    doc["_fnv1a64"] = m.fnv1a64(raw)
    # load_id_table refuses without the pack; the tests that need it skip (_skip_without_the_id_table)
    id_table = {} if m.missing_id_files() else m.load_id_table()
    card_names = {c["name"] for c in doc["cards"]}
    name_to_id = m.base_ids(id_table, card_names)
    return id_table, doc, name_to_id, card_names


def _skip_without_the_id_table(m) -> None:
    """The Supercell id -> card-name table, and a clean runner has none.

    `make_replay_fixture.load_id_table()` reads `data/raw/cr-15.535.29/csv_logic/` and refuses
    when a file is not there. It used to continue past a missing file and return an empty
    dict, so every recorded deploy resolved to nothing and a test keyed on a card name failed
    with a KeyError that looked like the card was missing from the table. It was not: the card
    is in the published table, and nothing could resolve to it. So these tests ask
    `missing_id_files()` first and skip out loud.
    """
    missing = m.missing_id_files()
    if missing:
        pytest.skip(
            f"SKIPPED, NOT PASSED: {len(missing)} of the id table's files are absent, so no recorded deploy can "
            "resolve to a card name. It is read from data/raw/cr-15.535.29/csv_logic/, the "
            "decoded asset pack, which is excluded on rights grounds and which a CLONE NEVER "
            "HAS -- permanently local coverage rather than a setup step somebody forgot. "
            "Nothing about the fixture maker's deploy or spell publishing has been checked "
            "here."
        )


def _build(m, maker_inputs, path):
    id_table, doc, name_to_id, card_names = maker_inputs
    return m.build(str(path), [], 1, None, None, id_table, doc, {}, name_to_id, card_names, {})


def _decode(col):
    out = []
    for v, run in zip(col[::2], col[1::2], strict=True):
        out.extend([v] * run)
    return out


@needs_modern_cards
def test_the_battle_publishes_timers_elixir_and_spell_objects(m, maker_inputs, tmp_path):
    _skip_without_the_id_table(m)
    header, frames = _battle()
    path = tmp_path / "frames-synthetic.native.oracle.jsonl.gz"
    _write_capture(path, header, frames)
    fx = _build(m, maker_inputs, path)
    truth = fx["truth"]
    assert truth["columns"] == list(m.TRUTH_COLUMNS)
    knight = next(e for e in truth["entities"] if e["key"] == 7)
    raw = [e for f in frames for e in f["entities"] if e["generation_key"] == 7]
    for col in ("attack_progress_ms", "attack_load_timer_ms", "event_timer_ms"):
        assert _decode(knight[col]) == [e[col] for e in raw], col
    assert _decode(knight["attack_component_valid"]) == [int(e["attack_component_valid"]) for e in raw]
    # every value of every column is an integer (or null, or a path cell list): no booleans
    for e in truth["entities"]:
        for col in truth["columns"]:
            assert not any(isinstance(v, bool) for v in e[col]), (e["key"], col)
    assert truth["elixir_raw"] == {
        "0": m.rle([f["elixir_raw"][0] for f in frames]),
        "1": m.rle([f["elixir_raw"][1] for f in frames]),
    }
    spells = {d["card"]: d for d in fx["deploys"] if d["kind"] == "spell"}
    # NAMED, not subscripted. A bare `spells["Fireball"]` can only RAISE, and a KeyError
    # cannot tell you whether the spell is absent, renamed, or resolved to something else --
    # three causes with three different repairs. This failure says which in one run.
    for want in ("Fireball", "Arrows"):
        assert want in spells, f"the battle published {sorted(spells)}, with no {want}"
    # the Fireball's launch is side 0's king tower centre, as published in `towers`
    king0 = next(t for t in fx["towers"] if t["side"] == 0 and t["slot"] == 0)
    assert spells["Fireball"]["objects"][0]["launch"] == [king0["x"], king0["y"]]
    assert spells["Fireball"]["objects"][0]["arrival"] == 5
    assert spells["Arrows"]["departures"] == [[2, 2], [5, 1]]
    assert spells["Arrows"]["pos"] == [(9500 + 7401 + 11000) // 3, (23100 + 23300 + 22000) // 3]
    # a capture without the elixir pair: no column, and nothing else changes
    bare = tmp_path / "bare" / "frames-synthetic.native.oracle.jsonl.gz"
    _write_capture(bare, header, frames, drop_elixir=True)
    fb = _build(m, maker_inputs, bare)
    assert "elixir_raw" not in fb["truth"]
    fb["truth"]["elixir_raw"] = truth["elixir_raw"]
    assert m.comparable(fb) == m.comparable(fx)


@needs_modern_cards
def test_a_spell_object_with_no_elixir_drop_is_a_release_not_a_cast(m, maker_inputs, tmp_path):
    """A spell object whose side's elixir never falls by the card's cost is a unit's release, not a
    cast: on the client 16.402 corpus the Heal Spirit's kamikaze projectile carries the Heal card's id
    and no elixir leaves the pool for it, while every cast of the corpus takes its cost on its first
    frame. It is not played, and `unresolved` says why. Here: the same battle with side 1's drop
    removed keeps its Fireball and loses its Arrows."""
    _skip_without_the_id_table(m)
    header, frames = _battle()
    for f in frames:
        f["elixir_raw"] = [f["elixir_raw"][0], 70000 + 178 * f["tick"]]
    path = tmp_path / "frames-synthetic.native.oracle.jsonl.gz"
    _write_capture(path, header, frames)
    fx = _build(m, maker_inputs, path)
    spells = sorted(d["card"] for d in fx["deploys"] if d["kind"] == "spell")
    assert spells == ["Fireball"], f"the battle published {spells}"
    why = [u["why"] for u in fx["unresolved"] if u["card"] == "Arrows"]
    assert why, f"no release recorded for the Arrows: {fx['unresolved']}"
    assert "no elixir drop" in why[0], why[0]


@needs_cards
def test_one_battle_is_one_fixture_whichever_way_up_it_was_recorded(m, maker_inputs, tmp_path):
    """The rotated branch of build(), end to end: the same battle recorded with side 0 at the
    top must give the same fixture as recorded with side 0 at the bottom -- every position,
    side, path cell, spell point, aim and elixir column -- except the `frame` note that says
    which way it came in and the header's native `local_side_native`."""
    _skip_without_the_id_table(m)
    header, frames = _battle()
    a = tmp_path / "a" / "frames-synthetic.native.oracle.jsonl.gz"
    b = tmp_path / "b" / "frames-synthetic.native.oracle.jsonl.gz"
    _write_capture(a, header, frames)
    _write_capture(b, *_turned(header, frames))
    fa, fb = _build(m, maker_inputs, a), _build(m, maker_inputs, b)
    assert fa["frame"]["transform"] == "identity"
    assert fb["frame"]["transform"].startswith("rotate180")
    # the convention the turn exists for: side 0 (Blue) defends low y in the fixture
    kings = {t["side"]: t["y"] for t in fb["towers"] if t["slot"] == 0}
    assert kings[0] < kings[1]
    assert fb["frame"]["blue_native_side"] == 0
    skip = ("frame", "local_side_native")
    ra = {k: v for k, v in fa.items() if k not in skip}
    rb = {k: v for k, v in fb.items() if k not in skip}
    for k in ra:
        assert rb[k] == ra[k], k
    assert rb == ra


GOBLINS, KNIGHT = 26000002, 26000000


def _late_battle():
    """(header towers, frames) of a battle whose capture SHOWS two Goblins groups late, as both seats of
    client 16.402's 20260920-010218 show side 1's Goblins (seat B from 572, seat A from 577; seat A
    shows their deploys end on 588, 592, 596 and 600: spawn 569, stagger 200 ms = 4 ticks).

    Frames 0..90 with 28 and 55 missed. Side 1's Goblins, keys 20-23: spawned on 9, shown from 12
    with no frame missed, deploys ending on 28 (missed: seen 27 -> 29), 32, 36, 40. Keys 30-33:
    spawned on 50, shown from 56 (55 missed), ends 69, 73, 77, 81. Member i > 0 waits in state 11
    (kind 12) and turns 4 on spawn + 4i - 1, as the capture's third and fourth do (576, 580).
    Controls, side 0: a Knight shown on its spawn 5 (end 24), and one shown on 60 whose deploy ends
    a tick late (80), which must not move it LATER. Side 1's elixir drops 2 on each Goblins group's
    first shown frame."""
    tower_rows = [
        (1, 0, 9000, 3000, 4824),
        (2, 0, 3500, 6500, 3052),
        (3, 0, 14500, 6500, 3052),
        (4, 1, 9000, 29000, 4824),
        (5, 1, 3500, 25500, 3052),
        (6, 1, 14500, 25500, 3052),
    ]
    header = [{"side": s, "x": x, "y": y} for _, s, x, y, _ in tower_rows]

    def ent(key, side, cid, x, y, hp, kind, state):
        return {"id": f"p{key}", "generation_key": key, "side": side, "x": x, "y": y, "card_id": cid,
                "level": 11, "kind": kind, "hp": hp, "max_hp": hp, "behavior_state": state, "target": None,
                "path_nodes": []}

    def goblin(key, i, spawn, shown, end0, t):
        if t < shown:
            return None
        start = spawn + 4 * i - 1 if i else spawn
        if t < start:
            kind, state = 12, 11
        elif t < end0 + 4 * i:
            kind, state = 14, 4
        else:
            kind, state = 15, 1
        return ent(key, 1, GOBLINS, 8500 + 700 * (i % 2), 30000 + 700 * (i // 2), 202, kind, state)

    def knight(key, shown, end, t):
        if t < shown:
            return None
        return ent(key, 0, KNIGHT, 3500, 12000, 1766, 14 if t < end else 15, 4 if t < end else 1)

    frames = []
    for t in [t for t in range(91) if t not in (28, 55)]:
        ents = [ent(k, s, -1, x, y, hp, 13, 0) for k, s, x, y, hp in tower_rows]
        for i in range(4):
            ents.append(goblin(20 + i, i, 9, 12, 28, t))
            ents.append(goblin(30 + i, i, 50, 56, 69, t))
        ents += [knight(40, 5, 24, t), knight(41, 60, 80, t)]
        frames.append({
            "tick": t,
            "entities": [e for e in ents if e is not None],
            "effects": [],
            "elixir_raw": [60000 + 178 * t, 50000 + 178 * t - 20000 * ((t >= 12) + (t >= 56))],
        })
    return header, frames


def test_the_first_deploy_end_decides_a_late_shown_group_only_where_the_stagger_agrees(m):
    """`shown_late_spawn` on its own: members listed by key, their deploys ending in turn."""
    ticks = list(range(100, 150))

    def member(first, end):  # seen from `first` in state 4, walking from `end`
        return [(t - 100, 4 if t < end else 1) for t in range(first, 150)]

    # shown from 112 (111 seen), deploy ends 128, 132, 136: spawn 109, with or without the stagger
    group = [member(112, 128), member(112, 132), member(112, 136)]
    assert m.shown_late_spawn(ticks, 12, group, 1000, 200)[0] == 109
    assert m.shown_late_spawn(ticks, 12, group, 1000)[0] == 109
    # the first end seen across a missed frame (128) leaves [109, 110]; the others pin 109. Where one
    # does not sit at its rank's stagger (the third ends on 140, where a fourth would), the range stands
    gap = [t for t in ticks if t != 128]
    ix = {t: k for k, t in enumerate(gap)}

    def member_gap(first, end):
        return [(ix[t], 4 if t < end else 1) for t in gap if t >= first]

    ok = [member_gap(112, 129), member_gap(112, 132), member_gap(112, 136)]
    tick, why = m.shown_late_spawn(gap, ix[112], ok, 1000, 200)
    assert (tick, why.startswith("exact")) == (109, True), why
    bad = [member_gap(112, 129), member_gap(112, 132), member_gap(112, 140)]
    tick, why = m.shown_late_spawn(gap, ix[112], bad, 1000, 200)
    assert (tick, why.startswith("range [109, 110]")) == (110, True), why
    # not late: the end puts the spawn inside the frame gap, or after the first frame
    assert m.shown_late_spawn(ticks, 12, [member(112, 131)], 1000) is None
    assert m.shown_late_spawn(ticks, 12, [member(112, 132)], 1000) is None


def test_a_late_group_is_timed_by_the_member_whose_deploy_ends_first_whatever_the_key_order(m):
    """`shown_late_spawn` sorts the members' deploy ends before it reads the first. Listed by key, the member whose
    deploy ends first need not come first: here the first listed ends on 132, the second on 128. The spawn is still
    109 (128 - 19), narrowed by the stagger to exactly that; read in key order it would be 113, not late at all."""
    ticks = list(range(100, 150))

    def member(first, end):
        return [(t - 100, 4 if t < end else 1) for t in range(first, 150)]

    out_of_order = [member(112, 132), member(112, 128), member(112, 136)]
    tick, why = m.shown_late_spawn(ticks, 12, out_of_order, 1000, 200)
    assert (tick, why.startswith("exact")) == (109, True), why


def test_a_group_whose_first_deploy_end_is_a_waiting_member_is_not_timed_late(m):
    """The first deploy end decides only when that member was first seen deploying (behavior_state 4). A member first
    seen still waiting out its stagger (state 11) started its own deploy later than the spawn, so its end says nothing
    about the spawn: the group is not timed late. Read as a deploying member, its end on 128 would give a spawn on
    109, three ticks before the frame gap allows."""
    ticks = list(range(100, 150))
    waiting = [(t - 100, m.STATE_STAGGER_WAIT if t < 115 else (4 if t < 128 else 1)) for t in range(112, 150)]
    assert m.shown_late_spawn(ticks, 12, [waiting], 1000, 200) is None
    deploying = [(t - 100, 4 if t < 128 else 1) for t in range(112, 150)]
    assert m.shown_late_spawn(ticks, 12, [deploying], 1000, 200)[0] == 109, "the scene no longer separates the two"


@needs_modern_cards
def test_a_group_the_capture_shows_late_is_deployed_on_its_spawn(m, maker_inputs, tmp_path):
    """A capture can show a deploy group some ticks after its spawn with no frame missed, so the frame
    gap does not bound the spawn; the group's deploy ends do (client 16.402, 20260920-010218: side 1's
    Goblins spawned on 569, shown on 572 by one seat and on 577 by the other). The late-shown groups take
    their spawn; a group shown on its spawn, and one whose deploy ends a tick late, keep their first
    frame; and a late-shown group's elixir drop, on the frame the capture first shows it, stays
    explained, so a same-cost tap with no drop of its own does not claim it."""
    _skip_without_the_id_table(m)
    header, frames = _late_battle()
    path = tmp_path / ("frames-synthetic" + m.CAPTURE_SUFFIX)
    _write_capture(path, header, frames)
    log = tmp_path / "placements-synthetic.jsonl"
    log.write_text(
        json.dumps({"tick": 0, "local_side_native": 1}) + "\n"
        + json.dumps({"tick": 3, "card": "Zap", "requested": [9.5, 11.5]}) + "\n",
        encoding="utf-8",
    )
    id_table, doc, name_to_id, card_names = maker_inputs
    fx = m.build(str(path), [str(log)], 1, None, None, id_table, doc, {}, name_to_id, card_names, {})
    rows = {(d["card"], d["keys"][0]): d for d in fx["deploys"] if d["keys"]}
    got = {k: (d["tick"], d["first_seen"]) for k, d in rows.items()}
    assert got == {
        ("Goblins", 20): (9, 12),
        ("Goblins", 30): (50, 56),
        ("Knight", 40): (5, 5),
        ("Knight", 41): (60, 60),
    }, got
    assert rows[("Goblins", 20)]["tick_evidence"].startswith("exact (deploy-end transition"), rows[("Goblins", 20)]
    # side 1 is the log's side: its plays keep what their units show, never a tile centre (AN UNLOGGED SIDE'S TROOP)
    assert (rows[("Goblins", 20)]["pos"], rows[("Goblins", 20)]["source"]) == ([8850, 30350], "centroid")
    assert [d["card"] for d in fx["deploys"] if d["kind"] == "spell"] == []
    assert [u["card"] for u in fx["unresolved"]] == ["Zap"], fx["unresolved"]


# ---------------------------------------------------------------------------
# a group with no tap and a clamped member (the maker's module doc: RECOVERED TILE)

#: The ring offsets from the tap tile's centre that the committed formation measurement shows (native): side 1's
#: Skeletons, side 0's Goblins, side 0's Minions.
SKELETONS_RING_SIDE1 = [(-700, 402), (-1, -808), (698, 402)]
GOBLINS_RING_SIDE0 = [(-762, -761), (-762, 761), (760, -761), (760, 761)]
MINIONS_RING_SIDE0 = [(-499, -288), (0, 579), (499, -288)]
#: Client 16.402, 20260918-122757.b1: side 1's Skeletons deployed on 494 with no logged tap, at their CREATION POINTS
#: (the first frame's x2, y2). The front member stands on its ring point; the two back members are clamped to side 1's
#: back bound, y 31000. Their centroid, (8499, 30897), lies in the tile below the one they were laid around.
SKELETONS_KING_BACK = [(8499, 30692), (7800, 31000), (9198, 31000)]
#: 20260918-130203.b2: side 0's Goblins deployed on 397 with no logged tap, laid around (500, 10500); the two left
#: members are clamped to the arena's bound, x 250. Their centroid is (755, 10500).
GOBLINS_LEFT_EDGE = [(250, 9739), (250, 11261), (1260, 11261), (1260, 9739)]


def test_a_group_with_a_clamped_member_is_placed_on_the_tile_its_members_agree_on(m):
    tile, why = m.recovered_tile(SKELETONS_KING_BACK, SKELETONS_RING_SIDE1)
    assert tile == [8500, 31500], f"placed on {tile} ({why}); the centroid (8499, 30897) is in the tile below"
    assert why == "1 of 3 members on a nominal offset from it, 2 clamped on one axis", why
    tile, why = m.recovered_tile(GOBLINS_LEFT_EDGE, GOBLINS_RING_SIDE0)
    assert tile == [500, 10500], f"placed on {tile} ({why}); the centroid is (755, 10500)"
    assert why == "2 of 4 members on a nominal offset from it, 2 clamped on one axis", why


def test_no_tile_is_recovered_where_the_members_do_not_single_one_out(m):
    """The recovery answers only where exactly one tile centre explains the members; every other group keeps its
    centroid, and the reason says why."""
    # a capture's opening frame (20260918-121158, tick 335): three Skeletons already walking, none on a ring point
    tile, why = m.recovered_tile([(4953, 26633), (5325, 25576), (13002, 26845)], SKELETONS_RING_SIDE1)
    assert tile is None, why
    assert why.startswith("no member's creation point"), why
    # a Minion on (499, -288) from (3500, 14500) is on (-501, -288) from (4500, 14500), and its sibling is on a nominal
    # offset from neither: two tile centres with one member each
    tile, why = m.recovered_tile([(3999, 14212), (3500, 15000)], MINIONS_RING_SIDE0)
    assert tile is None, why
    assert why.startswith("2 tile centres tie"), why
    # one member on its ring point and one on no nominal offset on either axis (moved before its first frame): one
    # member is not enough unless every other one is clamped
    tile, why = m.recovered_tile([(8499, 30692), (9000, 30000)], SKELETONS_RING_SIDE1)
    assert tile is None, why
    assert why.startswith("the best tile centre (8500, 31500) has 1 of 2"), why
    # a card the measurement has no groups for
    tile, why = m.recovered_tile(GOBLINS_LEFT_EDGE, [])
    assert tile is None, why
    assert why == "no nominal offsets for this card and side", why


def test_the_nominal_offsets_are_the_committed_measurement_s_centred_groups_both_ways_up(m):
    """nominal_offsets() keeps a measured group only when its members stand centred on its tap, and gives each side the
    other side's offsets turned a half-turn."""
    doc = {
        "groups": [
            # centred on its tap: kept, and turned for side 1
            {"card": "Goblins", "side": 0, "source": "tap_tile",
             "members": [{"offset": list(o)} for o in GOBLINS_RING_SIDE0]},
            # laid a tile below its logged tap (a tap moved off a tower): left out
            {"card": "Skeletons", "side": 0, "source": "tap_tile",
             "members": [{"offset": [-1, -193]}, {"offset": [699, -1403]}, {"offset": [-700, -1403]}]},
            # a centroid-sourced group has no tap to measure from: left out
            {"card": "Minions", "side": 0, "source": "centroid",
             "members": [{"offset": list(o)} for o in MINIONS_RING_SIDE0]},
        ]
    }
    got = m.nominal_offsets(doc)
    assert got.get(("Goblins", 0)) == sorted(GOBLINS_RING_SIDE0), got
    assert got.get(("Goblins", 1)) == sorted((-x, -y) for x, y in GOBLINS_RING_SIDE0), f"no half-turn for side 1: {got}"
    assert ("Skeletons", 0) not in got, got
    assert ("Minions", 0) not in got, got
    # the committed measurement gives the rings the tests above quote
    committed = m.load_nominal_offsets()
    for key, ring in ((("Skeletons", 1), SKELETONS_RING_SIDE1), (("Goblins", 0), GOBLINS_RING_SIDE0),
                      (("Minions", 0), MINIONS_RING_SIDE0)):
        assert set(ring) <= set(committed[key]), (key, committed.get(key))


def _clamped_battle():
    """(header towers, frames) of a battle with three Goblins groups and no placements log, frames 0..30. Side 0's at
    the arena's left edge from 5: laid around (500, 10500), two members clamped to x 250 (GOBLINS_LEFT_EDGE, as
    20260918-130203.b2 t397 shows them). Side 0's mid-field from 12: laid around (3500, 8500), every member on its ring
    point, the first one's first frame already pushed 150 by its first tick's contact push. Side 1's from 20: four
    members on no ring point."""
    tower_rows = [
        (1, 0, 9000, 3000, 4824),
        (2, 0, 3500, 6500, 3052),
        (3, 0, 14500, 6500, 3052),
        (4, 1, 9000, 29000, 4824),
        (5, 1, 3500, 25500, 3052),
        (6, 1, 14500, 25500, 3052),
    ]
    header = [{"side": s, "x": x, "y": y} for _, s, x, y, _ in tower_rows]

    def ent(key, side, cid, x, y, hp, kind, state, x2=None, y2=None):
        return {"id": f"p{key}", "generation_key": key, "side": side, "x": x, "y": y,
                "x2": x if x2 is None else x2, "y2": y if y2 is None else y2, "card_id": cid, "level": 11,
                "kind": kind, "hp": hp, "max_hp": hp, "behavior_state": state, "target": None, "path_nodes": []}

    mid = [(2738, 7739), (2738, 9261), (4260, 9261), (4260, 7739)]
    walking = [(6000, 20000), (6400, 20300), (7000, 21000), (7300, 20100)]
    frames = []
    for t in range(31):
        ents = [ent(k, s, -1, x, y, hp, 13, 0) for k, s, x, y, hp in tower_rows]
        if t >= 5:
            ents += [ent(20 + i, 0, GOBLINS, x, y, 202, 14, 4) for i, (x, y) in enumerate(GOBLINS_LEFT_EDGE)]
        if t >= 12:
            for i, (x, y) in enumerate(mid):
                if t == 12 and i == 0:  # the first frame: x, y after the push, x2, y2 the creation point
                    ents.append(ent(30, 0, GOBLINS, x - 150, y, 202, 14, 4, x, y))
                else:
                    ents.append(ent(30 + i, 0, GOBLINS, x - (150 if i == 0 else 0), y, 202, 14, 4))
        if t >= 20:
            ents += [ent(40 + i, 1, GOBLINS, x, y, 202, 14, 4) for i, (x, y) in enumerate(walking)]
        frames.append({"tick": t, "entities": ents, "effects": [], "elixir_raw": [50000 + 178 * t, 50000 + 178 * t]})
    return header, frames


@needs_modern_cards
def test_a_group_with_no_tap_is_played_on_its_recovered_tile(m, maker_inputs, tmp_path):
    """Built end to end: a group with no tap whose members agree on one tile centre is played there (`source`
    recovered_tile), its `centroid` kept as it was; a group whose members agree on none is left on its centroid,
    `recovery` says why, and as no log covers its side it is played on that centroid's tile centre (module doc, AN
    UNLOGGED SIDE'S TROOP). Before the recovery the left-edge group was played at its centroid, (755, 10500), and the
    engine laid the whole ring 255 to the right of where the game laid it."""
    _skip_without_the_id_table(m)
    header, frames = _clamped_battle()
    path = tmp_path / ("frames-synthetic" + m.CAPTURE_SUFFIX)
    _write_capture(path, header, frames)
    fx = _build(m, maker_inputs, path)
    rows = {d["keys"][0]: d for d in fx["deploys"] if d["keys"]}
    edge, mid, walking = rows[20], rows[30], rows[40]
    assert (edge["pos"], edge["source"]) == ([500, 10500], "recovered_tile"), (
        f"the left-edge Goblins are played at {edge['pos']} ({edge['source']}), not on the tile the game laid them on"
    )
    assert edge["centroid"] == [755, 10500], edge
    assert (mid["pos"], mid["source"]) == ([3500, 8500], "recovered_tile"), mid
    assert mid["centroid"] == [(2738 - 150 + 2738 + 4260 + 4260) // 4, 8500], mid
    assert walking["centroid"] == [6675, 20350], walking
    assert (walking["pos"], walking["source"]) == ([6500, 20500], "tile_centre"), walking
    assert walking["recovery"].startswith("no member's creation point"), walking


def test_replay_formations_reads_a_recovered_group_at_its_centroid():
    """The formation measurement never reads a tile the recovery chose: tools/replay_formations.py reports a
    `recovered_tile` group at its centroid, as a centroid group, so the offsets it measures are not the nominal offsets
    that chose the tile."""
    rf = _load_formations()

    def goblin(key, x, y):
        return {"key": key, "card_id": GOBLINS, "t0": 0, "n": 2, "x": [x, 2], "y": [y, 2], "hp": [202, 2],
                "target": [-1, 2], "path_n": [0, 2], "state": [4, 1, 1, 1]}

    fx = {
        "capture": "synthetic",
        "truth": {"ticks": [5, 6], "entities": [goblin(20 + i, x, y) for i, (x, y) in enumerate(GOBLINS_LEFT_EDGE)]},
        "deploys": [{"tick": 5, "side": 0, "card": "Goblins", "kind": "troop", "count": 4, "keys": [20, 21, 22, 23],
                     "pos": [500, 10500], "centroid": [755, 10500], "source": "recovered_tile"}],
    }
    (g,) = rf.group_rows(fx)
    assert (g["pos"], g["source"]) == ([755, 10500], "centroid"), (g["pos"], g["source"])
    assert [mm["offset"] for mm in g["members"]] == [[x - 755, y - 10500] for x, y in GOBLINS_LEFT_EDGE]


# ---------------------------------------------------------------------------
# a single unit pushed on its creation tick (the maker's module doc: CREATION POINT)


def _pushed_single_battle():
    """(header towers, frames) of a battle with five single Knights, frames 0..30 with 11 missed.

    Key 50, side 0: created on 5 at (3499, 14500) and pushed 150 on that tick, so its first frame stands at
    (3589, 14380) (client 16.402, 20260918-112751 t1202). Key 51, side 0: first seen on 12, after the missed 11, at
    (9394, 5394), its x2, y2 at (9500, 5500): it may have been created on 11. Key 52, side 1: on the board from the
    first frame, walking. Key 53, side 1: created on the missed 11 on (5499, 25499) and pushed on 11 and 12, first seen
    on (5660, 25656), its x2, y2 (5610, 25601) (client 16.402, 20260918-122757.b1's Bomber of t588). Key 54, side 1,
    a Giant: first seen on 12 on the king's back row at (9500, 31000), where a tower tap puts a troop, unpushed."""
    tower_rows = [
        (1, 0, 9000, 3000, 4824),
        (2, 0, 3500, 6500, 3052),
        (3, 0, 14500, 6500, 3052),
        (4, 1, 9000, 29000, 4824),
        (5, 1, 3500, 25500, 3052),
        (6, 1, 14500, 25500, 3052),
    ]
    header = [{"side": s, "x": x, "y": y} for _, s, x, y, _ in tower_rows]

    def ent(key, side, cid, xy, prev, hp, kind, state):
        return {"id": f"p{key}", "generation_key": key, "side": side, "x": xy[0], "y": xy[1], "x2": prev[0],
                "y2": prev[1], "card_id": cid, "level": 11, "kind": kind, "hp": hp, "max_hp": hp,
                "behavior_state": state, "target": None, "path_nodes": []}

    frames = []
    for t in [t for t in range(31) if t != 11]:
        ents = [ent(k, s, -1, (x, y), (x, y), hp, 13, 0) for k, s, x, y, hp in tower_rows]
        if t >= 5:
            ents.append(ent(50, 0, KNIGHT, (3589, 14380), (3499, 14500) if t == 5 else (3589, 14380), 1766, 14, 4))
        if t >= 12:
            ents.append(ent(51, 0, KNIGHT, (9394, 5394), (9500, 5500) if t == 12 else (9394, 5394), 1766, 14, 4))
            ents.append(ent(53, 1, KNIGHT, (5660, 25656), (5610, 25601) if t == 12 else (5660, 25656), 1766, 14, 4))
            # a Giant (26000003, 3968 hp at level 11), so it is its own group beside key 53
            ents.append(ent(54, 1, 26000003, (9500, 31000), (9500, 31000), 3968, 14, 4))
        ents.append(ent(52, 1, KNIGHT, (6000, 20000 - 10 * t), (6000, 20010 - 10 * t), 1766, 15, 1))
        frames.append({"tick": t, "entities": ents, "effects": [], "elixir_raw": [50000 + 178 * t, 50000 + 178 * t]})
    return header, frames


@needs_modern_cards
def test_a_single_unit_pushed_on_its_creation_tick_is_played_where_it_was_created(m, maker_inputs, tmp_path):
    """A single unit's first frame carries its first tick's contact push; its x2, y2 is where it was created. It is
    played there when the capture saw its creation tick; when a missed frame leaves that tick unseen it is played on its
    tile's laid point (LATE SINGLE, `test_a_single_created_on_a_missed_tick_is_played_on_its_tiles_laid_point`), and a
    unit that was not deploying keeps its first frame."""
    _skip_without_the_id_table(m)
    header, frames = _pushed_single_battle()
    path = tmp_path / ("frames-synthetic" + m.CAPTURE_SUFFIX)
    _write_capture(path, header, frames)
    fx = _build(m, maker_inputs, path)
    rows = {d["keys"][0]: d for d in fx["deploys"] if d["keys"]}
    pushed, unseen, walking = rows[50], rows[51], rows[52]
    assert (pushed["pos"], pushed["source"]) == ([3499, 14500], "creation_point"), (
        f"the Knight pushed on its creation tick is played at {pushed['pos']} ({pushed['source']}), its first frame"
    )
    assert pushed["centroid"] == [3589, 14380], pushed
    assert unseen["tick"] < unseen["first_seen"], f"the scene drifted: key 51 is dated on its first frame {unseen}"
    assert (unseen["pos"], unseen["source"]) == ([9500, 5500], "laid_point"), unseen
    assert (walking["pos"], walking["source"]) == (walking["centroid"], "centroid"), walking


@needs_modern_cards
def test_a_single_created_on_a_missed_tick_is_played_on_its_tiles_laid_point(m, maker_inputs, tmp_path):
    """A single unit created on a tick the capture missed was pushed before its first frame, so that frame's x2, y2
    is already off its creation point. It is played on the laid point of the tile its x2, y2 lies in (one unit lower
    in x on the left half and in y for side 1), when that point is off its first frame and within a push's reach of
    x2, y2; a unit that stands where no tile's laid point is (the king's back row a tower tap puts a troop on) keeps
    its first frame. Not vacuous: the old reading played key 53 on its first frame, 161 and 157 off."""
    _skip_without_the_id_table(m)
    header, frames = _pushed_single_battle()
    path = tmp_path / ("frames-synthetic" + m.CAPTURE_SUFFIX)
    _write_capture(path, header, frames)
    fx = _build(m, maker_inputs, path)
    rows = {d["keys"][0]: d for d in fx["deploys"] if d["keys"]}
    late, back_row = rows[53], rows[54]
    assert late["tick"] < late["first_seen"], f"the scene drifted: key 53 is dated on its first frame {late}"
    assert late["centroid"] == [5660, 25656], late
    assert (late["pos"], late["source"]) == ([5499, 25499], "laid_point"), late
    assert "(5610, 25601)" in late["recovery"], late["recovery"]
    assert "(5499, 25499)" in late["recovery"], late["recovery"]
    assert (back_row["pos"], back_row["source"]) == ([9500, 31000], "centroid"), back_row
    # the rule itself: the tile's laid point by side, half and layer, and the reach
    assert m.late_single_point((5610, 25601), 1, False, 1)[0] == [5499, 25499]
    assert m.late_single_point((5610, 25601), 1, True, 1)[0] == [5500, 25500]
    assert m.late_single_point((9394, 5394), 0, False, 1)[0] == [9500, 5500]
    assert m.late_single_point((9500, 31000), 1, False, 1)[0] is None
    assert m.late_single_point((9500, 31000), 1, False, 4)[0] == [9500, 31499]


@needs_cards
def test_a_spells_level_is_read_off_its_first_hit(m):
    """A cast carries no level in the captures. A damaging spell's level is the one whose damage its first hit took
    (20260918-112751: a Fireball took 357 + the hut's decay tick 1 from a Goblin Hut, the ladder's level 4, where the
    side mode 3 plays 325). A hit no level fits keeps the side mode; a tower's drop and the hut's own decay tick
    before the hit (531 -> 530) are never read."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    fb = cards["Fireball"]
    assert (m.spell_damage_at(doc, fb, 3), m.spell_damage_at(doc, fb, 4)) == (325, 357)

    def row(x, y, hp):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = x, y, hp
        return tuple(r)

    ticks = [100, 101, 102, 103]
    ents = {
        7: {"key": 7, "side": 1, "card_id": 27000001},  # a hut in the blast
        8: {"key": 8, "side": 1, "card_id": -1},  # a tower in the blast: never read
        9: {"key": 9, "side": 0, "card_id": 26000000},  # the caster's own unit
    }

    def rows(hut_after):
        return [
            {7: row(6500, 17500, 531), 8: row(6500, 18000, 3000), 9: row(6500, 17000, 600)},
            {7: row(6500, 17500, 530), 8: row(6500, 18000, 3000), 9: row(6500, 17000, 600)},
            {7: row(6500, 17500, hut_after), 8: row(6500, 18000, 2900), 9: row(6500, 17000, 600)},
            {7: row(6500, 17500, hut_after - 1), 8: row(6500, 18000, 2900), 9: row(6500, 17000, 600)},
        ]

    def cast():
        return {"tick": 100, "side": 0, "card": "Fireball", "kind": "spell", "level": 3,
                "level_source": "side mode", "pos": [6500, 17500]}

    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, rows(530 - 358), ticks)
    assert (d["level"], d["level_source"]) == (4, "damage"), d
    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, rows(530 - 326), ticks)
    assert (d["level"], d["level_source"]) == (3, "side mode"), d
    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, rows(530 - 500), ticks)
    assert (d["level"], d["level_source"]) == (3, "side mode"), "no level fits a 500 drop: the side mode stays"
    assert "fitting levels []" in d["level_evidence"]

    # A KILL BOUNDS THE LEVEL (item 58, 20260918-115249.b1): the hut, 358 before the hit, is gone on the hit's frame;
    # level 3's 326 cannot have killed it, level 4's 357 (with the hut's decay tick) can.
    def killed(last):
        r = rows(0)
        r[1] = {7: row(6500, 17500, last), 8: row(6500, 18000, 3000), 9: row(6500, 17000, 600)}
        r[2] = {8: row(6500, 18000, 2900), 9: row(6500, 17000, 600)}
        r[3] = dict(r[2])
        return r

    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, killed(358), ticks)
    assert (d["level"], d["level_source"]) == (4, "damage"), d
    assert "kills of last hp [358]" in d["level_evidence"], d["level_evidence"]
    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, killed(300), ticks)
    assert (d["level"], d["level_source"]) == (3, "side mode"), "a kill the side mode's 326 covers keeps it"

    # ONE LEVEL PER CARD PER SIDE (20260918-112751's side 0: Fireballs read at 4, 4, and a third unread): a cast whose
    # hit is not read takes the side's read level of that card; the other side's unread Fireball keeps its own mode.
    read = [cast(), cast()]
    far = dict(cast(), pos=[1000, 1000])
    other = dict(cast(), side=1, level=2, pos=[1000, 1000])
    m.spell_levels_from_damage([*read, far, other], doc, cards, ents, rows(530 - 358), ticks)
    assert [(d["level"], d["level_source"]) for d in read] == [(4, "damage"), (4, "damage")]
    assert (far["level"], far["level_source"]) == (4, "card level"), far
    assert "levels {4: 2}" in far["level_evidence"], far["level_evidence"]
    assert (other["level"], other["level_source"]) == (2, "side mode"), "the other side's cast took side 0's level"


@needs_cards
def test_a_spells_level_is_one_its_card_can_be_played_at(m):
    """A Log is a Legendary (unified 9 to 16) whose damage scales on the Common ladder from 1, so its level-8 damage
    exists on the ladder but no Log lands it (live 20261007-005625-A read Logs at 2, 6 and 8, and the harness dropped
    each row as a level the card does not have). Only the card's own levels are tried, and a drop below its least
    playable damage is no hit. A tie between read levels goes to the one nearest the side mode."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    log = cards["Log"]
    assert m.playable_levels(doc, log) == range(9, 18), "9 to 16, and one past the count (the live 17)"
    assert m.playable_levels(doc, cards["Fireball"]) == range(3, 18)
    ladder = doc["rarities"][log["level_scaling"].get("ladder_rarity") or log["level_scaling"]["rarity"]]
    assert m.spell_damage_at(doc, log, 17) == log["damage"] * ladder["unused_tail"][0] // 100, "17 on its ladder's next"
    assert m.spell_damage_at(doc, log, 18) is None
    at8, at11 = m.spell_damage_at(doc, log, 8), m.spell_damage_at(doc, log, 11)
    assert at8 is not None, "the ladder holds a level 8 the card does not"
    assert at8 < m.spell_damage_at(doc, log, 9)

    def row(hp):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = 6500, 17500, hp
        return tuple(r)

    ticks = [100, 101, 102]
    ents = {7: {"key": 7, "side": 1, "card_id": 26000000}}

    def rows(drop):
        return [{7: row(2000)}, {7: row(2000)}, {7: row(2000 - drop)}]

    def cast(level=11, pos=(6500, 17500)):
        return {"tick": 100, "side": 0, "card": "Log", "kind": "spell", "level": level,
                "level_source": "side mode", "pos": list(pos)}

    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, rows(at8), ticks)
    assert (d["level"], d["level_source"]) == (11, "side mode"), d
    d = cast(level=12)
    m.spell_levels_from_damage([d], doc, cards, ents, rows(at11), ticks)
    assert (d["level"], d["level_source"]) == (11, "damage"), d

    # One cast read at 10, one at 12 (two victims far apart), a third unread: it takes the read level nearest its side
    # mode 12, not the lowest.
    at10, at12 = m.spell_damage_at(doc, log, 10), m.spell_damage_at(doc, log, 12)

    def row_at(x, hp):
        r = list(row(hp))
        r[m.TRUTH_COLUMNS.index("x")] = x
        return tuple(r)

    two = {**ents, 8: {"key": 8, "side": 1, "card_id": 26000000}}
    still = {7: row_at(3500, 2000), 8: row_at(14500, 2000)}
    hit = [still, still, {7: row_at(3500, 2000 - at10), 8: row_at(14500, 2000 - at12)}]
    a, b, far = cast(12, (3500, 17500)), cast(12, (14500, 17500)), cast(12, (9000, 31000))
    m.spell_levels_from_damage([a, b, far], doc, cards, two, hit, ticks)
    assert (a["level"], b["level"]) == (10, 12), (a, b)
    assert (far["level"], far["level_source"]) == (12, "card level"), far


def test_a_key_the_capture_reused_for_another_unit_is_split(m):
    """005517-A's key 22: a Skeleton last seen at (13690, 8449) and seen again 15 ticks later at (12457, 28019), another
    Skeleton. The later rows go to a new key, a target naming key 22 from then on names the new key, and a unit seen
    again near where it was (a short gap) stays one entity."""

    def row(x, y, target=-1):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")] = x, y
        r[m.TRUTH_COLUMNS.index("hp")], r[m.TRUTH_COLUMNS.index("target")] = 81, target
        return tuple(r)

    ticks = [936, 937, 938, 953, 954, 960, 961, 963]
    per_tick_rows = [
        {22: row(13750, 8617), 30: row(9000, 9000, 22)},
        {22: row(13720, 8533), 30: row(9000, 9000, 22)},
        {22: row(13690, 8449), 30: row(9000, 9000, 22)},
        {22: row(12457, 28019), 30: row(9000, 9000, 22)},
        {22: row(12496, 27825), 30: row(9000, 9000, 22), 31: row(5000, 5000)},
        {31: row(5010, 5000)},
        {},
        {31: row(5100, 5000)},
    ]
    base = {"side": 1, "card_id": 26000010, "level": 11, "max_hp": 81, "kind_first": 14, "c0": None}
    ents = {
        22: dict(base, key=22, first_index=0, last_index=4, frames=5, x0=13750, y0=8617),
        30: dict(base, key=30, first_index=0, last_index=4, frames=5, x0=9000, y0=9000),
        31: dict(base, key=31, first_index=4, last_index=7, frames=3, x0=5000, y0=5000),
    }
    splits = m.split_reused_keys(ents, per_tick_rows, ticks)
    assert splits == [[22, 32, 953]], splits
    assert (ents[22]["first_index"], ents[22]["last_index"], ents[22]["frames"]) == (0, 2, 3)
    assert (ents[32]["first_index"], ents[32]["last_index"], ents[32]["frames"], ents[32]["c0"]) == (3, 4, 2, None)
    assert 22 not in per_tick_rows[3]
    assert per_tick_rows[3][32][:2] == (12457, 28019)
    t = m.TRUTH_COLUMNS.index("target")
    assert per_tick_rows[2][30][t] == 22, "targets follow the unit they meant"
    assert per_tick_rows[3][30][t] == 32, "targets follow the unit they meant"
    assert 31 in ents, "a short gap near the same point stays one unit"
    assert ents[31]["last_index"] == 7, "a short gap near the same point stays one unit"


def test_a_pair_with_no_measured_ring_is_played_on_the_tile_its_laid_point_names(m):
    """20260918-112751's Archer pairs: no measured group gives Archer offsets, so the tile comes from the ring's laid
    point, the exact mean of the members' creation points. Left half: created on (8999, 500) and (7999, 500), laid on
    (8499, 500), the tile (8500, 500). Right half: (10000, 7500) and (11000, 7500), laid on the tile (10500, 7500)
    itself. A side-1 ground ring is laid a unit lower in y, a flying one on the tile; a mean between two laid points
    names no tile."""
    assert m.laid_tile([(8999, 500), (7999, 500)], 0, False)[0] == [8500, 500]
    assert m.laid_tile([(10000, 7500), (11000, 7500)], 0, False)[0] == [10500, 7500]
    assert m.laid_tile([(3999, 14500), (2999, 14500)], 0, False)[0] == [3500, 14500]
    assert m.laid_tile([(10000, 30499), (9000, 30499)], 1, False)[0] == [9500, 30500]
    assert m.laid_tile([(9000, 30500), (8000, 30500)], 1, True)[0] == [8500, 30500]
    tile, why = m.laid_tile([(13701, 10828), (4280, 11616)], 0, False)
    assert tile is None, why
    assert why.startswith("the members' mean (8990.5, 11222) is no tile"), why


def test_a_king_launched_cast_seen_after_a_frame_gap_is_dated_by_its_steps(m):
    """20260920-071744-B: the Fireball's frames of 473 and 474 were lost; on 475 it stands three steps of 599 from its
    caster's king, so it left on 473, the tick the other seat's capture first shows it. A cast seen on its launch tick
    (previous point on the king) and a point off the king's line are not dated."""
    king = {"kind": 12, "side": 1, "x": 9000, "y": 29000}
    fb = 28000000

    def frame(x, y, x2, y2):
        return {"entities": [king], "effects": [{"card_id": fb, "side": 1, "x": x, "y": y, "x2": x2, "y2": y2}]}

    k, why = m.launch_tick(frame(9678, 27335, 9452, 27890), 1, fb)
    assert k == 3, why
    assert m.launch_tick(frame(9226, 28445, 9000, 29000), 1, fb) is None, "seen on its launch tick"
    assert m.launch_tick(frame(9678, 27335, 9452, 27990), 1, fb) is None, "off the line from the king"
    assert m.launch_tick(frame(9678, 27335, 9452, 27890), 0, fb) is None, "no king of that side on the frame"


# PAIR DATING (make_replay_fixture.py module doc): a row's window is (the frame before its first sighting, that
# sighting]; the battle's other seat's row for the same deploy pins it where the two windows share exactly one tick.
#: One seat's frames: 101 and 103 missed.
PAIR_TICKS = [100, 102, 104, 105, 106, 110]
PARTNER = "20260920-000000-B"


def _pair_row(card, tick, first_seen, side=0, evidence="alone"):
    return {"card": card, "side": side, "tick": tick, "first_seen": first_seen, "tick_evidence": evidence}


def _offer(card, tick, window, side=0):
    return {"card": card, "side": side, "tick": tick, "window": window}


def test_pair_dating_takes_the_one_tick_both_seats_windows_share(m):
    """A row first seen on 102 after a missed 101 could be 101 or 102; the other seat saw it on 101 with no gap, so it
    is 101. A cast dated by its elixir drop on 104 (103 missed) against the other seat's [102, 103] is 103. A row
    already on the shared tick keeps it, and all three say they were pair-dated."""
    knight = _pair_row("Knight", 102, 102, evidence="first-seen tick: the deploy-end transition contradicts")
    rage = _pair_row("Rage", 104, None, evidence="elixir drop of 2 on side 0 at 104")
    goblins = _pair_row("Goblins", 101, 102, side=1, evidence="range [101, 102] (frame gap), earliest used")
    offer = [_offer("Knight", 101, (101, 101)), _offer("Rage", 103, (102, 103)), _offer("Goblins", 101, (100, 101), 1)]
    moved = m.pair_date([knight, rage, goblins], PAIR_TICKS, [(PARTNER, offer)])
    assert moved == 2
    assert (knight["tick"], rage["tick"], goblins["tick"]) == (101, 103, 101)
    assert knight["tick_evidence"] == (
        f"exact (pair-dated with {PARTNER}: its window [101, 101] and this row's [101, 102] share only 101; this"
        " capture alone gave 102: first-seen tick: the deploy-end transition contradicts)"
    )
    assert rage["tick_evidence"].startswith(f"exact (pair-dated with {PARTNER}: its window [102, 103] and this row's")
    alone = "this capture alone gave 101: range [101, 102] (frame gap), earliest used)"
    assert goblins["tick_evidence"].endswith(alone), goblins


def test_pair_dating_keeps_a_row_whose_windows_share_two_ticks_or_none(m):
    """Two shared ticks leave the choice where it was, and so do windows that share none (the seats disagree)."""
    two = _pair_row("Knight", 103, 104, evidence="range [103, 104], earliest used")
    none = _pair_row("Arrows", 102, 102, side=1, evidence="first frame of the projectile (frame gap 2)")
    offer = [_offer("Knight", 103, (102, 104)), _offer("Arrows", 104, (103, 104), 1)]
    before = [dict(two), dict(none)]
    assert m.pair_date([two, none], PAIR_TICKS, [(PARTNER, offer)]) == 0
    assert [two, none] == before


def test_pair_dating_keeps_a_row_with_no_partner_row(m):
    """No partner capture, a partner row of another card or side or dated more than PAIR_MATCH_TICKS away, and two
    candidate rows: none of them dates the row. A one-tick window is never re-dated."""
    row = _pair_row("Knight", 102, 102)
    before = dict(row)
    assert m.pair_date([row], PAIR_TICKS, []) == 0
    far = 102 + m.PAIR_MATCH_TICKS + 1
    others = [_offer("Giant", 101, (101, 101)), _offer("Knight", 101, (101, 101), 1), _offer("Knight", far, (far, far))]
    assert m.pair_date([row], PAIR_TICKS, [(PARTNER, others)]) == 0
    twice = [_offer("Knight", 101, (101, 101)), _offer("Knight", 102, (102, 102))]
    assert m.pair_date([row], PAIR_TICKS, [(PARTNER, twice)]) == 0
    assert row == before
    single = _pair_row("Knight", 105, 105)
    assert m.pair_date([single], PAIR_TICKS, [(PARTNER, [_offer("Knight", 104, (103, 104))])]) == 0
    assert single["tick"] == 105


def test_a_rows_window_follows_its_first_sighting(m):
    """The window is (frame before the first sighting, the sighting]; a cast with no sighting is sighted on its tick;
    a row dated outside its window (shown late) offers its own tick; a row on the first frame offers nothing."""
    rows = [
        _pair_row("Knight", 102, 102),
        _pair_row("Rage", 104, None),
        _pair_row("Goblins", 98, 102),
        _pair_row("Tesla", 100, 100),
    ]
    offer = m.seat_offer(rows, PAIR_TICKS)
    assert [(o["card"], o["tick"], o["window"]) for o in offer] == [
        ("Knight", 102, (101, 102)),
        ("Rage", 104, (103, 104)),
        ("Goblins", 98, (98, 98)),
    ]


def test_a_captures_partners_are_the_other_seat_of_its_battle(m):
    """Same stamp, other seat; a twin stamped a few seconds apart; every part of a split capture; never the same
    seat's other part, another battle, or a capture without a seat tag."""
    s = m.CAPTURE_SUFFIX
    pool = [
        f"frames-auto-20260920-005517-40001{s}",
        f"frames-auto-20260920-005517-40002{s}",
        f"frames-auto-20260918-164951-40002{s}",
        f"frames-auto-20260918-164953-40001{s}",
        f"frames-auto-20260920-010218-40001.b1{s}",
        f"frames-auto-20260920-010218-40001.b2{s}",
        f"frames-auto-20260920-010218-40002{s}",
        f"frames-auto-20260920-010240-40002{s}",
        f"frames-20260918-122757.b1{s}",
        f"frames-20260918-122757.b2{s}",
    ]
    assert m.battle_partners(pool[0], pool) == [pool[1]]
    assert m.battle_partners(pool[2], pool) == [pool[3]]
    assert m.battle_partners(pool[6], pool) == [pool[4], pool[5]]
    assert m.battle_partners(pool[4], pool) == [pool[6]]
    assert m.battle_partners(pool[8], pool) == []


def _seat_battle(frame_ticks):
    """(header towers, frames) of one seat of a battle: six towers and a side-0 Knight created on 12 and deploying,
    on the frames `frame_ticks`."""
    tower_rows = [
        (1, 0, 9000, 3000, 4824),
        (2, 0, 3500, 6500, 3052),
        (3, 0, 14500, 6500, 3052),
        (4, 1, 9000, 29000, 4824),
        (5, 1, 3500, 25500, 3052),
        (6, 1, 14500, 25500, 3052),
    ]
    header = [{"side": s, "x": x, "y": y} for _, s, x, y, _ in tower_rows]

    def ent(key, side, cid, x, y, hp, kind, state):
        return {"id": f"p{key}", "generation_key": key, "side": side, "x": x, "y": y, "x2": x, "y2": y,
                "card_id": cid, "level": 11, "kind": kind, "hp": hp, "max_hp": hp, "behavior_state": state,
                "target": None, "path_nodes": []}

    frames = []
    for t in frame_ticks:
        ents = [ent(k, s, -1, x, y, hp, 13, 0) for k, s, x, y, hp in tower_rows]
        if t >= 12:
            ents.append(ent(51, 0, KNIGHT, 9500, 5500, 1766, 14, 4))
        frames.append({"tick": t, "entities": ents, "effects": []})
    return header, frames


@needs_modern_cards
def test_a_battle_recorded_from_both_seats_dates_a_row_by_the_other_seat(m, tmp_path):
    """End to end through the command line: seat A missed frame 11 and first sees the Knight on 12, so alone it
    dates it 11 (the earliest of [11, 12]); seat B saw 11 without it and 12 with it, so it is 12. `--all` writes A
    on 12, pair-dated with B, and B unchanged; `--check` of A alone rebuilds B to reach the same fixture."""
    _skip_without_the_id_table(m)
    reports = tmp_path / "reports"
    seat_a = reports / ("frames-auto-20260920-120000-40001" + m.CAPTURE_SUFFIX)
    seat_b = reports / ("frames-auto-20260920-120000-40002" + m.CAPTURE_SUFFIX)
    _write_capture(seat_a, *_seat_battle([t for t in range(31) if t != 11]))
    _write_capture(seat_b, *_seat_battle(list(range(31))))
    out = tmp_path / "out"
    tool = os.path.join(ROOT, "tools", "make_replay_fixture.py")
    run = subprocess.run([sys.executable, tool, "--all", "--reports", str(reports), "--out", str(out)],
                         capture_output=True, text=True, timeout=600, cwd=ROOT)
    assert run.returncode == 0, run.stdout + run.stderr

    def knight(name):
        with open(out / f"{name}.replay.json", encoding="utf-8") as fh:
            return next(d for d in json.load(fh)["deploys"] if d["card"] == "Knight")

    a, b = knight("20260920-120000-A"), knight("20260920-120000-B")
    assert (a["first_seen"], a["tick"]) == (12, 12), a
    assert a["tick_evidence"].startswith(
        "exact (pair-dated with 20260920-120000-B: its window [12, 12] and this row's [11, 12] share only 12;"
        " this capture alone gave 11: range [11, 12]"
    ), a["tick_evidence"]
    assert (b["tick"], b["tick_evidence"]) == (12, "exact"), b
    check = subprocess.run(
        # --reports names the test's own folder: without it the seat letters come from ROYALELIVE_REPORTS's captures,
        # and the check passed only while their seat tags happened to sort the same way as these two.
        [sys.executable, tool, str(seat_a), "--reports", str(reports),
         "--check", str(out / "20260920-120000-A.replay.json")],
        capture_output=True, text=True, timeout=600, cwd=ROOT,
    )
    assert check.returncode == 0, check.stdout + check.stderr
    assert "is current" in check.stdout, check.stdout


def test_a_frame_read_a_tick_late_takes_the_tick_its_contents_show(m):
    """20260918-134739-B's frame 253 carries tick 254's positions: its walkers step two ticks' worth into it and one
    out of it to frame 255. It is relabelled 254; a frame whose walkers step one tick's worth in and two out stays."""

    def frame(tick, ys):
        return {"tick": tick, "entities": [{"id": k, "x": 1000 * k, "y": y} for k, y in enumerate(ys)]}

    late = [frame(252, [0, 0, 0]), frame(253, [180, 180, 180]), frame(255, [270, 270, 270])]
    assert m.relabel_late_reads(late) == [[253, 254]]
    assert [f["tick"] for f in late] == [252, 254, 255]
    right = [frame(252, [0, 0, 0]), frame(253, [90, 90, 90]), frame(255, [270, 270, 270])]
    assert m.relabel_late_reads(right) == []
    assert [f["tick"] for f in right] == [252, 253, 255]
    few = [frame(252, [0, 0]), frame(253, [180, 180]), frame(255, [270, 270])]
    assert m.relabel_late_reads(few) == [], "two voters are not enough"


def test_the_table_switch_sets_the_card_file_the_id_pack_and_the_census_together(m):
    """make_replay_fixture.py --table: one switch points CARDS, RAW and CENSUS at a table, so a fixture made on the
    160402017 table resolves its client ids from that pack and reads that table's census (harness.rs `census_file`)."""
    saved = (m.CARDS, m.RAW, m.CENSUS, m.TABLE)
    try:
        m.select_table("160402017-20261006")
        assert m.CARDS.endswith(os.path.join("derived", "cards-160402017-20261006.json"))
        assert m.RAW.endswith(os.path.join("raw", "cr-160402017", "csv_logic"))
        assert m.CENSUS == "card_census-160402017-20261006.json"
        assert m.TABLE == "160402017-20261006"
        m.select_table("160402017")
        assert m.CARDS.endswith(os.path.join("derived", "cards-160402017.json"))
        assert m.RAW.endswith(os.path.join("raw", "cr-160402017", "csv_logic")), "both 160402017 tables name one pack"
        assert m.CENSUS == "card_census-160402017.json"
        m.select_table("15.535.29")
        assert saved == (m.CARDS, m.RAW, m.CENSUS, m.TABLE), "the default table is the module's own constants"
    finally:
        m.CARDS, m.RAW, m.CENSUS, m.TABLE = saved


def test_a_capture_is_made_on_the_table_of_its_own_content(m):
    """A capture's header stamp (`client.content_version`, written from the 2026-10-06 update on) picks its table: the
    update's table takes 16.402.19 and the later stamps of that build (16.402.21), never a capture with no stamp
    (recorded before the update); the install-time table takes only an unstamped capture; the 15.535.29 table takes
    any. A device takes an update when its game starts, so the stamp, not a date, decides."""
    new, old = "160402017-20261006", "160402017"
    assert m.content_refusal(new, "16.402.19") is None
    assert m.content_refusal(new, "16.402.21") is None
    assert "--table 160402017" in m.content_refusal(new, None)
    assert m.content_refusal(new, "16.402.12") is not None, "an older content of the build is not the update's"
    assert m.content_refusal(new, "16.403.1") is not None
    assert m.content_refusal(old, None) is None
    assert "--table 160402017-20261006" in m.content_refusal(old, "16.402.19")
    assert m.content_refusal(new, "x.y") is not None
    assert m.content_refusal("15.535.29", "16.402.19") is None
    assert m.content_refusal("15.535.29", None) is None


def test_a_forms_units_are_the_cards_objects(m):
    """classify_unit takes an evolution's or a hero form's objects for the card's (Live 2026-10-09: the Evo Bats'
    Bat_EV1, 48 base, 122 at level 11, was an unknown_object against the plain Bat's 81)."""
    with open(m.CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    bats = next(c for c in doc["cards"] if c["name"] == "Bats")
    evo = next((r for r in doc.get("evolutions") or [] if r["name"] == "Bats_EV1"), None)
    if evo is None:
        pytest.skip("this cards.json carries no Bats_EV1")
    hp = evo["hitpoints"] * m.ladder_percent(doc, evo, evo["summon_character"], 11) // 100
    unit, own, how = m.classify_unit(doc, bats, 11, hp)
    assert (unit, own, how) == (evo["summon_character"], True, "exact"), (unit, own, how, hp)
    plain = bats["hitpoints"] * m.ladder_percent(doc, bats, bats["summon_character"], 11) // 100
    assert m.classify_unit(doc, bats, 11, plain)[0] == bats["summon_character"], "the plain Bat still reads plain"


def test_a_live_captures_stamp_is_read_and_it_has_no_partner(m, tmp_path):
    """capture_stamp reads YYYYMMDD-HHMMSS after any prefix (frames-liveplay-...: it read 'liveplay'), and a live
    capture is its battle's only seat: another account's live capture seconds later is no partner."""
    a = tmp_path / "frames-liveplay-20261007-114236-40001.jsonl.gz"
    b = tmp_path / "frames-liveplay-20261007-114240-40002.jsonl.gz"
    for f in (a, b):
        f.write_bytes(b"")
    assert m.capture_stamp(str(a)) == ("20261007", 11 * 3600 + 42 * 60 + 36)
    assert m.capture_stamp(str(tmp_path / "frames-auto-20260920-003751-31.jsonl.gz")) == ("20260920", 37 * 60 + 51)
    assert m.battle_partners(str(a), [str(a), str(b)]) == []


def test_an_evo_skeletons_copies_are_not_plays(m):
    """An Evo Skeleton's hit makes a copy, which the capture shows as a new unit of the card (module doc, EVO COPIES).
    A group born out of its deploy, each member first standing within EVO_COPY_REACH of a living member of an earlier
    group, is that group's copies whatever its size (live 20261002-152325-A's keys 55-59 stood 826-925 off one;
    20261007-112004-A's keys 111-113 are three on one tick); one too far, or one whose only neighbour is dead, is
    not."""
    row = {10: (14500, 12500, 32), 11: (13600, 12400, 32), 12: (16000, 12500, 0)}
    one = [{"key": 20, "x0": 14500, "y0": 13413}]
    assert m.evo_copy_of(one, {10, 11}, row) is not None
    assert "[913]" in m.evo_copy_of(one, {10, 11}, row)
    assert "3 copies" in m.evo_copy_of(one * 3, {10, 11}, row), "three copies on one tick are copies"
    assert m.evo_copy_of([{"key": 21, "x0": 14500, "y0": 12500 + m.EVO_COPY_REACH + 1}], {10}, row) is None
    assert m.evo_copy_of([{"key": 22, "x0": 16000, "y0": 12600}], {12}, row) is None, "a dead neighbour hits nobody"
    assert m.evo_copy_of(one, set(), row) is None, "no group to be a copy of"
    # born out of its deploy: first shown walking (1) or attacking (2), not deploying (4) nor waiting its stagger (11)
    assert m.born_out_of_deploy({"states": [(5, 1)]})
    assert m.born_out_of_deploy({"states": [(5, 2), (6, 1)]})
    assert not m.born_out_of_deploy({"states": [(5, 4)]})
    assert not m.born_out_of_deploy({"states": [(5, m.STATE_STAGGER_WAIT)]})


def test_a_scheduled_spells_cast_is_read_off_its_spawns(m):
    """The 16.402 Graveyard's skeletons come on its area's schedule and the capture shows no object for the cast
    (module doc, A SCHEDULED SPELL'S CAST): one cast per run of spawns within the area's life, dated by the first
    spawns less their delays, at the creation points of the spawns with no offset on that axis. A cast row already
    near (a tap's) stands for it."""
    entries = [
        {"delay_ms": 2200, "spawn_type": "CharacterType", "x": {"offset_milli": -3500}, "y": {"offset_milli": 0}},
        {"delay_ms": 2700, "spawn_type": "CharacterType", "x": {"offset_milli": -2500}, "y": {"offset_milli": 2500}},
        {"delay_ms": 3300, "spawn_type": "CharacterType", "x": {"offset_milli": 0}, "y": {"offset_milli": -3500}},
    ]
    card = {"name": "Graveyard", "kind": "spell",
            "spell": {"area_effect_object": {"life_duration_ms": 9000, "schedule": {"entries": entries}}}}
    ticks = list(range(0, 400))

    def ent(key, tick, x, y):
        return {"key": key, "first_index": tick, "card_id": 28000010, "level": 11, "x0": x + 7, "y0": y + 7,
                "c0": [x, y]}

    ents = {1: ent(1, 144, 13000, 26500), 2: ent(2, 154, 14000, 29000), 3: ent(3, 166, 16500, 23000),
            4: ent(4, 344, 1000, 20000)}
    groups = [{"side": 0, "card": "Graveyard", "tick": e["first_index"], "keys": [k]} for k, e in ents.items()]
    rows = m.schedule_casts(groups, ents, ticks, {"Graveyard": card}, [])
    first = rows[0]
    assert (first["tick"], first["pos"], first["source"]) == (100, [16500, 26500], "schedule"), first
    assert first["spawn_keys"] == [1, 2, 3]
    assert len(rows) == 1, "a lone spawn past the first cast's life pins no x and no y: no second cast"
    tapped = [{"kind": "spell", "side": 0, "card": "Graveyard", "tick": 95}]
    assert m.schedule_casts(groups, ents, ticks, {"Graveyard": card}, tapped) == [], "the tap's cast stands for it"


def test_a_spells_level_is_its_spawns(m):
    """A spell that puts units down records its level on them (module doc, A SPELL'S LEVEL FROM ITS SPAWN): live
    20261007-011702-A's Barbarian Log was played at the side mode 11, its Barbarian at 12. The earliest group of that
    side and card within SPELL_SPAWN_WINDOW after the cast, each claimed once; a cast with none keeps its level."""
    ents = {5: {"level": 12}, 6: {"level": 13}, 7: {"level": 13}}
    groups = [{"side": 0, "card": "BarbLog", "tick": 131, "keys": [5]},
              {"side": 0, "card": "GoblinBarrel", "tick": 157, "keys": [6, 7]}]

    def cast(card, tick, level=11):
        return {"kind": "spell", "side": 0, "card": card, "tick": tick, "level": level, "level_source": "side mode"}

    log, barrel, late = cast("BarbLog", 100), cast("GoblinBarrel", 100), cast("BarbLog", 120)
    m.spell_levels_from_spawn([log, barrel, late], groups, ents)
    assert (log["level"], log["level_source"]) == (12, "spawn"), log
    assert (barrel["level"], barrel["level_source"]) == (13, "spawn"), barrel
    assert (late["level"], late["level_source"]) == (11, "side mode"), "the Barbarian is the first cast's, claimed once"
    far = cast("BarbLog", 131 - m.SPELL_SPAWN_WINDOW - 1)
    m.spell_levels_from_spawn([far], groups, ents)
    assert far["level"] == 11, "a spawn past the window is not the cast's"


@needs_cards
def test_a_sides_tower_troop_is_read_off_its_princess_towers(m):
    """A capture names no tower troop; each has its own hitpoints on the princess tower's ladder (module doc, THE TOWER
    TROOP), so the row whose hitpoints at the tower level are the princess towers' max_hp is the side's. Level 11
    (218 %): the Princess 3052, the Cannoneer 2616, the Dagger Duchess 2768, the Royal Chef 2703. Two towers that
    disagree, or a max_hp no row gives, name none."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    assert [m.princess_tower_percent(lv) for lv in (1, 9, 10, 11, 16)] == [100, 181, 199, 218, 347]

    def side(s, hp, level=11):
        return [{"side": s, "slot": 0, "max_hp": 5000, "level": level},
                {"side": s, "slot": 1, "max_hp": hp, "level": level},
                {"side": s, "slot": 2, "max_hp": hp, "level": level}]

    assert m.tower_troops(side(0, 3052) + side(1, 2616), doc) == {"0": "PrincessTower", "1": "Cannoneer"}
    assert m.tower_troops(side(0, 2768) + side(1, 2703), doc) == {"0": "DaggerDuchess", "1": "ChefTower"}
    assert m.tower_troops(side(0, 1270 * 347 // 100, 16) + side(1, 3000), doc) == {"0": "DaggerDuchess", "1": None}
    mixed = side(0, 3052)
    mixed[2]["max_hp"] = 2616
    assert m.tower_troops(mixed, doc)["0"] is None, "the two towers disagree"


@needs_cards
def test_a_cost_only_spell_is_read_off_the_header_and_its_victims(m):
    """A Zap leaves no object a capture shows (module doc, A COST-ONLY SPELL): a live header's payment with nothing
    readable, whose cost fits exactly one instant spell of the side's revealed deck, is that spell, dated at the drop
    and aimed at the units its damage hit (live 20261007-141917 t282: five Bats at once). A payment a row explains, an
    "ability" press and one no victim answers are not played."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    assert m.instant_spell(cards["Zap"])
    assert not m.instant_spell(cards["Fireball"])
    assert not m.instant_spell(cards["Knight"])
    dmg = m.spell_damage_at(doc, cards["Zap"], 11)

    def row(x, y, hp):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = x, y, hp
        return tuple(r)

    ticks = [280, 281, 282, 283]
    ents = {k: {"key": k, "side": 1, "card_id": 26000005} for k in (7, 8)}
    before = {7: row(14000, 17000, 300), 8: row(14400, 17200, dmg)}
    rows = [before, before, {7: row(14000, 17000, 300 - dmg)}, {7: row(14000, 17000, 300 - dmg)}]
    header = {"reveal": {"0": ["Zap", "Knight"], "1": []},
              "opponent_plays": [{"tick": 282, "side": 0, "card": None, "cost": 2, "evidence": "elixir"},
                                 {"tick": 283, "side": 0, "card": "Knight", "cost": 3, "evidence": "ability"}]}
    unresolved = []
    got = m.cost_only_casts(header, [], doc, cards, {"Zap": 28000008}, ents, rows, ticks, unresolved)
    assert [(d["tick"], d["card"], d["source"], d["pos"]) for d in got] == [(282, "Zap", "cost_only", [14500, 17500])]
    assert got[0]["level"] in m.playable_levels(doc, cards["Zap"])
    explained = [{"tick": 282, "side": 0, "first_seen": 282}]
    assert m.cost_only_casts(header, explained, doc, cards, {"Zap": 28000008}, ents, rows, ticks, []) == []
    quiet = [before] * 4
    assert m.cost_only_casts(header, [], doc, cards, {"Zap": 28000008}, ents, quiet, ticks, unresolved) == []
    assert any("no victim" in u["why"] for u in unresolved)


def test_a_heros_display_twin_is_display_only(m):
    """A HERO'S DISPLAY TWIN (module doc): a Hero Ice Wizard deploy is two objects of its card with its hitpoints; the
    undamaged one is its floating ice, role "display". With both undamaged, the one first seen behind the other (toward
    its own king: side 1, + y) is the twin. A form with no display twin is left alone."""
    effect = {"kind": "ice_cube", "display_attached": "IceWizardHeroFloatingCube"}
    hero_form = {"name": "IceWizard_hero", "form_of": "IceWizard", "summon_character": "IceWizardHero",
                 "ability": {"effect": effect}}
    doc = {"hero_forms": [hero_form], "cards": [{"name": "IceWizard", "summon_character": "IceWizard"}]}

    def ent(key, y0, first=0, last=2):
        return {"key": key, "side": 1, "card_id": 203000023, "unit": "IceWizardHero", "role": "summon", "max_hp": 688,
                "x0": 13500, "y0": y0, "first_index": first, "last_index": last}

    hero, twin = ent(7, 20000), ent(8, 20000)
    rows = [{7: (0, 0, 688), 8: (0, 0, 688)}, {7: (0, 0, 600), 8: (0, 0, 688)}, {7: (0, 0, 511), 8: (0, 0, 688)}]
    assert m.display_twins({7: hero, 8: twin}, rows, doc) == 1
    assert (hero["role"], twin["role"]) == ("summon", "display"), "the undamaged one"
    # both undamaged: the one behind (side 1: the larger y)
    a, b = ent(7, 20150), ent(8, 20000)
    clean = [{7: (0, 0, 688), 8: (0, 0, 688)}] * 3
    assert m.display_twins({7: a, 8: b}, clean, doc) == 1
    assert (a["role"], b["role"]) == ("display", "summon"), "the one behind its hero"
    assert m.display_twins({7: ent(7, 20000), 8: ent(8, 20000)}, clean, {"hero_forms": []}) == 0
    # live (ob9): both members read as the base card's unit, IceWizard (their hitpoints match its row too)
    a, b = ent(7, 20000), ent(8, 20000)
    a["unit"] = b["unit"] = "IceWizard"
    assert m.display_twins({7: a, 8: b}, rows, doc) == 1
    assert (a["role"], b["role"]) == ("summon", "display"), "the twin read as an IceWizard"


def test_a_hero_forms_own_objects_are_its_cards(m):
    """An object only a hero form's own table holds (`form_units`) is its card's: the Hero Musketeer's turret,
    MusketeerTurret, 600 base on the Common ladder, is in no other table, and live all 67 turrets of 25 fixtures (level
    11, 1536 hp) were unknown objects the harness skipped. It is a spawned object, not a deploy summon."""
    path = os.path.join(ROOT, "data", "derived", "cards-160402017-20261006.json")
    if not os.path.exists(path):
        pytest.skip(f"{path} is absent (the 2026-10-06 table) -- a skip here is not a pass")
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh)
    ice = next(h for h in doc["hero_forms"] if h["name"] == "IceWizard_hero")
    assert m.reachable_units(doc, ice).get("IceWizardHero_IceCube") == 104, "the Hero Ice Wizard's cube is its card's"
    musketeer = next(c for c in doc["cards"] if c["name"] == "Musketeer")
    assert "MusketeerTurret" not in doc["units"], "the turret row is the hero form's alone"
    common = doc["rarities"]["Common"]
    assert 600 * common["multiplier_percent_by_level"][11 - common["relative_level"] - 1] // 100 == 1536
    assert m.classify_unit(doc, musketeer, 11, 1536) == ("MusketeerTurret", False, "exact")


@needs_cards
def test_a_pulsing_spell_first_seen_after_a_gap_is_dated_by_its_train(m):
    """A Poison first seen after missing frames (module doc, A PULSING SPELL FIRST SEEN AFTER A FRAME GAP) is cast on
    the one tick inside the gap whose drop + 25 + 20k its victims' pulses fall on (live 20261007-161446: first seen 810
    after 18 missing ticks, its train's drop 793). A train that fits two ticks, or one pulse, moves nothing, nor does a
    15.535.29 capture."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    pulse = m.spell_pulse_at(doc, cards["Poison"], 11, False)
    assert pulse
    assert m.PULSE_AFTER_DROP == {"Poison": 25, "Earthquake": 19}

    def row(hp):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = 9000, 22000, hp
        return tuple(r)

    def battle(drop, frames):
        # a Knight under the area from the start, losing a pulse on drop + 25 + 20k
        hp, rows = 1500, []
        for t in frames:
            hp_now = hp - pulse * sum(1 for k in range(8) if drop + 25 + 20 * k <= t)
            rows.append({5: row(hp_now)})
        return rows

    ents = {5: {"key": 5, "side": 1, "card_id": 26000000, "kind_first": 1}}

    def cast(seen):
        return {"tick": seen, "first_seen": seen, "side": 0, "card": "Poison", "kind": "spell", "level": 11,
                "pos": [9000, 22000], "card_id": 28000009}

    # first seen 810 after a gap from 792; the drop 793; every frame from 810 on
    frames = [790, 792, *range(810, 900)]
    d = cast(810)
    assert m.pulse_dates([d], doc, cards, ents, battle(793, frames), frames, "160402017-20261006")
    assert (d["tick"], d["timing"]) == (793, "pulse_train"), d
    assert "first frame 810, after a frame gap of 18" in d["tick_evidence"]
    # one pulse only (the train cut at its first): no move
    short = [790, 792, *range(810, 820)]
    d = cast(810)
    assert not m.pulse_dates([d], doc, cards, ents, battle(793, short), short, "160402017-20261006")
    assert d["tick"] == 810
    # a drop on 800 whose pulses (825, 845, ...) each fall between frames a tick either side fits 800 and 801: no move
    sparse = [790, 792, *range(810, 900, 2)]
    d = cast(810)
    assert not m.pulse_dates([d], doc, cards, ents, battle(800, sparse), sparse, "160402017-20261006")
    assert d["tick"] == 810
    # a 15.535.29 capture is left alone
    d = cast(810)
    assert not m.pulse_dates([d], doc, cards, ents, battle(793, frames), frames, "15.535.29")
    assert d["tick"] == 810


@needs_cards
def test_a_lightnings_level_is_read_off_its_bolt(m):
    """A spell whose area strikes with a projectile (`area_strike`: the Lightning's bolt) hits with that projectile's
    damage, anywhere in its area, from its first strike on (`effect_leads`: the 2026-10-06 table's bolt 500 ms after the
    cast): a drop on a victim 3000 from the cast point 10 ticks on reads the level, and a bigger chip on the same victim
    3 ticks after the cast is not taken for it. A crown tower takes the bolt's crown share."""
    path = os.path.join(ROOT, "data", "derived", "cards-160402017-20261006.json")
    if not os.path.exists(path):
        pytest.skip(f"{path} is absent (the 2026-10-06 table) -- a skip here is not a pass")
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    lt = cards["Lightning"]
    bolt = m.area_strike(lt)
    assert lt.get("damage") is None, "the card carries no damage of its own"
    assert bolt.get("damage"), "the bolt carries the damage"
    assert m.effect_leads(lt)[0] == 10
    ls = lt["level_scaling"]
    table = ls["multiplier_percent_by_level"]
    first = ls.get("base_level", doc["rarities"][ls["rarity"]]["relative_level"] + 1)
    dmg11 = bolt["damage"] * table[11 - first] // 100
    assert m.spell_damage_at(doc, lt, 11) == dmg11
    pct = bolt["crown_tower_damage_percent"]
    assert m.spell_tower_damage_at(doc, lt, 11) == dmg11 - dmg11 * (100 - pct) // 100

    def row(x, y, hp):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = x, y, hp
        return tuple(r)

    ticks = list(range(100, 116))
    ents = {9: {"key": 9, "side": 1, "card_id": 26000000}}
    hp = [3000] * 3 + [3000 - 900] * 7 + [3000 - 900 - dmg11] * 6  # a 900 chip on 103, the bolt on 110
    rows = [{9: row(9000, 25000, h)} for h in hp]
    d = {"tick": 100, "side": 0, "card": "Lightning", "kind": "spell", "level": 13, "level_source": "side mode",
         "pos": [9000, 22000]}
    m.spell_levels_from_damage([d], doc, cards, ents, rows, ticks)
    assert (d["level"], d["level_source"]) == (11, "damage"), d
    assert d["level_evidence"].startswith(f"hit on 110: drops [{dmg11}]"), d["level_evidence"]


@needs_cards
def test_a_spells_level_is_read_off_a_crown_towers_drop(m):
    """A crown tower is a victim too, its drop the spell's damage less the crown share, floored
    (`spell_tower_damage_at`: a Fireball at 11 takes 159 off a tower; rounding to nearest would give 158). A cast whose
    kills fit several levels
    is no read: it takes the side's read level of the card where that is one of its own fits (live 20261008-051029)."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    cards = {c["name"]: c for c in doc["cards"]}
    fb = cards["Fireball"]
    t11 = m.spell_tower_damage_at(doc, fb, 11)
    dmg11 = m.spell_damage_at(doc, fb, 11)
    assert t11 == dmg11 - dmg11 * (100 - fb["crown_tower_damage_percent"]) // 100
    assert m.spell_tower_damage_at(doc, cards["Zap"], 11) is None, "a Zap's tower hit is its own TowerDamage"

    def row(x, y, hp):
        r = [0] * len(m.TRUTH_COLUMNS)
        r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = x, y, hp
        return tuple(r)

    ticks = [100, 101, 102, 103]
    ents = {1: {"key": 1, "side": 1, "card_id": -1}, 9: {"key": 9, "side": 1, "card_id": 26000000}}

    def cast(level=13, pos=(3500, 25500)):
        return {"tick": 100, "side": 0, "card": "Fireball", "kind": "spell", "level": level,
                "level_source": "side mode", "pos": list(pos)}

    tower_only = [{1: row(3500, 25500, 3000)}, {1: row(3500, 25500, 3000)}, {1: row(3500, 25500, 3000 - t11)},
                  {1: row(3500, 25500, 3000 - t11)}]
    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, tower_only, ticks)
    assert (d["level"], d["level_source"]) == (11, "damage"), d
    assert f"crown towers took [{t11}]" in d["level_evidence"]
    # a second cast that only killed a 400-hp unit (several levels fit) takes the side's read 11, one of its fits
    kill = [{**r, 9: row(9000, 22000, 400)} for r in tower_only[:2]] + [dict(r) for r in tower_only[2:]]
    a, b = cast(), cast(pos=(9000, 22000))
    m.spell_levels_from_damage([a, b], doc, cards, ents, kill, ticks)
    assert (a["level"], a["level_source"]) == (11, "damage"), a
    assert "fitting levels [" in b["level_evidence"]
    assert (b["level"], b["level_source"]) == (11, "card level"), b


TABLE_16402 = os.path.join(ROOT, "data", "derived", "cards-160402017-20261006.json")


@pytest.fixture(scope="module")
def doc16402():
    if not os.path.exists(TABLE_16402):
        pytest.skip(f"{TABLE_16402} is absent (the 2026-10-06 table) -- a skip here is not a pass")
    with open(TABLE_16402, encoding="utf-8") as fh:
        return json.load(fh)


def test_a_cards_death_and_form_blocks_put_down_its_objects(m, doc16402):
    """Objects a card's death projectile, death area, action graph, form row and form mechanic put down are its
    spawned objects (live ob10: 832 unknown objects, 777 of them these; the harness skipped them all). Each base on
    the Common ladder: the Phoenix's egg 124 (PhoenixFireball's spawn_character), the Suspicious Bush's BushGoblin 132
    (its death area SuspiciousBush_DummyAEO), the Evo Skeleton Army's Spectral 1, the Evo Goblin Giant's Goblin 79 (its
    form row's interval spawner), the Evo Royal Ghost's pair 32, the Evo Lumberjack's ghost 1 (evo_rage_ghost), the
    Hero Dark Prince's mount 530 (its button's `mount`)."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    live = [  # (card, level, max_hp, unit): live hp of ob10 entities
        ("Phoenix", 11, 317, "PhoenixEgg"), ("Phoenix", 16, 507, "PhoenixEgg"),
        ("SuspiciousBush", 15, 491, "BushGoblin"), ("SuspiciousBush", 16, 539, "BushGoblin"),
        ("SkeletonArmy", 8, 1, "SkeletonArmy_EV1_Spectral"), ("SkeletonArmy", 13, 3, "SkeletonArmy_EV1_Spectral"),
        ("SkeletonArmy", 16, 4, "SkeletonArmy_EV1_Spectral"),
        ("GoblinGiant", 15, 293, "Goblin"), ("GoblinGiant", 16, 323, "Goblin"),
        ("Ghost", 11, 81, "Ghost_EV1_Summon_Left"), ("Ghost", 16, 130, "Ghost_EV1_Summon_Left"),
        ("RageBarbarian", 11, 2, "RageBarbarianEvoGhost"), ("RageBarbarian", 16, 4, "RageBarbarianEvoGhost"),
        ("DarkPrince", 11, 1356, "DarkPrinceHero_Mount"), ("DarkPrince", 16, 2167, "DarkPrinceHero_Mount"),
    ]
    for name, level, hp, unit in live:
        assert m.classify_unit(doc16402, cards[name], level, hp) == (unit, False, "exact"), (name, level, hp)
    # the Lumberjack's 1-base objects are his evolution's ghost, not the bottle his death area drops: the bottle row
    # has no hitpoints (the engine runs it as a fuse, no entity)
    assert doc16402["units"]["RageBarbarianBottle"].get("hitpoints") is None
    # each card's own unit is still its deploy summon
    for name, level in (("Phoenix", 11), ("SuspiciousBush", 16), ("GoblinGiant", 16), ("Ghost", 11)):
        pct = m.ladder_percent(doc16402, cards[name], cards[name]["summon_character"], level)
        own = cards[name]["hitpoints"] * pct // 100
        assert m.classify_unit(doc16402, cards[name], level, own) == (cards[name]["summon_character"], True, "exact")


def test_a_skeleton_kings_soul_copies_are_his_spawns(m, doc16402):
    """A SOUL COPY: the Skeleton King's button puts down copies of SkeletonKingSkeleton at 1 hitpoint (calibration
    spells.CLONE_HITPOINTS) at every level (live ob10: 223 of 223 at levels 11, 15 and 16, each an unknown object). Only
    the copy hitpoints, and only under a card whose button is a soul summon."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    king = cards["SkeletonKing"]
    for level in (11, 15, 16):
        assert m.classify_unit(doc16402, king, level, 1) == ("SkeletonKingSkeleton", False, "soul copy")
    assert m.classify_unit(doc16402, king, 11, 2)[2].startswith("unknown_object"), "2 is no copy's"
    assert m.classify_unit(doc16402, cards["Knight"], 11, 1)[2].startswith("unknown_object"), "no soul summon"


def test_an_eggs_hatchling_is_no_play(m, doc16402):
    """A SPAWNED OBJECT'S HATCHLING: the Phoenix's egg puts PhoenixNoRespawn (the Phoenix's 411 base) down on its last
    frame and goes; it read as a Phoenix deploy summon, a play nobody made (live ob10 20261007-112004 key 50: egg 45's
    last frame 922 at (15296, 14031), the hatchling first seen on the next frame at (15296, 12931); 17 of 17 such). A
    Phoenix shown later, farther, on the other side, or after an egg that is not a spawned object stays a summon."""
    rows = [{} for _ in range(10)]
    rows[4] = {45: (15296, 14031, 233)}

    def ent(key, role, unit, x0, y0, first, last, side=1, max_hp=1052):
        return {"key": key, "side": side, "card_id": 26000087, "card": "Phoenix", "role": role, "unit": unit,
                "level": 11, "max_hp": max_hp, "x0": x0, "y0": y0, "first_index": first, "last_index": last}

    egg = ent(45, "spawned", "PhoenixEgg", 15296, 14031, 0, 4, max_hp=317)
    ents = {45: egg, 50: ent(50, "summon", "Phoenix", 15296, 12931, 5, 9),
            51: ent(51, "summon", "Phoenix", 15296, 12931, 6, 9),
            52: ent(52, "summon", "Phoenix", 15296, 11931, 4, 9),
            53: ent(53, "summon", "Phoenix", 15296, 12931, 4, 9, side=0)}
    assert m.hatched_spawns(ents, rows, doc16402) == 1
    assert (ents[50]["role"], ents[50]["unit"], ents[50]["deploy_summon"]) == ("spawned", "PhoenixNoRespawn", False)
    assert [ents[k]["role"] for k in (51, 52, 53)] == ["summon"] * 3, "later, farther, the other side"
    unread = {45: {**egg, "role": "unknown_object", "unit": None}, 50: ent(50, "summon", "Phoenix", 15296, 12931, 5, 9)}
    assert m.hatched_spawns(unread, rows, doc16402) == 0, "an egg the classification did not read hatches nothing"


def _evo_skeleton_battle(m, doc):
    """(header, frames) of side 0's Evo Skeletons play (keys 10-12, card 13000010) shown deploying on t10 and its
    copies, each born out of its deploy (state 2): key 30 on t40 a tile from a played member (a copy the old rule read
    too), key 31 on t50 a tile from key 30 and 2100 from every played member (a copy of a copy, its hitter 10 ticks
    old), keys 32-34 on t60 (three copies on one tick), key 35 on t81 a tile from key 34 (11 ticks after a Skeletons
    tap on t70); and the base Skeletons play the tap made (keys 40-42, card 26000010) shown deploying on t91."""
    evo = next(r for r in doc["evolutions"] if r["name"] == "Skeletons_EV1")
    base = next(c for c in doc["cards"] if c["name"] == "Skeletons")
    hp_evo = evo["hitpoints"] * m.ladder_percent(doc, evo, evo["summon_character"], 11) // 100
    hp_base = base["hitpoints"] * m.ladder_percent(doc, base, base.get("summon_character") or "Skeletons", 11) // 100
    towers = [(1, 0, 9000, 3000, 4824), (2, 0, 3500, 6500, 3052), (3, 0, 14500, 6500, 3052),
              (4, 1, 9000, 29000, 4824), (5, 1, 3500, 25500, 3052), (6, 1, 14500, 25500, 3052)]
    header = [{"side": s, "x": x, "y": y} for _, s, x, y, _ in towers]
    units = [  # key, card id, hp, x, y, first tick, first state
        (10, 13000010, hp_evo, 9000, 10500, 10, 4), (11, 13000010, hp_evo, 9500, 10500, 10, 4),
        (12, 13000010, hp_evo, 10000, 10500, 10, 4),
        (30, 13000010, hp_evo, 9500, 11500, 40, 2),
        (31, 13000010, hp_evo, 9500, 12600, 50, 2),
        (32, 13000010, hp_evo, 9000, 11500, 60, 2), (33, 13000010, hp_evo, 10000, 11500, 60, 2),
        (34, 13000010, hp_evo, 9500, 13500, 60, 2),
        (35, 13000010, hp_evo, 9500, 14400, 81, 2),
        (40, 26000010, hp_base, 4000, 20500, 91, 4), (41, 26000010, hp_base, 4500, 20500, 91, 4),
        (42, 26000010, hp_base, 5000, 20500, 91, 4),
    ]

    def ent(k, s, cid, hp, x, y, state, kind):
        return {"id": f"p{k}", "generation_key": k, "side": s, "x": x, "y": y, "x2": x, "y2": y, "card_id": cid,
                "level": 11, "kind": kind, "hp": hp, "max_hp": hp, "behavior_state": state, "target": None,
                "path_nodes": [], "attack_progress_ms": 0, "attack_load_timer_ms": 0, "event_timer_ms": 0,
                "attack_component_valid": True}

    frames = []
    for t in range(0, 120):
        es = [ent(k, s, -1, hp, x, y, 0, 13) for k, s, x, y, hp in towers]
        for k, cid, hp, x, y, t0, st in units:
            if t >= t0:
                es.append(ent(k, 0, cid, hp, x, y, st if st != 4 or t < t0 + 20 else 1, 14))
        frames.append({"tick": t, "entities": es, "effects": []})
    return header, frames


@needs_modern_cards
def test_evo_skeletons_copies_of_copies_and_full_ticks_are_no_plays(m, maker_inputs, tmp_path):
    """EVO COPIES on a built battle: every group born out of its deploy near a living member is copies (spawned,
    `copy_of`), a copy of a young copy and three on one tick too, and a copy shown inside a Skeletons tap's window
    leaves the tap to the base play it belongs to (20261007-112004-A t1143 took the tap of t1132 from the Skeletons
    of t1153). The play itself stays the one Evo Skeletons row."""
    _skip_without_the_id_table(m)
    id_table, doc, name_to_id, card_names = maker_inputs
    header, frames = _evo_skeleton_battle(m, doc)
    path = tmp_path / ("frames-synthetic" + m.CAPTURE_SUFFIX)
    _write_capture(path, header, frames)
    log = tmp_path / "placements-synthetic.jsonl"
    log.write_text(json.dumps({"tick": 0, "local_side_native": 0}) + "\n"
                   + json.dumps({"tick": 70, "card": "Skeletons", "requested": [4.5, 20.5]}) + "\n", encoding="utf-8")
    fx = m.build(str(path), [str(log)], 1, None, None, id_table, doc, {}, name_to_id, card_names, {})
    rows = [(d["tick"], d["keys"], d.get("form"), (d.get("tap") or {}).get("tick")) for d in fx["deploys"]]
    assert rows == [(10, [10, 11, 12], "ev1", None), (91, [40, 41, 42], "base", 70)], rows
    copies = [g["keys"] for g in fx["spawned_groups"] if g.get("copy_of")]
    assert copies == [[30], [31], [32, 33, 34], [35]], fx["spawned_groups"]
    roles = {e["key"]: e["role"] for e in fx["truth"]["entities"] if e["key"] >= 10}
    assert {k for k, r in roles.items() if r == "spawned"} == {30, 31, 32, 33, 34, 35}, roles


def test_a_mirrored_spells_units_are_the_copied_cards(m):
    """A MIRRORED SPELL'S UNITS (module doc): live 20261007-125654-A's three Mirror casts of the Goblin Barrel put down
    goblins under the Mirror's id (28000006, no object of the Mirror card): renamed GoblinBarrel / BarrelGoblin, and
    the Mirror row takes their level 11 (the copied barrel's own level, not the 'mirrored row + 1' 12). The barrel's
    own goblins, a Mirror row of a spell that puts nothing down (Arrows), a group out of the window and one whose hp
    no object of the copied card has are left as they are."""
    doc = {"units": {"BarrelGoblin": {"hitpoints": 79, "rarity": "Common"}}, "evolutions": [], "hero_forms": [],
           "rarities": {"Common": {"relative_level": 0, "multiplier_percent_by_level":
                                   [100, 110, 121, 133, 146, 160, 176, 193, 212, 233, 256, 281, 309, 339]}}}
    barrel = {"name": "GoblinBarrel", "kind": "spell", "level_scaling": {"rarity": "Epic"},
              "spell": {"spawn": {"character": "BarrelGoblin", "count": 3}}}
    cards = {"GoblinBarrel": barrel, "Arrows": {"name": "Arrows", "kind": "spell", "spell": {}},
             "Mirror": {"name": "Mirror", "kind": "spell", "spell": {"mirror": True}}}

    def row(tick, card, cid, kind, mirrored=None):
        d = {"tick": tick, "side": 1, "card": card, "card_id": cid, "kind": kind, "keys": [], "level": 14,
             "level_source": "side mode"}
        if mirrored:
            d["mirrored"] = {"card": mirrored, "card_id": 0}
        return d

    def group(tick, cid, keys, card, hp_match):
        return {"tick": tick, "side": 1, "card": card, "card_id": cid, "role": "spawned", "unit": None,
                "count": len(keys), "keys": keys, "hp_match": hp_match}

    deploys = [
        row(1019, "GoblinBarrel", 28000004, "spell"),
        row(1219, "Mirror", 28000006, "mirror", "GoblinBarrel"),
        row(1500, "Mirror", 28000006, "mirror", "Arrows"),
        row(2000, "Mirror", 28000006, "mirror", "GoblinBarrel"),
    ]
    groups = [group(1077, 28000004, [54], "GoblinBarrel", "exact"),
              group(1277, 28000006, [57, 58, 59], "Mirror", "no_object"),
              group(1520, 28000006, [70], "Mirror", "no_object"), group(2300, 28000006, [80], "Mirror", "no_object")]
    ents = {k: {"key": k, "level": 11, "max_hp": 202, "card": "Mirror", "unit": None, "hp_match": "no_object",
                "role": "spawned"} for k in (57, 58, 59, 80)}
    ents[70] = {**ents[57], "key": 70, "max_hp": 5000}
    ents[54] = {**ents[57], "key": 54, "card": "GoblinBarrel", "unit": "BarrelGoblin", "hp_match": "exact"}
    assert m.mirror_spawns(deploys, groups, ents, doc, cards) == 1
    assert [(ents[k]["card"], ents[k]["unit"], ents[k]["hp_match"]) for k in (57, 58, 59)] == [
        ("GoblinBarrel", "BarrelGoblin", "exact")] * 3
    assert (groups[1]["card"], groups[1]["unit"], groups[1]["hp_match"]) == ("GoblinBarrel", "BarrelGoblin", "exact")
    assert "1219" in groups[1]["mirror_of"]
    assert (deploys[1]["level"], deploys[1]["level_source"]) == (11, "spawn")
    assert (deploys[2]["level"], deploys[2]["level_source"]) == (14, "side mode"), "a Mirror'd Arrows puts nothing down"
    assert (deploys[3]["level"], deploys[3]["level_source"]) == (14, "side mode"), "its group is out of the window"
    assert ents[80]["card"] == ents[70]["card"] == "Mirror"
    assert groups[2]["card"] == groups[3]["card"] == "Mirror"
    assert groups[0] == group(1077, 28000004, [54], "GoblinBarrel", "exact")


REKEY_TOWERS = [  # (key, side, x, y, max_hp, kind): the 16.402 level-11 towers
    (1, 0, 9000, 3000, 4824, 12),
    (2, 0, 3500, 6500, 3052, 13),
    (3, 0, 14500, 6500, 3052, 13),
    (4, 1, 9000, 29000, 4824, 12),
    (5, 1, 3500, 25500, 3052, 13),
    (6, 1, 14500, 25500, 3052, 13),
]


def _truth_row(m, x, y, hp, target=-1):
    r = [0] * len(m.TRUTH_COLUMNS)
    r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")] = x, y
    r[m.TRUTH_COLUMNS.index("hp")], r[m.TRUTH_COLUMNS.index("target")] = hp, target
    return tuple(r)


def test_a_unit_the_converter_rekeyed_across_a_frame_gap_is_one_entity(m):
    """20261007-142635, gap 1628 -> 1690: the converter gave every track a new key (towers 1-6 -> 33-38, the Giant
    Buffer 32 -> 39) because it was last seen more than 20 ticks before; the pointer and card are the same. Each pair
    is one entity again and targets follow. A new pointer (a play inside the gap) stays new, and a pointer a NEW unit of
    the card took inside the gap (20261002-105120.b1: a Tombstone skeleton's, 11,597 away in 76 ticks at speed 90) is
    refused by its reach."""
    ticks = [1626, 1627, 1628, 1690, 1691]
    t = m.TRUTH_COLUMNS.index("target")
    r = _truth_row
    before = [(m, 5931, 28600), (m, 5931, 28550), (m, 5931, 28493)]
    per = [{1: r(m, 3500, 6500, 2352), 32: r(*g, 3091, 1), 50: r(m, 3761, 16836, 67)} for g in before] + [
        {33: r(m, 3500, 6500, 2352), 39: r(m, 5139, 25231, 3091, 33), 40: r(m, 1730, 650, 89, 33),
         51: r(m, 4789, 5285, 67)},
        {33: r(m, 3500, 6500, 2352), 39: r(m, 5139, 25180, 3091, 33), 40: r(m, 1730, 650, 89, 33),
         51: r(m, 4789, 5360, 67)},
    ]

    def ent(key, ptr, cid, side, hp, fi, li, speed):
        return {"key": key, "ptr": ptr, "card_id": cid, "side": side, "level": 13, "max_hp": hp, "kind_first": 15,
                "first_index": fi, "last_index": li, "frames": li - fi + 1, "x0": 0, "y0": 0, "c0": None,
                "speed": speed}

    ents = {
        1: ent(1, "0xT", -1, 0, 3668, 0, 2, 0), 33: ent(33, "0xT", -1, 0, 3668, 3, 4, 0),
        32: ent(32, "0xG", 26000101, 1, 3091, 0, 2, 60), 39: ent(39, "0xG", 26000101, 1, 3091, 3, 4, 60),
        40: ent(40, "0xN", 26000012, 0, 89, 3, 4, 90),
        50: ent(50, "0xS", 27000009, 0, 67, 0, 2, 90), 51: ent(51, "0xS", 27000009, 0, 67, 3, 4, 90),
    }

    def reach(a, gap):
        return int(a["speed"] * m.REJOIN_SPEED_SLACK * gap) + m.REJOIN_BURST_NATIVE

    out = m.rejoin_gap_rekeys(ents, per, ticks, reach)
    assert out == [[1, 33, 1690], [32, 39, 1690]], out
    assert sorted(ents) == [1, 32, 40, 50, 51]
    assert (ents[1]["last_index"], ents[1]["frames"]) == (4, 5)
    assert per[3][1][:3] == (3500, 6500, 2352)
    assert 33 not in per[3]
    assert per[3][32][t] == 1, "targets follow the kept key"
    assert per[4][40][t] == 1, "targets follow the kept key"
    assert ents[50]["last_index"] == 2, "a reused pointer out of reach stays two units"
    assert ents[51]["first_index"] == 3, "a reused pointer out of reach stays two units"
    # a gap the converter does not re-key across (20 ticks or fewer) never reaches here: the keys are the same


@needs_modern_cards
def test_the_reach_of_a_unit_is_its_speed_over_the_gap(m, maker_inputs):
    """`rejoin_reach`: a Skeleton (speed 90) over 76 ticks reaches 90 x 1.3 x 76 + 1,000 = 9,892; the reused pointer of
    105120.b1 stood 11,597 away. A Bat (120) over 21 ticks reaches 4,276; the measured Bats moved 2,520."""
    _, doc, _, _ = maker_inputs
    cards = {c["name"]: c for c in doc["cards"]}
    skel = cards["Skeletons"]
    hp = doc["units"]["Skeleton"]["hitpoints"] * m.ladder_percent(doc, skel, "Skeleton", 9) // 100
    assert m.classify_unit(doc, skel, 9, hp)[0] == "Skeleton"
    assert doc["units"]["Skeleton"]["speed"] == 90
    assert m.rejoin_reach(doc, skel, 9, hp, 76) == int(90 * 1.3 * 76) + 1000 == 9892
    assert 9892 < 11597, "the reused pointer of 105120.b1 is out of reach"
    # a card -1 unit (no card): the table's fastest walk
    assert m.rejoin_reach(doc, None, 11, 629, 21) == int(120 * 1.3 * 21) + 1000 == 4276


@needs_modern_cards
def test_a_cardless_unit_off_the_crown_towers_is_a_buff_death_spawn(m, maker_inputs, tmp_path):
    """The Mother Witch's hog (card -1, hp 629 at level 11) first seen mid-battle off every crown tower's point is
    `cardless_unit`, role spawned (harness.rs CARDLESS_ROOT pairs it with the engine's buff death spawn), in the
    spawned groups, never a seventh tower; the six towers keep their names."""
    frames = []
    for t in range(0, 40):
        ents = [{"id": f"0xT{k}", "generation_key": k, "side": s, "x": x, "y": y, "x2": x, "y2": y, "card_id": -1,
                 "level": 11, "kind": kind, "hp": hp, "max_hp": hp, "behavior_state": 0, "target": None,
                 "path_nodes": []} for k, s, x, y, hp, kind in REKEY_TOWERS]
        if t >= 20:
            ents.append({"id": "0xH", "generation_key": 7, "side": 0, "x": 8000, "y": 12000 + 120 * (t - 20),
                         "x2": 8000, "y2": 12000, "card_id": -1, "level": 11, "kind": 14, "hp": 629, "max_hp": 629,
                         "behavior_state": 1, "target": None, "path_nodes": []})
        frames.append({"tick": t, "entities": ents, "effects": [], "elixir_raw": [50000, 50000]})
    path = tmp_path / "frames-synthetic.native.oracle.jsonl.gz"
    _write_capture(path, [{"side": s, "x": x, "y": y} for _, s, x, y, _, _ in REKEY_TOWERS], frames)
    fx = _build(m, maker_inputs, path)
    by_key = {e["key"]: e for e in fx["truth"]["entities"]}
    hog = by_key[7]
    assert (hog["card"], hog["role"], hog["card_id"]) == ("cardless_unit", "spawned", -1), hog
    assert hog["unit"] == "VoodooHog", hog["unit"]
    assert [g["keys"] for g in fx["spawned_groups"] if g["card"] == "cardless_unit"] == [[7]]
    assert [(by_key[k]["card"], by_key[k]["role"]) for k in (1, 2, 3, 4, 5, 6)] == [
        ("KingTower", "tower"), ("PrincessTower", "tower"), ("PrincessTower", "tower")] * 2
    assert len(fx["towers"]) == 6
    assert not any("towers" in r for r in fx["unplayable_reasons"])


@needs_modern_cards
def test_a_frame_gap_the_converter_rekeyed_leaves_six_towers(m, maker_inputs, tmp_path):
    """20261008-055422, gap 149 -> 232: every tower track took a new key (1-6 -> 7-12). The fixture holds six tower
    entities that run to the end, both Kings named KingTower, and truth_rejoined records the six folds."""
    frames = []
    for t in list(range(0, 30)) + list(range(60, 70)):
        shift = 6 if t >= 60 else 0
        frames.append({"tick": t, "effects": [], "elixir_raw": [50000, 50000], "entities": [
            {"id": f"0xT{k}", "generation_key": k + shift, "side": s, "x": x, "y": y, "x2": x, "y2": y, "card_id": -1,
             "level": 11, "kind": kind, "hp": hp, "max_hp": hp, "behavior_state": 0, "target": None, "path_nodes": []}
            for k, s, x, y, hp, kind in REKEY_TOWERS]})
    path = tmp_path / "frames-synthetic.native.oracle.jsonl.gz"
    _write_capture(path, [{"side": s, "x": x, "y": y} for _, s, x, y, _, _ in REKEY_TOWERS], frames)
    fx = _build(m, maker_inputs, path)
    towers = [e for e in fx["truth"]["entities"] if e["card_id"] == -1]
    assert [(e["key"], e["card"], e["n"]) for e in towers] == [
        (1, "KingTower", 40), (2, "PrincessTower", 40), (3, "PrincessTower", 40),
        (4, "KingTower", 40), (5, "PrincessTower", 40), (6, "PrincessTower", 40)], towers
    assert fx["truth_rejoined"] == [[k, k + 6, 60] for k in range(1, 7)]


def _heal_doc():
    ladder = [100, 110, 121, 133, 146, 160, 176, 193, 212, 233, 256, 281]
    common = {"relative_level": 0, "multiplier_percent_by_level": ladder}
    heal = {"name": "Heal", "kind": "spell", "elixir": 1, "hitpoints": None,
            "spell": {"summon": {"character": "HealSpirit", "count": 1}},
            "level_scaling": {"rarity": "Rare", "multiplier_percent_by_level": [100]}}
    units = {"HealSpirit": {"name": "HealSpirit", "rarity": "Common", "hitpoints": 84, "deploy_time_ms": 1000}}
    rarities = {"Common": common, "Rare": dict(common, relative_level=2)}
    return {"rarities": rarities, "units": units, "cards": [heal]}, heal


def test_a_spell_summons_unit_is_the_cards_object(m):
    """A SPELL SUMMON'S UNIT: the Heal card's HealSpirit (84 base, Common ladder) is one of the card's objects: live
    16.402, a level-11 spirit has 215 hitpoints and a level-12 one 236. It is no deploy summon of the card (a spell is
    cast): spawned, as before, but named."""
    doc, heal = _heal_doc()
    assert m.classify_unit(doc, heal, 11, 215) == ("HealSpirit", False, "exact")
    assert m.classify_unit(doc, heal, 12, 236) == ("HealSpirit", False, "exact")


def test_a_spell_summons_cast_is_read_off_its_unit(m):
    """A SPELL SUMMON'S CAST (module doc): live 20261007-014738 side 0 (no tap log) put a Heal Spirit on (15500, 14500)
    first seen on 697 after a missed 696, deploying 18 frames (its deploy ends 19 ticks after a spawn on 696);
    20261008-170736 side 1 one created on (14500, 17499) and pushed to (14575, 17628) on its first frame 2012. Each is a
    Heal row at its spawn, on its creation point's tile. A spirit a tapped row already stands for (on its first frame),
    and one first seen out of its deploy (a reused key's second unit), are not."""
    doc, heal = _heal_doc()
    ticks = list(range(600, 696)) + list(range(697, 3300))
    ix = {t: i for i, t in enumerate(ticks)}

    def spirit(key, side, t0, c0, xy, deploy_frames, first_state=4):
        fi = ix[t0]
        states = [(fi + k, first_state if k < deploy_frames else 1) for k in range(deploy_frames + 5)]
        return {"key": key, "side": side, "card_id": 28000016, "card": "Heal", "level": 11, "unit": "HealSpirit",
                "role": "spawned", "first_index": fi, "x0": xy[0], "y0": xy[1], "c0": c0, "states": states,
                "positions": [(fi + k, xy[0], xy[1]) for k in range(deploy_frames + 5)]}

    ents = {26: spirit(26, 0, 697, (15500, 14500), (15500, 14500), 18),
            40: spirit(40, 1, 2012, (14500, 17499), (14575, 17628), 19),
            20: spirit(20, 0, 1154, (9500, 500), (9500, 500), 19),
            77: spirit(77, 1, 2500, (3499, 17499), (3499, 17499), 3, first_state=1)}
    groups = [{"tick": ticks[e["first_index"]], "side": e["side"], "card": "Heal", "card_id": 28000016,
               "role": "spawned", "unit": "HealSpirit", "keys": [k]} for k, e in ents.items()]
    tapped = [{"kind": "spell", "side": 0, "card": "Heal", "tick": 1154, "tap": {"tick": 1132}}]
    rows = m.summon_casts(groups, ents, ticks, {"Heal": heal}, tapped, doc)
    got = [(d["tick"], d["first_seen"], d["side"], d["pos"], d["kind"], d["source"], d["spawn_keys"]) for d in rows]
    assert got == [(696, 697, 0, [15500, 14500], "spell", "summon", [26]),
                   (2012, 2012, 1, [14500, 17500], "spell", "summon", [40])], got
    assert all(d["card"] == "Heal" and d["keys"] == [] and d["level"] is None for d in rows)


def _flag_inputs(m, press: bool):
    """A Hero Goblins play on side 0: four Goblin_Stab units (hero class 203) die by 99, their flag stands 100..229
    (role spawned); pressed, it is in cast state from the drop's frame 180 for 9 frames and its Goblin_dummy spawns come
    on 189 and 193 (live 20261008-015445: drop 2452, state 10 2452..2460, dummies 2461 and 2465). Side 0's pool also
    drops 1 on 90 (goblins alive, a dummy 99 ticks later) and on 150 (flag up, no cast state): neither is a press."""
    eff = {"kind": "flag_spawns", "unit": "GoblinHero_Flag_Building",
           "spawns": [{"unit": "Goblin_dummy", "dx_milli": 1000, "dy_milli": -500, "delay_ms": 0},
                      {"unit": "Goblin_dummy", "dx_milli": -1000, "dy_milli": -500, "delay_ms": 200}]}
    doc = {"hero_forms": [{"name": "Goblins_hero", "form_of": "Goblins",
                           "ability": {"name": "GoblinHero_Ability", "mana_cost": 1, "cast_ms": 500, "effect": eff}}]}
    cards = {"Goblins": {"name": "Goblins"}}
    ticks = list(range(0, 260))

    def ent(key, unit, role, t0, t1):
        return {"key": key, "side": 0, "card_id": 203000002, "card": "Goblins", "unit": unit, "role": role,
                "level": 11, "first_index": t0, "last_index": t1}

    ents = {k: ent(k, "Goblin_Stab", "summon", 20, 99) for k in (1, 2, 3, 4)}
    ents[9] = ent(9, "GoblinHero_Flag_Building", "spawned", 100, 196 if press else 229)
    if press:
        ents[10] = ent(10, "Goblin_dummy", "spawned", 189, 240)
        ents[11] = ent(11, "Goblin_dummy", "spawned", 193, 240)
    rows = []
    for t in ticks:
        r = {}
        for k, e in ents.items():
            if e["first_index"] <= t <= e["last_index"]:
                st = 10 if (k == 9 and press and 180 <= t <= 188) else 0
                r[k] = (0, 0, 100, -1, 0, st)
        rows.append(r)
    pool = []
    for t in ticks:
        v = 50000 + 100 * t
        v -= 10000 * ((t >= 90) + (t >= 150) + (press and t >= 180))
        pool.append(v)
    return doc, cards, ents, rows, ticks, {0: pool, 1: [None] * len(ticks)}


def test_a_hero_flags_press_is_read_off_the_flag(m):
    """A FLAG HOLDS ITS FORM'S BUTTON: the Hero Goblins' press is the flag's (its cast state on the drop's frame), not
    its goblins' (dead, and holding no button), written as the hero's press: card Goblins, form hero, keys [the
    flag]."""
    doc, cards, ents, rows, ticks, pools = _flag_inputs(m, press=True)
    got = m.ability_presses([], set(), ents, rows, ticks, pools, doc, cards, {"Goblins": 26000002})
    assert [(d["tick"], d["card"], d["card_id"], d["kind"], d["form"], d["form_row"], d["keys"]) for d in got] == [
        (180, "Goblins", 26000002, "ability", "hero", "Goblins_hero", [9])], got
    assert got[0]["press"]["evidence"] == "elixir drop of 1; cast state 10 at +0"


def test_an_unpressed_flag_writes_no_press(m):
    """A flag never in cast state, whose side's pool drops 1 while it stands, has no press (24 of 24 unpressed flags,
    ob10: none in state 10)."""
    doc, cards, ents, rows, ticks, pools = _flag_inputs(m, press=False)
    assert m.ability_presses([], set(), ents, rows, ticks, pools, doc, cards, {"Goblins": 26000002}) == []


def test_a_ladder_runs_on_past_its_rarity_count(m, doc16402):
    """A level past a rarity's count (rarities.csv lists more multipliers than LevelCount; cards.json `unused_tail`)
    is on the ladder, as the engine reads it (card.rs `level_multiplier`): live, 62 level-17 objects of 11 battles
    stood at 450 % of their base (Common's entry after 409), and with the ladder ending at 16 each was "no_object", a
    deploy summon, so a Lava Hound's pups (378), an Elixir Golem's golemites and a Phoenix's egg were plays."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    assert doc16402["rarities"]["Common"]["unused_tail"][0] == 450
    assert m.classify_unit(doc16402, cards["Valkyrie"], 17, 3352) == ("Valkyrie", True, "exact")
    assert m.classify_unit(doc16402, cards["LavaHound"], 17, 6295) == ("LavaHound", True, "exact")
    assert m.classify_unit(doc16402, cards["LavaHound"], 17, 378) == ("LavaPups", False, "exact")
    assert m.classify_unit(doc16402, cards["Phoenix"], 17, 558) == ("PhoenixEgg", False, "exact")
    assert m.classify_unit(doc16402, cards["ElixirGolem"], 17, 1341) == ("ElixirGolem2", False, "exact")
    # the ladder still ends where its multipliers do: Common lists 19, the last for level 20
    assert m.ladder_percent(doc16402, cards["Knight"], "Knight", 20) == 600
    assert m.ladder_percent(doc16402, cards["Knight"], "Knight", 21) is None


def _zap_row(m, x, y, hp, state=1):
    r = [0] * len(m.TRUTH_COLUMNS)
    for c, v in (("x", x), ("y", y), ("hp", hp), ("state", state)):
        r[m.TRUTH_COLUMNS.index(c)] = v
    return tuple(r)


def _zap_bat(key, side):
    return {"key": key, "side": side, "card_id": 26000049, "card": "Bats", "unit": "Bat", "role": "summon"}


def _zap_tower(key, side, card="PrincessTower"):
    return {"key": key, "side": side, "card_id": -1, "card": card, "unit": None, "role": "tower"}


def _zap_header(side, tick, evidence="elixir", card=None, deck=("Zap",)):
    return {"reveal": {str(side): list(deck), str(1 - side): []},
            "opponent_plays": [{"tick": tick, "side": side, "card": card, "cost": 2, "evidence": evidence}]}


def _zap_snap(p):
    return [p[0] // 1000 * 1000 + 500, p[1] // 1000 * 1000 + 500]


def _zap_bats_die(m, ents, ticks, hit_index, victims, others=None):
    """per-tick rows: every unit of `victims` {key: (x, y, hp)} stands to frame hit_index - 1 and is gone on hit_index;
    every unit of `others` {key: (x, y, hp, hp_after)} stands throughout and reads hp_after from hit_index on."""
    others = others or {}
    rows = []
    for i in range(len(ticks)):
        r = {}
        for k, (x, y, hp) in victims.items():
            if i < hit_index:
                r[k] = _zap_row(m, x, y, hp)
        for k, (x, y, hp, after) in others.items():
            r[k] = _zap_row(m, x, y, hp if i < hit_index else after)
        rows.append(r)
    return rows


# ---------------------------------------------------------------------------------------------------------------------
# THE CAST'S TILE


def test_a_cost_only_zap_goes_on_the_tile_that_spares_what_it_spared(m, doc16402):
    """THE CAST'S TILE (module doc, A COST-ONLY SPELL): three Bats die to a level-16 Zap on t824 and a fourth 2,600
    below their centroid lives (live 20261007-183256 t824, whose centroid's tile also killed bats 24 and 25 in r73's
    engine). The centroid's tile reaches the fourth; the cast goes on the nearest tile centre that reaches the three and
    spares it, (13500, 7500)."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    ticks = [822, 823, 824, 825]
    victims = {21: (12600, 6000, 100), 22: (14400, 6000, 100), 23: (13500, 7600, 100)}
    spared = {24: (13500, 3900, 100, 100)}
    ents = {k: _zap_bat(k, 0) for k in (21, 22, 23, 24)}
    rows = _zap_bats_die(m, ents, ticks, 2, victims, spared)
    deploys = [{"tick": 100, "first_seen": 100, "side": 1, "level": 16}]
    centroid = [sum(v[0] for v in victims.values()) // 3, sum(v[1] for v in victims.values()) // 3]
    body = m.area_body(doc16402, ents[24])
    assert body == (False, 500)
    # the old aim's tile (the engine snaps the centroid to it) reaches the Bat the client spared
    assert m.area_reaches(tuple(_zap_snap(centroid)), (13500, 3900), body, 2500)
    got = m.cost_only_casts(_zap_header(1, 824), deploys, doc16402, cards, {"Zap": 28000008}, ents, rows, ticks, [])
    assert [(d["tick"], d["first_seen"], d["level"], d["pos"]) for d in got] == [(824, 824, 16, [13500, 7500])], got
    pos = tuple(got[0]["pos"])
    assert all(m.area_reaches(pos, v[:2], body, 2500) for v in victims.values())
    assert not m.area_reaches(pos, (13500, 3900), body, 2500)
    assert got[0]["aim"] == "the tile centre reaching 3 of the 3 units its damage hit and 0 of the 1 it spared"


def test_area_reach_is_the_engines(m):
    """`area_reaches` is spell.rs `impact`'s test: a troop's disc edge-inclusive (radius + its collision radius), a
    building's square of half-side its radius and a crown tower's square of half-side 1000, both strictly."""
    troop, cannon, tower = (False, 500), (True, 600), (True, m.CROWN_SQUARE_HALF_NATIVE)
    assert m.area_reaches((0, 0), (3000, 0), troop, 2500)          # 3000 = 2500 + 500: on the edge, in
    assert not m.area_reaches((0, 0), (3001, 0), troop, 2500)
    assert not m.area_reaches((0, 0), (3100, 0), cannon, 2500)     # 2500 from the square: not strictly below
    assert m.area_reaches((0, 0), (3099, 0), cannon, 2500)
    assert not m.area_reaches((2500, 21500), (3500, 25500), tower, 2500)   # 3000 from its square
    assert m.area_reaches((2500, 22500), (3500, 25500), tower, 2500)       # 2000


# ---------------------------------------------------------------------------------------------------------------------
# A CROWN TOWER IS A VICTIM TOO


def test_a_crown_tower_that_took_the_zaps_crown_hit_votes_its_level_and_is_aimed_at(m, doc16402):
    """A CROWN TOWER IS A VICTIM TOO: two Bats killed fit levels 4 to 17 and the side mode is 13, but the princess tower
    took 64, level 14's crown hit (19 on the Common ladder), on the same frame (live 20261007-153404 t982: the old row
    read level 13 and its tile (2500, 21500) left the tower untouched in r73's engine). The cast reads level 14 and goes
    on (2500, 22500), which reaches the tower's square; the king, untouched, is spared."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    zap = cards["Zap"]
    assert [m.instant_crown_hit_at(doc16402, zap, lv) for lv in (11, 13, 14, 16)] == [48, 58, 64, 77]
    assert [m.spell_damage_at(doc16402, zap, lv) for lv in (11, 14, 16)] == [192, 254, 306]
    ticks = [980, 981, 982, 983]
    victims = {33: (2600, 21300, 100), 34: (2900, 21800, 100)}
    others = {5: (3500, 25500, 3000, 2936), 4: (9000, 29000, 4000, 4000)}
    ents = {33: _zap_bat(33, 1), 34: _zap_bat(34, 1), 5: _zap_tower(5, 1), 4: _zap_tower(4, 1, "KingTower")}
    rows = _zap_bats_die(m, ents, ticks, 2, victims, others)
    deploys = [{"tick": 100, "first_seen": 100, "side": 0, "level": 13}]
    got = m.cost_only_casts(_zap_header(0, 982), deploys, doc16402, cards, {"Zap": 28000008}, ents, rows, ticks, [])
    assert [(d["tick"], d["level"], d["pos"]) for d in got] == [(982, 14, [2500, 22500])], got
    assert got[0]["level_evidence"] == (
        "cost-only cast: 2 victims took its level-14 damage on 982, 1 crown towers its crown hit 64"
    )
    assert m.area_reaches(tuple(got[0]["pos"]), (3500, 25500), (True, 1000), 2500)
    # the old aim: the two kills' centroid, whose tile misses the tower
    assert _zap_snap([2750, 21550]) == [2500, 21500]
    assert not m.area_reaches((2500, 21500), (3500, 25500), (True, 1000), 2500)


# ---------------------------------------------------------------------------------------------------------------------
# DATED ON ITS HIT


def test_a_cost_only_zap_is_dated_on_its_hit_and_keeps_its_drop(m, doc16402):
    """The cast is DATED ON ITS HIT's frame: the drop shows on 371, frame 372 is missing, the Bats are gone on 373 (live
    20261011-213425 t371, where r73's engine landed the Zap on 371 and the client's damage shows on 373). The row is
    dated 373 (the engine lands an instant area on its row's tick) and `first_seen` keeps 371, so the drop is still
    explained: no press reads it, and a second payment beside it is not a second cast."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    ticks = [369, 370, 371, 373, 374]
    victims = {8: (5500, 8000, 100), 9: (5800, 8600, 100)}
    ents = {8: _zap_bat(8, 0), 9: _zap_bat(9, 0)}
    rows = _zap_bats_die(m, ents, ticks, 3, victims)
    deploys = [{"tick": 100, "first_seen": 100, "side": 1, "level": 16}]
    got = m.cost_only_casts(_zap_header(1, 371), deploys, doc16402, cards, {"Zap": 28000008}, ents, rows, ticks, [])
    assert [(d["tick"], d["first_seen"]) for d in got] == [(373, 371)], got
    assert got[0]["tick_evidence"].startswith("its damage on 373; ")
    again = m.cost_only_casts(
        _zap_header(1, 370), deploys + got, doc16402, cards, {"Zap": 28000008}, ents, rows, ticks, []
    )
    assert again == []


# ---------------------------------------------------------------------------------------------------------------------
# A PRESS THE SPELL'S DAMAGE OVERRULES


def _zap_press_inputs(m, doc16402, hero="IceGolemite", hero_id=203000038, cast_state_on_drop=False, victims=None,
                  tower_after=3000 - 77):
    """Side 0 holds a hero of `hero` (alive throughout) and pays 2 on t745, which the live reader logged as its press;
    on that frame three side-1 Bats die and a princess tower loses 77 (live 20261008-064926 t745)."""
    ticks = list(range(740, 750))
    hit = ticks.index(745)
    if victims is None:
        victims = {27: (14000, 22000, 100), 28: (14600, 22400, 100), 29: (15000, 22100, 100)}
    others = {6: (14500, 25500, 3000, tower_after)}
    ents = {k: _zap_bat(k, 1) for k in victims}
    ents[6] = _zap_tower(6, 1)
    ents[12] = {"key": 12, "side": 0, "card_id": hero_id, "card": hero, "unit": hero, "role": "summon", "level": 16,
                "first_index": 0, "last_index": len(ticks) - 1}
    rows = _zap_bats_die(m, ents, ticks, hit, victims, others)
    for i, r in enumerate(rows):
        state = m.CAST_STATE if (cast_state_on_drop and i >= hit) else 1
        r[12] = _zap_row(m, 9000, 20000, 1000, state)
    pool = [50_000 + 100 * t - 2 * m.ELIXIR_UNIT * (t >= 745) for t in ticks]
    deploys = [{"tick": 100, "first_seen": 100, "side": 0, "level": 16, "kind": "troop", "card": hero}]
    return ticks, ents, rows, {0: pool, 1: [None] * len(ticks)}, deploys


def test_a_zap_paid_beside_a_hero_is_the_zap_not_its_press(m, doc16402):
    """A PRESS THE SPELL'S DAMAGE OVERRULES: the live reader logged the payment of t745 as the Hero Ice Golem's press
    (its ability costs 2 and holds no cast), and ed83c924 wrote that press, so the engine cast no Zap. Its level-16
    damage shows on the drop's frame on three Bats and a crown tower (77): the payment is the Zap, and
    `ability_presses`, which without the cast row writes the press, then reads no press off that drop."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    ticks, ents, rows, pools, deploys = _zap_press_inputs(m, doc16402)
    hdr = _zap_header(0, 745, "ability", "IceGolemite", ("Zap", "IceGolemite"))
    got = m.cost_only_casts(hdr, deploys, doc16402, cards, {"Zap": 28000008}, ents, rows, ticks, [])
    assert [(d["tick"], d["first_seen"], d["card"], d["level"], d["source"]) for d in got] == [
        (745, 745, "Zap", 16, "cost_only")], got
    assert "logged as a IceGolemite press" in got[0]["tick_evidence"]
    assert got[0]["level_evidence"].endswith("1 crown towers its crown hit 77")
    names = {"IceGolemite": 26000038}
    # the press is what ed83c924 writes off that drop with no cast row...
    presses = m.ability_presses([], set(), ents, rows, ticks, pools, doc16402, cards, names)
    assert [(d["tick"], d["card"], d["press"]["evidence"]) for d in presses] == [
        (745, "IceGolemite", "elixir drop of 2; no cast hold")], presses
    # ...and no press is read once the Zap's row explains the drop
    assert m.ability_presses(got, set(), ents, rows, ticks, pools, doc16402, cards, names) == []


def test_an_ability_payment_needs_the_spells_proof(m, doc16402):
    """The press keeps a payment the spell's damage does not prove: one victim only; a level far from the side's mode
    (two units losing 110 fit only level 5's Zap, and 110 is the level-16 Hero Ice Golem's own pulse: live
    20261008-064926 t1239, pulses on +1 and +31), which an "elixir" payment would still read as a level-5 Zap; or a
    hero of that cost with a cast hold (the Hero Knight, 1200 ms) entering CAST_STATE on the drop's frame."""
    cards = {c["name"]: c for c in doc16402["cards"]}
    names = {"Zap": 28000008}
    hdr = _zap_header(0, 745, "ability", "IceGolemite", ("Zap", "IceGolemite"))
    # one victim, no tower hit
    ticks, ents, rows, _, deploys = _zap_press_inputs(m, doc16402, victims={27: (14000, 22000, 100)}, tower_after=3000)
    assert m.cost_only_casts(hdr, deploys, doc16402, cards, names, ents, rows, ticks, []) == []
    header = _zap_header(0, 745, deck=("Zap",))
    assert len(m.cost_only_casts(header, deploys, doc16402, cards, names, ents, rows, ticks, [])) == 1
    # two survivors losing 110: level 5's Zap, eleven levels below the side's 16
    ticks, ents, rows, _, deploys = _zap_press_inputs(m, doc16402, victims={}, tower_after=3000)
    for i, r in enumerate(rows):
        for k, x in ((40, 14000), (41, 14800)):
            r[k] = _zap_row(m, x, 22000, 500 if i < ticks.index(745) else 390)
    ents[40], ents[41] = _zap_bat(40, 1), _zap_bat(41, 1)
    assert m.cost_only_casts(hdr, deploys, doc16402, cards, names, ents, rows, ticks, []) == []
    header = _zap_header(0, 745, deck=("Zap",))
    elixir = m.cost_only_casts(header, deploys, doc16402, cards, names, ents, rows, ticks, [])
    assert [d["level"] for d in elixir] == [5], elixir
    # the Hero Knight's press shows: it holds the drop although three Bats and a tower took the Zap's damage
    held = _zap_press_inputs(m, doc16402, hero="Knight", hero_id=203000000, cast_state_on_drop=True)
    ticks, ents, rows, _, deploys = held
    hdr_k = _zap_header(0, 745, "ability", "Knight", ("Zap", "Knight"))
    assert m.cost_only_casts(hdr_k, deploys, doc16402, cards, names, ents, rows, ticks, []) == []
    ticks, ents, rows, _, deploys = _zap_press_inputs(m, doc16402, hero="Knight", hero_id=203000000)
    assert len(m.cost_only_casts(hdr_k, deploys, doc16402, cards, names, ents, rows, ticks, [])) == 1


# ---------------------------------------------------------------------------------------------------------------------
# the existing test's one changed expectation


def _snow_table():
    if not os.path.exists(TABLE_16402):
        pytest.skip(f"{TABLE_16402} is absent (the 2026-10-06 table) -- a skip here is not a pass")
    with open(TABLE_16402, encoding="utf-8") as fh:
        doc = json.load(fh)
    return doc, {c["name"]: c for c in doc["cards"]}


def _snow_row(m, x, y, hp):
    r = [0] * len(m.TRUTH_COLUMNS)
    r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = x, y, hp
    return tuple(r)


def _snow_timeline(m, ticks, ents_hp):
    """per_tick_rows from {key: (x, y, {tick: hp after that tick's drop})}: each victim stands still, its hp the last
    value at or before the frame's tick."""
    rows = []
    for t in ticks:
        fr = {}
        for k, (x, y, hps) in ents_hp.items():
            hp = [h for tt, h in sorted(hps.items()) if tt <= t]
            if hp and hp[-1] is not None:
                fr[k] = _snow_row(m, x, y, hp[-1])
        rows.append(fr)
    return rows


def _snow_evo(doc):
    return next(r for r in doc["evolutions"] if r["name"] == "Snowball_EV1")


def _snow_flight(first, last_seen, arrival, target, launch=(9000, 3000)):
    return {"first": first, "launch": list(launch), "depart": first, "target": list(target), "last_seen": last_seen,
            "end": list(target), "arrival": arrival}


def test_an_evolved_snowball_is_read_off_its_ball(m):
    """AN EVOLVED ROLL CAST (module doc): the Evo Giant Snowball's flight lands a ball that rolls 4000 on (its
    evolution's flight spawns SnowballSpell_EV1_Rolling; the base Snowball's spawns nothing). The capture shows it as a
    second object first seen on the flight's arrival frame, on the flight's target (live 20261008-163400-A t1545: the
    flight last seen 1554, the ball first seen 1555 on (10500, 11500), rolling to (10500, 15500); 49 of 49 two-object
    Snowball casts of the live set, every one the third play of its side's bbE cycle). One object, a ball off the
    target or seen after the arrival, a card whose base already rolls (the Log) and an evolution with no capture
    roll are no evolved roll."""
    doc, cards = _snow_table()
    evo, base = _snow_evo(doc), cards["Snowball"]
    flight = _snow_flight(1545, 1554, 1555, (10500, 11500))
    ball = {"first": 1555, "launch": [10500, 11500], "depart": 1556, "target": [10500, 15500], "last_seen": 1567,
            "end": [10500, 15100], "arrival": 1569}
    why = m.evo_roll_cast({"tracks": [flight, ball]}, evo, base)
    assert why is not None, why
    assert "ball first seen on 1555" in why, why
    # a frame gap over the landing (20261009-025346-A t4206: flight last seen 4217, frame 4218 missing, the ball on
    # 4219)
    gap = ({**flight, "last_seen": 1553, "arrival": 1555}, ball)
    assert m.evo_roll_cast({"tracks": list(gap)}, evo, base) is not None
    assert m.evo_roll_cast({"tracks": [flight]}, evo, base) is None, "a base cast is one object"
    assert m.evo_roll_cast({"tracks": [flight, {**ball, "launch": [10500, 12000]}]}, evo, base) is None
    assert m.evo_roll_cast({"tracks": [flight, {**ball, "first": 1556}]}, evo, base) is None
    assert m.evo_roll_cast({"tracks": [flight, ball]}, evo, cards["Log"]) is None, "a base card that rolls itself"
    zap = next(r for r in doc["evolutions"] if r["name"] == "Zap_EV1")
    assert m.evo_roll_cast({"tracks": [flight, ball]}, zap, cards["Zap"]) is None
    # the rule reaches the Snowball alone today: no other evolution's flight spawns a roll its base card's does not
    rolls = [
        r["form_of"] for r in doc["evolutions"]
        if r.get("evo_capture") and (r.get("projectile") or {}).get("spawn_projectile")
        and not ((cards.get(r["form_of"]) or {}).get("projectile") or {}).get("spawn_projectile")
    ]
    assert rolls == ["Snowball"], rolls


def _snow_cast(objects, level=10, side=0, pos=(10500, 11500), **kw):
    return {"tick": objects[0]["first"], "first_seen": objects[0]["first"], "side": side, "card": "Snowball",
            "kind": "spell", "level": level, "level_source": "side mode", "pos": list(pos), "source": "effect",
            "objects": objects, **kw}


def test_a_projectile_spells_level_is_read_on_its_landing(m):
    """A SPELL'S HIT FRAMES (module doc): a projectile spell's hit is its object's landing, last_seen + 1 (live
    20261009-032259-A t2729: a Musketeer's 164 on 2736, inside the Snowball's flight, fitted level 10 and was read; the
    Snowball itself took 135, level 8, on its arrival 2743). The live set: 301 of 312 Snowball drops that fit a level
    fall on the landing, 921 of 969 Fireball and 99 of 101 Rocket ones. A drop on the landing frame alone reads, and a
    cast whose object never lands inside the capture is no read."""
    doc, cards = _snow_table()
    sb = cards["Snowball"]
    at8, at10 = m.spell_damage_at(doc, sb, 8), m.spell_damage_at(doc, sb, 10)
    assert (at8, at10) == (135, 163)
    ticks = list(range(2729, 2750))
    ents = {7: {"key": 7, "side": 1, "card_id": 26000000}}
    rows = _snow_timeline(m, ticks, {7: (14500, 13500, {2729: 1000, 2736: 1000 - 164, 2743: 1000 - 164 - at8})})
    d = _snow_cast([_snow_flight(2729, 2742, 2743, (14500, 13500))], pos=(14500, 13500))
    m.spell_levels_from_damage([d], doc, cards, ents, rows, ticks)
    assert (d["level"], d["level_source"]) == (8, "damage"), d
    assert d["level_evidence"].startswith(f"hit on 2743: drops [{at8}]"), d["level_evidence"]
    # the same cast without its objects keeps the first drop after its tick (the rule before): the chip reads 10
    legacy = {k: v for k, v in _snow_cast([_snow_flight(2729, 2742, 2743, (14500, 13500))], pos=(14500, 13500)).items()
              if k not in ("objects", "source")}
    m.spell_levels_from_damage([legacy], doc, cards, ents, rows, ticks)
    assert legacy["level"] == 10, legacy
    # a landing frame lost to a gap: the drop shows on the next frame present
    gap_ticks = [t for t in ticks if t != 2743]
    gap_rows = [r for t, r in zip(ticks, rows, strict=False) if t != 2743]
    d = _snow_cast([_snow_flight(2729, 2742, 2744, (14500, 13500))], pos=(14500, 13500))
    m.spell_levels_from_damage([d], doc, cards, ents, gap_rows, gap_ticks)
    assert d["level"] == 8, d
    # never lands inside the capture: no read, the side mode stays
    d = _snow_cast([_snow_flight(2729, 2749, None, (14500, 13500))], pos=(14500, 13500))
    m.spell_levels_from_damage([d], doc, cards, ents, rows, ticks)
    assert (d["level"], d["level_source"]) == (10, "side mode"), d
    assert "never land" in d["level_evidence"]


def test_an_evolved_snowballs_level_is_read_on_its_capture(m):
    """The Evo Giant Snowball's flight strikes nothing (SnowballSpell_EV1, damage 0): its ball hits and captures
    CAPTURE_DELAY_TICKS after the flight's landing, 600 on along its roll (card.rs CaptureRollDef; live: 35 of 42
    evolved casts with a fitting drop have one on last_seen + 3 or the next frame present). Live 20261008-005044-A
    t2603: the Little Prince took 286 (level 16) on 2619, where the side mode was 15."""
    doc, cards = _snow_table()
    sb = cards["Snowball"]
    at15, at16 = m.spell_damage_at(doc, sb, 15), m.spell_damage_at(doc, sb, 16)
    assert (at15, at16) == (260, 286)
    flight = _snow_flight(2603, 2616, 2617, (13500, 13500))
    ball = {"first": 2617, "launch": [13500, 13500], "depart": 2618, "target": [13500, 17500], "last_seen": 2630,
            "end": [13500, 17400], "arrival": 2631}
    ticks = list(range(2603, 2640))
    ents = {7: {"key": 7, "side": 1, "card_id": 26000000}, 8: {"key": 8, "side": 1, "card_id": 26000001}}
    # a tower shot of 173 on the landing frame on another victim, the ball's 286 two ticks later
    rows = _snow_timeline(m, ticks, {7: (13600, 14300, {2603: 3000, 2619: 3000 - at16}),
                                8: (13500, 12400, {2603: 900, 2617: 900 - 173})})
    d = _snow_cast([flight, ball], level=15, pos=(13500, 13500), form="ev1", form_row="Snowball_EV1")
    m.spell_levels_from_damage([d], doc, cards, ents, rows, ticks)
    assert (d["level"], d["level_source"]) == (16, "damage"), d
    assert d["level_evidence"].startswith(f"hit on 2619: drops [{at16}]"), d["level_evidence"]


def test_a_rolling_spells_level_is_read_along_its_roll(m):
    """A rolling spell (the Log, the Barbarian Barrel) hits along its ball's line while the ball rolls (live
    20261007-005625-A t883: a Goblinstein 3752 from the landing point took the Log's 295, level 12, on 903; the old
    reach around the landing missed it and read a 246 on 978, after the roll, as level 10). The live set: 771 of
    956 Log drops on troops that fit a level inside the ball's band and frames stand 600 to 1800 ahead of it."""
    doc, cards = _snow_table()
    log = cards["Log"]
    at10, at12 = m.spell_damage_at(doc, log, 10), m.spell_damage_at(doc, log, 12)
    assert (at10, at12) == (244, 295)
    airborne = {"first": 883, "launch": [14500, 6500], "depart": 883, "target": [14500, 9500], "last_seen": 890,
                "end": [14500, 9380], "arrival": 891}
    ball = {"first": 891, "launch": [14500, 9500], "depart": 892, "target": [14500, 19600], "last_seen": 941,
            "end": [14500, 19500], "arrival": 942}
    ticks = list(range(883, 990))
    ents = {20: {"key": 20, "side": 1, "card_id": 26000000}, 21: {"key": 21, "side": 1, "card_id": 26000001}}
    rows = _snow_timeline(m, ticks, {20: (14229, 13242, {883: 3000, 903: 3000 - at12}),
                                21: (14231, 9726, {883: 2000, 978: 2000 - 246})})
    d = {"tick": 883, "first_seen": 883, "side": 0, "card": "Log", "kind": "spell", "level": 10,
         "level_source": "side mode", "pos": [14500, 9500], "source": "effect", "objects": [airborne, ball]}
    m.spell_levels_from_damage([d], doc, cards, ents, rows, ticks)
    assert (d["level"], d["level_source"]) == (12, "damage"), d
    assert "hit on 903" in d["level_evidence"], d["level_evidence"]
    assert "978" not in d["level_evidence"], d["level_evidence"]


def test_a_volleys_level_is_read_on_its_first_waves_landings(m):
    """An Arrows volley is read on its first wave's landings (live 20261008-174803-A t722: a Musketeer's 217 on 737,
    inside the flight, fitted level 17; the first wave's 122 on 739 is level 11). The live set: 1,404 drops that fit a
    level fall on a wave's first landing, 809 one tick after."""
    doc, cards = _snow_table()
    ar = cards["Arrows"]
    assert (m.spell_damage_at(doc, ar, 11), m.spell_damage_at(doc, ar, 17)) == (122, 216)

    def arrow(depart, last_seen, target):
        return {"first": 722, "launch": [9000, 29000], "depart": depart, "target": list(target),
                "last_seen": last_seen, "end": list(target), "arrival": last_seen + 1}

    objs = [arrow(722, 738, (14600, 10400)), arrow(722, 739, (14200, 10500)), arrow(726, 742, (14400, 10400))]
    ticks = list(range(722, 760))
    ents = {13: {"key": 13, "side": 0, "card_id": 26000014}}
    rows = _snow_timeline(m, ticks, {13: (14400, 10500, {722: 900, 737: 900 - 217, 739: 900 - 217 - 122,
                                                    743: 900 - 217 - 244})})
    d = {"tick": 722, "first_seen": 722, "side": 1, "card": "Arrows", "kind": "spell", "level": 17,
         "level_source": "side mode", "pos": [14366, 10460], "source": "effect", "objects": objs}
    m.spell_levels_from_damage([d], doc, cards, ents, rows, ticks)
    assert (d["level"], d["level_source"]) == (11, "damage"), d


def test_a_laser_balls_level_is_read_off_its_strike_trains(m):
    """DarkMagic (the Void) deals its strike as one pulse of a count tier's buff (`strike_pulses_at`), every 1000 ms:
    at 15, 1011 on a lone victim, 429 on two to four, 223 on more. Read as a pulsing spell's trains (live
    20261008-041851-A t1472: a Giant Buffer took 429 on 1524 and 1544, level 15, where the side mode was 16; 602 on 1504
    is 429 + a tower's 173). The live set: 6 of 14 casts read, 3 of them off the side mode."""
    doc, cards = _snow_table()
    dm = cards["DarkMagic"]
    assert m.strike_pulses_at(doc, dm, 15) == [1011, 429, 223]
    assert m.strike_pulses_at(doc, dm, 16) == [1112, 472, 245]
    assert m.strike_pulses_at(doc, cards["Poison"], 13) == [], "an area buff's pulse is spell_pulse_at's"
    assert m.pulse_period_ticks(dm) == 20
    assert m.pulse_period_ticks(cards["Poison"]) == 20
    ticks = list(range(1472, 1600))
    ents = {26: {"key": 26, "side": 1, "card_id": 26000000}, 43: {"key": 43, "side": 1, "card_id": 26000001}}
    rows = _snow_timeline(m, ticks, {26: (3500, 13900, {1472: 4000, 1504: 4000 - 602, 1524: 4000 - 602 - 429,
                                                   1544: 4000 - 602 - 858}),
                                43: (3300, 13200, {1472: 2000, 1504: 2000 - 429, 1524: 2000 - 858, 1544: 2000 - 1287})})
    d = {"tick": 1472, "first_seen": 1472, "side": 0, "card": "DarkMagic", "kind": "spell", "level": 16,
         "level_source": "side mode", "pos": [3500, 13500], "source": "effect"}
    m.spell_levels_from_damage([d], doc, cards, ents, rows, ticks)
    assert (d["level"], d["level_source"]) == (15, "damage"), d
    assert "level votes {15: 5}" in d["level_evidence"], d["level_evidence"]


def test_a_read_its_sides_other_reads_outvote_takes_their_level(m):
    """ONE LEVEL PER CARD PER SIDE: a card has one level in a battle, so a read the side's other reads of the card
    outvote is a coincidence (live 20261008-174111-A side 1: Snowballs read 11, 11, 11, and 16 off a Hog Rider's 288,
    179 + another hit) and takes their level; a tie keeps each read."""
    doc, cards = _snow_table()
    sb = cards["Snowball"]
    at11, at16 = m.spell_damage_at(doc, sb, 11), m.spell_damage_at(doc, sb, 16)
    ticks = list(range(100, 200))
    ents = {k: {"key": k, "side": 1, "card_id": 26000000 + k} for k in (1, 2, 3, 4)}
    pts = {1: (3500, 13500), 2: (14500, 13500), 3: (3500, 20500), 4: (14500, 20500)}
    drops = {1: at11, 2: at11, 3: at11, 4: at16 + 2}
    rows = _snow_timeline(m, ticks, {k: (*pts[k], {100: 3000, 120 + k: 3000 - drops[k]}) for k in pts})
    casts = [_snow_cast([_snow_flight(100 + k, 119 + k, 120 + k, pts[k])], level=11, side=0, pos=pts[k])
             for k in pts]
    m.spell_levels_from_damage(casts, doc, cards, ents, rows, ticks)
    assert [d["level"] for d in casts] == [11, 11, 11, 11], [d["level_evidence"] for d in casts]
    assert casts[3]["level_source"] == "card level"
    assert "outvoted" in casts[3]["level_evidence"]
    # one read each way: no majority, each keeps its own
    two = [_snow_cast([_snow_flight(100 + k, 119 + k, 120 + k, pts[k])], level=11, side=0, pos=pts[k]) for k in (1, 4)]
    m.spell_levels_from_damage(two, doc, cards, ents, rows, ticks)
    assert [d["level"] for d in two] == [11, 16], [d["level_evidence"] for d in two]


@pytest.fixture(scope="module")
def table15535():
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    return doc, {c["name"]: c for c in doc["cards"]}


def _hp_row(m, x, y, hp):
    r = [0] * len(m.TRUTH_COLUMNS)
    r[m.TRUTH_COLUMNS.index("x")], r[m.TRUTH_COLUMNS.index("y")], r[m.TRUTH_COLUMNS.index("hp")] = x, y, hp
    return tuple(r)


def test_an_unread_hp_is_the_readers_minus_one_on_a_unit_seen_again(m):
    """AN UNREAD HP: the reader's -1 on a frame the unit is seen on again later is unread (an overheal); on its last
    frame it is its death frame; a read hp is never unread."""
    rows = [
        {7: _hp_row(m, 0, 0, 122)}, {7: _hp_row(m, 0, 0, -1)}, {7: _hp_row(m, 0, 0, 102)}, {},
        {8: _hp_row(m, 0, 0, -1)},
    ]
    assert m.hp_unread(rows, 7, 1)
    assert not m.hp_unread(rows, 7, 0)
    assert not m.hp_unread(rows, 7, 2)
    assert not m.hp_unread(rows, 8, 4), "a -1 on the unit's last frame is its death frame"
    assert not m.hp_unread(rows, 9, 1), "no row, nothing to read"


@needs_modern_cards
def test_an_overheal_read_as_minus_one_is_no_kill_bounding_a_spells_level(m, table15535):
    """Live 20261008-163400 (side 0's Fireball of t2864): an Evo Witch healed above her max reads -1 from 2867 and 476
    from 2888, where the Fireball's 473 lands on a Knight and a Dart Goblin. Read as a kill of last hp 695 on 2867 it
    bounded the level from below (fits 12-17, written 12); the side's four other Fireballs read 7 by damage. The -1 is
    no kill, so the hit is the Knight's drop."""
    doc, cards = table15535
    dmg4 = m.spell_damage_at(doc, cards["Fireball"], 4)
    ticks = [100, 101, 102, 103, 104, 105]
    ents = {7: {"key": 7, "side": 1, "card_id": 26000007}, 8: {"key": 8, "side": 1, "card_id": 26000000}}

    def frames(witch):
        knight = [1000, 1000, 1000, 1000 - dmg4, 1000 - dmg4, 1000 - dmg4]
        out = []
        for i in range(len(ticks)):
            r = {8: _hp_row(m, 6600, 17400, knight[i])}
            if witch[i] is not None:
                r[7] = _hp_row(m, 6500, 17500, witch[i])
            out.append(r)
        return out

    def cast():
        return {"tick": 100, "side": 0, "card": "Fireball", "kind": "spell", "level": 3,
                "level_source": "side mode", "pos": [6500, 17500]}

    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, frames([600, 600, -1, -1, 650, 650]), ticks)
    assert (d["level"], d["level_source"]) == (4, "damage"), d
    assert d["level_evidence"].startswith(f"hit on 103: drops [{dmg4}]"), d["level_evidence"]
    # the control: the same -1 on her LAST frame is her death frame, still a kill of last hp 600 on 102
    d = cast()
    m.spell_levels_from_damage([d], doc, cards, ents, frames([600, 600, -1, None, None, None]), ticks)
    assert "hit on 102" in d["level_evidence"], d["level_evidence"]
    assert "kills of last hp [600]" in d["level_evidence"], d["level_evidence"]


@needs_modern_cards
def test_an_overheal_read_as_minus_one_is_no_victim_of_a_cost_only_zap(m, table15535):
    """A cost-only Zap is aimed at the tile that reaches its victims (A COST-ONLY SPELL). An Evo Bat that heals above
    its max on the Zap's frame (reader -1) and flies on is no victim; the Knight that took the Zap's damage is the
    only one, and the tile the one of the four around him nearest first in x, then y."""
    doc, cards = table15535
    dmg = m.spell_damage_at(doc, cards["Zap"], 11)
    ticks = [280, 281, 282, 283]
    ents = {7: {"key": 7, "side": 1, "card_id": 26000049}, 8: {"key": 8, "side": 1, "card_id": 26000000}}
    before = {7: _hp_row(m, 14000, 17000, dmg // 2), 8: _hp_row(m, 15000, 17000, 300 + dmg)}
    hit = {7: _hp_row(m, 14000, 17000, -1), 8: _hp_row(m, 15000, 17000, 300)}
    rows = [before, before, hit, {7: _hp_row(m, 14000, 17000, dmg // 2 + 30), 8: _hp_row(m, 15000, 17000, 300)}]
    header = {"reveal": {"0": ["Zap", "Knight"], "1": []},
              "opponent_plays": [{"tick": 282, "side": 0, "card": None, "cost": 2, "evidence": "elixir"}]}
    got = m.cost_only_casts(header, [], doc, cards, {"Zap": 28000008}, ents, rows, ticks, [])
    assert [(d["tick"], d["pos"]) for d in got] == [(282, [14500, 16500])], got
    assert got[0]["level_evidence"].startswith("cost-only cast: 1 victims"), got[0]["level_evidence"]
    # the control: the bat not seen again (its -1 is its death frame) is a victim, so the cast has two
    rows[3] = {8: _hp_row(m, 15000, 17000, 300)}
    got = m.cost_only_casts(header, [], doc, cards, {"Zap": 28000008}, ents, rows, ticks, [])
    assert [(d["tick"], d["pos"]) for d in got] == [(282, [14500, 16500])], got
    assert got[0]["level_evidence"].startswith("cost-only cast: 2 victims"), got[0]["level_evidence"]


def test_the_rng_column_is_the_frames_state_as_the_specials_fixtures_carry_it(m):
    """THE CLIENT'S GENERATOR: the first state and every frame tick it changes on; the algorithm by the header's
    rng_algorithm tail, else by the client build; a state read as a signed int is its uint32; none without states."""
    frames = [{"tick": 3, "rng_state": 7}, {"tick": 4, "rng_state": 7}, {"tick": 6, "rng_state": -2},
              {"tick": 7}, {"tick": 8, "rng_state": -2}]
    got = m.client_rng_column({"rng_algorithm": "libg_xorshift32_v160402020"}, frames)
    assert got["algorithm"] == "client_xorshift32_v160402020"
    assert (got["first"], got["changes"]) == ([3, 7], [[6, 4294967294]])
    assert m.client_rng_column({"client": {"build": 160402012}}, frames)["algorithm"] == "client_xorshift32_v160402012"
    assert m.client_rng_column({"client": {"build": 160402012}}, [{"tick": 3}, {"tick": 4}]) is None


HP_TOWERS = [(1, 0, 9000, 3000, 4824), (2, 0, 3500, 6500, 3052), (3, 0, 14500, 6500, 3052),
          (4, 1, 9000, 29000, 4824), (5, 1, 3500, 25500, 3052), (6, 1, 14500, 25500, 3052)]


def _hp_capture(path, knight, rng=None, header_extra=None):
    """Six towers and a Knight (key 7) from frame 1; knight[i] = (hp, max_hp, level) on frame i, ticks 0..n-1."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with gzip.open(path, "wt", encoding="utf-8") as fh:
        header = {"record": "header", "towers": [{"side": s, "x": x, "y": y} for _, s, x, y, _ in HP_TOWERS],
                  "local_side_native": 0, **(header_extra or {})}
        fh.write(json.dumps(header) + "\n")
        for i, (hp, mx, lv) in enumerate(knight):
            ents = [{"id": f"p{k}", "generation_key": k, "side": s, "x": x, "y": y, "x2": x, "y2": y, "card_id": -1,
                     "level": 11, "kind": 13, "hp": h, "max_hp": h, "behavior_state": 0, "target": None,
                     "path_nodes": [], "attack_progress_ms": 0, "attack_load_timer_ms": 0, "event_timer_ms": 0,
                     "attack_component_valid": True} for k, s, x, y, h in HP_TOWERS]
            if i >= 1:
                ents.append({"id": "p7", "generation_key": 7, "side": 0, "x": 3400, "y": 12000 + 60 * i,
                             "x2": 3400, "y2": 12000 + 60 * (i - 1), "card_id": 26000000, "level": lv,
                             "kind": 14 if i < 3 else 15, "hp": hp, "max_hp": mx,
                             "behavior_state": 4 if i < 3 else 1, "target": None, "path_nodes": [],
                             "attack_progress_ms": 0, "attack_load_timer_ms": 0, "event_timer_ms": 0,
                             "attack_component_valid": True})
            state = {"tick": i, "entities": ents, "effects": []}
            if rng is not None:
                state["rng_state"] = rng[i]
            fh.write(json.dumps({"record": "frame", "state": state}) + "\n")


def _hp_build(m, path):
    with open(CARDS, "rb") as fh:
        raw = fh.read()
    doc = json.loads(raw.decode("utf-8"))
    doc["_fnv1a64"] = m.fnv1a64(raw)
    id_table = {} if m.missing_id_files() else m.load_id_table()
    card_names = {c["name"] for c in doc["cards"]}
    return m.build(str(path), [], 1, None, None, id_table, doc, {}, m.base_ids(id_table, card_names), card_names, {})


@needs_modern_cards
def test_a_capture_publishes_its_unread_hp_level_changes_and_generator(m, tmp_path):
    """AN UNREAD HP, A LEVEL CHANGE, THE CLIENT'S GENERATOR on one synthetic capture: the Knight's -1 frames stay -1
    in the hp column and are counted; a pancake's level 12 / max 1940 on tick 5 is listed (the entity keeps its first
    read); the frames' generator states become `rng`. The same capture without them carries none of the keys."""
    unread = [(None, None, None), (1766, 1766, 11), (-1, -1, 11), (-1, -1, 11), (1700, 1766, 11),
              (1868, 1940, 12), (1868, 1940, 12), (1868, 1940, 12)]
    path = tmp_path / "a" / "frames-synthetic.native.oracle.jsonl.gz"
    _hp_capture(
        path, unread, rng=[5, 5, 9, 9, 9, 12, 12, 12], header_extra={"rng_algorithm": "native_xorshift32_v160402020"},
    )
    fx = _hp_build(m, path)
    knight = next(e for e in fx["truth"]["entities"] if e["key"] == 7)
    hp = []
    for v, run in zip(knight["hp"][::2], knight["hp"][1::2], strict=False):
        hp += [v] * run
    assert hp == [1766, -1, -1, 1700, 1868, 1868, 1868], "the hp column keeps the reader's -1 (null reads absent)"
    assert knight["hp_unread_frames"] == 2
    assert fx["truth"]["hp_unread"]["value"] == -1
    assert (knight["level"], knight["max_hp"]) == (11, 1766)
    assert knight["max_hp_changes"] == [[5, 12, 1940]]
    assert fx["rng"]["algorithm"] == "client_xorshift32_v160402020"
    assert (fx["rng"]["first"], fx["rng"]["changes"]) == ([0, 5], [[2, 9], [5, 12]])
    # the control: every hp read, no level change, no generator state
    plain = [(None, None, None)] + [(1766, 1766, 11)] * 7
    bare = tmp_path / "b" / "frames-synthetic.native.oracle.jsonl.gz"
    _hp_capture(bare, plain)
    fb = _hp_build(m, bare)
    assert "rng" not in fb
    assert "hp_unread" not in fb["truth"]
    assert not any("hp_unread_frames" in e or "max_hp_changes" in e for e in fb["truth"]["entities"])


GOBLINSTEIN = {"count": 1, "second_summon": {"character": "goblinstein_doctor", "count": 1},
               "summon_radius_milli": 3500, "summon_width_milli": -1000}


# ---------------------------------------------------------------------------------------------------------- item 9
def test_a_tie_is_broken_by_the_member_on_the_tap_point(m):
    """A TIE IS BROKEN BY THE TAP POINT (module doc, RECOVERED TILE): side 1's Goblinstein on a back-row tile, its
    Monster on its tile's laid point and its Doctor, 3500 behind, clamped to y 31000 -- which is 3500 behind the tile
    centre two rows down, so the two tiles tie with one member each (live 20261011-192922 t916: played on the centroid
    (11000, 29749) before, the engine's Monster 1250-1345 off). The Monster stands on the zero offset of its own tile,
    where no clamp moved it: that tile is taken."""
    tile, why = m.recovered_tile([(11500, 28499), (10500, 31000)], m.layout_offsets(GOBLINSTEIN, 1, 11500))
    assert tile == [11500, 28500], why
    assert why.startswith("1 of 2 members on a nominal offset from it, 1 clamped on one axis"), why
    assert "tied" in why, why
    # t2845 (the right half: the Doctor toward the centre is -x) and 20261007-161836 t1962 (one row further back)
    assert m.recovered_tile([(17500, 28499), (16500, 31000)], m.layout_offsets(GOBLINSTEIN, 1, 17500))[0] == [
        17500, 28500]
    assert m.recovered_tile([(13500, 29499), (12500, 31000)], m.layout_offsets(GOBLINSTEIN, 1, 13500))[0] == [
        13500, 29500]
    # a Monster pushed off its point stands on no tap point: the Doctor's tile alone is not enough (and no tie is left)
    tile, why = m.recovered_tile([(11530, 28499), (10500, 31000)], m.layout_offsets(GOBLINSTEIN, 1, 11530))
    assert tile is None, why
    # a ring has no member on its tap point, so its ties still stand (the Minions of the test above)
    tile, why = m.recovered_tile([(3999, 14212), (3500, 15000)], MINIONS_RING_SIDE0)
    assert tile is None, why
    assert why.startswith("2 tile centres tie"), why


# ---------------------------------------------------------------------------------------------------------- item 8
def test_a_ring_measured_on_another_table_is_scaled_to_this_ones_radius(m):
    """A RING MEASURED ON ANOTHER TABLE (module doc, RECOVERED TILE): the committed measurement ran the 2026-09 table,
    where the Skeletons' SummonRadius was 700 (ring 808); the 2026-10-06 update made it 400 (ring 462). Scaled, the ring
    places live 20261009-162704 t724's Skeletons (tapped on (14500, 17500), laid on (15500, 17500), the front one
    clamped to the river row's y 17499) on their tile; at the 808 ring no tile is found."""
    nominal = {("Skeletons", 1): SKELETONS_RING_SIDE1, ("Bats", 1): [(0, -1404)]}
    measured = {"Skeletons": {"summon_radius_milli": 700}, "Bats": {"summon_radius_milli": 750}}
    now = {"Skeletons": {"summon_radius_milli": 400}, "Bats": {"summon_radius_milli": 750}}
    got = m.scaled_nominal_offsets(nominal, now, measured)
    assert got[("Skeletons", 1)] == [(-400, 230), (-1, -462), (399, 230)], got
    assert got[("Bats", 1)] == [(0, -1404)], "an unchanged radius keeps its offsets"
    assert m.scaled_nominal_offsets(nominal, {}, measured) == nominal, "a radius unknown on this table: unscaled"
    c0 = [(15500, 17499), (15899, 17729), (15101, 17729)]
    tile, why = m.recovered_tile(c0, SKELETONS_RING_SIDE1)
    assert tile is None, why
    assert why.startswith("no member's creation point"), why
    tile, why = m.recovered_tile(c0, got[("Skeletons", 1)])
    assert tile == [15500, 17500], why
    assert why == "2 of 3 members on a nominal offset from it, 1 clamped on one axis", why


def test_the_committed_measurement_is_scaled_only_where_the_tables_differ(m):
    """The measurement's table and the 2026-10-06 table differ on one formation radius only (the Skeletons): every
    other measured card keeps its offsets, so a fixture of the 2026-09 corpus is made exactly as before."""
    import os

    def cards(name):
        path = os.path.join(m.ROOT, "data", "derived", m.TABLES[name][0])
        if not os.path.exists(path):
            pytest.skip(f"{path} is absent -- a skip here is not a pass")
        with open(path, encoding="utf-8") as fh:
            return {c["name"]: c for c in json.load(fh)["cards"]}

    nominal = m.load_nominal_offsets()
    before = m.scaled_nominal_offsets(nominal, cards(m.FORMATIONS_TABLE))
    assert before == nominal, "the measurement's own table scales nothing"
    after = m.scaled_nominal_offsets(nominal, cards("160402017-20261006"))
    assert {k[0] for k in nominal if after[k] != nominal[k]} == {"Skeletons"}, after


def _off_tap_battle():
    """(header towers, frames 0..40) of a battle side 0 logs, three Goblins groups (all deploying, no frame missed):
    keys 20.. first seen on 25 on the ring around (4500, 8500), a tile right of their tap's (3500, 8500); keys 30.. on
    30 on the ring around their tap's tile (14500, 8500); keys 40.. on 35 pushed off any ring around (9500, 13500),
    ten tiles from their tap's (9500, 3500)."""
    tower_rows = [(1, 0, 9000, 3000, 4824), (2, 0, 3500, 6500, 3052), (3, 0, 14500, 6500, 3052),
                  (4, 1, 9000, 29000, 4824), (5, 1, 3500, 25500, 3052), (6, 1, 14500, 25500, 3052)]
    header = [{"side": s, "x": x, "y": y} for _, s, x, y, _ in tower_rows]

    def ent(key, side, cid, x, y, hp, kind, state):
        return {"id": f"p{key}", "generation_key": key, "side": side, "x": x, "y": y, "x2": x, "y2": y,
                "card_id": cid, "level": 11, "kind": kind, "hp": hp, "max_hp": hp, "behavior_state": state,
                "target": None, "path_nodes": []}

    moved = [(4500 + ox, 8500 + oy) for ox, oy in GOBLINS_RING_SIDE0]
    on_tap = [(14500 + ox, 8500 + oy) for ox, oy in GOBLINS_RING_SIDE0]
    pushed = [(8850, 12850), (8850, 14150), (10150, 14150), (10150, 12850)]
    frames = []
    for t in range(41):
        es = [ent(k, s, -1, x, y, hp, 13, 0) for k, s, x, y, hp in tower_rows]
        for base, t0, pts in ((20, 25, moved), (30, 30, on_tap), (40, 35, pushed)):
            if t >= t0:
                es += [ent(base + i, 0, GOBLINS, x, y, 202, 14, 4) for i, (x, y) in enumerate(pts)]
        frames.append({"tick": t, "entities": es, "effects": [], "elixir_raw": [50000 + 178 * t, 50000 + 178 * t]})
    return header, frames


@needs_modern_cards
def test_a_tapped_group_records_the_tile_its_members_stand_on(m, maker_inputs, tmp_path):
    """THE MEMBERS' TILE OF A TAPPED GROUP and A TAP FAR OFF ITS GROUP (module doc, RECOVERED TILE), built end to end
    with a placements log: a group whose members agree on the tile right of their tap's records that tile as
    `members_tile` and stays on its tap (live 20261009-162704 t724's Skeletons, tapped on (14500, 17500), laid on
    (15500, 17500)); a group on its tap's tile records nothing; a group ten tiles off whose members recover no tile
    (seen pushed) records its centroid's tile (live 20261011-215918 t1415's Skeletons, tapped on (14500, 8500), laid on
    (14500, 14500)). The harness, not the maker, decides which one plays (harness.rs `members_point`)."""
    _skip_without_the_id_table(m)
    id_table, doc, name_to_id, card_names = maker_inputs
    header, frames = _off_tap_battle()
    path = tmp_path / ("frames-synthetic" + m.CAPTURE_SUFFIX)
    _write_capture(path, header, frames)
    log = tmp_path / "placements-synthetic.jsonl"
    log.write_text(
        json.dumps({"tick": 0, "local_side_native": 0}) + "\n"
        + json.dumps({"tick": 3, "card": "Goblins", "requested": [14.5, 8.5]}) + "\n"  # (3500, 8500)
        + json.dumps({"tick": 8, "card": "Goblins", "requested": [3.5, 8.5]}) + "\n"  # (14500, 8500)
        + json.dumps({"tick": 12, "card": "Goblins", "requested": [8.5, 3.5]}) + "\n",  # (9500, 3500)
        encoding="utf-8",
    )
    fx = m.build(str(path), [str(log)], 1, None, None, id_table, doc, {}, name_to_id, card_names, {})
    rows = {d["keys"][0]: d for d in fx["deploys"] if d["keys"]}
    moved, kept, far = rows[20], rows[30], rows[40]
    # the row stays on its tap (a relocation is the engine's to answer); the members' tile rides along for the harness
    assert (moved["pos"], moved["source"], moved["members_tile"]) == ([3500, 8500], "tap_tile", [4500, 8500]), moved
    assert (moved["tap"]["tick"], moved["tap"]["native"]) == (3, [3500, 8500]), moved
    assert moved["recovery"].endswith("not the tap's tile [3500, 8500]"), moved["recovery"]
    assert (kept["pos"], kept["source"]) == ([14500, 8500], "tap_tile"), kept
    assert "members_tile" not in kept, kept
    assert "recovery" not in kept, kept
    assert (far["pos"], far["source"], far["members_tile"]) == ([9500, 3500], "tap_tile", [9500, 13500]), far
    assert far["recovery"].endswith("its own tile"), far["recovery"]


# ---------------------------------------------------------------------------------------------------------- item 10
def test_a_unit_first_seen_out_of_its_deploy_after_frames_that_did_not_show_it(m):
    """A UNIT FIRST SEEN OUT OF ITS DEPLOY (module doc), the frames before it stale: live 20261008-055422's frames 232
    to 254 (every 2-3 ticks) repeat one board, and side 1's Skeletons, tapped on 234, are first seen on 256 walking
    (state 1, their first step from their creation points). The gap is two ticks, shorter than their 20-tick deploy, so
    the old rule took its earliest tick, 255; their tap says the frames did not show them, and they end their deploy on
    256: spawned on 236 (side 1's elixir on 256, 93738 and rising 178 a tick from a capped 10, left 20-21 ticks
    before)."""
    ticks = [149, 232, 234, 237, 239, 242, 244, 247, 249, 252, 254, 256, 257, 258]
    fi = ticks.index(256)
    walking = [(fi, 1), (fi + 1, 1), (fi + 2, 1)]
    tick, first_seen, why = m.refine_spawn_tick(ticks, fi, walking, 1000, tap_tick=234)
    assert (tick, first_seen) == (236, 256), why
    assert "after frames that did not show it (tapped on 234)" in why, why
    # no tap, or one too late for a whole deploy before the first frame: the gap's earliest tick, as before
    assert m.refine_spawn_tick(ticks, fi, walking, 1000)[:2] == (255, 256)
    assert m.refine_spawn_tick(ticks, fi, walking, 1000, tap_tick=237)[:2] == (255, 256)
    # first seen deploying (or waiting its stagger) is no unit out of its deploy, tap or none
    assert m.refine_spawn_tick(ticks, fi, [(fi, 4), (fi + 1, 4)], 1000, tap_tick=234)[0] == 255
    assert m.refine_spawn_tick(ticks, fi, [(fi, 11), (fi + 1, 11)], 1000, tap_tick=234)[0] == 255
    # a gap a deploy long needs no tap (the rule as it was)
    assert m.refine_spawn_tick([799, 872, 873], 1, [(1, 1), (2, 1)], 1000, tap_tick=None)[:2] == (852, 872)


def test_an_area_first_seen_after_a_frame_gap_is_dated_by_its_strike(m):
    """AN AREA FIRST SEEN AFTER A FRAME GAP: live 20261007-205045 t2889 (frame 2888 missed): the Royal Delivery's
    crate is first seen on 2928, after a present 2927, so the cast was 2928 - 40 = 2888. Live 20261008-165815 t1841
    (1840 missed) shows no crate; its recruit is first seen on 1881 after a missed 1880, so the cast was 1839 or 1840,
    and the area's own window (1840, 1841] leaves 1840. Witnesses that leave two ticks pin nothing, and an area that
    strikes three times (a Lightning's) is left alone."""
    ticks = list(range(2880, 2888)) + list(range(2889, 2935))
    cast = {"first_index": ticks.index(2889), "tracks": [
        {"first": 2889, "last_seen": 2927, "arrival": 2928},
        {"first": 2928, "last_seen": 2928, "arrival": 2930},
    ]}
    tick, why = m.area_strike_tick(cast, {}, ticks, 1, 28000018, [40])
    assert tick == 2888, why
    assert "strike object first seen on 2928" in why
    assert m.area_strike_tick(cast, {}, ticks, 1, 28000018, [10, 20, 30]) is None, "a Lightning strikes three times"
    # two missed frames before the area (2887, 2888) and the crate after a missed 2928: [2888, 2889] twice, no pin
    loose = [t for t in ticks if t not in (2887, 2928)]
    loose_cast = {"first_index": loose.index(2889), "tracks": [{"first": 2889}, {"first": 2929}]}
    assert m.area_strike_tick(loose_cast, {}, loose, 1, 28000018, [40]) is None
    # no crate: the recruit (the card's unit), first seen after a missed frame, still pins it with the area's window
    ticks2 = list(range(1830, 1840)) + list(range(1841, 1880)) + list(range(1881, 1900))
    cast2 = {"first_index": ticks2.index(1841), "tracks": [{"first": 1841, "last_seen": 1879, "arrival": 1881}]}
    ents = {
        50: {"key": 50, "side": 1, "card_id": 28000018, "first_index": ticks2.index(1881)},
        51: {"key": 51, "side": 0, "card_id": 28000018, "first_index": ticks2.index(1870)},
    }
    tick, why = m.area_strike_tick(cast2, ents, ticks2, 1, 28000018, [40])
    assert tick == 1840, why
    assert "unit first seen on 1881 (after 1879)" in why
