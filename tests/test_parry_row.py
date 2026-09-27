"""The counter block the extractor writes (tools/extract_cards.py `parry`).

WHAT THIS PINS. On the client 15.535.29 tables one character row runs an ActionCounter: the Ronin. Its
OnStartingAction is a group of the counter and a cosmetic effect. The counter's SelfAction group is a forced
animation and a cooldown tag, and its InstigatorAction group is a stun (a BuffType spawn), a damage and an effect,
each at its SubActionsDelay. The damage's type sets EnableLevelScaling false in a [DAMAGE_TYPE] section, which the
extractor now routes into its own table. The extractor writes the whole as a `parry` block, on the unit row and on
the card row, and on no other row. The loader (crates/royalesim/src/card.rs `parry_of`) reads the block;
tests/test_card_reads.py holds every field of it to a read.

TRUE AND FALSE ARE PINNED, NOT TRUTHINESS. `deploy_active` and `reflect_level_scaling` come through `flag`, which gives
None for a column the table does not carry at all. The loader refuses None for both, so a None here would be a
refused Ronin, and a truthiness check would not see it.

THE BUILDER IS FAIL-CLOSED. An action it does not know, a group inside a group, a second spawn or damage, a spawn of
anything but a buff, a self group with more than its two actions, a delay list that does not pair, or another root
naming an action row gives no block, and the loader then refuses the row's graph with today's message. The self-test
plants each but the last into a copy of the actions table and sees no block every time, so a builder that accepted any
graph would fail here.

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

# The Ronin's block, field by field, as the 15.535.29 tables give it (ronin.toml).
RONIN = {
    "counter_cooldown_ms": 3500,
    "deploy_active": True,
    "damage_scalar_pct": 200,
    "defense_scalar_pct": 0,
    "root_delays_ms": [0, 0],
    "counter_at": 0,
    "self_delays_ms": [0, 0],
    "self_forced_ms": 500,
    "self_tag_ms": 3500,
    "instigator_delays_ms": [50, 300, 150],
    "stun_at": 0,
    "reflect_at": 1,
    "stun_time_ms": 500,
    "reflect_level_scaling": False,
}
# The stun row's three multipliers: the walk stopped, the attack clock at 5 %.
STUN = {
    "name": "ronin_reflect_stun_buff",
    "speed_multiplier_raw": -100,
    "hit_speed_multiplier_raw": -95,
    "spawn_speed_multiplier_raw": -100,
}


@pytest.fixture(scope="module")
def tables():
    if not RAW.is_dir():
        pytest.skip(
            f"{RAW} is absent: the client 15.535.29 tables are not in this checkout, so the counter builder was NOT "
            "checked. This is not a pass."
        )
    return ec.load_tables("15.535.29")


def block(t, name: str = "Ronin"):
    return ec.parry(t, t["characters"].get(name))


def check_ronin(got: dict | None) -> None:
    assert got is not None, "no counter block on the Ronin"
    for k, want in RONIN.items():
        # `is` for the two flags: True / False, never None and never a truthy stand-in.
        if isinstance(want, bool):
            assert got[k] is want, f"{k}: {got[k]!r}, want {want!r}"
        else:
            assert got[k] == want, f"{k}: {got[k]!r}, want {want!r}"
    for k, want in STUN.items():
        assert got["stun"][k] == want, f"stun.{k}: {got['stun'][k]!r}, want {want!r}"
    extra = set(got) - set(RONIN) - {"stun"}
    assert not extra, f"fields this test does not pin: {sorted(extra)}"


def test_the_ronins_block_field_by_field(tables):
    check_ronin(block(tables))


def test_the_damage_type_table_carries_the_reflects_row(tables):
    row = tables["damage_types"].get("ronin_reflect")
    assert row is not None, "the [DAMAGE_TYPE.ronin_reflect] section was not routed"
    assert ec.flag(row, "EnableLevelScaling") is False
    assert row["ClassType"] == "DamageTypeBasic"


def test_only_the_ronin_carries_one(tables):
    found = []
    for table in ("characters", "buildings"):
        for name in tables[table].records:
            if ec.parry(tables, tables[table].get(name)) is not None:
                found.append(name)
    assert found == ["Ronin"], found
    # The rows whose own action graph also runs a group: the builder is what tells the Ronin apart, so the census
    # above is not vacuous.
    grouped = []
    for table in ("characters", "buildings"):
        for name in tables[table].records:
            g = ec.action_graph(tables, tables[table].get(name))
            if g and "ActionGroup" in g["class_types"]:
                grouped.append(name)
    assert len(grouped) > 2, f"vacuous: only {grouped} run an action group"


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


def instigator_subs(acts, extra: str, delay: int = 0):
    arr = acts.arrays["ronin_reflect_target_group"]
    arr["SubActions"] = [*arr["SubActions"], extra]
    arr["SubActionsDelay"] = [*arr["SubActionsDelay"], delay]


def test_the_builder_refuses_every_other_shape(tables):
    # the control: the unplanted copy still gives the block
    check_ronin(block(planted(tables, lambda acts: None)))

    def extra_class(acts):
        add_action(acts, "PlantHeal", "ronin_reflect_vfx", ClassType="ActionHeal")
        instigator_subs(acts, "PlantHeal")

    def second_spawn(acts):
        add_action(acts, "PlantSecondStun", "ronin_reflect_stun")
        instigator_subs(acts, "PlantSecondStun", 100)

    def second_damage(acts):
        add_action(acts, "PlantSecondDamage", "ronin_reflect_damage")
        instigator_subs(acts, "PlantSecondDamage", 100)

    def unit_spawn(acts):
        acts.records["ronin_reflect_stun"]["SpawnType"] = "CharacterType"

    def nested_group(acts):
        add_action(acts, "PlantInnerGroup", "ronin_counter_self_group")
        instigator_subs(acts, "PlantInnerGroup")

    def self_group_extra(acts):
        add_action(acts, "PlantSelfEffect", "ronin_reflect_vfx")
        arr = acts.arrays["ronin_counter_self_group"]
        arr["SubActions"] = [*arr["SubActions"], "PlantSelfEffect"]
        arr["SubActionsDelay"] = [*arr["SubActionsDelay"], 0]

    def unpaired_delays(acts):
        acts.arrays["ronin_reflect_target_group"]["SubActionsDelay"] = [50, 300]

    for plant, edit in [
        ("an extra class", extra_class),
        ("a second stun", second_spawn),
        ("a second damage", second_damage),
        ("a spawn of a unit", unit_spawn),
        ("a group inside a group", nested_group),
        ("a self group with more than the animation and the tag", self_group_extra),
        ("a delay list that does not pair", unpaired_delays),
    ]:
        assert block(planted(tables, edit)) is None, f"the builder accepted {plant}"
    # the plants touched a copy only
    check_ronin(block(tables))


def load_or_skip(path: pathlib.Path, make: str) -> dict:
    if not path.is_file():
        pytest.skip(f"{path} is absent ({make}), so its rows were NOT checked. This is not a pass.")
    return json.loads(path.read_text(encoding="utf-8"))


def test_the_committed_15535_table_carries_the_block_on_the_ronin_alone():
    doc = load_or_skip(CARDS_15535, "it is committed to this repository")
    carriers = sorted(
        [("card", c["name"]) for c in doc["cards"] if "parry" in c]
        + [("unit", n) for n, u in doc["units"].items() if "parry" in u]
    )
    assert carriers == [("card", "Ronin"), ("unit", "Ronin")]
    check_ronin(next(c for c in doc["cards"] if c["name"] == "Ronin")["parry"])
    check_ronin(doc["units"]["Ronin"]["parry"])


def test_no_2018_row_carries_the_block():
    doc = load_or_skip(CARDS_2018, "tools/extract_cards.py --vintage 2018 writes it")
    carriers = [c["name"] for c in doc["cards"] if "parry" in c]
    carriers += [n for n, u in doc["units"].items() if "parry" in u]
    assert carriers == [], carriers
