#!/usr/bin/env python3
r"""Measure engine throughput through the Python surface.

Simulates whole battles with both seats deploying at random and reports ticks/s.
The rate depends heavily on how often the caller decodes state back into Python,
so both ends are measured: --read-every 1 decodes every step, --read-every 0 never does.

    ..\.venv\Scripts\python tools\throughput.py
    ..\.venv\Scripts\python tools\throughput.py --battles 5 --read-every 0
"""
from __future__ import annotations

import argparse
import json
import random
import time

import royalesim

DECK = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Goblins"]
TICKS_PER_STEP = 20
T = royalesim.SUBTILE


def one_battle(seed: int, steps: int, read_every: int) -> tuple[int, int, int]:
    rng = random.Random(seed)
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]])
    b.reset(seed=seed, decks=[list(range(8))] * 2, shuffle=0, start_tick=0,
            elixir_milli=[10_000, 10_000], tower_hp=None, spawns=[])
    entity_samples, samples = 0, 0
    for i in range(steps):
        cmds = []
        for side in (0, 1):
            if rng.random() < 0.25:
                y = rng.randint(2, 14) if side == 0 else rng.randint(17, 29)
                cmds.append((side, rng.randint(0, 3), rng.randint(1, 16) * T, y * T))
        b.step(cmds, TICKS_PER_STEP)
        if read_every and i % read_every == 0:
            s = json.loads(bytes(b.state_json()))
            entity_samples += len(s["entities"])
            samples += 1
    return steps * TICKS_PER_STEP, entity_samples, samples


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--battles", type=int, default=5)
    ap.add_argument("--steps", type=int, default=180,
                    help="steps per battle; 180 x 20 ticks = a 3-minute match")
    ap.add_argument("--read-every", type=int, default=1,
                    help="decode state every N steps; 0 = never")
    args = ap.parse_args()

    total_ticks = ent = samp = 0
    t0 = time.perf_counter()
    for seed in range(1, args.battles + 1):
        ticks, e, s = one_battle(seed, args.steps, args.read_every)
        total_ticks += ticks
        ent += e
        samp += s
    dt = time.perf_counter() - t0

    mean_live = f"{ent / samp:.1f}" if samp else "not sampled"
    print(f"{args.battles} battles x {args.steps} steps x {TICKS_PER_STEP} ticks = "
          f"{total_ticks:,} ticks in {dt:.2f} s")
    if args.read_every:
        print(f"  {total_ticks / dt:,.0f} ticks/s on one core through the Python surface "
              f"(state decoded every {args.read_every} step(s))")
    else:
        print(f"  {total_ticks / dt:,.0f} ticks/s on one core, state never decoded")
    print(f"  mean live entities: {mean_live}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
