"""Client 16.635's default crown tower once a princess tower is down (targeting.FALLEN_LANE_TOWER_PICK =
client16635_last_lane, the shipped value), against the client's own engine.

WHAT THIS PINS. Each fixture in tests/data/walk_lane16635 is a constructed scene run in client 16.635.1's own battle
engine, headless: the scene (seed, the client's deck order for that seed, card levels, the crown towers destroyed
before the first update: Blue's princess tower on one side, in three scenes a red one as well, the plays) and the
client's track of the red Knight in it, x, y and hp on every tick it lived. A blue Cannon, Giant or Hog Rider drags the
Knight; once it is dead the Knight walks to its default tower. On the shipped calibration, with client 16.635's card
table, RoyaleSim must reproduce the track exactly, tick for tick, and the Knight must live on the same ticks. The five
cases, one per fixture (`why`):
  * off the lane strips on its own half it keeps its spawn lane's tower, wherever it stands;
  * over the other lane's bridge, or its lane strip, it takes that lane (that tower down: the king);
  * its spawn lane's tower down, the king, whatever it walked over;
  * the lane cells around its own king count for nothing.
The ledger entry (data/calibration.json) carries the full measurement.

PLANTS: `walk_lane_never_moves` -- the walked lane stays the spawn lane: test_the_knight_walks_as_in_the_client
[lane_enemyR_x8000_c11500_14000] and [lane4_corrGC_x8000_c16000_20000_t640]. `king_area_lane_counts`:
[lane3_kingH_x10000_y26000_t300].
"""

from __future__ import annotations

import json
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
DATA = pathlib.Path(__file__).resolve().parent / "data" / "walk_lane16635"
FIXTURES = sorted(DATA.glob("*.json"))
KEY = "targeting.FALLEN_LANE_TOWER_PICK"
#: the scenes ran on client 16.635, so on its card table (docs/api.md, "Card table")
TABLE_16635 = {"cards.CARD_TABLE": json.dumps("client160635007_20261103")}


def knight_track(fx, overrides=None):
    """The red Knight's {tick: [x, y, hp]} (logic units) when the fixture's scene runs on the shipped calibration with
    client 16.635's card table (and `overrides`)."""
    b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]],
                         calibration_overrides={**TABLE_16635, **(overrides or {})})
    ids = {row[0]: i for i, row in enumerate(json.loads(b.catalogue_json()))}
    decks = [[ids[n] for n in side] for side in fx["order"]]
    reset = dict(seed=fx["seed"], decks=decks, shuffle=0, start_tick=0, elixir_milli=[5000, 5000], spawns=[],
                 levels=fx["levels"], tower_levels=[1, 1])
    b.reset(tower_hp=None, **reset)
    down = {tuple(p) for p in fx["towers_down"]}
    hp = [[0, 0, 0], [0, 0, 0]]
    matched = set()
    for e in json.loads(b.state_json())["entities"]:
        if e[F["tower_slot"]] >= 0:
            at = (e[F["x"]] // SUB, e[F["y"]] // SUB)
            hp[e[F["team"]]][e[F["tower_slot"]]] = 0 if at in down else e[F["hp"]]
            matched |= {at} & down
    assert matched == down, f"towers_down {sorted(down - matched)} match no crown tower"
    b.reset(tower_hp=hp, **reset)
    plays = {}
    for p in fx["plays"]:
        plays.setdefault(p["tick"], []).append(p)
    track, knight = {}, None
    for _ in range(fx["ticks"] + 1):
        s = json.loads(b.state_json())
        for e in s["entities"]:
            if knight is None and e[F["team"]] == 1 and e[F["card_id"]] == ids["Knight"]:
                knight = e[F["uid"]]
            if e[F["uid"]] == knight:
                track[str(s["tick"])] = [e[F["x"]] // SUB, e[F["y"]] // SUB, e[F["hp"]]]
        if s["game_over"]:
            break
        cmds = []
        for p in plays.get(s["tick"], []):
            hand = s["players"][p["side"]]["hand"]
            assert ids[p["card"]] in hand, f"{p}: not in hand {hand} (the fixture's deck order no longer holds)"
            cmds.append((p["side"], hand.index(ids[p["card"]]), p["x"] * SUB, p["y"] * SUB))
        for row in b.step(cmds, 1):
            assert royalesim.DEPLOY_REASONS[row[1]] == "OK", f"tick {s['tick']}: a play was refused"
    return track


def first_split(want, got):
    """The first tick where the two tracks differ, a tick the Knight lives in one and not the other included."""
    for t in sorted(set(want) | set(got), key=int):
        if got.get(t) != want.get(t):
            return t, want.get(t), got.get(t)
    return None


@pytest.mark.parametrize("path", FIXTURES, ids=[p.stem for p in FIXTURES])
def test_the_knight_walks_as_in_the_client(path):
    fx = json.loads(path.read_text())
    split = first_split(fx["knight"], knight_track(fx))
    assert split is None, f"{fx['why']}: first split at tick {split[0]}: client {split[1]}, RoyaleSim {split[2]}"


@pytest.mark.parametrize("stem", ["lane_ownR_x8000_c12500_19500", "lane3_kingH_x10000_y26000_t300"])
def test_the_16402_arm_misses_the_cases_that_separate_them(stem):
    """The fixtures are not vacuous: client16402_spawn_lane_king sends the Knight elsewhere in these two."""
    fx = json.loads((DATA / f"{stem}.json").read_text())
    assert first_split(fx["knight"], knight_track(fx, {KEY: json.dumps("client16402_spawn_lane_king")})) is not None


def test_there_are_five_fixtures():
    assert len(FIXTURES) == 5
