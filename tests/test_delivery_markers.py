"""THE ABILITY'S DELIVERY MARKER, TAKEN OUT OF THE GRAPH: tools/extract_cards.py `strip_delivery_markers` (16.635 on).

A button of RefundUntilDelivered is refunded until its graph reaches an ActionAbilityDelivered, a step that does
nothing else. The strip takes every marker out so the readers meet the 16.402 graph. Pinned on synthetic tables (no
client pack needed):
  1. a marker in a group's SubActions (at delay 0) is dropped with its delay; a group left with one step keeps it as
     its scalar value;
  2. a NextAction to a marker that leads nowhere is removed (the row and its set fields), and one to a marker that
     leads on takes the marker's NextAction, its wait matching;
  3. an ActionRunOnInstigator that only runs a marker goes, its pointer with it, where a group reaching it makes the
     marker's onward variable write itself at delay 0;
  4. each shape it cannot drop without losing something stops the build: a marker that sets more, sits at a delay
     or leads on from a group, a wait that differs, a run whose onward write no reaching group makes, a marker still
     named by another table's row or by a button;
  5. a graph with no marker is left as it is.
"""

from __future__ import annotations

import copy
import os
import sys
from pathlib import Path

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import extract_cards as ec  # noqa: E402

SRC = Path("synthetic.toml")


def tables(actions: dict, **other: dict) -> ec.Tables:
    """A Tables of the 16.635 vintage holding `actions` (and any other table key -> rows) as overlay tables."""
    t = ec.Tables(ec.VINTAGES["160635007-20261103"])
    for key, rows in {"actions": actions, **other}.items():
        tb = ec.OverlayTable(key, None)
        tb.schemaless = True
        tb.overlay(SRC, copy.deepcopy(rows), key)
        t[key] = tb
    return t


MARK = {"ClassType": "ActionAbilityDelivered"}


def test_a_marker_step_is_dropped_from_its_group():
    t = tables({
        "G": {"ClassType": "ActionGroup", "SubActions": ["M", "A", "B"], "SubActionsDelay": [0, 0, 0]},
        "H": {"ClassType": "ActionGroup", "SubActions": ["M", "A"], "SubActionsDelay": [0, 0]},
        "M": MARK, "A": {"ClassType": "ActionPlayEffect"}, "B": {"ClassType": "ActionPlayEffect"},
    })
    ec.strip_delivery_markers(t)
    acts = t["actions"]
    assert "M" not in acts.records
    assert "M" not in acts.set_fields
    assert acts.arrays["G"] == {"SubActions": ["A", "B"], "SubActionsDelay": [0, 0]}
    assert (acts.records["H"]["SubActions"], acts.records["H"]["SubActionsDelay"]) == ("A", 0)
    assert "SubActions" not in acts.arrays["H"], "one step left: the scalar alone, as the overlay writes one"


def test_a_next_action_to_a_marker_is_removed_or_passed_on():
    t = tables({
        "S": {"ClassType": "ActionSpawn", "NextAction": "M"},
        "W": {"ClassType": "ActionSetVariable", "NextAction": "N", "NextActionWait": True},
        "M": MARK,
        "N": {"ClassType": "ActionAbilityDelivered", "NextAction": "R", "NextActionWait": True},
        "R": {"ClassType": "ActionSetVariable"},
    })
    ec.strip_delivery_markers(t)
    acts = t["actions"]
    assert "NextAction" not in acts.records["S"]
    assert "NextAction" not in acts.set_fields["S"]
    assert (acts.records["W"]["NextAction"], acts.records["W"]["NextActionWait"]) == ("R", True)
    assert not {"M", "N"} & set(acts.records)


