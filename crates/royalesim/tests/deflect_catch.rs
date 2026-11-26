//! combat.DEFLECT_CATCH: where a deflecting champion's area (card.rs `AbilityEffect::Deflect::radius`, the Monk's 1500)
//! catches an enemy shot at him and where the caught shot goes back from (combat.rs `step_projectiles`,
//! `deflect_reaches`, `flight_height`, `DEFLECT_FLIGHT_ROWS`).
//!
//! THE LAW, measured on client 16.635 (parity r101, the oracle's il-mg-16635 set: 72 of 72 caught shots in 12 deflect
//! windows and 667 of 667 tick decisions; the ground plane fits 23 of the 72): his area is a sphere standing on the
//! ground. A shot is caught on the tick its point after the step, at its flight height there, lies strictly inside it,
//! or on its arrival. The height is the arc ProjectileStartZ x (1 less t / total) plus (t less total) t Gravity x -500 /
//! 1000, each division truncated, t and total whole ticks of flight (the distance from its flight start to its point,
//! and to his, over its speed). A splash shot is caught too and lands on him alone. The caught shot goes back from its
//! own point (on an arrival, its point before the step). Under flat_edge (the engine's) a single-target shot is caught
//! when its ground point comes within 1500 of him and goes back from his centre, and a splash shot lands.
//!
//! THE SCENES: 16.635's table, levels 11.
//!   * The arrows: blue's Monk alone, held on (3500, Y) below red's left princess tower for each Y in YS, pressed after
//!     the tower's first arrow has landed and LEAD ticks have passed; the first arrow launched 10 or more ticks after the
//!     press is followed: its launch point L, its point after each tick it flew on, the tick C he lost hp to it, and the
//!     point B of the shot sent back after C.
//!   * The splash: blue's Monk held on MONK, a blue Knight held on KNIGHT (943 from him), a red Ice Wizard held on WIZ
//!     (5,000 up his lane), pressed after LEAD ticks; the first shot at him launched 30 or more ticks after the press.
//!
//! WHAT IS PINNED:
//!   * flat_edge: every point the arrow held after a tick it flew on lies outside 1500 of him on the ground, and B is his
//!     centre; the Ice Wizard's shot lands on him and splashes the Knight, and nothing goes back.
//!   * client16635_flight_height: every such point lies outside the sphere at the arrow's height (the PrincessTower row
//!     of DEFLECT_FLIGHT_ROWS, read off L), and B is the arrow's own point: inside the sphere a step nearer him, or its
//!     last point on an arrival; C is never before flat_edge's. The Ice Wizard's shot is caught: he loses hp, the
//!     Knight none, and a shot goes back.
//!   * The vacuity check: for at least one Y the two arms' C differ.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! deflect_catch`):
//!   * `deflect_catch_on_ground` -- the new arm still catches on the ground plane, a single-target shot only, back from
//!     his centre: `under_client16635_flight_height_an_arrow_is_caught_inside_the_sphere_at_its_height` and
//!     `under_client16635_flight_height_a_splash_shot_is_caught_and_lands_on_him_alone` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::CardDb;
use royalesim::combat::DEFLECT_FLIGHT_ROWS;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, CardTable, DeflectCatch};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Monk", "Knight", "IceWizard", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
/// The Monk's points below red's left princess tower (3500, 25500), 5,000 to 5,600 short of its centre.
const YS: [i32; 13] = [19900, 19950, 20000, 20050, 20100, 20150, 20200, 20250, 20300, 20350, 20400, 20450, 20500];
/// The ticks from the scene's start to the press, at least.
const LEAD: u32 = 30;
/// The Monk's deflect radius, native units.
const R: i64 = 1500;
/// The splash scene's points.
const MONK: (i32, i32) = (3500, 12500);
const KNIGHT: (i32, i32) = (4300, 12000);
const WIZ: (i32, i32) = (3500, 17500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// 16.635's table, levels 11, combat.DEFLECT_CATCH = `arm`, past the deploy lockout.
fn battle(arm: DeflectCatch) -> BattleState {
    let table = CardTable::Client160635007Nov03;
    let mut cfg = BattleConfig::with_cards(CardDb::load_table(table).expect("the table loads"));
    cfg.calib.card_table = table;
    cfg.calib.deflect_catch = arm;
    let deck: Vec<String> = DECK.iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s
}

fn hp(s: &BattleState, id: EntityId) -> i32 {
    s.entity(id).expect("the unit lives").hp
}

/// The shots at `id` that no deflect sent back, after the tick: (point, speed in SUBTILES a tick).
fn shots_at(s: &BattleState, id: EntityId) -> Vec<(Vec2, i32)> {
    s.projectiles().iter().filter(|q| q.target == id && !q.deflected).map(|q| (q.pos, q.speed)).collect()
}

/// `a` to `b`, native units, squared.
fn native2(a: Vec2, b: Vec2) -> i64 {
    let k = i64::from(K);
    let (dx, dy) = (i64::from(a.x - b.x) / k, i64::from(a.y - b.y) / k);
    dx * dx + dy * dy
}

/// The arrow's flight height (native units) at `q` on its flight from `l` to `at` at `speed` (native units a tick): the
/// PrincessTower row of DEFLECT_FLIGHT_ROWS on the arc of combat.rs `flight_height`.
fn arrow_height(l: Vec2, q: Vec2, at: Vec2, speed: i64) -> i64 {
    let &(_, z0, g) = DEFLECT_FLIGHT_ROWS.iter().find(|(name, _, _)| *name == "PrincessTower").expect("the princess tower's row");
    let (z0, g) = (i64::from(z0), i64::from(g));
    let total = (isqrt(native2(at, l)) / speed).max(1);
    let t = isqrt(native2(q, l)) / speed;
    (t - total) * t * (g * -500 / 1000) + -z0 * t / total + z0
}

/// One followed arrow: its launch point L, its point after each tick it flew on, the tick C he lost hp to it, the point
/// B the shot sent back held after C, and its speed (native units a tick).
#[derive(Debug)]
struct Followed {
    launch: Vec2,
    path: Vec<Vec2>,
    caught_on: u32,
    back: Vec2,
    speed: i64,
}

/// THE ARROWS' SCENE under `arm`, the Monk on (3500, `y`).
fn follow_arrow(arm: DeflectCatch, y: i32) -> Followed {
    let mut s = battle(arm);
    let at = n((3500, y));
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", at, None).expect("the Monk");
    let spawned = s.tick_count();
    let mut landed = false;
    while !landed || s.tick_count() < spawned + LEAD {
        assert!(s.tick_count() < spawned + 400, "the scene drifted: no tower arrow landed on the Monk (Y {y})");
        assert!(s.debug_set_pos(monk, at), "the Monk lives");
        let before = hp(&s, monk);
        s.tick();
        landed |= hp(&s, monk) < before;
    }
    assert!(s.debug_set_pos(monk, at), "the Monk lives");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut flying = !shots_at(&s, monk).is_empty();
    let mut followed: Option<(Vec2, i64, Vec<Vec2>)> = None;
    while s.tick_count() < p + 90 {
        assert!(s.debug_set_pos(monk, at), "the Monk lives");
        let now = s.tick_count();
        let before = hp(&s, monk);
        s.tick();
        let lost = before - hp(&s, monk);
        let shots = shots_at(&s, monk);
        assert!(shots.len() <= 1, "the scene drifted: {} arrows at him at once (Y {y})", shots.len());
        match followed.as_mut() {
            None => {
                if let (false, true, Some(&(pos, speed))) = (flying, now >= p + 10, shots.first()) {
                    assert_eq!(lost, 0, "the scene drifted: he lost hp on the arrow's launch tick (Y {y})");
                    followed = Some((pos, i64::from(speed / K), Vec::new()));
                }
            }
            Some((launch, speed, path)) => {
                if lost > 0 {
                    assert!(shots.is_empty(), "the caught arrow is gone after its catch tick (Y {y})");
                    let back = s
                        .projectiles()
                        .iter()
                        .filter(|q| q.deflected)
                        .map(|q| q.pos)
                        .min_by_key(|q| q.dist2(at))
                        .expect("the caught arrow is sent back on its catch tick");
                    return Followed { launch: *launch, path: path.clone(), caught_on: now, back, speed: *speed };
                }
                let &(pos, _) = shots.first().expect("the followed arrow flies on until he loses hp to it");
                path.push(pos);
            }
        }
        flying = !shots.is_empty();
    }
    panic!("the scene drifted: no arrow followed and caught by P + 90 (Y {y})");
}

/// combat.DEFLECT_CATCH = client16635_flight_height (client 16.635, parity r101, the oracle's il-mg-16635 set: 66 of 66
/// princess-tower arrows caught inside the sphere at their height, 49 of the 72 caught shots a tick after the ground
/// plane would): the followed arrow flies on while it is outside the sphere at its height and goes back from its own
/// point inside it. Under flat_edge it flies on while it is outside 1500 on the ground and goes back from his centre.
/// The vacuity check: the arms part on at least one Y. Plant: deflect_catch_on_ground.
#[test]
fn under_client16635_flight_height_an_arrow_is_caught_inside_the_sphere_at_its_height() {
    let mut parted = Vec::new();
    for y in YS {
        let at = n((3500, y));
        let old = follow_arrow(DeflectCatch::FlatEdge, y);
        let new = follow_arrow(DeflectCatch::Client16635FlightHeight, y);
        assert!(!old.path.is_empty() && !new.path.is_empty(), "the scene drifted: an arrow caught on its first step (Y {y})");
        for q in &old.path {
            assert!(native2(*q, at) > R * R, "flat_edge (Y {y}): the arrow flies on only outside 1500 on the ground: {old:?}");
        }
        assert_eq!(old.back, at, "flat_edge (Y {y}): the caught arrow goes back from his centre: {old:?}");
        let height = |q: Vec2| arrow_height(new.launch, q, at, new.speed);
        for q in &new.path {
            let z = height(*q);
            assert!(native2(*q, at) + z * z >= R * R, "client16635_flight_height (Y {y}): the arrow flies on only outside the sphere ({q:?} at height {z}): {new:?}");
        }
        let last = *new.path.last().expect("checked above");
        assert_ne!(new.back, at, "client16635_flight_height (Y {y}): the caught arrow goes back from its own point, not his centre: {new:?}");
        if new.back != last {
            let z = height(new.back);
            assert!(native2(new.back, at) + z * z < R * R, "client16635_flight_height (Y {y}): it is caught inside the sphere ({:?} at height {z}): {new:?}", new.back);
            assert!(native2(new.back, at) < native2(last, at), "client16635_flight_height (Y {y}): it is caught a step nearer him: {new:?}");
        }
        assert!(new.caught_on >= old.caught_on, "client16635_flight_height (Y {y}): never caught before flat_edge's tick ({} against {})", new.caught_on, old.caught_on);
        if new.caught_on != old.caught_on {
            parted.push(y);
        }
    }
    assert!(!parted.is_empty(), "the scene drifted: the arms catch every followed arrow on the same tick (the vacuity check)");
}

/// THE SPLASH SCENE under `arm`: the hp the Monk and the Knight lose on the tick the followed Ice Wizard shot ends, and
/// whether a shot a deflect sent back flies after it.
fn splash_end(arm: DeflectCatch) -> (i32, i32, bool) {
    let mut s = battle(arm);
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n(MONK), None).expect("the Monk");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", n(KNIGHT), None).expect("the Knight");
    let wiz = s.scenario_spawn_now(Team::Red, "IceWizard", n(WIZ), None).expect("the Ice Wizard");
    let hold = |s: &mut BattleState| {
        assert!(s.debug_set_pos(monk, n(MONK)) && s.debug_set_pos(knight, n(KNIGHT)) && s.debug_set_pos(wiz, n(WIZ)), "the scene drifted: a unit is gone");
    };
    for _ in 0..LEAD {
        hold(&mut s);
        s.tick();
    }
    hold(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut flying = !shots_at(&s, monk).is_empty();
    let mut followed = false;
    while s.tick_count() < p + 80 {
        hold(&mut s);
        let now = s.tick_count();
        let (m, k) = (hp(&s, monk), hp(&s, knight));
        s.tick();
        let shots = shots_at(&s, monk);
        if !followed {
            followed = !flying && now >= p + 30 && !shots.is_empty();
        } else if shots.is_empty() {
            let back = s.projectiles().iter().any(|q| q.deflected);
            return (m - hp(&s, monk), k - hp(&s, knight), back);
        }
        flying = !shots.is_empty();
    }
    panic!("the scene drifted: no Ice Wizard shot at the Monk followed to its end by P + 80");
}

/// combat.DEFLECT_CATCH = client16635_flight_height (client 16.635, parity r101, the oracle's il-mg-16635 set: 5 of 5
/// Ice Wizard shots caught, 50 of 143 off the Monk, nothing off his neighbours, 143 off the wizard): the deflecting
/// Monk's area catches an Ice Wizard's splash shot; he loses hp to it, the Knight beside him none, and it goes back.
/// Under flat_edge (the vacuity check) it lands on him and splashes the Knight, and nothing goes back. Plant:
/// deflect_catch_on_ground.
#[test]
fn under_client16635_flight_height_a_splash_shot_is_caught_and_lands_on_him_alone() {
    let (monk_old, knight_old, back_old) = splash_end(DeflectCatch::FlatEdge);
    assert!(monk_old > 0 && knight_old > 0, "flat_edge: the shot lands on him and splashes the Knight ({monk_old}, {knight_old})");
    assert!(!back_old, "flat_edge: a splash shot goes nowhere");
    let (monk_new, knight_new, back_new) = splash_end(DeflectCatch::Client16635FlightHeight);
    assert!(monk_new > 0, "client16635_flight_height: he loses hp to the caught shot");
    assert_eq!(knight_new, 0, "client16635_flight_height: the caught shot splashes nobody");
    assert!(back_new, "client16635_flight_height: the caught shot goes back");
}

/// combat.DEFLECT_CATCH ships at flat_edge (client16635_flight_height is a candidate until scored).
#[test]
fn the_shipped_arm_is_flat_edge() {
    assert_eq!(Calib::shipped().deflect_catch, DeflectCatch::FlatEdge);
}
