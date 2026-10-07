//! THE EVO MEGA KNIGHT (tools/extract_cards.py `uppercut_block`; card.rs `UppercutDef`; state.rs `uppercut_count`,
//! `UppercutRun`, `uppercut_pass`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-MegaKnight-evo-s0): after his deploy's dash blow, his first hit (268 off a
//! Knight, 105 on the ladder) threw it from the hit's tick + 2 toward its king tower, 250 a tick for 8 ticks, then 225,
//! 200, 175, 150, 125 ...; his next hit threw nothing, the one after threw it again the same way.
//! Read off the table, not measured: the root after the throw (no move from 1000 ms after the hit for 400 ms), and the
//! ladder's tail past 125.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_mega_knight`):
//!   - uppercut_never -> `every_second_hit_throws_its_target_toward_its_king` red;
//!   - uppercut_throw_never -> the same red;
//!   - uppercut_flight_collides -> `a_thrown_unit_meets_no_body_in_its_flight_under_client15535_out_of_pass` red;
//!   - uppercut_flight_ground_target -> `a_ground_attacker_lets_the_thrown_unit_go_under_client15535_airborne` red (309);
//!   - uppercut_stand_unread -> `he_stands_until_his_load_reads_100_under_client15535_until_load_100` red (315).
//!
//! targeting.UPPERCUT_FLIGHT_TARGETABILITY = client15535_airborne (item 309) and combat.UPPERCUT_STAND =
//! client15535_until_load_100 (item 315), client 15.535.29, sp-form-MegaKnight-evo-s0 t1078-1100: his uppercut's Knight
//! thrown from the hit's tick + 2, he held it on t1080 and the red Musketeer from t1081, stood with 1,150 on his load
//! timer and walked on the tick it read 100.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, UppercutFlightContact, UppercutFlightTargetability, UppercutStand};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["MegaKnight".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    // Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    s
}

#[test]
fn every_second_hit_throws_its_target_toward_its_king() {
    // The form held at (9000, 12500), a red Knight 1400 ahead in his reach and clear of his body (750 + 500; its king
    // straight ahead at (9000, 29000)), held and topped up until his second hit, then let go (topped up still). No dash:
    // the Knight is inside his jump's minimum range.
    let mut s = battle();
    let (at, kn_at) = (n(9000, 12500), n(9000, 13900));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let (mut hits, mut ys) = (Vec::new(), Vec::new());
    for k in 0..200 {
        assert!(s.debug_set_pos(mk, at));
        let top = s.entity(knight).expect("the Knight").max_hp;
        if hits.len() < 2 {
            assert!(s.debug_set_pos(knight, kn_at));
        }
        assert!(s.debug_set_hp(knight, top));
        s.tick();
        let e = s.entity(knight).expect("the Knight");
        if top - e.hp == 268 {
            hits.push(k);
        }
        ys.push((e.pos.x / K, e.pos.y / K));
    }
    assert!(hits.len() >= 2, "two hits of 268: {hits:?}");
    let (h1, h2) = (hits[0], hits[1]);
    assert!(ys[h1 + 1..h2].iter().all(|p| *p == (9000, 13900)), "no throw after the first hit: {:?}", &ys[h1 + 1..h2]);
    // From h2 + 2: 8 steps of 250, then 225 down to 25, 0, and the step back of 25, straight at its king (x unchanged).
    let mut want: Vec<i32> = vec![250; 8];
    want.extend((1..=9).map(|j| 250 - 25 * j));
    want.extend([0, -25]);
    for (j, w) in want.iter().enumerate() {
        let k = h2 + 2 + j;
        let (dx, dy) = (ys[k].0 - ys[k - 1].0, ys[k].1 - ys[k - 1].1);
        assert_eq!((dx, dy), (0, *w), "step {j} (frame {k}): {:?}", &ys[h2..h2 + 24]);
    }
    // The root: from 1000 ms after the hit, 400 ms, its point kept.
    let root = h2 + 20;
    assert!(ys[root..root + 8].iter().all(|p| *p == ys[root]), "rooted h2 + 20 .. h2 + 27: {:?}", &ys[root..root + 10]);
}

