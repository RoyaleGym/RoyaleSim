//! A spell that summons (the Rage's bottle, the Heal Spirit) and the mechanisms it brought: the spell object chain
//! (card.rs `SpellShape::Fuse` and a pulsing area's child; spell.rs `objects_for`, `shape_at`), the own-side area
//! filter (spell.rs `eligible`), and a troop projectile's area (card.rs `CardDef::projectile_area`; combat.rs
//! `step_projectiles`). The area's buff-time cap and the pulse's level scaling are status.AREA_BUFF_SOURCE_BINDING's and
//! status.BUFF_PULSE_AMOUNT's; this file measures them on the Rage and the Heal Spirit.
//!
//! THE LAW, measured on client 16.402 unless named:
//!   - a Rage cast on tick C (the tick whose Spawn phase casts) releases its area on C + 9 (the bottle's DeployTime
//!     counted like a unit's; spells.SUMMON_FUSE_START), the area first acts on C + 10 and a unit inside takes its
//!     first raged step into C + 11 (5 of 5 casts, 9 units);
//!   - the Rage's damage (RageDamage, the area's child) lands one tick after the first buff, on C + 11
//!     (spells.CHILD_AREA_BIRTH; 5 of 5);
//!   - the Rage buffs the caster's side only (OnlyOwnTroops);
//!   - a unit that stays inside to the end takes its last raged step 105 ticks after the cast, where the full BuffTime
//!     gives 114 (CapBuffTimeToAreaEffectTime, status.AREA_BUFF_SOURCE_BINDING: the area's last application, on
//!     C + 94, lasts its life left after that tick, 250 ms, plus one HitSpeed, 300 ms). The 2026-09-28 round 9 reading:
//!     8 units over 4 casts (three in 20260919-182539, one in 20260918-134739 read in both seats), every one on C + 105;
//!   - a unit that walks out of the Rage after its application on C + 82 (900 ms of the area left) takes that
//!     application's full BuffTime, 1,000 ms (the cap binds only below it), and its last raged step lands a tick later
//!     than under the rule before the round 9 fix (950 ms): three Skeletons of 20260919-182539's cast 177 end on C + 104,
//!     103 and 102 in the client and the engine, and one tick earlier each under that rule;
//!   - the Heal Spirit card deploys its unit like a one-unit troop, and the spirit's shot leaves a one-shot heal
//!     area on its own side; each heal pulse is +100 at level 11 (status.BUFF_PULSE_AMOUNT =
//!     scaled_per_second_times_frequency; 11 of 11).
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads Rage as a Fuse over an own-side pulsing area with a one-shot child, and Heal as a Summon of
//!      the Heal Spirit, whose projectile carries its heal area;
//!   2. the Rage's first raged step is the step into C + 11;
//!   3. the Rage's damage lands on C + 11, not C + 10;
//!   4. an enemy inside the Rage takes no buff;
//!   5. a Blue Knight that walks into the Rage and is still inside when the area ends takes its last raged step on
//!      C + 105 under status.AREA_BUFF_SOURCE_BINDING = client_source_bound, and on C + 114 (the full BuffTime) under
//!      not_read. The rule the engine ran before the round 9 fix, the life left this tick included plus one tick, gave
//!      C + 101, the step the old assertion (bound before unbound) let through;
//!   6. the Heal Spirit is on the board after the cast tick and first moves DeployTime later;
//!   7. an own damaged Knight next to the spirit's target is healed in pulses of exactly the amount
//!      status.BUFF_PULSE_AMOUNT = scaled_per_second_times_frequency gives (+100 at level 11), and the enemy is not;
//!   8. a spell object's chain depth is state: a save edited only in it fails the load's hash self-check;
//!   9. a Blue Knight that walks through a Rage and out of it after the application on C + 82 takes that
//!      application at 1,000 ms (950 under the rule before the round 9 fix), no later one, and its last raged step on
//!      C + 102 (C + 101 under that rule); not_read gives the same, the cap not binding at 900 ms left.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test spell_summon`):
//!   * `fuse_counts_from_next_tick` -- the bottle releases one tick late: (2) goes red.
//!   * `child_area_with_parent` -- the child is born with its parent: (3) goes red.
//!   * `own_area_hits_both_sides` -- OnlyOwnTroops read as no team filter: (4) goes red.
//!   * `area_cap_unread` (spell.rs's cap plant) -- CapBuffTimeToAreaEffectTime not read: (5) goes red.
//!   * `area_cap_one_tick` (spell.rs) -- the cap is the life left plus one tick, the Earthquake-only fit: (5) and (9)
//!     go red.
//!   * `summon_released_late` -- the summoned unit is created at the end of the tick: (6) goes red.
//!   * `projectile_area_dropped` -- the shot's area is dropped: (7) goes red.
//!   * `pulse_share_scaled` (status.rs's pulse plant) -- the share first, then the level: (7) goes red.
//!   * `projectile_area_unread` -- the loader drops the projectile's area: (1) goes red.
//!   * `hash_skips_spell_depth` -- the depth is not hashed: (8) goes red.
//!   * `own_full_stop_area_loads` -- an own-side full-stop area loads: tests/loadable_census.rs goes red on the 2018
//!     table.
//!   * `summon_unit_unreported`: see tests/test_export_ids.py (not a Rust plant).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{SpellDef, SpellPlacement, SpellShape};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AreaBuffSourceBinding, BattleConfig, BattleState, PulseAmount};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn step(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

