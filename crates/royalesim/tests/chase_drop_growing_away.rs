//! targeting.CHASE_DROP_WALKING_AWAY = client15535_growing_away, the third reading (target.rs `walks_away`, `scan_with`
//! `after_drop`; state.rs `chase_pass_start`, entity.rs `chase_last_pos`), with targeting.CHASE_DROP_MEASURE =
//! client15535_lane_dy.
//!
//! THE READING (client 15.535.29): a troop past the chase-drop limit walks away when it walked as the tick's Target
//! phase began, its own step since the last Target phase points away from the chaser in y, and its |dy| grew since
//! then. The edge lets a held troop go only then; the drop tick's own rescan passes over every other troop walking away
//! past the limit, and the troop let go stays barred while past it (as any_target); any other rescan takes them.
//! acq-HogRider-away-5800 t202: a Knight let go of a Hog Rider on the tick the Hog entered its attack on the tower, in
//! its own turn earlier in the sequential pass. sweep-RoyalHogs t353: the Knight letting Hog 7 go passed over Hog 8,
//! walking away 6,071 -> 6,245. ub-sd14-a2 t307: a Knight's first pick took a Knight walking away 5,665 -> 5,725.
//!
//! Every runner stands on Red's half (no river leap): Blue troops running north, for Red's left princess tower (the first
//! scene) or, with that tower destroyed, for the king tower; a Red Knight chasing them.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! chase_drop_growing_away`):
//!   chase_walk_read_live          the walking mark read as the target stands: `a_target_entering_its_attack_in_the_
//!                                 pass_is_let_go` red.
//!   chase_growth_unread           growth not read: `the_drop_ticks_rescan_takes_a_troop_whose_distance_fell` red.
//!   chase_pass_over_every_rescan  every rescan passes over: `a_first_pick_takes_a_troop_walking_away` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, ChaseDropMeasure, ChaseDropRange, ChaseDropWalkingAway, TickOrder};
use royalesim::{EntityId, Team};

fn with_arm(arm: ChaseDropWalkingAway) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.chase_drop_range = ChaseDropRange::ClientSightMinus1000;
    cfg.calib.chase_drop_measure = ChaseDropMeasure::Client15535LaneDy;
    cfg.calib.chase_drop_walking_away = arm;
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    // The file's scenes are client 15.535.29's, measured under drop_tick's rescans (no pass-over on the drop tick); the
    // shipped client16402_receding_or_behind_every_rescan (r63 R10, client 16.402) passes over there too:
    // tests/chase_rescan_pass_over.rs holds it.
    cfg.calib.chase_rescan_pass_over = royalesim::state::ChaseRescanPassOver::DropTick;
    cfg
}

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Red's left princess tower, native.
fn red_left_tower(s: &BattleState) -> (i32, i32) {
    let t = s.entities().find(|e| e.team == Team::Red && e.card == "PrincessTower" && e.pos.x < 9000 * K).expect("Red's left princess tower");
    (t.pos.x / K, t.pos.y / K)
}

/// A battle at `arm` past the deploy lockout with Red's left princess tower destroyed, so Blue troops in that lane walk
/// on for the king tower, out of any tower's reach for the scenes' ticks; its point, native.
fn without_red_left_tower(arm: ChaseDropWalkingAway) -> (BattleState, (i32, i32)) {
    let mut s = BattleState::new(0, with_arm(arm));
    past_deploy_lockout(&mut s);
    let at = red_left_tower(&s);
    let t = s.entities().find(|e| e.team == Team::Red && e.card == "PrincessTower" && e.pos.x < 9000 * K).expect("the tower").id;
    assert!(s.debug_set_hp(t, 0), "could not destroy the tower");
    s.tick();
    assert!(s.entity(t).is_none(), "the scene drifted: the tower stands");
    (s, at)
}

/// A Knight's chase-drop limit on `card`, native: SightRange + both radii - 1000.
fn limit_on(s: &BattleState, card: &str) -> i32 {
    let cards = &s.config().cards;
    let k = cards.get(cards.index("Knight").expect("Knight loads"));
    let c = cards.get(cards.index(card).expect("the card loads"));
    (k.sight_range + k.collision_radius + c.collision_radius) / K - 1000
}

