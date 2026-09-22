"""tools/make_replay_fixture.py: the pure pieces (spawn-tick recovery, the RLE truth
encoding, the deploy-versus-spawn classification, the spell-cast grouping, the
cards.json hash) on hand-built inputs, and the committed sample's script against what
its capture is documented to hold."""

from __future__ import annotations

import importlib.util
import json
import os

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
    # transition in (190, 192] -> spawn in [172, 173]; frame gap -> [171, 173]
    assert (tick, first_seen) == (173, 173)
    assert why.startswith("range [172, 173]")


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
    # a formation member (no positions passed) keeps the latest of the range
    tick, _, why = m.refine_spawn_tick(ticks, fi0, states, 1000)
    assert tick == 937
    assert why.startswith("range [936, 937]")
    # the first step alone: gap-free before the first moved frame
    assert m.first_step_spawn_tick([100, 101, 102], [(0, 0, 0), (1, 0, 0), (2, 0, 59)], 20) == 82
    # a walk that does not fit whole steps of its own speed says nothing
    positions[-3] = (ticks.index(956), 3490, 8530)
    assert m.first_step_spawn_tick(ticks, positions, 20) is None
    # a step count outside the gap (three steps across a two-tick gap) says nothing
    positions[-3] = (ticks.index(956), 3437, 8668)
    assert m.first_step_spawn_tick(ticks, positions, 20) is None


def test_spawn_tick_falls_back_to_first_seen_without_a_transition(m):
    ticks = [170, 173, 174]
    tick, first_seen, why = m.refine_spawn_tick(ticks, 1, [(1, 4), (2, 4)], 1000)
    assert (tick, first_seen) == (173, 173)
    assert "no transition" in why
    # a summon-delay card starts in state 11, not 4: no transition read either
    tick, _, why = m.refine_spawn_tick(ticks, 1, [(1, 11), (2, 4)], 1000)
    assert tick == 173
    assert "no transition" in why


@needs_cards
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
    # beyond it the entity is an unknown object: truth only, never a deploy. The Goblin
    # Drill's Goblins (202 hp at level 11) against its only object, the 2560-hp dig
    # troop, were 22 building deploys before this rule
    drill = cards["GoblinDrill"]
    unit, is_own, how = m.classify_unit(doc, drill, 11, 202)
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


def test_fnv1a64_matches_the_harness_known_answers(m):
    # the same known answers tests/replay_parity.rs pins for harness.rs fnv1a64
    assert m.fnv1a64(b"") == "cbf29ce484222325"
    assert m.fnv1a64(b"a") == "af63dc4c8601ec8c"


@needs_cards
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
