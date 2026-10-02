---
name: Something is wrong
about: The engine crashes, refuses something it should take, or does something the game does not
labels: bug
---

**What happened**

**What you expected**

**How to see it**
A few lines of Python that show it, with the seed.

**Your setup**
Paste the output of:

    python -c "import royalesim, sys; print(royalesim.Battle.provenance(), royalesim.card_table_source(), sys.version)"
