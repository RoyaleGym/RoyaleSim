"""A Phoenix's death fires its DeathSpawnProjectile: a fireball and an egg that hatches a new Phoenix
(spawner.DEATH_SPAWN_PROJECTILE).

WHAT THIS PINS. On client 15.535.29 a Phoenix whose last frame is D is followed, on D+2, by its DeathSpawnProjectile
(PhoenixFireball) at its death point: every enemy whose centre is within the fireball's Radius 2500 plus its own radius
loses Damage 64 at the Phoenix's level (163 at level 11), and the projectile's SpawnCharacter, a PhoenixEgg (124 hp at
level 1, 317 at level 11), appears there for the Phoenix's side. The egg is the PhoenixEgg row, a spawner that makes
one PhoenixNoRespawn at SpawnStartTime 3800 ms: it hatched 76 ticks after it appeared, into a full-hp Phoenix (1052 at
level 11) 1100 ahead of it (the egg's radius 600 plus the Phoenix's 500, forward for its side), deploying. Measured in
the 15.535.29 Phoenix scenarios: the fireball's 163 on four enemies at 915-2517 from the death point and on a Minion at
2590 (killed), the egg on D+2 in all three runs, the hatch on the egg's first tick + 76 at (0, +1100). Today's engine
reads no DeathSpawnProjectile: the Phoenix dies and nothing follows.

ONE COLUMN OF THE EGG ROW IS NOT READ YET: DestroyAtLimit. On client 15.535.29 the egg is gone once its one Phoenix
is out; the engine's egg lives on. test_the_egg_is_gone_once_its_phoenix_is_out is a strict xfail naming it. The egg
row's SpawnCharacterWithDeploy (the new Phoenix deploys) is not pinned here.

NO CONTROL beyond the old arm: every other death spawn in the card data is a DeathSpawnCharacter, which the engine
already reads and this key does not touch.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `death_projectile_unread` -- a Phoenix dies with nothing following under the new arm too:
    test_the_egg_appears_on_d_plus_2_and_hatches_76_ticks_later,
    test_the_fireball_hits_an_enemy_within_reach_on_d_plus_2.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spawner.DEATH_SPAWN_PROJECTILE"
NEW_ARM, OLD_ARM = "client_projectile", "none"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
EGG_HP, PHOENIX_HP, FIREBALL, HATCH_TICKS, HATCH_AHEAD = 317, 1052, 163, 76, 1100
#: a blue Phoenix at 1 hp and a red Minion at 100 hp beside it: the Minion kills the Phoenix and dies to it or to the
#: fireball, so nothing is left to kill the egg
HATCH_SCENE = [(0, 1, 9500, 12000, 1), (1, 0, 9500, 13500, 100)]
#: the same, plus a red Knight (ground only, it cannot hit the Phoenix) standing within the fireball's reach
FIREBALL_SCENE = [*HATCH_SCENE, (1, 2, 11000, 12500, -1)]


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def run(arm, spawns, ticks=110):
    cards = ["Minions", "Phoenix", "Knight"]
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[1] * 8, [0] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns],
    )
    rows = []
    for _ in range(ticks + 1):
        rows.append({e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0})
        b.step([], 1)
    return rows


def pos(e):
    return e[F["x"]] // SUB, e[F["y"]] // SUB


def death(rows):
    """(D, the Phoenix's last position): D is its last tick."""
    ph = next(u for u, e in rows[0].items() if e[F["team"]] == 0)
    d = max(t for t, r in enumerate(rows) if ph in r)
    return d, pos(rows[d][ph])


def births(rows, t, team):
    return [e for u, e in rows[t].items() if u not in rows[t - 1] and e[F["team"]] == team]


def near(a, b, tol=60):
    return abs(a[0] - b[0]) <= tol and abs(a[1] - b[1]) <= tol


def test_the_egg_appears_on_d_plus_2_and_hatches_76_ticks_later():
    rows = run(NEW_ARM, HATCH_SCENE)
    d, at = death(rows)
    assert d < 40, f"the scene drifted: the Phoenix lived to {d}"
    eggs = [e for e in births(rows, d + 2, 0) if e[F["max_hp"]] == EGG_HP]
    assert len(eggs) == 1, f"no egg of {EGG_HP} hp on {d + 2} (D = {d}): {births(rows, d + 2, 0)}"
    egg = eggs[0]
    assert near(pos(egg), at), f"the egg appeared at {pos(egg)}, not at the death point {at}"
    t = d + 2 + HATCH_TICKS
    uid = egg[F["uid"]]
    moved = [s for s in range(d + 3, t) if uid in rows[s] and not near(pos(rows[s][uid]), pos(egg))]
    assert moved == [], f"the egg moved before it hatched, first on {moved[:1]}"
    born = [e for e in births(rows, t, 0) if e[F["max_hp"]] == PHOENIX_HP]
    assert len(born) == 1, f"no Phoenix of {PHOENIX_HP} hp hatched on {t}: {births(rows, t, 0)}"
    ahead = (pos(egg)[0], pos(egg)[1] + HATCH_AHEAD)
    assert near(pos(born[0]), ahead), f"it hatched at {pos(born[0])}, not {ahead}"
    early = [s for s in range(d + 3, t) if any(e[F["max_hp"]] == PHOENIX_HP for e in births(rows, s, 0))]
    assert early == [], f"a Phoenix hatched early, on {early}"


@pytest.mark.xfail(strict=True, reason="the PhoenixEgg row's DestroyAtLimit is not read: the egg outlives its hatch")
def test_the_egg_is_gone_once_its_phoenix_is_out():
    rows = run(NEW_ARM, HATCH_SCENE)
    d, _ = death(rows)
    eggs = [e for e in births(rows, d + 2, 0) if e[F["max_hp"]] == EGG_HP]
    assert len(eggs) == 1, f"the scene drifted: no egg of {EGG_HP} hp on {d + 2} (D = {d})"
    t = d + 2 + HATCH_TICKS
    assert len(rows) > t + 2, f"the run ended on {len(rows) - 1}"
    assert eggs[0][F["uid"]] not in rows[t + 2], "the egg outlived its hatch"


def test_the_fireball_hits_an_enemy_within_reach_on_d_plus_2():
    rows = run(NEW_ARM, FIREBALL_SCENE, ticks=40)
    d, at = death(rows)
    knight = next(u for u, e in rows[0].items() if e[F["team"]] == 1 and e[F["max_hp"]] > 1000)
    k = rows[d + 2][knight]
    dist = ((pos(k)[0] - at[0]) ** 2 + (pos(k)[1] - at[1]) ** 2) ** 0.5
    assert dist <= 2500 + 500, f"the scene drifted: the Knight stands {dist:.0f} from the death point"
    losses = {t: rows[t - 1][knight][F["hp"]] - rows[t][knight][F["hp"]] for t in range(d + 1, d + 4)}
    assert losses[d + 2] == FIREBALL, f"the Knight's losses on D+1..D+3 (D = {d}): {losses}"


def test_old_arm_is_todays_engine():
    rows = run(OLD_ARM, HATCH_SCENE, ticks=60)
    d, _ = death(rows)
    later = [e[F["max_hp"]] for t in range(d + 1, len(rows)) for e in births(rows, t, 0)]
    assert later == [], f"old arm: something of the Phoenix's side appeared after its death: {later}"
