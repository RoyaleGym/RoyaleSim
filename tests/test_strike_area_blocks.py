"""The striking-area and Clone blocks: tools/extract_cards.py `strike_area_block` (the Vines' ranked catches, the
Void's laser ball), `clone_action_block`, the CLONE_* globals and a unit's IgnoreClone.

WHAT THIS PINS. The loader (card.rs `strike_area_shape`, `clone_shape`) runs these cards from the blocks alone, so each
block must say exactly what the tables say, and must be ABSENT for any action shape its builder has not read key by key:
  1. the Vines' block: the start delay 900, the catch offsets 0, 50 and 150, once per target, ranked by current hp plus
     shield, the 2500 circle, the filter's flags, the air-to-ground durations, and the seven size options, which are
     one buff by value under seven names;
  2. the Void's block: the start delay 500, FirstHitDelay 1000, HitFrequency 1200, DetectionRadius 2500, the count
     limits [1, 4] and the three inline tier buffs in LIST order (lv3, lv2, lv1), one pulse each;
  3. a key the builder has never read, on any action of either shape or on the filter, gives no block;
  4. the Clone's area and its action, the GlobalClone event's (whose own Buff keeps it refused), and the eleven CLONE_*
     globals with the 15.535.29 values;
  5. IgnoreClone on the unit rows that set it, and nowhere else.

The tables come from data/raw/cr-15.535.29, the decoded asset pack, which a clone does not have: every test here
skips LOUDLY without it, naming the directory, and a skip is not a pass.

PLANT. Test 3 is its own plant: it adds an unread key to each action in turn and asserts the block disappears, then
takes the key off and asserts the block is back.
"""

from __future__ import annotations

import os
import sys

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RAW = os.path.join(ROOT, "data", "raw", "cr-15.535.29")
sys.path.insert(0, os.path.join(ROOT, "tools"))
import extract_cards as ec  # noqa: E402


@pytest.fixture(scope="module")
def tables():
    if not os.path.isdir(os.path.join(RAW, "csv_logic")):
        pytest.skip(
            f"SKIPPED, NOT PASSED: {RAW} is absent. It is the decoded 15.535.29 asset pack, which a clone never has; "
            "nothing about the striking-area blocks has been checked here."
        )
    return ec.load_tables("15.535.29")


def _block(t, area):
    return ec.norm_aeo(t, area).get("strike_area")


MECHANIC_BUFF_KEYS = (
    "speed_multiplier_raw",
    "hit_speed_multiplier_raw",
    "spawn_speed_multiplier_raw",
    "damage_per_second",
    "hit_frequency_ms",
    "heal_per_second",
    "crown_tower_damage_per_hit",
    "enable_stacking",
)

VINES_OPTIONS = [
    "Vines_Trap_Snare_XXLarge",
    "Vines_Trap_Snare_XLarge",
    "Vines_Trap_Snare_Large",
    "Vines_Trap_Snare_Medium",
    "Vines_Trap_Snare_Small",
    "Vines_Trap_Snare_Medium",
    "Vines_Trap_Snare_Large",
]


def test_the_vines_block_is_the_tables(tables):
    b = _block(tables, "Vines_AeO")
    assert b is not None, "the Vines' area has no striking-area block"
    assert (b["kind"], b["start_delay_ms"], b["catch_offsets_ms"]) == ("ranked_catches", 900, [0, 50, 150])
    mode = ("HighestCurrentHpIncludeShields", 2500)
    assert b["once_per_target"] is True
    assert (b["selection_mode"], b["radius_milli"]) == mode
    f = b["filter"]
    assert f["name"] == "enemy_troops_for_vines"
    assert (f["match_team_enemy"], f["match_type_characters"]) == (True, True)
    assert (f["filter_buildings"], f["filter_summoner"], f["filter_princess_towers"]) == (False, False, False)
    assert (f["filter_underground"], f["filter_dash_immune"], f["filter_cloning"]) == (True, True, False)
    assert "filter_hidden" not in f, "the Vines' filter leaves FilterHidden out"
    assert "filter_invisible" not in f, "the Vines' filter leaves FilterInvisible out"
    assert f["tags"] == ["NO_CHECKAVOIDANCE", "NO_CHECKCOLLISIONS", "UNTARGETABLE"]
    ag = b["air_to_ground"]
    got = (ag["transition_ms"], ag["total_ms"], ag["abort_if_instigator_dies"], ag["singleton"])
    assert got == (50, 2000, False, True)
    assert ag["allow_is_ground_tag_on_idle"] is True
    names = [o["name"] for o in b["options"]]
    assert names == VINES_OPTIONS, names
    assert b["option_time_ms"] == [2000] * 7
    first = {k: b["options"][0][k] for k in MECHANIC_BUFF_KEYS}
    assert first == {
        "speed_multiplier_raw": -100,
        "hit_speed_multiplier_raw": -100,
        "spawn_speed_multiplier_raw": -100,
        "damage_per_second": 60,
        "hit_frequency_ms": 1000,
        "heal_per_second": None,
        "crown_tower_damage_per_hit": 14,
        "enable_stacking": True,
    }, first
    for o in b["options"]:
        same = {k: o[k] for k in MECHANIC_BUFF_KEYS} == first
        assert same, f"{o['name']} differs from the first option in a mechanic column"
    assert len(b["conditions"]) == 6


