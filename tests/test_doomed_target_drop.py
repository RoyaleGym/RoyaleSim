"""A projectile attacker drops a target that damage already in flight will kill (targeting.DOOMED_TARGET_DROP).

WHAT THIS PINS. Call a unit DOOMED at the end of a tick when the damage of the projectiles already flying at it is at
least its hitpoints and that damage lands within 600 ms. On client 15.535.29 an attacker WITH A PROJECTILE (a troop, a
building or a crown tower) that has not yet fired at its target drops that target on the next tick once it is doomed.
Walking or in its windup, flying or on the ground, it makes no difference. It takes another target (a tower when no
other enemy is in sight) and does not pick the doomed unit again while it lives. The damage is summed over the shots
in flight: two shots that are each too weak doom the unit together. Damage that lands later than 600 ms does not
count: the attacker keeps its target until the countdown reaches 600 ms, then drops it on the next tick. Measured on
client 15.535.29, over 40 scenario runs: in 62 of 62 such cases the attacker dropped the target on the next tick
(14 walking Minions, 11 Musketeers and 19 princess towers among them) and none took it back; 2 of them were doomed only
by the sum of two shots; 28 attackers kept a target whose lethal damage was 650-750 ms away and dropped it the tick
after the countdown read 600. The keep arm never drops a doomed target: a walking Minion keeps it, a Minion in its
windup fires at it, and a crown tower and a Musketeer in their windup keep it.

WHY THE CONTROLS ARE HERE. The rule is narrower than "drop a doomed target", and each control refuses one wider
reading. On client 15.535.29 an attacker that has already fired at its target and is in reach keeps it until it dies
(on 1,362 of 1,362 ticks, 219 attacker-target pairs), and an attacker with no projectile keeps it, even while walking
(160 of 160 ticks, 34 pairs, among them a Knight and a Ronin walking at a doomed unit, and an Inferno Tower and an
Electro Wizard, which attack from range). What decides is the projectile, not range or flight: the Minions that drop
are ranged flyers, so a walking Inferno Dragon (a ranged flyer with no projectile) must keep. A target whose pending
damage is below its hitpoints is not doomed. The four controls hold on both arms. The ETA test refuses a rule that
ignores the 600 ms, and the two-shot test refuses a rule that asks one shot to be lethal on its own.

WHICH ARM. projectile_attackers is the SHIPPED arm since the 2026-09-26 flip. keep is the engine before the flip.
The tests below pin each arm BY NAME through the battle's calibration, never through the shipped value, and the last
one runs the shipped build with no override at all.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.DOOMED_TARGET_DROP"
SHIPPED_ARM, KEEP_ARM = "projectile_attackers", "keep"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
P = {name: i for i, name in enumerate(royalesim.PROJECTILE_FIELDS)}
TICK_MS = 50
ETA_LIMIT_MS = 600
#: a red Knight that stands still, attacking the blue right princess tower (14500, 6500) from 2650 away
KNIGHT_AT_TOWER = (14500, 9150)
#: three blue Minions 5700-5800 from it: in sight, and out of reach (3500) until it dies
WALKERS = ((8700, 9150), (8800, 8000), (8900, 10400))
#: a blue Minion that starts 4100 from the Knight, walks 7 ticks and is in its first windup from tick 8
WINDUP_MINION = (10400, 9150)
#: phase codes of `attack_phase`
WINDUP, FIRED = 1, 2
#: `firer_card_id` of a crown tower's arrow
TOWER_FIRER = -1


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME; None runs the shipped build with no override."""
    return {} if arm is None else {KEY: json.dumps(arm)}


def play(cards, spawns, arm, ticks=40):
    """Spawn (team, card index, x, y, hp) and step one tick at a time. Returns states[t] = (entities by uid,
    projectiles) after tick t (states[0] is the reset)."""
    b = royalesim.Battle(list(cards), [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [0] * 8], 0, 200, [10_000, 10_000], None,
            [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns])
    states = []
    for t in range(ticks + 1):
        if t:
            b.step([], 1)
        s = json.loads(b.state_json())
        states.append(({e[F["uid"]]: e for e in s["entities"]}, s["projectiles"]))
    return states


def uids(states, cards, name, team):
    return sorted(u for u, e in states[0][0].items()
                  if e[F["tower_slot"]] < 0 and e[F["team"]] == team and cards[e[F["card_id"]]] == name)


def towers(states):
    return {u for u, e in states[0][0].items() if e[F["tower_slot"]] >= 0}


