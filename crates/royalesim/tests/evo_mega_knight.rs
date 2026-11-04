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
//!   - uppercut_stand_unread -> `he_stands_until_his_load_reads_100_under_client15535_until_load_100` red (315);
//!   - uppercut_aims_king -> `under_client16402_nearest_own_tower_the_throw_runs_at_its_nearest_tower` red;
//!   - copy_form_unstarted -> `under_client16402_deploy_blow_not_dash_a_copys_knight_counts_as_a_played_one` red.
//!
//! knockback.UPPERCUT_THROW_AIM: these scenes are client 15.535.29's, so they run its own_king (two equidistant princess
//! towers would make the old scene a tie); client 16.402 throws at the thrown unit's nearest standing crown tower (parity
//! r75, live13: 78 of 78 throws).
//!
//! targeting.UPPERCUT_FLIGHT_TARGETABILITY = client15535_airborne (item 309) and combat.UPPERCUT_STAND =
//! client15535_until_load_100 (item 315), client 15.535.29, sp-form-MegaKnight-evo-s0 t1078-1100: his uppercut's Knight
//! thrown from the hit's tick + 2, he held it on t1080 and the red Musketeer from t1081, stood with 1,150 on his load
//! timer and walked on the tick it read 100.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{
    BattleConfig, BattleState, UppercutCount, UppercutFinish, UppercutFlightContact, UppercutFlightHold, UppercutFlightTargetability, UppercutRootWindow,
    UppercutStand, UppercutStandContact, UppercutStandDash, UppercutStandVictim, UppercutThrowAim,
};
use royalesim::entity::AttackPhase;
use royalesim::EntityId;
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    battle_aim(UppercutThrowAim::OwnKing)
}

fn battle_aim(aim: UppercutThrowAim) -> BattleState {
    battle_with(|c| c.calib.uppercut_throw_aim = aim)
}

