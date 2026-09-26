"""The Princess's volley does damage: its first projectile is the card's CustomFirstProjectile
(combat.CUSTOM_FIRST_PROJECTILE).

WHAT THIS PINS. The Princess fires a volley of MultipleProjectiles = 5 arrows. The first is her CustomFirstProjectile,
PrincessProjectile: 66 at level 1 (168 at level 11), radius 2000, speed 600. The other four are her Projectile column,
PrincessProjectileDeco, which has no damage. On client 15.535.29 (both sides) her first volley takes 168 off two Goblins
on the same frame, and her second kills both. The engine loaded only the Projectile column, the decoration, so its
Princess dealt no damage at all.

WHY THE CONTROL IS HERE. The old arm must be today's engine: in the 15.535.29 scenario rebuilt no Goblin ever loses 168,
only the princess towers' 109. That is also what shows the scenario puts Goblins inside her range.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `custom_first_projectile_unread` -- the new arm fires the Projectile column, the Princess's decoration:
    test_the_first_volley_hits_two_goblins.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.CUSTOM_FIRST_PROJECTILE"
NEW_ARM, OLD_ARM = "client_first_of_volley", "not_read"
# ENTITY_FIELDS: 0 uid, 1 team, 3 card_id, 4 tower_slot, 7 hp
UID, TEAM, CARD, SLOT, HP = 0, 1, 3, 4, 7
VOLLEY = 168


def goblin_drops(arm: str, ticks: int = 200) -> list:
    """The 15.535.29 princess-goblins scenario: a blue Princess tapped at (9500, 8500) and red Goblins at (9500, 18499),
    both issued on tick 100. Per tick with any Goblin drop, the list of drops."""
    b = royalesim.Battle(["Princess", "Goblins"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    b.reset(2, [[0, 1] * 4, [0, 1] * 4], 0, 0, None, None, [])
    b.step([], 100)
    played = b.step([(0, 0, 9500 * SUB, 8500 * SUB), (1, 1, 9500 * SUB, 18499 * SUB)], 1)
    assert len(played) == 2, f"a play is missing: {played}"
    assert all(p[1] == 0 for p in played), f"a play was refused: {played}"
    last, out = {}, []
    for _ in range(ticks):
        row = []
        for e in json.loads(b.state_json())["entities"]:
            if e[TEAM] == 1 and e[SLOT] < 0 and e[CARD] == 1:
                if e[UID] in last and e[HP] < last[e[UID]]:
                    row.append(last[e[UID]] - e[HP])
                last[e[UID]] = e[HP]
        if row:
            out.append(row)
        b.step([], 1)
    return out


def test_the_first_volley_hits_two_goblins():
    got = goblin_drops(NEW_ARM)
    assert any(row.count(VOLLEY) >= 2 for row in got), got


def test_the_old_arm_is_todays_engine():
    """Checked on 7bfcfd2: the Goblins lose only 109s (princess towers), never 168."""
    got = goblin_drops(OLD_ARM)
    assert got, "no Goblin lost any hp"
    assert not any(VOLLEY in row for row in got), got
