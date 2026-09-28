"""A King Tower's shot is born ProjectileYOffset further along its own forward y (combat.PROJECTILE_Y_OFFSET).

WHAT THIS PINS. On the 16.402 corpus every king-tower shot's first frame sits 400 past the point
combat.PROJECTILE_LAUNCH gives (ProjectileStartRadius 750 from the centre toward the target), along the owner's own
forward y: +400 for the Blue king (191 of 191 shots, one seat per battle), -400 for the Red king (222 of 222). 400 is
the King Tower row's ProjectileYOffset. Flown from there the shot lands on the client's tick (251 of 251 followed
flights); flown from the plain point, 58 of them land late. In 20260918-122757.b1 the Blue king's shot at a Knight
walking at it, fired on tick 2427, lands on 2434 in the client and on 2435 in today's engine.

THE SCENES. The king is made to shoot by destroying its own princess towers before the battle (the king is then
already active). A: a Blue Knight walks up the middle at the Red king. B: the mirror, a Red Knight at the Blue king.
C (the null): with the towers standing, a Blue Knight walks up the left lane and the Red left princess tower shoots it;
its row sets no ProjectileYOffset, so its shot sits on the plain point under both arms. D: in scene A the first king
shot lands one tick earlier under the new arm, the Knight having walked the same ticks in both runs.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `projectile_y_offset_unread` -- the loader drops the column, so the new arm moves nothing:
    test_a_king_shot_is_born_past_the_start_radius_point, test_the_moved_start_lands_the_first_hit_a_tick_earlier.
  * `projectile_y_offset_arena_frame` -- the offset goes up the arena for both seats:
    test_a_king_shot_is_born_past_the_start_radius_point[blue-knight-at-red-king].
"""

from __future__ import annotations

import itertools
import json
import math
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.PROJECTILE_Y_OFFSET"
NEW_ARM, OLD_ARM = "client_forward_y", "not_read"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
KING_R, PRINCESS_R, Y_OFFSET = 750, 300, 400  # the rows' ProjectileStartRadius and the King Tower's ProjectileYOffset
FULL = [4824, 3052, 3052]
#: (name, the Knight's team, where it starts, the tower hp per team, the shooter's team, its centre, its radius)
SCENES = {
    "blue-knight-at-red-king": (0, (9000, 20000), [FULL, [4824, 0, 0]], 1, (9000, 29000), KING_R),
    "red-knight-at-blue-king": (1, (9000, 12000), [[4824, 0, 0], FULL], 0, (9000, 3000), KING_R),
    "blue-knight-at-red-princess": (0, (3500, 17000), [FULL, FULL], 1, (3500, 25500), PRINCESS_R),
}
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def trunc_div(a, b):
    """Integer division truncated toward zero (b > 0), as the engine's i128 arithmetic."""
    return a // b if a >= 0 else -((-a) // b)


def in_frame(v, team):
    """A subtile coordinate on the native grid in `team`'s frame (path.rs `native_in_frame`): floor for Blue, ceil for
    Red."""
    return v // SUB if team == 0 else -((-v) // SUB)


def plain_point(centre, aim, team, radius):
    """combat.PROJECTILE_LAUNCH's point, native: the centre + trunc0(v * R / isqrt(v.v)), v = aim - centre."""
    sx, sy = centre
    vx, vy = in_frame(aim[0], team) - sx, in_frame(aim[1], team) - sy
    n = math.isqrt(vx * vx + vy * vy)
    return sx + trunc_div(vx * radius, n), sy + trunc_div(vy * radius, n)


