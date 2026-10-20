"""THE 2026-10-19 TABLE IS THE 2026-10-06 ONE WITH THE SEASON 88 MID-SEASON CHANGES, AND NOTHING ELSE.

data/derived/cards-160402017-20261019.json (tools/extract_cards.py --vintage 160402017-20261019, content 16.402.22)
against data/derived/cards-160402017-20261006.json (content 16.402.19): every value that differs, by card, is one of
the update's announced changes (the balance notes of 2026-10-18: Rune Giant, Minion Giant, Cannon Cart, Dart Goblin
and Evo Skeleton Barrel nerfed; Hero Electro Wizard, Electro Dragon, Flying Machine, Void and Skeleton Barrel buffed;
the Skeletons' spawn ring fixed), or the 2026-10-08 content's Evo Electro Giant pulse, which passes over the Clone
spell's copies. A regenerated table that moves anything else turns this red, so a reader change cannot ride in unseen.
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DERIVED = ROOT / "data" / "derived"

#: card (or record) -> {field path: (2026-10-06 value, 2026-10-19 value)}: the values the extractor writes in `cards`.
CHANGES = {
    "BlowdartGoblin": {"damage": (59, 56), "projectile/damage": (59, 56)},
    "DarkMagic": {"spell/area_effect_object/strike_area/tiers[None]/buff/damage_per_second": (1155, 1200)},
    "DartBarrell": {"damage": (67, 70), "projectile/damage": (67, 70)},
    "ElectroDragon": {"load_time_ms": (1400, 1600)},
    "GiantBuffer": {"enchant_friends/enchant/finish_if_instigator_dies_ms": (5000, 500)},
    "MinionGiant": {"damage": (66, 60), "projectile/damage": (66, 60)},
    "MovingCannon": {"damage": (83, 79), "mass": (3, 6), "projectile/damage": (83, 79)},
    "Skeletons": {"summon_radius_milli": (400, 500)},
}


def load(name: str) -> dict:
    return json.loads((DERIVED / name).read_text(encoding="utf-8"))


def diff(x, y, path: str, out: dict) -> None:
    if isinstance(x, dict) and isinstance(y, dict):
        for k in sorted(set(x) | set(y)):
            diff(x.get(k), y.get(k), f"{path}/{k}" if path else k, out)
    elif isinstance(x, list) and isinstance(y, list) and len(x) == len(y):
        for i, (p, q) in enumerate(zip(x, y, strict=True)):
            diff(p, q, f"{path}[{p.get('name') if isinstance(p, dict) else i}]", out)
    elif x != y:
        out[path] = (x, y)


def test_the_cards_differ_by_the_updates_changes_alone():
    old, new = load("cards-160402017-20261006.json"), load("cards-160402017-20261019.json")
    assert [c["name"] for c in old["cards"]] == [c["name"] for c in new["cards"]], "the same catalogue"
    got = {}
    for a, b in zip(old["cards"], new["cards"], strict=True):
        d: dict = {}
        diff(a, b, "", d)
        if d:
            got[a["name"]] = d
    assert got == CHANGES


def test_the_evolutions_and_hero_forms_differ_by_the_updates_changes_alone():
    old, new = load("cards-160402017-20261006.json"), load("cards-160402017-20261019.json")
    got = {}
    for sec in ("evolutions", "hero_forms"):
        a = {e["name"]: e for e in old[sec]}
        b = {e["name"]: e for e in new[sec]}
        assert sorted(a) == sorted(b), sec
        for n in sorted(a):
            d: dict = {}
            diff(a[n], b[n], "", d)
            if d:
                got[n] = sorted(d)
    assert got == {
        "BlowdartGoblin_EV1": ["damage", "projectile/damage"],
        "ElectroDragon_EV1": ["load_time_ms"],
        # the 2026-10-08 content (16.402.20 on): the pulse's area filter excludes the Clone spell's copies
        "ElectroGiant_EV1": ["evo_delevel_pulse/filter/filter_clones", "evo_delevel_pulse/filter/name"],
        "ElectroWizard_hero": [
            "ability/effect/attack_start_delay_ms",
            "ability/effect/entry_stand_ms",
            "ability/refund_window_ms",
            "tables/units/ElectroWizardHero/list_columns/AttackSequenceList[None]/AttackStartDelay",
            "tables/units/ElectroWizardHero_Surge/list_columns/AttackSequenceList[None]/AttackStartDelay",
        ],
        "SkeletonBalloon_EV1": ["evo_barrel/death/death_damage", "evo_barrel/extra/death_damage"],
        "Skeletons_EV1": ["summon_radius_milli"],
    }
