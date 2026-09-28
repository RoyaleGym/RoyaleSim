"""A unit's first step meets the units dying on its creation tick (spawner.FIRST_STEP_DYING_BODIES).

WHAT THIS PINS. Under spawner.SPAWNED_FIRST_STEP a unit a spawner emits, and a death-spawned unit, takes its first
update on the tick it is created. On the client 16.402 corpus two units were born beside a unit that died on that same
tick (not their parent), and both met it. In 20260918-115249.b1, tick 802, a bomb killed a Tombstone and one of its
Skeletons walking beside the emission point; the four death Skeletons were pushed 150 off the dead Skeleton. In
20260919-182539, tick 491 (both seats), a Tombstone's periodic Skeleton came out beside another of its Skeletons that a
Knight's hit killed on that tick; it was turned (avoidance offset 190) and pushed off it. Each first frame is the
contact law's own answer, to the native unit, with the dying unit in the scans. Today's engine, the old arm hidden,
hides the dying unit: Reap despawns the tick's dead before a death spawn steps, and the doomed mask
(movement.DYING_UNIT_VISIBILITY) hides a unit an Attack-phase hit kills from an emission's step.

THE SCENES.
  * The death spawn: a Blue Tombstone at (9000, 8000) at 50 hp. Its first Skeleton comes out at once on the emission
    point (9000, 9500) and walks on up. On tick 102 Red plays a Zap on (9000, 8800); on the next tick it kills the
    Tombstone and that Skeleton together, the Skeleton after its move of that tick, still in contact with the emission
    point. The four death Skeletons come out on the emission point. Under hidden they stand one step past it, on the
    point the first Skeleton stood on one step earlier. Under client16402_seen the dead Skeleton's body pushes them off.
  * The emission: the same Tombstone at full hp, a Blue Knight at 1 hp on (9250, 9700) beside its emission point and a
    Red Knight on (9250, 10650) that fights it. The Red Knight's hit kills the Blue Knight on the tick the Tombstone's
    second Skeleton comes out. Under hidden the Skeleton steps as if the Knight were gone; under client16402_seen it
    steps against the Knight where it stands.
  * The control: the emission scene with the Blue Knight far down the other lane. Nothing dies beside the Skeleton, and
    both arms give it the same first frame.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `first_step_dying_hidden` -- client16402_seen still hides the dying units from a first update:
    test_a_death_spawn_steps_off_a_unit_dying_with_its_parent, test_an_emission_steps_off_a_unit_doomed_on_its_tick.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spawner.FIRST_STEP_DYING_BODIES"
NEW_ARM, OLD_ARM = "client16402_seen", "hidden"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
TOMB, EMISSION = (9000, 8000), (9000, 9500)
ZAP_AT, ZAP_ON = (9000, 8800), 102
BLUE_KNIGHT, RED_KNIGHT, FAR_KNIGHT = (9250, 9700), (9250, 10650), (3500, 9700)


def overrides(arm) -> dict:
    """`arm` None runs the build's own value."""
    return {KEY: json.dumps(arm)} if arm is not None else {}


def blue_units(b) -> dict:
    s = json.loads(b.state_json())
    units = {e[F["uid"]]: e for e in s["entities"] if e[F["team"]] == 0 and e[F["tower_slot"]] < 0}
    return s["tick"], units


def at(e) -> tuple:
    return e[F["x"]] // SUB, e[F["y"]] // SUB


def death_scene(arm):
    """The four death Skeletons' points on their first frame (a set), and the first Skeleton's position on the frame
    before, the last on which it was seen."""
    b = royalesim.Battle(["Tombstone", "Zap"], [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [1] * 8], 0, 100, [10_000, 10_000], None, [(0, 0, TOMB[0] * SUB, TOMB[1] * SUB, 50)])
    _, before = blue_units(b)
    assert len(before) == 1, "the scene drifted: the Tombstone is not alone"
    tomb = next(iter(before))
    for _ in range(12):
        tick = json.loads(b.state_json())["tick"]
        cmds = [(1, 0, ZAP_AT[0] * SUB, ZAP_AT[1] * SUB)] if tick == ZAP_ON else []
        played = b.step(cmds, 1)
        if cmds:
            assert played, "the scene drifted: the Zap play returned nothing"
            assert played[0][1] == 0, f"the scene drifted: the Zap was refused: {played}"
        _, now = blue_units(b)
        new = [e for u, e in now.items() if u not in before]
        if len(new) == 4:
            assert tomb not in now, "the scene drifted: four units came out and the Tombstone lives"
            gone = [e for u, e in before.items() if u not in now and u != tomb]
            assert len(gone) == 1, f"the scene drifted: {len(gone)} Skeletons died with the Tombstone, want 1"
            return {at(e) for e in new}, at(gone[0])
        before = now
    raise AssertionError("the scene drifted: the Tombstone did not die with four Skeletons within 12 ticks")


