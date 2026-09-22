#!/usr/bin/env python3
"""Spell impacts on crown towers in the live captures, as a committed fixture for
calibration combat.CROWN_TOWER_DAMAGE_ROUNDING.

    python tools/make_spell_impact_fixture.py            # rewrite the fixture
    python tools/make_spell_impact_fixture.py --check    # exit 1 if the committed one is stale

    ROYALELIVE_REPORTS  the captures folder (required; there is no default)

WHAT IT WRITES: crates/royalesim/tests/fixtures/spell_impacts.json -- every hit of a spell
on a crown tower that the ground-truth captures of the live client (CR 16.402,
*.native.oracle.jsonl.gz) show, with the tower's hp on the two frames around the hit, every
other thing that could have damaged the tower between those frames, and, when the same cast
also hit an ordinary unit or building that survived, that victim's hp step: the spell's FULL
damage, which a crown tower takes only a share of. tests/test_spell_impact_fixture.py
evaluates the candidate roundings of the crown-tower share against these rows.

WHERE A CAST COMES FROM
    A spell with a projectile is read off the capture's `effects` stream: every class-28
    object of one (side, card) with no gap over CAST_GAP_TICKS between sightings is one cast
    (tools/make_replay_fixture.py `spell_casts`, the same rule). An object lands on the tick
    after its last sighting (a Fireball last seen 162 short of its aim point, a step of 600
    native per tick, lands on the next tick), and a rolling object (the Log) hits what its
    track passes. A spell with no projectile (Tornado, Freeze, Earthquake, Poison, Zap, ...)
    exists only as a TAP in the placements log recorded beside the capture; it is kept only
    when a tower drop and a drop of another victim near the tap fall between the same two
    frames, and `cast.source` says `tap`.

WHAT CLEAN MEANS
    The tower's drop is measured between two OBSERVED frames (ta, tb] (the captures miss the
    odd frame, so tb - ta can exceed 1). The hit is `clean` when nothing else visible in the
    capture can have damaged the tower in (ta, tb]:
      - no enemy unit that attacks without a projectile (never the source of an effects object
        in the window) targets the tower with its attack under way (progress > 0) at ta or tb,
        unless its load timer proves it landed no hit in between: present on both frames and
        counting down 50 a tick throughout, still above zero at tb (a hit sets it back to
        LoadTime: calibration combat.ATTACK_CYCLE);
      - no other projectile aimed at the tower, or landing within SPLASH_REACH of it, left the
        stream in (ta, tb] (landed);
      - no enemy unit disappeared within DEATH_REACH of it (death damage);
      - fewer than three crown towers lost hp in (ta, tb] (the overtime drain takes every tower);
      - the tower survived (`destroyed` otherwise: the step is capped by its remaining hp).
    Otherwise `ambiguous`, with the reasons in `why`. A hit is invisible to these checks only
    if its projectile lived and landed entirely between two observed frames.

FULL DAMAGE
    A victim of the same cast that is not a crown tower, passed the same checks and survived
    gives the full damage: its hp step, less the lifetime decay a building loses every tick
    (DECAY: the building's steps between observed frames within DECAY_SPAN ticks of the hit,
    each read as floor..ceil of its per-tick rate, give a bracket [lo, hi] per tick, so the
    damage is a bracket too; a Tesla loses 2 or 3 a tick). Several witnesses must agree
    (their brackets intersect) or the row carries none. A clean victim the cast KILLED gives
    only a lower bound (`damage_at_least`).

LEVEL
    A cast carries no level in the captures. `side_level_mode` is the most common level among
    the caster side's units in the capture, as the replay fixtures assign it; it is context,
    not a measurement (a low-level account's cards differ from card to card).

NAMES
    Seats are letters over every capture in the folder (tools/capture_names.py folder_seats);
    `capture` is "<stamp>-<seat>" as in the replay fixtures and `battle` drops the seat, so the
    two seats of one battle, which record the same hit, share it.

--check: exit 1 if the committed fixture differs from what the captures give. Exit 0 current,
1 stale, 2 usage.
"""

from __future__ import annotations

import glob
import gzip
import json
import math
import os
import re
import sys
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capture_names import argv_guard, distinct_captures, folder_seats  # noqa: E402
from make_replay_fixture import (  # noqa: E402
    CARDS,
    CAST_GAP_TICKS,
    TAP_MAX,
    TAP_MIN,
    load_id_table,
    placement_files_for,
    public_name,
    read_placements,
)

