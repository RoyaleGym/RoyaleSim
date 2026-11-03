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
are not built. A regenerated table that moves anything else turns this red, so a reader change cannot ride in unseen.
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DERIVED = ROOT / "data" / "derived"
OLD, NEW = "cards-160402017-20261019.json", "cards-160635007-20261103.json"

#: card -> {field path: (2026-10-19 value, 16.635 value)}: the values the extractor writes in `cards`.
CHANGES = {
    "GoldenKnight": {"ability/refund_until_delivered": (None, True), "ability/refund_window_ms": (50, None)},
    "LittlePrince": {"ability/effect/hit_filter_dash_immune": (None, True)},
}
REFUND = ["ability/refund_until_delivered", "ability/refund_window_ms"]


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


def test_the_cards_differ_by_the_clients_changes_alone():
    old, new = load(OLD), load(NEW)
    assert [c["name"] for c in old["cards"]] == [c["name"] for c in new["cards"]], "the same catalogue"
    got = {}
    for a, b in zip(old["cards"], new["cards"], strict=True):
        d: dict = {}
        diff(a, b, "", d)
        if d:
            got[a["name"]] = d
    assert got == CHANGES


def test_the_evolutions_and_hero_forms_differ_by_the_clients_changes_alone():
    old, new = load(OLD), load(NEW)
    got = {}
    for sec in ("evolutions", "hero_forms"):
        a = {e["name"]: e for e in old[sec]}
        b = {e["name"]: e for e in new[sec]}
        assert sorted(a) == sorted(b), f"{sec}: the Evo Giant and the Evo Mirror are not built"
        for n in sorted(a):
            d: dict = {}
            diff(a[n], b[n], "", d)
            if d:
                got[n] = sorted(d)
    assert got == {
        "BattleRam_EV1": ["evo_ram/push/damage_flags"],
        # CommonAreaDamageFilterNoClones is gone from game_object_filters.toml
        "ElectroGiant_EV1": ["evo_delevel_pulse/filter/filter_clones", "evo_delevel_pulse/filter/name"],
        "MegaKnight_EV1": ["evo_uppercut/root_tags", "evo_uppercut/uppercut_tags"],
        "Balloon_hero": REFUND,
        "ElectroWizard_hero": [
            "ability/effect/entry_stand_ms",
            "ability/effect/stun_ms",
            "tables/units/ElectroWizardHero/list_columns/AttackSequenceList[None]/CustomBuffOnDamageTime",
            "tables/units/ElectroWizardHero_Surge/list_columns/AttackSequenceList[None]/CustomBuffOnDamageTime",
        ],
        "Giant_hero": REFUND,
        "IceWizard_hero": REFUND,
        "Valkyrie_hero": REFUND,
    }


def test_the_records_differ_by_the_clients_changes_alone():
    old, new = load(OLD), load(NEW)
    got = {}
    for sec in ("units", "projectiles", "area_effect_objects", "buffs"):
        a, b = old[sec], new[sec]
        assert not set(a) - set(b), f"{sec}: nothing removed"
        changed = {}
        for n in sorted(set(a) & set(b)):
            d: dict = {}
            diff(a[n], b[n], "", d)
            if d:
                changed[n] = sorted(d)
        got[sec] = (sorted(set(b) - set(a)), changed)
    assert got == {
        "units": (["Giant_EV1"], {}),
        "projectiles": ([], {"Cannon_EV1_barrage_projectile": ["target_buff/cast_id_specific"]}),
        "area_effect_objects": (
            ["GiantEvoRage1", "GiantEvoRage2", "GiantEvoRage3", "GiantEvoRage4", "GiantEvoRageBase"],
            {"FirecrackerFireworksBig_EV1": ["buff/cast_id_specific"],
             "FirecrackerFireworksSmall_EV1": ["buff/cast_id_specific"]},
        ),
        "buffs": ([], {"Cannon_EV1_barrage_damage_buff": ["cast_id_specific"],
                       "FirecrackerFireworks_EV1": ["cast_id_specific"],
                       "SkeletonDuplication_EV1": ["not_cloned"]}),
    }
    assert old["globals"] == new["globals"], "the read globals (Mirror, Clone) are unchanged"