def test_a_death_spawn_steps_off_a_unit_dying_with_its_parent():
    old, dead = death_scene(OLD_ARM)
    new, dead_new = death_scene(NEW_ARM)
    assert dead == dead_new, (
        f"the scene drifted: the Skeleton died from {dead} under one arm, {dead_new} under the other"
    )
    assert len(new) == 1, f"the four death Skeletons do not share one point on their first frame: {sorted(new)}"
    (o,), (n,) = old, new
    assert math.dist(n, o) >= 100, (
        f"the death spawn stands at {n}, {math.dist(n, o):.0f} from where it stands under hidden ({o}); the client "
        "pushes it off the Skeleton that died on its tick, by the contact cap of 150"
    )
    assert math.dist(n, dead) >= math.dist(o, dead) + 100, (
        f"the death spawn stands {math.dist(n, dead):.0f} from the dying Skeleton's last position {dead} "
        f"({math.dist(o, dead):.0f} under hidden): it was not pushed off it"
    )


def test_the_old_arm_steps_as_if_the_dying_unit_were_gone():
    old, dead = death_scene(OLD_ARM)
    assert len(old) == 1, f"the four death Skeletons do not share one point: {sorted(old)}"
    (o,) = old
    moved = math.dist(o, EMISSION)
    assert 60 <= moved <= 100, f"under hidden the death spawn stands {moved:.0f} from the emission point, not one step"
    assert math.dist(dead, EMISSION) < 1000, (
        "the scene drifted: the dying Skeleton is out of contact of the emission point"
    )


def emission_scene(arm, knight=BLUE_KNIGHT):
    """(the tick the Blue Knight died or None, the tick the Tombstone's second Skeleton came out, its first-frame point,
    the Blue Knight's last position)."""
    b = royalesim.Battle(["Tombstone", "Knight"], [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides(arm))
    spawns = [
        (0, 0, TOMB[0] * SUB, TOMB[1] * SUB, -1),
        (0, 1, knight[0] * SUB, knight[1] * SUB, 1),
        (1, 1, RED_KNIGHT[0] * SUB, RED_KNIGHT[1] * SUB, -1),
    ]
    b.reset(0, [[0] * 8, [1] * 8], 0, 100, [10_000, 10_000], None, spawns)
    _, before = blue_units(b)
    knights = [u for u, e in before.items() if e[F["radius"]] < 1000 * SUB]
    assert len(knights) == 1, "the scene drifted: the Blue Knight is not on the board"
    kid = knights[0]
    died, last, skeletons = None, None, []
    for _ in range(30):
        b.step([], 1)
        tick, now = blue_units(b)
        if kid in now:
            last = at(now[kid])
        elif died is None:
            died = tick
        skeletons += [(tick, at(e)) for u, e in now.items() if u not in before and u != kid]
        before = now
        if len(skeletons) >= 2:
            return died, skeletons[1][0], skeletons[1][1], last
    raise AssertionError("the scene drifted: the Tombstone did not emit two Skeletons within 30 ticks")


def test_an_emission_steps_off_a_unit_doomed_on_its_tick():
    died, born, old, knight = emission_scene(OLD_ARM)
    assert died == born, f"the scene drifted: the Blue Knight died on {died} and the Skeleton came out on {born}"
    died_n, born_n, new, knight_n = emission_scene(NEW_ARM)
    assert (died_n, born_n, knight_n) == (died, born, knight), "the scene drifted between the arms before the birth"
    assert math.dist(new, old) >= 80, (
        f"the Skeleton born on the Knight's death tick stands at {new}, {math.dist(new, old):.0f} from where it stands "
        f"under hidden ({old}); the client steps it against the dying Knight"
    )
    assert math.dist(new, knight) >= math.dist(old, knight) + 80, (
        f"the Skeleton stands {math.dist(new, knight):.0f} from the dying Knight at {knight} "
        f"({math.dist(old, knight):.0f} under hidden): the Knight's body did not turn or push it"
    )


def test_with_nobody_dying_beside_it_both_arms_step_alike():
    died, born, old, _ = emission_scene(OLD_ARM, knight=FAR_KNIGHT)
    assert died is None or died != born, "the scene drifted: the far Knight died on the birth tick"
    _, born_n, new, _ = emission_scene(NEW_ARM, knight=FAR_KNIGHT)
    assert (born_n, new) == (born, old), (
        f"with nobody dying beside it the arms part: {old} on {born}, {new} on {born_n}"
    )


def test_the_shipped_value_is_the_new_arm():
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    assert ledger["spawner"]["FIRST_STEP_DYING_BODIES"]["value"] == NEW_ARM
    assert death_scene(None) == death_scene(NEW_ARM), "the build's own value is not client16402_seen"
