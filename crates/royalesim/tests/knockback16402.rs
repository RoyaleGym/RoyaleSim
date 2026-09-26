//! THE KNOCKBACK LADDER -- calibration knockback.DISPLACEMENT_LAW = client16402,
//! the shipped displacement law as measured on the Giant of live capture
//! 20260918-122757.b1, ticks 1216..1223 (client 16.402). Implementation: move16402.rs
//! `start_pushback` / `pushback_step` / `nearest_land`; state.rs `arm_ladder` (Resolve)
//! and `phase_path16402` step 0 (the Path phase).
//!
//! WHAT IS PINNED, each from the data and the law, never a pasted number:
//!   1. the live Giant's eight per-axis steps (-21,148) (-18,123) (-14,98) (-10,74)
//!      (-7,49) (-3,24) (0,0) (3,-24) -- v0 175 for an L in (525, 700] along the heading
//!      (-36, 253)/256 -- reproduced by a Fireball whose length is capped to that L
//!      (pathfinding.MAX_PUSHBACK_LENGTH is the one knob that gives the 2018 data such
//!      a push), with the facing and the route untouched while the ladder runs;
//!   2. the cap: `L = min(Pushback, MAX_PUSHBACK_LENGTH)`;
//!   3. the zero-vector rule: `d == 0` pushes +-x by the parity of team_seq (the
//!      shipped client16402_x_by_id_parity), the caster's forward axis under caster_forward,
//!      nothing under none;
//!   4. the back-step: the tick the speed turns negative moves the unit 25 native
//!      units BACK along the heading, drops the path, and the next tick replans;
//!   5. AFFECTS_DEPLOYING_UNITS and IgnorePushback / PushbackAll unchanged; a
//!      deploying ground unit pushed at the river is clamped at the cell edge (as the
//!      walk's position write clamps a deploying unit), a walking one enters the water
//!      and is put on land at the start of the next ladder tick;
//!   6. the Log's push on an off-axis Knight, each arm of knockback.DIRECTION_ROLLING
//!      pinned through the battle's calibration: pure forward under travel_direction
//!      (composed with the law through the source point), and under the shipped
//!      radial_from_projectile_centre away from the point where the Log first touched
//!      it, the vector to within the per-step truncation; a Fireball's is radial;
//!   7. determinism, and a save / load mid-ladder that reproduces every later tick;
//!   8. seat symmetry: a rotation-symmetric pair of Fireballs on off-centre victims
//!      keeps the battle its own mirror image through the whole ladder;
//!   9. THE CHARGE TAIL INSIDE THE LADDER (calibration charge.RESET_ON_KNOCKBACK =
//!      true, the client16402 rule): a Prince's run-up ADVANCES by tdiv(L x 1000,
//!      ChargeRange) on every positive-speed ladder tick (L the ladder's requested
//!      step), reaches 10000 and charges mid-ladder, and is ZEROED -- the charge
//!      with it -- on the zero-speed tick; a charged Prince keeps its charge through
//!      the positive ticks and loses it on that same tick; the landing tick itself
//!      touches neither; under the `false` foil both are held across the ladder.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="knockback_instant_slide"' CARGO_TARGET_DIR=target/plant
//! cargo test --test knockback16402`): the whole length at once, no ladder -- red on
//! (1), (2), (4) and every settled-carry assertion.
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{milli, Vec2, SUBTILE, SUBTILE_PER_MILLITILE as K};
use royalesim::move16402::{ladder_speed, PUSHBACK_DECEL};
use royalesim::state::{BattleConfig, BattleState, Calib, KnockLaw, KnockZeroVector, RollDirection};
use royalesim::{EntityId, Team};
use serde_json::Value;

fn calib() -> Calib {
    Calib::shipped()
}

/// The Fireball's Pushback, NATIVE units, from cards.json.
fn fireball_push_native() -> i32 {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: Value = serde_json::from_str(&text).unwrap();
    let fb = doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "Fireball").unwrap();
    fb["projectile"]["pushback_milli"].as_i64().unwrap() as i32
}

fn log_push_native() -> i32 {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: Value = serde_json::from_str(&text).unwrap();
    let lg = doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "Log").unwrap();
    lg["projectile"]["spawn_projectile"]["pushback_milli"].as_i64().unwrap() as i32
}

/// The rolling Log's half-depth (ProjectileRadiusY) and the Knight's collision radius,
/// NATIVE units, from cards.json.
fn log_half_depth_and_knight_radius_native() -> (i32, i32) {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: Value = serde_json::from_str(&text).unwrap();
    let cards = doc["cards"].as_array().unwrap();
    let lg = cards.iter().find(|c| c["name"] == "Log").unwrap();
    let kn = cards.iter().find(|c| c["name"] == "Knight").unwrap();
    let half_depth = lg["projectile"]["spawn_projectile"]["projectile_radius_y_milli"].as_i64().unwrap() as i32;
    (half_depth, kn["collision_radius_milli"].as_i64().unwrap() as i32)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut c = config();
    f(&mut c.calib);
    c
}

/// Tick calls until every spell object is gone.
fn arrival_calls(s: &BattleState) -> u32 {
    let mut p = s.clone();
    for k in 1..=600 {
        p.tick();
        if p.spells().is_empty() {
            return k;
        }
    }
    panic!("spell never arrived");
}