def death_path(group_writes: bool) -> dict:
    """The Hero Ice Wizard's shape: a death group whose find spawns a cube, the spawn running the marker on the
    instigator, the marker leading on to the in-flight reset the group may or may not make itself."""
    return {
        "Death": {"ClassType": "ActionGroup", "SubActions": ["Find", "Reset"] if group_writes else ["Find", "Other"],
                  "SubActionsDelay": [0, 0]},
        "Find": {"ClassType": "ActionRunActionOnResolvedGameObjects", "Action": "Spawn"},
        "Spawn": {"ClassType": "ActionSpawnToLocation", "NextAction": "Run"},
        "Run": {"ClassType": "ActionRunOnInstigator", "ActionToExecute": "Mark"},
        "Mark": {"ClassType": "ActionAbilityDelivered", "NextAction": "Reset", "NextActionWait": True},
        "Reset": {"ClassType": "ActionSetVariable"},
        "Other": {"ClassType": "ActionSetVariable"},
        "Press": {"ClassType": "ActionSetVariable", "NextAction": "Mark", "NextActionWait": True},
    }


def test_a_run_of_a_marker_goes_where_its_group_makes_the_write():
    t = tables(death_path(group_writes=True))
    ec.strip_delivery_markers(t)
    acts = t["actions"]
    assert not {"Run", "Mark"} & set(acts.records)
    assert "NextAction" not in acts.records["Spawn"], "the spawn as the 16.402 graph has it"
    assert acts.records["Press"]["NextAction"] == "Reset"


@pytest.mark.parametrize(("why", "actions", "other"), [
    ("sets", {"G": {"ClassType": "ActionGroup", "SubActions": ["M", "A"], "SubActionsDelay": [0, 0]},
              "M": {**MARK, "ExecuteIfTrue": "x"}, "A": {"ClassType": "ActionPlayEffect"}}, {}),
    ("not a bare step at 0", {"G": {"ClassType": "ActionGroup", "SubActions": ["M", "A"], "SubActionsDelay": [50, 0]},
                              "M": MARK, "A": {"ClassType": "ActionPlayEffect"}}, {}),
    ("not a bare step at 0", {"G": {"ClassType": "ActionGroup", "SubActions": ["M", "A"], "SubActionsDelay": [0, 0]},
                              "M": {**MARK, "NextAction": "A"}, "A": {"ClassType": "ActionPlayEffect"}}, {}),
    ("of delivery markers alone", {"G": {"ClassType": "ActionGroup", "SubActions": ["M"], "SubActionsDelay": [0]},
                                   "M": MARK}, {}),
    ("not the marker's own", {"W": {"ClassType": "ActionSetVariable", "NextAction": "N"},
                              "N": {**MARK, "NextAction": "R", "NextActionWait": True},
                              "R": {"ClassType": "ActionSetVariable"}}, {}),
    ("leads nowhere", {"W": {"ClassType": "ActionSetVariable", "NextAction": "M", "NextActionWait": True},
                       "M": MARK}, {}),
    ("which no group reaching it", death_path(group_writes=False), {}),
    ("still names the delivery marker", {"M": MARK},
     {"characters": {"Hero": {"OnStartingAction": "M"}}}),
    ("still names the delivery marker", {"M": MARK, "G": {"ClassType": "ActionGroup",
                                                          "SubActions": [{"ClassType": "ActionSelect",
                                                                          "SubActions": ["M"]}],
                                                          "SubActionsDelay": [0]}}, {}),
])
def test_a_shape_it_cannot_drop_stops_the_build(why, actions, other):
    t = tables(actions, **other)
    with pytest.raises(SystemExit, match=why):
        ec.strip_delivery_markers(t)


def test_a_button_still_naming_a_marker_stops_the_build():
    t = tables({"M": MARK})
    t.abilities["HeroAbility"] = {"OnActivationAction": "M"}
    with pytest.raises(SystemExit, match=r"abilities\.HeroAbility: still names the delivery marker"):
        ec.strip_delivery_markers(t)


def test_a_graph_without_markers_is_left_as_it_is():
    rows = {"G": {"ClassType": "ActionGroup", "SubActions": ["A", "B"], "SubActionsDelay": [0, 0]},
            "S": {"ClassType": "ActionSpawn", "NextAction": "A"},
            "A": {"ClassType": "ActionPlayEffect"}, "B": {"ClassType": "ActionPlayEffect"}}
    t = tables(rows)
    before = (copy.deepcopy(dict(t["actions"].records)), copy.deepcopy(t["actions"].arrays),
              copy.deepcopy(t["actions"].set_fields))
    ec.strip_delivery_markers(t)
    assert (dict(t["actions"].records), t["actions"].arrays, t["actions"].set_fields) == before