fn pos(s: &BattleState, id: EntityId) -> (i32, i32) {
    let p = s.entity(id).expect("the unit stands").pos;
    (p.x / K, p.y / K)
}

/// THE SEQUENTIAL PASS: a Blue Hog Rider 4,500 short of Red's tower, created first, and a Red Knight 2,000 behind it.
/// Run once to find the tick T on which the Hog enters its attack on the tower; then again with the Knight put back on
/// T's eve to the Hog's |dy| + the limit + 50: whether the Knight still holds the Hog after T.
fn holds_the_hog_entering_its_attack(arm: ChaseDropWalkingAway) -> bool {
    let run = |put_back: Option<u32>| {
        let mut s = BattleState::new(0, with_arm(arm));
        past_deploy_lockout(&mut s);
        let (tx, ty) = red_left_tower(&s);
        let hog = s.scenario_spawn_now(Team::Blue, "HogRider", n(tx, ty - 4500), None).expect("the Hog Rider");
        let knight = s.scenario_spawn_now(Team::Red, "Knight", n(tx, ty - 6500), None).expect("the Knight");
        let lim = limit_on(&s, "HogRider");
        for tick in 1..=80u32 {
            if put_back == Some(tick) {
                assert_eq!(s.entity(knight).unwrap().target, Some(hog), "the scene drifted: the Knight does not hold the Hog on T's eve");
                assert_eq!(s.entity(hog).unwrap().attack_phase, AttackPhase::Idle, "the scene drifted: the Hog attacks on T's eve");
                let (hx, hy) = pos(&s, hog);
                assert!(s.debug_set_pos(knight, n(hx, hy - lim - 50)), "the Knight is gone");
            }
            s.tick();
            if s.entity(hog).expect("the Hog stands").attack_phase != AttackPhase::Idle {
                return (tick, s.entity(knight).expect("the Knight stands").target == Some(hog));
            }
        }
        panic!("the scene drifted: the Hog never reached its attack");
    };
    let (t, _) = run(None);
    let (t2, held) = run(Some(t));
    assert_eq!(t2, t, "the scene drifted: the Knight's move changed the Hog's attack tick");
    held
}

/// Plant: chase_walk_read_live.
#[test]
fn a_target_entering_its_attack_in_the_pass_is_let_go() {
    assert!(!holds_the_hog_entering_its_attack(ChaseDropWalkingAway::Client15535GrowingAway), "client15535_growing_away: the Knight kept the Hog");
    // The control: the second reading reads the Hog as it stands, in its attack, so it is not walking away.
    assert!(holds_the_hog_entering_its_attack(ChaseDropWalkingAway::ClientWalkingAway), "client_walking_away: the Knight let the Hog go");
}