fn rage_buff(s: &BattleState) -> u16 {
    let SpellShape::Fuse { then, .. } = &card_stat(s, "Rage").spell.as_ref().expect("Rage is a spell").shape else { panic!("Rage is not a Fuse") };
    let SpellShape::PulsingAreaEffect { hit, .. } = then.as_ref() else { panic!("the bottle releases no pulsing area") };
    hit.buff.expect("the Rage area carries a buff").buff
}

fn has_buff(s: &BattleState, id: EntityId, buff: u16) -> bool {
    s.entity(id).is_some_and(|v| v.buffs.iter().any(|b| b.id == buff + 1 && b.ms > 0))
}

/// The time left on `id`'s Rage buff, ms (0 without one).
fn buff_ms(s: &BattleState, id: EntityId, buff: u16) -> i32 {
    s.entity(id).map_or(0, |v| v.buffs.iter().filter(|b| b.id == buff + 1).map(|b| b.ms).max().unwrap_or(0))
}

/// Plant: projectile_area_unread.
#[test]
fn the_loader_reads_rage_as_a_fuse_and_heal_as_a_summon() {
    let s = BattleState::new(0, config());
    let rage = card_stat(&s, "Rage").spell.as_ref().expect("Rage loads as a spell");
    assert_eq!(rage.placement, SpellPlacement::Anywhere);
    let SpellShape::Fuse { fuse_ms, then } = &rage.shape else { panic!("Rage: {:?}", rage.shape) };
    assert_eq!(*fuse_ms, 500, "RageBottle's DeployTime");
    let SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms, child } = then.as_ref() else { panic!("{then:?}") };
    assert_eq!((*life_ms, *hit_speed_ms, hit.caps_buff_time), (4500, 300, true));
    assert!(hit.only_own_troops && !hit.only_enemies, "the Rage area is own-side");
    let Some(child) = child else { panic!("the Rage area makes no child") };
    let SpellShape::AreaEffect { hit: dmg } = child.as_ref() else { panic!("{child:?}") };
    assert!(dmg.only_enemies && dmg.damage == 70, "RageDamage: {dmg:?}");
    let heal = card_stat(&s, "Heal").spell.as_ref().expect("Heal loads as a spell");
    assert_eq!(heal.placement, SpellPlacement::TroopTerritory { on_buildings: false });
    let SpellShape::Summon { unit, count } = heal.shape else { panic!("Heal: {:?}", heal.shape) };
    assert_eq!((s.cards().get(unit).name.as_str(), count), ("HealSpirit", 1));
    let area = s.cards().get(unit).projectile_area.as_ref().expect("the Heal Spirit's shot leaves its heal area");
    let SpellDef { shape: SpellShape::AreaEffect { hit }, .. } = area else { panic!("{area:?}") };
    assert!(hit.only_own_troops && hit.ignore_buildings && hit.buff.is_some(), "the heal area: {hit:?}");
}