/// The native position of an entity (positions are whole native units under the
/// 16.402 locomotion).
fn native(p: Vec2) -> (i32, i32) {
    assert_eq!((p.x % K, p.y % K), (0, 0), "a position is a whole number of native units: {p:?}");
    (p.x / K, p.y / K)
}

/// The per-tick native steps of `id` from the tick after `s`'s current one while its
/// ladder runs (the landing tick is `s`'s: the ladder is armed, nothing moved yet),
/// the route after each tick, and (active, speed) after each tick.
#[allow(clippy::type_complexity)]
fn ladder_steps(s: &mut BattleState, id: EntityId) -> (Vec<(i32, i32)>, Vec<Vec<Vec2>>, Vec<(bool, i32)>) {
    let (mut steps, mut routes, mut flags) = (Vec::new(), Vec::new(), Vec::new());
    let mut prev = native(s.entity(id).unwrap().pos);
    assert!(s.entity(id).unwrap().push_active, "the ladder is not armed on the landing tick");
    for _ in 0..60 {
        s.tick();
        let e = s.entity(id).unwrap();
        let now = native(e.pos);
        steps.push((now.0 - prev.0, now.1 - prev.1));
        routes.push(e.route.to_vec());
        flags.push((e.push_active, e.push_speed));
        prev = now;
        if !e.push_active {
            break;
        }
    }
    (steps, routes, flags)
}

/// A Blue Knight walking up the left lane, alone, and a Blue Fireball timed and aimed
/// so that it lands on the tick the Knight stands at `offset` (native) FROM THE IMPACT
/// -- i.e. the impact is `knight - offset`. Returns the battle stopped at the landing
/// tick and the Knight's id. (The Knight moves during the flight, so the tap is solved
/// by a fixed point on a probe battle without the spell.)
fn walking_knight_hit(cfg: BattleConfig, offset: (i32, i32)) -> (BattleState, EntityId) {
    let mut s = BattleState::new(7, cfg);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", t(350, 2100), None).unwrap();
    for _ in 0..10 {
        s.tick();
    }
    assert!(!s.entity(knight).unwrap().route.is_empty(), "vacuous: the Knight is not walking a route");
    let mut tap = s.entity(knight).unwrap().pos;
    let mut arrival = 0;
    for _ in 0..6 {
        let mut trial = s.clone();
        trial.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
        arrival = arrival_calls(&trial);
        let mut probe = s.clone();
        for _ in 0..arrival {
            probe.tick();
        }
        let at = probe.entity(knight).unwrap().pos;
        let want = Vec2::new(at.x - offset.0 * K, at.y - offset.1 * K);
        if want == tap {
            break;
        }
        tap = want;
    }
    s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
    assert_eq!(arrival_calls(&s), arrival);
    for _ in 0..arrival {
        s.tick();
    }
    assert!(s.spells().is_empty());
    let at = native(s.entity(knight).unwrap().pos);
    assert_eq!((at.0 - tap.x / K, at.1 - tap.y / K), offset, "the Knight is not at the intended offset on the landing tick");
    (s, knight)
}

#[test]
fn the_live_giant_ladder_is_reproduced_step_for_step() {
    // MEASURED on the live Giant: a push of L in (525, 700] along the heading
    // (-36, 253)/256 -- eight ticks of (-21,148) (-18,123) (-14,98) (-10,74) (-7,49)
    // (-3,24) (0,0) (3,-24), nodes and facing unchanged. The 2018 data has no such
    // push, so the Fireball's 1800 is CAPPED to the L by MAX_PUSHBACK_LENGTH (the law's
    // own min), and the target offset (-100, 688) -- length 695, heading (-36, 253) --
    // is put at exactly L from the impact so the target IS the offset.
    let (tx, ty) = (-100, 688);
    let l = royalesim::move16402::isqrt(tx * tx + ty * ty);
    assert!(l > 525 && l <= 700 && ladder_speed(l) == 175, "the geometry is not the Giant's: L {l}");
    assert!(fireball_push_native() > l, "the cap must bind");
    let (mut s, knight) = walking_knight_hit(with_calib(|c| c.max_pushback_length = l), (tx, ty));
    let e = s.entity(knight).unwrap();
    let start = native(e.pos);
    let route0 = e.route.to_vec();
    assert_eq!(e.push_speed, 175);
    assert_eq!((e.push_target.x - start.0, e.push_target.y - start.1), (tx, ty), "the target is L along the source line");
    // (the facing is not a view field; that it is never touched is move16402.rs's own
    // unit test `the_ladder_reads_150_down_to_zero_then_one_step_back` and a
    // debug_assert in phase_path16402)
    let (steps, routes, flags) = ladder_steps(&mut s, knight);
    let measured = vec![(-21, 148), (-18, 123), (-14, 98), (-10, 74), (-7, 49), (-3, 24), (0, 0), (3, -24)];
    assert_eq!(steps, measured, "the measured ladder");
    assert_eq!(flags.iter().map(|f| f.1).collect::<Vec<_>>(), vec![150, 125, 100, 75, 50, 25, 0, -25]);
    assert!(flags[..7].iter().all(|f| f.0) && !flags[7].0, "active while the speed is >= 0");
    for r in &routes[..7] {
        assert_eq!(*r, route0, "nodes unchanged during the knockback");
    }
    assert!(routes[7].is_empty(), "the path is dropped on the back-step tick");
    let total = steps.iter().fold((0, 0), |a, s| (a.0 + s.0, a.1 + s.1));
    assert_eq!(total, measured.iter().fold((0, 0), |a, s| (a.0 + s.0, a.1 + s.1)), "the carry is the sum of the measured steps");
    assert_eq!(ladder_travel(l), 500, "25n(n-1)/2 - 25 along the heading; the per-axis truncations make it {total:?}");
}

