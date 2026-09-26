"""A leaping Hog Rider cannot be targeted by a ground-only attacker (targeting.LEAPING_UNIT_TARGETABILITY).

WHAT THIS PINS. On client 15.535.29 a Hog Rider that leaps the river is dropped by every ground-only attacker that was
fighting it. Let F be the Hog's first leap frame (the hop frame) and `land` the frame it lands on. The attacker still
targets the Hog on F, is off it on every frame F+1..land with its swing cancelled (attack progress 0), and takes it
again on land+1. The witnesses are a Knight, a Valkyrie and a Mini PEKKA (10 runs, 3 distinct leaps); none of them
hit the leaping Hog. Today's engine keeps the leaping Hog targetable: the Knight holds it through the leap and its
swing lands while the Hog is in the air.

WHY THE CONTROLS ARE HERE. An air-and-ground attacker keeps the leaping Hog: on client 15.535.29 a Musketeer targets
it on every frame F..land and its shot damages it mid-leap (2 runs). An implementation that makes a leaping unit
untargetable by everyone fails the control. The control runs beside the Knight, as on client 15.535.29.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `leap_targetable_by_ground` -- a leaping troop stays a ground target under the new arm too:
    test_ground_only_knight_drops_the_leaping_hog, test_one_frame_lag_each_way.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.LEAPING_UNIT_TARGETABILITY"
NEW_ARM, OLD_ARM = "airborne", "ground"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["Knight", "HogRider", "Musketeer"]
#: a blue Hog Rider that walks to the right bridge and leaps the river
HOG_AT = (9500, 11500)
#: a red Knight (ground only) that fights the Hog on the blue bank
KNIGHT_AT = (12000, 12500)
#: a red Musketeer (air and ground) on the red bank, in range of the whole leap
MUSKETEER_AT = (11500, 18000)
#: a leap tick moves the Hog JumpSpeed (160); a walk tick moves it about 120
LEAP_STEP = (155, 165)
#: level 11 damage per hit, used to tell who hit the Hog
KNIGHT_HIT, MUSKETEER_SHOT = 202, 217
IDLE = 0
WINDUP = 1


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def leap_run(arm, musketeer, ticks=60):
    """The Hog and the Knight (and the Musketeer when `musketeer`) spawn on the first tick. Returns (first, land, on,
    phase, drops): first is the Hog's first leap tick F (its hop tick, the tick before its first JumpSpeed step), land
    its landing tick (its last JumpSpeed step); on[name][t] is True when that unit targets the Hog on tick t;
    phase[name][t] is its attack phase; drops maps a tick to the hp the Hog lost on it."""
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    spawns = [(0, 1, *HOG_AT), (1, 0, *KNIGHT_AT)] + ([(1, 2, *MUSKETEER_AT)] if musketeer else [])
    b.reset(
        0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [(t, c, x * SUB, y * SUB, -1) for t, c, x, y in spawns]
    )
    hog, pos, hp, on, phase = None, {}, {}, {}, {}
    for t in range(1, ticks + 1):
        b.step([], 1)
        units = [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0]
        h = [e for e in units if CARDS[e[F["card_id"]]] == "HogRider"]
        assert h, f"the Hog died or vanished on tick {t}"
        hog = h[0][F["uid"]]
        pos[t], hp[t] = (h[0][F["x"]] / SUB, h[0][F["y"]] / SUB), h[0][F["hp"]]
        for e in units:
            if e[F["uid"]] != hog:
                name = CARDS[e[F["card_id"]]]
                on.setdefault(name, {})[t] = e[F["target_uid"]] == hog
                phase.setdefault(name, {})[t] = e[F["attack_phase"]]
    step = {
        t: ((pos[t][0] - pos[t - 1][0]) ** 2 + (pos[t][1] - pos[t - 1][1]) ** 2) ** 0.5 for t in range(2, ticks + 1)
    }
    leap = [t for t, s in step.items() if LEAP_STEP[0] <= s <= LEAP_STEP[1]]
    assert leap, "the Hog never leapt"
    first, land = leap[0] - 1, leap[-1]
    assert leap == list(range(first + 1, land + 1)), f"the leap steps are not one run: {leap}"
    assert land - first >= 8, f"a short leap: F {first}, land {land}"
    assert land + 4 <= ticks, f"the run ends too soon after the landing on {land}"
    assert all(step[t] < 130 for t in range(land + 1, land + 4)), (
        f"the landing is not clean (steps after it: {[round(step[t]) for t in range(land + 1, land + 4)]})"
    )
    drops = {t: hp[t - 1] - hp[t] for t in range(2, ticks + 1) if hp[t] < hp[t - 1]}
    assert set(drops.values()) <= {KNIGHT_HIT, MUSKETEER_SHOT}, (
        f"a hit that is not one Knight or Musketeer hit: {drops}"
    )
    # the precondition: the Knight is fighting the Hog, mid-swing, when the leap begins, and no hit falls on F
    assert on["Knight"][first - 1], "the Knight is not on the Hog on the tick before the leap"
    assert phase["Knight"][first] == WINDUP, f"the Knight is not mid-swing on F (phase {phase['Knight'][first]})"
    assert any(t < first and d == KNIGHT_HIT for t, d in drops.items()), f"no Knight hit before the leap: {drops}"
    assert drops.get(first) != KNIGHT_HIT, "a Knight hit falls on the hop tick F; move the scenario"
    return first, land, on, phase, drops


@pytest.mark.parametrize("musketeer", [False, True])
def test_ground_only_knight_drops_the_leaping_hog(musketeer):
    first, land, on, phase, drops = leap_run(NEW_ARM, musketeer)
    held = [t for t in range(first + 1, land + 1) if on["Knight"][t]]
    assert held == [], f"the Knight targets the leaping Hog on {held} (F {first}, land {land})"
    busy = {t: phase["Knight"][t] for t in range(first + 1, land + 1) if phase["Knight"][t] != IDLE}
    assert busy == {}, f"the Knight's swing is not cancelled on F+1..land (tick: phase): {busy}"
    hits = [t for t in range(first + 1, land + 1) if drops.get(t) == KNIGHT_HIT]
    assert hits == [], f"a Knight hit lands on the leaping Hog on {hits} (F {first}, land {land})"


def test_one_frame_lag_each_way():
    """On client 15.535.29 the ground-only attacker still targets the Hog on F, is off it from F+1, is still off it
    on the landing frame, and takes it again on land+1 (10 of 10 witness rows; 9 retake on land+1, the tenth had
    switched to a Cannon)."""
    first, land, on, _phase, _drops = leap_run(NEW_ARM, musketeer=False)
    k = on["Knight"]
    got = {"F": k[first], "F+1": k[first + 1], "land": k[land], "land+1": k[land + 1]}
    want = {"F": True, "F+1": False, "land": False, "land+1": True}
    assert got == want, f"the Knight on the Hog (F {first}, land {land}): got {got}, want {want}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_air_and_ground_musketeer_keeps_and_damages_the_leaping_hog(arm):
    """Control: the Musketeer targets the Hog on every tick F..land and a Musketeer shot hurts it mid-leap."""
    first, land, on, _phase, drops = leap_run(arm, musketeer=True)
    lost = [t for t in range(first, land + 1) if not on["Musketeer"][t]]
    assert lost == [], f"the Musketeer is off the leaping Hog on {lost} (F {first}, land {land})"
    shots = [t for t in range(first + 1, land) if drops.get(t) == MUSKETEER_SHOT]
    assert shots, f"no Musketeer shot hurts the Hog on F+1..land-1 (F {first}, land {land}, drops {drops})"


def test_old_arm_is_todays_engine():
    first, land, on, _phase, drops = leap_run(OLD_ARM, musketeer=False)
    lost = [t for t in range(first, land + 1) if not on["Knight"][t]]
    assert lost == [], f"old arm: the Knight is off the leaping Hog on {lost}"
    hits = [t for t in range(first + 1, land) if drops.get(t) == KNIGHT_HIT]
    assert hits, f"old arm: no Knight hit on the leaping Hog (F {first}, land {land}, drops {drops})"