/// THE DROP TICK'S RESCAN: a Blue Hog Rider 2,000 ahead of a Red Knight on Red's half, and a second Blue troop `other`
/// (a Hog Rider, faster than the Knight's 80 a tick, or a Giant, slower) 1,000 across and past the Knight's limit on it,
/// all walking on for Red's king tower; on the sixth tick the Hog is put past the limit, |dy| the limit + 50. Whether the
/// Knight takes `other` on that tick; asserted: the Hog was let go, and `other` stood past the limit in round sight,
/// walking away, its |dy| over the drop tick's Target phase and the one before growing or falling as `grows` says.
fn drop_tick_takes(arm: ChaseDropWalkingAway, other: &str, grows: bool) -> bool {
    let (mut s, (tx, _)) = without_red_left_tower(arm);
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", n(tx, 19600), None).expect("the Hog Rider");
    let lim_o = limit_on(&s, other);
    let o = s.scenario_spawn_now(Team::Blue, other, n(tx + 1000, 17600 + lim_o + 300), None).expect("the other troop");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(tx, 17600), None).expect("the Knight");
    // The Knight is put 80 further north before each tick (its steps as the Target phases read them): faster than a
    // Giant, slower than a Hog Rider.
    for k in 1..=4 {
        assert!(s.debug_set_pos(knight, n(tx, 17600 + 80 * k)), "the Knight is gone");
        s.tick();
    }
    assert!(s.debug_set_pos(knight, n(tx, 17600 + 400)), "the Knight is gone");
    // The fifth tick's Target phase reads these; the sixth's (the drop tick's) the ones after it.
    let last = pos(&s, o).1 - pos(&s, knight).1;
    s.tick();
    assert_eq!(s.entity(knight).unwrap().target, Some(hog), "the scene drifted: the Knight does not hold the Hog");
    assert!(s.debug_set_pos(knight, n(tx, 17600 + 480)), "the Knight is gone");
    let (kx, ky) = pos(&s, knight);
    let (hx, _) = pos(&s, hog);
    let (ox, oy) = pos(&s, o);
    let now = oy - ky;
    let (dx, dy) = (i64::from(ox - kx), i64::from(now));
    assert!(now > lim_o && dx * dx + dy * dy < i64::from(lim_o + 1000).pow(2), "the scene drifted: {other} at {now} is not past the limit in round sight");
    assert_eq!(now > last, grows, "the scene drifted: {other}'s |dy| went {last} -> {now}");
    assert!(s.debug_set_pos(hog, n(hx, ky + limit_on(&s, "HogRider") + 50)), "the Hog is gone");
    s.tick();
    let k = s.entity(knight).expect("the Knight stands");
    assert_ne!(k.target, Some(hog), "the scene drifted: the Knight kept the Hog past the limit");
    assert!(s.entity(o).unwrap().attack_phase == AttackPhase::Idle, "the scene drifted: {other} attacks");
    k.target == Some(o)
}

#[test]
fn the_drop_ticks_rescan_passes_over_a_troop_walking_away() {
    assert!(!drop_tick_takes(ChaseDropWalkingAway::Client15535GrowingAway, "HogRider", true), "client15535_growing_away: took the other Hog");
    assert!(drop_tick_takes(ChaseDropWalkingAway::AnyTarget, "HogRider", true), "any_target: passed over the other Hog (vacuous otherwise)");
}

/// Plant: chase_growth_unread.
#[test]
fn the_drop_ticks_rescan_takes_a_troop_whose_distance_fell() {
    assert!(drop_tick_takes(ChaseDropWalkingAway::Client15535GrowingAway, "Giant", false), "client15535_growing_away: passed over the Giant");
    // The control: the second reading passes over it, by its facing alone.
    assert!(!drop_tick_takes(ChaseDropWalkingAway::ClientWalkingAway, "Giant", false), "client_walking_away: took the Giant");
}

/// A FIRST PICK: a Blue Hog Rider running north on Red's half for three ticks, then a Red Knight put down behind it, |dy|
/// the limit + 200. Whether the Knight's first decision takes it.
fn first_pick_takes(arm: ChaseDropWalkingAway) -> bool {
    let (mut s, (tx, _)) = without_red_left_tower(arm);
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", n(tx, 19000), None).expect("the Hog Rider");
    for _ in 0..3 {
        s.tick();
    }
    let (hx, hy) = pos(&s, hog);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(hx, hy - limit_on(&s, "HogRider") - 200), None).expect("the Knight");
    s.tick();
    assert_eq!(s.entity(hog).unwrap().attack_phase, AttackPhase::Idle, "the scene drifted: the Hog attacks");
    s.entity(knight).expect("the Knight stands").target == Some(hog)
}

/// Plant: chase_pass_over_every_rescan.
#[test]
fn a_first_pick_takes_a_troop_walking_away() {
    assert!(first_pick_takes(ChaseDropWalkingAway::Client15535GrowingAway), "client15535_growing_away: the first pick passed over the Hog");
    // The control: the second reading passes over it in every rescan.
    assert!(!first_pick_takes(ChaseDropWalkingAway::ClientWalkingAway), "client_walking_away: the first pick took the Hog");
}