def shots_at(states, t, victim):
    return [p for p in states[t][1] if p[P["target_uid"]] == victim]


def gone_tick(states, victim):
    gone = [t for t, (ents, _) in enumerate(states) if victim not in ents]
    assert gone, "the victim never died: the scenario drifted"
    return gone[0]


class Doom:
    """The one lethal projectile at `victim`, the first to fly at it from tick `since` on: launched at the end of tick
    `d`, the victim gone on tick `k`."""

    def __init__(self, states, victim, firer_card, since=0):
        at = [t for t in range(since, len(states)) if shots_at(states, t, victim)]
        assert at, "no projectile ever flew at the victim: the scenario drifted"
        self.d = at[0]
        self.k = gone_tick(states, victim)
        assert self.d < self.k, f"the victim died on tick {self.k} before any projectile flew at it"
        first = {p[P["firer_card_id"]] for p in shots_at(states, self.d, victim)}
        assert first == {firer_card}, f"the first shot at the victim came from firer card(s) {first}, not {firer_card}"
        hp = {states[t][0][victim][F["hp"]] for t in range(self.d, self.k)}
        pos = {(states[t][0][victim][F["x"]], states[t][0][victim][F["y"]]) for t in range(self.d, self.k)}
        assert len(hp) == 1, f"the victim's hp changed between the launch and its death: {sorted(hp)}"
        assert len(pos) == 1, "the victim moved in flight, so its ETA is not the ticks to its death"

    def eta_ms(self, t):
        """The lethal damage's ETA at the end of tick t. The victim does not move, so it is the ticks to its death."""
        return (self.k - t) * TICK_MS


def target(states, t, u):
    return states[t][0][u][F["target_uid"]]


def phase(states, t, u):
    return states[t][0][u][F["attack_phase"]]


def assert_dropped_for_good(states, doom, victim, attackers, first, tws):
    """From tick `first` until the victim dies, no attacker targets it, and each one's target is a tower or none."""
    held = {(u, t) for u in attackers for t in range(first, doom.k) if target(states, t, u) == victim}
    assert not held, f"still (or again) targeting the doomed unit, (uid, tick) with D={doom.d}: {sorted(held)}"
    odd = {(u, target(states, first, u)) for u in attackers if target(states, first, u) not in tws | {-1}}
    assert not odd, f"retargeted to something other than a tower or none: {odd}"


def assert_kept(states, doom, victim, attackers, first, last):
    lost = {(u, t) for u in attackers for t in range(first, last + 1) if target(states, t, u) != victim}
    assert not lost, f"let go of the target, (uid, tick) with D={doom.d}, K={doom.k}: {sorted(lost)}"


def walk_scene(arm, knight_hp=60, extra=()):
    cards = ("Knight", "Minions")
    spawns = [(1, 0, *KNIGHT_AT_TOWER, knight_hp)] + [(0, 1, x, y, -1) for x, y in (*WALKERS, *extra)]
    states = play(cards, spawns, arm)
    (knight,) = uids(states, cards, "Knight", 1)
    minions = uids(states, cards, "Minions", 0)
    assert len(minions) == 3 + len(extra)
    return states, knight, minions


def test_walking_minions_drop_a_doomed_target_on_the_next_tick():
    """The client 15.535.29 walking scenario: a tower arrow (ETA 200-250 ms) dooms the Knight; the walkers drop it."""
    states, knight, minions = walk_scene(SHIPPED_ARM)
    doom = Doom(states, knight, firer_card=TOWER_FIRER)
    assert doom.eta_ms(doom.d) <= ETA_LIMIT_MS, f"precondition: the arrow's ETA is {doom.eta_ms(doom.d)} ms"
    for u in minions:
        assert target(states, doom.d, u) == knight, f"precondition: Minion {u} was not after the Knight at D"
        assert all(phase(states, t, u) == 0 for t in range(1, doom.d + 1)), f"precondition: Minion {u} not walking"
    assert_dropped_for_good(states, doom, knight, minions, doom.d + 1, towers(states))