/// `battle` with `tweak` applied to its config.
fn battle_with(tweak: impl FnOnce(&mut BattleConfig)) -> BattleState {
    let mut cfg: BattleConfig = config();
    // the scenes count his hits as the engine did before parity r82 (combat.UPPERCUT_COUNT = hits_and_dash_blows: his
    // second hit throws); client16402_deploy_blow_not_dash's own scenes below set their count
    cfg.calib.uppercut_count = UppercutCount::HitsAndDashBlows;
    // and root the thrown unit as the engine did before parity r83 (knockback.UPPERCUT_ROOT_WINDOW = on_delay: rooted
    // from 1000 ms after the hit); client16402_after_a_free_step's own scenes below set their window
    cfg.calib.uppercut_root_window = UppercutRootWindow::OnDelay;
    tweak(&mut cfg);
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
    // the scenes count his hits as the engine did before parity r82 (combat.UPPERCUT_COUNT = hits_and_dash_blows: his
    // second hit throws); client16402_deploy_blow_not_dash's own scenes below set their count
    cfg.calib.uppercut_count = UppercutCount::HitsAndDashBlows;
    cfg.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
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
    // the scenes count his hits as the engine did before parity r82 (combat.UPPERCUT_COUNT = hits_and_dash_blows: his
    // second hit throws); client16402_deploy_blow_not_dash's own scenes below set their count
    cfg.calib.uppercut_count = UppercutCount::HitsAndDashBlows;
    cfg.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
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

/// Under `aim`: the Evo Mega Knight held at (14000, 14600), a red Knight 1400 ahead (held and topped up until his second
/// hit, then let go, topped up still): the Knight's first throw step (the hit + 2). Its nearest red crown tower is the right
/// princess tower (14500, 25500), about 9,500 away; its king (9000, 29000) about 13,900.
fn first_throw_step(aim: UppercutThrowAim) -> (i32, i32) {
    let mut s = battle_aim(aim);
    let (at, kn_at) = (n(14000, 14600), n(14000, 16000));
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
    let k = hits[1] + 2;
    (ys[k].0 - ys[k - 1].0, ys[k].1 - ys[k - 1].1)
}

/// knockback.UPPERCUT_THROW_AIM = client16402_nearest_own_tower (client 16.402, parity r75, live13: 78 of 78 throws at the
/// thrown unit's nearest standing crown tower): the throw runs at the right princess tower (x up a little), where own_king's
/// runs at the king (x down; the vacuity check). Plant: uppercut_aims_king.
#[test]
fn under_client16402_nearest_own_tower_the_throw_runs_at_its_nearest_tower() {
    let old = first_throw_step(UppercutThrowAim::OwnKing);
    assert!(old.0 < -60 && old.1 > 200, "the scene: own_king throws at the king ({old:?})");
    let new = first_throw_step(UppercutThrowAim::Client16402NearestOwnTower);
    assert!(new.0 > 0 && new.0 < 30 && new.1 > 240, "client16402_nearest_own_tower: not at the right princess tower ({new:?})");
}

/// Whether each of the Evo Mega Knight's first three regular hits (268 off a red Knight put back on its point before every
/// tick) threw it (400 or more toward its king over the hit's tick + 1 .. + 4), under combat.UPPERCUT_COUNT = `arm`. `played`:
/// played (his deploy blow cast) and held at (9000, 12500), the Knight 1,400 ahead, inside his jump's minimum range; else a
/// setup spawn (no deploy blow), free at (9000, 8500), the Knight 5,200 ahead: he jumps to it first.
fn throws(arm: UppercutCount, played: bool) -> Vec<bool> {
    throws_put(arm, played, false)
}

/// `throws`, the setup spawn a copy's (`copy`: state.rs `scenario_spawn_copy`) when not `played`.
fn throws_put(arm: UppercutCount, played: bool, copy: bool) -> Vec<bool> {
    let mut s = battle_with(|c| {
        c.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
        c.calib.uppercut_count = arm;
    });
    let (at, kn_at) = if played { (n(9000, 12500), n(9000, 13900)) } else { (n(9000, 8500), n(9000, 13700)) };
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    let mk = if played {
        s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
        s.tick();
        find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id
    } else if copy {
        s.scenario_spawn_copy(&[(Team::Blue, "MegaKnight_EV1", at, None, None)]).expect("the Mega Knight")[0]
    } else {
        s.scenario_spawn_now(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight")
    };
    let (mut hits, mut steps, mut farthest) = (Vec::new(), Vec::new(), 0);
    for k in 0..400 {
        if played {
            assert!(s.debug_set_pos(mk, at));
        }
        let top = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(knight, kn_at));
        assert!(s.debug_set_hp(knight, top));
        s.tick();
        let e = s.entity(knight).expect("the Knight");
        if top - e.hp == 268 {
            hits.push(k);
        }
        steps.push((e.pos.y - kn_at.y) / K);
        if let Some(m) = s.entity(mk) {
            farthest = farthest.max((m.pos.y - at.y) / K);
        }
        if hits.len() == 3 && k >= hits[2] + 4 {
            break;
        }
    }
    assert!(hits.len() >= 3, "three hits of 268: {hits:?}");
    if !played {
        assert!(farthest >= 2000, "the scene drifted: he never jumped (farthest {farthest})");
    }
    hits.iter().take(3).map(|&h| steps[h + 1..=h + 4].iter().sum::<i32>() >= 400).collect()
}

/// combat.UPPERCUT_COUNT = client16402_deploy_blow_not_dash (client 16.402, parity r81, live13: 128 of 132 hits, the engine's
/// rule 40 of 132): a played Evo Mega Knight's deploy blow is his first attack, so his first hit throws. Plant:
/// uppercut_deploy_uncounted.
#[test]
fn under_client16402_deploy_blow_not_dash_a_played_knights_first_hit_throws() {
    assert_eq!(throws(UppercutCount::Client16402DeployBlowNotDash, true), vec![true, false, true]);
    // NOT VACUOUS: the engine's rule throws on the 2nd
    assert_eq!(throws(UppercutCount::HitsAndDashBlows, true), vec![false, true, false]);
}

/// combat.UPPERCUT_COUNT = client16402_deploy_blow_not_dash: his jump's blow is no attack for the count (live13: 50 of 50 hits
/// after a jump). A setup spawn casts no deploy blow, so he starts at 0. Plant: uppercut_dash_counted.
#[test]
fn under_client16402_deploy_blow_not_dash_his_jump_is_no_attack() {
    assert_eq!(throws(UppercutCount::Client16402DeployBlowNotDash, false), vec![false, true, false]);
    // NOT VACUOUS: the engine's rule counts the jump, so his 1st hit throws
    assert_eq!(throws(UppercutCount::HitsAndDashBlows, false), vec![true, false, true]);
}

/// A COPY OF A BATTLE UNDER WAY (state.rs `scenario_spawn_copy`) starts his count as his play leaves it, his deploy blow
/// counted though not cast: put down where the setup spawn is, he jumps first, and his first hit throws as a played
/// knight's does. Plant: copy_form_unstarted.
#[test]
fn under_client16402_deploy_blow_not_dash_a_copys_knight_counts_as_a_played_one() {
    assert_eq!(throws_put(UppercutCount::Client16402DeployBlowNotDash, false, true), vec![true, false, true]);
    // the control: the same point as a setup spawn starts at 0
    assert_eq!(throws(UppercutCount::Client16402DeployBlowNotDash, false), vec![false, true, false]);
}

/// Under combat.UPPERCUT_COUNT = client16402_deploy_blow_not_dash (his 1st hit his uppercut) and UPPERCUT_STAND = `stand`: a
/// played Evo Mega Knight held at (9000, 12500) kills a red Knight 1,400 ahead (100 hp) with his first hit; a second red Knight
/// is held 2,900 to his right (out of his reach, inside his jump's minimum range), so he walks to it. (ticks from the tick the first
/// Knight is gone to his first step, his load timer on the tick before that step).
fn walk_after_kill(stand: UppercutStand) -> (usize, i32) {
    let mut s = battle_with(|c| {
        c.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
        c.calib.uppercut_count = UppercutCount::Client16402DeployBlowNotDash;
        c.calib.uppercut_stand = stand;
    });
    let at = n(9000, 12500);
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    // his deploy blow lands with nothing in reach (on his landing): his count reads 1
    let mut waited = 0;
    while s.entity(mk).expect("the Mega Knight").deploying || waited < 3 {
        assert!(waited < 80, "the scene drifted: he never landed");
        assert!(s.debug_set_pos(mk, at));
        s.tick();
        waited += 1;
    }
    let victim = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 13900), Some(100)).expect("the first Knight");
    let next_at = n(11900, 12500);
    let next = s.scenario_spawn_now(Team::Red, "Knight", next_at, None).expect("the second Knight");
    let (mut rows, mut gone): (Vec<(Vec2, i32)>, Option<usize>) = (Vec::new(), None);
    for k in 0..200 {
        if gone.is_none() {
            assert!(s.debug_set_pos(mk, at));
        }
        let top = s.entity(next).expect("the second Knight").max_hp;
        assert!(s.debug_set_pos(next, next_at) && s.debug_set_hp(next, top));
        s.tick();
        let m = s.entity(mk).expect("the Mega Knight");
        rows.push((m.pos, m.attack_load_ms));
        if gone.is_none() && s.entity(victim).is_none() {
            gone = Some(k);
        }
        if let Some(h) = gone {
            if let Some(w) = (h + 1..rows.len()).find(|&j| rows[j].0 != at) {
                return (w - h, rows[w - 1].1);
            }
        }
    }
    panic!("{stand:?}: the scene drifted: the first Knight gone {gone:?}, he never stepped after it");
}

