//! THE RUNE GIANT'S ENCHANT (card.rs `EnchantDef`, `CardDb::resolve_enchants`; state.rs `enchant_pass`,
//! `launch_due_enchants`, `apply_effects`, `tick_enchant_finish`; combat.rs `enchant_bonus`; the ledger's `enchant`
//! section).
//!
//! THE LAW, measured on client 15.535.29 (21 Rune Giant runs):
//!   - his ActionDelay (1000) counts from the tick he is created: his first look is 20 ticks later. With nobody in
//!     reach he looks again every tick;
//!   - a look picks the nearest own troops in reach (air in, buildings out, deploying in, an enchanted friend out), the
//!     reach [7536.9, 7560) centre to centre on start-of-tick positions (7550 an interim), as many as he has free
//!     places (MaxFriendlyTroops 2, held at once);
//!   - the projectile leaves 7 ticks after the pick from where he stands after his move, steps 600 a tick from the next
//!     tick, and the enchant takes on the tick it lands. The next look is Cooldown 3000 after the launch;
//!   - the bonus lands on the carrier's 3rd, 6th, 9th ... attack after the enchant: AddedDamage 86 scaled at HIS level
//!     (+220 at 11, +182 at 9), per mille of the level-1 figure for a listed attacker (the Electro Wizard 500: +110 a
//!     bolt; the Hunter 100: +20 a pellet), on every splash victim, AddedCrownTowerDamage on a crown tower;
//!   - the enchant outlives him by FinishIfInstigatorDies (5000 ms): paid 80 ticks after his death, not 120;
//!   - a stun holds his look, not his clocks; he does not stop walking when he enchants.
//!
//! WHAT IS PINNED, each with its precondition (F is the Rune Giant's creation tick; each test names the arms it runs
//! through its config, `shipped()` or `with()`):
//!   1. the loader reads his EnchantDef, and resolves the multipliers by the rows each card's unit fires (the Ram
//!      Rider's rider, units.RamRider, takes its bola's 0; TriWizards, whose own unit is the TriWizard and whose row
//!      fires the Wizard's projectile, takes nothing) and the tagged PhoenixEgg into `excluded`;
//!   2. the first projectile appears at the end of F + 27 on his post-move position and is 600 (+-1) nearer the friend
//!      one tick later; under the named arms at_deploy_end F + 47 and buff_delay_ceil F + 26;
//!   3. alone past his first look, a Knight created on F + 36 is sent a projectile on F + 43 (restart_cooldown: F + 88);
//!   4. a look picks the two nearest friends whatever their creation order, and Minions over a nearer Cannon;
//!   5. with both still deploying, a friend at centre 7537 is picked on F + 20 and one at 7560 is not (the named arms
//!      centre_7000 and centre_7750 each flip one);
//!   6. the enchant appears in the Resolve of the tick the projectile lands, not before; a projectile whose friend
//!      died in flight enchants nobody;
//!   7. an enchanted Knight hits a Red Knight for 202, 202, 422, repeating (first_three_attacks, the Rune Giant killed
//!      so that he sends no second enchant: 422 on 1 to 3, the enchant gone with the 3rd, then 202);
//!   8. a level-9 Rune Giant gives +182 to carriers at 11 and 12 (carrier_level: 220 and 241);
//!   9. the Electro Wizard's bonus attack deals 227 a bolt, the Hunter's bonus volley 104 a pellet (per_mille_of_scaled:
//!      106);
//!  10. a Valkyrie's bonus swing gives 486 to both Red Knights beside her (primary_target_only: one of them);
//!  11. a Musketeer's shot whose target died in flight still counts: the bonus lands on its 3rd and 6th shots;
//!  12. a crown tower takes AddedCrownTowerDamage: 202, 202, 422 on a Red princess tower; with the column at 40, 304 on
//!      the tower and still 422 on a troop (added_damage_with_attacker_percent: 422 on the tower);
//!  13. the enchant outlives him: killed 80 ticks before the carrier's 6th hit, it pays; 120 before, it does not, while
//!      the 3rd hit of that run still does (ends_at_death: neither);
//!  14. a free place refills: the second launch is 68 ticks after the first, to the nearer Archer only, the enchanted
//!      Knight never sent a second one (fresh_pick_of_max_targets: both Archers; from_pick: 61 ticks);
//!  15. one friend over 400 ticks is sent one projectile;
//!  16. a stun inside his deploy leaves the projectile on F + 27; a stun on the due look delays it to the first unheld
//!      tick + 7 (timer_pauses and ignored each give another tick). The client's projectile came 2 ticks earlier than
//!      this rule gives: the named gap of enchant.STUN_AT_PICK;
//!  17. he does not stop walking around the launch;
//!  18. a battle with a pending launch, a projectile in flight and an enchant mid-count survives save and load, and the
//!      enchant, the look and the projectile are state (an edited save fails the load's hash self-check);
//!  19. a row tagged NO_GIANTBUFFER_CHEF_ENCHANTMENT is never picked.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test enchant`):
//!   * `enchant_multiplier_by_card_name` -- a multiplier matched by the card's own name: (1) goes red.
//!   * `enchant_never_picks` -- a look finds nobody: (2), (3), (4), (5), (6), (14), (15), (17) go red.
//!   * `enchant_launch_at_pick` -- the projectile leaves on the pick tick: (2), (5) and (16) go red.
//!   * `enchant_empty_pick_waits_cooldown` -- an empty look waits a whole Cooldown: (3) goes red.
//!   * `enchant_picks_by_creation` -- the earliest created first: (4) goes red.
//!   * `enchant_picks_buildings` -- his own buildings may be picked: (4) goes red.
//!   * `enchant_reach_7500` -- the reach 7500: (5) goes red.
//!   * `projectile_enchant_dropped` -- a landing enchants nobody: (6) to (13) go red.
//!   * `enchant_bonus_ignored` -- no attack carries the bonus: (7) to (13) go red.
//!   * `enchant_bonus_first_three` -- the bonus on attacks 1 to 3: (7), (9), (10), (11), (12), (13) go red.
//!   * `enchant_scaled_by_carrier` -- the bonus at the carrier's level: (8) goes red.
//!   * `enchant_multiplier_after_scaling` -- per mille of the scaled bonus: (9) goes red (the Hunter's 106).
//!   * `enchant_multiplier_percent` -- the table's values read as percents: (9) goes red.
//!   * `splash_bonus_primary_only` -- one splash victim takes the bonus: (10) goes red.
//!   * `enchant_shots_uncounted` -- a launch is not counted: (9) and (11) go red.
//!   * `crown_bonus_from_added_damage` -- a crown tower takes AddedDamage: (12) goes red.
//!   * `enchant_outlives_instigator` -- an enchant lasts for ever: (13) goes red.
//!   * `enchant_ends_with_instigator` -- it ends with him: (13) goes red.
//!   * `enchant_fresh_pick_of_two` -- every look picks two: (14) goes red.
//!   * `enchant_cooldown_from_pick` -- the Cooldown counts from the pick: (14) goes red.
//!   * `enchant_repicks_enchanted` -- an enchanted friend is picked again: (15) goes red.
//!   * `enchant_stun_pauses_timer` -- a stun stops his clocks: (16) goes red.
//!   * `enchant_stun_ignored` -- he looks while held: (16) goes red.
//!   * `enchant_pause_on_launch` -- he stands 600 ms on each launch: (17) goes red.
//!   * `save_drops_enchant` -- the enchants are lost across a save: (18) goes red.
//!   * `hash_skips_enchant` -- the look and the enchant are not hashed: (18) goes red.
//!   * `enchant_picks_tagged_unit` -- the tagged rows are picked: (19) goes red.
//!
//! Scenes: the Rune Giant and his friends are played with `spawn_unit` (they deploy, as a play does) unless a test says
//! otherwise, and the unit that takes the hits is given 1,000,000 hp so that no death ends a count. Where a Red unit
//! stands in Blue's half, Blue's princess towers are taken down first, so no tower's hit mixes into what is measured.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::CardDb;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{
    BattleConfig, BattleState, Calib, EnchantBonusAttacks, EnchantBonusLevel, EnchantCooldownOrigin, EnchantCrownBonus, EnchantDelayOrigin, EnchantEmptyPick,
    EnchantInstigatorDeath, EnchantLaunchDelay, EnchantMultiplier, EnchantPickOrder, EnchantPickReach, EnchantSlots, EnchantSplashBonus, EnchantStunAtPick,
};
use royalesim::{EntityId, Team};