def test_walking_minions_keep_the_target_until_the_eta_reaches_600_ms():
    """The client 15.535.29 slow-arrow scenario: the Knight, attacking a Goblin Cage 8500 from the tower, is doomed by
    an arrow with more than 600 ms to fly. The walkers keep it while the ETA read at the end of the previous tick is
    above 600 ms and drop it on the first tick after it reads 600 or less."""
    cards = ("Knight", "Minions", "GoblinCage")
    spawns = [(1, 0, 14500, 15300, 60), (0, 2, 14500, 13500, -1),
              (0, 1, 10500, 10500, -1), (0, 1, 10000, 11000, -1), (0, 1, 11000, 10000, -1)]
    states = play(cards, spawns, SHIPPED_ARM)
    (knight,) = uids(states, cards, "Knight", 1)
    minions = uids(states, cards, "Minions", 0)
    doom = Doom(states, knight, firer_card=TOWER_FIRER)
    assert doom.eta_ms(doom.d) > ETA_LIMIT_MS, (
        f"precondition: the arrow's ETA at launch is {doom.eta_ms(doom.d)} ms, so this scene cannot test the gate")
    drop = next(t for t in range(doom.d + 1, doom.k) if doom.eta_ms(t - 1) <= ETA_LIMIT_MS)
    for u in minions:
        assert all(phase(states, t, u) == 0 for t in range(1, drop)), f"precondition: Minion {u} was not walking"
    assert_kept(states, doom, knight, minions, doom.d + 1, drop - 1)
    assert_dropped_for_good(states, doom, knight, minions, drop, towers(states))


def test_walking_minions_drop_a_target_doomed_only_by_the_sum_of_two_shots():
    """The client 15.535.29 Phoenix scenario: two walking Minions dropped a Phoenix (hp 317) the tick after a Musketeer
    shot (217) and a Minion spit (107) were both in flight at it; neither alone was lethal. Here a 150-hp Knight is hit
    by the tower's arrow (109) and a fourth Minion's first spit (107). The walkers keep it while only the arrow flies,
    and drop it the tick after both fly. A rule that needs one lethal shot keeps it until the arrow lands."""
    arrow = hit_damage(TOWER_FIRER)
    spit = hit_damage(1)
    states, knight, minions = walk_scene(SHIPPED_ARM, knight_hp=150, extra=(WINDUP_MINION,))
    hp0 = states[0][0][knight][F["hp"]]
    assert max(arrow, spit) < hp0 <= arrow + spit, f"precondition: hp {hp0}, arrow {arrow}, spit {spit}"
    one = next(t for t in range(len(states)) if shots_at(states, t, knight))
    two = next(t for t in range(len(states)) if len(shots_at(states, t, knight)) >= 2)
    k = gone_tick(states, knight)
    landed = next(t for t in range(k) if states[t][0][knight][F["hp"]] < hp0)
    assert one < two < landed < k, f"precondition: one shot on {one}, two on {two}, first landing {landed}, gone {k}"
    assert sorted(p[P["firer_card_id"]] for p in shots_at(states, two, knight)) == [TOWER_FIRER, 1]
    assert (k - two) * TICK_MS <= ETA_LIMIT_MS, "precondition: the second shot lands more than 600 ms after tick two"
    shooter = next(u for u in minions if any(p[P["firer_card_id"]] == 1 for p in shots_at(states, two, knight))
                   and phase(states, two, u) == FIRED)
    walkers = [u for u in minions if u != shooter]
    assert len(walkers) == 3
    for u in walkers:
        assert all(phase(states, t, u) == 0 for t in range(1, two + 1)), f"precondition: Minion {u} not walking"
    lost = {(u, t) for u in walkers for t in range(one + 1, two + 1) if target(states, t, u) != knight}
    assert not lost, f"let go of the Knight while only the arrow (not lethal alone) flew, (uid, tick): {sorted(lost)}"
    held = {(u, t) for u in walkers for t in range(two + 1, k) if target(states, t, u) == knight}
    assert not held, f"still targeting the Knight doomed by two shots on {two}, gone {k}, (uid, tick): {sorted(held)}"


def hit_damage(firer_card):
    """The damage one shot of the tower (TOWER_FIRER) or of a Minion (1) does to a 1000-hp Knight, measured here."""
    spawns = [(1, 0, *KNIGHT_AT_TOWER, 1000)]
    if firer_card == 1:
        spawns.append((0, 1, 11000, 9150, -1))
    states = play(("Knight", "Minions"), spawns, KEEP_ARM, ticks=30)
    (knight,) = uids(states, ("Knight", "Minions"), "Knight", 1)
    first = next(t for t in range(len(states)) if shots_at(states, t, knight))
    assert {p[P["firer_card_id"]] for p in shots_at(states, first, knight)} == {firer_card}
    landed = next(t for t in range(first, len(states)) if states[t][0][knight][F["hp"]] < 1000)
    return 1000 - states[landed][0][knight][F["hp"]]