/// combat.UPPERCUT_STAND = client16402_after_a_throw (client 16.402, parity r82, live13: after a killing uppercut he walked on
/// the hit + 6 in 22 of 22, never at his load's 100). Plant: uppercut_kill_stands.
#[test]
fn under_client16402_after_a_throw_a_killing_uppercut_arms_no_stand() {
    let (dt, load) = walk_after_kill(UppercutStand::Client16402AfterAThrow);
    assert!(dt <= 8 && load >= 500, "client16402_after_a_throw: he walks after the post-kill wait: {dt} ticks, load {load}");
    // NOT VACUOUS: client15535_until_load_100 holds him to his load's 100
    let (dt0, load0) = walk_after_kill(UppercutStand::Client15535UntilLoad100);
    assert!(load0 <= 150 && dt0 >= 12, "client15535_until_load_100: {dt0} ticks, load {load0}");
}

/// combat.UPPERCUT_STAND = client16402_after_a_throw: an uppercut that threw still holds him to his load's 100 (the scene of
/// he_stands_until_his_load_reads_100_under_client15535_until_load_100).
#[test]
fn under_client16402_after_a_throw_a_throw_still_stands() {
    let new = after_the_uppercut(UppercutFlightTargetability::Client15535Airborne, UppercutStand::Client16402AfterAThrow);
    let start = new[0].1;
    let first_move = new.iter().position(|r| r.1 != start).unwrap_or_else(|| panic!("the scene drifted: he never moved: {new:?}"));
    assert!(new[first_move - 1].2 <= 150, "client16402_after_a_throw: he walked at load {} after a throw: {new:?}", new[first_move - 1].2);
    assert!(new[..first_move].iter().filter(|r| r.2 > 100).count() >= 15, "the scene drifted: a short stand: {new:?}");
}

/// The throw scene of `after_the_uppercut` (his count under hits_and_dash_blows: his second hit throws) with the red Musketeer out
/// of his sight, held at (16500, 4500), and no stand, under targeting.UPPERCUT_FLIGHT_TARGETABILITY = `flight`: per tick from his
/// uppercut (index 0, the hit's tick), whether he holds the Knight, and the Knight's step (native).
fn retake(flight: UppercutFlightTargetability) -> Vec<(bool, i64)> {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["MegaKnight".into(), "Knight".into()], vec!["Knight".into(), "Musketeer".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.uppercut_flight_targetability = flight;
    cfg.calib.uppercut_stand = UppercutStand::None;
    cfg.calib.uppercut_count = UppercutCount::HitsAndDashBlows;
    cfg.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let (at, kn_at, mu_at) = (n(9000, 12500), n(9000, 13900), n(16500, 4500));
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
        let (top, before) = (s.entity(knight).unwrap().max_hp, s.entity(knight).unwrap().pos);
        s.tick();
        let kv = s.entity(knight).expect("the Knight");
        if hits < 2 && top - kv.hp == 268 {
            hits += 1;
        }
        if hits >= 2 {
            let (dx, dy) = (i64::from((kv.pos.x - before.x) / K), i64::from((kv.pos.y - before.y) / K));
            let held: Option<EntityId> = s.entity(mk).expect("the Mega Knight").target;
            out.push((held == Some(knight), royalesim::fixed::isqrt(dx * dx + dy * dy)));
            if out.len() > 40 {
                return out;
            }
        }
    }
    panic!("{flight:?}: the scene drifted: no second hit");
}