/// Hit points that no fight in these scenes gets through.
const BIG: i32 = 1_000_000;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(v: Vec2) -> (i64, i64) {
    ((v.x / K) as i64, (v.y / K) as i64)
}

fn dist(a: Vec2, b: Vec2) -> i64 {
    let (a, b) = (native(a), native(b));
    isqrt((a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1))
}

/// The shipped enchant arms, asserted, so a ledger change re-points this file instead of silently moving its numbers.
fn shipped() -> BattleConfig {
    let cfg = config();
    let c = &cfg.calib;
    assert_eq!(c.enchant_collect_delay_origin, EnchantDelayOrigin::FromCreation, "the shipped enchant.COLLECT_DELAY_ORIGIN");
    assert_eq!(c.enchant_launch_delay, EnchantLaunchDelay::BuffDelayCeilPlusOne, "the shipped enchant.LAUNCH_DELAY");
    assert_eq!(c.enchant_pick_reach, EnchantPickReach::Centre7550, "the shipped enchant.PICK_REACH");
    assert_eq!(c.enchant_pick_order, EnchantPickOrder::NearestThenTeamSeq, "the shipped enchant.PICK_ORDER");
    assert_eq!(c.enchant_slots, EnchantSlots::HeldAtOnce, "the shipped enchant.SLOTS");
    assert_eq!(c.enchant_empty_pick, EnchantEmptyPick::RetryEachTick, "the shipped enchant.EMPTY_PICK");
    assert_eq!(c.enchant_cooldown_origin, EnchantCooldownOrigin::FromLaunch, "the shipped enchant.COOLDOWN_ORIGIN");
    assert_eq!(c.enchant_stun_at_pick, EnchantStunAtPick::PickWaitsForStunEnd, "the shipped enchant.STUN_AT_PICK");
    assert_eq!(c.enchant_bonus_attacks, EnchantBonusAttacks::EveryThirdFromEnchant, "the shipped enchant.BONUS_ATTACKS");
    assert_eq!(c.enchant_bonus_level, EnchantBonusLevel::Instigator, "the shipped enchant.BONUS_LEVEL_SCALING");
    assert_eq!(c.enchant_multiplier, EnchantMultiplier::Level1ThenScaled, "the shipped enchant.MULTIPLIER");
    assert_eq!(c.enchant_splash_bonus, EnchantSplashBonus::EveryVictim, "the shipped enchant.SPLASH_BONUS");
    assert_eq!(c.enchant_crown_bonus, EnchantCrownBonus::CrownColumn, "the shipped enchant.CROWN_TOWER_BONUS");
    assert_eq!(c.enchant_instigator_death, EnchantInstigatorDeath::FinishAfter, "the shipped enchant.INSTIGATOR_DEATH");
    cfg
}

/// The shipped config with one or more enchant arms named by the test.
fn with(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = shipped();
    f(&mut cfg.calib);
    cfg
}

/// Every live `card` of `team`.
fn find(s: &BattleState, team: Team, card: &str) -> Vec<EntityId> {
    s.entities().filter(|v| v.team == team && v.card == card).map(|v| v.id).collect()
}

/// The one live `card` of `team`.
fn the(s: &BattleState, team: Team, card: &str) -> EntityId {
    let v = find(s, team, card);
    assert_eq!(v.len(), 1, "one {card} of {team:?} on the board: {v:?}");
    v[0]
}

/// The Rune Giant's projectiles in flight: (target, position).
fn bolts(s: &BattleState) -> Vec<(EntityId, Vec2)> {
    s.projectiles().iter().filter(|p| p.enchant.is_some()).map(|p| (p.target, p.pos)).collect()
}

/// Run `ticks` ticks; every Rune Giant launch, as (the tick it left on, its target). A projectile is visible at the
/// end of its launch tick and cannot land before the next, so a new target in the list is a launch.
fn launches(s: &mut BattleState, ticks: u32) -> Vec<(u32, EntityId)> {
    let mut prev: Vec<EntityId> = bolts(s).iter().map(|b| b.0).collect();
    let mut out = Vec::new();
    for _ in 0..ticks {
        let k = s.tick_count();
        s.tick();
        let now: Vec<EntityId> = bolts(s).iter().map(|b| b.0).collect();
        for t in &now {
            if !prev.contains(t) {
                out.push((k, *t));
            }
        }
        prev = now;
    }
    out
}

/// The enchant count of `carrier` (0 without one).
fn count_of(s: &BattleState, carrier: EntityId) -> u32 {
    s.entity(carrier).and_then(|v| v.enchant).map_or(0, |e| e.count)
}

/// Run up to `ticks` ticks until `n` hp drops of `foe` have been seen; each as (the tick, `carrier`'s enchant count
/// after it, the drop).
fn drops(s: &mut BattleState, carrier: EntityId, foe: EntityId, ticks: u32, n: usize) -> Vec<(u32, u32, i32)> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        let Some(before) = s.entity(foe).map(|v| v.hp) else { break };
        let k = s.tick_count();
        s.tick();
        let Some(after) = s.entity(foe).map(|v| v.hp) else { break };
        if after < before {
            out.push((k, count_of(s, carrier), before - after));
            if out.len() >= n {
                break;
            }
        }
    }
    out
}

/// Where a Blue friend and the Blue Rune Giant are played: 2000 apart in the middle of Blue's half.
const FRIEND_AT: (i32, i32) = (9000, 12000);
const GIANT_AT: (i32, i32) = (9000, 10000);