def test_a_minion_in_its_windup_that_has_not_fired_drops_the_target():
    """The client 15.535.29 attack scenario: a Minion 6 frames into its first windup dropped the doomed Knight."""
    cards = ("Knight", "Minions")
    states = play(cards, [(1, 0, *KNIGHT_AT_TOWER, 60), (0, 1, *WINDUP_MINION, -1)], SHIPPED_ARM)
    (knight,) = uids(states, cards, "Knight", 1)
    (minion,) = uids(states, cards, "Minions", 0)
    doom = Doom(states, knight, firer_card=TOWER_FIRER)
    assert doom.eta_ms(doom.d) <= ETA_LIMIT_MS
    assert phase(states, doom.d, minion) == WINDUP, "precondition: the Minion is not in its windup at D"
    assert all(phase(states, t, minion) != FIRED for t in range(1, doom.d + 1)), (
        "precondition: the Minion fired before the doom, so this is not the not-yet-fired case")
    assert_dropped_for_good(states, doom, knight, [minion], doom.d + 1, towers(states))
    shots = [t for t in range(doom.d + 1, doom.k) for p in shots_at(states, t, knight) if p[P["firer_card_id"]] == 1]
    assert not shots, f"the Minion fired at the doomed Knight on ticks {shots}"


def tower_windup_scene(arm):
    """A blue Minion in reach of the red Knight from tick 1 spits first; the spit (107) dooms the 60-hp Knight while
    the blue right princess tower and a blue Musketeer (a ground troop, 6000 away) are still in the windup of their
    first shot at it."""
    cards = ("Knight", "Minions", "Musketeer")
    states = play(cards, [(1, 0, *KNIGHT_AT_TOWER, 60), (0, 1, 11000, 9150, -1), (0, 2, 8500, 9150, -1)], arm)
    (knight,) = uids(states, cards, "Knight", 1)
    (musketeer,) = uids(states, cards, "Musketeer", 0)
    tower = next(u for u, e in states[1][0].items() if e[F["tower_slot"]] >= 0 and e[F["target_uid"]] == knight)
    doom = Doom(states, knight, firer_card=1)
    assert doom.eta_ms(doom.d) <= ETA_LIMIT_MS
    for u in (tower, musketeer):
        assert target(states, doom.d, u) == knight, f"precondition: {u} was not after the Knight at D"
        assert phase(states, doom.d, u) == WINDUP, f"precondition: {u} is not in its windup at D"
        assert all(phase(states, t, u) != FIRED for t in range(1, doom.d + 1)), f"precondition: {u} fired before D"
    return states, knight, [tower, musketeer], doom


def test_a_crown_tower_and_a_musketeer_in_their_windup_drop_a_target_doomed_by_another_shot():
    """Crown towers and ground troops follow the rule too. On client 15.535.29, princess towers dropped a doomed
    target they had not yet fired at 19 times, and Musketeers 11 times."""
    states, knight, attackers, doom = tower_windup_scene(SHIPPED_ARM)
    assert_dropped_for_good(states, doom, knight, attackers, doom.d + 1, set())


@pytest.mark.parametrize("arm", [SHIPPED_ARM, KEEP_ARM])
def test_an_attacker_that_has_fired_and_is_in_reach_keeps_the_doomed_target(arm):
    """Control: the Minion, in reach from tick 1, spits first (the Knight 150 -> 43); then the tower's arrow dooms
    it. Both have fired at it and are in reach: both keep it until it dies."""
    cards = ("Knight", "Minions")
    states = play(cards, [(1, 0, *KNIGHT_AT_TOWER, 150), (0, 1, 11000, 9150, -1)], arm)
    (knight,) = uids(states, cards, "Knight", 1)
    (minion,) = uids(states, cards, "Minions", 0)
    tower = next(u for u, e in states[1][0].items() if e[F["tower_slot"]] >= 0 and e[F["target_uid"]] == knight)
    landed = next(t for t in range(1, 40) if states[t][0][knight][F["hp"]] < 150)
    doom = Doom(states, knight, firer_card=TOWER_FIRER, since=landed)
    assert states[landed][0][knight][F["hp"]] <= 109, "precondition: the spit left more hp than one arrow"
    assert any(p[P["firer_card_id"]] == 1 for t in range(1, landed) for p in states[t][1]), (
        "precondition: the first damage was not the Minion's spit")
    assert all(target(states, t, minion) == knight for t in range(1, doom.d + 1)), "precondition: the Minion switched"
    assert_kept(states, doom, knight, [minion, tower], doom.d + 1, doom.k - 1)