/// targeting.UPPERCUT_FLIGHT_TARGETABILITY = client16402_ground_after_last_step (client 16.402, parity r83, live13: ground
/// attackers re-took a thrown unit on its last forward step + 1 in 63 of 76; the engine's client15535_airborne on + 3, 4 of 4).
/// Plant: uppercut_flight_retake_late.
#[test]
fn under_client16402_ground_after_last_step_he_retakes_the_thrown_unit_on_its_zero_step() {
    let last_forward = |r: &[(bool, i64)]| (2..r.len() - 1).find(|&i| r[i].1 > 0 && r[i + 1].1 == 0).unwrap_or_else(|| panic!("the scene drifted: no landing: {r:?}"));
    let new = retake(UppercutFlightTargetability::Client16402GroundAfterLastStep);
    let fl = last_forward(&new);
    assert!(fl >= 10 && !new[4].0, "the scene drifted: a short flight, or he held it airborne: {new:?}");
    assert_eq!((fl + 1..new.len()).find(|&i| new[i].0), Some(fl + 1), "client16402_ground_after_last_step: retaken on its zero step: {new:?}");
    // NOT VACUOUS: client15535_airborne holds it airborne through the step back
    let old = retake(UppercutFlightTargetability::Client15535Airborne);
    let fl0 = last_forward(&old);
    assert_eq!(fl0, fl, "the same flight under both arms");
    assert_eq!((fl0 + 1..old.len()).find(|&i| old[i].0), Some(fl0 + 3), "client15535_airborne: {old:?}");
}

/// knockback.UPPERCUT_FLIGHT_CONTACT = client16402_out_of_pass_from_hit_plus_1 (client 16.402, parity r83, live13: 406 of 406
/// overlap ticks of 34 throws kept the 250 ladder): the thrown Knight meets no body in its flight. Plant: uppercut_flight_collides.
#[test]
fn under_client16402_out_of_pass_from_hit_plus_1_the_thrown_knight_meets_no_body_in_its_flight() {
    let new = musketeer_in_the_throw(UppercutFlightContact::Client16402OutOfPassFromHitPlus1);
    assert!(new.windows(2).all(|w| w[0] == w[1]), "client16402_out_of_pass_from_hit_plus_1: the Musketeer was pushed: {new:?}");
    // NOT VACUOUS: stunned_body (the arm shipped for client 16.402 before it) pushes it as the Knight flies past
    let old = musketeer_in_the_throw(UppercutFlightContact::StunnedBody);
    assert!(old.windows(2).any(|w| w[0] != w[1]), "stunned_body: the Musketeer never moved: {old:?}");
}

/// Under knockback.UPPERCUT_FLIGHT_CONTACT = `arm`: the throw scene of every_second_hit_throws_its_target_toward_its_king with a
/// second red Knight put 800 behind the victim on its line (into its body, the two radii 1,000) on every tick to the hit + 1: the
/// victim's move (native) on the hit + 1, before its first flight step (it attacks him: it takes no walking step).
fn victim_pushed_on_hit_plus_1(arm: UppercutFlightContact) -> i64 {
    let mut s = battle_with(|c| {
        c.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
        c.calib.uppercut_flight_contact = arm;
    });
    let (at, kn_at) = (n(9000, 12500), n(9000, 13900));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    let pusher = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 14700), None).expect("a second red Knight");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let mut hits = 0;
    for _ in 0..200 {
        assert!(s.debug_set_pos(mk, at));
        for id in [knight, pusher] {
            let top = s.entity(id).expect("a Knight").max_hp;
            assert!(s.debug_set_hp(id, top));
        }
        let top = s.entity(knight).expect("the Knight").max_hp;
        if hits < 2 {
            assert!(s.debug_set_pos(knight, kn_at));
        }
        let before = s.entity(knight).expect("the Knight").pos;
        assert!(s.debug_set_pos(pusher, Vec2::new(before.x, before.y + 800 * K)));
        s.tick();
        let after = s.entity(knight).expect("the Knight");
        if hits >= 2 {
            let (dx, dy) = (i64::from((after.pos.x - before.x) / K), i64::from((after.pos.y - before.y) / K));
            return royalesim::fixed::isqrt(dx * dx + dy * dy);
        }
        if top - after.hp == 268 {
            hits += 1;
        }
    }
    panic!("{arm:?}: the scene drifted: no second hit");
}

/// knockback.UPPERCUT_FLIGHT_CONTACT = client16402_out_of_pass_from_hit_plus_1 on the hit + 1 (client 16.402, parity r83, live13:
/// no contact push on a thrown unit from the hit + 1; the Mega Knight stood unpushed on the hit + 1 in 4 of 4 where his victim had
/// been pushing him, the engine pushed him 2 of 2). Plant: uppercut_flight_collides.
#[test]
fn under_client16402_out_of_pass_from_hit_plus_1_the_thrown_knight_takes_no_push_on_the_hit_plus_1() {
    let new = victim_pushed_on_hit_plus_1(UppercutFlightContact::Client16402OutOfPassFromHitPlus1);
    assert_eq!(new, 0, "client16402_out_of_pass_from_hit_plus_1: the thrown Knight was pushed on the hit + 1");
    // NOT VACUOUS: client15535_out_of_pass leaves it a body on the hit + 1
    let old = victim_pushed_on_hit_plus_1(UppercutFlightContact::Client15535OutOfPass);
    assert!(old > 0, "client15535_out_of_pass: the second Knight did not push it on the hit + 1");
}

/// One tick of thrown_rows: the victim's (attack_ms, attack_load_ms) and its point (native).
type ThrownRow = ((i32, i32), (i32, i32));

