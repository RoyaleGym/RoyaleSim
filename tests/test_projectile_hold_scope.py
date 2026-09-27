"""A walking projectile troop holds nothing past its keep reach (targeting.PROJECTILE_HOLD_SCOPE).

WHAT THIS PINS. Under targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = projectile_attackers_only a projectile attacker
keeps its target to Range + both radii + 500. On the 16.402 corpus a projectile TROOP holds that far only while it is
in its attack. Walking to a target that stands past its keep reach (Range + both radii + 25) and inside the hold, with
a valid enemy nearer by centre in sight, it took the nearer enemy on the tick in 8 of 8 samples (7 events, 4 battles;
in 20260918-112751 on tick 1590 an Archer walking to a Goblin Hut 225.3 past its reach takes the Hut's new wave member,
5,304.3 from it, on the member's 8th frame). In its attack it kept the target on 13 of 14 (the 14th had launched at it
from beyond its reach). Client 15.535.29 agrees: a Mega Minion walking to a Goblin Hut takes the wave member on its 8th
frame, and attacking Musketeers and Minions keep a leaving Hog Rider against a nearer Cannon. Today's engine holds the
walking troop's target too.

THE SCENE. A blue Archer at (3500, 9000) walks up the left lane and a red Giant at (3500, 21000) walks down it. The
Archer takes the Giant when it comes into sight and walks on toward it; for a few ticks the Giant stands past the
Archer's keep reach (Range 5000 + radii 500 and 750 = 6250, + 25) and inside its hold (+ 500). On the first of those
ticks red plays a Knight at (6500, 17500), nearer to the Archer than the Giant and inside its sight (5500 + 500 + 500).
Under the new arm the walking Archer takes the Knight while the Giant still stands inside the hold. Under the old arm
it keeps the Giant.

WHY THE CONTROLS ARE HERE.
- With no Knight, both arms: the walking Archer keeps the Giant on every tick it stands inside the hold, and then
  attacks it. The rescan returns the same target, so an implementation that drops a target past the keep reach fails.
- The hold of an attacker IN ITS ATTACK, both arms: a red Musketeer whose blue Hog Rider runs out of its reach keeps
  it against a nearer blue Cannon while the Hog stands inside the hold (tests/test_reach_loss_switch.py's scene). An
  implementation that drops the hold for every troop fails.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `projectile_hold_while_walking` -- client_troop_in_attack still holds a walking troop's target past its keep
    reach: test_a_walking_troop_takes_a_nearer_enemy_inside_its_hold.
"""

from __future__ import annotations

import json
import math
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.PROJECTILE_HOLD_SCOPE"
NEW_ARM, OLD_ARM = "client_troop_in_attack", "every_tick"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
IDLE = 0
ARCHER_AT, GIANT_AT, KNIGHT_AT = (3500, 9000), (3500, 21000), (6500, 17500)
#: the Archer's reach on the Giant (Range 5000 + radii 500 and 750), its keep extension and its hold past reach
REACH_ON_GIANT, KEEP_EXTENSION, HOLD = 5000 + 500 + 750, 25, 500
#: the Archer's sight on the Knight, SightRange 5500 + radii 500 and 500
SIGHT_ON_KNIGHT = 5500 + 500 + 500
KNIGHT_R, GIANT_R = 500, 750
#: the Musketeer control: a red Musketeer south of a blue Hog Rider running north, a blue Cannon on the blue bank
MUSKETEER_AT, HOG_AT, CANNON_AT = (14500, 12500), (14500, 17000), (10000, 14000)
REACH_ON_HOG = 6000 + 500 + 600
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"


def overrides(arm) -> dict:
    """`arm` None runs the build's own value."""
    return {KEY: json.dumps(arm)} if arm is not None else {}


def inside_hold(d: float, reach: int) -> bool:
    """Past the keep reach and inside the hold."""
    return reach + KEEP_EXTENSION < d <= reach + HOLD


