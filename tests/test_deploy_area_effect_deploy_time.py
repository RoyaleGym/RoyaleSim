"""The character a card's area effect spawns deploys one tick longer (spells.DEPLOY_AREA_EFFECT_DEPLOY_TIME).

WHAT THIS PINS. The Ice Wizard and the Electro Wizard cards are area effects (IceWizardCold, ElectroWizardZap) whose
starting action spawns the character (spells.DEPLOY_AREA_EFFECT). On client 16.402 every Ice Wizard recorded from its
first frame F left its deploy on F + 20 and took its first step on F + 21: 4 deploys in 3 battles (20260920-070448
ticks 672 and 2612, 20260920-010218 tick 1446, 20260920-005517 tick 640). The Electro Wizard of 20260920-010218 (tick
2675) agrees. The other single-unit troops recorded from their first frame left their deploy on F + 19: 153 of them,
one seat per battle (Knights, Bombers, Ice Spirits, Giants, Musketeers and others); the Princess (F + 23) and the Golem
(its 3000 ms DeployTime) have rules of their own. Client 15.535.29 agrees: in its single-card sweep scenes both Wizards
leave their deploy on F + 20 and the Ice Wizard attacks on F + 21, where 40 other troops (95 Knights) leave on F + 19.
Today's engine deploys the Wizards as any troop, so they act a tick early.

THE SCENE. Blue plays one card at (3500, 10500) on tick 102, alone on the board. The unit's first frame is F; the test
reads the first frame on which it has moved. A Knight moves on F + 20 under both arms. Under client_one_tick_longer the
Ice Wizard and the Electro Wizard move on F + 21, and on F + 20 under unit_deploy_time. A scenario spawn of the Ice
Wizard (the character put down, not the card played) moves on the same frame under both arms.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module in a
scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop --release`), then run
this file: the named tests go red and the rest stay green.
  * `area_character_deploy_unread` -- client_one_tick_longer deploys the character in its DeployTime:
    test_a_played_wizard_first_moves_a_tick_later_than_a_knight.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spells.DEPLOY_AREA_EFFECT_DEPLOY_TIME"
NEW_ARM, OLD_ARM = "client_one_tick_longer", "unit_deploy_time"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
AT, PLAY_ON = (3500, 10500), 102


def overrides(arm) -> dict:
    """`arm` None runs the build's own value."""
    return {KEY: json.dumps(arm)} if arm is not None else {}


def first_move(card, arm, played=True):
    """(the unit's first frame F, the first frame on which it stands elsewhere than on F), ticks."""
    b = royalesim.Battle([card], [[0, 1, 1], [0, 1, 1]], calibration_overrides=overrides(arm))
    spawns = [] if played else [(0, 0, AT[0] * SUB, AT[1] * SUB, -1)]
    b.reset(0, [[0] * 8, [0] * 8], 0, 100, [10_000, 10_000], None, spawns)
    first, start = None, None
    for _ in range(60):
        tick = json.loads(b.state_json())["tick"]
        cmds = [(0, 0, AT[0] * SUB, AT[1] * SUB)] if played and tick == PLAY_ON else []
        got = b.step(cmds, 1)
        if cmds:
            assert got, "the scene drifted: the play returned nothing"
            assert got[0][1] == 0, f"the scene drifted: the play was refused: {got}"
        s = json.loads(b.state_json())
        units = [e for e in s["entities"] if e[F["team"]] == 0 and e[F["tower_slot"]] < 0]
        if not units:
            continue
        assert len(units) == 1, f"the scene drifted: {len(units)} Blue units"
        pos = (units[0][F["x"]], units[0][F["y"]])
        if first is None:
            first, start = s["tick"], pos
        elif pos != start:
            return first, s["tick"]
    raise AssertionError(f"the scene drifted: the {card} did not move within 60 ticks")


@pytest.mark.parametrize("wizard", ["IceWizard", "ElectroWizard"])
def test_a_played_wizard_first_moves_a_tick_later_than_a_knight(wizard):
    f, moved = first_move(wizard, NEW_ARM)
    kf, kmoved = first_move("Knight", NEW_ARM)
    assert kmoved - kf == 20, f"the scene drifted: the Knight first moved on F + {kmoved - kf}, not F + 20"
    assert moved - f == 21, f"under {NEW_ARM} the {wizard} first moved on F + {moved - f}; the client: F + 21"


@pytest.mark.parametrize("wizard", ["IceWizard", "ElectroWizard"])
def test_the_old_arm_deploys_a_wizard_as_a_knight(wizard):
    f, moved = first_move(wizard, OLD_ARM)
    assert moved - f == 20, f"under {OLD_ARM} the {wizard} first moved on F + {moved - f}, not F + 20"


def test_a_knight_and_a_scenario_wizard_are_the_same_under_both_arms():
    assert first_move("Knight", NEW_ARM) == first_move("Knight", OLD_ARM), "the arms part on a Knight"
    new = first_move("IceWizard", NEW_ARM, played=False)
    assert new == first_move("IceWizard", OLD_ARM, played=False), (
        "the arms part on an Ice Wizard put down by a scenario"
    )


def test_the_shipped_value_is_the_new_arm():
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    assert ledger["spells"]["DEPLOY_AREA_EFFECT_DEPLOY_TIME"]["value"] == NEW_ARM
    assert first_move("IceWizard", None) == first_move("IceWizard", NEW_ARM), "the build's own value is not the new arm"