/// The throw scene of every_second_hit_throws_its_target_toward_its_king with a red `victim` (attacking him) for the Knight, under
/// knockback.UPPERCUT_FLIGHT_HOLD = `hold` and UPPERCUT_ROOT_WINDOW = `root`: per tick from his second hit (index 0, the hit's
/// tick), the victim's (attack_ms, attack_load_ms) and its point (native).
fn thrown_rows(victim: &str, hold: UppercutFlightHold, root: UppercutRootWindow) -> Vec<ThrownRow> {
    let mut s = battle_with(|c| {
        c.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
        c.calib.uppercut_flight_hold = hold;
        c.calib.uppercut_root_window = root;
    });
    let (at, v_at) = (n(9000, 12500), n(9000, 13900));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let v = s.scenario_spawn_now(Team::Red, victim, v_at, None).expect("the victim");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let (mut hits, mut out) = (0, Vec::new());
    for _ in 0..300 {
        assert!(s.debug_set_pos(mk, at));
        let top = s.entity(v).expect("the victim").max_hp;
        if hits < 2 {
            assert!(s.debug_set_pos(v, v_at));
        }
        assert!(s.debug_set_hp(v, top));
        s.tick();
        let e = s.entity(v).expect("the victim");
        if hits < 2 && top - e.hp == 268 {
            hits += 1;
        }
        if hits >= 2 {
            out.push(((e.attack_ms, e.attack_load_ms), (e.pos.x / K, e.pos.y / K)));
            if out.len() > 40 {
                return out;
            }
        }
    }
    panic!("{victim} {hold:?}: the scene drifted: no second hit");
}

/// knockback.UPPERCUT_FLIGHT_HOLD = client16402_reset_from_hit_plus_1 (client 16.402, parity r83, live13: 45 of 46 attacking
/// victims read progress 0 on the hit + 2; the engine paused 11 of 11 swings and resumed them after the flight): a Musketeer
/// shooting him, thrown: its swing resets on the hit + 2 and re-enters fresh after the flight. (A running attack's load timer
/// reads 0, so this scene cannot show the timer running through the flight.) Plant: uppercut_hold_from_first_step.
#[test]
fn under_client16402_reset_from_hit_plus_1_the_thrown_swing_resets_and_re_enters_fresh() {
    let new = thrown_rows("Musketeer", UppercutFlightHold::Client16402ResetFromHitPlus1, UppercutRootWindow::OnDelay);
    assert!(new[1].0 .0 > 0, "the scene drifted: the Musketeer was not swinging on the hit + 1: {new:?}");
    assert_eq!(new[2].0 .0, 0, "client16402_reset_from_hit_plus_1: its swing reset on the hit + 2: {new:?}");
    // NOT VACUOUS: stun_from_first_step pauses its swing through the flight and resumes it
    let old = thrown_rows("Musketeer", UppercutFlightHold::StunFromFirstStep, UppercutRootWindow::OnDelay);
    assert!(old[3].0 .0 > 0, "stun_from_first_step: the swing was reset: {old:?}");
    assert!(old[3..=6].windows(2).all(|w| w[0].0 == w[1].0), "stun_from_first_step: its swing moved in the flight: {old:?}");
    let resumed = |r: &[ThrownRow]| r.iter().skip(20).map(|x| x.0 .0).find(|&p| p > 0);
    let (rn, ro) = (resumed(&new).expect("no attack after the flight"), resumed(&old).expect("no attack after the flight"));
    assert!(rn < ro && rn < 1000, "client16402_reset_from_hit_plus_1: re-entered at {rn}, the paused swing resumed at {ro}: {new:?}");
}

/// knockback.UPPERCUT_FLIGHT_HOLD = client16402_reset_from_hit_plus_1: a thrown Knight's first flight step is the throw's 250 alone,
/// and the flight's hold ends on the same tick under both arms (its first own move after the flight on the same tick).
#[test]
fn under_client16402_reset_from_hit_plus_1_the_flight_ends_as_before() {
    let new = thrown_rows("Knight", UppercutFlightHold::Client16402ResetFromHitPlus1, UppercutRootWindow::OnDelay);
    let old = thrown_rows("Knight", UppercutFlightHold::StunFromFirstStep, UppercutRootWindow::OnDelay);
    let pts = |r: &[ThrownRow]| r.iter().map(|x| x.1).collect::<Vec<_>>();
    assert_eq!(pts(&new)[..28], pts(&old)[..28], "the same flight and root under both arms");
    assert_eq!(pts(&new)[2].1 - pts(&new)[1].1, 250, "the first flight step: {:?}", pts(&new));
}

