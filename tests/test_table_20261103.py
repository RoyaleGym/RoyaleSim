"""THE 16.635 TABLE IS THE 2026-10-19 ONE WITH THE 16.635 CLIENT'S CHANGES, AND NOTHING ELSE.

data/derived/cards-160635007-20261103.json (tools/extract_cards.py --vintage 160635007-20261103: build 160635007, the
2026-11-03 Play update, content 16.635.8) against data/derived/cards-160402017-20261019.json (content 16.402.22). The
new client moves no card's numbers. What differs is how a few mechanics are written: the hero buttons and the Golden
Knight's are refunded until their ability is delivered (RefundUntilDelivered, in place of a 50 ms RefundWindow), the
Little Prince's guard charge leaves out the dash-immune, the Evo Battle Ram's push damage is flagged NoCounter, the Evo
Mega Knight stands while its uppercut runs and its root lets a pull move the victim, the Hero Electro Wizard's surge
stuns 600 ms and no longer stands, the Evo Electro Giant's pulse is back on CommonAreaDamageFilter (it takes a Clone
copy again), the Evo Cannon's and Evo Firecracker's damage buffs stack by cast (CastIdSpecificBuff), and an Evo
Skeletons copy takes no duplication. The Evo Giant's unit and rage areas are new rows; the Evo Giant and the Evo Mirror
are not built. Every changed value is pinned, old and new: a regenerated table that moves anything else, or moves one
of these to another value, turns this red, so a reader change cannot ride in unseen.
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DERIVED = ROOT / "data" / "derived"
OLD, NEW = "cards-160402017-20261019.json", "cards-160635007-20261103.json"

REFUND = {"ability/refund_until_delivered": (None, True), "ability/refund_window_ms": (50, None)}
SURGE_STUN = "list_columns/AttackSequenceList[{}]/CustomBuffOnDamageTime"
CAST = (None, True)

#: card -> {field path: (2026-10-19 value, 16.635 value)}: the values the extractor writes in `cards`.
CARDS = {
    "GoldenKnight": REFUND,
    "LittlePrince": {"ability/effect/hit_filter_dash_immune": (None, True)},
}
#: evolution or hero form -> {field path: (2026-10-19 value, 16.635 value)}.
FORMS = {
    "BattleRam_EV1": {"evo_ram/push/damage_flags": (None, "NoCounter")},
    # CommonAreaDamageFilterNoClones is gone from game_object_filters.toml
    "ElectroGiant_EV1": {
        "evo_delevel_pulse/filter/filter_clones": (True, None),
        "evo_delevel_pulse/filter/name": ("CommonAreaDamageFilterNoClones", "CommonAreaDamageFilter"),
    },
    "MegaKnight_EV1": {
        "evo_uppercut/root_tags": (None, "NO_MOVE_ALLOW_ATTRACT"),
        "evo_uppercut/uppercut_tags": (None, "NO_ATTACK,NO_MOVE_ALLOW_ATTRACT"),
    },
    "Balloon_hero": REFUND,
    "ElectroWizard_hero": {
        "ability/effect/entry_stand_ms": (150, None),
        "ability/effect/stun_ms": (None, 600),
        **{f"tables/units/{u}/{SURGE_STUN.format(i)}": (None, 600)
           for u in ("ElectroWizardHero", "ElectroWizardHero_Surge") for i in (1, 2)},
    },
    "Giant_hero": REFUND,
    "IceWizard_hero": REFUND,
    "Valkyrie_hero": REFUND,
}
#: section -> (records added, {record: {field path: (2026-10-19 value, 16.635 value)}}).
RECORDS = {
    "units": (["Giant_EV1"], {}),
    "projectiles": ([], {"Cannon_EV1_barrage_projectile": {"target_buff/cast_id_specific": CAST}}),
    "area_effect_objects": (
        ["GiantEvoRage1", "GiantEvoRage2", "GiantEvoRage3", "GiantEvoRage4", "GiantEvoRageBase"],
        {"FirecrackerFireworksBig_EV1": {"buff/cast_id_specific": CAST},
         "FirecrackerFireworksSmall_EV1": {"buff/cast_id_specific": CAST}},
    ),
    "buffs": ([], {"Cannon_EV1_barrage_damage_buff": {"cast_id_specific": CAST},
                   "FirecrackerFireworks_EV1": {"cast_id_specific": CAST},
                   "SkeletonDuplication_EV1": {"not_cloned": CAST}}),
}


def load(name: str) -> dict:
    return json.loads((DERIVED / name).read_text(encoding="utf-8"))


def diff(x, y, path: str, out: dict) -> None:
    """Every leaf that differs, as path -> (x, y). A list element is named by its `name`, else by its index."""
    if isinstance(x, dict) and isinstance(y, dict):
        for k in sorted(set(x) | set(y)):
            diff(x.get(k), y.get(k), f"{path}/{k}" if path else k, out)
    elif isinstance(x, list) and isinstance(y, list) and len(x) == len(y):
        for i, (p, q) in enumerate(zip(x, y, strict=True)):
            label = p.get("name", i) if isinstance(p, dict) else i
            diff(p, q, f"{path}[{label}]", out)
    elif x != y:
        out[path] = (x, y)


def changes(a: dict, b: dict) -> dict:
    d: dict = {}
    diff(a, b, "", d)
    return d


def test_the_cards_differ_by_the_clients_changes_alone():
    old, new = load(OLD), load(NEW)
    assert [c["name"] for c in old["cards"]] == [c["name"] for c in new["cards"]], "the same catalogue"
    got = {a["name"]: changes(a, b) for a, b in zip(old["cards"], new["cards"], strict=True)}
    assert {n: d for n, d in got.items() if d} == CARDS


def test_the_evolutions_and_hero_forms_differ_by_the_clients_changes_alone():
    old, new = load(OLD), load(NEW)
    got = {}
    for sec in ("evolutions", "hero_forms"):
        a = {e["name"]: e for e in old[sec]}
        b = {e["name"]: e for e in new[sec]}
        assert sorted(a) == sorted(b), f"{sec}: the Evo Giant and the Evo Mirror are not built"
        for n in sorted(a):
            d = changes(a[n], b[n])
            if d:
                got[n] = d
    assert got == FORMS


def test_the_records_differ_by_the_clients_changes_alone():
    old, new = load(OLD), load(NEW)
    got = {}
    for sec in RECORDS:
        a, b = old[sec], new[sec]
        assert not set(a) - set(b), f"{sec}: nothing removed"
        changed = {n: changes(a[n], b[n]) for n in sorted(set(a) & set(b))}
        got[sec] = (sorted(set(b) - set(a)), {n: d for n, d in changed.items() if d})
    assert got == RECORDS
    assert old["globals"] == new["globals"], "the read globals (Mirror, Clone) are unchanged"


def test_the_diff_names_an_unnamed_entry_by_its_index():
    """The helper the pins above rest on: an entry without a name is told apart by its place, so a change to the
    first attack entry cannot pass for one to the second."""
    a = {"l": [{"x": 1}, {"x": 2}]}
    b = {"l": [{"x": 1}, {"x": 3}]}
    assert changes(a, b) == {"l[1]/x": (2, 3)}
