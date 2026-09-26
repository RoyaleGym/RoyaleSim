"""The Electro Wizard's card zaps on deploy: its area effect acts on the character's first tick
(spells.DEPLOY_AREA_EFFECT).

WHAT THIS PINS. On client 15.535.29 the Electro Wizard card is an area effect (ElectroWizardZap: Damage 75, Radius
3000, Buff ZapFreeze for 500 ms, CrownTowerDamagePercent -100) whose starting action spawns the ElectroWizard
character. On the character's first tick F, an enemy Knight 1271 away lost 192 (Damage 75 at level 11) and dropped its
target; it took no step on F+1..F+10 (the 500 ms freeze) and targeted the Electro Wizard on F+11. Both sides of the
15.535.29 scenario agree. Today's engine loads only the character: the Knight is untouched until the Electro Wizard's
first bolt.

THE ICE WIZARD has the same shape (IceWizardCold: Damage 33, Radius 3000, a 2500 ms slow of -30, spawning the IceWizard
character). On client 15.535.29, both sides: a Knight 3232 from it lost 84 on its first tick F and walked 42 a tick
(floor(60 x 0.7)) on F+1..F+50, full speed on F+51; a Knight 3711 away was untouched. So the reach is the centre within
Radius + the victim's collision radius (3500 for a Knight), not the centre within Radius. Killing the Ice Wizard on F+2
does not shorten the slow.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `deploy_area_effect_unread` -- the new arm deploys the character alone, as the old one does:
    test_the_zap_hits_on_the_first_tick_and_freezes,
    test_the_ice_wizard_zaps_and_slows_a_knight_within_radius_plus_its_radius.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spells.DEPLOY_AREA_EFFECT"
NEW_ARM, OLD_ARM = "client_area_effect", "none"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: a red Knight walking on the blue half; the blue Electro Wizard played 2000 from it
KNIGHT_AT, PLAY_AT, PLAY_TICK = (12500, 12500), (12500, 10500), 2
ZAP, FREEZE = 192, 10


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def run(arm, ticks=30):
    """Per tick: every unit's row by uid."""
    b = royalesim.Battle(["Knight", "ElectroWizard"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [(1, 0, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB, -1)])
    rows = [{e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}]
    for t in range(1, ticks + 1):
        b.step([(0, 0, PLAY_AT[0] * SUB, PLAY_AT[1] * SUB)] if t == PLAY_TICK else [], 1)
        rows.append({e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0})
    return rows


def scene(arm):
    rows = run(arm)
    knight = next(iter(rows[0]))
    first = next(t for t, r in enumerate(rows) if any(e[F["team"]] == 0 for e in r.values()))
    return rows, knight, first


def step(rows, k, t):
    a, b = rows[t - 1][k], rows[t][k]
    return ((b[F["x"]] - a[F["x"]]) ** 2 + (b[F["y"]] - a[F["y"]]) ** 2) ** 0.5 / SUB


def test_the_zap_hits_on_the_first_tick_and_freezes():
    rows, k, f = scene(NEW_ARM)
    loss = rows[f - 1][k][F["hp"]] - rows[f][k][F["hp"]]
    assert loss == ZAP, f"the Knight lost {loss} on {f}, the Electro Wizard's first tick, not {ZAP}"
    moved = [(t, round(step(rows, k, t))) for t in range(f + 1, f + 1 + FREEZE) if step(rows, k, t) > 0]
    assert moved == [], f"the Knight moved during the freeze F+1..F+{FREEZE}: {moved}"


def test_old_arm_is_todays_engine():
    rows, k, f = scene(OLD_ARM)
    losses = [rows[t - 1][k][F["hp"]] - rows[t][k][F["hp"]] for t in range(f, f + 3)]
    assert losses == [0, 0, 0], f"old arm: the Knight lost {losses} on the Electro Wizard's first ticks"


#: the Ice Wizard played on the left bridge's blue end; a red Knight walking south over the bridge, spawned so that on
#: the Wizard's first tick it stands 3288 (inside 3000 + 500, outside 3000) or 3788 (outside) from it
ICE_AT, BAND_Y, FAR_Y = (3500, 14500), 14500 + 3400, 14500 + 3900
ICE_ZAP, SLOW_STEP, FULL_STEP = 84, 42, 55


def ice_scene(arm, knight_y, ticks=30):
    b = royalesim.Battle(["Knight", "IceWizard"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [10_000, 10_000], None, [(1, 0, ICE_AT[0] * SUB, knight_y * SUB, -1)])
    rows = [{e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}]
    for t in range(1, ticks + 1):
        b.step([(0, 0, ICE_AT[0] * SUB, ICE_AT[1] * SUB)] if t == PLAY_TICK else [], 1)
        rows.append({e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0})
    knight = next(iter(rows[0]))
    first = next(t for t, r in enumerate(rows) if any(e[F["team"]] == 0 for e in r.values()))
    return rows, knight, first


def test_the_ice_wizard_zaps_and_slows_a_knight_within_radius_plus_its_radius():
    rows, k, f = ice_scene(NEW_ARM, BAND_Y)
    loss = rows[f - 1][k][F["hp"]] - rows[f][k][F["hp"]]
    assert loss == ICE_ZAP, f"the Knight lost {loss} on {f}, the Ice Wizard's first tick, not {ICE_ZAP}"
    steps = [round(step(rows, k, t)) for t in range(f + 1, f + 16)]
    assert all(SLOW_STEP - 2 <= s <= SLOW_STEP + 1 for s in steps), f"the Knight's steps on F+1..F+15: {steps}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_knight_beyond_radius_plus_its_radius_is_untouched(arm):
    rows, k, f = ice_scene(arm, FAR_Y)
    loss = rows[f - 1][k][F["hp"]] - rows[f][k][F["hp"]]
    steps = [round(step(rows, k, t)) for t in range(f + 1, f + 11)]
    assert loss == 0, f"{arm}: the far Knight lost {loss} on the Ice Wizard's first tick"
    assert all(s >= FULL_STEP for s in steps), f"{arm}: the far Knight stepped {steps} on F+1..F+10"


def test_old_arm_leaves_the_ice_wizard_inert():
    rows, k, f = ice_scene(OLD_ARM, BAND_Y)
    loss = rows[f - 1][k][F["hp"]] - rows[f][k][F["hp"]]
    steps = [round(step(rows, k, t)) for t in range(f + 1, f + 6)]
    assert loss == 0, f"old arm: the band Knight lost {loss} on the Ice Wizard's first tick"
    assert all(s >= FULL_STEP for s in steps), f"old arm: the band Knight stepped {steps} on F+1..F+5"