/// knockback.UPPERCUT_ROOT_WINDOW = client16402_after_a_free_step (client 16.402, parity r83, live13: free on H + 21, kept H + 22 ..
/// H + 30 in 59 of 60, walking again on H + 31 in 48 of 50; the engine kept H + 20 .. H + 27): the Knight thrown under
/// knockback.UPPERCUT_FLIGHT_HOLD = client16402_reset_from_hit_plus_1, so its swing is reset and it walks after the flight (under
/// stun_from_first_step its paused swing resumes and it walks only after it fires). Plant: uppercut_root_on_delay.
#[test]
fn under_client16402_after_a_free_step_the_thrown_knight_steps_once_then_is_rooted_nine_ticks() {
    let new: Vec<(i32, i32)> = thrown_rows("Knight", UppercutFlightHold::Client16402ResetFromHitPlus1, UppercutRootWindow::Client16402AfterAFreeStep).iter().map(|x| x.1).collect();
    assert_ne!(new[21], new[20], "client16402_after_a_free_step: its own step on H + 21: {new:?}");
    assert!(new[22..=30].iter().all(|p| *p == new[21]), "client16402_after_a_free_step: kept H + 22 .. H + 30: {new:?}");
    assert_ne!(new[31], new[30], "client16402_after_a_free_step: it walks again on H + 31: {new:?}");
    // NOT VACUOUS: on_delay roots it from H + 20 and lets it walk on H + 28
    let old: Vec<(i32, i32)> = thrown_rows("Knight", UppercutFlightHold::Client16402ResetFromHitPlus1, UppercutRootWindow::OnDelay).iter().map(|x| x.1).collect();
    assert_eq!(old[21], old[20], "on_delay: rooted on H + 21: {old:?}");
    assert_ne!(old[28], old[27], "on_delay: it walks on H + 28: {old:?}");
}

/// One tick of `throw_rows`, from the second hit (index 0, the hit's tick): the knight's (attack_ms, attack_phase, point, load,
/// holds the Knight), the red Knight's step, and the other unit's (holds the Knight).
type ThrowRow = (i32, AttackPhase, (i32, i32), i32, bool, i64, bool);

