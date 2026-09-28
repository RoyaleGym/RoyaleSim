"""A walking Inferno Dragon stops only inside Range + the target's radius (targeting.VARIABLE_DAMAGE_WALK_REACH).

WHAT THIS PINS. On the 16.402 corpus and on client 15.535.29 an Inferno Dragon (Range 3500, CollisionRadius 500) that
walks after a target stops, and starts its attack, on the first tick that starts with the target's centre within
Range + the TARGET's radius. Inside Range + both radii but outside that, it walks on: 31 corpus ticks and 16 sweep
ticks, none stood. Walkers of every other card stand there (979 corpus and 235 sweep ticks). Once it stands, its reach
is Range + both radii again: with the target in the band it held its place on 42 + 17 ticks, and its attack gate let go
only past Range + both radii. Today's engine walks every unit to Range + both radii, so its Inferno Dragon stops 500
short and starts to burn early. The arm reads a flying row that sets VariableDamage2, the Inferno Dragon's alone in the
15.535.29 tables; crates/royalesim/tests/variable_damage_walk_reach.rs pins that the Mighty Miner, a ground row with the
column and unmeasured, keeps its own radius.

The scene is the client 15.535.29 sweep's: a blue Inferno Dragon at (9500, 11500) and a red Knight (radius 500) walking
down the right lane from (14000, 16500). A Baby Dragon (the same Range and radius, no VariableDamage2) is the control.

WHY THE CONTROLS ARE HERE. "It stops inside Range + the target's radius" also passes for an engine that shortens every
flyer's reach, so the Baby Dragon must still stop in the band under the new arm. And "it stands in the band once
attacking" is what separates the measured law from one that uses the short reach always: the Knight walks on past
the Dragon, so a standing Dragon meets the band from inside. The old arm must be today's engine.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then
run this file: the named tests go red and the rest stay green.
  * `walk_reach_keeps_own_radius` -- the new arm still adds the walker's own radius:
    test_a_walking_inferno_dragon_stops_only_inside_range_plus_the_target_radius.
"""

from __future__ import annotations

import json
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.VARIABLE_DAMAGE_WALK_REACH"
NEW_ARM, OLD_ARM = "client16402_no_own_radius_walking", "range_plus_both_radii"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the sweep scene: the Dragon in its own half, the Knight walking down the right lane to the blue right tower
DRAGON_AT, KNIGHT_AT = (9500, 11500), (14000, 16500)
#: the Inferno Dragon's and the Baby Dragon's Range (cards.json), and both radii (checked on the entities below)
RANGE, OWN_R, KNIGHT_R = 3500, 500, 500
SHORT, LONG = RANGE + KNIGHT_R, RANGE + OWN_R + KNIGHT_R
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def overrides(arm):
    """The calibration overrides for `arm` (None: the build's own value)."""
    return {KEY: json.dumps(arm)} if arm is not None else {}


def walk(arm, flyer="InfernoDragon", ticks=150):
    """Per tick, from the first: (tick, the Knight's START-of-tick centre distance from the flyer, whether the flyer
    moved on the tick, whether it targets the Knight after it). Stops when either unit is gone."""
    b = royalesim.Battle([flyer, "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[0] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(0, 0, DRAGON_AT[0] * SUB, DRAGON_AT[1] * SUB, -1), (1, 1, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB, -1)],
    )

    def entities():
        return {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}

    now = entities()
    dragon = next(u for u, e in now.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    knight = next(u for u, e in now.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    assert (now[dragon][F["radius"]], now[knight][F["radius"]]) == (OWN_R * SUB, KNIGHT_R * SUB), (
        "the card data moved: the radii are not 500 and 500"
    )
    rows = []
    for t in range(1, ticks + 1):
        prev = now
        b.step([], 1)
        now = entities()
        if dragon not in now or knight not in now:
            break
        a, k = prev[dragon], prev[knight]
        d = ((a[F["x"]] - k[F["x"]]) ** 2 + (a[F["y"]] - k[F["y"]]) ** 2) ** 0.5 / SUB
        moved = (now[dragon][F["x"]], now[dragon][F["y"]]) != (a[F["x"]], a[F["y"]])
        rows.append((t, d, moved, now[dragon][F["target_uid"]] == knight))
    return rows


def first_stand(rows, what):
    """Index of the first tick the flyer stands with the Knight as its target, after walking to it."""
    chase = [i for i, r in enumerate(rows) if r[3]]
    assert chase, f"{what}: the scene drifted: the flyer never targeted the Knight"
    i = next((i for i in chase if not rows[i][2]), None)
    assert i is not None, f"{what}: the scene drifted: the flyer never stood with the Knight as its target"
    assert rows[chase[0]][1] > LONG, f"{what}: the scene drifted: the Knight was taken already inside the reach"
    return i


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_walking_inferno_dragon_stops_only_inside_range_plus_the_target_radius(arm):
    rows = walk(arm)
    i = first_stand(rows, arm)
    band = [r for r in rows[: i + 1] if r[3] and SHORT < r[1] <= LONG]
    assert band, f"{arm}: the scene drifted: no tick of the chase started with the Knight inside Range + both radii"
    t, d, _, _ = rows[i]
    assert d <= SHORT, f"{arm}: on {t} the Dragon stopped {d:.1f} from the Knight, outside Range + its radius ({SHORT})"
    stood = [r for r in rows[:i] if r[3] and r[1] > SHORT and not r[2]]
    assert not stood, f"{arm}: the Dragon stood outside Range + the Knight's radius ({SHORT}): {stood[:3]}"


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_standing_inferno_dragon_keeps_its_place_inside_both_radii(arm):
    rows = walk(arm)
    i = first_stand(rows, arm)
    after = [r for j, r in enumerate(rows) if j > i and r[3] and not rows[j - 1][2] and r[1] <= LONG]
    band = [r for r in after if r[1] > SHORT]
    assert band, f"{arm}: the scene drifted: the Knight never stood in the band from a standing Dragon"
    walked = [r for r in after if r[2]]
    assert not walked, (
        f"{arm}: a standing Dragon walked with the Knight inside Range + both radii ({LONG}): {walked[:3]}"
    )


@pytest.mark.parametrize("arm", [OLD_ARM])
def test_the_old_arm_stops_inside_both_radii(arm):
    rows = walk(arm)
    t, d, _, _ = rows[first_stand(rows, arm)]
    assert SHORT < d <= LONG, f"{arm}: on {t} the Dragon stopped {d:.1f} from the Knight, not in ({SHORT}, {LONG}]"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_baby_dragon_stops_inside_both_radii(arm):
    rows = walk(arm, flyer="BabyDragon")
    t, d, _, _ = rows[first_stand(rows, arm)]
    assert SHORT < d <= LONG, f"{arm}: on {t} the Baby Dragon stopped {d:.1f} from the Knight, not in ({SHORT}, {LONG}]"


def test_the_shipped_value_is_the_new_arm():
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["targeting"]["VARIABLE_DAMAGE_WALK_REACH"]
    assert entry["value"] == NEW_ARM
    assert entry["candidates"] == [OLD_ARM, NEW_ARM]
