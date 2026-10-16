//! CROWN TOWER TROOPS (card.rs `CardDb::load_tower_troops`, TOWER_TROOPS; state.rs `BattleConfig::tower_troops`): a side's
//! princess towers made with the Dagger Duchess's, the Cannoneer's or the Chef's record, the PrincessTower's own tower
//! record with the troop's attack. Measured on client 16.402 (the live population, parity's r62 item G): 48 of 1,084
//! tower sides field one (Cannoneer 20, Duchess 18, Chef 10), told apart by max_hp (1270 / 1200 / 1240 against 1400 on
//! the tower ladder); the engine shot as a Princess, 50 every 16 ticks, where the Duchess threw 42 every ~9.5 and the
//! Cannoneer fired 125 every 44: 28 first divergences (the tower troop shot the first diverging unit).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! tower_troops`): tower_troops_unloaded -> `a_side_fields_its_tower_troop` red; king_level_unread ->
//! `a_side_s_king_stands_at_its_own_level` red; burst_never and burst_unread ->
//! `under_client16402_burst_attack_the_duchess_throws_her_charges_then_one_each_31_ticks` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Blue fielding `troop` (None: the Princess) at tower level 11: Blue's left tower's record name and hitpoints, and the
/// ticks of its first shots at a red Golem held 5500 ahead of it (in its reach, too big to die).
fn left_tower(troop: Option<&str>) -> (String, i32, Vec<u32>) {
    let mut cfg = config();
    cfg.tower_level = [11, 11];
    cfg.card_level = [11, 11];
    cfg.tower_troops = [troop.map(str::to_string), None];
    let mut s = BattleState::try_new(0, cfg).expect("the battle");
    let (tid, card, max_hp) = s
        .entities()
        .find(|e| e.team == Team::Blue && e.pos == n((3500, 6500)))
        .map(|e| (e.id, e.card.to_string(), e.max_hp))
        .expect("Blue's left tower");
    let at = n((3500, 12000));
    let golem = s.scenario_spawn_now(Team::Red, "Golem", at, None).expect("a red Golem");
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(tid)).count();
    let mut known = mine(&s);
    let mut out = Vec::new();
    for _ in 0..240 {
        if let Some(e) = s.entity(golem) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(golem, at));
            assert!(s.debug_set_hp(golem, full));
        }
        s.tick();
        let m = mine(&s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    (card, max_hp, out)
}

fn gaps(t: &[u32]) -> Vec<u32> {
    t.windows(2).map(|w| w[1] - w[0]).collect()
}

/// Plant: tower_troops_unloaded.
#[test]
fn a_side_fields_its_tower_troop() {
    let (card, princess_hp, t) = left_tower(None);
    assert_eq!(card, "PrincessTower", "no troop: the Princess");
    let g = gaps(&t);
    assert!(g.len() >= 3 && g[1..3] == [16, 16], "the Princess shoots every 16 ticks (800 ms): {t:?}");
    for (troop, hp_base, gap) in [("DaggerDuchess", 1270, 10), ("Cannoneer", 1200, 44), ("ChefTower", 1240, 20)] {
        let (card, hp, t) = left_tower(Some(troop));
        assert_eq!(card, troop, "Blue's princess towers are made with the {troop}'s record");
        // the crown tower ladder scales the troop's hitpoints as it scales the Princess's 1400
        assert_eq!(hp, princess_hp * hp_base / 1400, "{troop}: hitpoints {hp} at tower level 11 (the Princess {princess_hp})");
        let g = gaps(&t);
        assert!(g.len() >= 2 && g[1] == gap, "{troop} shoots every {gap} ticks: {t:?}");
    }
}

#[test]
fn a_tower_troop_that_is_not_a_record_is_refused() {
    let mut cfg = config();
    cfg.tower_troops = [None, Some("Knight".into())];
    assert!(BattleState::try_new(0, cfg).is_err(), "a troop that is not a tower troop record is refused");
}

