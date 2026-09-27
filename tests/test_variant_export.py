"""The Spirit Empress and the Mirror in cards.json and through the protocol (tools/extract_cards.py `variant_block`,
`globals_block`; tools/check_data.py gate 11; card.rs `convert_variant`, `CardGlobals`; state.rs `resolve_play`).

WHAT THIS PINS, 1: THE EXPORT. The 15.535.29 spells_other row MergeMaiden is a LogicBattleSpellVariantData row whose
Options pick a form by the elixir at play: MergeMaiden_Mounted at 6000, MergeMaiden_Normal at 3000, each with
PrecastPendingTime 1200. The extractor writes that as `spell.variant` on that row alone, and `spell.mirror` on the
Mirror's row alone, and a top-level `globals` block with the Mirror's three globals.csv rows. The 2018 file carries
none of it. The builder fails closed: an option key it does not know, or a form that is no card row, stops the build.
Gate 11 holds the table to what the loader reads, and its plant lands.

WHAT THIS PINS, 2: THE PROTOCOL (it needs an extension built from this tree): the catalogue's 10th element lists the
Empress's forms; each player's `hand_costs` and `mirror_target` move with the elixir and the plays; a Mirror with
nothing to copy answers NOTHING_TO_MIRROR. The default catalogue (card_names=None) leaves out the Mirror (code 6) and
the cards that travel under ground (the Miner, the Goblin Drill), which a decoder of codes 0 to 4 cannot place yet, so
it holds no kind code 6; a card_names list that names one gets it (py.rs `Battle::new`).
"""

from __future__ import annotations

import copy
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
CARDS = ROOT / "data" / "derived" / "cards.json"
OLD = ROOT / "data" / "derived" / "cards-2018.json"
RAW = ROOT / "data" / "raw" / "cr-15.535.29"


