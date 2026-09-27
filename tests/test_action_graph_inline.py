"""The inline walk of an area's action graph: tools/extract_cards.py `action_graph` with WALK_INLINE.

WHAT THIS PINS. An area row's *Action column may be an inline table rather than an action's name (the Void's
DarkMagicAOE: its OnStartingAction is an ActionGroup whose SubActions are inline tables, one of them an ActionLaserBall
that spawns three inline buffs), and a named action may hold inline sub-actions (the Goblin Curse circle's
GoblinCurseCreateBuffs). The graph the loader refuses a mechanic by has to see both, or a row whose mechanic lives in
inline tables reads as having none:
  1. the Void's area: its two inline roots are named after the row, the walk reaches the laser ball and its three
     inline buffs, and the graph is a mechanic (before the walk it was null, and the Void was refused for its hit flags
     alone);
  2. the Goblin Curse circle: its named group's two inline spawns are seen, and the graph is still exactly the circle's
     readable hit (card.rs `refuse_action_mechanic_but_on_hit` accepts a graph of ActionGroup and ActionSpawn whose
     spawns are its on_hit buffs), so the Goblin Curse keeps loading;
  3. the walk changes exactly four area rows, and flips the mechanic flag of none that a loaded card reaches but
     those two;
  4. a UNIT row is not walked: the Berserker's inline OnStartingAction stays out of its graph, so it keeps loading.

The tables come from data/raw/cr-15.535.29, the decoded asset pack, which a clone does not have: every test here
skips LOUDLY without it, naming the directory, and a skip is not a pass.

PLANT `inline_roots_skipped`: WALK_INLINE off (monkeypatched) takes the Void's graph back to null. Test 1 runs its own
check under the plant too, and asserts that it fails there, so the check is one that can fail.
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
            "nothing about the inline walk has been checked here."
        )
    return ec.load_tables("15.535.29")


def _void_graph_ok(t) -> str | None:
    """None when the Void's area graph is the walked one, else what is wrong."""
    g = ec.norm_aeo(t, "DarkMagicAOE")["action_graph"]
    if g is None:
        return "the Void's area has no graph"
    roots = {
        "OnLifeTimeEndAction": "DarkMagicAOE.OnLifeTimeEndAction",
        "OnStartingAction": "DarkMagicAOE.OnStartingAction",
    }
    if g["roots"] != roots:
        return f"roots {g['roots']}"
    if g["class_types"] != ["ActionGroup", "ActionLaserBall", "ActionPlayEffect", "ActionSpawn"]:
        return f"classes {g['class_types']}"
    want = ["BuffType:DarkMagicAOE_Damage_lv3", "BuffType:DarkMagicAOE_Damage_lv2", "BuffType:DarkMagicAOE_Damage_lv1"]
    if g["spawns"] != want:
        return f"spawns {g['spawns']}"
    if g["mechanic"] is not True:
        return "not a mechanic"
    return None


def test_the_void_area_graph_reaches_its_laser_ball_and_its_inline_buffs(tables, monkeypatch):
    assert _void_graph_ok(tables) is None, _void_graph_ok(tables)
    # The plant: the walk off, and the same check fails.
    monkeypatch.setattr(ec, "WALK_INLINE", False)
    assert _void_graph_ok(tables) is not None, "the check passes with the inline walk off: it cannot fail"


def test_the_curse_circle_graph_is_its_readable_hit(tables):
    rec = ec.norm_aeo(tables, "GoblinCurseBase")
    g = rec["action_graph"]
    assert g["roots"] == {"OnHitAction": "GoblinCurseCreateBuffs"}
    assert g["class_types"] == ["ActionGroup", "ActionSpawn"], g
    assert g["spawns"] == ["BuffType:GoblinCurse", "BuffType:GoblinCurseDamage"], g
    assert g["mechanic"] is True
    # What card.rs `refuse_action_mechanic_but_on_hit` accepts: only group and spawn classes, every spawn one of the
    # hit's BuffType entries, and the one root OnHitAction.
    hit = {f"BuffType:{e['spawn']}" for e in rec["on_hit"]["entries"] if e.get("spawn_type") == "BuffType"}
    assert set(g["spawns"]) <= hit, (g["spawns"], hit)


def test_the_walk_changes_exactly_four_area_rows(tables, monkeypatch):
    names = sorted(tables["area_effect_objects"].records)
    on = {n: ec.norm_aeo(tables, n)["action_graph"] for n in names}
    monkeypatch.setattr(ec, "WALK_INLINE", False)
    off = {n: ec.norm_aeo(tables, n)["action_graph"] for n in names}
    moved = sorted(n for n in names if on[n] != off[n])
    assert moved == ["DarkMagicAOE", "GoblinCurseBase", "Tesla_EV1_DummyAEO", "dead_goblinstein"], moved
    flips = {n: ((off[n] or {}).get("mechanic"), (on[n] or {}).get("mechanic")) for n in moved}
    assert flips == {
        "DarkMagicAOE": (None, True),
        "GoblinCurseBase": (False, True),
        "Tesla_EV1_DummyAEO": (True, True),
        "dead_goblinstein": (None, False),
    }, flips


def test_a_unit_row_is_not_walked(tables):
    u = ec.norm_unit(tables, "Berserker")
    assert u["action_graph"] is None, f"the Berserker's graph moved: {u['action_graph']}"