/// combat.TOWER_TROOP_DAMAGE_LADDER = client16402_card_ladder (client 16.402, parity's r63 census: the Duchess's dagger 171
/// at tower level 16 = 42 x 409 %, the Cannoneer's 511 = 125 x 409 %, the Chef's on the tower ladder): at tower level 16 a
/// Dagger Duchess tower's damage is its record's card-ladder value, a Chef's the tower ladder's; tower_ladder (the vacuity
/// check) scales the Duchess on the tower ladder. Plant: tower_troop_damage_tower_ladder.
#[test]
fn the_duchess_and_the_cannoneer_hit_on_their_card_ladder() {
    use royalesim::state::TowerTroopDamageLadder as L;
    // The damage of the first shot Blue's left tower fires at a red Golem held in its reach, and the record's card-ladder
    // value at 16.
    let damage = |troop: &str, arm: L| -> (i32, i32) {
        let mut cfg = config();
        cfg.tower_troops = [Some(troop.to_string()), None];
        cfg.tower_level = [16, 16];
        cfg.card_level = [16, 16];
        cfg.calib.tower_troop_damage_ladder = arm;
        let mut s = BattleState::try_new(0, cfg).unwrap_or_else(|e| panic!("{troop}: {e}"));
        let tid = s.entities().find(|e| e.team == Team::Blue && e.pos == n((3500, 6500))).map(|e| e.id).expect("Blue's left tower");
        let at = n((3500, 12000));
        let golem = s.scenario_spawn_now(Team::Red, "Golem", at, None).expect("a red Golem");
        let mut shot = None;
        for _ in 0..240 {
            assert!(s.debug_set_pos(golem, at));
            s.tick();
            if let Some(q) = s.projectiles().iter().find(|q| q.firer == Some(tid)) {
                shot = Some(q.damage);
                break;
            }
        }
        let card = s.cards().index(troop).expect("the troop's record");
        (shot.expect("the tower never fired"), s.cards().scaled(card, 16, s.cards().get(card).damage).expect("the card ladder at 16"))
    };
    for troop in ["DaggerDuchess", "Cannoneer"] {
        let (got, card_ladder) = damage(troop, L::Client16402CardLadder);
        assert_eq!(got, card_ladder, "{troop}: its damage on its card ladder at 16");
        let (old, _) = damage(troop, L::TowerLadder);
        assert_ne!(old, card_ladder, "{troop}: tower_ladder gives the card ladder's value too (the vacuity check)");
    }
    let (chef, chef_card) = damage("ChefTower", L::Client16402CardLadder);
    let (chef_old, _) = damage("ChefTower", L::TowerLadder);
    assert_eq!(chef, chef_old, "the Chef's damage stays on the tower ladder");
    assert_ne!(chef, chef_card, "the scene: the Chef's two ladders part at 16");
}

/// BattleConfig::king_level (client 16.402, parity's r64 census: 19 sides of the ob3 live set field a king above their
/// princess towers): Red's king at its own level 13 beside level-10 towers has the level-13 king's hitpoints, Blue's (None)
/// its tower level's; the princesses stay at 10. Plant: king_level_unread.
#[test]
fn a_side_s_king_stands_at_its_own_level() {
    let kings = |tower: i32, king: [Option<i32>; 2]| -> ([i32; 2], [i32; 2]) {
        let mut cfg = config();
        cfg.tower_level = [tower, tower];
        cfg.card_level = [tower, tower];
        cfg.king_level = king;
        let s = BattleState::try_new(0, cfg).expect("the battle");
        let hp = |t: Team| s.tower_hp(t);
        let (b, r) = (hp(Team::Blue), hp(Team::Red));
        ([b[0], r[0]], [b[1], r[1]])
    };
    let (k13, _) = kings(13, [None, None]);
    let (k10, p10) = kings(10, [None, None]);
    assert_ne!(k13[1], k10[1], "the scene: a level-13 king's hitpoints are a level-10 king's");
    let (k, p) = kings(10, [None, Some(13)]);
    assert_eq!(k[1], k13[1], "Red's king not at its own level 13");
    assert_eq!(k[0], k10[0], "Blue's king (None) not at its tower level");
    assert_eq!(p, p10, "the princess towers moved off their tower level");
}