/// The other unit of `throw_rows`: (side, card, point, held each tick at the knight's point + this offset from the hit on).
type Other = (Team, &'static str, (i32, i32), Option<(i32, i32)>);

/// The throw scene of `after_the_uppercut` (his second hit throws; UPPERCUT_STAND = client16402_after_a_throw) under `tweak`, with
/// `other` put down first: (side, card, point, held each tick at the knight's point + this offset from the hit on, else held at
/// its point until the hit). 60 rows from the hit.
fn throw_rows(tweak: impl FnOnce(&mut BattleConfig), other: Option<Other>) -> Vec<ThrowRow> {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["MegaKnight".into(), "Knight".into()], vec!["Knight".into(), "Musketeer".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.uppercut_stand = UppercutStand::Client16402AfterAThrow;
    cfg.calib.uppercut_count = UppercutCount::HitsAndDashBlows;
    cfg.calib.uppercut_throw_aim = UppercutThrowAim::OwnKing;
    cfg.calib.uppercut_root_window = UppercutRootWindow::OnDelay;
    // and the flight targetable as before parity r84 (client15535_airborne, the arm these scenes were read under): the shipped
    // client16402_from_hit_plus_2 lets him retake the landed Knight on the hit + 19, which hands his jump stand to it
    cfg.calib.uppercut_flight_targetability = UppercutFlightTargetability::Client15535Airborne;
    tweak(&mut cfg);
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let (at, kn_at) = (n(9000, 12500), n(9000, 13900));
    let o = other.map(|(team, card, p, follow)| (s.scenario_spawn_now(team, card, n(p.0, p.1), None).expect("the other unit"), n(p.0, p.1), follow));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let (mut hits, mut out) = (0, Vec::new());
    for _ in 0..300 {
        if hits < 2 {
            assert!(s.debug_set_pos(mk, at) && s.debug_set_pos(knight, kn_at));
        }
        if let Some((id, p, follow)) = o {
            if let Some(full) = s.entity(id).map(|v| v.max_hp) {
                match follow {
                    Some(d) if hits >= 2 => {
                        let m = s.entity(mk).expect("the Mega Knight").pos;
                        assert!(s.debug_set_pos(id, Vec2::new(m.x + d.0 * K, m.y + d.1 * K)));
                    }
                    _ if hits < 2 => assert!(s.debug_set_pos(id, p)),
                    _ => {}
                }
                assert!(s.debug_set_hp(id, full));
            }
        }
        for id in [knight, mk] {
            let top = s.entity(id).expect("alive").max_hp;
            assert!(s.debug_set_hp(id, top));
        }
        let (top, before) = (s.entity(knight).unwrap().max_hp, s.entity(knight).unwrap().pos);
        s.tick();
        let kv = s.entity(knight).expect("the Knight");
        if hits < 2 && top - kv.hp == 268 {
            hits += 1;
        }
        if hits >= 2 {
            let (dx, dy) = (i64::from((kv.pos.x - before.x) / K), i64::from((kv.pos.y - before.y) / K));
            let m = s.entity(mk).expect("the Mega Knight");
            let other_holds = o.is_some_and(|(id, _, _)| s.entity(id).is_some_and(|v| v.target == Some(knight)));
            out.push((m.attack_ms, m.attack_phase, (m.pos.x / K, m.pos.y / K), m.attack_load_ms, m.target == Some(knight), royalesim::fixed::isqrt(dx * dx + dy * dy), other_holds));
            if out.len() >= 60 {
                return out;
            }
        }
    }
    panic!("the scene drifted: no second hit");
}

/// combat.UPPERCUT_FINISH = client16402_finish_on_hit_plus_1 (client 16.402, parity r84, live14: state 0 with progress 0 on the
/// hit + 1 in 103 of 103 throws and on the hit + 2 in 102 of 102, no entry before the hit + 4; the engine ran the swing on in 26
/// of 27). Plant: uppercut_swing_runs_on.
#[test]
fn under_client16402_finish_on_hit_plus_1_his_attack_ends_on_the_hit_plus_1() {
    let new = throw_rows(|c| c.calib.uppercut_finish = UppercutFinish::Client16402FinishOnHitPlus1, None);
    assert!(new[1].4, "the scene drifted: he lets the Knight go on the hit + 1: {new:?}");
    assert_eq!((new[1].0, new[1].1), (0, AttackPhase::Idle), "client16402_finish_on_hit_plus_1: his attack on the hit + 1: {new:?}");
    assert!(new[2].4, "the scene drifted: he lets the Knight go on the hit + 2: {new:?}");
    assert_eq!((new[2].0, new[2].1), (0, AttackPhase::Idle), "client16402_finish_on_hit_plus_1: a new swing on the hit + 2: {new:?}");
    assert_eq!(new[1].3, new[0].3 - 50, "client16402_finish_on_hit_plus_1: his load timer stood on the hit + 1: {new:?}");
    // NOT VACUOUS: swing_runs_on runs his swing on through the hit + 1
    let old = throw_rows(|c| c.calib.uppercut_finish = UppercutFinish::SwingRunsOn, None);
    assert!(old[1].0 > 0, "swing_runs_on: his swing ended on the hit + 1: {old:?}");
}

/// combat.UPPERCUT_STAND_CONTACT = client16402_unpushed (client 16.402, parity r84, live14: still on 62 of 62 stand ticks with an
/// overlapping ground neighbour): a Blue Knight held each tick 900 beside him (overlapping him by 350) from the hit on. Plant:
/// uppercut_stand_pushed.
#[test]
fn under_client16402_unpushed_he_stands_unpushed_through_his_uppercut_stand() {
    let stand = |r: &[ThrowRow]| -> Vec<(i32, i32)> { r[3..].iter().take_while(|x| x.3 > 100 && x.1 == AttackPhase::Idle).map(|x| x.2).collect() };
    let other = Some((Team::Blue, "Knight", (16500, 4500), Some((900, 0))));
    let new = stand(&throw_rows(|c| c.calib.uppercut_stand_contact = UppercutStandContact::Client16402Unpushed, other));
    assert!(new.len() >= 12, "the scene drifted: a short stand: {new:?}");
    assert!(new.windows(2).all(|w| w[0] == w[1]), "client16402_unpushed: he moved in his stand: {new:?}");
    // NOT VACUOUS: pushed lets the overlapping Knight shift him
    let old = stand(&throw_rows(|c| c.calib.uppercut_stand_contact = UppercutStandContact::Pushed, other));
    assert!(old.first() != old.last(), "pushed: he never moved in his stand: {old:?}");
}

/// combat.UPPERCUT_STAND_DASH = client16402_held_by_the_stand (client 16.402, parity r84, live14: 0 of 102 throws jumped inside the
/// stand; 13 of 14 jumps after one on its end + 17): a red Musketeer held 5000 ahead of his post (in his jump's reach, outside its
/// minimum range edge to edge) until the hit. Under `runs` he jumps at it inside his stand; under the new arm not in it (after it
/// the thrown Knight, back inside his jump's minimum range, is his target, and he walks). Plant: uppercut_stand_dash_runs.
#[test]
fn under_client16402_held_by_the_stand_he_jumps_only_after_his_stand() {
    let other = Some((Team::Red, "Musketeer", (14000, 12500), None));
    let jump = |r: &[ThrowRow]| r.iter().enumerate().skip(1).find(|(k, x)| x.2 != r[k - 1].2 && {
        let (dx, dy) = (i64::from(x.2 .0 - r[k - 1].2 .0), i64::from(x.2 .1 - r[k - 1].2 .1));
        dx * dx + dy * dy > 200 * 200
    }).map(|(k, _)| k);
    let new = throw_rows(|c| c.calib.uppercut_stand_dash = UppercutStandDash::Client16402HeldByTheStand, other);
    let l100 = new.iter().skip(3).position(|x| x.3 <= 100).map(|k| k + 3).expect("no stand end");
    assert!(l100 >= 12, "the scene drifted: a short stand ({l100}): {new:?}");
    let j = jump(&new);
    assert!(j.is_none_or(|j| j > l100), "client16402_held_by_the_stand: he jumped at {j:?}, inside his stand (it ended at {l100}): {new:?}");
    // NOT VACUOUS: runs jumps inside the stand, his load still above 100
    let old = throw_rows(|c| c.calib.uppercut_stand_dash = UppercutStandDash::Runs, other);
    let j0 = jump(&old).unwrap_or_else(|| panic!("runs: no jump: {old:?}"));
    assert!(old[j0 - 1].3 > 100, "runs: the jump came after the stand: {old:?}");
}

/// targeting.UPPERCUT_FLIGHT_TARGETABILITY = client16402_from_hit_plus_2 (client 16.402, parity r84, live14: ground holders dropped
/// the thrown unit on the hit + 2 in 70 of 70; the thrower on + 3, 95 of 97): a Blue Knight held 2500 beside the red Knight until
/// the hit (out of its reach, walking at it). Plant: uppercut_flight_drop_late.
#[test]
fn under_client16402_from_hit_plus_2_a_ground_holder_drops_the_thrown_unit_a_tick_before_the_thrower() {
    let other = Some((Team::Blue, "Knight", (11500, 13900), None));
    let new = throw_rows(|c| c.calib.uppercut_flight_targetability = UppercutFlightTargetability::Client16402FromHitPlus2, other);
    assert!(new[1].4 && new[1].6, "the scene drifted: neither holds the Knight on the hit + 1: {new:?}");
    assert!(new[2].4 && !new[2].6, "client16402_from_hit_plus_2: the thrower keeps it on the hit + 2, the Blue Knight lets it go: {new:?}");
    assert!(!new[3].4, "the thrower lets it go on the hit + 3: {new:?}");
    // NOT VACUOUS: client15535_airborne lets the Blue Knight keep it on the hit + 2
    let old = throw_rows(|c| c.calib.uppercut_flight_targetability = UppercutFlightTargetability::Client15535Airborne, other);
    assert!(old[2].6, "client15535_airborne: the Blue Knight dropped it on the hit + 2: {old:?}");
}

/// Since parity r84 (client15535_airborne from parity r66, ground before it; client 15.535.29 keeps client15535_airborne).
#[test]
fn the_shipped_uppercut_flight_targetability_is_client16402_from_hit_plus_2() {
    assert_eq!(royalesim::state::Calib::shipped().uppercut_flight_targetability, UppercutFlightTargetability::Client16402FromHitPlus2);
}

/// `victim_entry`'s run: per tick (his attack_ms, his target), the second hit's tick h2, the red Knight.
type VictimEntry = (Vec<(i32, Option<EntityId>)>, usize, EntityId);

/// The throw scene of `every_second_hit_throws_its_target_toward_its_king` under combat.UPPERCUT_STAND =
/// client16402_after_a_throw and combat.UPPERCUT_STAND_VICTIM = `arm`: the red Knight held 1400 ahead until his second hit,
/// let fly, and held there again from the hit + 19 (in his reach); per tick (his attack_ms, his target) and the second hit's
/// tick h2.
fn victim_entry(arm: UppercutStandVictim) -> VictimEntry {
    let mut s = battle_with(|c| {
        c.calib.uppercut_stand = UppercutStand::Client16402AfterAThrow;
        c.calib.uppercut_stand_victim = arm;
    });
    let (at, kn_at) = (n(9000, 12500), n(9000, 13900));
    s.spawn_unit(Team::Blue, "MegaKnight_EV1", at, None).expect("the Mega Knight");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    s.tick();
    let mk = find_live(&s, Team::Blue, "MegaKnight_EV1").first().expect("the Mega Knight").id;
    let (mut hits, mut rows) = (Vec::new(), Vec::new());
    for k in 0..200 {
        assert!(s.debug_set_pos(mk, at));
        let top = s.entity(knight).expect("the Knight").max_hp;
        if hits.len() < 2 || hits.get(1).is_some_and(|h2: &usize| k >= h2 + 19) {
            assert!(s.debug_set_pos(knight, kn_at));
        }
        assert!(s.debug_set_hp(knight, top));
        s.tick();
        if top - s.entity(knight).expect("the Knight").hp == 268 {
            hits.push(k);
        }
        let m = s.entity(mk).expect("the Mega Knight");
        rows.push((m.attack_ms, m.target));
        if hits.len() >= 2 && k > hits[1] + 30 {
            break;
        }
    }
    assert!(hits.len() >= 2, "the scene drifted: two hits of 268: {hits:?}");
    (rows, hits[1], knight)
}

/// combat.UPPERCUT_STAND_VICTIM = client16402_held_to_stand_end (client 16.402, parity r85, live14: 14 of 14 retaken victims in
/// reach on the hit + 19 entered on the hit + 22, none of 30 earlier). Plant: uppercut_stand_enters_victim.
#[test]
fn under_client16402_held_to_stand_end_he_attacks_his_thrown_victim_when_his_load_reads_100() {
    let (r, h2, knight) = victim_entry(UppercutStandVictim::Client16402HeldToStandEnd);
    assert_eq!(r[h2 + 19].1, Some(knight), "the scene drifted: he did not retake the Knight on the hit + 19: {:?}", &r[h2..h2 + 24]);
    assert!(r[h2 + 19..=h2 + 21].iter().all(|x| x.0 == 0), "client16402_held_to_stand_end: he entered inside his stand: {:?}", &r[h2..h2 + 24]);
    assert!(r[h2 + 22].0 > 0, "client16402_held_to_stand_end: no entry on the hit + 22: {:?}", &r[h2..h2 + 24]);
    // NOT VACUOUS: enters on the hit + 19
    let (o, h0, _) = victim_entry(UppercutStandVictim::Enters);
    assert!(o[h0 + 19].0 > 0, "enters: no entry on the hit + 19: {:?}", &o[h0..h0 + 24]);
}

#[test]
fn the_shipped_knight_enters_his_victim() {
    assert_eq!(royalesim::state::Calib::shipped().uppercut_stand_victim, UppercutStandVictim::Enters);
}