/// knockback.UPPERCUT_FLIGHT_CONTACT (client 15.535.29, sp-form-MegaKnight-evo-s0: the thrown Knight crossed an attacking
/// Musketeer, which moved 0 of 12 overlap ticks): the throw scene with a red Musketeer standing 445 to the side of the
/// throw line, 1,000 up it: its points from the second hit to 20 ticks on.
fn musketeer_in_the_throw(arm: UppercutFlightContact) -> Vec<(i32, i32)> {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["MegaKnight".into(), "Knight".into()], vec!["Knight".into(), "Musketeer".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.uppercut_flight_contact = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let (at, kn_at, mu_at) = (n(9000, 12500), n(9000, 13900), n(9445, 14900));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", mu_at, None).expect("a red Musketeer");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let (mut hits, mut out) = (0, Vec::new());
    for _ in 0..200 {
        assert!(s.debug_set_pos(mk, at));
        let top = s.entity(knight).expect("the Knight").max_hp;
        if hits < 2 {
            assert!(s.debug_set_pos(knight, kn_at));
            assert!(s.debug_set_pos(musk, mu_at));
        }
        assert!(s.debug_set_hp(knight, top));
        let mtop = s.entity(musk).expect("the Musketeer").max_hp;
        assert!(s.debug_set_hp(musk, mtop));
        s.tick();
        if top - s.entity(knight).expect("the Knight").hp == 268 {
            hits += 1;
        }
        if hits >= 2 {
            let m = s.entity(musk).expect("the Musketeer").pos;
            out.push((m.x / K, m.y / K));
            if out.len() > 20 {
                return out;
            }
        }
    }
    panic!("{arm:?}: the scene drifted: no second hit");
}

/// Plant: uppercut_flight_collides.
#[test]
fn a_thrown_unit_meets_no_body_in_its_flight_under_client15535_out_of_pass() {
    let new = musketeer_in_the_throw(UppercutFlightContact::Client15535OutOfPass);
    assert!(new.windows(2).all(|w| w[0] == w[1]), "client15535_out_of_pass: the Musketeer was pushed: {new:?}");
    // NOT VACUOUS: stunned_body pushes it as the Knight flies past.
    let old = musketeer_in_the_throw(UppercutFlightContact::StunnedBody);
    assert!(old.windows(2).any(|w| w[0] != w[1]), "stunned_body: the Musketeer never moved: {old:?}");
}

/// Items 309 and 315: the throw scene with a red Musketeer 2,500 to his side (in his sight, out of his reach, inside his
/// jump's minimum range), the Mega Knight held until his second hit (his uppercut) and let go then; per tick from that
/// hit (h2 = index 0): whether he holds the Knight, his point (native), his load timer.
fn after_the_uppercut(flight: UppercutFlightTargetability, stand: UppercutStand) -> Vec<(bool, (i32, i32), i32)> {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["MegaKnight".into(), "Knight".into()], vec!["Knight".into(), "Musketeer".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.uppercut_flight_targetability = flight;
    cfg.calib.uppercut_stand = stand;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let (at, kn_at, mu_at) = (n(9000, 12500), n(9000, 13900), n(11500, 12500));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", mu_at, None).expect("a red Musketeer");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let (mut hits, mut out) = (0, Vec::new());
    for _ in 0..300 {
        if hits < 2 {
            assert!(s.debug_set_pos(mk, at) && s.debug_set_pos(knight, kn_at));
        }
        assert!(s.debug_set_pos(musk, mu_at));
        for id in [knight, musk, mk] {
            let top = s.entity(id).expect("alive").max_hp;
            assert!(s.debug_set_hp(id, top));
        }
        let top = s.entity(knight).unwrap().max_hp;
        s.tick();
        if hits < 2 && top - s.entity(knight).expect("the Knight").hp == 268 {
            hits += 1;
        }
        if hits >= 2 {
            let m = s.entity(mk).expect("the Mega Knight");
            out.push((m.target == Some(knight), (m.pos.x / K, m.pos.y / K), m.attack_load_ms));
            if out.len() > 40 {
                return out;
            }
        }
    }
    panic!("{flight:?} {stand:?}: the scene drifted: no second hit");
}

/// Plant: uppercut_flight_ground_target.
#[test]
fn a_ground_attacker_lets_the_thrown_unit_go_under_client15535_airborne() {
    let new = after_the_uppercut(UppercutFlightTargetability::Client15535Airborne, UppercutStand::None);
    assert!(new[1].0, "the scene drifted: he does not hold the Knight on the tick after his uppercut: {new:?}");
    assert!(!new[4].0, "client15535_airborne: he holds the thrown Knight two ticks into its flight: {new:?}");
    // NOT VACUOUS: the engine's arm keeps the thrown Knight a ground target.
    let old = after_the_uppercut(UppercutFlightTargetability::Ground, UppercutStand::None);
    assert!(old[4].0, "ground: he let the thrown Knight go: {old:?}");
}

/// Plant: uppercut_stand_unread.
#[test]
fn he_stands_until_his_load_reads_100_under_client15535_until_load_100() {
    let new = after_the_uppercut(UppercutFlightTargetability::Client15535Airborne, UppercutStand::Client15535UntilLoad100);
    let start = new[0].1;
    let first_move = new.iter().position(|r| r.1 != start).unwrap_or_else(|| panic!("the scene drifted: he never moved: {new:?}"));
    assert!(new[first_move - 1].2 <= 150, "client15535_until_load_100: he walked at load {}: {new:?}", new[first_move - 1].2);
    assert!(new[..first_move].iter().filter(|r| r.2 > 100).count() >= 15, "the scene drifted: a short stand: {new:?}");
    // NOT VACUOUS: the engine's arm walks on while his timer is high.
    let old = after_the_uppercut(UppercutFlightTargetability::Client15535Airborne, UppercutStand::None);
    let start = old[0].1;
    let moved = old.iter().position(|r| r.1 != start).unwrap_or_else(|| panic!("none: he never moved: {old:?}"));
    assert!(old[moved].2 > 300, "none: he waited for his timer: {old:?}");
}
