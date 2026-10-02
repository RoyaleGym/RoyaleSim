"""Check an INSTALLED royalesim the way a new user meets it: away from any checkout.

CI runs this from an empty folder after `pip install royalesim` from a built wheel. It fails (exit 1) unless the
engine loads its full compiled-in card table, the shipped data folder is complete, and a battle runs.
"""

import json
import sys

import royalesim


def main() -> int:
    errors = []
    source = royalesim.card_table_source()
    if source != "embedded":
        errors.append(f"card table source is {source!r}, not the compiled-in table")
    data = royalesim.data_dir()
    for f in ["calibration.json", "derived/arena.json", "derived/cards.json", "derived/globals.json"]:
        if not (data / f).is_file():
            errors.append(f"{data / f} is missing")
    b = royalesim.Battle(card_names=None, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    catalogue = json.loads(b.catalogue_json())
    if len(catalogue) < 100:
        errors.append(f"the catalogue has {len(catalogue)} cards, not the full table")
    b.reset(0, [list(range(8)), list(range(8, 16))], 1, 0, [5000, 5000], None, [])
    b.step([], 600)
    state = json.loads(b.state_json())
    print(f"royalesim {royalesim.Battle.provenance()} cards={len(catalogue)} source={source} data={data}")
    print(f"after 600 ticks: tick={state.get('tick')}")
    for e in errors:
        print("FAIL:", e, file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
