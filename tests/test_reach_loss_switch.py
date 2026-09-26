"""An attacker that strikes directly drops a target that leaves its reach mid-swing and keeps the swing for an enemy in
reach (targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = projectile_attackers_only, combat.RETARGET_PROGRESS =
keep_when_dead_or_in_reach).

WHAT THIS PINS. On client 15.535.29 a Knight swinging at a Hog Rider that runs out of its reach (Range 1200 + both
radii, plus the 25 of LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET) targets, on the next tick, a Cannon already in reach. It
stays in its attack, its attack progress runs on unbroken, and its next hit lands on the Cannon on the tick its swing
at the Hog would have landed. Measured in the 15.535.29 Cannon-in-range scenario (the Hog at 2415 on 291, the Cannon
targeted on 292 at progress 1100, hit on 294, the tick the no-Cannon control hits the Hog) and in the Royal Hogs
scenario (a Royal Hog kept at 2322, beyond at 2380 on 293, the next Royal Hog targeted on 294 at progress 3150, hit on
303 on the running cycle). Today's engine keeps the leaving target under its windup lock and lands the swing on it
beyond reach; with the lock off it switches but restarts the swing.

WHY THE CONTROLS ARE HERE.
- With no other enemy in sight the client's Knight keeps the leaving Hog and lands its swing 473 beyond reach. That
  holds on both arms, so an implementation that drops a leaving target whether or not another enemy is nearer fails.
- A Musketeer, which fires a projectile, keeps a Hog that leaves its reach while a Cannon stands in reach, on both
  arms, for the first two ticks past reach. It refuses a plain `false`.

THE PROJECTILE HALF (new arm). On client 15.535.29 a Musketeer whose Hog Rider runs out of reach while a Cannon stands
in reach keeps the Hog while the Hog's start-of-tick distance is within reach + 500 (H in [499.45, 544.52), 15 of 15
switches at every frame residue), then targets the Cannon on the first tick past it. A launch at the Hog beyond reach
ends the hold too, on the next tick. Today's engine keeps the Hog to reach + 1500 (the cancel range).
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
LOCK_KEY, PROGRESS_KEY = "targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED", "combat.RETARGET_PROGRESS"
KEYS = (LOCK_KEY, PROGRESS_KEY)
NEW = ("projectile_attackers_only", "keep_when_dead_or_in_reach")
OLD = (True, "keep_when_dead")
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: a red attacker on the red half; a blue Hog Rider running north past it to the red right princess tower (no river
#: on its way); and, unless left out, a blue Cannon in the attacker's reach but farther than the Hog at the start
KNIGHT_SCENE = ("Knight", (13300, 21000), (14500, 19500), (11300, 21600))
#: the projectile control: a red Musketeer south of a Hog running north off the bridge, a Cannon on the blue bank
MUSKETEER_SCENE = ("Musketeer", (14500, 12500), (14500, 17000), (10000, 14000))
#: reach on the Hog, Range + the attacker's radius + the Hog's 600, and the keep-target extension
REACH_ON_HOG = {"Knight": 1200 + 500 + 600, "Musketeer": 6000 + 500 + 600}
KEEP_EXTENSION = 25
#: a projectile attacker's hold past reach
PROJECTILE_HOLD = 500
KNIGHT_HIT, KNIGHT_PERIOD = 202, 24


def overrides(arms) -> dict:
    return {LOCK_KEY: json.dumps(arms[0]), PROGRESS_KEY: json.dumps(arms[1])}


def scene(arms, setup=KNIGHT_SCENE, with_cannon: bool = True, ticks: int = 60):
    """One row per tick: the attacker's target ("hog", "cannon", other or None), the Hog's centre distance from the
    attacker, and the hits of at least KNIGHT_HIT the Hog and the Cannon take on that tick."""
    attacker, at, hog_at, cannon_at = setup
    cards = [attacker, "HogRider", "Cannon"]
    spawns = [(1, 0, *at, -1), (0, 1, *hog_at, -1)] + ([(0, 2, *cannon_at, -1)] if with_cannon else [])
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arms))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None,
            [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns])
    ents = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
    uid = {cards[e[F["card_id"]]]: u for u, e in ents.items() if e[F["tower_slot"]] < 0}
    names = {uid["HogRider"]: "hog", uid.get("Cannon"): "cannon"}
    rows, prev = [], ents
    for t in range(1, ticks + 1):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        me, hog = now.get(uid[attacker]), now.get(uid["HogRider"])
        if me is None or hog is None:
            break
        target = names.get(me[F["target_uid"]], None if me[F["target_uid"]] < 0 else "other")
        dist = ((hog[F["x"]] - me[F["x"]]) ** 2 + (hog[F["y"]] - me[F["y"]]) ** 2) ** 0.5 / SUB
        hits = {n for u, n in names.items() if u in now and u in prev
                and prev[u][F["hp"]] - now[u][F["hp"]] >= KNIGHT_HIT}
        rows.append({"t": t, "target": target, "dist": dist, "hits": hits})
        prev = now
    return rows


def leave_tick(rows, attacker: str = "Knight") -> int:
    """The first tick the attacker holds the Hog and the Hog stands beyond reach + the keep extension."""
    return next(r["t"] for r in rows if r["target"] == "hog" and r["dist"] > REACH_ON_HOG[attacker] + KEEP_EXTENSION)


def last_hit_before(rows, t: int, who: str) -> int:
    return max(r["t"] for r in rows if r["t"] < t and who in r["hits"])


def test_a_direct_striker_switches_to_an_enemy_in_reach_and_keeps_its_swing():
    rows = scene(NEW)
    out = leave_tick(rows)
    last = last_hit_before(rows, out, "hog")
    due = last + KNIGHT_PERIOD
    assert last < out < due, f"the scene drifted: the Hog left reach on {out}, not inside the swing {last}..{due}"
    by_t = {r["t"]: r for r in rows}
    assert by_t[out]["target"] == "hog", f"the Knight let go of the Hog on {out}, before it had read it beyond reach"
    assert by_t[out + 1]["target"] == "cannon", (
        f"the Knight targets {by_t[out + 1]['target']} on {out + 1}, the tick after the Hog left reach, not the Cannon")
    late_hog = [r["t"] for r in rows if r["t"] > out and "hog" in r["hits"]]
    assert late_hog == [], f"the Knight still hit the leaving Hog on {late_hog}"
    first_cannon = next((r["t"] for r in rows if "cannon" in r["hits"]), None)
    assert first_cannon == due, (
        f"the first hit on the Cannon came on {first_cannon}, not on {due} (the running cycle: last hit {last} + 24)")


@pytest.mark.parametrize("arms", [NEW, OLD])
def test_with_no_other_enemy_the_swing_finishes_on_the_leaving_target(arms):
    rows = scene(arms, with_cannon=False)
    out = leave_tick(rows)
    last = last_hit_before(rows, out, "hog")
    due = last + KNIGHT_PERIOD
    by_t = {r["t"]: r for r in rows}
    held = [t for t in range(out, due + 1) if by_t[t]["target"] != "hog"]
    assert held == [], f"the Knight let go of the Hog on {held} with no other enemy in sight"
    assert "hog" in by_t[due]["hits"], f"no hit on the Hog on {due} (the running cycle, beyond reach)"


@pytest.mark.parametrize("arms", [NEW, OLD])
def test_a_projectile_attacker_keeps_a_target_that_leaves_its_reach(arms):
    rows = scene(arms, setup=MUSKETEER_SCENE)
    out = leave_tick(rows, "Musketeer")
    by_t = {r["t"]: r for r in rows}
    assert any("hog" in r["hits"] for r in rows if r["t"] < out), "the scene drifted: the Hog was not shot before"
    kept = [by_t[t]["target"] for t in (out + 1, out + 2)]
    assert kept == ["hog", "hog"], f"the Musketeer targets {kept} on {out + 1}, {out + 2}, after the Hog left reach"


def test_old_arms_are_todays_engine():
    rows = scene(OLD)
    out = leave_tick(rows)
    last = last_hit_before(rows, out, "hog")
    due = last + KNIGHT_PERIOD
    by_t = {r["t"]: r for r in rows}
    assert "hog" in by_t[due]["hits"], f"old arms: no hit on the Hog on {due} beyond reach"
    assert by_t[due + 1]["target"] == "cannon", f"old arms: the Knight targets {by_t[due + 1]['target']} after its hit"


def test_a_projectile_attacker_holds_a_leaving_target_to_reach_plus_500():
    rows = scene(NEW, setup=MUSKETEER_SCENE, ticks=80)
    by_t = {r["t"]: r for r in rows}
    switch = next((r["t"] for r in rows if r["target"] == "cannon"), None)
    assert switch is not None, "the Musketeer never took the Cannon"
    limit = REACH_ON_HOG["Musketeer"] + PROJECTILE_HOLD
    start, before = by_t[switch - 1]["dist"], by_t[switch - 2]["dist"]
    assert before <= limit < start, (
        f"the Musketeer took the Cannon on {switch} with the Hog {start:.0f} away at the start of the tick (held at "
        f"{before:.0f}), not on the first tick past reach + {PROJECTILE_HOLD} = {limit}")
