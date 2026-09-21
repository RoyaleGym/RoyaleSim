//! SETUP SPAWN ORDER -- a MatchSetup's list order must not reach the battle.
//!
//! WHY IT EXISTS: `team_seq` (spawn ordinal within a team) is the last component of
//! every seat-invariant tie-break -- the target key, the coincident push, the
//! `YieldKey`, `obstacle_key`. Materialising setup spawns in LIST order makes the
//! list a hidden input: a rotation-mirrored MatchSetup with Red's spawns listed in
//! reverse (a full-hp Knight and a 300-hp Knight stacked on one point) desyncs at
//! tick 1, because the coincident push sends the lower team_seq to the team's
//! own-left and the two seats' lower team_seq are different Knights.
//! `BattleState::scenario_spawn_batch` spawns in a canonical order instead
//! (team, own-frame y, own-frame x, card name, starting hp).
//!
//! THE CHECKS: four boards, each under several list permutations
//! (Red same order, Red reversed, interleaved, Red first, Blue reversed): the
//! rotation mirror holds on every tick for 600 ticks, AND the final `state_hash` is
//! identical across every permutation -- the whole state, slots included, is a
//! function of the spawn multiset.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant
//! cargo test --test setup_spawn_order`), each of which must turn this file red:
//!   setup_spawn_list_order       list order again -> the mirror check,
//!       "stacked Knight full + Knight hp 300 / red reversed", tick 1; and the
//!       pinned order in `canonical_order_is_team_then_own_frame_y_x_card_name_hp`
//!   setup_spawn_team_list_order  team_seq canonical but the TEAMS spawn in list
//!       order -> the mirror holds and only the state_hash assertion goes red
//!       ("stacked Knight + Giant", "red first"), so that assertion can fail.
//!
//! WHAT IT CANNOT CATCH: the deferred spawn queue (`deploy` / `spawn_unit`) is a
//! different path. Through the Python protocol it is canonical already (at most one
//! deploy per team per step, applied in (team, slot) order); several `spawn_unit`
//! calls for one team in one tick still take CALL order.
mod common;

use royalesim::fixed::Vec2;
use royalesim::state::{BattleState, DeployError};
use royalesim::Team;
use common::*;