def walk(arm, knight: bool = True, ticks: int = 90):
    """The Archer scene. One row per tick, the state after it: the Archer's target ("giant", "knight", other or None),
    its attack phase, its distance from the Giant and from the Knight (None before the Knight exists), whether the
    Knight was played before this tick, and the Archer's and the Knight's positions (subtiles)."""
    b = royalesim.Battle(["Archer", "Giant", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[0] * 8, [2] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [(0, 0, ARCHER_AT[0] * SUB, ARCHER_AT[1] * SUB, -1), (1, 1, GIANT_AT[0] * SUB, GIANT_AT[1] * SUB, -1)],
    )
    ents = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}
    archer = next(u for u, e in ents.items() if e[F["team"]] == 0)
    giant = next(u for u, e in ents.items() if e[F["team"]] == 1)
    rows, played = [], False
    now = ents
    for _ in range(ticks):
        a, g = now.get(archer), now.get(giant)
        cmds = []
        if (
            knight
            and not played
            and a is not None
            and g is not None
            and a[F["target_uid"]] == giant
            and a[F["attack_phase"]] == IDLE
            and inside_hold(dist(a, g), REACH_ON_GIANT)
        ):
            cmds = [(1, 0, KNIGHT_AT[0] * SUB, KNIGHT_AT[1] * SUB)]
        if cmds:
            out = b.step(cmds, 1)
            assert out, "the scene drifted: the Knight play returned nothing"
            assert out[0][1] == 0, f"the scene drifted: the Knight play was refused: {out}"
            played = True
        else:
            b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}
        a, g = now.get(archer), now.get(giant)
        if a is None or g is None:
            break
        k = next((e for u, e in now.items() if e[F["team"]] == 1 and u != giant), None)
        t = a[F["target_uid"]]
        target = (
            "giant"
            if t == giant
            else "knight"
            if (k is not None and t == k[F["uid"]])
            else (None if t < 0 else "other")
        )
        rows.append(
            {
                "target": target,
                "phase": a[F["attack_phase"]],
                "dg": dist(a, g),
                "dk": dist(a, k) if k is not None else None,
                "played": played,
                "at": (a[F["x"]], a[F["y"]]),
                "knight_at": (k[F["x"]], k[F["y"]]) if k is not None else None,
            }
        )
    return rows


def dist(a, b) -> float:
    return math.dist((a[F["x"]], a[F["y"]]), (b[F["x"]], b[F["y"]])) / SUB


def knight_tick(rows, what: str) -> int:
    """The index of the first row with the Knight on the board, its preconditions checked on the row before (the
    start-of-tick state of its first Target phase)."""
    i = next((i for i, r in enumerate(rows) if r["dk"] is not None), None)
    assert i is not None, f"{what}: the scene drifted: the Knight never came"
    assert i > 0, f"{what}: the scene drifted: the Knight stood on the board from the start"
    p = rows[i - 1]
    assert p["target"] == "giant", f"{what}: the scene drifted: before the Knight the Archer's target was {p['target']}"
    assert p["phase"] == IDLE, f"{what}: the scene drifted: before the Knight the Archer was in its attack"
    assert inside_hold(p["dg"], REACH_ON_GIANT), (
        f"{what}: the scene drifted: the Giant stood {p['dg']:.1f} from the Archer, not inside its hold"
    )
    dk = math.dist(p["at"], rows[i]["knight_at"]) / SUB  # the Knight's creation point, the Archer at the tick's start
    assert dk <= SIGHT_ON_KNIGHT, f"{what}: the scene drifted: the Knight stands {dk:.1f} away, out of sight"
    assert dk < p["dg"], (
        f"{what}: the scene drifted: the Knight ({dk:.1f}) is not nearer than the Giant ({p['dg']:.1f})"
    )
    assert dk - KNIGHT_R < p["dg"] - GIANT_R, f"{what}: the scene drifted: the Knight is not nearer by centre - radius"
    return i


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_a_walking_troop_takes_a_nearer_enemy_inside_its_hold(arm):
    rows = walk(arm)
    i = knight_tick(rows, arm)
    switch = next((j for j in range(i, len(rows)) if rows[j]["target"] == "knight"), None)
    kept = (
        f"{arm}: the walking Archer kept the Giant {rows[i - 1]['dg']:.1f} away (inside its hold) with the Knight "
        f"{math.dist(rows[i - 1]['at'], rows[i]['knight_at']) / SUB:.1f} away in sight; it took the Knight on row "
        f"{switch} (the Knight came on row {i})"
    )
    assert switch is not None, kept
    assert switch <= i + 1, kept
    assert inside_hold(rows[switch - 1]["dg"], REACH_ON_GIANT), (
        f"{arm}: the switch on row {switch} did not come while the Giant stood inside the hold"
    )
    assert rows[switch - 1]["phase"] == IDLE, f"{arm}: the switch on row {switch} came in the Archer's attack"