#[test]
fn the_length_is_capped_at_max_pushback_length() {
    // The same Fireball on the same Knight: uncapped (the shipped 40000) it carries
    // ladder_travel(Pushback) native (the 2018 row's 1800: 1600; the 15.535 row's
    // 1000: 875), capped at 600 it carries ladder_travel(600) = 500; both derived from
    // the law, and the direction (+x) exact.
    let push = fireball_push_native();
    assert!(push > 600, "scene: the Fireball's Pushback {push} must exceed the 600 cap");
    assert!(calib().max_pushback_length > push, "the shipped cap does not bind a Fireball");
    for (cap, want) in [(calib().max_pushback_length, ladder_travel(push)), (600, ladder_travel(600))] {
        let mut s = BattleState::new(3, with_calib(|c| c.max_pushback_length = cap));
        let tap = t(900, 1900);
        s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
        let arrival = arrival_calls(&s);
        for k in 0..arrival {
            if k + 4 == arrival {
                s.spawn_unit(Team::Red, "Knight", Vec2::new(tap.x + SUBTILE, tap.y), None).unwrap();
            }
            s.tick();
        }
        let knight = find_live(&s, Team::Red, "Knight")[0].id;
        let before = native(s.entity(knight).unwrap().pos);
        let e = s.entity(knight).unwrap();
        assert_eq!(e.push_speed, ladder_speed(push.min(cap)), "cap {cap}: v0");
        assert_eq!(e.push_target.x - before.0, push.min(cap), "cap {cap}: the target is L away");
        let (steps, _, _) = ladder_steps(&mut s, knight);
        let total = steps.iter().fold((0, 0), |a, s| (a.0 + s.0, a.1 + s.1));
        assert_eq!(total, (want, 0), "cap {cap}: the carry");
        assert!(s.entity(knight).unwrap().deploying, "the Knight must still be deploying: the carry is the ladder's alone");
    }
    // The law on a length whose ladder starts above the 250 step cap (the 2018
    // Fireball's 1800; the 15.535 one's 1000 starts at 225 and never meets it): v0
    // 300, the cap on the first two steps, then 225 .. 0 and -25 -- 1600 in 13 ticks.
    assert_eq!(ladder_speed(1800), 300);
    assert_eq!(ladder_travel(1800), 1600, "a 1800 push: v0 300, the 250 cap on the first two steps, then 225 .. 0 and -25");
    assert_eq!(ladder_ticks(1800), 13);
}

#[test]
fn a_victim_on_the_impact_goes_by_the_zero_vector_rule() {
    // Two deploying Red Knights each under its own Fireball centre, in one battle: the
    // first troop of the team has team_seq 3 (the three towers take 0..2), the second 4
    // -- so under the shipped rule (+-x by the parity of the id) they go
    // -x and +x, the whole carry each; caster_forward sends both down the caster's
    // forward axis; none leaves both unpushed (and the damage still lands).
    let push = fireball_push_native();
    let carry = ladder_travel(push);
    let (p1, p2) = (t(500, 1900), t(1300, 1900));
    let run = |cfg: BattleConfig| -> Vec<(u32, (i32, i32), bool, i32)> {
        let mut s = BattleState::new(3, cfg);
        s.spawn_unit(Team::Blue, "Fireball", p1, None).unwrap();
        s.spawn_unit(Team::Blue, "Fireball", p2, None).unwrap();
        let arrival = arrival_calls(&s);
        for k in 0..arrival {
            if k + 4 == arrival {
                s.spawn_unit(Team::Red, "Knight", p1, None).unwrap();
                s.spawn_unit(Team::Red, "Knight", p2, None).unwrap();
            }
            s.tick();
        }
        let ids: Vec<EntityId> = find_live(&s, Team::Red, "Knight").iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 2);
        let before: Vec<(i32, i32)> = ids.iter().map(|&id| native(s.entity(id).unwrap().pos)).collect();
        let hp0: Vec<i32> = ids.iter().map(|&id| s.entity(id).unwrap().max_hp).collect();
        for _ in 0..40 {
            if !s.entities().any(|e| e.push_active) {
                break;
            }
            s.tick();
        }
        ids.iter()
            .zip(before)
            .zip(hp0)
            .map(|((&id, b), h0)| {
                let e = s.entity(id).unwrap();
                assert!(e.deploying);
                let n = native(e.pos);
                (e.team_seq, (n.0 - b.0, n.1 - b.1), e.hp < h0, e.team_seq as i32)
            })
            .collect()
    };
    assert_eq!(calib().knock_zero_vector, KnockZeroVector::Client16402XByIdParity, "re-point this test at the shipped zero-vector rule");
    let got = run(config());
    assert_eq!(got.iter().map(|g| g.0 & 1).collect::<Vec<_>>(), vec![1, 0], "team_seq parity of the two Knights");
    assert_eq!(got[0].1, (-carry, 0), "odd team_seq: -x");
    assert_eq!(got[1].1, (carry, 0), "even team_seq: +x");
    assert!(got.iter().all(|g| g.2), "the damage lands either way");
    let got = run(with_calib(|c| c.knock_zero_vector = KnockZeroVector::CasterForward));
    assert!(got.iter().all(|g| g.1 == (0, carry)), "caster_forward: down the Blue caster's +y: {got:?}");
    let got = run(with_calib(|c| c.knock_zero_vector = KnockZeroVector::NoPush));
    assert!(got.iter().all(|g| g.1 == (0, 0) && g.2), "none: unpushed, still damaged: {got:?}");
}

