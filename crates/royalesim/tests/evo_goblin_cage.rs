//! THE EVO GOBLIN CAGE (tools/extract_cards.py `cage_block`; card.rs `CageDef`, CAGE_GRAB_LAG_TICKS, CAGE_DRAG_EXTRA,
//! CAGE_RELEASE_HOLD_TICKS; state.rs EvoBoard `cages`, `cage_pass`, the release in `phase_reap`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; sp-form-GoblinCage-evo-s0 and Oracle's sp-f2-cage*, sp-grab-*): a troop is
//! grabbed 3 ticks after its moved point first comes within 3000 plus its radius of the cage's centre (13 grabs); it
//! stands 10 ticks, steps 5 times toward the cage's point and is set on it on G + 16; it loses 366 (143 at level 1) on
//! G + 20 and every 20 ticks; the cage's death lets it (a Golem) go on the cage's point, standing that tick and the
//! next; after a
//! captive's death the cage takes its next target 6 ticks on, and a Skeleton in reach then made its last step 10 ticks
//! after the death.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_goblin_cage`):
//!   - cage_never -> every test red;
//!   - cage_grab_at_once -> `a_knight_is_grabbed_3_ticks_after_it_comes_in_reach_held_dragged_and_hit_every_20_ticks`
//!     and `after_its_captives_death_the_next_troop_in_reach_is_grabbed_10_ticks_on` red;
//!   - cage_hits_never -> `a_knight_is_grabbed_3_ticks_after_it_comes_in_reach_held_dragged_and_hit_every_20_ticks` and
//!     `after_its_captives_death_the_next_troop_in_reach_is_grabbed_10_ticks_on` red;
//!   - cage_takes_on_free -> `after_its_captives_death_the_next_troop_in_reach_is_grabbed_10_ticks_on` red;
//!   - cage_release_at_once -> `the_cages_death_lets_its_captive_go_on_its_point_standing_a_tick` and
//!     `the_cages_death_lets_a_knight_go_on_its_point_unpushed_by_its_brawler` red;
//!   - cage_release_pushed -> the same two red;
//!   - cage_release_stunned -> `a_captive_let_go_takes_its_target_on_the_next_tick_under_client15535_scans_while_held`
//!     red.
//!
//! combat.CAGE_RELEASE_SCAN = client15535_scans_while_held (client 15.535.29, 4 of 4 captives let go): the captive takes
//! its target on K + 1, standing, and walks on K + 2; the engine's stun scanned nothing until K + 3, and on K + 2 only the
//! Brawler's push moved it (which the two release tests above read as its walk).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CageReleaseScan};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The cage's point: clear of blue's king's reach once blue's princess towers are down.
const CAGE: (i32, i32) = (9500, 12500);

/// A point's squared distance from the cage's centre (millitiles squared: the engine is integer-only).
fn d(p: (i32, i32)) -> i64 {
    let (dx, dy) = (i64::from(p.0 - CAGE.0), i64::from(p.1 - CAGE.1));
    dx * dx + dy * dy
}

/// The cage's grab circle, squared: 3000 plus a radius of 500 (the Knight's and the Skeleton's).
const GRAB2: i64 = 3500 * 3500;

/// Blue's princess towers down (no crown tower reaches the scene) and the form put down on CAGE: the battle and the
/// cage.
fn battle() -> (BattleState, EntityId) {
    battle_arm(CageReleaseScan::Stunned)
}

