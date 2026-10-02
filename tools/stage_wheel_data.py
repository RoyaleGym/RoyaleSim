"""Copy the engine's runtime data into python/royalesim/data/, so a wheel built next carries it.

Run from the repository root after stage 3 (README, "Install"), before `maturin build`. The list is every file the
engine or its Python consumers read at run time, all of which a clean clone can generate; nothing from the decoded
client pack, which is not redistributed (mechanic_register.json needs that pack, so it is not shipped).
"""

import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = [
    "calibration.json",
    "derived/arena.json",
    "derived/cards.json",
    "derived/cards-15.535.json",
    "derived/cards-2018.json",
    "derived/globals.json",
    "raw/retroroyale-2018/README.md",
    "raw/retroroyale-2018/csv_logic/globals.csv",
    "raw/retroroyale-2018/csv_logic/rarities.csv",
]


def main() -> int:
    src, dst = ROOT / "data", ROOT / "python" / "royalesim" / "data"
    missing = [f for f in FILES if not (src / f).is_file()]
    if missing:
        print(f"stage_wheel_data: missing {missing}; run stage 3 first", file=sys.stderr)
        return 1
    if (src / "derived/cards.json").read_bytes() != (src / "derived/cards-15.535.json").read_bytes():
        why = "cards.json is not cards-15.535.json, the table the engine compiles in"
        print(f"stage_wheel_data: {why}", file=sys.stderr)
        return 1
    shutil.rmtree(dst, ignore_errors=True)
    for f in FILES:
        (dst / f).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src / f, dst / f)
    print(f"stage_wheel_data: {len(FILES)} files -> {dst.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