#[test]
fn the_back_step_drops_the_path_and_the_next_tick_replans() {
    // A WALKING Knight (a live route) under an uncapped Fireball from straight behind:
    // v0 300, the speeds 275 .. 0 then -25; the last tick moves it 25 back along the
    // heading (+y), the route is emptied on that tick and non-empty again one tick
    // later (the replan gate sees no path), and the walk resumes.
    let push = fireball_push_native();
    let (mut s, knight) = walking_knight_hit(config(), (0, 300));
    let route0 = s.entity(knight).unwrap().route.to_vec();
    assert!(!route0.is_empty());
    let (steps, routes, flags) = ladder_steps(&mut s, knight);
    let n = ladder_ticks(push) as usize;
    assert_eq!(steps.len(), n, "one tick per speed value and the back-step");
    assert_eq!(*steps.last().unwrap(), (0, -PUSHBACK_DECEL), "the back-step: 25 native units back along the heading");
    assert_eq!(flags.last().unwrap(), &(false, -PUSHBACK_DECEL));
    assert!(steps[..n - 1].iter().all(|st| st.1 > 0 || *st == (0, 0)), "every earlier step is forward: {steps:?}");
    assert!(routes[..n - 1].iter().all(|r| *r == route0), "the route is untouched while the ladder runs");
    assert!(routes[n - 1].is_empty(), "the back-step tick drops the path");
    let pos_end = s.entity(knight).unwrap().pos;
    s.tick();
    let e = s.entity(knight).unwrap();
    assert!(!e.route.is_empty(), "the next tick replans");
    assert!(!e.push_active);
    assert_ne!(e.pos, pos_end, "and the walk resumes");
    let total: i32 = steps.iter().map(|st| st.1).sum();
    assert_eq!(total, ladder_travel(push), "the carry down the heading");
}

