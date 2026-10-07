"""`royalesim.STATUS_BITS`: the names of an entity row's `status_flags` bits, in bit order.

A reader asks the list for a bit by name. A bit missing from it is one the engine does not report, never one it
reports as unset, so a reader can tell an older engine from a unit that is not grounded.
"""

from __future__ import annotations

import pytest

royalesim = pytest.importorskip("royalesim")


def bit(name: str) -> int:
    return 1 << royalesim.STATUS_BITS.index(name)


def test_status_bits_names_every_bit_in_order():
    assert list(royalesim.STATUS_BITS) == [
        "underground",
        "invisible",
        "hidden",
        "evolved",
        "hero",
        "clone",
        "ability_windup",
        "ability_active",
        "charged",
        "grounded",
    ]
    assert (bit("evolved"), bit("hero"), bit("grounded")) == (8, 16, 512)
    assert "status_flags" in royalesim.ENTITY_FIELDS