/// `battle` under combat.CAGE_RELEASE_SCAN = `arm`.
fn battle_arm(arm: CageReleaseScan) -> (BattleState, EntityId) {
    let mut cfg: BattleConfig = config();
    cfg.calib.cage_release_scan = arm;
    cfg.decks = [vec!["GoblinCage".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.spawn_unit(Team::Blue, "GoblinCage_EV1", n(CAGE.0, CAGE.1), None).expect("the cage");
    s.tick();
    let cage = find_live(&s, Team::Blue, "GoblinCage_EV1").first().expect("the cage").id;
    (s, cage)
}

fn point(s: &BattleState, id: EntityId) -> Option<(i32, i32)> {
    s.entity(id).map(|e| (e.pos.x / K, e.pos.y / K))
}

/// The grab scene (`grabbed`): the battle, the cage, the Knight, the Knight's points and hitpoints by tick, and the
/// index of the grab's tick.
type Grab = (BattleState, EntityId, EntityId, Vec<(i32, i32)>, Vec<i32>, usize);

/// A red Knight held 3700 to the cage's right (out of its 3500 reach) until the cage is up, then let walk in for 80
/// ticks (its fifth hit would kill it): (the battle, the cage, the Knight, its points and hitpoints from the tick before
/// it walks, the index of the grab's tick G, the first whose point the next tick keeps).
fn grabbed() -> Grab {
    grabbed_arm(CageReleaseScan::Stunned)
}

/// `grabbed` under combat.CAGE_RELEASE_SCAN = `arm`.
fn grabbed_arm(arm: CageReleaseScan) -> Grab {
    let (mut s, cage) = battle_arm(arm);
    let far = n(CAGE.0 + 3700, CAGE.1);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", far, None).expect("a red Knight");
    for _ in 0..40 {
        assert!(s.debug_set_pos(knight, far));
        s.tick();
    }
    let hp = |s: &BattleState| s.entity(knight).expect("the Knight").hp;
    let mut pts = vec![point(&s, knight).expect("the Knight")];
    let mut hps = vec![hp(&s)];
    for _ in 0..80 {
        s.tick();
        pts.push(point(&s, knight).expect("the Knight"));
        hps.push(hp(&s));
    }
    let g = (1..pts.len() - 1).find(|&k| pts[k] != pts[k - 1] && pts[k + 1] == pts[k]).expect("the Knight stopped");
    (s, cage, knight, pts, hps, g)
}

#[test]
fn a_knight_is_grabbed_3_ticks_after_it_comes_in_reach_held_dragged_and_hit_every_20_ticks() {
    let (_, _, _, pts, hps, g) = grabbed();
    // First within 3000 + 500 of the centre on G - 3; walks on through G.
    assert!(g >= 4 && d(pts[g - 3]) <= GRAB2 && d(pts[g - 4]) > GRAB2, "grabbed on {g}: {:?}", &pts[..=g]);
    assert!((g - 2..=g).all(|k| pts[k] != pts[k - 1]), "walks on through G: {:?}", &pts[..=g]);
    // Stands through G + 10.
    assert!(pts[g..=g + 10].iter().all(|p| *p == pts[g]), "stands through G + 10: {:?}", &pts[g..g + 12]);
    // Five steps closer, then on the cage's point on G + 16, and there after.
    assert!((g + 11..=g + 15).all(|k| d(pts[k]) < d(pts[k - 1])), "the drag: {:?}", &pts[g + 10..g + 17]);
    assert!(pts[g + 16..=g + 45].iter().all(|p| *p == CAGE), "on the point from G + 16: {:?}", &pts[g + 14..g + 20]);
    // 366 on G + 20 and G + 40, nothing between.
    let top = hps[g];
    assert!(hps[..g + 20].iter().all(|h| *h == top), "no hit before G + 20: {:?}", &hps[..g + 22]);
    assert_eq!((hps[g + 20], hps[g + 39], hps[g + 40]), (top - 366, top - 366, top - 732), "the cage's hits: {:?}", &hps[g + 18..g + 42]);
}

#[test]
fn the_cages_death_lets_its_captive_go_on_its_point_standing_a_tick() {
    // The measured captive: a red Golem (Oracle's sp-f2-cagegolem-s0, sp-f2-cagefb-s0), held 4000 to the cage's right
    // until the cage is up, then let walk in; it is taken and set on the cage's point. The cage dies on K: the Golem
    // stands on the point on K and K + 1 and walks on K + 2; the Brawler comes out.
    let (mut s, cage) = battle();
    let far = n(CAGE.0 + 4000, CAGE.1);
    let golem = s.scenario_spawn_now(Team::Red, "Golem", far, None).expect("a red Golem");
    for _ in 0..40 {
        assert!(s.debug_set_pos(golem, far));
        s.tick();
    }
    let mut on = 0;
    for _ in 0..120 {
        s.tick();
        on = if point(&s, golem) == Some(CAGE) { on + 1 } else { 0 };
        if on == 4 {
            break;
        }
    }
    assert_eq!(on, 4, "caged on the point");
    let after = release(&mut s, cage, golem);
    assert_eq!((after[0], after[1]), (CAGE, CAGE), "stands on the point: {after:?}");
    assert_ne!(after[2], CAGE, "walks on K + 2: {after:?}");
    let brawlers = find_live(&s, Team::Blue, "GoblinCage_EV1_GoblinBrawler");
    assert!(brawlers.len() == 1 && brawlers[0].max_hp == 1080, "its Brawler");
}

/// The cage dies on K with `captive` set on its point: the captive's points after K, K + 1 and K + 2.
fn release(s: &mut BattleState, cage: EntityId, captive: EntityId) -> Vec<(i32, i32)> {
    assert!(s.debug_set_hp(cage, 0));
    let mut after = Vec::new();
    for _ in 0..3 {
        s.tick();
        after.push(point(s, captive).expect("the captive"));
    }
    assert!(s.entity(cage).is_none(), "the cage died");
    after
}

#[test]
fn the_cages_death_lets_a_knight_go_on_its_point_unpushed_by_its_brawler() {
    // Measured (Oracle's sp-f2-cageknfb-s0, a Fireball killing the cage): the Knight on the point on K and K + 1, though
    // the Brawler is put down on it, and walking from K + 2; the Brawler, deploying, slides 150 a tick off it toward its
    // own side.
    let (mut s, cage, knight, pts, _, _) = grabbed();
    assert_eq!(*pts.last().expect("points"), CAGE, "caged");
    let after = release(&mut s, cage, knight);
    assert_eq!((after[0], after[1]), (CAGE, CAGE), "stands on the point: {after:?}");
    assert_ne!(after[2], CAGE, "walks on K + 2: {after:?}");
    let b = find_live(&s, Team::Blue, "GoblinCage_EV1_GoblinBrawler");
    let at: Vec<(i32, i32)> = b.iter().map(|e| (e.pos.x / K, e.pos.y / K)).collect();
    assert!(at.len() == 1 && at[0].1 < CAGE.1 - 150, "its Brawler slid off toward blue's side: {at:?}");
}

#[test]
fn after_its_captives_death_the_next_troop_in_reach_is_grabbed_10_ticks_on() {
    // A red Skeleton walks in from 3600 and is taken; a second, held far off until then, is put beside the cage (2000
    // from its centre, attacking it). The cage's first hit kills the first (81 hitpoints) on D; the cage is freed on
    // D + 6 (300 ms), takes the second on D + 7, grabs it on D + 10, and its drag's first step is D + 21.
    let (mut s, _) = battle();
    let (fa, fb, side) = (n(CAGE.0 + 3600, CAGE.1), n(CAGE.0 - 6000, CAGE.1), n(CAGE.0 - 2000, CAGE.1));
    let a = s.scenario_spawn_now(Team::Red, "Skeleton", fa, None).expect("the first Skeleton");
    let b = s.scenario_spawn_now(Team::Red, "Skeleton", fb, None).expect("the second Skeleton");
    for _ in 0..40 {
        assert!(s.debug_set_pos(a, fa) && s.debug_set_pos(b, fb));
        s.tick();
    }
    let (mut taken, mut death, mut pts) = (false, None, Vec::new());
    for k in 0..90 {
        if !taken {
            assert!(s.debug_set_pos(b, fb));
        }
        s.tick();
        if !taken && point(&s, a).is_some_and(|p| d(p) <= GRAB2) {
            taken = true;
            assert!(s.debug_set_pos(b, side));
        }
        // The cage's hits kill the second too, after its drag: its points stop there.
        let Some(p) = point(&s, b) else { break };
        pts.push(p);
        if death.is_none() && s.entity(a).is_none() {
            death = Some(k);
        }
    }
    let dk = death.expect("the first Skeleton died");
    assert!(taken && pts[dk] == pts[dk - 1], "the second stood beside the cage by D: {:?}", &pts[dk - 2..=dk]);
    let first = (dk + 1..pts.len()).find(|&k| pts[k] != pts[k - 1]).expect("the second Skeleton moved");
    assert_eq!(first - dk, 21, "its drag's first step, from D: {:?}", &pts[dk - 1..(dk + 24).min(pts.len())]);
    assert!(d(pts[first]) < d(pts[first - 1]), "dragged toward the cage: {:?}", &pts[first - 1..=first]);
}

/// The Knight of `grabbed_arm(arm)` let go when the cage dies on K: its point and whether it holds a target on K, K + 1,
/// K + 2 and K + 3.
fn let_go(arm: CageReleaseScan) -> Vec<((i32, i32), bool)> {
    let (mut s, cage, knight, pts, _, _) = grabbed_arm(arm);
    assert_eq!(*pts.last().expect("points"), CAGE, "caged");
    assert!(s.debug_set_hp(cage, 0));
    let mut out = Vec::new();
    for _ in 0..4 {
        s.tick();
        let e = s.entity(knight).expect("the Knight");
        out.push(((e.pos.x / K, e.pos.y / K), e.target.is_some()));
    }
    assert!(s.entity(cage).is_none(), "the cage died");
    out
}

/// Plant: cage_release_stunned.
#[test]
fn a_captive_let_go_takes_its_target_on_the_next_tick_under_client15535_scans_while_held() {
    // NOT VACUOUS: the stun scans nothing through K + 2.
    let old = let_go(CageReleaseScan::Stunned);
    assert!(!old[1].1 && !old[2].1, "stunned: the Knight held a target before K + 3: {old:?}");
    let new = let_go(CageReleaseScan::Client15535ScansWhileHeld);
    assert!(new[1].1, "client15535_scans_while_held: no target on K + 1: {new:?}");
    assert_eq!((new[0].0, new[1].0), (CAGE, CAGE), "client15535_scans_while_held: not standing on K and K + 1: {new:?}");
    assert!(new[2].1 && new[2].0 != CAGE, "client15535_scans_while_held: not walking on K + 2: {new:?}");
}