#[test]
fn deploying_units_ignore_pushback_and_the_water_edge_under_the_ladder() {
    // AFFECTS_DEPLOYING_UNITS true (shipped): a deploying Knight is pushed; false: it
    // is not (no ladder armed, no displacement) though it still takes the damage.
    // IgnorePushback: a deploying Giant under a Fireball is never armed; under the Log
    // (PushbackAll) it is. THE WATER EDGE: a deploying ground Knight on the near bank
    // pushed straight at the river stops on the last dry cell's edge (the deploying
    // clamp of the position write: y = row x 500 + 499); a WALKING Knight on the same
    // push enters the water cell and is put on the nearest land at the start of the
    // next ladder tick, never ending in the river while its speed is positive.
    let push = fireball_push_native();
    let carry = ladder_travel(push);
    let stage = t(900, 1900);
    let deploying_knight = |cfg: BattleConfig, tap: Vec2, at: Vec2| -> (bool, bool, (i32, i32), i32) {
        let mut s = BattleState::new(3, cfg);
        s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
        let arrival = arrival_calls(&s);
        for k in 0..arrival {
            if k + 4 == arrival {
                s.spawn_unit(Team::Red, "Knight", at, None).unwrap();
            }
            s.tick();
        }
        let id = find_live(&s, Team::Red, "Knight")[0].id;
        let e = s.entity(id).unwrap();
        let (armed, before, hp) = (e.push_active, native(e.pos), e.hp < e.max_hp);
        for _ in 0..40 {
            if !s.entity(id).unwrap().push_active {
                break;
            }
            s.tick();
        }
        let e = s.entity(id).unwrap();
        assert!(e.deploying);
        let n = native(e.pos);
        (armed, hp, (n.0 - before.0, n.1 - before.1), n.1)
    };
    let (armed, hp, d, _) = deploying_knight(config(), stage, Vec2::new(stage.x + SUBTILE, stage.y));
    assert!(armed && hp && d == (carry, 0), "shipped: a deploying Knight is pushed the whole carry ({armed}, {hp}, {d:?})");
    let (armed, hp, d, _) = deploying_knight(with_calib(|c| c.knock_affects_deploying = false), stage, Vec2::new(stage.x + SUBTILE, stage.y));
    assert!(!armed && hp && d == (0, 0), "AFFECTS_DEPLOYING_UNITS = false: damaged, not pushed ({armed}, {hp}, {d:?})");

    // IgnorePushback / PushbackAll
    let mut s = BattleState::new(3, config());
    s.spawn_unit(Team::Blue, "Fireball", stage, None).unwrap();
    let arrival = arrival_calls(&s);
    for k in 0..arrival {
        if k + 4 == arrival {
            s.spawn_unit(Team::Red, "Giant", Vec2::new(stage.x - SUBTILE, stage.y), None).unwrap();
        }
        s.tick();
    }
    let g = find_live(&s, Team::Red, "Giant")[0];
    assert!(card_stat(&s, "Giant").ignore_pushback && g.hp < g.max_hp && !g.push_active, "a Fireball never arms a ladder on an IgnorePushback Giant");
    let mut s = BattleState::new(3, config());
    let tap = t(900, 1100);
    s.spawn_unit(Team::Blue, "Log", tap, None).unwrap();
    let mut armed = false;
    for k in 0..80 {
        if k == 8 {
            s.spawn_unit(Team::Red, "Giant", tap, None).unwrap();
        }
        s.tick();
        if let Some(g) = find_live(&s, Team::Red, "Giant").first() {
            armed |= g.push_active;
        }
    }
    assert!(armed, "the Log (PushbackAll) arms the ladder on the IgnorePushback Giant");

    // the water edge, deploying vs walking
    let a = s.arena().clone();
    let bank = Vec2::new(t(900, 0).x, a.water_y_min - milli(600));
    assert!(a.is_passable_ground(bank));
    let tap = Vec2::new(bank.x, bank.y - SUBTILE);
    let (armed, _, d, y_end) = deploying_knight(config(), tap, bank);
    // the last dry cell's edge (row x 500 + 499, the row before the river),
    // and the ladder's back-step from it
    let edge = a.water_y_min / K - 1;
    assert!(armed && d.0 == 0 && y_end == edge - PUSHBACK_DECEL, "a deploying ground unit is clamped at the water cell's edge: end y {y_end}, edge {edge}, moved {d:?}");
    assert!(d.1 < carry, "the clamp cut the carry short");
    // walking: a Red Knight standing on the bank on the landing tick (it walks -y, away
    // from the river, so it is placed two ticks before the landing a little closer to
    // the water) pushed +y into the river by a Fireball 300 native units behind it
    let bank_hit = || -> (BattleState, EntityId) {
        let mut s = BattleState::new(3, config());
        let mut tap = Vec2::new(bank.x, bank.y - 300 * K);
        let mut spawn_at = Vec2::new(bank.x, bank.y);
        let mut arrival = 0;
        let mut knight = None;
        for _ in 0..8 {
            let mut trial = s.clone();
            trial.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
            arrival = arrival_calls(&trial);
            let mut id = None;
            for k in 0..arrival {
                if k + 2 == arrival {
                    id = Some(trial.scenario_spawn_now(Team::Red, "Knight", spawn_at, None).unwrap());
                }
                trial.tick();
            }
            let id = id.unwrap();
            let at = trial.entity(id).unwrap().pos;
            let want_tap = Vec2::new(at.x, at.y - 300 * K);
            if want_tap == tap && (at.y - bank.y).abs() < 60 * K {
                knight = Some(id);
                break;
            }
            // walk the spawn point back by what the Knight walked (its route leans
            // toward a bridge, so the impact follows its x too), and re-aim
            spawn_at = Vec2::new(spawn_at.x, spawn_at.y + (bank.y - at.y));
            tap = want_tap;
        }
        let knight_id = knight.expect("the bank scene did not converge");
        s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
        let mut id = None;
        for k in 0..arrival {
            if k + 2 == arrival {
                id = Some(s.scenario_spawn_now(Team::Red, "Knight", spawn_at, None).unwrap());
            }
            s.tick();
        }
        assert_eq!(id, Some(knight_id));
        (s, knight_id)
    };
    let (mut s, knight) = bank_hit();
    let e = s.entity(knight).unwrap();
    assert!(e.push_active && !e.deploying, "vacuous: the walking Knight was not armed");
    assert!(a.is_passable_ground(e.pos) && (e.push_target.y - a.water_y_min / K) > 0, "vacuous: the target is not in the river");
    let mut on_water_ticks = Vec::new();
    let mut teleported = false;
    for k in 0..40 {
        let (before_pos, speed, active) = {
            let b = s.entity(knight).unwrap();
            (b.pos, b.push_speed, b.push_active)
        };
        let was_on_water = a.is_water(before_pos);
        if !active {
            break;
        }
        s.tick();
        let e = s.entity(knight).unwrap();
        if a.is_water(e.pos) {
            on_water_ticks.push(k);
        }
        // the ejection: standing on water at the start of a tick with a positive speed,
        // the unit is put on land BEFORE the step, so it can only
        // be on water again if the step itself took it back in
        if was_on_water && speed > 0 {
            teleported = true;
            let n = native(e.pos);
            let b = native(before_pos);
            let land = royalesim::move16402::nearest_land(b.0, b.1, a.cols, a.rows, |c, r| a.cell_bits(c, r) & a.bit_water != 0);
            assert!((n.1 - land.1).abs() <= 250 && (n.0 - land.0).abs() <= 250, "tick {k}: the step from the teleport point {land:?} did not end near it: {n:?}");
        }
    }
    let e = s.entity(knight).unwrap();
    assert!(!on_water_ticks.is_empty(), "vacuous: the walking Knight never entered the river ({on_water_ticks:?})");
    assert!(teleported, "vacuous: no ladder tick started on water with a positive speed");
    assert!(!e.push_active && a.is_passable_ground(e.pos), "the ladder ends on land: the back-step tick moves away from the river");
}