@pytest.fixture(scope="module")
def doc() -> dict:
    if not CARDS.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {CARDS.relative_to(ROOT)} is absent (the README's stage 3 writes it)")
    return json.loads(CARDS.read_text(encoding="utf-8"))


def test_the_empress_is_the_one_variant_card(doc):
    rows = [c for c in doc["cards"] if "variant" in (c.get("spell") or {})]
    assert [c["name"] for c in rows] == ["MergeMaiden"]
    assert rows[0]["spell"]["variant"] == {
        "options": [
            {"trigger_milli": 6000, "precast_pending_ms": 1200, "card": "MergeMaiden_Mounted"},
            {"trigger_milli": 3000, "precast_pending_ms": 1200, "card": "MergeMaiden_Normal"},
        ],
        "use_projected_time_summon": True,
        "mirror_uses_root_spell": False,
        "source": "spells_other.MergeMaiden.Options (2 entries)",
    }
    forms = {c["name"]: c for c in doc["cards"] if c["name"].startswith("MergeMaiden_")}
    assert {n: (c["kind"], c["elixir"]) for n, c in forms.items()} == {
        "MergeMaiden_Normal": ("troop", 3),
        "MergeMaiden_Mounted": ("troop", 6),
    }
    assert rows[0]["elixir"] == 6, "the card's own cost is its first form's"


def test_the_mirror_flag_and_its_globals(doc):
    assert [c["name"] for c in doc["cards"] if (c.get("spell") or {}).get("mirror")] == ["Mirror"]
    assert doc["globals"] == {
        "MIRROR_LEVEL_OFFSET": 1,
        "MIRROR_CAP_TO_MAX_LEVEL": False,
        "MIRROR_IGNORE_CHAMPIONS": False,
    }
    assert "globals.csv" in doc["provenance"]["files"], "the file the globals come from is in the provenance"


def test_the_2018_file_carries_none_of_it():
    if not OLD.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {OLD.relative_to(ROOT)} is absent (tools/extract_cards.py --vintage 2018)")
    old = json.loads(OLD.read_text(encoding="utf-8"))
    assert "globals" not in old
    assert not [c["name"] for c in old["cards"] if {"mirror", "variant"} & set(c.get("spell") or {})]


def test_gate_11_holds_the_table_to_what_the_loader_reads(doc):
    import sys

    sys.path.insert(0, str(ROOT / "tools"))
    import check_data

    fail, info = check_data.variant_cards(doc)
    assert (fail, info) == ([], ["variant cards: 1 checked"])
    for what, edit, want in [
        ("a trigger off its form's cost", lambda o, c: o[0].__setitem__("trigger_milli", 5000), "is not its form"),
        ("triggers ascending", lambda o, c: o.reverse(), "does not strictly descend"),
        ("a form that is no card", lambda o, c: o[1].__setitem__("card", "Nobody"), "no card row"),
        ("the card's own cost moved", lambda o, c: c.__setitem__("elixir", 5), "costs 5"),
    ]:
        d = copy.deepcopy(doc)
        card = next(c for c in d["cards"] if c["name"] == "MergeMaiden")
        edit(card["spell"]["variant"]["options"], card)
        fail, _ = check_data.variant_cards(d)
        assert any(want in f for f in fail), f"{what}: {fail}"
    none = {"cards": [c for c in doc["cards"] if c["name"] != "MergeMaiden"]}
    assert check_data.variant_cards(none) == ([], ["variant cards: 0 checked (this table has none)"])


@pytest.fixture(scope="module")
def tables():
    if not RAW.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {RAW.relative_to(ROOT)} is absent; the builder reads the 15.535.29 tables")
    import sys

    sys.path.insert(0, str(ROOT / "tools"))
    import extract_cards as ec

    return ec, ec.load_tables()


def test_the_variant_builder_fails_closed(tables):
    ec, t = tables
    s = t["spells_other"].get("MergeMaiden")
    assert ec.variant_block(t, s) is not None
    opts = t["spells_other"].arrays["MergeMaiden"]["Options"]
    for k, key, value, want in [(0, "UnknownKey", 1, "unknown keys"), (1, "SpellData", "Nobody", "is no card row")]:
        old = opts[k].get(key)
        opts[k][key] = value
        try:
            with pytest.raises(SystemExit, match=want):
                ec.variant_block(t, s)
        finally:
            if old is None:
                del opts[k][key]
            else:
                opts[k][key] = old
    cls = s["CustomClassType"]
    s["CustomClassType"] = None
    try:
        with pytest.raises(SystemExit, match="Options without LogicBattleSpellVariantData"):
            ec.variant_block(t, s)
    finally:
        s["CustomClassType"] = cls
    assert ec.variant_block(t, s) is not None, "the rows were not restored"
    others = [
        n for n, r in t["spells_other"].records.items() if n != "MergeMaiden" and ec.variant_block(t, r) is not None
    ]
    assert others == [], others
    assert ec.globals_block(t.vintage)["MIRROR_LEVEL_OFFSET"] == 1


# ---- 2. the protocol


NAMES = ["MergeMaiden", "MergeMaiden_Mounted", "MergeMaiden_Normal", "Mirror", "Knight", "Archer", "Giant", "Fireball"]
ID = {n: k for k, n in enumerate(NAMES)}
SLOTS = [[0, 1, 2], [0, 1, 2]]


@pytest.fixture(scope="module")
def royalesim():
    rs = pytest.importorskip("royalesim")
    try:
        rs.Battle(card_names=NAMES, slot_of_k=SLOTS)
    except Exception as e:
        pytest.fail(f"this build refuses the Spirit Empress or the Mirror ({e}): rebuild the extension from this tree")
    if "NOTHING_TO_MIRROR" not in rs.DEPLOY_REASONS:
        pytest.fail("this build has no NOTHING_TO_MIRROR reason: rebuild the extension from this tree")
    return rs


def test_the_catalogue_lists_the_empress_forms(royalesim):
    b = royalesim.Battle(card_names=NAMES, slot_of_k=SLOTS)
    rows = json.loads(b.catalogue_json())
    col = list(royalesim.CATALOGUE_FIELDS).index("variants")
    assert rows[ID["MergeMaiden"]][col] == [[6000, ID["MergeMaiden_Mounted"], 6], [3000, ID["MergeMaiden_Normal"], 3]]
    assert rows[ID["Mirror"]][1] == 6, "the Mirror's kind code"
    assert all(r[col] is None for k, r in enumerate(rows) if k != ID["MergeMaiden"])


def test_the_default_catalogue_leaves_the_mirror_and_the_tunnellers_out(royalesim):
    """A card_names=None catalogue holds no Mirror, so no kind code 6: a decoder that maps only codes 0 to 4 refuses
    a catalogue with a 6 (RoyaleGym's does, until it maps 6). The Miner and the Goblin Drill are left out too: they
    go down anywhere but water with a troop's or a building's footprint rule, a pair no code 0 to 4 describes (code
    4 is that territory for a spell, with no footprint). Named in card_names each is there. The other cards this
    batch loads stay in the default catalogue."""
    rows = json.loads(royalesim.Battle(None, SLOTS).catalogue_json())
    names = [r[0] for r in rows]
    for card in ("Mirror", "Miner", "GoblinDrill"):
        assert card not in names, f"the default catalogue holds {card}"
    for card in ("MergeMaiden", "ThreeMusketeers", "RamRider", "Elixir Collector", "FirespiritHut"):
        assert card in names, f"the default catalogue lost {card} besides the Mirror and the tunnellers"
    assert {r[1] for r in rows} <= {0, 1, 2, 3, 4}, sorted({r[1] for r in rows})
    named = [r[0] for r in json.loads(royalesim.Battle(["Knight", "Miner", "GoblinDrill"], SLOTS).catalogue_json())]
    assert {"Miner", "GoblinDrill"} <= set(named), named


def test_the_hand_prices_move_with_the_elixir_and_the_plays(royalesim):
    sub = royalesim.SUBTILE_PER_MILLITILE
    deck = [
        ID[n]
        for n in [
            "Knight",
            "MergeMaiden",
            "Archer",
            "Giant",
            "Mirror",
            "Fireball",
            "MergeMaiden_Normal",
            "MergeMaiden_Mounted",
        ]
    ]
    b = royalesim.Battle(card_names=NAMES, slot_of_k=SLOTS)
    b.reset(0, [deck, deck], 0, 200, [5_000, 5_000], None, [])
    p = json.loads(b.state_json())["players"][0]
    assert (p["hand_costs"], p["mirror_target"]) == ([3, 3, 3, 5], -1), p
    assert b.hand_costs(0) == [3, 3, 3, 5]
    assert b.mirror_target(0) == -1
    [(_card, reason, *_)] = b.step([(0, 0, 9500 * sub, 9500 * sub)], 1)
    assert royalesim.DEPLOY_REASONS[reason] == "OK"
    p = json.loads(b.state_json())["players"][0]
    assert p["hand"][0] == ID["Mirror"], p
    assert p["mirror_target"] == ID["Knight"], p
    assert p["hand_costs"][0] == 4, p


def test_a_mirror_with_nothing_to_copy_answers_nothing_to_mirror(royalesim):
    sub = royalesim.SUBTILE_PER_MILLITILE
    # A four-card deck: the Mirror is dealt with nothing behind the hand.
    deck = [ID[n] for n in ["Mirror", "Knight", "Archer", "Giant"]]
    b = royalesim.Battle(card_names=NAMES, slot_of_k=SLOTS)
    b.reset(0, [deck, deck], 0, 200, [10_000, 10_000], None, [])
    assert royalesim.DEPLOY_REASONS[b.check_deploy(0, 0, 9500 * sub, 9500 * sub)] == "NOTHING_TO_MIRROR"
    assert b.hand_costs(0)[0] == -1
