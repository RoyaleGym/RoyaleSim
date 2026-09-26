"""A direct hit on a target more than LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE beyond reach lands for no damage; the
swing and the target are kept (combat.HIT_BEYOND_CANCEL_RANGE).

WHAT THIS PINS. On client 15.535.29 a single-target direct striker whose target runs out of its reach mid-swing, with
no nearer enemy, keeps the target and completes the swing on its running cycle. The hit deals its damage when the target
stood within reach + 1500 at the start of the hit tick (up to 1149 beyond measured) and none beyond (1507 and 1970
measured; a Battle Ram 2697 beyond in the 15.535.29 catalogue scenario). Over the client 15.535.29 scenarios, 83 of 83
completed direct hits on a target beyond reach fit. Today's engine instead cancels the windup when the target passes
reach + 1500 (the windup lock), and with the lock off it lands the hit for full damage wherever the target stands.

THE SIBLING KEY. A direct striker's windup lock is targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED, proposed at
projectile_attackers_only (item-reach-loss-switch). The new arms here are the pair, because under today's lock the swing
never completes that far out.

WHY THE CONTROL IS HERE. A hit on a target beyond reach but within reach + 1500 deals full damage on both arm pairs, so
an implementation that zeroes every hit beyond reach fails.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `hit_beyond_cancel_deals_damage` -- the far hit deals its full damage under the new arm too:
    test_a_hit_beyond_the_cancel_range_lands_for_no_damage.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
LOCK_KEY, KEY = "targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED", "combat.HIT_BEYOND_CANCEL_RANGE"
KEYS = (LOCK_KEY, KEY)
NEW = ("projectile_attackers_only", "no_damage")
OLD = (True, "damage")
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the Knight's reach on a Hog Rider (Range 1200, radii 500 and 600) and the cancel range
REACH_ON_HOG, CANCEL = 2300, 1500
KNIGHT_HIT, KNIGHT_PERIOD = 202, 24
#: a red Knight at the red end of the right bridge; a blue Hog Rider crossing it and running on to the red tower
FAR = ((13500, 18000), (14500, 12000))
#: the same Knight further from the bridge: its second hit lands just beyond reach
NEAR = ((13300, 17600), (14500, 13500))


def overrides(arms) -> dict:
    return {LOCK_KEY: json.dumps(arms[0]), KEY: json.dumps(arms[1])}


def hits(arms, scene, ticks=90):
    """(tick, the Knight still targets the Hog, distance beyond reach at the start of the tick, damage to the Hog) for
    every tick the Knight's attack phase shows a hit."""
    (kx, ky), (hx, hy) = scene
    b = royalesim.Battle(["Knight", "HogRider"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arms))
    b.reset(
        0,
        [[0] * 8, [1] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(1, 0, kx * SUB, ky * SUB, -1), (0, 1, hx * SUB, hy * SUB, -1)],
    )
    prev = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    k = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    h = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    out = []
    for t in range(1, ticks + 1):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        if k not in now or h not in now:
            break
        if now[k][F["attack_phase"]] == 2:
            dx, dy = prev[h][F["x"]] - prev[k][F["x"]], prev[h][F["y"]] - prev[k][F["y"]]
            beyond = (dx * dx + dy * dy) ** 0.5 / SUB - REACH_ON_HOG
            out.append((t, now[k][F["target_uid"]] == h, round(beyond), prev[h][F["hp"]] - now[h][F["hp"]]))
        prev = now
    return out


def test_a_hit_beyond_the_cancel_range_lands_for_no_damage():
    rows = hits(NEW, FAR)
    near = [t for t, _, beyond, _ in rows if beyond <= CANCEL]
    assert len(near) >= 2, f"the scene drifted: hits {rows}"
    due = max(near) + KNIGHT_PERIOD
    far = [r for r in rows if r[0] == due]
    assert far, f"no hit on {due}, the running cycle after the hit on {max(near)}: the swing did not complete; {rows}"
    t, kept, beyond, dmg = far[0]
    assert beyond > CANCEL, f"the scene drifted: the hit on {t} was only {beyond} beyond reach"
    assert kept, f"the Knight let go of the Hog before its hit on {t}"
    assert dmg == 0, f"the hit on {t}, {beyond} beyond reach, dealt {dmg}"


@pytest.mark.parametrize("arms", [NEW, OLD])
def test_a_hit_within_the_cancel_range_deals_full_damage(arms):
    rows = hits(arms, NEAR, ticks=60)
    beyond_reach = [r for r in rows if 0 < r[2] <= CANCEL]
    assert beyond_reach, f"the scene drifted: no hit beyond reach within the cancel range; hits {rows}"
    short = [r for r in beyond_reach if r[3] != KNIGHT_HIT]
    assert short == [], f"(tick, kept, beyond, damage): {short}"


def test_old_arms_are_todays_engine():
    rows = hits(OLD, FAR)
    near = [t for t, _, beyond, _ in rows if beyond <= CANCEL]
    due = max(near) + KNIGHT_PERIOD
    assert all(t < due for t, *_ in rows), f"old arms: a hit on or after {due}: {rows}"
