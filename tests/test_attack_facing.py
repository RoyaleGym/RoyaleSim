"""An attacking unit faces its target on every attack tick (movement.ATTACK_FACING).

WHAT THIS PINS. On client 15.535.29 a unit in its attack state has, on every tick, the facing (its movement
direction, length 256) that points from its start-of-tick position at its target's start-of-tick position, by the
client's integer normalize: tdiv(v * 256, isqrt(|v|^2)). Measured over the per-frame records of 538 client 15.535.29
scenarios: exact on 73,620 of the 75,326 attack frames, and on 66,367 of the 68,073 frames where it differs from the
heading of the unit's last walking frame, which is exact on none. Every miss is the Ram Rider's (its attack state
includes its charge). The facing matters when the attack ends: the first walking tick's avoidance look-ahead probes
256 ahead along it. Today's engine keeps the last walking heading through the attack.

WHY THE CONTROL IS HERE. A WALKING unit faces along its path, not at its target: a Knight whose target, a Cannon,
stands across the river walks to a bridge and faces along that route. That holds on both arms, so an implementation
that points every unit with a target at it fails.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "movement.ATTACK_FACING"
NEW_ARM, OLD_ARM = "toward_target", "kept"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: a red Knight standing on the red half and a blue Hog Rider running north past it (no river on its way)
KNIGHT_AT, HOG_AT = (13300, 21000), (14500, 19500)
#: a red Knight near the river and, across it, a blue Cannon in its sight: the Knight walks round by a bridge
WALKER_AT, CANNON_AT = (8000, 18500), (9000, 13500)
#: a step direction and a target direction that part by more than this per component are a discriminating tick; the
#: facing must then lie within PATH_TOLERANCE of the step direction (a 60-native step normalizes to within about 4)
SPLIT, PATH_TOLERANCE = 40, 8


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def tdiv(a: int, b: int) -> int:
    q = abs(a) // abs(b)
    return q if (a < 0) == (b < 0) else -q


def normalize256(dx: int, dy: int) -> tuple[int, int]:
    """The client's integer normalize to length 256 (an isqrt of the squared length, divisions toward zero)."""
    n = int((dx * dx + dy * dy) ** 0.5)
    while n * n > dx * dx + dy * dy:
        n -= 1
    while (n + 1) * (n + 1) <= dx * dx + dy * dy:
        n += 1
    return tdiv(dx * 256, n), tdiv(dy * 256, n)


def native(e) -> tuple[int, int]:
    return e[F["x"]] // SUB, e[F["y"]] // SUB


def run(arm, spawns, cards, ticks):
    """Per tick: every unit's entity row, keyed by the card it came from."""
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [len(cards) - 1] * 8], 0, 200, [10_000, 10_000], None,
            [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns])
    rows = [{e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}]
    for _ in range(ticks):
        b.step([], 1)
        rows.append({e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]})
    uid = {cards[e[F["card_id"]]]: u for u, e in rows[0].items() if e[F["tower_slot"]] < 0}
    return rows, uid


def attack_ticks(arm):
    """(tick, facing, expected facing) for every tick the Knight is attacking the Hog."""
    rows, uid = run(arm, [(1, 0, *KNIGHT_AT, -1), (0, 1, *HOG_AT, -1)], ["Knight", "HogRider"], 40)
    k, h = uid["Knight"], uid["HogRider"]
    out = []
    for t in range(1, len(rows)):
        now, before = rows[t], rows[t - 1]
        if k not in now or h not in now or k not in before or h not in before:
            break
        me = now[k]
        if me[F["attack_phase"]] == 0 or me[F["target_uid"]] != h:
            continue
        (kx, ky), (hx, hy) = native(before[k]), native(before[h])
        out.append((t, tuple(me[F["facing"]]), normalize256(hx - kx, hy - ky)))
    return out


def test_an_attacking_unit_faces_its_target_every_tick():
    rows = attack_ticks(NEW_ARM)
    assert len(rows) >= 10, f"the scene drifted: the Knight attacked on {len(rows)} ticks"
    assert len({want for _, _, want in rows}) >= 5, "the scene drifted: the Hog did not move across the Knight's front"
    wrong = [(t, got, want) for t, got, want in rows if got != want]
    assert wrong == [], f"(tick, facing, toward the Hog at the start of the tick): {wrong[:5]}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_walking_unit_faces_along_its_path_not_at_its_target(arm):
    rows, uid = run(arm, [(1, 0, *WALKER_AT, -1), (0, 1, *CANNON_AT, -1)], ["Knight", "Cannon"], 150)
    k = uid["Knight"]
    split = []
    for t in range(1, len(rows)):
        me, before = rows[t].get(k), rows[t - 1].get(k)
        if me is None or before is None or me[F["attack_phase"]] != 0 or me[F["target_uid"]] < 0:
            continue
        tg = rows[t][me[F["target_uid"]]]
        (x0, y0), (x1, y1), (tx, ty) = native(before), native(me), native(tg)
        if (x1, y1) == (x0, y0):
            continue
        facing, step = tuple(me[F["facing"]]), normalize256(x1 - x0, y1 - y0)
        toward = normalize256(tx - x0, ty - y0)
        if max(abs(a - b) for a, b in zip(step, toward, strict=True)) > SPLIT:
            split.append((t, facing, step, toward))
    assert len(split) >= 50, f"the scene drifted: {len(split)} walking ticks where path and target part"
    off_path = [r for r in split if max(abs(a - b) for a, b in zip(r[1], r[2], strict=True)) > PATH_TOLERANCE]
    assert off_path == [], f"(tick, facing, step direction, target direction): {off_path[:5]}"


def test_old_arm_is_todays_engine():
    rows = attack_ticks(OLD_ARM)
    assert len(rows) >= 10, f"the scene drifted: the Knight attacked on {len(rows)} ticks"
    facings = {got for _, got, _ in rows}
    assert facings == {(0, -256)}, f"old arm: the Knight's facing moved while it attacked: {sorted(facings)}"