/// The ticks of a Dagger Duchess tower's daggers at a red Golem held in her reach, under combat.TOWER_TROOP_BURST = `arm`.
fn duchess_daggers(arm: royalesim::state::TowerTroopBurst) -> Vec<u32> {
    let mut cfg = config();
    cfg.tower_level = [11, 11];
    cfg.card_level = [11, 11];
    cfg.tower_troops = [Some("DaggerDuchess".to_string()), None];
    cfg.calib.tower_troop_burst = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the battle");
    let tid = s.entities().find(|e| e.team == Team::Blue && e.pos == n((3500, 6500))).map(|e| e.id).expect("Blue's left tower");
    assert!(s.cards().get(s.entity(tid).expect("her tower").card_idx).burst.is_some(), "precondition: her row carries its burst");
    let at = n((3500, 12000));
    let golem = s.scenario_spawn_now(Team::Red, "Golem", at, None).expect("a red Golem");
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(tid)).count();
    let mut known = mine(&s);
    let mut out = Vec::new();
    for _ in 0..240 {
        if let Some(e) = s.entity(golem) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(golem, at));
            assert!(s.debug_set_hp(golem, full));
        }
        s.tick();
        let m = mine(&s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    out
}

/// combat.TOWER_TROOP_BURST = client16402_burst_attack (client 16.402, Oracle's 017 batteries: 207 of 207 daggers on their
/// tick, parity's r67 fit): eight daggers, seven 10 ticks apart and the eighth 15 on, then one every 31 ticks; under
/// row_hit_speed one every 10 for good (the vacuity check). Plants: burst_never, burst_unread.
#[test]
fn under_client16402_burst_attack_the_duchess_throws_her_charges_then_one_each_31_ticks() {
    use royalesim::state::TowerTroopBurst as B;
    let g = gaps(&duchess_daggers(B::Client16402BurstAttack));
    assert!(g.len() >= 10, "her daggers: {g:?}");
    assert_eq!(&g[..7], &[10, 10, 10, 10, 10, 10, 15], "the burst: seven at 10, the eighth 15 on: {g:?}");
    assert!(g[7..10].iter().all(|x| *x == 31), "depleted: one every 31 ticks: {g:?}");
    let g = gaps(&duchess_daggers(B::RowHitSpeed));
    assert!(g.len() >= 10 && g[..10].iter().all(|x| *x == 10), "row_hit_speed (the vacuity check): every 10 ticks: {g:?}");
}

/// A battle with Blue's princess towers Royal Chefs under combat.TOWER_TROOP_COOKING = `arm`.
fn chef_side(arm: royalesim::state::TowerTroopCooking) -> BattleState {
    let mut cfg = config();
    cfg.tower_level = [11, 11];
    cfg.card_level = [11, 11];
    cfg.tower_troops = [Some("ChefTower".to_string()), None];
    cfg.calib.tower_troop_cooking = arm;
    BattleState::try_new(0, cfg).expect("the battle")
}

/// Unit `id` held on `at` for `ticks` ticks, its hp set to `hp_at.1` before the tick `hp_at.0` runs: (each pancake's release
/// tick and first point) and (each tick, `id`'s max hp and hp).
#[allow(clippy::type_complexity)]
fn cook(s: &mut BattleState, id: royalesim::EntityId, at: Vec2, ticks: u32, hp_at: Option<(u32, i32)>) -> (Vec<(u32, Vec2)>, Vec<(u32, i32, i32)>) {
    let mut seen = 0;
    let (mut out, mut rows) = (Vec::new(), Vec::new());
    for _ in 0..ticks {
        assert!(s.debug_set_pos(id, at));
        if let Some((t, hp)) = hp_at {
            if s.tick_count() == t {
                assert!(s.debug_set_hp(id, hp));
            }
        }
        s.tick();
        let p: Vec<_> = s.projectiles().iter().filter(|q| q.pancake).collect();
        if p.len() > seen {
            out.push((s.tick_count() - 1, p.last().expect("a pancake").pos));
        }
        seen = p.len();
        let e = s.entity(id).expect("the friend");
        rows.push((s.tick_count() - 1, e.max_hp, e.hp));
    }
    (out, rows)
}