LIVE = os.environ.get("ROYALELIVE_REPORTS")
SUFFIX = ".native.oracle.jsonl.gz"
OUT = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "spell_impacts.json")
FORMAT = "spell-impact-fixture-1"

SPELL_CLASS = 28
#: An object whose aim point lies within this of a victim (native, centre to centre) can have
#: hit it on its landing tick. Fireball's 2500 radius plus a princess tower's reach: the
#: corpus has a Fireball aimed 3606 from a princess tower's centre that hit it.
REACH = 4500
#: A rolling object hits what its track passes within this of.
ROLL_REACH = 2500
#: The spells whose second object rolls along the ground from where the first one lands and
#: damages what it passes (the capture shows an airborne object, then a rolling one). Every
#: other object damages only where it lands: a Fireball in flight hits nothing it passes.
ROLLING = ("Log", "BarbLog")
#: A tap-only spell (no projectile) is centred on its tap; Tornado's 5500 radius.
TAP_REACH = 6000
#: How long after its tap a tap-only spell is looked for, in ticks.
TAP_WINDOW = 160
#: A foreign projectile landing within this of a victim may have splashed it.
SPLASH_REACH = 3500
#: A unit disappearing within this of a victim may have dealt death damage to it.
DEATH_REACH = 4000
#: Frames either side of a building's step that its decay is measured over, and the largest
#: single-tick drop still counted as decay rather than a hit.
DECAY_SPAN = 15
DECAY_MAX = 10
#: Frames parsed around every window (the decay span plus one).
MARGIN = DECAY_SPAN + 1
#: Every LEVEL_STRIDE-th frame is parsed in full for the side level mode.
LEVEL_STRIDE = 50
KING_X = 9000

TICK = re.compile(r'"tick": (\d+)')
CLASS28 = re.compile(r'"card_id": 28\d{6}\b')

# compact entity row
CID, SIDE, KIND, LVL, HP, MAXHP, X, Y, TGT, PROG, LOAD = range(11)
EK = ("card_id", "side", "kind", "level", "hp", "max_hp", "x", "y", "target", "attack_progress_ms",
      "attack_load_timer_ms")
#: calibration combat.ATTACK_CYCLE (measured): the load timer counts down this much every tick
#: and is set back to LoadTime by a hit.
LOAD_STEP = 50


def dist(ax: float, ay: float, bx: float, by: float) -> float:
    return math.hypot(ax - bx, ay - by)


def battle_of(capture: str) -> str:
    """The capture name without its seat letter: both seats of one battle share it."""
    return re.sub(r"-[A-Z](?=\.|$)", "", capture)