type Spec = (Team, &'static str, Vec2, Option<i32>);
/// A Blue-side spec: (card, position in hundredths of a tile, hp override).
type BlueSpec = (&'static str, (i32, i32), Option<i32>);

/// Blue's specs and their rotations for Red, in the same order.
fn board(s: &BattleState, blue: &[BlueSpec]) -> (Vec<Spec>, Vec<Spec>) {
    let b: Vec<Spec> = blue.iter().map(|&(c, at, hp)| (Team::Blue, c, t(at.0, at.1), hp)).collect();
    let r: Vec<Spec> = b.iter().map(|&(_, c, p, hp)| (Team::Red, c, mirror(s, p), hp)).collect();
    (b, r)
}

fn permutations(b: &[Spec], r: &[Spec]) -> Vec<(&'static str, Vec<Spec>)> {
    let rev = |v: &[Spec]| v.iter().rev().copied().collect::<Vec<_>>();
    vec![
        ("red same order", [b, r].concat()),
        ("red reversed", [b.to_vec(), rev(r)].concat()),
        ("interleaved", b.iter().zip(r).flat_map(|(x, y)| [*x, *y]).collect()),
        ("red first", [r, b].concat()),
        ("blue reversed", [rev(b), r.to_vec()].concat()),
    ]
}

#[test]
fn setup_list_order_cannot_reach_the_battle() {
    // The four boards, positions in
    // hundredths of a tile on Blue's side.
    let boards: [(&str, Vec<BlueSpec>); 4] = [
        ("stacked Knight full + Knight hp 300", vec![("Knight", (450, 1000), None), ("Knight", (450, 1000), Some(300))]),
        ("stacked Knight + Giant", vec![("Knight", (500, 1100), None), ("Giant", (500, 1100), None)]),
        ("Knight and Knight hp 300 side by side", vec![("Knight", (400, 1200), None), ("Knight", (500, 1200), Some(300))]),
        (
            "4 Goblins stacked, varied hp",
            vec![("Goblins", (1400, 900), None), ("Goblins", (1400, 900), Some(100)), ("Goblins", (1400, 900), Some(50)), ("Goblins", (1400, 900), Some(20))],
        ),
    ];
    for (label, blue) in boards {
        let probe = BattleState::new(1, symmetric_config());
        let (b, r) = board(&probe, &blue);
        let mut hashes = Vec::new();
        for (order, specs) in permutations(&b, &r) {
            let mut s = BattleState::new(1, symmetric_config());
            let ids = s.scenario_spawn_batch(&specs).unwrap_or_else(|(k, e)| panic!("{label} / {order}: spec {k}: {e:?}"));
            // ids come back in INPUT order.
            for (k, id) in ids.iter().enumerate() {
                let e = s.entity(*id).unwrap();
                assert_eq!((e.team, e.card, e.pos), (specs[k].0, specs[k].1, specs[k].2), "{label} / {order}: id {k} is not spec {k}");
            }
            check_mirror(&s).unwrap_or_else(|e| panic!("{label} / {order}: before tick 1: {e}"));
            for _ in 0..600 {
                s.tick();
                check_mirror(&s).unwrap_or_else(|e| panic!("{label} / {order}: {e}"));
            }
            hashes.push((order, s.state_hash()));
        }
        assert!(hashes.iter().all(|(_, h)| *h == hashes[0].1), "{label}: state_hash depends on the setup list order: {hashes:x?}");
    }
}

#[test]
fn canonical_order_is_team_then_own_frame_y_x_card_name_hp() {
    // Pin the order itself (a second engine must reproduce it from the protocol):
    // team_seq follows (own-frame y, own-frame x, card NAME, starting hp) per team,
    // after the three crown towers (team_seq 0..2).
    let mut s = BattleState::new(1, symmetric_config());
    let knight_full = {
        let db = s.cards();
        let k = db.index("Knight").unwrap();
        db.scaled(k, s.config().card_level[0], db.get(k).hitpoints).unwrap()
    };
    let p = |x, y| t(x, y);
    let specs: Vec<Spec> = vec![
        (Team::Blue, "Knight", p(500, 1000), None),              // y 10, x 5, Knight, full
        (Team::Blue, "Giant", p(500, 1000), None),               // y 10, x 5, Giant sorts before Knight
        (Team::Blue, "Knight", p(400, 1200), None),              // y 12
        (Team::Blue, "Knight", p(900, 1000), Some(knight_full)), // y 10, x 9
        (Team::Blue, "Knight", p(500, 1000), Some(1)),           // y 10, x 5, Knight, hp 1 before full
        (Team::Blue, "Archer", p(1700, 1100), None),             // y 11
    ];
    let red: Vec<Spec> = specs.iter().rev().map(|&(_, c, q, hp)| (Team::Red, c, mirror(&s, q), hp)).collect();
    let all = [specs.clone(), red].concat();
    let ids = s.scenario_spawn_batch(&all).unwrap();
    let seq = |k: usize| s.entity(ids[k]).unwrap().team_seq;
    // Expected Blue order: Giant(1), Knight hp 1(4), Knight full(0), Knight x9(3), Archer y11(5), Knight y12(2).
    let blue: Vec<u32> = (0..6).map(seq).collect();
    assert_eq!(blue, vec![5, 3, 8, 6, 4, 7], "Blue team_seq by input index");
    // Red listed in reverse; own-frame order is the same, so twins share team_seq.
    for k in 0..6 {
        assert_eq!(seq(6 + (5 - k)), seq(k), "Red twin of Blue spec {k} has a different team_seq");
    }
}

#[test]
fn batch_validates_everything_first_and_names_the_list_entry() {
    let mut s = BattleState::new(1, symmetric_config());
    let before = s.live_count();
    let river = Vec2::new(s.arena().width / 2, (s.arena().water_y_min + s.arena().water_y_max) / 2);
    let specs: Vec<Spec> = vec![
        (Team::Blue, "Knight", t(450, 1000), None),
        (Team::Red, "Knight", t(450, 2200), None),
        (Team::Blue, "Knight", river, None),
        (Team::Blue, "NoSuchCard", t(450, 1000), None),
    ];
    match s.scenario_spawn_batch(&specs) {
        Err((2, DeployError::Water)) => {}
        other => panic!("expected the first bad entry (index 2, Water), got {other:?}"),
    }
    assert_eq!(s.live_count(), before, "a refused batch must spawn nothing");
    assert!(matches!(s.scenario_spawn_batch(&specs[3..]), Err((0, DeployError::UnknownCard(_)))));
}
