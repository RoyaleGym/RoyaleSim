//! THE HERO ELECTRO WIZARD'S SURGE (card.rs `AbilityEffect::Surge`, `SurgeDef`; state.rs `SurgeRun`, `surge_pass`;
//! combat.rs `fire`), on client 16.635's table (cards.CARD_TABLE client160635007_20261103) at level 11.
//!
//! THE MEASUREMENTS (client 16.402, Live, 6 presses; P the cast's first frame): the cast holds him P .. P + 8; from the
//! trigger (P + 4) his attack clock runs at 360 % (a pair of bolts every 10 ticks, 50 Damage each on his ladder, a lone
//! target taking both), each bolt on a crown tower halved while the buff is on; his entry 0 again on P + 65, the swing in
//! flight dropped; no attack P + 65 .. P + 74 while he walks; a fresh normal swing from P + 75. The 16.402 entry also holds
//! each surge swing 500 ms first (AttackStartDelay); 16.635's is 0, which is the table this file plays, so its first pair
//! comes sooner than 16.402's P + 22 (not asserted to the tick: a prediction until a 16.635 press is recorded).
//!
//! Each scene here asserts what the surge does relative to what the hero does without it: the pair's damage against the
//! row's, its stun against the row's, the 10-tick cadence, the recovery's hold, the root, the walk, the tower's half.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_electro_wizard`):
//!   - surge_effect_refused -> every test red (the form does not load);
//!   - surge_never, surge_damage_unread -> `a_press_bolts_a_pair_every_10_ticks_at_its_damage` red;
//!   - surge_stun_unread -> `its_bolts_stun_100_ms_longer_than_the_rows` red;
//!   - surge_never_ends, surge_recovery_unheld -> `the_recovery_holds_his_attack_10_ticks_from_p_plus_65` red (its attack clock);
//!   - surge_swing_kept -> `the_swing_in_flight_is_dropped_at_the_end` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{AbilityEffect, CardDb, SurgeDef, FORM_HERO};
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CardTable};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["ElectroWizard", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];
const HERO: &str = "ElectroWizard_hero";
const LEVEL: i32 = 11;
/// On blue's side, out of every red tower's reach.
const AT: (i32, i32) = (9000, 9000);

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn db() -> CardDb {
    CardDb::load_table(CardTable::Client160635007Nov03).expect("the 16.635 table")
}

fn surge(db: &CardDb) -> SurgeDef {
    let idx = db.index(HERO).expect("the hero form loads on 16.635");
    match db.get(idx).ability.as_ref().map(|a| &a.effect) {
        Some(AbilityEffect::Surge(sg)) => *sg,
        other => panic!("the hero's button is not a surge: {other:?}"),
    }
}

/// A battle on 16.635's table, blue's deck holding the hero form, both sides at LEVEL, blue's princess towers down so no
/// crown tower of blue's reaches a red unit in the scene.
fn battle() -> BattleState {
    let mut cfg = BattleConfig::with_cards(db());
    cfg.calib.card_table = CardTable::Client160635007Nov03;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [LEVEL, LEVEL];
    cfg.tower_level = [LEVEL, LEVEL];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    // Past the battle's opening lockout, so the button can be pressed once the hero stands (as tests/hero_wizard.rs).
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    s
}

/// One frame after a tick: the damage the victim took that tick (its hp topped up before each tick), its stun, and the
/// hero's attack clock, point and buff slots.
struct Frame {
    dmg: i32,
    stun_ms: i32,
    hero_attack_ms: i32,
    hero_pos: Vec2,
    hero_buffs: Vec<u16>,
}

enum Victim {
    /// A red Knight held at this point, its hp topped up.
    Knight((i32, i32)),
    /// Red's left princess tower, the hero 4500 short of it on its own lane, its hp topped up.
    Tower,
}

/// The hero deployed with the victim beside him, held, until both stand; then the press and `frames` frames, frame k being
/// P + k (P the press's first frame). `kill_at`: the victim dies on that frame (the scene's target gone).
fn scene(victim: Victim, frames: usize, kill_at: Option<usize>) -> Vec<Frame> {
    let mut s = battle();
    // Spawns land on the next tick: the Knight is found after it.
    let (hero_at, tower) = match victim {
        Victim::Knight(p) => {
            s.spawn_unit_resolved(Team::Red, "Knight", n(p.0, p.1), None).expect("the Knight");
            (n(AT.0, AT.1), None)
        }
        Victim::Tower => {
            let t = s.entities().filter(|e| e.team == Team::Red && e.kind == EntityKind::PrincessTower).min_by_key(|e| e.pos.x).expect("red's left tower");
            (Vec2::new(t.pos.x, t.pos.y - 4500 * K), Some(t.id))
        }
    };
    s.spawn_unit(Team::Blue, HERO, hero_at, None).expect("the hero");
    s.tick();
    let vid = tower.unwrap_or_else(|| s.entities().find(|e| e.team == Team::Red && e.card == "Knight").expect("the Knight").id);
    let hero: EntityId = find_live(&s, Team::Blue, HERO)[0].id;
    let vpos = s.entity(vid).expect("the victim").pos;
    let (vtop, htop) = (s.entity(vid).unwrap().max_hp, s.entity(hero).unwrap().max_hp);
    let hold = |s: &mut BattleState| {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, hero_at));
            assert!(s.debug_set_hp(hero, htop));
        }
        if s.entity(vid).is_some() {
            assert!(s.debug_set_pos(vid, vpos));
            assert!(s.debug_set_hp(vid, vtop));
        }
    };
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    hold(&mut s);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    for k in 0..frames {
        if kill_at == Some(k) {
            assert!(s.debug_set_hp(vid, 0));
        } else if kill_at.map_or(true, |d| k < d) {
            hold(&mut s);
        } else if s.entity(hero).is_some() {
            assert!(s.debug_set_hp(hero, htop));
        }
        s.tick();
        let v = s.entity(vid);
        let h = s.entity(hero).expect("the hero lives through the scene");
        out.push(Frame {
            dmg: v.as_ref().map_or(0, |v| vtop - v.hp),
            stun_ms: v.as_ref().map_or(0, |v| v.stun_ms),
            hero_attack_ms: h.attack_ms,
            hero_pos: h.pos,
            hero_buffs: h.buffs.iter().map(|b| b.id).filter(|id| *id > 0).collect(),
        });
    }
    out
}

