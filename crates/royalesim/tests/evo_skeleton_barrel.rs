//! THE EVO SKELETON BARREL (card.rs `BarrelDef`; state.rs `barrel_pass`, `phase_reap`'s death bomb), against client
//! 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-SkeletonBalloon-evo-s0):
//!   - the third play of an evolved Skeleton Barrel entry is the form (DarkElixirCost 2); it has 665 hitpoints (260 at
//!     level 1);
//!   - a hit took it from 665 to 448 (under 75 %) on t974, and its first seven Skeletons appeared on t987;
//!   - it was gone on t1033, and its next seven appeared on t1045;
//!   - each seven in a ring about 1,480 from a point beside the barrel.
//! Read off the table, not measured: the drop points' offsets ((-350, 450) and (350, 0) in the owner's frame) and the
//! drops' blow (no enemy stood under either).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_skeleton_barrel`):
//!   - barrel_drop_never -> `under_75_percent_it_drops_seven_skeletons_13_ticks_on` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const AT: (i32, i32) = (9500, 11500);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["SkeletonBalloon".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The form put at AT and held there, deployed; then `before(k)` before each of `frames` ticks may set its hp (Some)
/// or kill it; returns the barrel's id and, per frame, the blue Skeletons' points.
fn scene(frames: usize, mut before: impl FnMut(usize, &mut BattleState, EntityId)) -> (EntityId, Vec<Vec<Vec2>>, Vec<Option<i32>>) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "SkeletonBalloon_EV1", n(AT.0, AT.1), None).expect("the barrel");
    s.tick();
    let barrel = find_live(&s, Team::Blue, "SkeletonBalloon_EV1")[0].id;
    for _ in 0..25 {
        assert!(s.debug_set_pos(barrel, n(AT.0, AT.1)));
        s.tick();
    }
    let mut skel = Vec::new();
    let mut hp = Vec::new();
    for k in 0..frames {
        if s.entity(barrel).is_some() {
            assert!(s.debug_set_pos(barrel, n(AT.0, AT.1)));
        }
        before(k, &mut s, barrel);
        s.tick();
        skel.push(find_live(&s, Team::Blue, "Skeleton").iter().map(|e| e.pos).collect());
        hp.push(s.entity(barrel).map(|e| e.hp));
    }
    (barrel, skel, hp)
}

#[test]
fn the_third_play_is_the_evolved_barrel() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "SkeletonBalloon").expect("the barrel in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for _ in 0..2 {
        assert_eq!(next(&s), "SkeletonBalloon", "a basic play");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "SkeletonBalloon", n(14500, 3500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "SkeletonBalloon_EV1", "the third play is the form");
}

#[test]
fn under_75_percent_it_drops_seven_skeletons_13_ticks_on() {
    // Set to 448 of 665 before frame 5's tick: as a hit landing on the tick before (the client's t974).
    let (_, skel, hp) = scene(30, |k, s, b| {
        if k == 5 {
            assert!(s.debug_set_hp(b, 448));
        }
    });
    assert!(hp.iter().all(|h| h.is_some()), "the barrel lives");
    let first = skel.iter().position(|v| !v.is_empty()).expect("a drop");
    assert_eq!(first, 5 + 12, "the Skeletons on the 13th frame from the hit's (t974 -> t987)");
    assert_eq!(skel[first].len(), 7, "seven");
    // A ring about 1,480 around the drop point, (-350, 450) from the barrel in the owner's frame.
    let c = n(AT.0 - 350, AT.1 + 450);
    for p in &skel[first] {
        let d = p.dist(c) / K;
        assert!((1200..=1600).contains(&d), "about 1,480 from the drop point: {d}");
    }
    assert!(skel[first..].iter().all(|v| v.len() <= 7), "one drop at the line, not one a tick");
}

#[test]
fn its_death_drops_seven_more_12_ticks_after() {
    // Under the line on frame 2 (the first drop), killed on frame 20.
    let (_, skel, hp) = scene(40, |k, s, b| {
        if k == 2 {
            assert!(s.debug_set_hp(b, 400));
        }
        if k == 20 {
            assert!(s.debug_set_hp(b, 0));
        }
    });
    let gone = hp.iter().position(|h| h.is_none()).expect("the barrel dies");
    assert_eq!(gone, 20, "gone on the killing frame");
    let more = (gone..skel.len()).find(|&k| skel[k].len() > skel[k - 1].len()).expect("the death drop");
    assert_eq!(more, gone + 12, "its Skeletons 12 frames after (t1033 -> t1045)");
    assert_eq!(skel[more].len() - skel[more - 1].len(), 7, "seven more");
}
