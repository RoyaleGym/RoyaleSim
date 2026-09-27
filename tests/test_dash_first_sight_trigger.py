"""A dasher that first sees its target inside its trigger distance triggers on that tick
(combat.DASH_FIRST_SIGHT_TRIGGER).

WHAT THIS PINS. Take F, the first tick a dasher's target is its target. On client 15.535.29, 12 of 12 dashers whose
target was already inside DashMaxRange + the target's radius on F (and not nearer than DashMinRange edge to edge)
moved on F + 17 (the Mega Knight, 2 of 2) or F + 16 (the Bandit, 10 of 10). The 16 that walked into the trigger
distance moved on T + 17 and T + 16, T the first tick within it. So F is the trigger. Today's engine triggers on
F + 1 and moves each of them one tick late. In the Mega Knight's single-card scenario that tick moves its goal cell
by 500, and the Mega Knight walks its whole route from the wrong cell.

THE SCENES ARE THE CLIENT'S. A red Giant stands hitting the blue right princess tower at (14731, 9439). Blue plays a
Mega Knight at (9500, 9500), 5,231 from it (inside 5,000 + 750, an edge of 3,731 over DashMinRange 3,500), or a
Bandit at (8500, 9500), 6,231 from it (inside 6,000 + 750). A played unit deploys, then targets the Giant on its first
active tick + 1, which is F.

WHY THE CONTROL IS HERE. A Mega Knight that walks into its trigger distance (set down 6,061 from the Giant) must jump
on the trigger + 17 under both values. An implementation that moves every dash a tick earlier fails it.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then
run this file: the named tests go red and the rest stay green.
  * `dash_first_sight_next_tick` -- first_sight_tick still triggers on the tick after first sight:
    test_a_dasher_played_inside_its_trigger_moves_on_the_first_sight_plus_its_count[MegaKnight],
    test_a_dasher_played_inside_its_trigger_moves_on_the_first_sight_plus_its_count[Assassin].
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.DASH_FIRST_SIGHT_TRIGGER"
NEW_ARM, OLD_ARM = "first_sight_tick", "next_tick"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
GIANT_AT = (14731, 9439)
#: where each dasher is played, and its count from the trigger to its first move (the Mega Knight moves on its entry
#: tick, DashCooldown 900 / 50 - 1 = 17; the Bandit is still on its entry, 800 / 50 - 1 = 15, and moves on 16)
PLAYED = {"MegaKnight": ((9500, 9500), 17), "Assassin": ((8500, 9500), 16)}
#: the control: a Mega Knight set down 6,061 from the Giant walks into DashMaxRange 5000 + the Giant's 750
MK_WALK_AT, MK_TRIGGER = (9500, 12500), 5000 + 750


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def played(arm, card, ticks=80):
    """Blue plays `card` at its PLAYED point beside the red Giant. Returns (F, the onset, the centre distance to the
    Giant on F's start): F the first tick the dasher targets the Giant, the onset its first move over 150."""
    at = PLAYED[card][0]
    b = royalesim.Battle(["Giant", card], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0, 1, 0, 1, 0, 1, 0, 1]] * 2, 0, 200, [10_000, 10_000], None,
            [(1, 0, GIANT_AT[0] * SUB, GIANT_AT[1] * SUB, -1)])
    now = json.loads(b.state_json())["entities"]
    giant = next(e[F["uid"]] for e in now if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    out = b.step([(0, 1, at[0] * SUB, at[1] * SUB)], 1)
    assert out, f"the {card} play returned nothing"
    assert out[0][1] == 0, f"the {card} was refused: {out}"
    first_sight = onset = start = None
    prev = prev_giant = None
    for _ in range(ticks):
        s = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        me = next((e for e in s.values() if e[F["team"]] == 0 and e[F["tower_slot"]] < 0), None)
        if me is not None and prev is not None:
            if first_sight is None and me[F["target_uid"]] == giant:
                first_sight = json.loads(b.state_json())["tick"]
                g0 = prev_giant
                start = ((g0[F["x"]] - prev[F["x"]]) ** 2 + (g0[F["y"]] - prev[F["y"]]) ** 2) ** 0.5 / SUB
            step = ((me[F["x"]] - prev[F["x"]]) ** 2 + (me[F["y"]] - prev[F["y"]]) ** 2) ** 0.5 / SUB
            if onset is None and step > 150:
                onset = json.loads(b.state_json())["tick"]
        prev, prev_giant = me, s.get(giant)
        b.step([], 1)
    return first_sight, onset, start


@pytest.mark.parametrize("card", sorted(PLAYED))
def test_a_dasher_played_inside_its_trigger_moves_on_the_first_sight_plus_its_count(card):
    f, onset, start = played(NEW_ARM, card)
    assert f is not None, "the scene drifted: the dasher never targeted the Giant"
    assert onset is not None, "the scene drifted: the dasher never dashed"
    reach = {"MegaKnight": 5000, "Assassin": 6000}[card] + 750
    assert start <= reach, f"the scene drifted: first sight at {start:.1f}, outside the trigger {reach}"
    count = PLAYED[card][1]
    assert onset - f == count, f"{NEW_ARM}: the {card} first saw the Giant on {f}, moved on {onset}, not on F + {count}"


@pytest.mark.parametrize("card", sorted(PLAYED))
def test_old_arm_is_todays_engine(card):
    f, onset, _ = played(OLD_ARM, card)
    assert onset - f == PLAYED[card][1] + 1, f"{OLD_ARM}: the {card} moved on F + {onset - f}"


def walk_rows(arm, ticks=60):
    """The control: per tick (start-of-tick centre distance to the Giant, the Mega Knight's step)."""
    b = royalesim.Battle(["Giant", "MegaKnight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(1, 0, GIANT_AT[0] * SUB, GIANT_AT[1] * SUB, -1), (0, 1, MK_WALK_AT[0] * SUB, MK_WALK_AT[1] * SUB, -1)])
    prev = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    g = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    m = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    rows = []
    for _ in range(ticks):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        if m not in now or g not in now:
            break
        M0, G0, M = prev[m], prev[g], now[m]
        rows.append((((G0[F["x"]] - M0[F["x"]]) ** 2 + (G0[F["y"]] - M0[F["y"]]) ** 2) ** 0.5 / SUB,
                     ((M[F["x"]] - M0[F["x"]]) ** 2 + (M[F["y"]] - M0[F["y"]]) ** 2) ** 0.5 / SUB))
        prev = now
    return rows


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_walking_trigger_is_the_same_under_both_values(arm):
    rows = walk_rows(arm)
    assert rows[0][0] > MK_TRIGGER, f"the scene drifted: the Mega Knight started {rows[0][0]:.0f} away"
    trig = next(i for i, r in enumerate(rows) if r[0] <= MK_TRIGGER)
    jump = next((i for i, r in enumerate(rows) if r[1] > 200), None)
    assert jump == trig + 17, f"{arm}: the jump began on {jump}, not on the trigger {trig} + 17"
