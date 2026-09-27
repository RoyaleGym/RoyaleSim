"""The health-threshold transformation block the extractor writes (tools/extract_cards.py `transform_at_hp`).

WHAT THIS PINS. On the client 15.535.29 tables two character rows run an ActionRunActionAtHealth whose action is an
ActionChangeGameObjectData into another character row. The Cannon Cart (MovingCannon) becomes BrokenCannon at 50 % of
its hitpoints. The Goblin Demolisher, at 50 %, spawns a taunt cancel and 100 ms later becomes
GoblinDemolisher_kamikaze_form with its target reset. The extractor writes each as a `transform_at_hp` block, on the
unit row and on the card row, and on no other row. The loader (crates/royalesim/src/card.rs `transform_of`) reads the
block; tests/test_card_reads.py holds every field of it to a read.

THE BUILDER IS FAIL-CLOSED. An action it does not know, a group inside the group, a second transformation or a change
of a projectile gives no block, and the loader then refuses the row's graph with today's message. The self-test plants
each of those into a copy of the actions table and sees no block every time, so a builder that accepted any graph
would fail here.

THE TABLES. The raw client tables (data/raw/cr-15.535.29) are not in every checkout. Without them the tests that read
them SKIP LOUDLY, naming the directory: a skip is not a pass. The two tests on the committed card tables need only
those files, and skip loudly when one is absent.
"""

from __future__ import annotations

import copy
import json
import pathlib
import sys

import pytest

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
import extract_cards as ec  # noqa: E402

RAW = ROOT / "data" / "raw" / "cr-15.535.29"
CARDS_15535 = ROOT / "data" / "derived" / "cards-15.535.json"
CARDS_2018 = ROOT / "data" / "derived" / "cards-2018.json"

CART = {
    "into": "BrokenCannon",
    "reset_target": False,
    "group_delays_ms": [],
    "at": 0,
    "pct": 50,
    "noop_spawns": [],
}
DEMOLISHER = {
    "into": "GoblinDemolisher_kamikaze_form",
    "reset_target": True,
    "group_delays_ms": [0, 100],
    "at": 1,
    "pct": 50,
    "noop_spawns": ["AreaEffectType:CancelTauntAEO"],
}


@pytest.fixture(scope="module")
def tables():
    if not RAW.is_dir():
        pytest.skip(
            f"{RAW} is absent: the client 15.535.29 tables are not in this checkout, so the transformation "
            "builder was NOT checked. This is not a pass."
        )
    return ec.load_tables("15.535.29")


def block(t, name: str):
    return ec.transform_at_hp(t, t["characters"].get(name))


def test_the_two_blocks_exactly(tables):
    assert block(tables, "MovingCannon") == CART
    assert block(tables, "GoblinDemolisher") == DEMOLISHER


def test_only_the_two_rows_carry_one(tables):
    found = []
    for table in ("characters", "buildings"):
        for name in tables[table].records:
            if ec.transform_at_hp(tables, tables[table].get(name)) is not None:
                found.append(name)
    assert sorted(found) == ["GoblinDemolisher", "MovingCannon"], found
    # The rows that also start from an ActionRunActionAtHealth, and are refused as before: the builder is what
    # tells them apart, so the census above is not vacuous.
    starts_at_health = []
    for table in ("characters", "buildings"):
        for name in tables[table].records:
            row = tables[table].get(name)
            g = ec.action_graph(tables, row)
            root = (g or {}).get("roots", {}).get("OnStartingAction")
            a = tables["actions"].get(root) if root else None
            if a is not None and a["ClassType"] == "ActionRunActionAtHealth":
                starts_at_health.append(name)
    assert len(starts_at_health) > 2, f"vacuous: only {starts_at_health} start from a health trigger"


def planted(tables, edit):
    """A copy of the tables whose actions table `edit` has changed; the original is left alone."""
    t = copy.copy(tables)
    t["actions"] = copy.deepcopy(tables["actions"])
    edit(t["actions"])
    return t


