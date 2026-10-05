"""Each entity row's `unit_type`: what the unit IS, beside `card_id`, the card that put it down.

A summon reports its producer as `card_id`, so a Witch's Skeletons and a Tombstone's report two card ids. `unit_type`
indexes `Battle.unit_types_json()`, one list of unit names per data build, and gives every Skeleton the same id.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
SLOTS = [[0, 1, 2], [0, 1, 2]]
CARDS = ["Witch", "Tombstone", "Skeletons", "SkeletonArmy", "Knight", "Archer", "Giant", "Zap"]
PRODUCERS = {"Witch", "Tombstone", "Skeletons", "SkeletonArmy"}
F = {name: k for k, name in enumerate(royalesim.ENTITY_FIELDS)}


def fnv1a64(data: bytes) -> str:
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def battle() -> "royalesim.Battle":
    return royalesim.Battle(card_names=CARDS, slot_of_k=SLOTS)


def test_the_vocabulary_is_sorted_unique_and_named_by_its_digest():
    b = battle()
    text = b.unit_types_json()
    names = json.loads(text)
    assert names == sorted(set(names))
    for n in ("Skeleton", "Witch", "Tombstone", "Knight", "KingTower", "PrincessTower"):
        assert n in names
    assert "Zap" not in names, "a spell is never on the board"
    assert b.unit_types_digest() == fnv1a64(text.encode("utf-8"))
    # One list per data build, whatever the catalogue lists.
    other = royalesim.Battle(card_names=["Knight", "Giant"], slot_of_k=SLOTS)
    assert other.unit_types_json() == text
    assert other.unit_types_digest() == b.unit_types_digest()


def test_skeletons_from_every_producer_share_one_unit_type():
    b = battle()
    names = json.loads(b.unit_types_json())
    skeleton = names.index("Skeleton")
    ids = list(range(len(CARDS)))
    b.reset(3, [ids, ids], 0, 200, [10000, 10000], None, [], [[0] * 8, [0] * 8])
    played: set[str] = set()
    under: dict[int, set[int]] = {}
    own_named = 0
    for _ in range(400):
        s = json.loads(b.state_json())
        hand = s["players"][0]["hand"]
        costs = s["players"][0]["hand_costs"]
        mana = s["players"][0]["elixir_milli"] // 1000
        for k, cid in enumerate(hand):
            name = CARDS[cid] if 0 <= cid < len(CARDS) else ""
            if name in PRODUCERS and name not in played and 0 <= costs[k] <= mana:
                x = (3 + 4 * len(played)) * 1000 * SUB
                if b.step([(0, k, x, 6000 * SUB)], 1)[0][1] == 0:
                    played.add(name)
                break
        else:
            b.step([], 5)
        for e in json.loads(b.state_json())["entities"]:
            ut = e[F["unit_type"]]
            assert 0 <= ut < len(names), e
            if e[F["tower_slot"]] >= 0:
                assert names[ut] in ("KingTower", "PrincessTower")
                own_named += 1
            elif names[ut] == "Skeleton":
                under.setdefault(ut, set()).add(e[F["card_id"]])
            elif e[F["card_id"]] == CARDS.index("Witch") and e[F["team"]] == 0:
                assert names[ut] == "Witch"
                own_named += 1
        if played == PRODUCERS and len(under.get(skeleton, ())) >= 3:
            break
    assert own_named > 0
    assert set(under) == {skeleton}, "every Skeleton reads the one Skeleton unit_type"
    got = {CARDS[c] for c in under[skeleton]}
    assert len(got) >= 3 and got <= PRODUCERS, f"Skeletons seen under {got}, played {played}"
