//! THE EVO BABY DRAGON (card.rs `WindDef`; state.rs `evo_after_fire`, `wind_pass`), against client 15.535.29 at level
//! 11.
//!
//! THE MEASUREMENTS (sp-form-BabyDragon-evo-s0; Oracle's sp-bdw-red-6500-6500, -blue-6500-6500 and -red-9500-6500):
//!   - sp-ec-BabyDragon: the third play of an evolved Baby Dragon entry puts the form down (DarkElixirCost 2);
//!   - each attack (re)starts a wind riding on the dragon. From two ticks after the attack a red Knight inside it
//!     walks 42-47 a tick (60 x 70 / 100) and a blue one 78-79 (x 130 / 100);
//!   - its reach is a rectangle in the arena's axes, 8000 by 9000, around the point 1500 in front of the dragon along
//!     the owner's forward, grown by the unit's radius. A red Knight slows on the tick it comes within 5000 in y of that
//!     point, whichever way the dragon faces.
//!
//! THE SCENES hold the dragon and red and blue Knights still (they would walk) and read each Knight's composed speed,
//! `speed_now`, frame by frame: the buff is what the wind does, and the step the walk makes of it.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_baby_dragon`):
//!   - wind_never -> `the_wind_slows_enemies_in_its_rectangle_from_the_tick_after_the_attack` red;
//!   - wind_pulses_at_once -> `the_wind_slows_enemies_in_its_rectangle_from_the_tick_after_the_attack` red (a tick early);
//!   - wind_ally_unbuffed -> `it_speeds_its_own_side_but_not_itself` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

fn battle() -> BattleState {
    let mut cfg = config();
    cfg.decks = [vec!["BabyDragon".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    s
}

/// The dragon at (9000, 8000) and Knights at the given offsets from the wind's point (the dragon's plus 1500 forward,
/// +y for Blue), red or blue, all held where they stand before every tick: on each frame from the first, each Knight's
/// `speed_now` in the Speed column's units and whether the dragon fired on that frame (its attack progress passed a multiple of HitSpeed).
/// `kill_at`: the frame before whose tick the dragon is struck to 0.
fn scene(knights: &[(Team, (i32, i32))], frames: u32, kill_at: Option<u32>) -> (Vec<Vec<i32>>, Vec<bool>, i32) {
    let mut s = battle();
    let at = n(9000, 8000);
    s.spawn_unit_resolved(Team::Blue, "BabyDragon_EV1", at, None).expect("the form");
    for (team, (dx, dy)) in knights {
        s.spawn_unit_resolved(*team, "Knight", n(9000 + dx, 9500 + dy), None).expect("a Knight");
    }
    s.tick();
    let dragon = find_live(&s, Team::Blue, "BabyDragon_EV1")[0].id;
    let mut ids: Vec<(EntityId, Vec2)> = Vec::new();
    for (team, (dx, dy)) in knights {
        let p = n(9000 + dx, 9500 + dy);
        let e = s.entities().find(|e| e.team == *team && e.card == "Knight" && e.pos == p).unwrap_or_else(|| {
            panic!("no Knight at {p:?}: {:?}", s.entities().filter(|e| e.card == "Knight").map(|e| (e.team, e.pos)).collect::<Vec<_>>())
        });
        ids.push((e.id, p));
    }
    let hs = s.cards().get(s.entity(dragon).unwrap().card_idx).hit_speed_ms;
    let spt = s.config().calib.speed_to_subtiles_per_tick;
    let mut speeds = vec![Vec::new(); knights.len()];
    let mut fired = Vec::new();
    let mut prog = 0;
    let mut dragon_speed = 0;
    for k in 0..frames {
        if kill_at == Some(k) {
            assert!(s.debug_set_hp(dragon, 0));
        }
        if s.entity(dragon).is_some() {
            assert!(s.debug_set_pos(dragon, at));
        }
        for (id, p) in &ids {
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, 1766));
        }
        s.tick();
        for (m, (id, _)) in ids.iter().enumerate() {
            speeds[m].push(s.entity(*id).expect("the Knight lives").speed_now / spt);
        }
        let d = s.entity(dragon);
        let p = d.as_ref().map_or(0, |e| e.attack_ms);
        fired.push(d.is_some() && p / hs > prog / hs);
        prog = p;
        if let Some(e) = d {
            dragon_speed = e.speed_now / spt;
        }
    }
    (speeds, fired, dragon_speed)
}

#[test]
fn the_third_play_is_the_evolved_dragon() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "BabyDragon").expect("the dragon in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for _ in 0..2 {
        assert_eq!(next(&s), "BabyDragon", "a basic play");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "BabyDragon", n(14500, 3500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "BabyDragon_EV1", "the third play is the form");
}

#[test]
fn the_wind_slows_enemies_in_its_rectangle_from_the_tick_after_the_attack() {
    // Red Knights (radius 500) 2500 aside (inside), 4400 aside (inside by the radius: 4000 + 500), 4600 aside (out),
    // 4900 ahead (inside: 4500 + 500) and 5100 ahead (out), each clear of the others and of the princess tower, so
    // nothing pushes one.
    let knights = [
        (Team::Red, (-2500, 0)),
        (Team::Red, (-4400, 1500)),
        (Team::Red, (-4600, 3500)),
        (Team::Red, (-2000, 4900)),
        (Team::Red, (2000, 5100)),
    ];
    let (speeds, fired, _) = scene(&knights, 120, None);
    let f = fired.iter().position(|x| *x).expect("the dragon attacks");
    assert!(speeds.iter().all(|sp| sp[f] == 60), "no slow on the attack's own frame: {:?}", speeds.iter().map(|sp| sp[f]).collect::<Vec<_>>());
    let at = |m: usize| speeds[m][f + 1];
    assert_eq!((at(0), at(1), at(2), at(3), at(4)), (42, 42, 60, 42, 60), "the rectangle, grown by the radius, from the next frame");
    assert!(speeds[0][f + 1..].iter().all(|v| *v == 42), "slowed while the dragon attacks: {:?}", &speeds[0][f..]);
}

#[test]
fn it_speeds_its_own_side_but_not_itself() {
    let (speeds, fired, dragon) = scene(&[(Team::Blue, (-2500, 0)), (Team::Red, (2500, 0))], 120, None);
    let f = fired.iter().position(|x| *x).expect("the dragon attacks");
    assert_eq!((speeds[0][f], speeds[0][f + 1]), (60, 78), "a blue Knight inside: 60 x 130 / 100 from the next frame");
    assert_eq!(dragon, 90, "the dragon's IgnoreBuff keeps its own buff off it");
}

#[test]
fn the_wind_outlives_its_dragon_by_2000_ms() {
    // The dragon struck to 0 on frame 60, well inside its wind's life: the wind stands 2000 ms (40 ticks) more.
    let (speeds, fired, _) = scene(&[(Team::Red, (-2500, 0))], 130, Some(60));
    assert!(fired[..60].iter().any(|x| *x), "the dragon attacked before it died");
    let sp = &speeds[0];
    assert!(sp[62..95].iter().all(|v| *v == 42), "slowed for the wind's stay: {:?}", &sp[60..110]);
    assert!(sp[110..].iter().all(|v| *v == 60), "free once the wind is gone: {:?}", &sp[95..]);
}
