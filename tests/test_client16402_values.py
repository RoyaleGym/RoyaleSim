"""Six card values where the 16.402 client differs from the 15.535.29 tables (cards.CLIENT16402_VALUES).

WHAT THIS PINS. Read off the 16.402 corpus (every recorded unit's spawn hp and every attacker's hit damage, the mode per
unit and level, each inverted to the one integer base value that reproduces it at every level seen):
  IceSpirits Hitpoints 84 (the tables: 85): 215 at level 11, 32 of 32 spawns;
  IceGolemite Hitpoints 480 (514): 1228 at level 11, 9 of 9;
  GoblinBrawler Hitpoints 438 (422): 1121 at level 11, 6 of 6 (the GoblinCage's death spawn);
  HealSpirit Hitpoints 84 (85): 215 at level 11, 2 of 2 spawns (one battle, both seats);
  Bomber projectile Damage 83 (88): 212 at level 11 in the mode, and 132 / 100 at levels 6 / 3;
  Fireball CrownTowerDamagePercent -77 (-75): 3 of 3 crown hits at levels 3, 5 and 11.
Every other basic unit's hp and hit damage agrees with the tables. The key overrides the six at load; cards.json stays
the 15.535.29 extraction.

THE CHECKS. Each value is read through the engine at level 11, the corpus's level: a spawn's hp, the Bomber's first
hit on a red Knight, and a Fireball's damage to a red princess tower. The old arm must give the tables' values.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `card_values_unread` -- the new arm runs the tables' values: test_the_16402_value.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
#: explicit, because catalogue ids are positional; the Fireball first so it is in the opening hand
DECK = ["Fireball", "IceGolemite", "GoblinCage", "Bomber", "IceSpirits", "Knight", "Giant", "Tesla"]
IDS = list(range(len(DECK)))
FB, IG, GC, BO, IS, KN = 0, 1, 2, 3, 4, 5
#: the Heal Spirit is played, not set up (its card is a spell that puts the spirit down): a deck with it in the hand
HEAL_DECK = ["Heal", "Knight", "Giant", "Fireball", "Bomber", "IceSpirits", "IceGolemite", "Tesla"]
KEY = "cards.CLIENT16402_VALUES"
# ENTITY_FIELDS: 1 team, 2 kind, 3 card_id, 4 tower_slot, 5 x, 6 y, 7 hp
TEAM, KIND, CARD, SLOT, X, Y, HP = 1, 2, 3, 4, 5, 6, 7


def arm(name: str) -> dict:
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    value = ledger.get("cards", {}).get("CLIENT16402_VALUES", {}).get("value")
    if value is None:
        pytest.fail(f"{KEY} is not in the compiled-in ledger: this build predates the key")
    return {KEY: json.dumps({**value, "arm": name})}


def battle(overrides: dict, spawns: list) -> object:
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns])
    return b


def rows(b) -> list:
    return [e for e in json.loads(b.state_json())["entities"] if e[SLOT] < 0]


def spawn_hp(overrides: dict, card: int) -> int:
    b = battle(overrides, [(0, card, 9000, 8000, -1)])
    b.step([], 1)
    return next(e[HP] for e in rows(b) if e[TEAM] == 0)


def heal_spirit_hp(overrides: dict) -> int:
    """The Heal Spirit card played on blue's side: the spirit's hp on the first tick it stands."""
    b = royalesim.Battle(card_names=HEAL_DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    ids = list(range(len(HEAL_DECK)))
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], None, [])
    res = b.step([(0, 0, 9000 * SUB, 8000 * SUB)], 1)
    assert [r[1] for r in res] == [0], f"the Heal Spirit play was refused: {res}"
    for _ in range(60):
        units = [e for e in rows(b) if e[TEAM] == 0]
        if units:
            return units[0][HP]
        b.step([], 1)
    raise AssertionError("no Heal Spirit within 60 ticks")


def brawler_hp(overrides: dict) -> int:
    """A 3-hp GoblinCage decays at once and its Brawler comes out."""
    b = battle(overrides, [(0, GC, 9000, 8000, 3)])
    for _ in range(30):
        b.step([], 1)
        troops = [e for e in rows(b) if e[TEAM] == 0 and e[KIND] == 0]
        if troops:
            return troops[0][HP]
    raise AssertionError("no Brawler within 30 ticks")


def bomber_first_hit(overrides: dict) -> int:
    b = battle(overrides, [(0, BO, 9000, 10000, -1), (1, KN, 9000, 13500, -1)])
    prev = None
    for _ in range(120):
        b.step([], 1)
        kn = next((e for e in rows(b) if e[TEAM] == 1), None)
        if kn is None:
            break
        if prev is not None and kn[HP] < prev:
            return prev - kn[HP]
        prev = kn[HP]
    raise AssertionError("the Bomber never hit the Knight")


def fireball_on_tower(overrides: dict) -> int:
    b = battle(overrides, [])
    b.step([], 1)

    def towers():
        entities = json.loads(b.state_json())["entities"]
        return {(e[X], e[Y]): e[HP] for e in entities if e[TEAM] == 1 and e[SLOT] >= 0}

    before = towers()
    b.step([(0, FB, 3500 * SUB, 25500 * SUB)], 1)
    for _ in range(80):
        b.step([], 1)
        lost = [before[k] - v for k, v in towers().items() if v != before[k]]
        if lost:
            return lost[0]
    raise AssertionError("the Fireball never reached the tower")


CASES = [
    ("IceSpirits hp", lambda o: spawn_hp(o, IS), 215, 217),
    ("IceGolemite hp", lambda o: spawn_hp(o, IG), 1228, 1315),
    ("GoblinBrawler hp", brawler_hp, 1121, 1080),
    ("HealSpirit hp", heal_spirit_hp, 215, 217),
    ("Bomber hit", bomber_first_hit, 212, 225),
    ("Fireball on a princess tower", fireball_on_tower, 159, 172),
]


@pytest.mark.parametrize(("what", "measure", "client16402", "tables"), CASES, ids=[c[0] for c in CASES])
def test_the_16402_value(what, measure, client16402, tables):
    got = measure(arm("client16402"))
    assert got == client16402, f"{what}: {got}; the 16.402 client gives {client16402}"


@pytest.mark.parametrize(("what", "measure", "client16402", "tables"), CASES, ids=[c[0] for c in CASES])
def test_the_old_arm_is_the_tables(what, measure, client16402, tables):
    got = measure(arm("none"))
    assert got == tables, f"{what}: {got}; the 15.535.29 tables give {tables}"


@pytest.mark.parametrize("name", ["client16402", "none"])
def test_the_catalogue_reports_the_hp_a_spawn_takes(name):
    """catalogue_json's hitpoints are the ones this object's battles run, under either arm."""
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=arm(name))
    listed = {row[0]: row[6] for row in json.loads(b.catalogue_json())}
    for card in (IS, IG):
        spawned = spawn_hp(arm(name), card)
        assert listed[DECK[card]] == spawned, (
            f"{name}: the catalogue lists {DECK[card]} at {listed[DECK[card]]}, a spawn takes {spawned}"
        )