def scene(arm, name, ticks=160):
    """Per tick: (tick, the Knight's position and hp, the shooter team's projectiles as (x, y, aim_x, aim_y) in
    subtiles). `arm` None runs the build's own value."""
    team, at, tower_hp, shooter, _, _ = SCENES[name]
    overrides = {KEY: json.dumps(arm)} if arm is not None else {}
    b = royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(2, [[0] * 8, [0] * 8], 0, 100, [10_000, 10_000], tower_hp, [(team, 0, at[0] * SUB, at[1] * SUB, -1)])
    rows = []
    for _ in range(ticks):
        b.step([], 1)
        s = json.loads(b.state_json())
        knights = [e for e in s["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == team]
        assert len(knights) == 1, f"{name}: the scene drifted: {len(knights)} troops on the Knight's side"
        k = knights[0]
        mine = [p for p in s.get("projectiles", []) if p[P["team"]] == shooter]
        shots = [(p[P["x"]], p[P["y"]], p[P["aim_x"]], p[P["aim_y"]]) for p in mine]
        rows.append((s["tick"], (k[F["x"]], k[F["y"]]), k[F["hp"]], shots))
    return rows


def first_shot(rows, name):
    """(tick, position, aim) of the shooter's first projectile, on the tick it first appears."""
    for tick, _, _, shots in rows:
        if shots:
            assert len(shots) == 1, f"{name}: the scene drifted: {len(shots)} shots on the first tick"
            x, y, ax, ay = shots[0]
            return tick, (x, y), (ax, ay)
    raise AssertionError(f"{name}: the scene drifted: the tower never shot")


def first_hit(rows):
    for (_, _, hp0, _), (tick, _, hp1, _) in itertools.pairwise(rows):
        if hp1 < hp0:
            return tick
    raise AssertionError("the Knight was never hit")


def expected_birth(rows, name, offset):
    _, _, _, shooter, centre, radius = SCENES[name]
    _, pos, aim = first_shot(rows, name)
    px, py = plain_point(centre, aim, shooter, radius)
    forward = 1 if shooter == 0 else -1
    return pos, (px * SUB, (py + forward * offset) * SUB)


@pytest.mark.parametrize("name", ["blue-knight-at-red-king", "red-knight-at-blue-king"])
def test_a_king_shot_is_born_past_the_start_radius_point(name):
    rows = scene(NEW_ARM, name)
    got, want = expected_birth(rows, name, Y_OFFSET)
    assert got == want, (
        f"{NEW_ARM}, {name}: the king's first shot was born at ({got[0] / SUB:.0f}, {got[1] / SUB:.0f}), want "
        f"({want[0] / SUB:.0f}, {want[1] / SUB:.0f}): the ProjectileStartRadius point plus {Y_OFFSET} along the "
        "king's own forward y"
    )


@pytest.mark.parametrize("name", ["blue-knight-at-red-king", "red-knight-at-blue-king"])
def test_the_old_arm_bears_a_king_shot_at_the_start_radius_point(name):
    rows = scene(OLD_ARM, name)
    got, want = expected_birth(rows, name, 0)
    assert got == want, f"{OLD_ARM}, {name}: the king's first shot was born at {got}, want the plain point {want}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_princess_shot_is_born_at_the_start_radius_point(arm):
    name = "blue-knight-at-red-princess"
    rows = scene(arm, name)
    got, want = expected_birth(rows, name, 0)
    assert got == want, f"{arm}: the princess tower's first shot was born at {got}, want the plain point {want}"


def test_the_moved_start_lands_the_first_hit_a_tick_earlier():
    name = "blue-knight-at-red-king"
    new, old = scene(NEW_ARM, name), scene(OLD_ARM, name)
    t_new, t_old = first_hit(new), first_hit(old)
    walked = [r[0] for r, q in zip(new, old, strict=True) if r[0] < t_old and r[1] != q[1]]
    assert not walked, f"the scene drifted: the Knight walked differently under the two arms on {walked[:3]}"
    assert t_new == t_old - 1, (
        f"the first king shot landed on {t_new} under {NEW_ARM} and on {t_old} under {OLD_ARM}; the shot born "
        f"{Y_OFFSET} nearer should land one tick earlier"
    )


def test_the_shipped_value_is_the_new_arm():
    ledger = json.loads(LEDGER.read_text(encoding="utf-8"))
    entry = ledger["combat"]["PROJECTILE_Y_OFFSET"]
    assert entry["value"] == NEW_ARM
    assert set(entry["candidates"]) == {OLD_ARM, NEW_ARM}