/// combat.TOWER_TROOP_COOKING = client16402_pancakes (client 16.402, parity's r74 batch-6 census: 83 pancakes in 14 Chef battles;
/// the release on the bar's full + 7, 63 of 83 exact; the landing a level up, 73 of 73): with both Chefs idle the bar fills 50
/// a tick from tick 140, full on 599, the pancake released on 606 from Blue's left tower's centre + 200 toward the Knight, and
/// the next on 1072; landing on 611 it sets the Knight from level 11 to 12, its max hp the card's own at 12 and its hp
/// floor(1000 x M12 / M11); under not_cooked (the vacuity check) nothing is thrown. Plant: chef_never_cooks.
#[test]
fn under_client16402_pancakes_the_chef_cooks_from_tick_140_and_levels_a_friend() {
    use royalesim::state::TowerTroopCooking as C;
    let at = n((3500, 9500));
    let mut s = chef_side(C::Client16402Pancakes);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at, None).expect("the Knight");
    let (l, rows) = cook(&mut s, k, at, 1100, Some((610, 1000)));
    assert_eq!(l.iter().map(|x| x.0).take(2).collect::<Vec<_>>(), vec![606, 1072], "client16402_pancakes: the releases: {l:?}");
    assert_eq!(l[0].1, n((3500, 6700)), "from Blue's left tower, 200 toward the Knight");
    let card = s.entity(k).expect("the Knight").card_idx;
    let base = s.cards().get(card).hitpoints;
    let (m11, m12) = (s.cards().scaled(card, 11, base).expect("level 11"), s.cards().scaled(card, 12, base).expect("level 12"));
    assert_eq!(rows.iter().find(|r| r.0 == 610).map(|r| r.1), Some(m11), "before the landing, level 11: {:?}", &rows[605..615]);
    assert_eq!(rows.iter().find(|r| r.0 == 611).map(|r| (r.1, r.2)), Some((m12, 1000 * m12 / m11)), "the landing on 611, level 12: {:?}", &rows[605..615]);
    let mut s0 = chef_side(C::NotCooked);
    let k0 = s0.scenario_spawn_now(Team::Blue, "Knight", at, None).expect("the Knight");
    let (l0, r0) = cook(&mut s0, k0, at, 1100, None);
    let m0 = s0.entity(k0).expect("the Knight").max_hp;
    assert!(l0.is_empty() && r0.iter().all(|r| r.1 == m0), "not_cooked (the vacuity check): a pancake, or the Knight moved off its level");
}

/// The pick (client 16.402, 82 of 83): the friend with the most hitpoints, un-pancaked first: a Giant at (14500, 9500) and a
/// Knight at (3500, 9500), both held; the first pancake (606) goes to the Giant from Blue's right tower, the second (1072) to
/// the Knight, the Giant now pancaked.
#[test]
fn under_client16402_pancakes_the_pick_is_the_most_hitpoints_un_pancaked_first() {
    use royalesim::state::TowerTroopCooking as C;
    let mut s = chef_side(C::Client16402Pancakes);
    let (ga, ka) = (n((14500, 9500)), n((3500, 9500)));
    let g = s.scenario_spawn_now(Team::Blue, "Giant", ga, None).expect("the Giant");
    let k = s.scenario_spawn_now(Team::Blue, "Knight", ka, None).expect("the Knight");
    let mut starts = Vec::new();
    let mut seen = 0;
    for _ in 0..1100 {
        assert!(s.debug_set_pos(g, ga) && s.debug_set_pos(k, ka));
        s.tick();
        let p: Vec<_> = s.projectiles().iter().filter(|q| q.pancake).collect();
        if p.len() > seen {
            starts.push(p.last().expect("a pancake").pos);
        }
        seen = p.len();
    }
    assert!(starts.len() >= 2, "two pancakes: {starts:?}");
    assert_eq!(starts[0], n((14500, 6700)), "the first to the Giant, from the right tower: {starts:?}");
    assert_eq!(starts[1], n((3500, 6700)), "the second to the Knight, from the left tower: {starts:?}");
    let at12 = |s: &BattleState, id: royalesim::EntityId| {
        let c = s.entity(id).expect("the friend").card_idx;
        s.cards().scaled(c, 12, s.cards().get(c).hitpoints).expect("level 12")
    };
    assert_eq!(s.entity(g).expect("the Giant").max_hp, at12(&s, g), "the Giant a level up");
    assert_eq!(s.entity(k).expect("the Knight").max_hp, at12(&s, k), "the Knight a level up");
}
