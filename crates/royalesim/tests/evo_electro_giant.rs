//! THE EVO ELECTRO GIANT (16.402 on; card.rs `DelevelPulseDef`; state.rs `pulse_pass`, `level_down`), on the 160402017
//! table (calibration cards.CARD_TABLE) at level 11.
//!
//! Read off the table, not measured (Oracle request 27 asks the client): the first firing's tick, the drop's
//! arithmetic and floor. Pinned here as the engine reads the table:
//!   1. the 160402017 table loads the form, its pulse as written;
//!   2. a clock armed at his creation fires 2750 ms on (the creation tick its first 50) and then every 6000 ms; each
//!      firing puts a ring on him that grows from Radius 1 to 6000 over 550 ms and takes each enemy once when it
//!      reaches the enemy's edge; each enemy it takes drops a level: its max hitpoints its card's on the level below,
//!      its hitpoints kept in proportion (floor);
//!   3. the drop stops at the card's first level: a level-11 Princess (Legendary, first level 9) drops twice in three
//!      pulses, a level-11 Golden Knight (Champion, first level 11) never.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_electro_giant`):
//!   - pulse_never -> 2 and 3 red;
//!   - delevel_keeps_stats -> 2 and 3 red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, DelevelPulseDef};
use royalesim::fixed::{milli, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CardTable};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn table() -> CardDb {
    CardDb::load_table(CardTable::Client160402017).expect("the 160402017 table")
}

fn battle() -> BattleState {
    let mut cfg = BattleConfig::with_cards(table());
    cfg.calib.card_table = CardTable::Client160402017;
    cfg.decks = [vec!["ElectroGiant".into(), "Knight".into()], vec!["Knight".into(), "Princess".into(), "GoldenKnight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The level-`level` max hitpoints of `card` on the 160402017 table.
fn max_hp(card: &str, level: i32) -> i32 {
    let db = table();
    let i = db.index(card).expect("the card");
    db.scaled(i, level, db.get(i).hitpoints).expect("the level")
}

/// An evolved Electro Giant put down at `at` (native), the ticks run until he stands; his id. The caller's tick k (from
/// 1) is the k-th after the one he was created on.
fn giant(s: &mut BattleState, at: (i32, i32)) -> EntityId {
    s.spawn_unit(Team::Blue, "ElectroGiant_EV1", n(at.0, at.1), None).expect("the giant");
    for _ in 0..40 {
        s.tick();
        if let Some(g) = find_live(s, Team::Blue, "ElectroGiant_EV1").first() {
            return g.id;
        }
    }
    panic!("the giant never stood");
}

#[test]
fn the_160402017_table_loads_the_evo_electro_giant() {
    let db = table();
    let i = db
        .index("ElectroGiant_EV1")
        .unwrap_or_else(|| panic!("refused: {:?}", db.rejected.iter().find(|(r, _)| r == "ElectroGiant_EV1")));
    let p = db.get(i).evo.as_ref().and_then(|v| v.delevel_pulse).expect("its pulse");
    let want = DelevelPulseDef {
        first_ms: 2750,
        every_ms: 6000,
        hit_speed_scaled: true,
        min_radius: milli(1),
        max_radius: milli(6000),
        life_ms: 550,
        hits_air: true,
        hits_ground: true,
    };
    assert_eq!(p, want);
}

#[test]
fn each_pulse_takes_a_level_off_each_enemy_it_reaches() {
    // The giant (put down on his own half, then held at (9000, 20000) on the red half, the red towers' fire topped up);
    // red Knights held 3000 and 5000 from him on the red half, past a Knight's attack reach (1200 plus both radii) and
    // out of every blue crown tower's (7500 plus both radii). The pulse is made on the 54th tick after his creation tick (2700 ms
    // left after it, 50 a tick); its ring's first update is the next (age 0), and it reaches 3000 - 500 at age 250 (1 +
    // 5999 x 250 / 550 = 2727) and 5000 - 500 at age 450 (4909): ticks 60 and 64. The next pulse is 120 ticks on.
    let mut s = battle();
    let g = giant(&mut s, (9000, 10500));
    let a = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 23000), None).expect("Knight A");
    let b = s.scenario_spawn_now(Team::Red, "Knight", n(12000, 24000), None).expect("Knight B");
    let (m11, m10, m9) = (max_hp("Knight", 11), max_hp("Knight", 10), max_hp("Knight", 9));
    assert_eq!(s.entity(a).expect("A").max_hp, m11, "a level-11 Knight to start");
    assert!(s.debug_set_hp(a, 1000));
    let mut drops: [Vec<(u32, i32, i32)>; 2] = [Vec::new(), Vec::new()];
    for k in 1..=200u32 {
        let top = s.entity(g).expect("the giant").max_hp;
        assert!(s.debug_set_pos(g, n(9000, 20000)) && s.debug_set_hp(g, top));
        assert!(s.debug_set_pos(a, n(9000, 23000)) && s.debug_set_pos(b, n(12000, 24000)));
        let before = [a, b].map(|id| s.entity(id).expect("a Knight").max_hp);
        s.tick();
        for (w, id) in [a, b].iter().enumerate() {
            let e = s.entity(*id).expect("a Knight held alive");
            if e.max_hp != before[w] {
                drops[w].push((k, e.max_hp, e.hp));
            }
        }
    }
    let ticks = |w: usize| drops[w].iter().map(|d| d.0).collect::<Vec<_>>();
    assert_eq!(ticks(0), [60, 180], "Knight A (3000): {drops:?}");
    assert_eq!(ticks(1), [64, 184], "Knight B (5000): {drops:?}");
    assert_eq!((drops[0][0].1, drops[0][1].1), (m10, m9), "one level a pulse: {drops:?}");
    assert_eq!(drops[0][0].2, (1000_i64 * m10 as i64 / m11 as i64) as i32, "1000 of {m11} kept in proportion: {drops:?}");
}

#[test]
fn the_drop_stops_at_the_cards_first_level() {
    // A red Princess (Legendary: level 11 is her third) and a red Golden Knight (Champion: level 11 is his first) held
    // 3000 from the giant, topped up each tick; three pulses reach them (ticks 60, 180 and 300).
    let mut s = battle();
    let g = giant(&mut s, (9000, 10500));
    let p = s.scenario_spawn_now(Team::Red, "Princess", n(6000, 10500), None).expect("the Princess");
    let gk = s.scenario_spawn_now(Team::Red, "GoldenKnight", n(12000, 10500), None).expect("the Golden Knight");
    let mut seen: [Vec<i32>; 2] = [Vec::new(), Vec::new()];
    for _ in 1..=320 {
        for (id, at) in [(g, (9000, 10500)), (p, (6000, 10500)), (gk, (12000, 10500))] {
            let top = s.entity(id).expect("held alive").max_hp;
            assert!(s.debug_set_pos(id, n(at.0, at.1)) && s.debug_set_hp(id, top));
        }
        s.tick();
        for (w, id) in [p, gk].iter().enumerate() {
            let m = s.entity(*id).expect("held alive").max_hp;
            if seen[w].last() != Some(&m) {
                seen[w].push(m);
            }
        }
    }
    let princess = [11, 10, 9].map(|l| max_hp("Princess", l));
    assert_eq!(seen[0], princess, "the Princess: 11, 10, 9 and no lower");
    assert_eq!(seen[1], [max_hp("GoldenKnight", 11)], "the Golden Knight keeps his level");
}