/// A Blue `friend` and a Blue Rune Giant 2000 behind it, played on one tick with Blue's princess towers down (so no
/// tower reaches a fight in the middle of Blue's half); both given BIG hp once they stand, and run until the friend
/// carries the enchant. Returns (battle, friend, Rune Giant).
fn enchanted(cfg: BattleConfig, friend: &str, friend_level: Option<i32>, giant_level: Option<i32>) -> (BattleState, EntityId, EntityId) {
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).expect("take a Blue princess tower down");
    s.scenario_set_tower_hp(Team::Blue, 2, 0).expect("take the other down");
    s.spawn_unit(Team::Blue, friend, at(FRIEND_AT), friend_level).expect("play the friend");
    s.spawn_unit(Team::Blue, "GiantBuffer", at(GIANT_AT), giant_level).expect("play the Rune Giant");
    s.tick();
    let giant = the(&s, Team::Blue, "GiantBuffer");
    let f = the(&s, Team::Blue, friend);
    assert!(s.debug_set_hp(giant, BIG) && s.debug_set_hp(f, BIG));
    let k = run_until(&mut s, 60, |s| s.entity(f).is_some_and(|v| v.enchant.is_some()));
    assert!(k < 60, "the scene drifted: the {friend} was not enchanted within 60 ticks");
    assert_eq!(count_of(&s, f), 0, "the {friend} attacked before its enchant landed: the count's start is not measured");
    (s, f, giant)
}

/// A Red `card` with BIG hp beside `friend`, 1300 off it on the first dry side of four.
fn foe_beside(s: &mut BattleState, friend: EntityId, card: &str, off: (i32, i32)) -> EntityId {
    let p = native(s.entity(friend).expect("the friend stands").pos);
    for (dx, dy) in [off, (-off.0, -off.1), (off.1, off.0), (-off.1, -off.0)] {
        let q = at((p.0 as i32 + dx, p.1 as i32 + dy));
        if let Ok(id) = s.scenario_spawn_now(Team::Red, card, q, Some(BIG)) {
            return id;
        }
    }
    panic!("no dry point beside the friend at {p:?}");
}

/// A level-1 figure of `card` scaled at `level`.
fn scaled(db: &CardDb, card: &str, level: i32, base: i32) -> i32 {
    db.scaled(db.index(card).unwrap_or_else(|| panic!("{card} loads")), level, base).expect("a level the card has")
}

// ---------------------------------------------------------------------------
// 1. the loader

/// Plant: enchant_multiplier_by_card_name.
#[test]
fn the_rune_giant_loads_with_his_enchant() {
    let s = BattleState::new(0, config());
    let e = card_stat(&s, "GiantBuffer").enchant.clone().expect("the Rune Giant carries his enchant");
    assert_eq!(
        (e.first_ms, e.cooldown_ms, e.max_targets, e.pick_radius, e.buff_radius, e.buff_delay_ms, e.bolt_speed, e.period, e.added, e.added_crown, e.finish_ms),
        (1000, 3000, 2, 7000 * K, 8500 * K, 280, 600, 3, 86, 86, 5000),
        "the 15.535.29 tables' ActionGiantBufferCollectFriends and ActionGiantBufferBuff"
    );
    let db = s.cards();
    let idx = |n: &str| db.index(n).unwrap_or_else(|| panic!("{n} loads"));
    // The names-differ case where no multiplier names the row: TriWizards' own unit is the TriWizard, which fires the
    // Wizard's projectile, and no multiplier names it (its Electro Wizard is the ElectroWizard card's record).
    assert_eq!(db.get(idx("TriWizards")).unit_name, "TriWizard", "the scene: TriWizards' own unit is the TriWizard");
    // A spawned unit's row: the Ram Rider's rider (units.RamRider, the row RamRider) fires RamRiderBola, which the table
    // lists at 0. The bola releases no spark, so the spark keeps 1000. The card's own unit, the Ram, fires nothing listed.
    assert_eq!(db.get(idx("units.RamRider")).unit_name, "RamRider", "the scene: the rider is the RamRider row");
    let mut want = vec![
        (idx("ElectroWizard"), 500, 1000),
        (idx("Hunter"), 100, 1000),
        (idx("Firecracker"), 1000, 200),
        (idx("units.RamRider"), 0, 1000),
        // The Goblin Giant's riders (the row SpearGoblinGiant) fire SpearGoblinGiantProjectile, which the table lists at
        // 0, as the Ram Rider's rider's bola.
        (idx("SpearGoblinGiant"), 0, 1000),
    ];
    want.sort_unstable();
    assert_eq!(e.per_attacker, want, "the loaded attackers the table's multipliers reach, by the rows their units fire");
    assert_eq!(e.per_mille(idx("Knight")), (1000, 1000), "an unlisted attacker takes the whole bonus");
    assert_eq!(e.per_mille(idx("RamRider")), (1000, 1000), "the Ram Rider card's Ram is unlisted: only its rider takes the 0");
    let egg = db.index("PhoenixEgg").expect("the Phoenix's egg loads with the Phoenix");
    assert_eq!(e.excluded, vec![egg], "the tagged row, PhoenixEgg, is excluded from the pick");
    assert_eq!(e.excluded_units, vec!["PhoenixEgg".to_string()]);
}

// ---------------------------------------------------------------------------
// 2, 3. the look and the launch

/// The first-launch scene measured on client 15.535.29: a Knight played at (14500, 8500), the Rune Giant at (14500, 14500) 20 ticks
/// later. Returns (battle, F, Knight, Rune Giant) right after F.
fn lag_scene(cfg: BattleConfig) -> (BattleState, u32, EntityId, EntityId) {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "Knight", at((14500, 8500)), None).expect("play the Knight");
    for _ in 0..20 {
        s.tick();
    }
    s.spawn_unit(Team::Blue, "GiantBuffer", at((14500, 14500)), None).expect("play the Rune Giant");
    let f = s.tick_count();
    s.tick();
    let knight = the(&s, Team::Blue, "Knight");
    let giant = the(&s, Team::Blue, "GiantBuffer");
    (s, f, knight, giant)
}

/// The tick the first projectile left on, in the lag scene under `cfg`, less F.
fn first_launch_after_f(cfg: BattleConfig) -> u32 {
    let (mut s, f, knight, _) = lag_scene(cfg);
    let got = launches(&mut s, 80);
    let &(k, t) = got.first().expect("a projectile left within 80 ticks");
    assert_eq!(t, knight, "the first projectile goes to the Knight");
    k - f
}

/// Plants: enchant_never_picks, enchant_launch_at_pick.
#[test]
fn the_first_projectile_leaves_27_ticks_after_he_appears() {
    let (mut s, f, knight, giant) = lag_scene(shipped());
    for _ in f + 1..f + 27 {
        s.tick();
        assert!(bolts(&s).is_empty(), "a projectile before F + 27 (tick {})", s.tick_count() - 1);
    }
    s.tick();
    let b = bolts(&s);
    assert_eq!(b.len(), 1, "one projectile at the end of F + 27: {b:?}");
    assert_eq!(b[0].0, knight);
    let giant_pos = s.entity(giant).expect("he stands").pos;
    assert_eq!(b[0].1, giant_pos, "it appears where the Rune Giant stands after his move");
    s.tick();
    let next = bolts(&s);
    assert_eq!(next.len(), 1, "still in flight on F + 28");
    let step = dist(b[0].1, next[0].1);
    assert!((599..=601).contains(&step), "its first step is 600 (the row's Speed), got {step}");
}

