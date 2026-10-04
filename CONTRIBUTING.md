# Contributing

Thanks for helping. The most useful thing you can send is a place where the engine does something the game does
not. Bug reports, fixes, tests and docs are welcome too.

## Reporting a mechanic that differs from the game

Open an issue with the **Differs from the game** template. A report we can act on has:

- **What the game does**, with evidence: a video or replay with timestamps, the client version, the card levels, and
  what you saw happen (a unit's path, a hit's timing, who a tower shoots first).
- **What the engine does** in the same situation: a few lines of Python with the seed, so we can run it.
- **Which rule it is**, if you know. Every measured rule in the engine is an entry in the ledger,
  [data/calibration.json](data/calibration.json), with its value, how sure we are, and where it was measured.
  [docs/calibration.md](docs/calibration.md) explains how to read it, and
  [docs/mechanics.md](docs/mechanics.md) lists the rules by topic.

A rule changes when the evidence shows it: one clear measurement beats a remembered number. If the game behaves
differently in different situations, say which ones you checked.

## Changing the engine

[docs/contributing.md](docs/contributing.md) is the development loop: building, the full set of checks and how to run
them, and the code's conventions. Before you open a pull request:

    cargo test --release
    cargo clippy --release --all-targets -- -D warnings
    python -m pytest -q -rs
    python -m ruff check .

- Add a test that fails without your change and passes with it.
- A skipped test is not a passing one. If a test skips on your machine, say which and why.
- A changed rule changes its ledger entry too: the value, its status and the evidence.
- Keep public text plain: short sentences, and no paths from your own machine.

## Reporting any other bug

Open an issue with the **Something is wrong** template. It asks for a short way to see the problem and the line that
prints your engine's version.

## Licence

By contributing you agree your work is released under this repository's [licence](LICENSE).