def test_the_void_block_is_the_tables_in_list_order(tables):
    b = _block(tables, "DarkMagicAOE")
    assert b is not None, "the Void's area has no striking-area block"
    got = (b["kind"], b["start_delay_ms"], b["first_hit_delay_ms"], b["hit_frequency_ms"])
    assert got == ("laser_ball", 500, 1000, 1200), got
    assert (b["detection_radius_milli"], b["max_units_per_list"]) == (2500, [1, 4])
    f = b["filter"]
    assert f["name"] == "ForcedCharacterTargets"
    assert (f["match_team_enemy"], f["match_type_characters"], f["filter_hidden"]) == (True, True, True)
    assert "filter_buildings" not in f
    assert "filter_underground" not in f
    tiers = [
        (
            t["buff"]["name"],
            t["buff"]["damage_per_second"],
            t["buff"]["hit_frequency_ms"],
            t["buff"]["crown_tower_damage_per_hit"],
            t["time_ms"],
            t["add_as_individual_buff"],
        )
        for t in b["tiers"]
    ]
    assert tiers == [
        ("DarkMagicAOE_Damage_lv3", 2720, 100, 38, 100, True),
        ("DarkMagicAOE_Damage_lv2", 1150, 100, 20, 100, True),
        ("DarkMagicAOE_Damage_lv1", 600, 100, 14, 100, True),
    ], tiers
    for t in b["tiers"]:
        assert t["buff"]["speed_multiplier_raw"] is None, t["buff"]
        assert t["buff"]["hit_speed_multiplier_raw"] is None, t["buff"]


VINES_ACTIONS = [
    "Vines_Start_Action_Group",
    "Vines_Target_Selector",
    "Vines_Action_Group",
    "Vines_Air_To_Ground",
    "Vines_Select_Buff_Size",
    "Vines_Apply_Snare_Small",
]


def test_an_unread_key_gives_no_block(tables):
    acts = tables["actions"]
    # The Vines: each named action of the shape in turn.
    for name in VINES_ACTIONS:
        rec = acts.records[name]
        rec["WaitForTarget"] = True
        try:
            assert _block(tables, "Vines_AeO") is None, f"an unread key on {name} still gives a block"
        finally:
            del rec["WaitForTarget"]
    assert _block(tables, "Vines_AeO") is not None, "the Vines' block did not come back"
    # The Void: its inline laser ball and one inline tier buff.
    root = tables["area_effect_objects"].records["DarkMagicAOE"]["OnStartingAction"]
    lb = next(s for s in root["SubActions"] if s.get("ClassType") == "ActionLaserBall")
    for target in [lb, lb["OnDetectedUnitActionList"][0], lb["OnDetectedUnitActionList"][1]["SpawnData"]]:
        what = target.get("ClassType") or target.get("Name")
        target["WaitForTarget"] = True
        try:
            assert _block(tables, "DarkMagicAOE") is None, f"an unread key on {what} still gives a block"
        finally:
            del target["WaitForTarget"]
    assert _block(tables, "DarkMagicAOE") is not None, "the Void's block did not come back"
    # A filter key the reader does not know.
    tables.filters["ForcedCharacterTargets"]["FilterJumping"] = True
    try:
        assert ec.filter_block(tables, "ForcedCharacterTargets") is None
        assert _block(tables, "DarkMagicAOE") is None
    finally:
        del tables.filters["ForcedCharacterTargets"]["FilterJumping"]
    assert ec.filter_block(tables, "ForcedCharacterTargets") is not None


def test_the_clone_block_and_its_globals(tables):
    a = ec.norm_aeo(tables, "Clone")
    assert a["clone"] is True
    ca = a["clone_action"]
    assert ca is not None, "the Clone's area has no clone_action"
    on = ca["on_cloned"]
    assert (on["spawn_type"], on["spawn"], on["spawn_time_ms"]) == ("BuffType", "Clone", 500)
    assert on["buff"]["clone"] is True
    buff = on["buff"]
    hold = (buff["speed_multiplier_raw"], buff["hit_speed_multiplier_raw"], buff["spawn_speed_multiplier_raw"])
    assert hold == (-100, -100, -100)
    assert "strike_area" not in a
    g = ec.norm_aeo(tables, "GlobalClone")
    assert g["clone"] is True
    assert g["clone_action"] is not None
    assert g["buff"]["name"] == "Clone", "the event's area hangs Buff Clone itself"
    got = ec.globals_block(tables.vintage)
    assert {k: got[k] for k in got if k.startswith("CLONE_")} == {
        "CLONE_LEVEL_OFFSET": 0,
        "CLONE_DISTANCE_X": 0,
        "CLONE_DISTANCE_Y": 250,
        "CLONE_PRESERVE_SHIELD": True,
        "CLONE_CLONED_UNITS": False,
        "CLONE_MOVE_PARENT": True,
        "CLONE_DEATH_SPAWN_UNITS": True,
        "CLONE_DEATH_SPAWN_BUILDINGS": True,
        "CLONE_RESET_TARGET": False,
        "CLONE_RESET_CHARGE": False,
        "CLONE_INHERIT_CHARGE": False,
    }


def test_ignore_clone_is_written_where_set_and_nowhere_else(tables):
    for name in ["GoblinDrillDig", "Recruit_Chess"]:
        assert ec.norm_unit(tables, name).get("ignore_clone") is True, name
    for name in ["Knight", "Giant", "Cannon"]:
        assert "ignore_clone" not in ec.norm_unit(tables, name), name