def add_action(acts, name: str, like: str, **cols):
    row = copy.deepcopy(acts.get(like))
    for k, v in cols.items():
        row[k] = v
    acts.records[name] = row
    acts.arrays[name] = {}


def group_subs(acts, extra: str, delay: int = 0):
    arr = acts.arrays["GoblinDemolisher_transformation_group"]
    arr["SubActions"] = [*arr["SubActions"], extra]
    arr["SubActionsDelay"] = [*arr["SubActionsDelay"], delay]


def test_the_builder_refuses_every_other_shape(tables):
    # the control: the unplanted copy still gives both blocks
    same = planted(tables, lambda acts: None)
    assert (block(same, "MovingCannon"), block(same, "GoblinDemolisher")) == (CART, DEMOLISHER)

    def extra_class(acts):
        add_action(acts, "PlantDealDamage", "GoblinDemolisher_kamikaze_transformation", ClassType="ActionDealDamage")
        group_subs(acts, "PlantDealDamage")

    def nested_group(acts):
        add_action(acts, "PlantOuterGroup", "GoblinDemolisher_transformation_group")
        acts.arrays["PlantOuterGroup"] = {
            "SubActions": ["GoblinDemolisher_transformation_group"],
            "SubActionsDelay": [0],
        }
        acts.records["GoblinDemolisher_trigger_at_health"]["Actions"] = "PlantOuterGroup"

    def second_change(acts):
        add_action(acts, "PlantSecondChange", "GoblinDemolisher_kamikaze_transformation")
        group_subs(acts, "PlantSecondChange", 150)

    def projectile_change(acts):
        row = acts.records["MovingCannon_transformation"]
        row["NewCharacterData"] = None
        row["NewProjectileData"] = "MovingCannonProjectile"

    def unpaired_delays(acts):
        acts.arrays["GoblinDemolisher_transformation_group"]["SubActionsDelay"] = [0, 100, 200]

    for plant, edit, card in [
        ("an extra class", extra_class, "GoblinDemolisher"),
        ("a group inside the group", nested_group, "GoblinDemolisher"),
        ("a second transformation", second_change, "GoblinDemolisher"),
        ("a change of a projectile", projectile_change, "MovingCannon"),
        ("a delay list that does not pair", unpaired_delays, "GoblinDemolisher"),
    ]:
        assert block(planted(tables, edit), card) is None, f"the builder accepted {plant}"
    # the plants touched a copy only
    assert (block(tables, "MovingCannon"), block(tables, "GoblinDemolisher")) == (CART, DEMOLISHER)


def load_or_skip(path: pathlib.Path, make: str) -> dict:
    if not path.is_file():
        pytest.skip(f"{path} is absent ({make}), so its rows were NOT checked. This is not a pass.")
    return json.loads(path.read_text(encoding="utf-8"))


def test_the_committed_15535_table_carries_the_two_blocks_and_no_other():
    doc = load_or_skip(CARDS_15535, "it is committed to this repository")
    carriers = sorted(
        [("card", c["name"]) for c in doc["cards"] if "transform_at_hp" in c]
        + [("unit", n) for n, u in doc["units"].items() if "transform_at_hp" in u]
    )
    assert carriers == [
        ("card", "GoblinDemolisher"),
        ("card", "MovingCannon"),
        ("unit", "GoblinDemolisher"),
        ("unit", "MovingCannon"),
    ]
    rows = {c["name"]: c for c in doc["cards"]}
    assert rows["MovingCannon"]["transform_at_hp"] == CART
    assert rows["GoblinDemolisher"]["transform_at_hp"] == DEMOLISHER
    assert doc["units"]["MovingCannon"]["transform_at_hp"] == CART
    assert doc["units"]["GoblinDemolisher"]["transform_at_hp"] == DEMOLISHER


def test_no_2018_row_carries_the_block():
    doc = load_or_skip(CARDS_2018, "tools/extract_cards.py --vintage 2018 writes it")
    carriers = [c["name"] for c in doc["cards"] if "transform_at_hp" in c]
    carriers += [n for n, u in doc["units"].items() if "transform_at_hp" in u]
    assert carriers == [], carriers