/// A Blue Knight walking up the left lane, and a Rage cast on it once it walks; per tick after the cast (k = 0 is
/// the cast tick C): the Knight's step. The Knight starts 2.5 tiles past the Blue left princess tower's centre. It
/// must not start inside the tower: a Knight at (3500, 6000) is pushed out of it for nine ticks, with steps up to 199.
fn raged_walk() -> Vec<i64> {
    let mut s = BattleState::new(0, config());
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at((3500, 9000)), None).expect("spawn");
    for _ in 0..3 {
        s.tick();
    }
    let p = s.entity(k).unwrap().pos;
    s.spawn_unit(Team::Blue, "Rage", p, None).expect("cast Rage");
    let mut steps = Vec::new();
    for _ in 0..20 {
        let a = s.entity(k).unwrap().pos;
        s.tick();
        steps.push(step(a, s.entity(k).unwrap().pos));
    }
    steps
}

/// Plant: fuse_counts_from_next_tick.
#[test]
fn the_rages_first_raged_step_is_the_eleventh_tick_after_the_cast() {
    let steps = raged_walk();
    assert!(steps[..11].iter().all(|&x| (55..=61).contains(&x)), "unraged steps through C + 10: {steps:?}");
    assert!(steps[11..].iter().all(|&x| (74..=79).contains(&x)), "raged steps from C + 11: {steps:?}");
}

/// Plant: child_area_with_parent.
#[test]
fn the_rages_damage_lands_on_the_eleventh_tick_after_the_cast() {
    let mut s = BattleState::new(0, config());
    let e = s.scenario_spawn_now(Team::Red, "Knight", at((3500, 9000)), None).expect("spawn");
    s.tick();
    s.spawn_unit(Team::Blue, "Rage", s.entity(e).unwrap().pos, None).expect("cast Rage");
    let mut losses = Vec::new();
    for k in 0..14u32 {
        let before = s.entity(e).unwrap().hp;
        s.tick();
        let lost = before - s.entity(e).unwrap().hp;
        if lost > 0 {
            losses.push((k, lost));
        }
    }
    let want = s.cards().scaled(s.cards().index("Rage").unwrap(), s.config().card_level[1], 70).unwrap();
    assert_eq!(losses, vec![(11, want)], "RageDamage on C + 11 only");
}

/// Plant: own_area_hits_both_sides.
#[test]
fn the_rage_buffs_the_casters_side_only() {
    let mut s = BattleState::new(0, config());
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", at((3500, 9000)), None), (Team::Red, "Knight", at((4500, 9000)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    s.tick();
    s.spawn_unit(Team::Blue, "Rage", at((4000, 9000)), None).expect("cast Rage");
    let buff = rage_buff(&s);
    let (mut own, mut enemy) = (false, false);
    for _ in 0..20 {
        s.tick();
        own |= has_buff(&s, ids[0], buff);
        enemy |= has_buff(&s, ids[1], buff);
    }
    assert!(own, "the scene drifted: the own Knight was never raged");
    assert!(!enemy, "the enemy Knight took the Rage buff");
}

/// A Blue Knight walking up the left lane into a Rage cast 5,500 ahead of it, and per tick after the cast (k = 0 is
/// the cast tick C) its step: it enters the area about 35 ticks in and is still inside, raged, when the area ends.
fn late_raged_walk(binding: AreaBuffSourceBinding) -> Vec<i64> {
    let mut cfg: BattleConfig = config();
    cfg.calib.area_buff_source_binding = binding;
    let mut s = BattleState::new(0, cfg);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at((3500, 9000)), None).expect("spawn");
    for _ in 0..3 {
        s.tick();
    }
    let p = s.entity(k).unwrap().pos;
    s.spawn_unit(Team::Blue, "Rage", Vec2::new(p.x, p.y + 5500 * K), None).expect("cast Rage");
    let mut steps = Vec::new();
    for _ in 0..130 {
        let a = s.entity(k).unwrap().pos;
        s.tick();
        steps.push(step(a, s.entity(k).unwrap().pos));
    }
    steps
}