def decay_bracket(steps: list[tuple[int, int]]) -> tuple[int, int] | None:
    """[lo, hi] per-tick lifetime decay from a building's (drop, ticks) steps between observed
    frames: a drop d over g ticks is between floor(d / g) and ceil(d / g) a tick. A step over
    DECAY_MAX a tick is a hit, not decay, and is left out."""
    ok = [(d, g) for d, g in steps if g > 0 and 0 <= d <= DECAY_MAX * g]
    if not ok:
        return None
    return min(d // g for d, g in ok), max(-(-d // g) for d, g in ok)


def damage_bracket(drop: int, gap: int, decay: tuple[int, int] | None, building: bool):
    """The full damage a victim's step implies: the step less gap ticks of decay."""
    if not building:
        return [drop, drop]
    if decay is None:
        return None
    lo, hi = drop - decay[1] * gap, drop - decay[0] * gap
    return [lo, hi] if lo > 0 else None


def compact_state(st: dict):
    ents = {e["id"]: tuple(e.get(k) for k in EK) for e in st["entities"]}
    effs = [
        (e["id"], e.get("card_id"), e["side"], e["x"], e["y"], e.get("projectile_x"),
         e.get("projectile_y"), e.get("source"), e.get("target"))
        for e in st.get("effects") or []
    ]
    return ents, effs


def scan(path: str):
    """Pass 1: every frame tick (first wins), the class-28 effects objects per tick, and the
    level of every unit on every LEVEL_STRIDE-th frame. Parses only the frames it needs."""
    ticks, seen = [], set()
    spell_objs: dict[int, list] = {}
    levels: dict[int, Counter] = defaultdict(Counter)
    header = None
    with gzip.open(path, "rt", encoding="utf-8") as fh:
        for line in fh:
            if not line.startswith('{"record": "frame"'):
                if header is None and '"header"' in line[:40]:
                    header = json.loads(line)
                continue
            m = TICK.search(line, 0, 200)
            if not m:
                continue
            t = int(m.group(1))
            if t in seen:
                continue
            seen.add(t)
            ticks.append(t)
            has28 = CLASS28.search(line) is not None
            if not has28 and len(ticks) % LEVEL_STRIDE != 1:
                continue
            st = json.loads(line)["state"]
            if len(ticks) % LEVEL_STRIDE == 1:
                for e in st["entities"]:
                    if (e.get("card_id") or -1) >= 0:
                        levels[e["side"]][e["level"]] += 1
            objs = [
                (e["id"], e["card_id"], e["side"], e["x"], e["y"], e.get("projectile_x"),
                 e.get("projectile_y"), e.get("target"))
                for e in st.get("effects") or []
                if (e.get("card_id") or -1) >= 0 and e["card_id"] // 1_000_000 == SPELL_CLASS
            ]
            if objs:
                spell_objs[t] = objs
    return header, ticks, spell_objs, levels


def load_frames(path: str, want: set[int]) -> dict[int, tuple]:
    """Pass 2: the frames whose tick is in `want`, compacted."""
    frames: dict[int, tuple] = {}
    with gzip.open(path, "rt", encoding="utf-8") as fh:
        for line in fh:
            if not line.startswith('{"record": "frame"'):
                continue
            m = TICK.search(line, 0, 200)
            if not m:
                continue
            t = int(m.group(1))
            if t in want and t not in frames:
                frames[t] = compact_state(json.loads(line)["state"])
    return frames


def effect_casts(spell_objs: dict[int, list], names: dict[int, str]) -> list[dict]:
    """One cast per run of class-28 objects of one (side, card), gaps <= CAST_GAP_TICKS."""
    open_c: dict[tuple, dict] = {}
    out = []
    for t in sorted(spell_objs):
        for oid, cid, side, x, y, px, py, tgt in spell_objs[t]:
            key = (side, cid)
            c = open_c.get(key)
            if c is not None and t - c["last"] > CAST_GAP_TICKS:
                out.append(c)
                c = None
            if c is None:
                c = open_c[key] = {"source": "effects", "side": side, "card_id": cid,
                                   "card": names.get(cid, str(cid)), "first": t, "last": t,
                                   "objects": {}}
            c["last"] = t
            o = c["objects"].setdefault(oid, {"last": t, "aim": (x, y) if px is None else (px, py),
                                              "target": tgt, "track": {}})
            o["last"] = t
            o["track"][t] = (x, y)
    out.extend(open_c.values())
    out.sort(key=lambda c: (c["first"], c["side"], c["card_id"]))
    return out


def damage_kinds() -> dict[str, str]:
    """spell -> "over_time" when its area effect deals damage through a buff (damage per second
    turned into hits every hit_frequency: Tornado, Poison, Earthquake), else "direct". An
    over-time hit is itself a rounded product, and the crown share may be taken before it is
    formed, so it tests the crown rounding only together with that other step."""
    with open(CARDS, encoding="utf-8") as fh:
        doc = json.load(fh)
    aeos = doc.get("area_effect_objects") or {}
    out = {}
    for c in doc["cards"]:
        if c.get("kind") != "spell":
            continue
        name = (c.get("spell") or {}).get("area_effect_object")
        if isinstance(name, dict):
            name = name.get("name")
        buff = ((aeos.get(name) or {}).get("buff") or {}) if name else {}
        out[c["name"]] = "over_time" if buff.get("damage_per_second") else "direct"
    return out


def tap_casts(path: str, casts: list[dict], names: dict[int, str]) -> list[dict]:
    """The spell taps no effects cast answers: spells with no projectile."""
    files = placement_files_for(path, os.path.dirname(path))
    if not files:
        return []
    with open(CARDS, encoding="utf-8") as fh:
        card_names = {c["name"] for c in json.load(fh)["cards"]}
    name_to_id = {}
    for cid, n in names.items():
        name_to_id.setdefault(n, cid)
    taps, _ = read_placements(files, card_names, name_to_id)
    out = []
    for t in taps:
        if t["kind"] != "cast" or t["id"] is None or not t["native"]:
            continue
        if t["id"] // 1_000_000 != SPELL_CLASS:
            continue
        answered = any(
            c["side"] == t["side"] and c["card_id"] == t["id"]
            and t["tick"] + TAP_MIN <= c["first"] <= t["tick"] + TAP_MAX
            for c in casts
        )
        if not answered:
            out.append({"source": "tap", "side": t["side"], "card_id": t["id"],
                        "card": names.get(t["id"], t["card"]), "tap_tick": t["tick"],
                        "tap": list(t["native"])})
    return out


class Window:
    """The parsed frames around one cast and the evidence checks on them."""

    def __init__(self, frames: dict[int, tuple], ticks: list[int], names: dict[int, str]):
        self.f = frames
        self.ticks = [t for t in ticks if t in frames]
        self.names = names
        self.sources = {e[7] for t in self.ticks for e in frames[t][1] if e[7]}

    def name(self, cid) -> str:
        return "crown tower" if cid is None or cid < 0 else self.names.get(cid, str(cid))

    def brackets(self, lo: int, hi: int):
        """Consecutive observed frames (ta, tb] with ta < hi and tb > lo."""
        for a, b in zip(self.ticks, self.ticks[1:], strict=False):
            if a < hi and b > lo:
                yield a, b

    def landed(self, ta: int, tb: int):
        """Effects objects present at ta and gone at tb: they landed in (ta, tb]."""
        at_b = {e[0] for e in self.f[tb][1]}
        return [e for e in self.f[ta][1] if e[0] not in at_b]

    def other_sources(self, vid, ta: int, tb: int, own: set) -> list[str]:
        """Everything visible that can have damaged entity `vid` in (ta, tb], besides the
        cast whose object ids are `own`."""
        a, b = self.f[ta][0], self.f[tb][0]
        v = a[vid]
        side, vx, vy = v[SIDE], v[X], v[Y]
        why = []
        for eid in set(a) | set(b):
            ra, rb = a.get(eid), b.get(eid)
            r = rb or ra
            if r[SIDE] == side or eid in self.sources:
                continue
            if not any(q and q[TGT] == vid and (q[PROG] or 0) > 0 for q in (ra, rb)):
                continue
            if not hit_ruled_out(ra, rb, tb - ta):
                why.append(f"{self.name(r[CID])} attacking it without a projectile")
        for e in self.landed(ta, tb):
            oid, cid, eside, x, y, _px, _py, _src, tgt = e
            if oid in own or eside == side:
                continue
            if tgt == vid:
                why.append(f"a {self.name(cid)} projectile aimed at it landed")
            elif dist(x, y, vx, vy) <= SPLASH_REACH:
                why.append(f"a {self.name(cid)} projectile landed {round(dist(x, y, vx, vy))} away")
        for eid, r in a.items():
            if r[SIDE] != side and eid not in b and r[CID] is not None and r[CID] >= 0:
                d = dist(r[X], r[Y], vx, vy)
                if d <= DEATH_REACH:
                    why.append(f"a {self.name(r[CID])} disappeared {round(d)} away")
        drained = sum(1 for eid, r in b.items() if r[CID] == -1 and eid in a and a[eid][HP] > r[HP])
        if drained >= 3:
            why.append(f"{drained} crown towers lost hp")
        return sorted(set(why))

    def decay(self, vid, ta: int, tb: int):
        steps = []
        for x, y in zip(self.ticks, self.ticks[1:], strict=False):
            if (x, y) == (ta, tb) or abs(x - ta) > DECAY_SPAN:
                continue
            ra, rb = self.f[x][0].get(vid), self.f[y][0].get(vid)
            if ra and rb:
                steps.append((ra[HP] - rb[HP], y - x))
        return decay_bracket(steps)


def hit_ruled_out(ra, rb, gap: int) -> bool:
    """True when an attacker's load timer PROVES it landed no hit in the bracket: present on
    both frames and counting down LOAD_STEP a tick throughout, still above zero at the end (a
    hit, or a fresh attack, sets it back to LoadTime; combat.ATTACK_CYCLE)."""
    if ra is None or rb is None or ra[LOAD] is None or rb[LOAD] is None:
        return False
    return rb[LOAD] > 0 and rb[LOAD] == ra[LOAD] - LOAD_STEP * gap


def near_cast(cast: dict, win: Window, ta: int, tb: int, vx: int, vy: int) -> int | None:
    """How close the cast came to (vx, vy) in (ta, tb]: the aim of an object that landed in
    the bracket, or the track of an object seen at ta or tb. None when it did not."""
    if cast["source"] == "tap":
        d = dist(cast["tap"][0], cast["tap"][1], vx, vy)
        return round(d) if d <= TAP_REACH else None
    best = None
    rolls = cast["card"] in ROLLING
    for o in cast["objects"].values():
        cands = []
        if ta < o["last"] + 1 <= tb:
            cands.append((dist(o["aim"][0], o["aim"][1], vx, vy), REACH))
        for tt in (ta, tb):
            if rolls and tt in o["track"]:
                px, py = o["track"][tt]
                cands.append((dist(px, py, vx, vy), ROLL_REACH))
        for d, reach in cands:
            if d <= reach and (best is None or d < best):
                best = d
    return None if best is None else round(best)


def cast_window(cast: dict) -> tuple[int, int]:
    if cast["source"] == "tap":
        return cast["tap_tick"], cast["tap_tick"] + TAP_WINDOW
    return cast["first"] - 1, cast["last"] + 2


def witnesses(cast: dict, win: Window, tower_bracket: tuple[int, int]):
    """(clean surviving victims with their damage bracket, lower bound from clean kills)."""
    lo, hi = cast_window(cast)
    own = set(cast.get("objects", {}))
    out, killed = [], []
    for ta, tb in win.brackets(lo, hi):
        if cast["source"] == "tap" and (ta, tb) != tower_bracket:
            continue
        a, b = win.f[ta][0], win.f[tb][0]
        for vid, r in a.items():
            if r[SIDE] == cast["side"] or r[CID] is None or r[CID] < 0:
                continue
            rb = b.get(vid)
            if rb is not None and rb[HP] >= r[HP]:
                continue
            if near_cast(cast, win, ta, tb, r[X], r[Y]) is None:
                continue
            if win.other_sources(vid, ta, tb, own):
                continue
            building = r[KIND] in (12, 13)
            if rb is None or rb[HP] <= 0:
                if not building:
                    killed.append(r[HP])
                continue
            dec = win.decay(vid, ta, tb) if building else None
            dmg = damage_bracket(r[HP] - rb[HP], tb - ta, dec, building)
            if dmg is None:
                continue
            out.append({
                "victim": win.name(r[CID]), "building": building, "frames": [ta, tb],
                "x": r[X], "y": r[Y], "hp_before": r[HP], "hp_after": rb[HP],
                "decay_per_tick": list(dec) if dec else None, "damage": dmg,
            })
    return out, (max(killed) if killed else None)


def events_for(cast: dict, win: Window, cap: str, level: int | None, kind: str) -> list[dict]:
    lo, hi = cast_window(cast)
    own = set(cast.get("objects", {}))
    rows = []
    for ta, tb in win.brackets(lo, hi):
        a, b = win.f[ta][0], win.f[tb][0]
        for tid, r in a.items():
            if r[CID] != -1 or r[SIDE] == cast["side"]:
                continue
            rb = b.get(tid)
            if rb is not None and rb[HP] >= r[HP]:
                continue
            near = near_cast(cast, win, ta, tb, r[X], r[Y])
            if near is None:
                continue
            why = win.other_sources(tid, ta, tb, own)
            destroyed = rb is None or rb[HP] <= 0
            status = "destroyed" if destroyed else ("ambiguous" if why else "clean")
            # A tap-only spell has no object to time it by: a tower drop is its hit only when
            # the tower is otherwise untouched AND another victim near the tap took a step
            # between the same two frames.
            if cast["source"] == "tap" and (status != "clean" or not witnesses(cast, win, (ta, tb))[0]):
                continue
            row = {
                "capture": cap, "battle": battle_of(cap), "spell": cast["card"],
                "caster_side": cast["side"], "side_level_mode": level, "damage_kind": kind,
                "cast": ({"source": "effects", "first_seen": cast["first"],
                          "last_seen": cast["last"], "objects": len(cast["objects"])}
                         if cast["source"] == "effects" else
                         {"source": "tap", "tap_tick": cast["tap_tick"], "tap": cast["tap"]}),
                "tower": {"side": r[SIDE], "role": "king" if r[X] == KING_X else "princess",
                          "x": r[X], "y": r[Y], "level": r[LVL], "max_hp": r[MAXHP]},
                "reach": near, "frames": [ta, tb], "hp_before": r[HP],
                "hp_after": None if rb is None else rb[HP],
                "drop": r[HP] - (0 if rb is None else rb[HP]),
                "status": status, "why": why,
                "full_damage": None, "witnesses": [], "damage_at_least": None,
            }
            if status == "clean":
                wit, least = witnesses(cast, win, (ta, tb))
                row["witnesses"] = wit
                row["damage_at_least"] = least
                if wit:
                    blo = max(w["damage"][0] for w in wit)
                    bhi = min(w["damage"][1] for w in wit)
                    if blo <= bhi:  # else the witnesses disagree and the row carries none
                        row["full_damage"] = [blo, bhi]
            rows.append(row)
    return rows


def build(reports: str) -> dict:
    names = load_id_table()
    kinds = damage_kinds()
    seats = folder_seats(reports, SUFFIX)
    paths = distinct_captures(glob.glob(os.path.join(reports, "*" + SUFFIX)))
    census = Counter()
    by_card = Counter()
    captures, events = [], []
    for path in paths:
        cap = public_name(path, seats)
        captures.append(cap)
        header, ticks, spell_objs, levels = scan(path)
        towers = (header or {}).get("towers") or []
        if any(t["side"] == 0 and t["y"] > 16000 for t in towers):
            raise SystemExit(f"{cap}: side 0 defends the high-y half; this maker publishes native "
                             "positions and handles no rotated capture")
        census["frames"] += len(ticks)
        casts = effect_casts(spell_objs, names)
        casts += tap_casts(path, casts, names)
        if not casts:
            continue
        level = {s: c.most_common(1)[0][0] for s, c in levels.items() if c}
        want = set()
        for c in casts:
            lo, hi = cast_window(c)
            want.update(range(lo - MARGIN, hi + MARGIN + 1))
        win = Window(load_frames(path, want), ticks, names)
        for c in casts:
            census["casts_" + c["source"]] += 1
            by_card[c["card"]] += 1
            events += events_for(c, win, cap, level.get(c["side"]), kinds.get(c["card"], "direct"))
    assert len(set(captures)) == len(captures), "two captures resolved to one name"
    events.sort(key=lambda e: (e["capture"], e["frames"][0], e["tower"]["x"], e["tower"]["y"]))
    status = Counter(e["status"] for e in events)
    return {
        "format": FORMAT,
        "generated_by": "tools/make_spell_impact_fixture.py",
        "source": "ground-truth captures of the live client (CR 16.402): per frame, every "
                  "entity's hp and every spell projectile object; spell ids resolved against "
                  "the 15.535 spells_other.csv row order",
        "rules": {"cast_gap_ticks": CAST_GAP_TICKS, "reach": REACH, "roll_reach": ROLL_REACH,
                  "tap_reach": TAP_REACH, "tap_window": TAP_WINDOW,
                  "splash_reach": SPLASH_REACH, "death_reach": DEATH_REACH,
                  "decay_span": DECAY_SPAN, "decay_max": DECAY_MAX},
        "census": {
            "captures": len(captures), "frames": census["frames"],
            "casts_from_effects": census["casts_effects"], "casts_from_taps": census["casts_tap"],
            "casts_by_spell": dict(sorted(by_card.items())),
            "crown_tower_hits": len(events), "clean": status["clean"],
            "clean_with_full_damage": sum(1 for e in events if e["full_damage"]),
            "ambiguous": status["ambiguous"], "destroyed": status["destroyed"],
        },
        "captures": captures,
        "events": events,
    }


def text_of(doc: dict) -> str:
    return json.dumps(doc, indent=1) + "\n"


def main() -> int:
    argv_guard(sys.argv[1:], __doc__)
    check = "--check" in sys.argv[1:]
    if not LIVE:
        print("set ROYALELIVE_REPORTS to the folder holding the *" + SUFFIX + " captures", file=sys.stderr)
        return 2
    if not os.path.isdir(LIVE):
        print(f"no captures at {LIVE} (ROYALELIVE_REPORTS)", file=sys.stderr)
        return 2
    text = text_of(build(LIVE))
    if check:
        old = ""
        if os.path.exists(OUT):
            with open(OUT, encoding="utf-8") as fh:
                old = fh.read()
        if old != text:
            print("spell_impacts.json differs from the captures; rerun without --check", file=sys.stderr)
            return 1
        print("spell_impacts.json is current")
        return 0
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    doc = json.loads(text)
    print(f"wrote {OUT}: {json.dumps(doc['census'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