#[test]
fn each_log_arm_pushes_an_off_axis_knight_from_its_own_source_where_a_fireball_pushes_it_radially() {
    // A deploying Red Knight one tile off the Log's roll axis and a tile and a half ahead
    // of the tap, under each arm of knockback.DIRECTION_ROLLING pinned by name through
    // the battle's calibration.
    // * travel_direction composed with the law (the source point one native unit behind
    //   the victim on the axis): exactly (0, +carry), no sideways component.
    // * the shipped radial_from_projectile_centre: away from the Log's centre where it
    //   first touched the Knight. That is the point on the roll axis where the Log's
    //   front face (centre + half-depth) meets the Knight's near edge, or, if the Log was
    //   already past that point when the Knight appeared, the Log's centre then. The
    //   source comes from the data (the half-depth, the radius) and from the battle
    //   (where the Log is when the Knight appears), never a pasted number, and the push
    //   is pinned per axis to within the per-step truncation the Fireball diagonal below
    //   allows. A wrong source point that still gives a diagonal is red. This pins the
    //   engine's law on the tap as given: the angles measured on client 15.535.29 need
    //   the tap snapped to its tile centre, which the shipped build does not do
    //   (placement.TAP_SNAP = none).
    // A Fireball landing on the axis beside it gives the radial ladder, with a sideways
    // component of the same sign as the offset and a length of the carry to within the
    // per-step truncation.
    // Plants (read from spell.rs; not yet run): rolling_push_travel_direction (the
    // shipped arm pushes pure forward) and rolling_push_from_tick_end (the source is a
    // roll step further on, the Log's centre at the end of the touching tick).
    let stage = t(900, 1100);
    let at = Vec2::new(stage.x + SUBTILE, stage.y + 3 * SUBTILE / 2);
    let log_carry = ladder_travel(log_push_native());
    // (the settled push, how far ahead of the tap the Log's roll is when the Knight
    // appears), native
    let log_push = |arm: RollDirection| -> ((i32, i32), i32) {
        let mut s = BattleState::new(3, with_calib(|c| c.knock_direction_rolling = arm));
        s.spawn_unit(Team::Blue, "Log", stage, None).unwrap();
        let mut result = None;
        let mut log_at = None;
        for k in 0..80 {
            if k == 10 {
                s.spawn_unit(Team::Red, "Knight", at, None).unwrap();
                // Blue rolls toward +y; a Log still in the air starts its roll at roll_start.
                log_at = s.spells().iter().find_map(|sp| match &sp.motion {
                    royalesim::spell::SpellMotion::Rolling { pos, .. } => Some(native(*pos).1 - native(stage).1),
                    royalesim::spell::SpellMotion::Airborne { roll_start, .. } => Some(native(*roll_start).1 - native(stage).1),
                    _ => None,
                });
            }
            let before = find_live(&s, Team::Red, "Knight").first().map(|e| (e.push_active, native(e.pos)));
            s.tick();
            if let (Some((true, _)), Some(e)) = (before, find_live(&s, Team::Red, "Knight").first()) {
                if !e.push_active {
                    assert!(e.deploying);
                    result = Some(native(e.pos));
                    break;
                }
            }
        }
        let end = result.unwrap_or_else(|| panic!("{arm:?}: the Log never pushed the off-axis Knight"));
        let start = native(at);
        let log_at = log_at.unwrap_or_else(|| panic!("{arm:?}: no Log in flight or rolling when the Knight appeared"));
        ((end.0 - start.0, end.1 - start.1), log_at)
    };
    assert_eq!(log_push(RollDirection::TravelDirection).0, (0, log_carry), "the travel_direction arm: pure forward by the carry");
    let (d, log_at) = log_push(RollDirection::RadialFromCentre);
    let (half_depth, knight_radius) = log_half_depth_and_knight_radius_native();
    let (across, ahead) = ((at.x - stage.x) / K, (at.y - stage.y) / K);
    let source = (ahead - half_depth - knight_radius).max(log_at);
    let (dx, dy) = (across, ahead - source);
    let len = royalesim::move16402::isqrt(dx * dx + dy * dy);
    let want = (log_carry * dx / len, log_carry * dy / len);
    let tol = 2 * ladder_ticks(log_push_native());
    assert!(
        (d.0 - want.0).abs() <= tol && (d.1 - want.1).abs() <= tol,
        "radial_from_projectile_centre: away from the point {source} native ahead of the tap on the axis (the Log's roll {log_at} ahead when the Knight appeared): want {want:?} to within {tol} per axis, got {d:?}"
    );

    let fb_carry = ladder_travel(fireball_push_native());
    let mut s = BattleState::new(3, config());
    let tap = Vec2::new(at.x - SUBTILE, at.y);
    s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
    let arrival = arrival_calls(&s);
    for k in 0..arrival {
        if k + 4 == arrival {
            s.spawn_unit(Team::Red, "Knight", at, None).unwrap();
        }
        s.tick();
    }
    let id = find_live(&s, Team::Red, "Knight")[0].id;
    let (steps, _, _) = ladder_steps(&mut s, id);
    let d = steps.iter().fold((0, 0), |a, s| (a.0 + s.0, a.1 + s.1));
    assert_eq!(d, (fb_carry, 0), "a Fireball one tile beside it on the axis: radial, +x, the carry");
    // and one that is diagonal: the impact a tile behind and a tile beside
    let mut s = BattleState::new(3, config());
    let tap = Vec2::new(at.x - SUBTILE, at.y - SUBTILE);
    s.spawn_unit(Team::Blue, "Fireball", tap, None).unwrap();
    let arrival = arrival_calls(&s);
    for k in 0..arrival {
        if k + 4 == arrival {
            s.spawn_unit(Team::Red, "Knight", at, None).unwrap();
        }
        s.tick();
    }
    let id = find_live(&s, Team::Red, "Knight")[0].id;
    let (steps, _, _) = ladder_steps(&mut s, id);
    let d = steps.iter().fold((0, 0), |a, s| (a.0 + s.0, a.1 + s.1));
    let tol = 2 * steps.len() as i32;
    let want = fb_carry * 181 / 256;
    assert!(d.0 > 0 && d.1 > 0 && (d.0 - want).abs() <= tol && (d.1 - want).abs() <= tol, "diagonal: {d:?} against {want} per axis (tolerance {tol})");
}

