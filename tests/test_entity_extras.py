"""`Battle.state_json(extra=True)`: every entity row followed by `ENTITY_EXTRA_FIELDS`, opt-in.

Without `extra` the rows are byte for byte the ENTITY_FIELDS rows, so a reader that pins their length keeps working.
The Rust test `extra_entity_columns_read_what_the_battle_then_does` reads the spawner clock, the dash count and the
jump against the battle; this one reads the buff percents: a raged Knight walks and swings faster than 100, and a
frozen crown tower's attack clock reads 0.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
SLOTS = [[0, 1, 2], [0, 1, 2]]
CARDS = ["Knight", "Rage", "Freeze", "Archer", "Giant", "Zap", "Musketeer", "Valkyrie"]
F = {name: k for k, name in enumerate(royalesim.ENTITY_FIELDS)}
X = {name: len(royalesim.ENTITY_FIELDS) + k for k, name in enumerate(royalesim.ENTITY_EXTRA_FIELDS)}


def battle() -> "royalesim.Battle":
    b = royalesim.Battle(card_names=CARDS, slot_of_k=SLOTS)
    ids = list(range(len(CARDS)))
    b.reset(9, [ids, ids], 0, 200, [10000, 10000], None, [], [[0] * 8, [0] * 8])
    return b


def test_the_extra_columns_are_named_and_opt_in():
    assert list(royalesim.ENTITY_EXTRA_FIELDS) == [
        "spawn_ms", "spawn_wave_left", "dash_state", "dash_ticks", "speed_pct", "hit_speed_pct", "jumping",
    ]
    b = battle()
    b.step([], 30)
    assert b.state_json() == b.state_json(extra=False)
    plain = json.loads(b.state_json())["entities"]
    extra = json.loads(b.state_json(extra=True))["entities"]
    assert plain
    assert len(plain) == len(extra)
    for p, x in zip(plain, extra, strict=True):
        assert len(p) == len(royalesim.ENTITY_FIELDS)
        assert x[: len(p)] == p
        assert len(x) == len(royalesim.ENTITY_FIELDS) + len(royalesim.ENTITY_EXTRA_FIELDS)


def play(b, name: str, x: int, y: int) -> bool:
    """Play `name` for blue at (x, y) millitiles if it is in hand and affordable; True if the engine took it."""
    s = json.loads(b.state_json())["players"][0]
    for k, cid in enumerate(s["hand"]):
        if CARDS[cid] == name and 0 <= s["hand_costs"][k] <= s["elixir_milli"] // 1000:
            return b.step([(0, k, x * SUB, y * SUB)], 1)[0][1] == 0
    return False


def test_rage_and_freeze_read_in_the_percents():
    b = battle()
    raged = frozen = False
    for _ in range(1200):
        rows = json.loads(b.state_json(extra=True))["entities"]
        knights = [r for r in rows if r[F["team"]] == 0 and r[F["card_id"]] == CARDS.index("Knight")]
        if not knights:
            play(b, "Knight", 9000, 8000) or b.step([], 2)
            continue
        for r in rows:
            names = {m for n, _ in r[F["buffs"]] for m in n.split("|")}
            if r in knights and "Rage" in names:
                assert r[X["speed_pct"]] > 100, r
                assert r[X["hit_speed_pct"]] > 100, r
                raged = True
            if r[F["tower_slot"]] >= 0 and r[F["team"]] == 1 and "Freeze" in names:
                assert r[X["hit_speed_pct"]] == 0, r
                frozen = True
        if raged and frozen:
            break
        k = knights[0]
        if not raged and play(b, "Rage", k[F["x"]] // SUB, k[F["y"]] // SUB):
            continue
        if not frozen and play(b, "Freeze", 3500, 25500):
            continue
        b.step([], 2)
    assert raged, "no raged Knight was read"
    assert frozen, "no frozen red tower was read"