/// The first launch under the named arms of enchant.COLLECT_DELAY_ORIGIN and enchant.LAUNCH_DELAY.
#[test]
fn the_first_projectile_under_each_named_arm() {
    assert_eq!(first_launch_after_f(shipped()), 27, "from_creation and bolt_7_ticks_after_pick");
    assert_eq!(first_launch_after_f(with(|c| c.enchant_launch_delay = EnchantLaunchDelay::BuffDelayCeil)), 26, "buff_delay_ceil");
    assert_eq!(first_launch_after_f(with(|c| c.enchant_launch_delay = EnchantLaunchDelay::AtPick)), 20, "at_pick");
    assert_eq!(first_launch_after_f(with(|c| c.enchant_collect_delay_origin = EnchantDelayOrigin::AtDeployEnd)), 47, "at_deploy_end");
}

/// The empty-reach scene measured on client 15.535.29: the Rune Giant alone at (14500, 14500), a Knight played at (14500, 12500) on F + 36, the Knight's
/// first tick. The launches over the next `ticks` ticks, as (tick - F, target is the Knight).
fn empty_scene(cfg: BattleConfig, ticks: u32) -> Vec<(u32, bool)> {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "GiantBuffer", at((14500, 14500)), None).expect("play the Rune Giant");
    let f = s.tick_count();
    for _ in 0..36 {
        s.tick();
    }
    assert!(bolts(&s).is_empty(), "no projectile while he is alone");
    s.spawn_unit(Team::Blue, "Knight", at((14500, 12500)), None).expect("play the Knight");
    let got = launches(&mut s, ticks);
    let knight = the(&s, Team::Blue, "Knight");
    got.into_iter().map(|(k, t)| (k - f, t == knight)).collect()
}

/// Plant: enchant_empty_pick_waits_cooldown.
#[test]
fn an_empty_pick_is_retried_every_tick() {
    assert_eq!(empty_scene(shipped(), 20).first(), Some(&(43, true)), "retry_each_tick: the Knight's first tick + 7");
    assert_eq!(empty_scene(with(|c| c.enchant_empty_pick = EnchantEmptyPick::RestartCooldown), 70).first(), Some(&(88, true)), "restart_cooldown: F + 20 + 61 + 7");
}

// ---------------------------------------------------------------------------
// 4, 5. the pick

/// Friends played (in order) at their points, the Rune Giant at (14500, 14500) 2 ticks later; the targets of his first
/// launch, by card and team_seq.
fn first_picks(cfg: BattleConfig, friends: &[(&str, (i32, i32))]) -> (BattleState, Vec<EntityId>) {
    let mut s = BattleState::new(0, cfg);
    for (card, p) in friends {
        s.spawn_unit(Team::Blue, card, at(*p), None).expect("play the friend");
    }
    s.tick();
    s.tick();
    s.spawn_unit(Team::Blue, "GiantBuffer", at((14500, 14500)), None).expect("play the Rune Giant");
    let got = launches(&mut s, 40);
    let first = got.first().map(|g| g.0).expect("a launch");
    let picks = got.iter().filter(|g| g.0 == first).map(|g| g.1).collect();
    (s, picks)
}

/// Plants: enchant_picks_by_creation, enchant_picks_buildings.
#[test]
fn picks_the_nearest_troops_air_in_buildings_out() {
    // The order scene measured on client 15.535.29: the Knight played first 6000 behind him, the two Archers 3000 behind.
    let (s, picks) = first_picks(shipped(), &[("Knight", (14500, 8500)), ("Archer", (14500, 11500))]);
    let archers = find(&s, Team::Blue, "Archer");
    assert_eq!(archers.len(), 2, "the Archers card puts two on the board");
    let mut p = picks.clone();
    p.sort_unstable();
    let mut a = archers.clone();
    a.sort_unstable();
    assert_eq!(p, a, "the two nearest, the Archers, though the Knight was played first: {picks:?}");
    // The same with the creation order swapped: the Archers played first 6000 behind him, the Knight 3000 behind.
    let (s, picks) = first_picks(shipped(), &[("Archer", (14500, 8500)), ("Knight", (14500, 11500))]);
    let knight = the(&s, Team::Blue, "Knight");
    assert_eq!(picks.len(), 2, "two places: {picks:?}");
    assert!(picks.contains(&knight), "the nearest, the Knight, though the Archers were played first: {picks:?}");
    // enchant.PICK_ORDER = creation, named: the Archers, played first.
    let (s, picks) = first_picks(with(|c| c.enchant_pick_order = EnchantPickOrder::Creation), &[("Archer", (14500, 8500)), ("Knight", (14500, 11500))]);
    assert!(!picks.contains(&the(&s, Team::Blue, "Knight")), "creation: the two Archers, played first: {picks:?}");
    // The filter scene: a Cannon 2000 behind him, Minions 5000 behind: the Minions, air, and never the building.
    let (s, picks) = first_picks(shipped(), &[("Cannon", (14500, 12500)), ("Minions", (14500, 9500))]);
    let minions = find(&s, Team::Blue, "Minions");
    assert_eq!(picks.len(), 2, "two places: {picks:?}");
    assert!(picks.iter().all(|p| minions.contains(p)), "Minions only, never the nearer Cannon: {picks:?}");
}

/// Both still deploying: the Rune Giant at (2500, 11000) and a Knight `dx` to his right, set there after their
/// creation tick. The first launch, as (tick - F, target is the Knight), within F + 27.
fn reach_scene(cfg: BattleConfig, dx: i32) -> Option<(u32, bool)> {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "GiantBuffer", at((2500, 11000)), None).expect("play the Rune Giant");
    s.spawn_unit(Team::Blue, "Knight", at((2500 + dx, 11000)), None).expect("play the Knight");
    let f = s.tick_count();
    s.tick();
    let (giant, knight) = (the(&s, Team::Blue, "GiantBuffer"), the(&s, Team::Blue, "Knight"));
    assert!(s.debug_set_pos(giant, at((2500, 11000))) && s.debug_set_pos(knight, at((2500 + dx, 11000))));
    assert!(s.entity(giant).is_some_and(|v| v.deploying) && s.entity(knight).is_some_and(|v| v.deploying), "both deploying");
    let got = launches(&mut s, 27);
    got.first().map(|&(k, t)| (k - f, t == knight))
}

/// Plant: enchant_reach_7500.
#[test]
fn the_reach_on_start_of_tick_positions() {
    assert_eq!(reach_scene(shipped(), 7537), Some((27, true)), "7537: picked on F + 20 (the bound lies in [7536.9, 7560))");
    assert_eq!(reach_scene(shipped(), 7560), None, "7560: not picked on F + 20");
    assert_eq!(reach_scene(with(|c| c.enchant_pick_reach = EnchantPickReach::Centre7000), 7537), None, "centre_7000, named: 7537 is out");
    assert_eq!(reach_scene(with(|c| c.enchant_pick_reach = EnchantPickReach::Centre7750), 7560), Some((27, true)), "centre_7750, named: 7560 is in");
}