/// The last k whose step is raged (74..=79; a Knight walks 55..=61), after checking that every later step is an
/// unraged walk.
fn last_raged_step(binding: AreaBuffSourceBinding) -> usize {
    let steps = late_raged_walk(binding);
    let last = steps.iter().rposition(|x| (74..=79).contains(x)).unwrap_or_else(|| panic!("the scene drifted: the Knight was never raged: {steps:?}"));
    assert!(steps[..30].iter().all(|x| (55..=61).contains(x)), "the scene drifted: the Knight was raged before it reached the area: {steps:?}");
    assert!(steps[last + 1..].iter().all(|x| (55..=61).contains(x)), "the scene drifted: the Knight's steps after its last raged one: {steps:?}");
    last
}

/// Plants: area_cap_unread, area_cap_one_tick.
#[test]
fn the_rage_buff_ends_sooner_when_bound_to_its_area() {
    let bound = last_raged_step(AreaBuffSourceBinding::ClientSourceBound);
    let unbound = last_raged_step(AreaBuffSourceBinding::NotRead);
    // The area's last application is on C + 94 with 300 ms of life left this tick included: bound, it lasts 250 + 300
    // ms and the last raged step is C + 105; unbound, its 1,000 ms BuffTime gives C + 114.
    assert_eq!(bound, 105, "client_source_bound: the last raged step (not_read {unbound})");
    assert_eq!(unbound, 114, "not_read: the last raged step, the full BuffTime");
}

/// A Blue Knight walking up the left lane through a Rage cast 3,000 ahead of it, which it walks out of after the
/// area's application on C + 82: per tick after the cast (k = 0 is C), its step and the time left on its Rage buff.
fn walk_out(binding: AreaBuffSourceBinding) -> (Vec<i64>, Vec<i32>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.area_buff_source_binding = binding;
    let mut s = BattleState::new(0, cfg);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at((3500, 9000)), None).expect("spawn");
    for _ in 0..3 {
        s.tick();
    }
    let p = s.entity(k).unwrap().pos;
    s.spawn_unit(Team::Blue, "Rage", Vec2::new(p.x, p.y + 3000 * K), None).expect("cast Rage");
    let buff = rage_buff(&s);
    let (mut steps, mut left) = (Vec::new(), Vec::new());
    for _ in 0..130 {
        let a = s.entity(k).unwrap().pos;
        s.tick();
        steps.push(step(a, s.entity(k).unwrap().pos));
        left.push(buff_ms(&s, k, buff));
    }
    (steps, left)
}

/// (the k of the Knight's last Rage application, the buff time it left, the k of its last raged step). An application
/// is a tick on which the time left rises. Checks that every step after the last raged one is an unraged walk.
fn walk_out_ends(binding: AreaBuffSourceBinding) -> (usize, i32, usize) {
    let (steps, left) = walk_out(binding);
    let app = (1..left.len()).rfind(|&i| left[i] > left[i - 1]).unwrap_or_else(|| panic!("the scene drifted: the Knight was never raged: {left:?}"));
    let last = steps.iter().rposition(|x| (74..=79).contains(x)).unwrap_or_else(|| panic!("the scene drifted: no raged step: {steps:?}"));
    assert!(steps[last + 1..].iter().all(|x| (55..=61).contains(x)), "the scene drifted: the Knight's steps after its last raged one: {steps:?}");
    (app, left[app], last)
}

/// Plant: area_cap_one_tick. The area's application on C + 82 has 900 ms of life left, this tick included: capped at
/// 900 - 50 + 300 = 1,150 it keeps its BuffTime, 1,000 ms (the earlier rule's 900 + 50 = 950 bound it). The Knight is
/// out of the area by C + 88, so that application is its last, and its last raged step is C + 102 (C + 101 under the
/// earlier rule). not_read gives the same: the cap does not bind here.
#[test]
fn a_unit_that_walks_out_after_the_cast_plus_82_keeps_the_full_buff_time() {
    assert_eq!(walk_out_ends(AreaBuffSourceBinding::ClientSourceBound), (82, 1000, 102), "client_source_bound: (last application, its ms, last raged step)");
    assert_eq!(walk_out_ends(AreaBuffSourceBinding::NotRead), (82, 1000, 102), "not_read: (last application, its ms, last raged step)");
}

