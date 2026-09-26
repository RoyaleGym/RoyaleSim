"""An attacker whose target dies waits six ticks before it takes the next one, and the list arm
(combat.POST_KILL_RETARGET_WAIT = client16402_measured_list).

WHAT THIS PINS. In the 16.402 corpus and on client 15.535.29, a Knight whose victim dies keeps
attacking-state with NO target for six ticks, standing still, even with another enemy already in
range, and takes that enemy on the loss + 6. The engine took it on the loss + 1, so every direct
hitter walked (or swung) five ticks early after every kill, and every Prince charge that followed a
kill started five ticks early. The ledger entry carries the evidence.

WHICH ARM. The list arm, client16402_measured_list, makes the units it names wait; it shipped first.
Since 2026-09-25 the shipped arm is client16402_attack_finish, the condition that explains the list
(tests/test_post_kill_retarget_condition.py pins who waits under it). The list arm stays a candidate,
so this file keeps testing it, pinned BY NAME through the battle's own calibration, never through the
shipped value. The Knight's six ticks and the enemy in range it ignores are asked of both arms: a
Knight's attack progress is past 0 at its own kill, so the shipped arm holds it too. The Goblin and
the Musketeer are the list arm's own questions (a name match, and a unit the list does not name).

WHY THE CONTROLS ARE HERE. "6" alone would pass for a build that delays every retarget. So the same
file asks the Musketeer, which is NOT on the list, for its unchanged 1, and asks the none arm (the
engine before the wait) for 1 on the Knight too: three scenarios, two numbers.

THE SCENARIO. Mid-field, out of every crown tower's range: a blue attacker at (9, 12) tiles, and two
red Skeletons in front of it, the second already within reach when the first dies. A Skeleton is one
hit for either attacker at the default levels.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

TILE = 18_000
#: explicit, because catalogue ids are positional
DECK = ["Skeletons", "Giant", "Musketeer", "Goblins", "Knight", "Minions", "Cannon", "Tesla"]
IDS = list(range(len(DECK)))
SK, MU, GO, KN = 0, 2, 3, 4
ATTACKER = (9 * TILE, 12 * TILE)
VICTIMS = [(9 * TILE, int(13.3 * TILE)), (int(9.8 * TILE), int(13.5 * TILE))]
KEY = "combat.POST_KILL_RETARGET_WAIT"


def arm(name: str) -> dict:
    """The shipped value with only its arm swapped: the unit list and the ticks come from the ledger the
    module was built with, so the test never carries a second copy of the list."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    value = ledger.get("combat", {}).get("POST_KILL_RETARGET_WAIT", {}).get("value")
    if value is None:
        pytest.fail(f"{KEY} is not in the compiled-in ledger: this build predates the key")
    return {KEY: json.dumps({**value, "arm": name})}


LIST_ARM, SHIPPED_ARM, NONE_ARM = "client16402_measured_list", "client16402_attack_finish", "none"
#: the arms under which a Knight's own kill holds it six ticks: the list names the Knight, and under the
#: condition its attack progress is past 0 at the kill
WAITING_ARMS = [LIST_ARM, SHIPPED_ARM]
# ENTITY_FIELDS: 1 team, 4 tower_slot, 5 x, 6 y, 15 target_uid
TEAM, SLOT, X, Y, TARGET = 1, 4, 5, 6, 15


def first_kill(attacker, overrides: str):
    """(loss tick, next-target tick, attacker positions over the wait, red uids left at the loss)
    for the attacker's first kill."""
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=arm(overrides))
    units = [(0, attacker, *ATTACKER, -1)] + [(1, SK, *v, -1) for v in VICTIMS]
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, units)
    first, loss, positions = None, None, []
    for t in range(200):
        rows = json.loads(b.state_json())["entities"]
        me = next(e for e in rows if e[TEAM] == 0 and e[SLOT] < 0)
        tgt = me[TARGET]
        if first is None and tgt != -1:
            first = tgt
        elif first is not None and loss is None and tgt == -1:
            loss = t
            red_left = sorted(e[0] for e in rows if e[TEAM] == 1 and e[SLOT] < 0)
        if loss is not None:
            positions.append((me[X], me[Y]))
            if tgt not in (-1, first):
                return loss, t, positions, red_left
        b.step([], 1)
    raise AssertionError(f"no retarget within 200 ticks (first target {first}, loss {loss})")


@pytest.mark.parametrize("arm_name", WAITING_ARMS)
def test_a_knight_waits_six_ticks_after_its_kill(arm_name):
    loss, nxt, positions, red_left = first_kill(KN, arm_name)
    assert red_left, "the scenario lost its point: no second enemy was left when the first died"
    took = f"{arm_name}: the Knight took its next target {nxt - loss} ticks after the loss; the game takes 6"
    assert nxt - loss == 6, took
    assert len(set(positions[:-1])) == 1, f"{arm_name}: the Knight moved while it waited: {positions}"


def test_a_goblins_member_waits_like_the_listed_unit_it_is():
    """A Goblins member carries the Goblins CARD, while the list names the game's unit, Goblin_Stab.
    Matching the card's name made every Goblin retarget on the loss + 1 under the list arm; the
    Knight above could not see it, because the Knight card and its unit share a name."""
    loss, nxt, _, _ = first_kill(GO, LIST_ARM)
    assert nxt - loss == 6, f"the Goblin took its next target {nxt - loss} ticks after the loss; the game takes 6"


@pytest.mark.parametrize("arm_name", WAITING_ARMS)
def test_the_wait_ignores_an_enemy_already_in_range(arm_name):
    """The second Skeleton stands within the Knight's reach at the loss; it is still ignored for
    the six ticks."""
    loss, nxt, _, red_left = first_kill(KN, arm_name)
    assert len(red_left) == 1, "the second Skeleton was not on the board at the loss"
    assert nxt - loss == 6, f"{arm_name}: the Knight took the Skeleton in range {nxt - loss} ticks after the loss"


def test_an_unlisted_musketeer_is_unchanged():
    """Not on the list: under the list arm it takes the none arm's 1."""
    new = first_kill(MU, LIST_ARM)
    old = first_kill(MU, NONE_ARM)
    moved = f"the Musketeer's retarget moved: {new[1] - new[0]} vs {old[1] - old[0]}"
    assert new[1] - new[0] == old[1] - old[0] == 1, moved


def test_the_none_arm_gives_the_knight_one():
    loss, nxt, _, _ = first_kill(KN, NONE_ARM)
    assert nxt - loss == 1, f"the none arm must reproduce the engine before the wait (1 tick); it gave {nxt - loss}"
