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
    recovered_tile), its `centroid` kept as it was; a group whose members agree on none stays at its centroid, and
    `recovery` says why. Before the recovery the left-edge group was played at its centroid, (755, 10500), and the
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
    assert (walking["pos"], walking["source"]) == (walking["centroid"], "centroid"), walking
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
    """(header towers, frames) of a battle with three single Knights, frames 0..30 with 11 missed.

    Key 50, side 0: created on 5 at (3499, 14500) and pushed 150 on that tick, so its first frame stands at
    (3589, 14380) (client 16.402, 20260918-112751 t1202). Key 51, side 0: first seen on 12, after the missed 11, at
    (9394, 5394), its x2, y2 at (9500, 5500): it may have been created on 11. Key 52, side 1: on the board from the
    first frame, walking."""
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
        ents.append(ent(52, 1, KNIGHT, (6000, 20000 - 10 * t), (6000, 20010 - 10 * t), 1766, 15, 1))
        frames.append({"tick": t, "entities": ents, "effects": [], "elixir_raw": [50000 + 178 * t, 50000 + 178 * t]})
    return header, frames


@needs_modern_cards
def test_a_single_unit_pushed_on_its_creation_tick_is_played_where_it_was_created(m, maker_inputs, tmp_path):
    """A single unit's first frame carries its first tick's contact push; its x2, y2 is where it was created. It is
    played there when the capture saw its creation tick, and keeps its first frame when a missed frame leaves that tick
    unseen or it was not deploying."""
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
    assert (unseen["pos"], unseen["source"]) == ([9394, 5394], "centroid"), unseen
    assert (walking["pos"], walking["source"]) == (walking["centroid"], "centroid"), walking


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
