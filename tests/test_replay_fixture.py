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
            "ROYALELIVE_REPORTS is not set, so the committed sample was not rebuilt from its"
            " capture and may be stale -- a skip here is not a pass"
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
                "elixir_raw": [60000 + 178 * t - (30000 if t >= 1 else 0), 70000 + 178 * t],
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
    id_table = m.load_id_table()
    card_names = {c["name"] for c in doc["cards"]}
    name_to_id = {n: c for c, n in sorted(id_table.items(), reverse=True) if n in card_names}
    return id_table, doc, name_to_id, card_names


def _build(m, maker_inputs, path):
    id_table, doc, name_to_id, card_names = maker_inputs
    return m.build(str(path), [], 1, None, None, id_table, doc, {}, name_to_id, card_names, {})


def _decode(col):
    out = []
    for v, run in zip(col[::2], col[1::2], strict=True):
        out.extend([v] * run)
    return out


@needs_cards
def test_the_battle_publishes_timers_elixir_and_spell_objects(m, maker_inputs, tmp_path):
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


@needs_cards
def test_one_battle_is_one_fixture_whichever_way_up_it_was_recorded(m, maker_inputs, tmp_path):
    """The rotated branch of build(), end to end: the same battle recorded with side 0 at the
    top must give the same fixture as recorded with side 0 at the bottom -- every position,
    side, path cell, spell point, aim and elixir column -- except the `frame` note that says
    which way it came in and the header's native `local_side_native`."""
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
