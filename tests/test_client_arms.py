"""client_arms: the calibration overrides that run one client's measured setup (docs/api.md).

WHAT THIS PINS. `royalesim.client_arms("15.535.29")` is a dict a Battle takes as its `calibration_overrides`: the older
client's measured mechanics and the card tables' own values (cards.CLIENT16402_VALUES at arm none). The Fire Spirits
show it: the ledger runs the current client's Hitpoints 84, the 15.535.29 tables 85, so at level 11 a spirit has 215
under the default and 217 under the setup. "16.402" is the ledger itself ({}); an unknown client is a ValueError.

PLANTS. `client_arms_keep_card_values` (crates/royalesim/src/client_arms.rs) turns
test_the_15535_setup_runs_the_tables_own_card_values red.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")


def spirit_hp(overrides):
    """A Fire Spirit's hitpoints at level 11 in a Battle built with `overrides`."""
    b = royalesim.Battle(["FireSpirits", "Knight"], [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides)
    return next(hp for role, _name, hp in b.unit_hitpoints(0, 11) if role == "own")


def test_the_15535_setup_runs_the_tables_own_card_values():
    o = royalesim.client_arms("15.535.29")
    assert all(json.loads(v) is not None for v in o.values()), "every value is JSON"
    assert json.loads(o["cards.CLIENT16402_VALUES"])["arm"] == "none"
    assert spirit_hp({}) == 215, "the ledger runs the current client's Fire Spirits"
    assert spirit_hp(o) == 217, "the 15.535 setup runs the tables' own"


def test_the_current_client_is_the_ledger_and_an_unknown_client_is_refused():
    assert royalesim.client_arms("16.402") == {}
    assert royalesim.client_arms("15.535.13") == royalesim.client_arms("15.535.29")
    with pytest.raises(ValueError, match=r"15\.535\.29"):
        royalesim.client_arms("17.0")