/// A Blue Prince walking the lane, its run-up set to `progress` (and `charged`) on
/// the tick a Red Log lands on it: (push_speed, progress, charged) after every tick
/// of the ladder, the landing tick first.
fn prince_logged_mid_run_up(cfg: BattleConfig, progress: i32, charged: bool) -> (Vec<(i32, i32, bool)>, i32) {
    let mut s = BattleState::new(7, cfg);
    let id = s.scenario_spawn_now(Team::Blue, "Prince", t(375, 850), None).unwrap();
    for _ in 0..12 {
        s.tick();
    }
    let e = s.entity(id).unwrap();
    assert!(!e.route.is_empty() && e.charge_progress > 0, "vacuous: the Prince is not walking its run-up");
    let full = e.hp;
    let tap = Vec2::new(e.pos.x, e.pos.y + 4 * SUBTILE);
    s.spawn_unit(Team::Red, "Log", tap, None).unwrap();
    let mut landed = false;
    for _ in 0..80 {
        s.tick();
        let e = s.entity(id).unwrap();
        if e.hp < full {
            landed = true;
            break;
        }
    }
    assert!(landed, "vacuous: the Log never landed on the Prince");
    assert!(s.debug_set_charge(id, progress, charged));
    let e = s.entity(id).unwrap();
    assert!(e.push_active, "the landing tick arms the ladder");
    let range_raw = card_stat(&s, "Prince").charge.unwrap().range_raw;
    let mut out = vec![(e.push_speed, e.charge_progress, e.charged)];
    for _ in 0..40 {
        s.tick();
        let e = s.entity(id).unwrap();
        out.push((e.push_speed, e.charge_progress, e.charged));
        if !e.push_active {
            break;
        }
    }
    (out, range_raw)
}

#[test]
fn the_charge_tail_runs_inside_the_ladder() {
    // (9). The Log's ladder on a Prince (IgnorePushback, but the Log carries
    // PushbackAll): every tick's speed is the ladder's, and the run-up follows the
    // tail -- +tdiv(L x 1000, ChargeRange) while the speed is positive (L = min(speed,
    // dist, 250) = the speed here: the target is far), 0 on the zero tick and after.
    // Started 400 permille short of 10000 the Prince CHARGES on the first ladder tick
    // (the game's charge event) and is un-charged on the zero tick, like a Prince
    // that was charged when hit. Plant knockback_instant_slide: no ladder, nothing
    // to add.
    let (rows, range_raw) = prince_logged_mid_run_up(config(), 9600, false);
    let (v0, p0, c0) = rows[0];
    assert!(v0 > 0 && p0 == 9600 && !c0, "the landing tick: the ladder armed at {v0}, the run-up as set ({p0}, charged {c0})");
    let mut speed = v0;
    let mut progress = p0;
    let mut charged = false;
    let mut zero_tick = None;
    for (k, &(v, p, c)) in rows.iter().enumerate().skip(1) {
        speed -= PUSHBACK_DECEL;
        assert_eq!(v, speed, "ladder tick {k}: the speed");
        if speed > 0 {
            if !charged {
                progress += speed * 1000 / range_raw;
                if progress >= 10_000 {
                    charged = true;
                    progress = 0;
                }
            }
        } else {
            progress = 0;
            charged = false;
            zero_tick.get_or_insert(k);
        }
        assert_eq!((p, c), (progress, charged), "ladder tick {k} (speed {speed}): the run-up and the charge");
    }
    assert!(rows[1].2, "the first ladder tick (speed {}) takes the run-up past 10000: charged", rows[1].0);
    let z = zero_tick.expect("the ladder never reached its zero tick");
    assert!(!rows[z].2 && rows[z].1 == 0, "the zero tick clears the charge and the run-up");
    assert!(!rows.last().unwrap().2, "un-charged after the back-step");

    // A CHARGED Prince keeps its charge through the positive ticks and loses it on the
    // zero tick -- the landing itself does not clear it.
    let (rows, _) = prince_logged_mid_run_up(config(), 0, true);
    let z = rows.iter().position(|r| r.0 == 0).expect("no zero tick");
    assert!(z >= 2, "vacuous: no positive ladder tick before the zero tick ({rows:?})");
    assert!(rows[..z].iter().all(|r| r.2), "charged on the landing tick and every positive ladder tick ({rows:?})");
    assert!(rows[z..].iter().all(|r| !r.2 && r.1 == 0), "un-charged from the zero tick on ({rows:?})");

    // The foil: charge.RESET_ON_KNOCKBACK = false skips the tail on ladder ticks, the
    // run-up and the charge held across the whole ladder.
    let (rows, _) = prince_logged_mid_run_up(with_calib(|c| c.charge_reset_on_knockback = false), 9600, false);
    assert!(rows.iter().all(|r| r.1 == 9600 && !r.2), "held: {rows:?}");
    let (rows, _) = prince_logged_mid_run_up(with_calib(|c| c.charge_reset_on_knockback = false), 0, true);
    assert!(rows.iter().all(|r| r.2), "held: {rows:?}");
}

