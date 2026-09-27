"""The rows the scheduled areas and the Lumberjack's bottle load from: tools/extract_cards.py `action_schedule` and
`schedule_entry`, read against the tables they came from.

WHAT THIS PINS. The loader runs three areas by their action schedules (card.rs `scheduled_area`, `area_spawns_bottle`):
  1. the Graveyard's area: twelve ActionSpawnToLocation entries, each one Graveyard_rework_Skeleton with UseDeploy
     and DeployTime 500 (inherited from its Base action), each x and y offset the number its two position expressions
     carry, read here from graveyard_rework.toml by a regex of this file's own, and nothing unread;
  2. the Suspicious Bush's death area: BushGoblin at 675 ms with RelativeX -1 and at 625 ms with RelativeX +1,
     UseDeploy, no DeployTime, as suspicious_bush.toml writes them;
  3. the Lumberjack's death area: one ActionSpawn of RageBarbarianBottle at delay 0, with no position;
  4. an expression of another form is left unread (the loader then refuses the area), never read as a number;
  5. the 2018 build carries no schedule (tests/test_action_schedule.py pins the byte-identical file).

The tables come from data/raw/cr-15.535.29, the decoded asset pack, which a clone does not have: every test here
skips LOUDLY without it, naming the directory, and a skip is not a pass.

PLANT. Test 4 checks itself: an expression of the Graveyard's form is read, and the same expression with its sign
function renamed is left unread, so a reader that took any number out of any expression fails it.
"""

from __future__ import annotations

import os
import re
import sys
import tomllib

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RAW = os.path.join(ROOT, "data", "raw", "cr-15.535.29")
sys.path.insert(0, os.path.join(ROOT, "tools"))
import extract_cards as ec  # noqa: E402

#: this file's own reading of the two expressions, independent of the extractor's
X_NUMBER = re.compile(r"x \+ \((-?\d+) \* select")
Y_NUMBER = re.compile(r"y - \((-?\d+) \* team_y_direction")


@pytest.fixture(scope="module")
def tables():
    if not os.path.isdir(os.path.join(RAW, "csv_logic")):
        pytest.skip(
            f"SKIPPED, NOT PASSED: {RAW} is absent. It is the decoded 15.535.29 asset pack, which a clone never has; "
            "nothing about the scheduled areas' rows has been checked here."
        )
    return ec.load_tables("15.535.29")


def _toml(name):
    with open(os.path.join(RAW, "csv_logic", "characters", name), "rb") as fh:
        return tomllib.load(fh)


def _live(t, aeo):
    rec = ec.norm_aeo(t, aeo)
    assert rec is not None, f"no area_effect_objects row {aeo}"
    sched = rec["schedule"]
    assert sched is not None, f"{aeo} carries no schedule"
    return [e for e in sched["entries"] if not e.get("cosmetic")]


def test_the_graveyard_entries_carry_their_expressions_offsets_and_deploy(tables):
    live = _live(tables, "Graveyard_rework")
    assert len(live) == 12, f"twelve entries, the repeats kept: {len(live)}"
    doc = _toml("graveyard_rework.toml")
    group = doc["ACTION"]["Graveyard_rework_Group"]
    assert [e["action"] for e in live] == group["SubActions"]
    for e in live:
        row = doc["EXT"][e["action"]]
        want_x = int(X_NUMBER.search(row["XPositionExpression"]).group(1))
        want_y = int(Y_NUMBER.search(row["YPositionExpression"]).group(1))
        got = (e["class"], e["spawn_type"], e["spawn"], e["use_deploy"], e["deploy_time_ms"])
        want = ("ActionSpawnToLocation", "CharacterType", "Graveyard_rework_Skeleton", True, 500)
        assert got == want, (e["action"], got)
        assert e["x"] == {"form": "nearer_wall_mirror", "offset_milli": want_x}, e
        assert e["y"] == {"form": "team_y_direction", "offset_milli": want_y}, e
        assert "relative" not in e, e
        assert "unread" not in e, e
    base = doc["ACTION"]["Graveyard_rework_Spawn_Skeleton_Base"]
    assert (base["DeployTime"], base["UseDeploy"]) == (500, True), "the Base the entries inherit from"


def test_the_bushs_two_goblins_are_relative_and_one_tick_apart(tables):
    live = _live(tables, "SuspiciousBush_DummyAEO")
    got = [
        (e["delay_ms"], e["class"], e["spawn"], e["use_deploy"], e["deploy_time_ms"], e.get("relative")) for e in live
    ]
    assert got == [
        (675, "ActionSpawnToLocation", "BushGoblin", True, None, {"x": -1, "y": 0}),
        (625, "ActionSpawnToLocation", "BushGoblin", True, None, {"x": 1, "y": 0}),
    ], got
    for e in live:
        for key in ("x", "y", "unread"):
            assert key not in e, e
    doc = _toml("suspicious_bush.toml")
    group = doc["ACTION"]["SuspiciousBush_SpawnBushGoblin"]
    assert group["SubActionsDelay"] == [675, 625]
    assert [doc["ACTION"][a]["RelativeX"] for a in group["SubActions"]] == [-1, 1]


def test_the_lumberjacks_death_area_puts_down_one_bottle(tables):
    live = _live(tables, "RageBarbarianDummyForSpawn")
    assert len(live) == 1, live
    [e] = live
    got = (e["delay_ms"], e["class"], e["spawn_type"], e["spawn"])
    assert got == (0, "ActionSpawn", "CharacterType", "RageBarbarianBottle"), e
    assert all(k not in e for k in ("x", "y", "relative", "unread")), e
    bottle = ec.norm_unit(tables, "RageBarbarianBottle")
    got = (bottle["deploy_time_ms"], bottle["death_area_effect"], bottle["hitpoints"])
    assert got == (500, "BarbarianRage", None), bottle


def test_an_expression_of_another_form_is_left_unread(tables):
    base = {
        "ClassType": "ActionSpawnToLocation",
        "SpawnType": "CharacterType",
        "SpawnData": "Graveyard_rework_Skeleton",
        "UseDeploy": True,
    }
    good = "x + (-2500 * select(x > (map_width / 2), -1, 1))"
    read = ec.schedule_entry(tables, dict(base, XPositionExpression=good), "A", 0)
    assert read.get("x") == {"form": "nearer_wall_mirror", "offset_milli": -2500}, read
    assert "unread" not in read, read
    renamed = good.replace("select", "choose")
    other = ec.schedule_entry(tables, dict(base, XPositionExpression=renamed), "B", 0)
    assert "x" not in other, other
    assert any("XPositionExpression" in u for u in other.get("unread", [])), other


def test_the_2018_build_carries_no_schedule():
    path = os.path.join(ROOT, "data", "derived", "cards-2018.json")
    if not os.path.exists(path):
        pytest.skip(f"SKIPPED, NOT PASSED: {path} is absent; tools/extract_cards.py --vintage 2018 writes it")
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    for key in ('"schedule"', '"spawn_max_angle_deg"'):
        assert key not in text, f"the 2018 file carries {key}: it must stay byte-identical"