// ---------------------------------------------------------------------------
// 6. the landing

/// Plant: projectile_enchant_dropped.
#[test]
fn the_projectile_enchants_on_its_landing_tick() {
    let (mut s, _, knight, giant) = lag_scene(shipped());
    run_until(&mut s, 40, |s| !bolts(s).is_empty());
    assert!(!bolts(&s).is_empty(), "a projectile left");
    let mut last = bolts(&s)[0].1;
    let mut landed = None;
    for _ in 0..30 {
        assert!(s.entity(knight).and_then(|v| v.enchant).is_none(), "the Knight enchanted while the projectile still flies");
        s.tick();
        match bolts(&s).first() {
            Some(b) => {
                assert!(dist(last, b.1) <= 601, "a step of {} (600 a tick)", dist(last, b.1));
                last = b.1;
            }
            None => {
                landed = Some(s.tick_count() - 1);
                break;
            }
        }
    }
    assert!(landed.is_some(), "the projectile landed within 30 ticks");
    let e = s.entity(knight).and_then(|v| v.enchant).expect("the Knight carries the enchant at the end of the landing tick");
    assert_eq!((e.source, e.count, e.finish_ms), (giant, 0, -1), "his enchant, no attack counted, lasting while he lives");
    // A projectile whose friend died in flight enchants nobody.
    let (mut s, _, knight, _) = lag_scene(shipped());
    run_until(&mut s, 40, |s| !bolts(s).is_empty());
    assert!(s.debug_set_hp(knight, 0), "kill the Knight in flight");
    run_until(&mut s, 30, |s| bolts(s).is_empty());
    assert!(bolts(&s).is_empty(), "the projectile landed");
    assert!(s.entities().all(|v| v.enchant.is_none()), "a projectile whose friend died enchanted somebody");
}

// ---------------------------------------------------------------------------
// 7 to 13. the bonus

/// (attack number, drop) for the first `n` hits of an enchanted Blue Knight on a Red Knight beside it.
fn knight_hits(cfg: BattleConfig, n: usize) -> Vec<(u32, i32)> {
    let (mut s, knight, _) = enchanted(cfg, "Knight", None, None);
    let foe = foe_beside(&mut s, knight, "Knight", (1300, 0));
    drops(&mut s, knight, foe, 400, n).into_iter().map(|(_, c, d)| (c, d)).collect()
}

/// Plants: enchant_bonus_ignored, enchant_bonus_first_three.
#[test]
fn the_bonus_lands_on_every_third_attack() {
    let db = cards();
    let (base, bonus) = (scaled(&db, "Knight", 11, card_stat(&BattleState::new(0, config()), "Knight").damage), scaled(&db, "GiantBuffer", 11, 86));
    assert_eq!((base, bonus), (202, 220), "measured on client 15.535.29: a Knight's hit and the level-11 bonus");
    let got = knight_hits(shipped(), 9);
    assert_eq!(got.iter().map(|h| h.0).collect::<Vec<_>>(), (1..=9).collect::<Vec<u32>>(), "one drop per attack: {got:?}");
    for &(k, d) in &got {
        let want = if k % 3 == 0 { base + bonus } else { base };
        assert_eq!(d, want, "attack {k}: {got:?}");
    }
    // enchant.BONUS_ATTACKS = first_three_attacks, named: the bonus on attacks 1 to 3, then the enchant is gone. Once it
    // is gone a living Rune Giant sends the Knight a new one on his next look (a free place refills, 14), and that one
    // pays attacks 4 to 6 again. So he is killed as in `death_scene`, 120 ticks before the 6th hit: the 3rd hit still
    // falls inside the 100 ticks the enchant outlives him (13), and nobody sends a second enchant.
    let (reference, enchant_tick) = death_scene(shipped(), None);
    let t6 = reference[5].0;
    assert!(t6 >= enchant_tick + 121, "the scene: he dies after the enchant lands ({t6} vs {enchant_tick})");
    let (hits, _) = death_scene(with(|c| c.enchant_bonus_attacks = EnchantBonusAttacks::FirstThree), Some(t6 - 120));
    let got: Vec<(u32, i32)> = hits.iter().map(|h| (h.1, h.2)).collect();
    assert_eq!(
        got,
        vec![(1, base + bonus), (2, base + bonus), (0, base + bonus), (0, base), (0, base), (0, base)],
        "first_three_attacks, as (enchant count after the hit, drop): the enchant ends with the 3rd attack"
    );
}

/// The bonus (third drop less first) an enchanted Knight at `knight_level` deals, from a Rune Giant at `giant_level`.
fn bonus_at(cfg: BattleConfig, knight_level: i32, giant_level: i32) -> i32 {
    let (mut s, knight, _) = enchanted(cfg, "Knight", Some(knight_level), Some(giant_level));
    let foe = foe_beside(&mut s, knight, "Knight", (1300, 0));
    let got = drops(&mut s, knight, foe, 400, 3);
    assert_eq!(got.iter().map(|h| h.1).collect::<Vec<_>>(), vec![1, 2, 3], "three attacks: {got:?}");
    got[2].2 - got[0].2
}

/// Plant: enchant_scaled_by_carrier.
#[test]
fn the_bonus_is_scaled_by_the_rune_giants_level() {
    assert_eq!(bonus_at(shipped(), 11, 9), 182, "measured on client 15.535.29: a level-9 Rune Giant, a carrier at 11");
    assert_eq!(bonus_at(shipped(), 12, 9), 182, "and a carrier at 12");
    let by_carrier = with(|c| c.enchant_bonus_level = EnchantBonusLevel::Carrier);
    assert_eq!((bonus_at(by_carrier.clone(), 11, 9), bonus_at(by_carrier, 12, 9)), (220, 241), "carrier_level, named");
    assert_eq!(bonus_at(with(|c| c.enchant_bonus_level = EnchantBonusLevel::Flat), 11, 9), 86, "flat, named");
}

/// Every (attack number, drop) of an enchanted Blue `attacker` on a Red Knight beside it, over `ticks` ticks.
fn attacker_drops(cfg: BattleConfig, attacker: &str, ticks: u32) -> Vec<(u32, i32)> {
    let (mut s, a, _) = enchanted(cfg, attacker, None, None);
    let foe = foe_beside(&mut s, a, "Knight", (1300, 0));
    drops(&mut s, a, foe, ticks, 64).into_iter().map(|(_, c, d)| (c, d)).collect()
}

