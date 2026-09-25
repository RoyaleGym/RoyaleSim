"""An experiment's calibration reaches the battle, and says so on every frame.

WHAT THIS PINS. `Battle(calibration_overrides=...)` replaces ledger values for every battle
the object starts, so one rule can be reverted without editing `data/calibration.json` --
whose edit is a stale window for every session's engine. The Rust side proves the override
parses and changes only what it names (py.rs `a_calibration_override_changes_only_...`).
What only this file can prove is the PLUMBING: that the parsed calibration is the one
`reset` actually runs, rather than one that loads and is then dropped.

So the observable is behaviour, not a getter. `match.DEPLOY_LOCKOUT_TICKS` refuses every
deploy before tick 90, and the override to 0 must make a tick-0 deploy land where the
shipped engine refuses it. A getter that echoed the argument back would pass a getter test
with the override never applied.

WHY EVERY FRAME CARRIES IT. `build_digest` hashes the COMPILED-IN ledger, so it reads the
same with or without an override. A frame that did not say it was overridden could be
quoted as the shipped engine's behaviour, and nothing on it would contradict the quote.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

#: explicit, because catalogue ids are positional
DECK = ["Knight", "Archer", "Giant", "Minions", "Fireball", "Cannon", "Zap", "Musketeer"]
IDS = list(range(len(DECK)))
TILE = 18_000
SLOTS = [[0, 1, 2], [0, 1, 2]]


def started(**kw):
    b = royalesim.Battle(card_names=DECK, slot_of_k=SLOTS, **kw)
    b.reset(0, [IDS, IDS], 0, 0, [10_000, 10_000], None, [])  # start_tick 0: inside the lockout
    return b


def deploy_at_tick_0(b) -> str:
    """Play hand slot 0 (the Knight: no shuffle, so the hand is the deck's first four) at
    tick 0, and return the engine's own reason by NAME -- "OK" or why not."""
    assert json.loads(b.state_json())["tick"] == 0
    [(_card, reason, *_)] = b.step([(0, 0, 9 * TILE, 10 * TILE)], 1)
    return royalesim.DEPLOY_REASONS[reason]


def test_the_shipped_engine_refuses_a_tick_0_deploy():
    """The control. Without it the override test below cannot tell an override that works
    from a lockout that was never there."""
    assert deploy_at_tick_0(started()) == "TOO_EARLY"


def test_an_override_reaches_the_battle_and_changes_what_it_names():
    b = started(calibration_overrides={"match.DEPLOY_LOCKOUT_TICKS": json.dumps(0)})
    got = deploy_at_tick_0(b)
    assert got == "OK", (
        f"the lockout override parsed but did not reach the battle: a tick-0 deploy was "
        f"answered {got}, so `reset` is running the ledger's calibration and not the experiment's"
    )


def test_every_frame_of_an_overridden_battle_says_so_and_no_other_frame_does():
    plain = started()
    assert "calibration_overrides" not in json.loads(plain.state_json())
    assert plain.calibration_overrides() == {}

    b = started(calibration_overrides={"combat.RETARGET_PROGRESS": json.dumps("reset_always")})
    assert json.loads(b.state_json())["calibration_overrides"] == {"combat.RETARGET_PROGRESS": "reset_always"}
    assert b.calibration_overrides() == {"combat.RETARGET_PROGRESS": '"reset_always"'}
    b.step([], 5)
    assert "calibration_overrides" in json.loads(b.state_json()), "a later frame dropped the marker"


@pytest.mark.parametrize(
    ("overrides", "why"),
    [
        ({"match.NOT_A_KEY": "0"}, "not a key in the ledger"),
        ({"nodot": "0"}, "section.KEY"),
        ({"match.DEPLOY_LOCKOUT_TICKS": "zero"}, "is not JSON"),
        ({"combat.RETARGET_PROGRESS": json.dumps("no_such_arm")}, "does not load"),
    ],
)
def test_an_override_it_cannot_honour_is_refused_at_construction(overrides, why):
    """A typo that ran the shipped value would report an experiment done that never ran."""
    with pytest.raises(ValueError, match=why):
        royalesim.Battle(card_names=DECK, slot_of_k=SLOTS, calibration_overrides=overrides)
