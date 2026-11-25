//! combat.DEFLECT_RETURN_TOWER_SHARE: what a shot the Monk's Deflect sent back deals a crown tower (combat.rs
//! `step_projectiles`, `Projectile::deflected`, DEFLECTED_PRJ_TOWERS_DMG_MUL).
//!
//! THE LAW, measured on client 16.635 (parity r100, the oracle's il-mg-16635 set: 66 of 66 tower arrows a deflecting Monk
//! sent back took 43 of their 173 off their tower, 0 against; DEFLECTED_PRJ_TOWERS_DMG_MUL is 25 in the client tables'
//! globals): a returned shot deals a crown tower 25 % of its crown-tower damage, truncated, after the shot's crown-tower
//! percent; a troop or a building takes it whole. Under whole (the engine's) it deals the tower its crown-tower damage.
//!
//! THE SCENE: 16.635's table, levels 11; blue's Monk alone, held on AT (3500, 20000), 5,500 short of red's left princess
//! tower and inside its range, until the tower's first arrow has landed on him (F, its full damage) and LEAD ticks have
//! passed; then pressed (P the cast's first frame). The tower's arrows that land on him in his deflect window go back at
//! it.
//!
//! WHAT IS PINNED: under client16635_tower_quarter every drop of the tower's hp from P to P + 92 (at least 3) is
//! F * 25 / 100; under whole (the vacuity check) each is F, and the two arms count the same drops.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! deflect_return_tower`):
//!   * `deflect_return_tower_whole` -- the new arm still deals the return whole:
//!     `under_client16635_tower_quarter_a_returned_tower_arrow_deals_its_tower_a_quarter` red (each drop is F).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::CardDb;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, CardTable, DeflectReturnTowerShare};
use royalesim::Team;

const DECK: [&str; 8] = ["Monk", "Knight", "Archer", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
/// The Monk's point, held until his press: 5,500 short of red's left princess tower.
const AT: (i32, i32) = (3500, 20000);
/// The ticks from the Monk's spawn to his press, at least.
const LEAD: u32 = 30;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Under combat.DEFLECT_RETURN_TOWER_SHARE = `arm`: F, the hp the tower's first arrow took off the unpressed Monk, and
/// every drop of the tower's hp from the press's first frame P through P + 92.
fn tower_drops(arm: DeflectReturnTowerShare) -> (i32, Vec<i32>) {
    let table = CardTable::Client160635007Nov03;
    let mut cfg = BattleConfig::with_cards(CardDb::load_table(table).expect("the table loads"));
    cfg.calib.card_table = table;
    cfg.calib.deflect_return_tower_share = arm;
    let deck: Vec<String> = DECK.iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let tower = s.tower_ids(Team::Red)[1..]
        .iter()
        .flatten()
        .copied()
        .min_by_key(|id| s.entity(*id).map_or(i64::MAX, |e| e.pos.dist2(n(AT))))
        .expect("red's princess towers stand");
    let d = isqrt(s.entity(tower).expect("the tower").pos.dist2(n(AT))) / i64::from(K);
    assert!((5000..=6000).contains(&d), "the scene drifted: red's nearer princess tower stands {d} from AT");
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n(AT), None).expect("the Monk");
    let spawned = s.tick_count();
    let mut full = None;
    while full.is_none() || s.tick_count() < spawned + LEAD {
        assert!(s.tick_count() < spawned + 400, "the scene drifted: no tower arrow landed on the Monk");
        assert!(s.debug_set_pos(monk, n(AT)), "the Monk lives");
        let hp = s.entity(monk).expect("the Monk").hp;
        s.tick();
        let lost = hp - s.entity(monk).expect("the Monk").hp;
        if lost > 0 && full.is_none() {
            full = Some(lost);
        }
    }
    let full = full.expect("seen above");
    assert!(s.debug_set_pos(monk, n(AT)), "the Monk lives");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut drops = Vec::new();
    let mut hp = s.entity(tower).expect("the tower").hp;
    while s.tick_count() <= p + 92 {
        s.tick();
        let now = s.entity(tower).expect("the tower stands").hp;
        if now < hp {
            drops.push(hp - now);
        }
        hp = now;
    }
    (full, drops)
}

/// combat.DEFLECT_RETURN_TOWER_SHARE = client16635_tower_quarter (client 16.635, parity r100, the oracle's il-mg-16635 set:
/// 66 of 66 returned tower arrows took 43 of their 173 off their tower): every drop of the tower's hp in the Monk's deflect
/// window is a quarter of F, truncated; under whole (the vacuity check) each is F. Plant: deflect_return_tower_whole.
#[test]
fn under_client16635_tower_quarter_a_returned_tower_arrow_deals_its_tower_a_quarter() {
    let (full, old) = tower_drops(DeflectReturnTowerShare::Whole);
    assert!(full > 0, "the scene drifted: the tower's arrow took nothing off the Monk");
    assert!(old.len() >= 3, "the scene drifted: fewer than 3 returns reached the tower by P + 92: {old:?}");
    assert!(old.iter().all(|d| *d == full), "whole (the vacuity check): each return deals the tower F = {full}: {old:?}");
    let (again, new) = tower_drops(DeflectReturnTowerShare::Client16635TowerQuarter);
    assert_eq!(again, full, "the scene drifted: F differs between the arms");
    assert_eq!(new.len(), old.len(), "the scene drifted: the arms count different returns: {new:?} against {old:?}");
    let quarter = full * 25 / 100;
    assert!(new.iter().all(|d| *d == quarter), "client16635_tower_quarter: each return deals the tower {quarter} (F = {full}): {new:?}");
}

/// combat.DEFLECT_RETURN_TOWER_SHARE ships at whole (client16635_tower_quarter is a candidate until scored).
#[test]
fn the_shipped_arm_is_whole() {
    assert_eq!(Calib::shipped().deflect_return_tower_share, DeflectReturnTowerShare::Whole);
}
