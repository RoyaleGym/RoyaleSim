"""The command delay across the Python boundary (state.rs `BattleConfig::command_delay_ticks`).

``Battle.set_command_delay_ticks(blue, red)`` sets it for every battle the object starts from the next ``reset``;
a play then waits in ``pending_commands`` and runs k ticks on, checked again in full; a waiting card reads reason 18
(CardPending). With no delay, nothing waits: the default is the engine as it always ran.
"""

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = 18
DECK = ["Knight", "Archers", "Fireball", "Musketeer", "MiniPekka", "Skeletons", "Cannon", "Zap"]


def battle(blue, red):
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    b.set_command_delay_ticks(blue, red)
    ids = list(range(len(DECK)))
    b.reset(0, [ids, ids], 0, 0, [10000, 10000], None, [])
    b.step([], 90)
    return b


def test_default_is_no_delay():
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    assert b.command_delay_ticks() == (0, 0)
    ids = list(range(len(DECK)))
    b.reset(0, [ids, ids], 0, 0, [10000, 10000], None, [])
    b.step([], 90)
    b.step([(0, 0, 9500 * SUB, 8500 * SUB)], 1)
    assert b.pending_commands(0) == []


def test_a_play_waits_then_runs():
    b = battle(22, 0)
    out = b.step([(0, 0, 9500 * SUB, 8500 * SUB)], 0)
    assert out[0][1] == 0, f"the play is accepted: {out}"
    pending = b.pending_commands(0)
    assert len(pending) == 1, pending
    assert pending[0][0] == "deploy", pending
    assert pending[0][4] == 22, f"22 ticks left: {pending}"
    assert b.check_deploy(0, 0, 9500 * SUB, 9500 * SUB) == 18, "the waiting card reads CardPending"
    assert royalesim.DEPLOY_REASONS[18] == "CARD_PENDING"
    blue = json.loads(b.state_json())["players"][0]
    assert blue["pending_cost"] == 3, blue["pending_cost"]
    assert len(blue["pending"]) == 1, blue["pending"]
    assert blue["pending"][0][0] == 0, "a play"
    assert blue["pending"][0][4] == 22, "22 ticks left"
    b.step([], 22)
    assert len(b.pending_commands(0)) == 1, "it has not run after 22 ticks: it runs at the top of the next"
    b.step([], 1)
    assert b.pending_commands(0) == []
    assert b.commands_run() == [(0, "deploy", 0)], "it ran, accepted"
