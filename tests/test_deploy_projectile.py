"""A Mega Knight lands with a blow: its deploy projectile hits around it 6 ticks after it appears
(combat.DEPLOY_PROJECTILE).

WHAT THIS PINS. The Mega Knight's card carries a deploy projectile, MegaKnightAppear (168 at level 1, 430 at level 11;
radius 2200; pushback 1000; ground only). On client 15.535.29, both sides, a Mega Knight deployed 560 from a walking
Knight takes 430 off it on the 6th tick after its first frame, and the Knight then slides 199, 174, 149, 125, 100, 74,
49, 25 away: the knockback ladder the engine's spells already run. Goblins 5,300 away are untouched. The engine carried
the projectile in cards.json and never fired it.

TWO ARMS LAND IT. client_on_landing and client_on_landing_action_at_2, which ships since the 2026-09-28 round 9 flip,
land a PLAYED unit's blow alike; the second moves only the blow of a unit an action makes, the Hero Musketeer's turret
(crates/royalesim/tests/hero_turret.rs). So the Mega Knight's blow is pinned under both.

THE SCENE IS THE CLIENT'S. The Knight stands where it stood when the Mega Knight was played, a little off his y, so the
blow pushes it on a slant: each step is cut to whole units on each axis, and a step of 200 shows as 199. Level with him
(the same y), the same ladder moves it exactly 200, 175, 150, and so on.

WHY THE CONTROLS ARE HERE. A blow anywhere on the board also passes "the Knight loses 430", so a second enemy outside
the radius must lose nothing on that tick; and not_read, the engine before the 2026-09-26 flip, must land no blow, the
Knight losing nothing until the Mega Knight's first swing.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `deploy_projectile_unfired` -- no arm fires a deploy blow, as not_read does:
    test_the_mega_knight_lands_with_a_blow, under both arms.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.DEPLOY_PROJECTILE"
#: the arms that land a played unit's blow; the second ships
PLAYED_ARMS = ("client_on_landing", "client_on_landing_action_at_2")
NOT_READ = "not_read"
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y, 7 hp
TEAM, CARD, SLOT, X, Y, HP = 1, 3, 4, 5, 6, 7
MK, KN, GOB = 0, 1, 2
BLOW, DELAY = 430, 6
#: the recorded Knight's steps on the 8 ticks after the blow (client 15.535.29, side 0)
SLIDES = [199, 174, 149, 125, 100, 74, 49, 25]


def run(arm: str) -> dict:
    """Blue plays a Mega Knight at (14600, 13489), next to a red Knight standing at (14189, 13535), with a red Goblins
    group far away at (4000, 13500): the client scene's positions on the tick of the play. Returns the Mega Knight's
    first tick, and per tick the red units' hp drops and the Knight's step length."""
    deck = ["MegaKnight", "Knight", "Goblins"]
    b = royalesim.Battle(deck, [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    units = [(1, KN, 14189 * SUB, 13535 * SUB, -1), (1, GOB, 4000 * SUB, 13500 * SUB, -1)]
    b.reset(0, [[0, 1, 2, 0, 1, 2, 0, 1]] * 2, 0, 200, [10_000, 10_000], None, units)
    played = b.step([(0, MK, 14600 * SUB, 13489 * SUB)], 1)
    assert played, "the Mega Knight play returned nothing"
    assert played[0][1] == 0, f"the Mega Knight was refused: {played}"
    first, last, pos, rows = None, {}, None, {}
    for _ in range(40):
        s = json.loads(b.state_json())
        if first is None and any(e[TEAM] == 0 and e[SLOT] < 0 and e[CARD] == MK for e in s["entities"]):
            first = s["tick"]
        row = {}
        for e in s["entities"]:
            if e[TEAM] == 1 and e[SLOT] < 0:
                name = deck[e[CARD]]
                if (e[0]) in last and e[HP] < last[e[0]]:
                    row[name] = row.get(name, 0) + last[e[0]] - e[HP]
                last[e[0]] = e[HP]
                if e[CARD] == KN:
                    p = (e[X] // SUB, e[Y] // SUB)
                    if pos is not None:
                        row["knight_step"] = round(((p[0] - pos[0]) ** 2 + (p[1] - pos[1]) ** 2) ** 0.5)
                    pos = p
        rows[s["tick"]] = row
        b.step([], 1)
    return {"first": first, "rows": rows}


@pytest.mark.parametrize("arm", PLAYED_ARMS)
def test_the_mega_knight_lands_with_a_blow(arm):
    r = run(arm)
    assert r["first"] is not None, "the Mega Knight never appeared"
    hit = r["rows"].get(r["first"] + DELAY, {})
    assert hit.get("Knight") == BLOW, r["rows"]
    assert "Goblins" not in hit, "a Goblin outside the radius was hit"
    after = [r["rows"].get(r["first"] + DELAY + k, {}).get("knight_step") for k in range(1, len(SLIDES) + 1)]
    assert after == SLIDES, f"the Knight's steps after the blow: {after}, recorded {SLIDES}"


def test_not_read_lands_no_blow():
    r = run(NOT_READ)
    assert r["first"] is not None
    assert all(r["rows"].get(t, {}).get("Knight") != BLOW for t in range(r["first"], r["first"] + 12)), r["rows"]
