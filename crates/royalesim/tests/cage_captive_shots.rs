//! combat.CAGE_CAPTIVE_SHOTS: when an Evo Goblin Cage's drag moves its captive against the tick's projectiles, and when
//! the captive's hide (invisible, no hit lands) takes it (state.rs `cage_pass`, `phase_projectile`).
//!
//! THE LAW, measured on client 15.535.29 (Oracle's sp-grab-*: a cage at (14500, 11500), 5000 from Blue's right princess
//! tower, a red troop walked in down x 14730): the tower's arrow at the dragged captive landed, on the Knight on the
//! drag's fifth step (G + 15), on a Giant on the fourth, on a Mini P.E.K.K.A. and four Skeleton probes on the snap tick
//! (G + 16); the tower held the captive as its target through G + 17 and let it go on G + 18. So the cages' pass runs
//! before the shots step, and the hide lands on the tick after the snap.
//!
//! WHAT IS PINNED, on that scene (a red Knight played at (14500, 17500)), G its grab's tick (its last walk step):
//!   1. client15535_before_shots_hidden_after_snap: the tower's arrow (109 at level 11) lands on the Knight on G + 15, the
//!      drag's fifth step, its first loss, as the client's did (drawn after the shots step, it lands on G + 16: the
//!      plant's tick); after_shots_hidden_on_snap: nothing lands on it before the cage's first hit on G + 20;
//!   2. client15535_before_shots_hidden_after_snap: the tower holds the Knight as its target on G + 17 and not on G + 18;
//!      after_shots_hidden_on_snap: not on G + 17.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! cage_captive_shots`):
//!   * `cage_drag_after_shots` -- the new arm's cages drag after the shots step: (1) goes red;
//!   * `cage_hide_on_snap` -- the new arm hides the captive on the snap tick: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CageCaptiveShots};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The cage's point: 5000 from Blue's right princess tower (14500, 6500).
const CAGE: (i32, i32) = (14500, 11500);

/// What `scene` reads: the Knight's points, hitpoints and the tower's target by tick, and its grab's tick G.
type Scene = (Vec<(i32, i32)>, Vec<i32>, Vec<bool>, usize);

/// The Knight's points, hitpoints and the tower's target by tick, from the tick it is put down, and the index of its
/// grab's tick G (its last walk step: the first whose point the next tick keeps, once it has walked).
fn scene(arm: CageCaptiveShots) -> Scene {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["GoblinCage".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.cage_captive_shots = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "GoblinCage_EV1", n(CAGE.0, CAGE.1), None).expect("the cage");
    for _ in 0..25 {
        s.tick();
    }
    let tower: EntityId = s
        .entities()
        .find(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower && e.pos.x == 14500 * K)
        .expect("Blue's right princess tower")
        .id;
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s.deploy(Team::Red, "Knight", n(14500, 17500)).expect("the red Knight's play");
    let mut knight = None;
    for _ in 0..40 {
        s.tick();
        if let Some(e) = find_live(&s, Team::Red, "Knight").first() {
            knight = Some(e.id);
            break;
        }
    }
    let knight: EntityId = knight.expect("the red Knight");
    let (mut pts, mut hps, mut held) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..160 {
        s.tick();
        let k = s.entity(knight).expect("the Knight");
        pts.push((k.pos.x / K, k.pos.y / K));
        hps.push(k.hp);
        held.push(s.entity(tower).expect("the tower").target == Some(knight));
    }
    let g = (2..pts.len() - 1).find(|&k| pts[k - 1] != pts[k - 2] && pts[k] != pts[k - 1] && pts[k + 1] == pts[k]).expect("the Knight was never grabbed");
    assert_eq!(pts[g + 16], CAGE, "the scene drifted: the Knight is not on the cage's point on G + 16: {:?}", &pts[g..g + 18]);
    (pts, hps, held, g)
}

#[test]
fn a_tower_shot_lands_on_the_dragged_captive() {
    let (_, hps, _, g) = scene(CageCaptiveShots::Client15535BeforeShotsHiddenAfterSnap);
    let top = hps[g];
    let first = (g..g + 20).find(|&k| hps[k] < top).expect("client15535_before_shots_hidden_after_snap: no shot landed before the cage's first hit");
    assert_eq!(first - g, 15, "client15535_before_shots_hidden_after_snap: the first loss, as G + n: {:?}", &hps[g..g + 20]);
    assert_eq!(top - hps[first], 109, "the tower's arrow (level 11)");
    let (_, old, _, h) = scene(CageCaptiveShots::AfterShotsHiddenOnSnap);
    assert!(old[h..h + 20].iter().all(|x| *x == old[h]), "after_shots_hidden_on_snap: a hit landed before the cage's: {:?}", &old[h..h + 20]);
}

#[test]
fn the_tower_holds_the_captive_through_the_tick_after_the_snap() {
    let (_, _, held, g) = scene(CageCaptiveShots::Client15535BeforeShotsHiddenAfterSnap);
    assert!(held[g + 16], "the scene drifted: the tower does not hold the Knight on the snap tick");
    assert_eq!((held[g + 17], held[g + 18]), (true, false), "client15535_before_shots_hidden_after_snap: the tower's hold on G + 17 and G + 18");
    let (_, _, old, h) = scene(CageCaptiveShots::AfterShotsHiddenOnSnap);
    assert!(!old[h + 17], "after_shots_hidden_on_snap: the tower still held the hidden Knight on G + 17");
}
