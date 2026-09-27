"""The Furnace: a troop card with an interval spawner (tools/extract_cards.py `interval_spawner`, the card's kind
from the row it puts on the board; card.rs `interval_spawner_of`; spawner.INTERVAL_START_ORIGIN,
SPAWN_TO_LOCATION_OFFSET).

WHAT THIS PINS, 1: THE EXPORT. The 15.535.29 tables give the Furnace (FirespiritHut, a spells_buildings card) the
characters row Furnace_rework, which walks and shoots, and an ActionInterval (StartCounterAt 1950, Interval 5000)
running an ActionSpawnToLocation of one Fire Spirit (DeployTime 500, MirroredX 0, MirroredY 3). The extractor writes
that as an `interval_spawner` block on the unit and the card, and the card's kind as troop with `card_table_kind`
building beside it. Exactly one card changes kind; exactly one row carries the block; the builder returns nothing
for a row that sets a key it does not know (fail closed), so a later table cannot slip a mechanic past the loader.

WHAT THIS PINS, 2: THE BATTLE, through the protocol (it needs an extension built from this tree). The first Fire
Spirit 38 ticks after the Furnace's first frame, then every 100, measured on client 16.402 (2 of 2 and 7 of 7), at
(0, +1500) from the Furnace in its owner's frame; the catalogue row's kind code is a troop's.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
CARDS = ROOT / "data" / "derived" / "cards.json"
RAW = ROOT / "data" / "raw" / "cr-15.535.29"


@pytest.fixture(scope="module")
def doc() -> dict:
    if not CARDS.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {CARDS.relative_to(ROOT)} is absent (the README's stage 3 writes it)")
    return json.loads(CARDS.read_text(encoding="utf-8"))


def test_exactly_the_furnace_takes_its_kind_from_its_unit_row(doc):
    moved = [c["name"] for c in doc["cards"] if "card_table_kind" in c]
    assert moved == ["FirespiritHut"], f"cards whose kind is not their table's: {moved}"
    furnace = next(c for c in doc["cards"] if c["name"] == "FirespiritHut")
    assert (furnace["kind"], furnace["card_table_kind"]) == ("troop", "building")
    assert doc["units"][furnace["summon_character"]]["source_table"] == "characters"
    drill = next(c for c in doc["cards"] if c["name"] == "GoblinDrill")
    assert drill["kind"] == "building", "a row that travels underground first keeps its table's kind"


def test_the_interval_block_is_the_furnaces_alone(doc):
    units = sorted(n for n, u in doc["units"].items() if "interval_spawner" in u)
    cards = sorted(c["name"] for c in doc["cards"] if "interval_spawner" in c)
    assert (units, cards) == (["Furnace_rework"], ["FirespiritHut"])
    block = doc["units"]["Furnace_rework"]["interval_spawner"]
    assert block == {
        "start_counter_at_ms": 1950,
        "interval_ms": 5000,
        "affected_by_spawn_speed": True,
        "pause_tags": ["NO_SUMMON", "UNIT_CUSTOM_TAG_1"],
        "character": "FireSpirits",
        "deploy_time_ms": 500,
        "mirrored_x": 0,
        "mirrored_y": 3,
    }


@pytest.fixture(scope="module")
def tables():
    if not RAW.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {RAW.relative_to(ROOT)} is absent; the builder reads the 15.535.29 tables")
    import sys

    sys.path.insert(0, str(ROOT / "tools"))
    import extract_cards as ec

    return ec, ec.load_tables()


def test_the_builder_fails_closed_on_a_key_it_does_not_know(tables):
    """THE PLANT FOR THE KEY SETS: each action row grows one key the builder does not list, and the block must go.
    Without this the key-set rule could be a no-op and every assertion above would still pass."""
    ec, t = tables
    row = t["characters"].get("Furnace_rework")
    assert ec.interval_spawner(t, row) is not None, "the Furnace's own rows give no block"
    for action, key, value in [
        ("Furnace_rework_continuous_spawn", "ActionDelay", 100),
        ("Furnace_rework_spawn_forward", "RelativeX", 2),
        ("Furnace_rework_spawn_forward", "Count", 2),
    ]:
        rec = t["actions"].get(action)
        old = rec[key]
        rec[key] = value
        try:
            assert ec.interval_spawner(t, row) is None, f"{action}.{key} = {value} still gives a block"
        finally:
            rec[key] = old
    assert ec.interval_spawner(t, row) is not None, "the rows were not restored"


def test_every_other_action_interval_root_gives_no_block(tables):
    ec, t = tables
    got = []
    for tn in ("characters", "buildings"):
        for name, rec in t[tn].records.items():
            if ec.interval_spawner(t, rec) is not None:
                got.append(name)
    assert got == ["Furnace_rework"], got


# ---- 2. the battle, through the protocol


@pytest.fixture(scope="module")
def royalesim():
    rs = pytest.importorskip("royalesim")
    try:
        rs.Battle(card_names=["FirespiritHut", "FireSpirits"], slot_of_k=[[0, 1, 2], [0, 1, 2]])
    except Exception as e:
        pytest.fail(f"this build refuses the Furnace ({e}): rebuild the extension from this tree")
    return rs


def test_the_catalogue_calls_the_furnace_a_troop(royalesim):
    b = royalesim.Battle(card_names=["FirespiritHut", "FireSpirits"], slot_of_k=[[0, 1, 2], [0, 1, 2]])
    rows = json.loads(b.catalogue_json())
    assert rows[0][0] == "FirespiritHut"
    assert rows[0][1] == 0, f"kind code {rows[0][1]}; a troop's is 0"


def test_spirits_on_f_plus_38_then_every_100_at_1500_forward(royalesim):
    sub = royalesim.SUBTILE_PER_MILLITILE
    deck = ["FirespiritHut", "FireSpirits", "Knight", "Archer", "Giant", "Musketeer", "Fireball", "Zap"]
    ids = list(range(len(deck)))
    b = royalesim.Battle(card_names=deck, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [])
    b.step([], 1)
    played = b.step([(0, 0, 7000 * sub, 9000 * sub)], 1)
    assert played, "the play did not resolve"
    assert played[0][0] == 0, f"another card resolved: {played}"
    first: dict = {}
    furnace_at: dict = {}
    for t in range(260):
        for e in json.loads(b.state_json())["entities"]:
            # ENTITY_FIELDS: 0 uid, 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y
            if e[1] != 0 or e[4] >= 0:
                continue
            if e[3] == 0:
                furnace_at[t] = (e[5], e[6])
            if e[0] not in first:
                first[e[0]] = (t, e[3], e[5], e[6])
        b.step([], 1)
    furnaces = [v for v in first.values() if v[1] == 0]
    spirits = sorted(v for v in first.values() if v[1] == 1)
    assert len(furnaces) == 1, first
    f0 = furnaces[0][0]
    assert [s[0] - f0 for s in spirits] == [38, 138, 238], f"spirit ticks after the Furnace's first frame: {spirits}"
    t, _, x, y = spirits[0]
    fx, fy = furnace_at[t]
    assert (x - fx, y - fy) == (0, 1500 * sub), (
        f"the first spirit's offset, native {((x - fx) // sub, (y - fy) // sub)}"
    )
