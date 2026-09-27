"""The Three Musketeers: members at explicit offsets and an attack selector (tools/extract_cards.py
`summon_members`, `attack_select`; card.rs `SummonMemberDef`, `AttackSelectDef`; formation.EXPLICIT_OFFSETS_FRAME,
combat.ATTACK_SELECT_MOMENT, combat.ATTACK_SELECT_RANGE).

WHAT THIS PINS, 1: THE EXPORT. The 15.535.29 tables give the Three Musketeers card a SummonCharactersList of three
character rows with SummonCharactersOffsetsX [0, -1000, 1000] and OffsetsY [-1000, 1000, 1000], and
CharactersOffsetsXMirrored. Each character row starts its attack with an ActionFilter
"target_in_range(ThreeMusketeer_Rework_melee_range) && target_is_ground" (the variable's DefaultValue 1600) that sets
the AttackSequenceList entry: 1, a bayonet (ActionRunOnInstigator -> ActionDealDamage 123, a plain damage type), or 0,
the row's own projectile. The extractor writes `summon_members` and `summon_offsets_x_mirrored` on the card and
`attack_select` on the card and on the four musketeer rows, and nowhere else. The builders fail closed: a key they do
not know on any action they walk gives no block, and a list and an offsets table of different lengths stop the build.

WHAT THIS PINS, 2: THE BATTLE, through the protocol (it needs an extension built from this tree): the three members
stand at (0, +1000), (+1000, -1000), (-1000, -1000) from a side-0 tap on a right-half column, measured on client
15.535.29.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
CARDS = ROOT / "data" / "derived" / "cards.json"
OLD = ROOT / "data" / "derived" / "cards-2018.json"
RAW = ROOT / "data" / "raw" / "cr-15.535.29"
MEMBERS = [
    "ThreeMusketeer_Rework_Character_1",
    "ThreeMusketeer_Rework_Character_2",
    "ThreeMusketeer_Rework_Character_3",
]


@pytest.fixture(scope="module")
def doc() -> dict:
    if not CARDS.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {CARDS.relative_to(ROOT)} is absent (the README's stage 3 writes it)")
    return json.loads(CARDS.read_text(encoding="utf-8"))


def test_the_members_and_their_offsets_are_the_three_musketeers_alone(doc):
    with_members = [c["name"] for c in doc["cards"] if "summon_members" in c]
    assert with_members == ["ThreeMusketeers"], with_members
    card = next(c for c in doc["cards"] if c["name"] == "ThreeMusketeers")
    assert card["summon_members"] == [
        {"character": MEMBERS[0], "offset_x_milli": 0, "offset_y_milli": -1000},
        {"character": MEMBERS[1], "offset_x_milli": -1000, "offset_y_milli": 1000},
        {"character": MEMBERS[2], "offset_x_milli": 1000, "offset_y_milli": 1000},
    ]
    assert card["summon_offsets_x_mirrored"] is True
    assert card["summon_character"] == MEMBERS[0], "member 0 is the card's own row"
    assert [doc["units"][m]["load_time_ms"] for m in MEMBERS] == [700, 650, 700], "each member is its own row"


def test_the_selector_is_on_the_musketeers_alone(doc):
    block = {
        "melee_range_milli": 1600,
        "melee_ground_only": True,
        "melee_damage": 123,
        "melee_index": 1,
        "ranged_index": 0,
    }
    units = sorted(n for n, u in doc["units"].items() if "attack_select" in u)
    cards = sorted(c["name"] for c in doc["cards"] if "attack_select" in c)
    assert (units, cards) == (["ThreeMusketeer_Rework", *MEMBERS], ["ThreeMusketeers"])
    for n in units:
        assert doc["units"][n]["attack_select"] == block, n
        roots = doc["units"][n]["action_graph"]["roots"]
        assert roots["AttackSequenceList[1].DoAttackAction"] == "ThreeMusketeer_Rework_Bayonet_Attack", n


def test_the_2018_file_carries_none_of_it():
    if not OLD.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {OLD.relative_to(ROOT)} is absent (tools/extract_cards.py --vintage 2018)")
    old = json.loads(OLD.read_text(encoding="utf-8"))
    rows = [*old["cards"], *old["units"].values()]
    assert not [r["name"] for r in rows if "summon_members" in r or "attack_select" in r]


@pytest.fixture(scope="module")
def tables():
    if not RAW.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {RAW.relative_to(ROOT)} is absent; the builders read the 15.535.29 tables")
    import sys

    sys.path.insert(0, str(ROOT / "tools"))
    import extract_cards as ec

    return ec, ec.load_tables()


def test_the_selector_builder_fails_closed_on_a_key_it_does_not_know(tables):
    """THE PLANT FOR THE KEY SETS: each action the builder walks grows one key it does not list, and the block must
    go. Without this the key-set rule could be a no-op and every assertion above would still pass."""
    ec, t = tables
    name = MEMBERS[0]
    row = t["characters"].get(name)
    acts = t["actions"]
    assert ec.attack_select(t, "characters", name, row) is not None, "the musketeer's own rows give no block"
    f = row["OnStartingAttackAction"]
    run = t["characters"].arrays[name]["AttackSequenceList"][1]["DoAttackAction"]
    walked = [f, acts.get(f)["OnTrueAction"], acts.get(f)["OnFalseAction"], run, acts.get(run)["ActionToExecute"]]
    for action in walked:
        keys = acts.set_fields[action]
        acts.set_fields[action] = keys | {"UnknownKey"}
        try:
            assert ec.attack_select(t, "characters", name, row) is None, (
                f"{action} with an unknown key still gives a block"
            )
        finally:
            acts.set_fields[action] = keys
    # Another condition, and a reach variable that is not a positive integer.
    cond = acts.get(f)["Condition"]
    acts.get(f)["Condition"] = "target_in_range(ThreeMusketeer_Rework_melee_range)"
    try:
        assert ec.attack_select(t, "characters", name, row) is None, "a condition without target_is_ground"
    finally:
        acts.get(f)["Condition"] = cond
    var = t.variables["ThreeMusketeer_Rework_melee_range"]
    old = var["DefaultValue"]
    var["DefaultValue"] = 0
    try:
        assert ec.attack_select(t, "characters", name, row) is None, "a reach of 0"
    finally:
        var["DefaultValue"] = old
    assert ec.attack_select(t, "characters", name, row) is not None, "the rows were not restored"


def test_no_other_row_gives_a_selector(tables):
    ec, t = tables
    got = sorted(
        name
        for name, rec in t["characters"].records.items()
        if ec.attack_select(t, "characters", name, rec) is not None
    )
    assert got == ["ThreeMusketeer_Rework", *MEMBERS], got


def test_a_list_and_offsets_of_different_lengths_stop_the_build(tables):
    ec, t = tables
    s = t["spells_characters"].get("ThreeMusketeers")
    res = {"source": "spells_characters.ThreeMusketeers.SummonCharactersList (overlay; 3 entries)"}
    got = ec.summon_members(t, "spells_characters", s, res)
    assert [m["character"] for m in got] == MEMBERS
    assert (
        ec.summon_members(t, "spells_characters", s, {"source": "spells_characters.ThreeMusketeers.SummonCharacter"})
        is None
    ), "a card resolved any other way carries no members"
    arr = t["spells_characters"].arrays["ThreeMusketeers"]
    xs = arr["SummonCharactersOffsetsX"]
    arr["SummonCharactersOffsetsX"] = xs[:2]
    try:
        with pytest.raises(SystemExit, match="SummonCharactersList has 3 entries"):
            ec.summon_members(t, "spells_characters", s, res)
    finally:
        arr["SummonCharactersOffsetsX"] = xs


# ---- 2. the battle, through the protocol


@pytest.fixture(scope="module")
def royalesim():
    rs = pytest.importorskip("royalesim")
    try:
        rs.Battle(card_names=["ThreeMusketeers", "Knight"], slot_of_k=[[0, 1, 2], [0, 1, 2]])
    except Exception as e:
        pytest.fail(f"this build refuses the Three Musketeers ({e}): rebuild the extension from this tree")
    return rs


def test_the_members_stand_at_the_negated_offsets(royalesim):
    sub = royalesim.SUBTILE_PER_MILLITILE
    deck = ["ThreeMusketeers", "Knight", "Archer", "Giant", "Musketeer", "Fireball", "Zap", "Valkyrie"]
    ids = list(range(len(deck)))
    b = royalesim.Battle(card_names=deck, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [])
    tap = (12500, 11500)  # a tile centre: the tap snap moves nothing
    played = b.step([(0, 0, tap[0] * sub, tap[1] * sub)], 1)
    assert played, "the play did not resolve"
    assert played[0][1] == 0, f"the play was refused: {played}"
    b.step([], 1)
    # ENTITY_FIELDS: 0 uid, 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y
    members = sorted(
        (e[0], e[5] // sub, e[6] // sub)
        for e in json.loads(b.state_json())["entities"]
        if e[1] == 0 and e[4] < 0 and e[3] == 0
    )
    assert [(x - tap[0], y - tap[1]) for _, x, y in members] == [(0, 1000), (1000, -1000), (-1000, -1000)], members
