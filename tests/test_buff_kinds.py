"""`Battle.buff_kinds_json()`: every name an entity row's `buffs` entry can carry, one sorted list per data build.

A row names each buff it carries, and a name is one buff's row or a '|'-joined group of rows the card table gives
identical values. The list holds them whole; their members (split on '|') are the stable names a pin can hold.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
SLOTS = [[0, 1, 2], [0, 1, 2]]
CARDS = ["Freeze", "Zap", "Rage", "Poison", "IceWizard", "Knight", "Archer", "Giant"]
SPELLS_AT_THE_TOWER = {"Freeze", "Zap", "Poison"}
F = {name: k for k, name in enumerate(royalesim.ENTITY_FIELDS)}


def fnv1a64(data: bytes) -> str:
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def battle(cards=CARDS) -> "royalesim.Battle":
    return royalesim.Battle(card_names=cards, slot_of_k=SLOTS)


def test_the_list_is_sorted_unique_named_by_its_digest_and_one_per_table():
    b = battle()
    text = b.buff_kinds_json()
    kinds = json.loads(text)
    assert kinds, "the card table has buffs"
    assert kinds == sorted(set(kinds))
    assert all(k and all(m for m in k.split("|")) for k in kinds), "an empty name or member"
    assert b.buff_kinds_digest() == fnv1a64(text.encode("utf-8"))
    # One list per data build, whatever the catalogue lists.
    other = battle(["Knight", "Giant"])
    assert other.buff_kinds_json() == text
    assert other.buff_kinds_digest() == b.buff_kinds_digest()


def test_every_buff_a_battle_puts_on_a_row_is_in_the_list():
    """Freeze, Zap and Poison cast on the red left princess tower, Rage on the blue side, an Ice Wizard and a Knight
    beside it: every name any row carries over the battle is in the list. Not vacuous: the casts must land buffs."""
    b = battle()
    kinds = set(json.loads(b.buff_kinds_json()))
    ids = list(range(len(CARDS)))
    b.reset(5, [ids, ids], 0, 200, [10000, 10000], None, [], [[0] * 8, [0] * 8])
    played: set[str] = set()
    seen: set[str] = set()
    for _ in range(900):
        s = json.loads(b.state_json())
        hand = s["players"][0]["hand"]
        costs = s["players"][0]["hand_costs"]
        mana = s["players"][0]["elixir_milli"] // 1000
        for k, cid in enumerate(hand):
            name = CARDS[cid] if 0 <= cid < len(CARDS) else ""
            if name in played or not 0 <= costs[k] <= mana:
                continue
            x, y = (3500, 25500) if name in SPELLS_AT_THE_TOWER else (9000, 6000)
            if b.step([(0, k, x * SUB, y * SUB)], 1)[0][1] == 0:
                played.add(name)
            break
        else:
            b.step([], 3)
        for e in json.loads(b.state_json())["entities"]:
            for name, _ms in e[F["buffs"]]:
                assert name in kinds, f"a row carries {name!r}, which buff_kinds_json() does not list"
                seen.add(name)
        if played >= SPELLS_AT_THE_TOWER and len(seen) >= 2:
            break
    assert played >= SPELLS_AT_THE_TOWER, f"played only {played}"
    assert len(seen) >= 2, f"the casts put only {seen} on rows"