@pytest.mark.parametrize("arm", [SHIPPED_ARM, KEEP_ARM])
def test_a_walker_without_a_projectile_keeps_the_doomed_target(arm):
    """Control: a blue Knight walking at the doomed red Knight keeps it (client 15.535.29: a Knight and a Ronin)."""
    cards = ("Knight", "Minions")
    states = play(cards, [(1, 0, *KNIGHT_AT_TOWER, 60), (0, 0, 11000, 11000, -1)], arm)
    red, blue = uids(states, cards, "Knight", 1)[0], uids(states, cards, "Knight", 0)[0]
    doom = Doom(states, red, firer_card=TOWER_FIRER)
    assert target(states, doom.d, blue) == red, "precondition: the blue Knight was not after the red one at D"
    assert all(phase(states, t, blue) == 0 for t in range(1, doom.k)), "precondition: the blue Knight did not walk"
    assert_kept(states, doom, red, [blue], doom.d + 1, doom.k - 1)


@pytest.mark.parametrize("arm", [SHIPPED_ARM, KEEP_ARM])
def test_a_ranged_flyer_without_a_projectile_keeps_the_doomed_target(arm):
    """Control: a blue Inferno Dragon (flying, range 3500, no projectile) walking at the doomed red Knight keeps it.
    It refuses a rule keyed on range or on flight, which the walking Minions (flying, range 2500) cannot tell from the
    projectile. Client 15.535.29: ranged attackers without a projectile (an Inferno Tower, an Electro Wizard) kept a
    doomed target on 80 of 80 ticks."""
    cards = ("Knight", "InfernoDragon")
    states = play(cards, [(1, 0, *KNIGHT_AT_TOWER, 60), (0, 1, 8200, 9150, -1)], arm)
    (knight,) = uids(states, cards, "Knight", 1)
    (dragon,) = uids(states, cards, "InfernoDragon", 0)
    doom = Doom(states, knight, firer_card=TOWER_FIRER)
    assert states[0][0][dragon][F["flying"]], "precondition: the Inferno Dragon does not fly"
    assert target(states, doom.d, dragon) == knight, "precondition: the Inferno Dragon was not after the Knight at D"
    assert all(phase(states, t, dragon) == 0 for t in range(1, doom.k)), "precondition: the Inferno Dragon did not walk"
    assert_kept(states, doom, knight, [dragon], doom.d + 1, doom.k - 1)


@pytest.mark.parametrize("arm", [SHIPPED_ARM, KEEP_ARM])
def test_walkers_keep_a_target_whose_pending_damage_is_below_its_hitpoints(arm):
    """Control: the Knight has 300 hp, so the first arrow (109) leaves it alive; the walkers keep it."""
    states, knight, minions = walk_scene(arm, knight_hp=300)
    first = next(t for t, (_, pr) in enumerate(states) if any(p[P["target_uid"]] == knight for p in pr))
    lands = next(t for t in range(first, 40) if states[t][0][knight][F["hp"]] < 300)
    assert states[lands][0][knight][F["hp"]] > 0
    assert all(phase(states, t, u) == 0 for u in minions for t in range(1, lands)), "precondition: not walking"
    lost = {(u, t) for u in minions for t in range(first + 1, lands + 1) if target(states, t, u) != knight}
    assert not lost, f"let go of a target that was not doomed, (uid, tick): {sorted(lost)}"


def test_the_keep_arm_keeps_the_doomed_target():
    """Keep: the walking Minions, and the crown tower and the Musketeer in their windup, keep the doomed Knight until
    it dies."""
    states, knight, minions = walk_scene(KEEP_ARM)
    doom = Doom(states, knight, firer_card=TOWER_FIRER)
    assert_kept(states, doom, knight, minions, doom.d + 1, doom.k - 1)
    states, knight, attackers, doom = tower_windup_scene(KEEP_ARM)
    assert_kept(states, doom, knight, attackers, doom.d + 1, doom.k - 1)


def test_the_shipped_build_drops_the_doomed_target():
    """No override at all. The compiled-in ledger ships SHIPPED_ARM, so the shipped build behaves as the client."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    shipped = ledger["targeting"]["DOOMED_TARGET_DROP"]["value"]
    assert shipped == SHIPPED_ARM, f"{KEY} ships {shipped!r}, not the arm this file names as shipped"
    states, knight, minions = walk_scene(None)
    doom = Doom(states, knight, firer_card=TOWER_FIRER)
    assert_dropped_for_good(states, doom, knight, minions, doom.d + 1, towers(states))
