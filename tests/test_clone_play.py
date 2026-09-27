"""The Clone, the Vines and the Void through the Python binding (card.rs `SpellShape::Clone`,
`StrikePick::RankedCatches`, `StrikePick::CountTiers`; py.rs `clone_states`).

WHAT THIS PINS. A catalogue that names the three builds; a Clone played from the hand on a Knight puts one copy on the
board on the cast tick, which `clone_states` names (cloned, no window) and which reports its original's card id (the
client's own records carry the Clone card's id for it: a harness that attributes a copy to the cast reads
`clone_states`, not the card id); a Vines played on a Balloon grounds it, and `clone_states` shows its window counting
down.

PLANTS. Each is a cfg in the engine source. Prove one on a plant build of the module in a scratch venv
(`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run this file:
  * `clone_full_hp` -- a copy keeps its row's hitpoints: test_a_clone_from_the_hand_puts_one_copy_on_the_board.
  * `grounding_ignored` does NOT redden test_a_vines_catch_grounds_a_balloon: the window is written whatever reads it.
    `vines_skips_hidden` does not either. The Rust suite (crates/royalesim/tests/vines.rs) holds what the window does.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
#: the step the card is played on, past the opening deploy lockout (as tests/test_area_effect_clock.py plays)
CAST_STEP = 10


def battle(cards: list[str]):
    try:
        return royalesim.Battle(cards, [[0, 1, 2], [0, 1, 2]])
    except ValueError as e:
        pytest.fail(f"this royalesim build does not load {cards} ({e}): rebuild the extension from this tree")


def units(b, team: int) -> dict:
    rows = json.loads(b.state_json())["entities"]
    return {e[F["uid"]]: e for e in rows if e[F["team"]] == team and e[F["tower_slot"]] < 0}


def test_the_three_load_together():
    b = battle(["Clone", "Vines", "DarkMagic", "Knight"])
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [])
    assert b.clone_states() == [], "nothing is a copy before the battle starts"


def test_a_clone_from_the_hand_puts_one_copy_on_the_board():
    b = battle(["Clone", "Knight"])
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [(0, 1, 9000 * SUB, 9000 * SUB, -1)])
    (knight,) = units(b, 0)
    b.step([], CAST_STEP)
    at = units(b, 0)[knight]
    out = b.step([(0, 0, at[F["x"]], at[F["y"]])], 1)
    assert out[0][1] == 0, f"the Clone play was refused: {out}"
    now = units(b, 0)
    states = b.clone_states()
    assert len(states) == 1, f"clone_states: {states}"
    copy, cloned, window = states[0]
    assert (cloned, window) == (True, 0)
    assert set(now) == {knight, copy}, f"the board holds {sorted(now)}"
    assert (now[copy][F["hp"]], now[copy][F["x"]], now[copy][F["y"]]) == (1, now[knight][F["x"]], now[knight][F["y"]])
    assert now[copy][F["card_id"]] == now[knight][F["card_id"]], "a copy reports its original's card id"


def test_a_vines_catch_grounds_a_balloon():
    b = battle(["Vines", "Balloon"])
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [(1, 1, 9000 * SUB, 15600 * SUB, -1)])
    (balloon,) = units(b, 1)
    b.step([], CAST_STEP)
    at = units(b, 1)[balloon]
    out = b.step([(0, 0, at[F["x"]], at[F["y"]])], 0)
    assert out[0][1] == 0, f"the Vines play was refused: {out}"
    got = []
    for _ in range(24):
        b.step([], 1)
        got.append({uid: window for uid, _, window in b.clone_states()}.get(balloon, 0))
    first = next(k for k, w in enumerate(got) if w > 0)
    assert got[first] > got[first + 1] > 0, f"the window does not count down: {got}"
