"""The underground walk's data and its play through the protocol (tools/extract_cards.py `summon_card`
`can_deploy_on_enemy_side` / `touchdown_limited_deploy`; card.rs `SpawnPathfindDef`; state.rs `phase_tunnel`;
movement.SPAWN_PATHFIND_STATES and its sibling keys).

WHAT THIS PINS, 1: THE EXPORT. CanDeployOnEnemySide and TouchdownLimitedDeploy reach cards.json on exactly the two
15.535.29 card rows that tunnel (the Miner, the Goblin Drill) and on no unit row. The 2018 file carries neither, so
its Miner stays refused (the loader takes a tunnelling card only with CanDeployOnEnemySide).

WHAT THIS PINS, 2: THE WALK, through the protocol (it needs an extension built from this tree). A Miner played on
the enemy side reports status bit 0 (under ground) while it walks, is one entity from its King to the fight, and
comes up on the point the play resolved to. The Goblin Drill's building stands on the point its play resolved to,
and every unit of the play (the dig, the building, the Goblins the building spawns) reports the Goblin Drill's card
id: py.rs `ids_of_indices` follows the chain down, where a unit reached only through a unit used to report -1.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
CARDS_15535 = ROOT / "data" / "derived" / "cards-15.535.json"
CARDS_2018 = ROOT / "data" / "derived" / "cards-2018.json"
FLAGS = ("can_deploy_on_enemy_side", "touchdown_limited_deploy")


def load(p: Path) -> dict:
    if not p.exists():
        pytest.skip(f"SKIPPED, NOT PASSED: {p.relative_to(ROOT)} is absent")
    return json.loads(p.read_text(encoding="utf-8"))


def test_the_enemy_side_flags_are_on_exactly_the_tunnelling_cards():
    doc = load(CARDS_15535)
    tunnelling = sorted(c["name"] for c in doc["cards"] if c.get("spawn_pathfind"))
    assert tunnelling == ["GoblinDrill", "Miner"]
    for key in FLAGS:
        assert sorted(c["name"] for c in doc["cards"] if key in c) == tunnelling, key
        assert all(c[key] is True for c in doc["cards"] if key in c), key
        assert not any(key in u for u in doc["units"].values()), f"a unit row carries {key}"


def test_the_2018_file_carries_neither_flag():
    doc = load(CARDS_2018)
    assert any(c.get("spawn_pathfind") for c in doc["cards"]), "vacuous: the 2018 Miner does not tunnel"
    for key in FLAGS:
        assert not any(key in c for c in doc["cards"]), key


# ---- 2. the walk, through the protocol

DECK = ["Miner", "GoblinDrill", "Knight", "Archer", "Giant", "Minions", "Musketeer", "Valkyrie"]
IDS = list(range(len(DECK)))
MINER, DRILL = DECK.index("Miner"), DECK.index("GoblinDrill")
# ENTITY_FIELDS: 0 uid, 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y, 20 status_flags (bit 0 under ground)
UID, TEAM, CARD, SLOT, X, Y, STATUS = 0, 1, 3, 4, 5, 6, 20


@pytest.fixture(scope="module")
def royalesim():
    rs = pytest.importorskip("royalesim")
    try:
        rs.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    except Exception as e:
        pytest.fail(f"this build refuses the Miner or the Goblin Drill ({e}): rebuild the extension from this tree")
    return rs


def play(rs, slot: int, at: tuple[int, int], ticks: int):
    """Blue plays hand `slot` (the deck's first four, unshuffled) at `at` (native), 200 ticks in with full elixir.
    Returns the resolved point (subtiles) and, per tick, Blue's rows other than its crown towers."""
    sub = rs.SUBTILE_PER_MILLITILE
    b = rs.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [])
    [(_card, reason, _tick, rx, ry)] = b.step([(0, slot, at[0] * sub, at[1] * sub)], 1)
    assert rs.DEPLOY_REASONS[reason] == "OK", rs.DEPLOY_REASONS[reason]
    frames = []
    for _ in range(ticks):
        frames.append([e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 0 and e[SLOT] < 0])
        b.step([], 1)
    return (rx, ry), frames


def test_a_miner_reports_under_ground_while_it_walks_and_comes_up_where_it_was_played(royalesim):
    (rx, ry), frames = play(royalesim, MINER, (3500, 21500), 80)
    seen = [e for f in frames for e in f if e[CARD] == MINER]
    assert len({e[UID] for e in seen}) == 1, "the Miner that came up is not the one that went down"
    first_up = next((k for k, e in enumerate(seen) if not e[STATUS] & 1), None)
    assert first_up is not None, "the Miner never came up"
    assert first_up >= 5, f"the scene drifted: {first_up} frames under ground"
    assert all(e[STATUS] & 1 for e in seen[:first_up]), "a frame before it came up is not under ground"
    assert not any(e[STATUS] & 1 for e in seen[first_up:]), "the Miner went back under ground"
    assert (seen[first_up][X], seen[first_up][Y]) == (rx, ry), "the Miner came up off the point its play resolved to"


def test_the_drills_building_and_its_goblins_report_the_drills_card(royalesim):
    (rx, ry), frames = play(royalesim, DRILL, (9000, 10000), 200)
    rows = [e for f in frames for e in f]
    stray = sorted({(e[UID], e[CARD]) for e in rows if e[CARD] != DRILL})
    assert not stray, f"a unit of the Goblin Drill's play reports another card id: {stray}"
    assert any(not e[STATUS] & 1 and (e[X], e[Y]) == (rx, ry) for e in rows), "no building stands on the resolved point"
    # the dig, the building and at least one Goblin: three units, the last reached only through the building
    assert len({e[UID] for e in rows}) >= 3, "the scene drifted: no Goblin came out of the building in 200 ticks"
