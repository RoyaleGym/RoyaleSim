//! spawner.SOUL_POINT_BASE: where in the tick a Skeleton King's area draws its copies (state.rs `soul_pass`,
//! `soul_after_move`), and so around which of his points.
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, the Skeleton King scenes: on a tick he walked, each copy
//! stands at the draw's offset from his post-move point (28 of 34), the engine's from his pre-move point.
//!
//! THE SCENE (tests/skeleton_king.rs's): the King put back on one point before every tick, so he takes one walking step
//! within each once his cast hold is over; his press; his first copy made on a tick he walked (the ones made in his cast
//! hold stand on his unmoved point under both arms). Both arms draw from one generator state, so the draw is the same.
//!
//! WHAT IS PINNED, and the plant that turns it red (soul_point_pre_move):
//!   1. that copy under client15535_post_move stands from the one under pre_move (the engine's) by the King's own step on
//!      its tick, and that step is not nothing.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, SoulPointBase};
use royalesim::Team;

const DECK: [&str; 8] = ["SkeletonKing", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (9000, 11000);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The point of the first copy made on a tick the King walked, and his point at the end of that tick (he stood on AT as
/// it began).
fn first_copy(arm: SoulPointBase) -> (Vec2, Vec2) {
    let mut cfg = config();
    cfg.calib.soul_point_base = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let king = s.scenario_spawn_now(Team::Blue, "SkeletonKing", n(AT), None).expect("the Skeleton King");
    for _ in 0..40 {
        assert!(s.debug_set_pos(king, n(AT)));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut seen = Vec::new();
    for _ in 0..80 {
        assert!(s.debug_set_pos(king, n(AT)));
        s.tick();
        let at = s.entity(king).expect("the King").pos;
        let mut fresh: Vec<(u32, Vec2)> = s.entities().filter(|e| e.team == Team::Blue && e.cloned && !seen.contains(&e.id)).map(|e| (e.team_seq, e.pos)).collect();
        seen.extend(s.entities().filter(|e| e.team == Team::Blue && e.cloned).map(|e| e.id));
        fresh.sort_by_key(|c| c.0);
        if at != n(AT) {
            if let Some(c) = fresh.first() {
                return (c.1, at);
            }
        }
    }
    panic!("the scene drifted: no copy made on a tick he walked within 80 ticks of the press");
}

/// Plant: soul_point_pre_move.
#[test]
fn his_copies_are_drawn_around_his_post_move_point_under_client15535_post_move() {
    let (old, king_old) = first_copy(SoulPointBase::PreMove);
    let (new, king_new) = first_copy(SoulPointBase::Client15535PostMove);
    assert_eq!(king_new, king_old, "the scene drifted: the King's point parted between the arms");
    let step = king_new.sub(n(AT));
    // NOT VACUOUS: he walked on the copy's tick, so the two points are apart.
    assert!(step != Vec2::new(0, 0), "the scene drifted: the King did not walk on the copy's tick");
    assert_eq!(new.sub(old), step, "client15535_post_move: the copy stands {:?} from pre_move's, not the King's step {step:?}", new.sub(old));
}

#[test]
fn the_shipped_value_is_pre_move() {
    assert_eq!(Calib::shipped().soul_point_base, SoulPointBase::PreMove);
}