/// Plants: enchant_multiplier_after_scaling, enchant_multiplier_percent, enchant_shots_uncounted.
#[test]
fn per_mille_of_the_level1_bonus_then_scaled() {
    // The Electro Wizard: two bolts an attack, both on the one Red Knight (AllTargetsHit); 117 a bolt, +110 on the bonus.
    let got = attacker_drops(shipped(), "ElectroWizard", 260);
    assert!(got.iter().any(|h| h.0 == 3), "an Electro Wizard attack 3 was seen: {got:?}");
    for &(k, d) in &got {
        let want = if k % 3 == 0 { 2 * 227 } else { 2 * 117 };
        assert_eq!(d, want, "Electro Wizard attack {k} (measured on client 15.535.29: 454 against 234): {got:?}");
    }
    // The Hunter: pellets of 84, +20 on the bonus volley. A tick's drop is whole pellets of the volley that fired them.
    let check = |cfg: BattleConfig, bonus_pellet: i32, what: &str| {
        let got = attacker_drops(cfg, "Hunter", 300);
        assert!(got.iter().any(|h| h.0 == 3) && got.iter().any(|h| h.0 == 1), "{what}: Hunter volleys 1 and 3 were seen: {got:?}");
        for &(k, d) in &got {
            let pellet = if k % 3 == 0 { bonus_pellet } else { 84 };
            assert!(d > 0 && d % pellet == 0, "{what}: volley {k} dropped {d}, not whole pellets of {pellet}: {got:?}");
        }
    };
    check(shipped(), 104, "per_mille_of_level1_then_scaled (measured on client 15.535.29)");
    check(with(|c| c.enchant_multiplier = EnchantMultiplier::OfScaled), 106, "per_mille_of_scaled, named");
}

/// Plant: splash_bonus_primary_only.
#[test]
fn a_splash_bonus_reaches_every_victim() {
    type Hits = Vec<(u32, u32, i32)>;
    let run = |cfg: BattleConfig| -> (Hits, Hits) {
        let (mut s, valk, _) = enchanted(cfg, "Valkyrie", None, None);
        let a = foe_beside(&mut s, valk, "Knight", (1300, 0));
        let b = foe_beside(&mut s, valk, "Knight", (-1300, 0));
        let mut ha = Vec::new();
        let mut hb = Vec::new();
        for _ in 0..200 {
            let (pa, pb) = (s.entity(a).map_or(0, |v| v.hp), s.entity(b).map_or(0, |v| v.hp));
            let k = s.tick_count();
            s.tick();
            let c = count_of(&s, valk);
            let (na, nb) = (s.entity(a).map_or(0, |v| v.hp), s.entity(b).map_or(0, |v| v.hp));
            if na < pa {
                ha.push((k, c, pa - na));
            }
            if nb < pb {
                hb.push((k, c, pb - nb));
            }
        }
        (ha, hb)
    };
    let (a, b) = run(shipped());
    let third = |h: &[(u32, u32, i32)]| h.iter().find(|x| x.1 == 3).map(|x| x.2);
    assert_eq!((third(&a[..]), third(&b[..])), (Some(486), Some(486)), "measured on client 15.535.29: 266 + 220 on both victims\n{a:?}\n{b:?}");
    assert!(a.iter().chain(&b).filter(|x| x.1 % 3 != 0).all(|x| x.2 == 266), "every other swing 266\n{a:?}\n{b:?}");
    let (a, b) = run(with(|c| c.enchant_splash_bonus = EnchantSplashBonus::PrimaryTargetOnly));
    let mut both = vec![third(&a[..]), third(&b[..])];
    both.sort_unstable();
    assert_eq!(both, vec![Some(266), Some(486)], "primary_target_only, named: one victim takes it\n{a:?}\n{b:?}");
}

/// Plants: enchant_shots_uncounted, enchant_bonus_ignored.
#[test]
fn a_shot_that_hits_nothing_still_counts() {
    let (mut s, musk, _) = enchanted(shipped(), "Musketeer", None, None);
    let musk_card = s.entity(musk).expect("she stands").card_idx;
    let first = foe_beside(&mut s, musk, "Knight", (3000, 0));
    // Shot 1 at the first Red Knight; it is killed while the shot flies.
    let k = run_until(&mut s, 120, |s| s.projectiles().iter().any(|p| p.firer_card == Some(musk_card) && p.target == first));
    assert!(k < 120, "the Musketeer fired at the first Red Knight");
    assert_eq!(count_of(&s, musk), 1, "that shot is her first attack since the enchant");
    let hp_before = s.entity(first).expect("it stands").hp;
    assert!(s.debug_set_hp(first, 0), "kill it in flight");
    s.tick();
    assert!(s.entity(first).is_none(), "the first Red Knight died with the shot in flight (hp {hp_before} before)");
    let second = foe_beside(&mut s, musk, "Knight", (3000, 0));
    let got = drops(&mut s, musk, second, 300, 5);
    assert_eq!(got.iter().map(|h| h.1).collect::<Vec<_>>(), vec![2, 3, 4, 5, 6], "shots 2 to 6 on the second: {got:?}");
    assert_eq!(got.iter().map(|h| h.2).collect::<Vec<_>>(), vec![217, 437, 217, 217, 437], "the bonus on shots 3 and 6 (measured on client 15.535.29)");
}

/// A Blue Knight played at (12500, 21000) near the Red princess tower on its right, the Rune Giant at (7000, 21000) on
/// the other lane (he walks at the other tower); both BIG. The first `n` drops of that tower as (attack number, drop).
fn tower_hits(cfg: BattleConfig, n: usize) -> Vec<(u32, i32)> {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "Knight", at((12500, 21000)), None).expect("play the Knight");
    s.spawn_unit(Team::Blue, "GiantBuffer", at((7000, 21000)), None).expect("play the Rune Giant");
    s.tick();
    let (knight, giant) = (the(&s, Team::Blue, "Knight"), the(&s, Team::Blue, "GiantBuffer"));
    assert!(s.debug_set_hp(knight, BIG) && s.debug_set_hp(giant, BIG));
    let tower = s
        .entities()
        .filter(|v| v.team == Team::Red && v.card == "PrincessTower")
        .min_by_key(|v| dist(v.pos, at((14500, 25500))))
        .map(|v| v.id)
        .expect("the Red princess tower on the right");
    let k = run_until(&mut s, 60, |s| s.entity(knight).is_some_and(|v| v.enchant.is_some()));
    assert!(k < 60 && s.entity(tower).is_some_and(|v| v.hp == v.max_hp), "the scene: the Knight enchanted before it reaches the tower");
    drops(&mut s, knight, tower, 300, n).into_iter().map(|(_, c, d)| (c, d)).collect()
}

/// A config whose card table gives the Rune Giant an AddedCrownTowerDamage of 40 (the tables give 86).
fn crown_column_40(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut db = cards();
    let k = db.index("GiantBuffer").expect("the Rune Giant loads") as usize;
    db.cards[k].enchant.as_mut().expect("his enchant").added_crown = 40;
    let mut cfg = BattleConfig::with_cards(db);
    f(&mut cfg.calib);
    cfg
}

