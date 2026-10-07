//! THE EVO GIANT SNOWBALL (card.rs `SpellShape::CaptureRoll`, `CaptureRollDef`; spell.rs `capture_roll`), against
//! client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-Snowball-evo-s0; Oracle's sp-snow-* lone-unit scenes, the ball landing at (9500, 18500) on
//! red's half; R the tick the ball stands on the tap, its flight's arrival):
//!   - the ball rolls 300 a tick along the caster's forward, 4000 in all (its end point on R + 14);
//!   - on R + 2 it strikes every enemy within reach, 179 (70 at level 1), and takes every troop it struck;
//!   - on R + 3 .. R + 7 a captive steps straight at the ball's point by its distance to the ball at the capture times
//!     .0367, .0735, .107, .130, .146 (a Knight 1601 away: 59, 118, 172, 209, 234), and from R + 8 it is on the ball's
//!     point every tick;
//!   - on R + 15 a lone captive stands on the ball's end point and a pair 150 either side of it along the roll, the
//!     first made ahead; they walk from R + 16 at 65 % for the release's 3000 ms (-35 %): a Knight (hit on R + 2, so
//!     the hit's own slow was gone by R + 63) walked 39 a tick through R + 75 and 59 from R + 76.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_snowball`):
//!   - capture_snaps_at_once -> `a_captive_is_dragged_at_the_ball_then_rides_it` red;
//!   - capture_never_released -> `a_lone_captive_is_let_go_on_the_end_point_and_walks_slowed` red (its slow past
//!     R + 63). `a_pair_is_let_go_150_either_side_the_first_made_ahead` stays green under it: without the release the
//!     pair also ends 150 either side of the end point on R + 15, so that test cannot see the release.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, CaptureDragFacing, CaptureRoute};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const TAP: (i32, i32) = (9500, 18500);

/// A battle whose Blue deck is the Snowball, evolved, and a Knight; both sides at level 11, past the opening lockout.
fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Snowball".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// One frame: each red unit's point and hp (None once gone), and the ball's point and age (None before it lands and
/// after it is gone).
struct Frame {
    units: Vec<Option<(Vec2, i32)>>,
    ball: Option<(Vec2, u32)>,
}

/// Two basic plays far away, red Knights put at `offsets` from the tap (and held there until the ball takes them), the
/// evolved play on the tap, and `frames` frames from the play's tick. Returns the Knights' ids and the frames.
fn scene(offsets: &[(i32, i32)], frames: usize) -> (Vec<EntityId>, Vec<Frame>) {
    let mut s = battle();
    for _ in 0..2 {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "Snowball", n(3000, 28000)).expect("a basic play");
        for _ in 0..30 {
            s.tick();
        }
    }
    let homes: Vec<Vec2> = offsets.iter().map(|o| n(TAP.0 + o.0, TAP.1 + o.1)).collect();
    let ids: Vec<EntityId> = homes.iter().map(|p| s.scenario_spawn_now(Team::Red, "Knight", *p, None).expect("a red Knight")).collect();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "Snowball", n(TAP.0, TAP.1)).expect("the evolved play");
    let mut out = Vec::new();
    let mut taken = false;
    for _ in 0..frames {
        if !taken {
            for (id, p) in ids.iter().zip(&homes) {
                assert!(s.debug_set_pos(*id, *p));
            }
        }
        s.tick();
        let ball = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::CaptureRoll { pos, age, .. } => Some((*pos, *age)),
            _ => None,
        });
        taken |= ball.is_some_and(|b| b.1 >= 1);
        out.push(Frame { units: ids.iter().map(|id| s.entity(*id).map(|e| (e.pos, e.hp))).collect(), ball });
    }
    (ids, out)
}

/// The frame of the ball's first point (R).
fn landing(f: &[Frame]) -> usize {
    f.iter().position(|x| x.ball.is_some_and(|b| b.1 == 0)).expect("the ball lands")
}

#[test]
fn the_third_play_is_the_evolved_snowball() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "Snowball").expect("the Snowball in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for _ in 0..2 {
        assert_eq!(next(&s), "Snowball", "a basic play");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "Snowball", n(3000, 28000)).expect("the play");
        for _ in 0..30 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "Snowball_EV1", "the third play is the form");
}

