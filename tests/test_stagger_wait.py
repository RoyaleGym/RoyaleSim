"""A summon member waiting out its deploy stagger can be neither targeted nor pushed (formation.STAGGER_WAIT).

WHAT THIS PINS. A multi-unit deploy's members do not all start deploying at once: member k waits k x
SummonDeployDelay before its own DeployTime starts (formation.DEPLOY_STAGGER, already shipped). In the
16.402 corpus, a member in that wait is never the target of anything (0 of 972,681 target rows; in all
27 acquisitions with a closer waiting enemy, the attacker took a farther one) and never moves (1083 of
1083 consecutive-frame pairs still). The engine counted the wait as part of the deploy, so a waiting
member was targeted and pushed like any deploying unit. Once the wait ends, the member is an ordinary
deploying unit again: targetable (2,315 truth target rows) and pushable (1505 of 17530 pairs move).

A GOBLIN WAITS while its deploy timer exceeds its own DeployTime (1000 ms = 20 ticks): members 1, 2 and 3
of a Goblins deploy wait 4, 8 and 12 ticks, member 0 never does.

WHY THE CONTROLS ARE HERE. "Never targeted" and "never moves" both pass for a build that makes EVERY
deploying unit untargetable and immovable, and "never targeted" passes for a Musketeer that targets
nothing. So the same file asks, under the new arm, for a deploying member that is NOT waiting to be
targeted and pushed, and asks the old arm for today's behaviour, which is also what shows that each
scenario has a point.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
#: explicit, because catalogue ids are positional
DECK = ["Goblins", "Knight", "Musketeer", "Archer", "Giant", "Minions", "Cannon", "Tesla"]
IDS = list(range(len(DECK)))
GOB, KN, MU, CAN = 0, 1, 2, 6
#: the Goblin's own DeployTime, 1000 ms at 50 ms per tick
OWN_DEPLOY_TICKS = 20
#: a blue Goblins tap just short of the river, native units; its four members land on
#: (9761, 12239), (9761, 13761), (8239, 13761) and (8239, 12239) with nothing near them
TAP = (9000, 13000)
MEMBER_0, MEMBER_2 = (9761, 12239), (8239, 13761)
KEY = "formation.STAGGER_WAIT"
NEW_ARM, OLD_ARM = "client16402_untargetable_immovable", "deploying"
# ENTITY_FIELDS: 0 uid, 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y, 11 deploy_ticks, 15 target_uid
UID, TEAM, CARD, SLOT, X, Y, DEPLOY, TARGET = 0, 1, 3, 4, 5, 6, 11, 15


def goblins_frames(arm: str, spawns: list, ticks: int = 30) -> list:
    """Each tick's entity rows, from the tick after the tap resolves. `spawns` is reset's list of
    (team, card, x, y, hp) in native units."""
    overrides = {KEY: json.dumps(arm)}
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    units = [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns]
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, units)
    b.step([], 1)
    played = b.step([(0, GOB, TAP[0] * SUB, TAP[1] * SUB)], 1)
    assert played, "the Goblins tap did not resolve"
    assert played[0][0] == GOB, f"another card resolved: {played}"
    frames = []
    for _ in range(ticks):
        frames.append(json.loads(b.state_json())["entities"])
        b.step([], 1)
    return frames


def goblins(rows: list) -> list:
    """Blue Goblins rows in creation order (uid order), so index k is member k."""
    return sorted((e for e in rows if e[TEAM] == 0 and e[CARD] == GOB and e[SLOT] < 0), key=lambda e: e[UID])


def waiting(e: list) -> bool:
    return e[DEPLOY] > OWN_DEPLOY_TICKS


def at(e: list) -> tuple:
    """A row's position in native units."""
    return (e[X] // SUB, e[Y] // SUB)


def musketeer_scenario(arm: str) -> list:
    """(tick, the red Musketeer's target, that target's deploy_ticks or None) per tick. The Musketeer
    stands across the river at (9000, 17500), in sight of the formation."""
    out = []
    for t, rows in enumerate(goblins_frames(arm, [(1, MU, 9000, 17500, -1)])):
        mu = next(e for e in rows if e[TEAM] == 1 and e[CARD] == MU and e[SLOT] < 0)
        tgt = next((e for e in goblins(rows) if e[UID] == mu[TARGET]), None)
        out.append((t, mu[TARGET], None if tgt is None else tgt[DEPLOY]))
    return out


def moves_while(frames: list, member: int, pred) -> list:
    """(tick, from, to) for every consecutive pair in which member `member` satisfies `pred` in both
    frames and its position changed."""
    out = []
    for t in range(1, len(frames)):
        a, b = goblins(frames[t - 1]), goblins(frames[t])
        if member >= min(len(a), len(b)):
            continue
        a, b = a[member], b[member]
        if a[UID] == b[UID] and pred(a) and pred(b) and (a[X], a[Y]) != (b[X], b[Y]):
            out.append((t, at(a), at(b)))
    return out


def test_a_waiting_member_is_never_targeted():
    seen = musketeer_scenario(NEW_ARM)
    bad = [s for s in seen if s[2] is not None and s[2] > OWN_DEPLOY_TICKS]
    assert not bad, f"the Musketeer targeted a Goblin still waiting out its stagger (tick, uid, deploy_ticks): {bad}"
    assert any(s[2] is not None for s in seen), f"the Musketeer never targeted a Goblin, so nothing was tested: {seen}"


def test_a_deploying_member_that_is_not_waiting_is_still_targeted():
    """The rule is the WAIT, not the deploy: a deploying member past its wait is targetable."""
    seen = musketeer_scenario(NEW_ARM)
    hits = [s for s in seen if s[2] is not None and 0 < s[2] <= OWN_DEPLOY_TICKS]
    assert hits, f"no deploying, non-waiting Goblin was targeted: {seen}"


def test_a_waiting_member_is_not_pushed():
    """A blue Knight stands on member 2's landing point. Member 2 waits 8 ticks; while it waits it stays
    exactly where the same deploy puts it with nothing near it."""
    frames = goblins_frames(NEW_ARM, [(0, KN, *MEMBER_2, -1)], ticks=12)
    first = goblins(frames[0])[2]
    assert waiting(first), f"member 2 is not waiting on its first frame (deploy_ticks {first[DEPLOY]}): no point left"
    assert at(first) == MEMBER_2, f"member 2 was displaced on its first frame: {at(first)}"
    moved = moves_while(frames, 2, waiting)
    assert not moved, f"member 2 moved while waiting out its stagger (tick, from, to): {moved}"


def test_a_deploying_member_that_is_not_waiting_is_still_pushed():
    """A blue Cannon stands on member 0's landing point. Member 0 never waits; it is pushed off the
    Cannon while it deploys, under both arms."""
    frames = goblins_frames(NEW_ARM, [(0, CAN, *MEMBER_0, -1)], ticks=12)
    moved = moves_while(frames, 0, lambda e: 0 < e[DEPLOY] <= OWN_DEPLOY_TICKS)
    assert moved, "member 0 was not pushed off the Cannon while deploying: the rule reached past the wait"


def test_the_old_arm_is_todays_engine():
    """Today's engine: a waiting member is targeted and pushed like any deploying unit. This is also
    what shows both scenarios put a waiting member where the rule matters."""
    seen = musketeer_scenario(OLD_ARM)
    hits = [s for s in seen if s[2] is not None and s[2] > OWN_DEPLOY_TICKS]
    assert hits, f"the old arm never targeted a waiting Goblin: {seen}"
    frames = goblins_frames(OLD_ARM, [(0, KN, *MEMBER_2, -1)], ticks=12)
    first = goblins(frames[0])[2]
    moved = moves_while(frames, 2, waiting) or (waiting(first) and at(first) != MEMBER_2)
    assert moved, "the old arm did not push the waiting member 2 off the Knight"