/// Plant: summon_released_late.
#[test]
fn the_heal_spirit_deploys_like_a_one_unit_troop() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "Heal", at((9000, 8000)), None).expect("cast Heal");
    s.tick();
    let spirit = s.entities().find(|v| v.card == "HealSpirit").map(|v| v.id).expect("the Heal Spirit is on the board after the cast tick");
    let deploy = card_stat(&s, "HealSpirit").deploy_time_ms as u32 / s.config().calib.tick_ms as u32;
    let mut first_move = None;
    for k in 1..(deploy + 10) {
        let a = s.entity(spirit).map(|v| v.pos);
        s.tick();
        if a.is_some() && a != s.entity(spirit).map(|v| v.pos) {
            first_move = Some(k);
            break;
        }
    }
    assert_eq!(first_move, Some(deploy), "the spirit first moves DeployTime after the cast tick");
}

/// Plants: projectile_area_dropped, pulse_share_scaled.
#[test]
fn the_heal_spirits_shot_heals_its_own_side_in_scaled_rate_pulses() {
    let mut cfg: BattleConfig = config();
    cfg.calib.buff_pulse_amount = PulseAmount::ScaledPerSecondTimesFrequency;
    let mut s = BattleState::new(0, cfg);
    // All three on Blue's bank: the Knight fights the Giant, and the spirit's shot lands on the Giant beside it.
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", at((9000, 12000)), Some(300)), (Team::Red, "Giant", at((9000, 13300)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    s.spawn_unit(Team::Blue, "Heal", at((9000, 10500)), None).expect("cast Heal");
    let spirit_idx = s.cards().index("HealSpirit").expect("the Heal Spirit loads");
    let heal = s.cards().get(spirit_idx).projectile_area.as_ref().expect("the heal area");
    let SpellShape::AreaEffect { hit } = &heal.shape else { panic!() };
    let b = hit.buff.expect("the heal buff");
    let def = s.cards().buffs[b.buff as usize];
    let level = s.config().card_level[0];
    let want = -def.pulse_amount(PulseAmount::ScaledPerSecondTimesFrequency, |m| s.cards().scaled(spirit_idx, level, m)).unwrap();
    assert_eq!(level == 11, want == 100, "the measured +100 at level 11");
    let (mut gains, mut enemy_gains, mut giant_hurt) = (Vec::new(), 0, None);
    let (mut kh, mut gh) = (s.entity(ids[0]).unwrap().hp, s.entity(ids[1]).unwrap().hp);
    for _ in 0..150 {
        s.tick();
        let (Some(k), Some(g)) = (s.entity(ids[0]), s.entity(ids[1])) else { break };
        if k.hp > kh {
            gains.push(k.hp - kh);
            giant_hurt.get_or_insert(g.hp < g.max_hp);
        }
        if g.hp > gh {
            enemy_gains += 1;
        }
        (kh, gh) = (k.hp, g.hp);
    }
    assert!(!gains.is_empty(), "the own Knight was never healed");
    assert_eq!(giant_hurt, Some(true), "the scene drifted: the Giant had full hp when the heal landed, so a heal on it would not show");
    assert!(gains.iter().all(|&g| g == want), "each heal is {want}: {gains:?}");
    assert_eq!(enemy_gains, 0, "the enemy Giant was healed");
}

/// Plant: hash_skips_spell_depth.
#[test]
fn a_spell_objects_chain_depth_is_state() {
    let mut s = BattleState::new(0, config());
    s.tick();
    s.spawn_unit(Team::Blue, "Rage", at((4500, 9500)), None).expect("cast Rage");
    // run until the Rage area (depth 1) stands
    for _ in 0..12 {
        s.tick();
    }
    assert!(s.spells().iter().any(|sp| sp.depth == 1), "the scene drifted: no depth-1 object stands");
    let hashed = edit_is_hashed(&s, |v| {
        let spells = v["spells"].as_array_mut().expect("the snapshot lists its spells");
        let k = spells.iter().position(|sp| sp["depth"] == 1).expect("the depth-1 object is saved");
        spells[k]["depth"] = serde_json::Value::from(2);
    });
    assert!(hashed, "a save edited only in a spell's depth loads under the old hash: the depth is not hashed");
}