@pytest.mark.parametrize("arm", [OLD_ARM])
def test_the_old_arm_holds_the_walking_troop_s_target(arm):
    rows = walk(arm)
    i = knight_tick(rows, arm)
    held = [
        j
        for j in range(i, len(rows))
        if rows[j - 1]["phase"] == IDLE
        and rows[j - 1]["target"] == "giant"
        and inside_hold(rows[j - 1]["dg"], REACH_ON_GIANT)
    ]
    assert held, (
        f"{arm}: the scene drifted: no tick with the Knight on the board started with the Giant inside the hold"
    )
    taken = [j for j in held if rows[j]["target"] != "giant"]
    assert not taken, f"{arm}: the walking Archer let the Giant go inside its hold on rows {taken}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_with_no_nearer_enemy_the_walker_keeps_its_target(arm):
    rows = walk(arm, knight=False)
    band = [
        j
        for j in range(1, len(rows))
        if rows[j - 1]["phase"] == IDLE
        and rows[j - 1]["target"] == "giant"
        and inside_hold(rows[j - 1]["dg"], REACH_ON_GIANT)
    ]
    assert len(band) >= 2, f"{arm}: the scene drifted: the Archer walked inside its hold on {len(band)} ticks"
    lost = [j for j in band if rows[j]["target"] != "giant"]
    assert not lost, f"{arm}: with no other enemy the walking Archer let the Giant go on rows {lost}"
    after = [r for r in rows[band[-1] : band[-1] + 10] if r["target"] == "giant" and r["phase"] != IDLE]
    assert after, f"{arm}: the Archer never attacked the Giant after walking up to it"


def musketeer(arm, ticks: int = 60):
    """test_reach_loss_switch's projectile scene. One row per tick: the Musketeer's target, its attack phase and its
    distance from the Hog."""
    cards = ["Musketeer", "HogRider", "Cannon"]
    b = royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(
        0,
        [[0] * 8, [1] * 8],
        0,
        200,
        [10_000, 10_000],
        None,
        [
            (1, 0, MUSKETEER_AT[0] * SUB, MUSKETEER_AT[1] * SUB, -1),
            (0, 1, HOG_AT[0] * SUB, HOG_AT[1] * SUB, -1),
            (0, 2, CANNON_AT[0] * SUB, CANNON_AT[1] * SUB, -1),
        ],
    )
    uid = {
        cards[e[F["card_id"]]]: e[F["uid"]] for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0
    }
    names = {uid["HogRider"]: "hog", uid["Cannon"]: "cannon"}
    rows = []
    for _ in range(ticks):
        b.step([], 1)
        now = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"]}
        m, h = now.get(uid["Musketeer"]), now.get(uid["HogRider"])
        if m is None or h is None:
            break
        rows.append({"target": names.get(m[F["target_uid"]]), "phase": m[F["attack_phase"]], "dh": dist(m, h)})
    return rows


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_troop_in_its_attack_holds_a_leaving_target(arm):
    rows = musketeer(arm)
    band = [
        j
        for j in range(1, len(rows))
        if rows[j - 1]["target"] == "hog" and inside_hold(rows[j - 1]["dh"], REACH_ON_HOG)
    ]
    assert len(band) >= 3, f"{arm}: the scene drifted: the Hog stood inside the hold on {len(band)} ticks"
    walking = [j for j in band if rows[j - 1]["phase"] == IDLE]
    assert not walking, f"{arm}: the scene drifted: the Musketeer was not in its attack on rows {walking}"
    lost = [(j, rows[j]["target"]) for j in band if rows[j]["target"] != "hog"]
    assert not lost, f"{arm}: the Musketeer in its attack let the leaving Hog go inside its hold: {lost}"


def test_the_shipped_value_is_the_old_arm():
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["targeting"]["PROJECTILE_HOLD_SCOPE"]
    assert entry["value"] == OLD_ARM
    assert entry["candidates"] == [OLD_ARM, NEW_ARM]