#[test]
fn the_ladder_is_deterministic_and_survives_a_save_mid_way() {
    // Two identical battles hash identically through the ladder; a snapshot taken on a
    // tick with a ladder half run (speed > 0, active) loads and reproduces every one of
    // the next 40 ticks. Plant save_drops_knockback (the ladder columns are on it).
    let build = || {
        let mut s = BattleState::new(11, config());
        let stage = t(900, 1900);
        s.spawn_unit(Team::Blue, "Fireball", stage, None).unwrap();
        let arrival = arrival_calls(&s);
        for k in 0..arrival {
            if k + 4 == arrival {
                s.spawn_unit(Team::Red, "Knight", Vec2::new(stage.x + SUBTILE, stage.y + SUBTILE / 2), None).unwrap();
                s.spawn_unit(Team::Red, "Musketeer", Vec2::new(stage.x - SUBTILE, stage.y), None).unwrap();
            }
            s.tick();
        }
        s
    };
    let (mut a, mut b) = (build(), build());
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(a.entities().filter(|e| e.push_active).count() >= 2, "vacuous: no ladders armed");
    for _ in 0..3 {
        a.tick();
        b.tick();
    }
    assert!(a.entities().any(|e| e.push_active && e.push_speed > 0), "vacuous: no ladder mid-way");
    let blob = a.save();
    let mut l = BattleState::load(&blob).expect("load mid-ladder");
    assert_eq!(l.state_hash(), a.state_hash());
    for k in 0..40 {
        a.tick();
        b.tick();
        l.tick();
        assert_eq!(a.state_hash(), b.state_hash(), "tick {k}: two identical battles diverged during the ladder");
        assert_eq!(a.state_hash(), l.state_hash(), "tick {k}: the loaded battle diverged from the saved one");
    }
    assert!(!a.entities().any(|e| e.push_active));
}

#[test]
fn a_rotation_symmetric_pair_of_pushes_keeps_the_battle_its_own_mirror() {
    // Blue Fireballs a deploying Red Knight standing off-centre; Red does the rotated
    // same to a Blue Knight, cast on the same tick. The ladder is frame-free
    // arithmetic (per-axis truncation toward zero, odd under the rotation), so the
    // battle stays its own mirror image on every tick of both ladders -- checked with
    // common::check_mirror, which sees the ladder columns. The victims are OFF the
    // impact point: the zero-vector rule is the one absolute-frame point of the law.
    let mut s = BattleState::new(5, config());
    let a = s.arena().clone();
    let blue_tap = t(900, 1900);
    let red_tap = mirror(&s, blue_tap);
    s.spawn_unit(Team::Blue, "Fireball", blue_tap, None).unwrap();
    s.spawn_unit(Team::Red, "Fireball", red_tap, None).unwrap();
    let arrival = arrival_calls(&s);
    let red_at = Vec2::new(blue_tap.x + SUBTILE, blue_tap.y + SUBTILE / 2);
    let blue_at = mirror(&s, red_at);
    assert_eq!(Vec2::new(a.width - blue_at.x, a.height - blue_at.y), red_at);
    // checked while the victims deploy: once they walk, the shipped search plans in
    // absolute coordinates and the twins may take different routes (mirror.rs pins that)
    let mut checked = 0;
    for k in 0..arrival + 20 {
        if k + 4 == arrival {
            s.spawn_unit(Team::Red, "Knight", red_at, None).unwrap();
            s.spawn_unit(Team::Blue, "Knight", blue_at, None).unwrap();
        }
        s.tick();
        let knights: Vec<_> = s.entities().filter(|e| e.kind == EntityKind::Troop).collect();
        if knights.len() == 2 && knights.iter().any(|e| !e.deploying) {
            break;
        }
        check_mirror(&s).unwrap_or_else(|e| panic!("tick {k}: {e}"));
        if knights.iter().any(|e| e.push_active) {
            checked += 1;
        }
    }
    assert!(checked >= ladder_ticks(fireball_push_native()) as usize, "vacuous: the ladders never ran to the end while the victims deployed ({checked} ticks)");
    let knights: Vec<_> = s.entities().filter(|e| e.kind == EntityKind::Troop).collect();
    assert_eq!(knights.len(), 2);
    assert!(knights.iter().all(|e| !e.push_active));
    assert_eq!(mirror(&s, knights[0].pos), knights[1].pos, "the two carries are rotations of each other");
    assert_eq!(calib().knock_law, KnockLaw::Client16402);
}
