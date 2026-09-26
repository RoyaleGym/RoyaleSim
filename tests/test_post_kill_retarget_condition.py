"""The post-kill retarget wait decided by the attack-finish condition (combat.POST_KILL_RETARGET_WAIT =
client16402_attack_finish).

WHAT THIS PINS. A unit whose target is removed takes its next target after 1 tick if it is one of the four units
named in value.attack_finish_override_units (cards that set OverrideAttackFinishTime; the column marks three more,
which the engine does not read yet), or its attack progress is 0 on the loss tick, or it has a Projectile and its
victim was already doomed (the homing shots flying at the victim covered its hitpoints on its last live tick).
Otherwise it waits the 250 ms attack-finish time: 6 ticks. The ledger entry carries the 16.402 evidence (1,138 of
1,152 events).
It is the SHIPPED arm since 2026-09-25: the tests below pin each arm by name through the battle's calibration, and
the last one runs the shipped build with no override at all.

THE PAIR THAT SEPARATES THE ARMS. The same Musketeer, with a second red Skeleton waiting in its reach:
  A. it kills its first Skeleton with its own shot: the shot was in flight at the victim, so the victim was doomed
     -> 1 tick;
  B. a blue Knight kills that Skeleton first, while the Musketeer is winding up with no shot fired: not doomed
     -> 6 ticks.
The list arm (client16402_measured_list, shipped before this one) and none give the Musketeer 1 in both, so B is
the discriminating case, and A is its control. Each scenario asserts its own premise (a shot in flight at the
victim, or none), so a geometry change that loses the point fails loudly rather than passing.

NOT PINNED HERE: the override clause. Valkyrie's own first hit kills its victim with its progress reset, so clause
(b) would give 1 anyway; the four named cards rest on the corpus (Valkyrie 7/7, Bowler 7/7, Princess 3/3,
ElectroWizard 1/1 at 1 tick). Nor is it pinned anywhere else: crates/royalesim/tests/post_kill_wait.rs checks only
that the four names load. The missing test frees a unit by clause (a) alone: another unit kills its target while
its attack progress is past 0 and it has no projectile (1 tick; 6 with its name taken off the list).
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

TILE = 18_000
#: explicit, because catalogue ids are positional
DECK = ["Skeletons", "Giant", "Musketeer", "Archer", "Knight", "Minions", "Cannon", "Tesla"]
IDS = list(range(len(DECK)))
SK, MU, KN = 0, 2, 4
KEY = "combat.POST_KILL_RETARGET_WAIT"
MUSKETEER = (0, MU, 9 * TILE, 9 * TILE, -1)
#: in the Musketeer's reach and away from the Knight: the next target once the first is gone
FAR = (1, SK, int(12.5 * TILE), 12 * TILE, -1)
SCENARIO_A = [MUSKETEER, (1, SK, 9 * TILE, 12 * TILE, -1), FAR]
SCENARIO_B = [MUSKETEER, (0, KN, 9 * TILE, int(12.6 * TILE), -1), (1, SK, 9 * TILE, int(13.3 * TILE), -1), FAR]
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 15 target_uid; a projectile row's index 5 is its target uid
TEAM, CARD, SLOT, TARGET, P_TARGET = 1, 3, 4, 15, 5


def arm(name: str) -> dict:
    """The shipped value with only its arm swapped (the lists come from the ledger the module was built with)."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    value = ledger.get("combat", {}).get("POST_KILL_RETARGET_WAIT", {}).get("value")
    if value is None or "attack_finish_override_units" not in value:
        pytest.fail(f"{KEY} has no attack-finish arm in the compiled-in ledger: this build predates it")
    return {KEY: json.dumps({**value, "arm": name})}


def musketeer_first_loss(units: list, overrides: dict | None) -> dict:
    """The Musketeer's first target, the tick it is lost, the tick of the next one, and the shots flying at the
    victim on its last live tick. `overrides` None runs the shipped build."""
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, units)
    first = loss = None
    flying = []
    shots = 0
    for t in range(100):
        s = json.loads(b.state_json())
        mu = next(e for e in s["entities"] if e[TEAM] == 0 and e[CARD] == MU and e[SLOT] < 0)
        tgt = mu[TARGET]
        if first is None and tgt != -1:
            first = tgt
        elif first is not None and loss is None and tgt == -1:
            loss = t
            shots = sum(1 for p in flying if p[P_TARGET] == first)
        elif loss is not None and tgt not in (-1, first):
            return {"loss": loss, "wait": t - loss, "shots_at_victim": shots}
        flying = s["projectiles"]
        b.step([], 1)
    raise AssertionError(f"no retarget within 100 ticks (first {first}, loss {loss})")


def test_a_doomed_victim_frees_the_musketeer_after_one_tick():
    got = musketeer_first_loss(SCENARIO_A, arm("client16402_attack_finish"))
    assert got["shots_at_victim"] >= 1, f"scenario A lost its point: no Musketeer shot was flying at the victim ({got})"
    assert got["wait"] == 1, f"the Musketeer waited {got['wait']} after a doomed victim; the game takes 1"


def test_a_victim_that_was_not_doomed_holds_the_musketeer_six_ticks():
    got = musketeer_first_loss(SCENARIO_B, arm("client16402_attack_finish"))
    assert got["shots_at_victim"] == 0, f"scenario B lost its point: a shot was flying at the victim ({got})"
    assert got["wait"] == 6, f"the Musketeer waited {got['wait']} after a victim nothing had doomed; the game takes 6"


def test_the_list_arm_gives_the_musketeer_one_either_way():
    """The measured list does not name the Musketeer: 1 in both scenarios, which is what the condition arm corrects."""
    a = musketeer_first_loss(SCENARIO_A, arm("client16402_measured_list"))
    b = musketeer_first_loss(SCENARIO_B, arm("client16402_measured_list"))
    assert (a["wait"], b["wait"]) == (1, 1), f"the list arm gave {a['wait']} and {b['wait']}"


def test_the_none_arm_gives_one():
    b = musketeer_first_loss(SCENARIO_B, arm("none"))
    assert b["wait"] == 1, f"the none arm must reproduce the engine before the wait (1); it gave {b['wait']}"


def test_the_shipped_build_runs_the_condition():
    """No override at all. The compiled-in ledger ships client16402_attack_finish, so the shipped build gives
    scenario B's 6, where the list arm and none give 1, and scenario A's 1."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    shipped = ledger["combat"]["POST_KILL_RETARGET_WAIT"]["value"]["arm"]
    assert shipped == "client16402_attack_finish", f"{KEY} ships {shipped!r}, not the arm this file names as shipped"
    b = musketeer_first_loss(SCENARIO_B, None)
    assert b["shots_at_victim"] == 0, f"scenario B lost its point: a shot was flying at the victim ({b})"
    assert b["wait"] == 6, f"the shipped build freed the Musketeer after {b['wait']}; an undoomed victim holds it 6"
    a = musketeer_first_loss(SCENARIO_A, None)
    assert a["shots_at_victim"] >= 1, f"scenario A lost its point: no Musketeer shot was flying at the victim ({a})"
    assert a["wait"] == 1, f"the shipped build held the Musketeer {a['wait']} after a doomed victim; the game takes 1"
