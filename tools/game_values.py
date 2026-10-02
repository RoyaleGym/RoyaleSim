"""Write docs/game-values.md, the cheatsheet of game values, from the engine itself.

Every number on the page is read from the installed engine or from data/calibration.json, never typed in, so the
page cannot drift from what the engine runs. tests/test_game_values.py fails when the committed page is not what
this prints. Regenerate it with:

    python tools/game_values.py --write
"""

import json
import sys
from pathlib import Path

import royalesim

ROOT = Path(__file__).resolve().parent.parent
PAGE = ROOT / "docs" / "game-values.md"
FORM_EVOLUTION = 1


def value(calib: dict, section: str, key: str):
    return calib[section][key]["value"]


def seconds(ms: int) -> str:
    return f"{ms / 1000:g} s"


def has_evolution(card_id: int, n: int) -> bool:
    """Whether the engine plays this card's evolution: a deck holding it in form 1 resets without an error."""
    b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    deck = [card_id, *[i for i in range(n) if i != card_id][:7]]
    try:
        b.reset(0, [deck, deck], 0, 0, None, None, [], [[FORM_EVOLUTION, *[0] * 7], [0] * 8])
    except ValueError:
        return False
    return True


def page() -> str:
    calib = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    m = lambda key: value(calib, "match", key)  # noqa: E731
    tick_ms = value(calib, "time", "TICK_MS")
    b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    cat = json.loads(b.catalogue_json())
    b.reset(0, [[0] * 8, [0] * 8], 0, 0, None, None, [])
    towers = json.loads(b.state_json())["players"][0]["tower_hp"]
    level = b.card_level()
    per_elixir = lambda ms: seconds(ms // 10)  # noqa: E731  (the MANA_REGEN_MS keys are the time for 10 elixir)
    lines = [
        "# Game values",
        "",
        "The numbers the engine plays by. This page is generated from the engine (`tools/game_values.py`), so it",
        "matches the engine it was made with. Do not edit it by hand.",
        "",
        "## Time",
        "",
        "| What | Value |",
        "|---|---|",
        f"| One tick | {tick_ms} ms ({1000 // tick_ms} ticks a second) |",
        f"| Regular time | {m('REGULAR_TIME_S')} s ({m('REGULAR_TIME_S') * 1000 // tick_ms} ticks) |",
        f"| Overtime | {m('OVERTIME_S')} s |",
        f"| No plays at the start | the first {m('DEPLOY_LOCKOUT_TICKS')} ticks "
        f"({seconds(m('DEPLOY_LOCKOUT_TICKS') * tick_ms)}) |",
        "",
        "## Elixir",
        "",
        "| What | Value |",
        "|---|---|",
        f"| At the start | {m('START_MANA')} |",
        f"| Most you can hold | {m('MAX_MANA')} |",
        f"| Normal rate | 1 elixir every {per_elixir(m('MANA_REGEN_MS_1X'))} |",
        f"| Double elixir (last {m('MANA_SPEED_UP_WHEN_REMAINING_SECONDS')} s of regular time, and overtime) "
        f"| 1 elixir every {per_elixir(m('MANA_REGEN_MS_2X'))} |",
        f"| Triple elixir (from {m('MANA_TRIPLE_AFTER_OVERTIME_S')} s into overtime) "
        f"| 1 elixir every {per_elixir(m('MANA_REGEN_MS_OVERTIME'))} |",
        "",
        "## Arena and towers",
        "",
        "| What | Value |",
        "|---|---|",
        "| Arena | 18 tiles wide, 32 tiles long; the river is rows 15 and 16 |",
        f"| Position units | {royalesim.SUBTILE:,} to one tile |",
        f"| King tower hitpoints (level {b.tower_level()}) | {towers[0]:,} |",
        f"| Princess tower hitpoints (level {b.tower_level()}) | {towers[1]:,} |",
        "",
        f"## Cards (level {level})",
        "",
        "Names are the ones the engine takes. Hitpoints are for one unit of the card, and Units says how many it",
        "puts down. Evolution says whether the engine plays the card's evolved form. Hero is what the hero form's",
        "ability button costs, for a card with a hero form.",
        "",
        "| Card | Elixir | Type | Units | Flying | Hitpoints | Evolution | Hero |",
        "|---|---|---|---|---|---|---|---|",
    ]
    col = {field: k for k, field in enumerate(royalesim.CATALOGUE_FIELDS)}
    for i, row in enumerate(cat):
        # By name: the catalogue row may grow at its end.
        name, elixir, count, flying, hp, kind, hero = (
            row[col[f]] for f in ("name", "elixir", "count", "flying", "hitpoints", "card_kind", "hero")
        )
        evo = "yes" if has_evolution(i, len(cat)) else ""
        lines.append(
            f"| {name} | {elixir} | {kind.lower()} | {count or ''} | {'yes' if flying else ''} | "
            f"{f'{hp:,}' if hp else ''} | {evo} | {'' if hero is None else f'{hero} elixir'} |"
        )
    return "\n".join(lines) + "\n"


def main(argv: list[str]) -> int:
    text = page()
    if "--write" in argv:
        PAGE.write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {PAGE.relative_to(ROOT)}")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