#[test]
fn the_ball_rolls_300_a_tick_and_strikes_and_takes_on_its_third_tick() {
    let (_, f) = scene(&[(0, -1000)], 60);
    let r = landing(&f);
    assert_eq!(f[r].ball.unwrap().0, n(TAP.0, TAP.1), "its first point is the tap");
    for k in 1..=14 {
        assert_eq!(f[r + k].ball.unwrap().0, n(TAP.0, TAP.1 + (300 * k as i32).min(4000)), "300 a tick to 4000, R + {k}");
    }
    let hp = |k: usize| f[k].units[0].unwrap().1;
    assert_eq!(hp(r + 1), hp(r - 1), "nothing before R + 2");
    assert_eq!(hp(r + 1) - hp(r + 2), 179, "the hit on R + 2 (70 at level 1)");
    assert!((r + 3..r + 16).all(|k| hp(k) == hp(r + 2)), "no other hit");
}

#[test]
fn a_captive_is_dragged_at_the_ball_then_rides_it() {
    let (_, f) = scene(&[(0, -1000)], 60);
    let r = landing(&f);
    let at = |k: usize| f[k].units[0].unwrap().0;
    let ball = |k: usize| f[k].ball.unwrap().0;
    let d0 = at(r + 2).dist(ball(r + 2)) / K;
    for (k, share) in [367, 735, 1070, 1300, 1460].into_iter().enumerate() {
        let t = r + 3 + k;
        let step = at(t).dist(at(t - 1)) / K;
        let want = d0 * share / 10_000;
        assert!((step - want).abs() <= 2, "R + {}: a step of {step} for {want} ({d0} away at the capture)", 3 + k);
        // straight at the ball's point this tick
        let (to, was) = (ball(t), at(t - 1));
        let cross = ((at(t).x - was.x) / K) as i64 * ((to.y - was.y) / K) as i64 - ((at(t).y - was.y) / K) as i64 * ((to.x - was.x) / K) as i64;
        assert!(cross.abs() <= (step as i64 + 1) * (to.dist(was) / K) as i64 / 50, "R + {}: straight at the ball", 3 + k);
    }
    for k in 8..=14 {
        assert_eq!(at(r + k), ball(r + k), "on the ball's point, R + {k}");
    }
}

#[test]
fn a_lone_captive_is_let_go_on_the_end_point_and_walks_slowed() {
    let (_, f) = scene(&[(0, -1000)], 120);
    let r = landing(&f);
    let at = |k: usize| f[k].units[0].unwrap().0;
    let end = n(TAP.0, TAP.1 + 4000);
    assert!(f[r + 15].ball.is_none(), "the ball gone on R + 15");
    assert_eq!(at(r + 15), end, "on the ball's end point on R + 15");
    let step = |k: usize| at(r + k).dist(at(r + k - 1)) / K;
    for k in (17..=22).chain(70..=75) {
        assert!((37..=40).contains(&step(k)), "walking at 65 % of 60 on R + {k} (the release's buff): {}", step(k));
    }
    for k in 77..=80 {
        assert!((55..=62).contains(&step(k)), "back to its own speed on R + {k}: {}", step(k));
    }
}

#[test]
fn a_pair_is_let_go_150_either_side_the_first_made_ahead() {
    let (_, f) = scene(&[(0, -1000), (1000, 0)], 40);
    let r = landing(&f);
    let end = n(TAP.0, TAP.1 + 4000);
    assert_eq!(f[r + 15].units[0].unwrap().0, end.add(n(0, 150)), "the first made ahead");
    assert_eq!(f[r + 15].units[1].unwrap().0, end.add(n(0, -150)), "the second behind");
}

/// movement.CAPTURE_DRAG_FACING (client 15.535.29: 52 of 52 drag frames of 9 captives face the ball, the walk facing 7):
/// a red Knight held 1,600 to the side of the ball's path until it is taken; per drag tick (R + 3 .. R + 7), its facing
/// and the unit vector, 256 long, from its point to the ball's.
fn drag_facings(arm: CaptureDragFacing) -> Vec<(Vec2, (i32, i32))> {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Snowball".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.capture_drag_facing = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    for _ in 0..2 {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "Snowball", n(3000, 28000)).expect("a basic play");
        for _ in 0..30 {
            s.tick();
        }
    }
    let home = n(TAP.0 + 1600, TAP.1 + 600);
    let k = s.scenario_spawn_now(Team::Red, "Knight", home, None).expect("a red Knight");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "Snowball", n(TAP.0, TAP.1)).expect("the evolved play");
    let (mut out, mut taken) = (Vec::new(), false);
    for _ in 0..40 {
        if !taken {
            assert!(s.debug_set_pos(k, home));
        }
        s.tick();
        let ball = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::CaptureRoll { pos, age, .. } => Some((*pos, *age)),
            _ => None,
        });
        taken |= ball.is_some_and(|b| b.1 >= 1);
        if let Some((bp, _)) = ball.filter(|b| (3..=7).contains(&b.1)) {
            let e = s.entity(k).expect("the Knight");
            out.push((e.facing, ((bp.x - e.pos.x) / K, (bp.y - e.pos.y) / K)));
        }
    }
    assert_eq!(out.len(), 5, "the scene drifted: not five drag ticks ({})", out.len());
    out
}