/// Plant: crown_bonus_from_added_damage.
#[test]
fn the_crown_tower_bonus_is_its_own_column() {
    assert_eq!(tower_hits(shipped(), 3), vec![(1, 202), (2, 202), (3, 422)], "measured on client 15.535.29: a Knight's hits on a princess tower");
    let db = cards();
    let forty = scaled(&db, "GiantBuffer", 11, 40);
    assert_eq!(forty, 102, "40 at level 11");
    assert_eq!(tower_hits(crown_column_40(|_| {}), 3), vec![(1, 202), (2, 202), (3, 202 + forty)], "crown_column_no_percent: the tower takes the crown column");
    let (mut s, knight, _) = enchanted(crown_column_40(|_| {}), "Knight", None, None);
    let foe = foe_beside(&mut s, knight, "Knight", (1300, 0));
    let troop: Vec<i32> = drops(&mut s, knight, foe, 300, 3).into_iter().map(|h| h.2).collect();
    assert_eq!(troop, vec![202, 202, 422], "a troop still takes AddedDamage");
    let with_pct = crown_column_40(|c| c.enchant_crown_bonus = EnchantCrownBonus::AddedWithAttackerPercent);
    assert_eq!(tower_hits(with_pct, 3), vec![(1, 202), (2, 202), (3, 422)], "added_damage_with_attacker_percent, named: the AddedDamage bonus at the Knight's 100 %");
}

/// The Knight scene of `the_bonus_lands_on_every_third_attack`, the Rune Giant killed before tick `kill_at` if given:
/// every (tick, attack number, drop) of the first six hits.
fn death_scene(cfg: BattleConfig, kill_at: Option<u32>) -> (Vec<(u32, u32, i32)>, u32) {
    let (mut s, knight, giant) = enchanted(cfg, "Knight", None, None);
    let enchant_tick = s.tick_count() - 1;
    let foe = foe_beside(&mut s, knight, "Knight", (1300, 0));
    let mut out = Vec::new();
    for _ in 0..400 {
        if kill_at == Some(s.tick_count()) {
            assert!(s.debug_set_hp(giant, 0), "kill the Rune Giant");
        }
        let before = s.entity(foe).expect("the foe stands").hp;
        let k = s.tick_count();
        s.tick();
        let after = s.entity(foe).expect("the foe stands").hp;
        if after < before {
            let c = s.entity(knight).and_then(|v| v.enchant).map_or(0, |e| e.count);
            out.push((k, c, before - after));
            if out.len() >= 6 {
                break;
            }
        }
    }
    (out, enchant_tick)
}

/// Plants: enchant_outlives_instigator, enchant_ends_with_instigator.
#[test]
fn the_enchant_outlives_him_by_100_ticks() {
    let (reference, enchant_tick) = death_scene(shipped(), None);
    assert_eq!(reference.len(), 6, "six hits: {reference:?}");
    let t6 = reference[5].0;
    assert!(t6 >= enchant_tick + 121, "the scene: the 6th hit comes more than 120 ticks after the enchant ({t6} vs {enchant_tick})");
    let (base, bonus) = (reference[0].2, reference[2].2 - reference[0].2);
    assert_eq!(bonus, 220);
    // Killed 80 ticks before the 6th hit: it pays.
    let (a, _) = death_scene(shipped(), Some(t6 - 80));
    assert_eq!(a[5].0, t6, "the same 6th hit tick: {a:?}");
    assert_eq!(a[5].2, base + bonus, "80 ticks after his death the bonus still pays: {a:?}");
    // Killed 120 ticks before it: the 3rd hit (before the end) pays, the 6th does not.
    let (b, _) = death_scene(shipped(), Some(t6 - 120));
    assert_eq!(b[5].0, t6, "the same 6th hit tick: {b:?}");
    assert!(b[2].0 <= t6 - 120 + 100, "the scene: the 3rd hit lands inside the 100 ticks");
    assert_eq!((b[2].2, b[5].2), (base + bonus, base), "120 ticks after his death the enchant is gone: {b:?}");
    // enchant.INSTIGATOR_DEATH = ends_at_death, named: neither pays once he is dead.
    let (c, _) = death_scene(with(|c| c.enchant_instigator_death = EnchantInstigatorDeath::EndsAtDeath), Some(t6 - 120));
    assert_eq!((c[2].2, c[5].2), (base, base), "ends_at_death: {c:?}");
}

// ---------------------------------------------------------------------------
// 14, 15. the places

/// The re-pick scene measured on client 15.535.29: a Knight played at (7500, 9500), the Rune Giant at (11500, 12500) 2 ticks later, two Archers at
/// (14500, 11500) on F + 38, after the first projectile has landed. Every launch as (tick - F, target is the Knight).
fn repick_scene(cfg: BattleConfig) -> Vec<(u32, bool)> {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Blue, "Knight", at((7500, 9500)), None).expect("play the Knight");
    s.tick();
    s.tick();
    s.spawn_unit(Team::Blue, "GiantBuffer", at((11500, 12500)), None).expect("play the Rune Giant");
    let f = s.tick_count();
    let mut got = launches(&mut s, 38);
    let knight = the(&s, Team::Blue, "Knight");
    assert!(s.entity(knight).is_some_and(|v| v.enchant.is_some()), "the scene: the Knight enchanted before the Archers");
    s.spawn_unit(Team::Blue, "Archer", at((14500, 11500)), None).expect("play the Archers");
    got.extend(launches(&mut s, 160));
    got.into_iter().map(|(k, t)| (k - f, t == knight)).collect()
}

/// Plants: enchant_fresh_pick_of_two, enchant_cooldown_from_pick.
#[test]
fn a_free_place_refills_after_the_cooldown() {
    let got = repick_scene(shipped());
    assert_eq!(got, vec![(27, true), (95, false)], "one projectile to the Knight, one to an Archer 68 ticks later (measured on client 15.535.29)");
    let fresh = repick_scene(with(|c| c.enchant_slots = EnchantSlots::FreshPick));
    assert_eq!(fresh, vec![(27, true), (95, false), (95, false)], "fresh_pick_of_max_targets, named: both Archers");
    let from_pick = repick_scene(with(|c| c.enchant_cooldown_origin = EnchantCooldownOrigin::FromPick));
    assert_eq!(from_pick, vec![(27, true), (88, false)], "from_pick, named: 61 ticks after the first");
}

/// Plant: enchant_repicks_enchanted.
#[test]
fn an_enchanted_friend_is_not_sent_another() {
    let (mut s, _, _, giant) = lag_scene(shipped());
    assert!(s.debug_set_hp(giant, BIG));
    let knight = the(&s, Team::Blue, "Knight");
    assert!(s.debug_set_hp(knight, BIG));
    let got = launches(&mut s, 400);
    assert_eq!(got.len(), 1, "one projectile in 400 ticks with one friend: {got:?}");
}

// ---------------------------------------------------------------------------
// 16, 17. the stun and the walk

/// The lag scene with a Red Zap cast on the Rune Giant to land on F + `zap_at`: (the first launch - F, the first
/// look tick - F on which he is not held, from F + 20 on). A unit is held on a tick when its stun is still running at
/// the end of the tick before.
fn stun_scene(cfg: BattleConfig, zap_at: u32) -> (u32, u32) {
    let (mut s, f, _, giant) = lag_scene(cfg);
    let mut first_unheld = None;
    let mut launch = None;
    let mut prev_stun = s.entity(giant).expect("he stands").stun_ms;
    for _ in 0..80 {
        let k = s.tick_count();
        if k == f + zap_at {
            let p = s.entity(giant).expect("he stands").pos;
            s.spawn_unit(Team::Red, "Zap", p, None).expect("cast the Zap");
        }
        if first_unheld.is_none() && k >= f + 20 && prev_stun == 0 {
            first_unheld = Some(k - f);
        }
        s.tick();
        prev_stun = s.entity(giant).expect("he stands").stun_ms;
        if launch.is_none() && !bolts(&s).is_empty() {
            launch = Some(k - f);
        }
        if launch.is_some() && first_unheld.is_some() {
            break;
        }
    }
    (launch.expect("a launch"), first_unheld.expect("an unheld look"))
}

