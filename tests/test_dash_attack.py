"""A unit with a dash block stands, then dashes into its target for DashDamage (combat.DASH_ATTACK).

WHAT THIS PINS. On client 15.535.29 a Bandit (the Assassin row: DashMaxRange 6000, DashCooldown 800, JumpSpeed 500,
DashDamage 152, Range 750, radius 600) walking after a Knight (radius 500) stops on the first tick whose start-of-tick
centre distance is at most DashMaxRange + the target's radius (6500 here; 6750 for a Giant, 7000 for a princess tower).
It stands for 16 ticks (DashCooldown), then moves about 500 a tick as two half-steps. After each half-step it tests the
edge gap to the target against its Range 750 and stops on the first within, so its last move is about 250 or about 500
and it ends at an edge gap between 500 and 750. That tick deals DashDamage at level 11 (389), and its melee (194)
follows. Measured in the 15.535.29 Bandit dash scenarios (both sides) and 13 further dashes: the last move was about 250
in 9 and about 500 in 4, ending at 540.8-738.9. Today's engine does not read the dash block: the Bandit walks into
melee range.

WHY THE CONTROL IS HERE. A melee unit without a dash block (a Mini P.E.K.K.A) walks into melee range on both arms: no
step longer than a walk and no 16-tick stand before its first hit. An implementation that gives every melee unit a
dash fails it.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.DASH_ATTACK"
NEW_ARM, OLD_ARM = "client_dash", "none"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: a red Knight that walks to the blue right princess tower and stands hitting it; a blue attacker, in sight of it and
#: beyond the trigger distance, walking after it
KNIGHT_AT, ATTACKER_AT = (14231, 9500), (8200, 6200)
#: the control's walker starts nearer, well inside a Mini P.E.K.K.A's sight
CONTROL_AT = (10500, 7000)
#: DashMaxRange 6000 plus the target's radius (a Knight's 500), centre, start of tick
TRIGGER, STAND = 6500, 16
#: the Bandit's Range and both radii: the dash ends on the first step with an edge gap within Range
RANGE, RADII = 750, 600 + 500
DASH_HIT, STEP_LO, STEP_HI = 389, 494, 502
#: the last move may be one half-step
HALF_LO, HALF_HI = 240, 260


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def run(arm, attacker, ticks=80, start=None):
    """Per tick: (tick, the attacker's step, start-of-tick centre distance, end-of-tick edge gap, Knight's hp loss)."""
    cards = ["Knight", attacker]
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(1, 0, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB, -1),
             (0, 1, (start or ATTACKER_AT)[0] * SUB, (start or ATTACKER_AT)[1] * SUB, -1)])
    prev = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    k = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    a = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    rows = []
    for t in range(1, ticks + 1):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        if a not in now or k not in now:
            break
        A, K, A0, K0 = now[a], now[k], prev[a], prev[k]
        step = ((A[F["x"]] - A0[F["x"]]) ** 2 + (A[F["y"]] - A0[F["y"]]) ** 2) ** 0.5 / SUB
        start = ((K0[F["x"]] - A0[F["x"]]) ** 2 + (K0[F["y"]] - A0[F["y"]]) ** 2) ** 0.5 / SUB
        edge = ((K[F["x"]] - A[F["x"]]) ** 2 + (K[F["y"]] - A[F["y"]]) ** 2) ** 0.5 / SUB - RADII
        rows.append((t, step, start, edge, K0[F["hp"]] - K[F["hp"]]))
        prev = now
    return rows


def trigger_tick(rows) -> int:
    return next(t for t, _, start, _, _ in rows if start <= TRIGGER)


def test_a_bandit_stands_then_dashes_into_its_target():
    rows = run(NEW_ARM, "Assassin")
    assert rows[0][2] > TRIGGER + 200, f"the scene drifted: the Bandit started only {rows[0][2]:.0f} away"
    d = trigger_tick(rows)
    by_t = {r[0]: r for r in rows}
    moved = [(t, round(by_t[t][1], 1)) for t in range(d, d + STAND) if by_t[t][1] > 0]
    assert moved == [], f"the Bandit moved while it should stand, from the trigger tick {d}: {moved}"
    dash = []
    for t in range(d + STAND, d + STAND + 20):
        dash.append(by_t[t])
        if by_t[t][3] <= RANGE:
            break
    steps = [round(r[1], 1) for r in dash]
    assert all(STEP_LO <= s <= STEP_HI for s in steps[:-1]), f"dash steps from {d + STAND}: {steps}"
    last_ok = HALF_LO <= steps[-1] <= HALF_HI or STEP_LO <= steps[-1] <= STEP_HI
    assert last_ok, f"the last dash move {steps[-1]} is neither a half-step nor a whole one: {steps}"
    assert dash[-1][3] <= RANGE < dash[-2][3], f"the dash did not stop on the first move within Range: {dash[-2:]}"
    assert dash[-1][3] > RANGE - HALF_HI, f"the dash stopped at edge {dash[-1][3]:.1f}, past its first half-step within"
    last = dash[-1][0]
    early = [(t, loss) for t, *_, loss in rows if t < last and loss not in (0, 109)]
    assert early == [], f"the Knight lost hp to the Bandit before the dash's last step on {last}: {early}"
    assert by_t[last][4] == DASH_HIT, f"the Knight lost {by_t[last][4]} on {last}, the dash's last step, not {DASH_HIT}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_melee_unit_without_a_dash_walks_into_range(arm):
    rows = run(arm, "MiniPekka", start=CONTROL_AT)
    first_hit = next((t for t, *_, loss in rows if loss not in (0, 109)), None)
    assert first_hit is not None, "the scene drifted: the Mini P.E.K.K.A never hit the Knight"
    long_steps = [(t, round(st)) for t, st, *_ in rows if st > 150]
    assert long_steps == [], f"the Mini P.E.K.K.A took steps longer than a walk: {long_steps}"
    run_len, longest = 0, 0
    for t, st, *_ in rows:
        if t >= first_hit:
            break
        run_len = run_len + 1 if st == 0 else 0
        longest = max(longest, run_len)
    assert longest < STAND, f"the Mini P.E.K.K.A stood {longest} ticks before its first hit on {first_hit}"


def test_old_arm_is_todays_engine():
    rows = run(OLD_ARM, "Assassin")
    d = trigger_tick(rows)
    by_t = {r[0]: r for r in rows}
    assert by_t[d][1] > 0, "old arm: the Bandit stood at the trigger distance"
    assert max(s for _, s, *_ in rows) < 150, "old arm: the Bandit took a step longer than a walk"


#: the Mega Knight's jump against a Giant that stands hitting the blue right princess tower (DashMaxRange 5000 + 750).
#: It starts 6061 away, so it walks and the trigger falls mid-walk. A unit put down inside its trigger distance first
#: tests it on the frame after its first active one (measured, both units), which a spawned unit cannot show cleanly.
MK_AT, GIANT_AT = (9500, 12500), (14731, 9439)
MK_TRIGGER, MK_WAIT, MK_HIT_AFTER, MK_DASH_HIT, RADII_MK = 5000 + 750, 17, 16, 537, 750 + 750


def jump_rows(arm, ticks=60):
    """Per tick: (tick, MK step, start-of-tick centre distance, MK position, the Giant's position and hp loss)."""
    b = royalesim.Battle(["Giant", "MegaKnight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(1, 0, GIANT_AT[0] * SUB, GIANT_AT[1] * SUB, -1), (0, 1, MK_AT[0] * SUB, MK_AT[1] * SUB, -1)])
    prev = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    g = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 1)
    m = next(u for u, e in prev.items() if e[F["tower_slot"]] < 0 and e[F["team"]] == 0)
    rows = []
    for t in range(1, ticks + 1):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        if m not in now or g not in now:
            break
        M, G, M0, G0 = now[m], now[g], prev[m], prev[g]
        rows.append((t, ((M[F["x"]] - M0[F["x"]]) ** 2 + (M[F["y"]] - M0[F["y"]]) ** 2) ** 0.5 / SUB,
                     ((G0[F["x"]] - M0[F["x"]]) ** 2 + (G0[F["y"]] - M0[F["y"]]) ** 2) ** 0.5 / SUB,
                     (M0[F["x"]] / SUB, M0[F["y"]] / SUB), (M[F["x"]] / SUB, M[F["y"]] / SUB),
                     (G0[F["x"]] / SUB, G0[F["y"]] / SUB), G0[F["hp"]] - G[F["hp"]]))
        prev = now
    return rows


def goal_cell_centre(mk, giant):
    """The 500-cell holding the point (both radii) short of the Giant's centre on the line to the Mega Knight."""
    dx, dy = mk[0] - giant[0], mk[1] - giant[1]
    d = (dx * dx + dy * dy) ** 0.5
    px, py = giant[0] + dx * RADII_MK / d, giant[1] + dy * RADII_MK / d
    return (int(px // 500) * 500 + 250, int(py // 500) * 500 + 250)


def test_a_mega_knight_jumps_to_its_goal_cell_and_lands_its_blow_on_the_constant_time():
    rows = jump_rows(NEW_ARM)
    trig = next(t for t, _, start, *_ in rows if start <= MK_TRIGGER)
    onset = next((t for t, step, *_ in rows if step > 200), None)
    assert onset is not None, "the Mega Knight never jumped: no move longer than 200"
    assert onset == trig + MK_WAIT, f"the jump began on {onset}, not on the trigger {trig} + {MK_WAIT}"
    by_t = {r[0]: r for r in rows}
    cell = goal_cell_centre(by_t[onset][3], by_t[onset][5])
    moves = [(t, round(by_t[t][1], 1)) for t in range(onset, onset + 20) if t in by_t and by_t[t][1] > 0]
    assert all(240 <= s <= 260 for _, s in moves[:-1]), f"the jump's moves: {moves}"
    rest = by_t[moves[-1][0]][4]
    assert abs(rest[0] - cell[0]) <= 2, f"the jump came to rest on {rest}, not on the goal cell's centre {cell}"
    assert abs(rest[1] - cell[1]) <= 2, f"the jump came to rest on {rest}, not on the goal cell's centre {cell}"
    tower = {loss for t, *_, loss in rows if t < onset and loss}
    hit = by_t[onset + MK_HIT_AFTER][6]
    assert hit == MK_DASH_HIT or hit - MK_DASH_HIT in tower, (
        f"the Giant lost {hit} on {onset + MK_HIT_AFTER}, the onset + {MK_HIT_AFTER}, not {MK_DASH_HIT}")
    early = [(t, loss) for t, *_, loss in rows if onset <= t < onset + MK_HIT_AFTER and loss and loss not in tower]
    assert early == [], f"the Giant lost hp to the Mega Knight before the onset + {MK_HIT_AFTER}: {early}"


def test_old_arm_walks_the_mega_knight():
    rows = jump_rows(OLD_ARM)
    assert max(step for _, step, *_ in rows) < 100, "old arm: the Mega Knight took a step longer than a walk"