/// Whether facing `f` (256 long) points along (dx, dy) within 3 per 256 on each axis.
fn faces(f: Vec2, d: (i32, i32)) -> bool {
    let n = royalesim::fixed::isqrt(i64::from(d.0) * i64::from(d.0) + i64::from(d.1) * i64::from(d.1)).max(1);
    let (ux, uy) = (i64::from(d.0) * 256 / n, i64::from(d.1) * 256 / n);
    (i64::from(f.x) - ux).abs() <= 3 && (i64::from(f.y) - uy).abs() <= 3
}

/// Plant: drag_keeps_walk_facing.
#[test]
fn a_dragged_captive_faces_the_ball_under_client15535_faces_ball() {
    let new = drag_facings(CaptureDragFacing::Client15535FacesBall);
    assert!(new.iter().all(|&(f, d)| faces(f, d)), "client15535_faces_ball: a drag tick's facing is not toward the ball: {new:?}");
    // NOT VACUOUS: walk_facing keeps a facing that is not toward the ball.
    let old = drag_facings(CaptureDragFacing::WalkFacing);
    assert!(old.iter().any(|&(f, d)| !faces(f, d)), "walk_facing: the facing pointed at the ball anyway: {old:?}");
}

/// spells.CAPTURE_ROUTE (item 291; client 15.535.29: 0 of 60 ride frames held a route, the 2 captives holding one lost it
/// on the join frame): a red Knight put down 1,600 beside the ball's path 60 ticks before the play (walking, with a route)
/// and held there until it is taken; its route's length on the ball's age 1 (before the capture) and on its ride (ages 8 to
/// 14).
fn ride_routes(arm: CaptureRoute) -> (usize, Vec<usize>) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Snowball".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.capture_route = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let home = n(TAP.0 + 1600, TAP.1 + 600);
    let k = s.scenario_spawn_now(Team::Red, "Knight", home, None).expect("a red Knight");
    for _ in 0..2 {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "Snowball", n(3000, 28000)).expect("a basic play");
        for _ in 0..30 {
            assert!(s.debug_set_pos(k, home));
            s.tick();
        }
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "Snowball", n(TAP.0, TAP.1)).expect("the evolved play");
    let (mut before, mut ride, mut taken) = (None, Vec::new(), false);
    for _ in 0..40 {
        if !taken {
            assert!(s.debug_set_pos(k, home));
        }
        s.tick();
        let age = s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::CaptureRoll { age, .. } => Some(*age),
            _ => None,
        });
        taken |= age.is_some_and(|a| a >= 1);
        let len = s.entity(k).expect("the Knight").route.len();
        match age {
            Some(1) => before = Some(len),
            Some(a) if (8..=14).contains(&a) => ride.push(len),
            _ => {}
        }
    }
    (before.expect("the scene drifted: the ball never reached age 1"), ride)
}

/// Plant: capture_route_kept.
#[test]
fn a_captive_drops_its_route_when_it_joins_the_ball_under_client15535_dropped_at_join() {
    let (before, ride) = ride_routes(CaptureRoute::Client15535DroppedAtJoin);
    // NOT VACUOUS: the Knight walked a route before the capture.
    assert!(before > 0, "the scene drifted: the Knight held no route before the capture");
    assert!(ride.len() == 7 && ride.iter().all(|l| *l == 0), "client15535_dropped_at_join: a route on the ride: {ride:?}");
    let (_, ride) = ride_routes(CaptureRoute::Kept);
    assert!(ride.iter().any(|l| *l > 0), "kept: no route on the ride: {ride:?}");
}