/// Plants: enchant_stun_pauses_timer, enchant_stun_ignored, enchant_launch_at_pick.
#[test]
fn a_stun_at_the_pick_waits_for_its_end() {
    // A stun that ends inside his deploy: the look on F + 20 and the projectile on F + 27, as measured.
    let (launch, unheld) = stun_scene(shipped(), 3);
    assert_eq!(unheld, 20, "the scene: the stun ends before the look is due");
    assert_eq!(launch, 27, "a stun inside the deploy changes nothing (measured on client 15.535.29)");
    // A stun on the due look: the look waits for the first tick he is not held, and the projectile leaves 7 later.
    let (launch, unheld) = stun_scene(shipped(), 19);
    assert!(unheld > 20, "the scene: the stun covers the look on F + 20 (first unheld F + {unheld})");
    assert_eq!(launch, unheld + 7, "pick_waits_for_stun_end (the client's came 2 ticks earlier: the key's named gap)");
    // The named arms.
    let (launch, _) = stun_scene(with(|c| c.enchant_stun_at_pick = EnchantStunAtPick::Ignored), 19);
    assert_eq!(launch, 27, "ignored: the look runs held");
    let (launch, unheld) = stun_scene(with(|c| c.enchant_stun_at_pick = EnchantStunAtPick::TimerPauses), 3);
    assert_eq!(unheld, 20);
    assert!(launch > 27, "timer_pauses: a stun inside the deploy stops his clock (launch F + {launch})");
}

/// Plant: enchant_pause_on_launch.
#[test]
fn he_does_not_stop_walking_when_he_enchants() {
    let (mut s, f, _, giant) = lag_scene(shipped());
    let mut steps = Vec::new();
    let mut prev = s.entity(giant).expect("he stands").pos;
    for _ in f + 1..f + 40 {
        s.tick();
        let p = s.entity(giant).expect("he stands").pos;
        steps.push((s.tick_count() - 1 - f, dist(prev, p), s.entity(giant).map(|v| v.attack_phase)));
        prev = p;
    }
    assert!(steps.iter().any(|x| x.0 == 27), "the launch tick was run");
    // He walks from F + 20 (his deploy ends on F + 19); every step from F + 21 on, the launch included, is a walk.
    for &(k, d, phase) in steps.iter().filter(|x| x.0 >= 21) {
        assert_eq!(phase, Some(AttackPhase::Idle), "the scene: he is walking, not attacking, on F + {k}");
        assert!(d >= 50, "F + {k}: a step of {d} around the launch on F + 27 (measured on client 15.535.29: 59 throughout)\n{steps:?}");
    }
}

// ---------------------------------------------------------------------------
// 18. state

/// Run the battle and its loaded save side by side for `m` ticks; every hash equal.
fn resumes(mut s: BattleState, m: u32, what: &str) {
    let blob = s.save();
    let mut b = BattleState::load(&blob).unwrap_or_else(|e| panic!("{what}: the save does not load: {e}"));
    assert_eq!(b.state_hash(), s.state_hash(), "{what}: the loaded state");
    for k in 0..m {
        s.tick();
        b.tick();
        assert_eq!(b.state_hash(), s.state_hash(), "{what}: {k} ticks after the load");
    }
}

/// Plants: save_drops_enchant, hash_skips_enchant.
#[test]
fn a_rune_giant_battle_survives_save_and_load() {
    // A pending launch: the lag scene on F + 22 (picked on F + 20, the projectile due on F + 27).
    let (mut s, f, knight, giant) = lag_scene(shipped());
    while s.tick_count() < f + 23 {
        s.tick();
    }
    let g = s.entity(giant).expect("he stands");
    assert_eq!(g.enchant_state, 3, "the scene: a launch pending");
    assert_eq!(s.enchant_picks(giant), &[knight][..]);
    let gi = giant.index as usize;
    assert!(edit_is_hashed(&s, |v| v["ents"]["enchant_ms"][gi] = serde_json::json!(g.enchant_ms + 50)), "his launch clock is state");
    assert!(edit_is_hashed(&s, |v| v["ents"]["enchant_picks"][gi] = serde_json::json!([])), "his picks are state");
    resumes(s, 200, "a pending launch");
    // A projectile in flight.
    let (mut s, _, _, _) = lag_scene(shipped());
    run_until(&mut s, 40, |s| !bolts(s).is_empty());
    let j = s.projectiles().iter().position(|p| p.enchant.is_some()).expect("a projectile in flight");
    assert!(
        edit_is_hashed(&s, |v| {
            let l = v["projectiles"][j]["enchant"]["level"].as_i64().expect("a level");
            v["projectiles"][j]["enchant"]["level"] = serde_json::json!(l + 1);
        }),
        "the projectile's payload is state"
    );
    resumes(s, 200, "a projectile in flight");
    // An enchant mid-count.
    let (mut s, knight, _) = enchanted(shipped(), "Knight", None, None);
    let foe = foe_beside(&mut s, knight, "Knight", (1300, 0));
    let got = drops(&mut s, knight, foe, 200, 1);
    assert_eq!(got.first().map(|h| h.1), Some(1), "the scene: one attack counted");
    let ki = knight.index as usize;
    assert!(edit_is_hashed(&s, |v| v["ents"]["enchant"][ki]["count"] = serde_json::json!(2)), "the enchant's count is state");
    resumes(s, 200, "an enchant mid-count");
}

// ---------------------------------------------------------------------------
// 19. the tagged rows

/// Plant: enchant_picks_tagged_unit.
#[test]
fn a_tagged_unit_is_never_picked() {
    // The tables tag one loaded row, the PhoenixEgg, which no scene puts in reach of a look: tag the Knight instead.
    let mut db = cards();
    let (g, k) = (db.index("GiantBuffer").expect("the Rune Giant loads"), db.index("Knight").expect("the Knight loads"));
    let e = db.cards[g as usize].enchant.as_mut().expect("his enchant");
    e.excluded.push(k);
    e.excluded.sort_unstable();
    let mut cfg = BattleConfig::with_cards(db);
    cfg.calib = shipped().calib;
    // The Knight 3000 behind him, the Archers 6000: the Archers, never the nearer Knight.
    let (s, picks) = first_picks(cfg, &[("Knight", (14500, 11500)), ("Archer", (14500, 8500))]);
    let knight = the(&s, Team::Blue, "Knight");
    assert!(!picks.is_empty() && !picks.contains(&knight), "a tagged Knight was picked: {picks:?}");
}