/// The frames a victim took damage on, with the damage.
fn hits(f: &[Frame], from: usize, to: usize) -> Vec<(usize, i32)> {
    (from..=to.min(f.len() - 1)).filter(|k| f[*k].dmg > 0).map(|k| (k, f[k].dmg)).collect()
}

fn scaled(db: &CardDb, base: i32) -> i32 {
    db.scaled(db.index(HERO).unwrap(), LEVEL, base).expect("a level the hero has")
}

#[test]
fn the_form_loads_on_the_current_clients_table() {
    let db = db();
    let sg = surge(&db);
    assert_eq!((sg.damage, sg.stun_ms, sg.no_attack_ms, sg.root.time_ms), (50, 600, 500, 500), "the table's surge");
    assert_eq!(sg.buff.time_ms, 3000);
    assert_eq!(sg.root_range, 5000 * K);
    // The rate is an own buff of HitSpeedMultiplier 360, the surge buff carries the crown share.
    assert_eq!(db.buffs[sg.rate.buff as usize].hit_speed_pct, 360);
    assert_eq!(db.buffs[sg.root.buff as usize].speed_pct, -100);
}

#[test]
fn a_press_bolts_a_pair_every_10_ticks_at_its_damage() {
    let db = db();
    let f = scene(Victim::Knight((9000, 12000)), 70, None);
    assert!(hits(&f, 0, 8).is_empty(), "the cast holds him P .. P + 8: {:?}", hits(&f, 0, 8));
    let surge = hits(&f, 9, 64);
    assert!(surge.len() >= 4, "too few surge pairs: {surge:?}");
    let pair = 2 * scaled(&db, 50);
    for (k, d) in &surge {
        assert_eq!(*d, pair, "P + {k}: a surge pair is two bolts of 50 on his ladder on a lone target");
    }
    for w in surge.windows(2) {
        assert_eq!(w[1].0 - w[0].0, 10, "pairs 10 ticks apart (360 %): {surge:?}");
    }
}

#[test]
fn its_bolts_stun_100_ms_longer_than_the_rows() {
    let f = scene(Victim::Knight((9000, 12000)), 140, None);
    let first_surge = hits(&f, 9, 64)[0].0;
    let first_normal = hits(&f, 75, 139).first().map(|h| h.0).expect("a normal pair after the recovery");
    assert_eq!(f[first_surge].stun_ms - f[first_normal].stun_ms, 100, "the surge entry's 600 ms against the row's 500");
}

#[test]
fn the_recovery_holds_his_attack_10_ticks_from_p_plus_65() {
    let db = db();
    let f = scene(Victim::Knight((9000, 12000)), 140, None);
    assert!(hits(&f, 65, 74).is_empty(), "no attack P + 65 .. P + 74: {:?}", hits(&f, 65, 74));
    let after = hits(&f, 65, 139);
    let (k, d) = *after.first().expect("a normal pair after the recovery");
    assert_eq!(d, 2 * scaled(&db, 46), "the row's own pair again (P + {k})");
    // The hold itself: his attack clock stands at 0 from P + 65 through P + 74 and runs again from P + 75.
    for (k, fr) in f.iter().enumerate().take(75).skip(65) {
        assert_eq!(fr.hero_attack_ms, 0, "his attack clock ran on P + {k}, inside the recovery");
    }
    assert!((75..=77).any(|k| f[k].hero_attack_ms > 0), "his attack clock never ran again after the recovery");
}

#[test]
fn the_swing_in_flight_is_dropped_at_the_end() {
    let f = scene(Victim::Knight((9000, 12000)), 70, None);
    assert_eq!(f[65].hero_attack_ms, 0, "the swing in flight on P + 65");
}

#[test]
fn the_root_holds_him_while_his_target_stands_in_reach_and_he_walks_without_one() {
    let db = db();
    let root = surge(&db).root.buff + 1;
    let f = scene(Victim::Knight((9000, 12000)), 90, None);
    assert!(f[65].hero_buffs.contains(&root), "rooted on P + 65 with his target in reach");
    assert!(!f[80].hero_buffs.contains(&root), "the root gone by P + 80");
    // His target gone on P + 60: no root at the end, and he walks through the recovery (he is not held there).
    let g = scene(Victim::Knight((9000, 12000)), 90, Some(60));
    assert!(!g[65].hero_buffs.contains(&root), "no root without a target");
    assert_ne!(g[65].hero_pos, g[74].hero_pos, "he walks through the recovery");
}

#[test]
fn a_crown_tower_takes_half_of_each_bolt() {
    let db = db();
    let f = scene(Victim::Tower, 70, None);
    let surge = hits(&f, 9, 63);
    assert!(!surge.is_empty(), "no surge pair on the tower");
    let bolt = scaled(&db, 50);
    for (k, d) in &surge {
        assert!((d - bolt).abs() <= 1, "P + {k}: two bolts at half on a crown tower ({d} against one bolt's {bolt})");
    }
}
