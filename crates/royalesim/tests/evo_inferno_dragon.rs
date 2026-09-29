//! THE EVO INFERNO DRAGON (card.rs `StagesDef`; combat.rs `stage_damage`; state.rs `evo_after_fire`, `stages_pass`),
//! against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS:
//!   - sp-ec-InfernoDragon: the third play of an evolved Inferno Dragon entry puts the form down (DarkElixirCost 2);
//!   - sp-form-InfernoDragon-evo-s0: the form's hits, 400 ms apart, read 35, 35, a third on the Skeleton they kill, 35
//!     on a Knight, then 120 on each of the next four: the entry its hit count has reached (14 while under 4, 47 under
//!     9, 165 under 49, 330 after; the level-11 35, 120, 422 and 844), across targets. The 422 and 844 entries, the
//!     7000 ms decay and the hold (COMBAT_DISABLED) are the table's, unmeasured.
//!
//! THE SCENES put the form down with spawn_unit on Blue's side at (9000, 11500), both sides at level 11, Blue's princess
//! towers down, its targets held in place (and a Golem's hp topped up) so that nothing but the dragon moves them.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_inferno_dragon`):
//!   - stages_unread -> `its_hits_ramp_by_their_count_across_targets` red (the VariableDamage time ramp);
//!   - stage_count_never -> `its_hits_ramp_by_their_count_across_targets` red (every hit 35);
//!   - stage_decay_never -> `its_count_falls_to_0_7000_ms_after_its_last_hit` red;
//!   - stage_hold_kept -> `a_zap_on_it_zeroes_its_count` red.
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

/// A battle whose Blue deck is the Inferno Dragon, evolved, and a Knight, and Red's a Knight and the Zap; both sides at
/// level 11, past the opening lockout, Blue's princess towers down.
fn battle() -> BattleState {
    let mut cfg = config();
    cfg.decks = [vec!["InfernoDragon".into(), "Knight".into()], vec!["Knight".into(), "Zap".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    s
}

/// The form at (9000, 11500) with a red Skeleton 2500 ahead (in its reach) and a red Golem at (12000, 13500), 3,606 from
/// it: the battle, the dragon and the Golem.
fn scene() -> (BattleState, EntityId, EntityId) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "InfernoDragon_EV1", n(9000, 11500), None).expect("the form");
    s.spawn_unit_resolved(Team::Red, "Skeleton", n(9000, 14000), None).expect("the Skeleton");
    s.spawn_unit_resolved(Team::Red, "Golem", n(12000, 13500), None).expect("the Golem");
    s.tick();
    let dragon = find_live(&s, Team::Blue, "InfernoDragon_EV1")[0].id;
    let golem = find_live(&s, Team::Red, "Golem")[0].id;
    (s, dragon, golem)
}

/// Run `frames` frames with the Golem held at `at` and its hp topped up to 100000 before every tick, and `hold` (a unit
/// and its point) held too: the damage of each hit the Golem takes, in order.
fn golem_hits(s: &mut BattleState, golem: EntityId, at: Vec2, hold: Option<(EntityId, Vec2)>, frames: u32) -> Vec<i32> {
    let mut hits = Vec::new();
    for _ in 0..frames {
        assert!(s.debug_set_pos(golem, at));
        if let Some((id, p)) = hold {
            assert!(s.debug_set_pos(id, p));
        }
        assert!(s.debug_set_hp(golem, 100_000));
        s.tick();
        let drop = 100_000 - s.entity(golem).expect("the Golem stands").hp;
        if drop > 0 {
            hits.push(drop);
        }
    }
    hits
}

#[test]
fn the_third_play_is_the_evolved_dragon() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "InfernoDragon").expect("the dragon in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for _ in 0..2 {
        assert_eq!(next(&s), "InfernoDragon", "a basic play");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "InfernoDragon", n(14500, 3500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "InfernoDragon_EV1", "the third play is the form");
}

#[test]
fn its_hits_ramp_by_their_count_across_targets() {
    // Three hits kill the Skeleton (81 hp); the Golem's first hit is the fourth: 35, then 120 x 5, 422 x 40, then 844.
    let (mut s, _, golem) = scene();
    let at = s.entity(golem).unwrap().pos;
    let hits = golem_hits(&mut s, golem, at, None, 700);
    assert!(hits.len() >= 50, "the dragon hits the Golem 50 times: {hits:?}");
    let want: Vec<i32> = std::iter::once(35).chain([120; 5]).chain([422; 40]).chain([844; 4]).collect();
    assert_eq!(hits[..50], want[..], "the entries by hit count, across targets");
}

/// The first hit on the Golem after it is held out of the dragon's range for `gap` ticks (the dragon's count past its
/// fourth hit, the 120 entry, when it leaves).
fn first_hit_after_a_gap(gap: u32) -> i32 {
    let (mut s, dragon, golem) = scene();
    let at = s.entity(golem).unwrap().pos;
    // Three hits on the Golem (the sixth in all): the count is 6.
    let mut hits = Vec::new();
    while hits.len() < 3 {
        hits.extend(golem_hits(&mut s, golem, at, None, 1));
    }
    assert_eq!(hits, vec![35, 120, 120], "the count reaches 6");
    // The Golem 5000 further off, out of the dragon's reach, and the dragon held where it stands.
    let d = s.entity(dragon).unwrap().pos;
    let away = Vec2::new(at.x + 5000 * K, at.y);
    assert!(golem_hits(&mut s, golem, away, Some((dragon, d)), gap).is_empty(), "no hit while the Golem is away");
    *golem_hits(&mut s, golem, at, Some((dragon, d)), 200).first().expect("the dragon hits the Golem again")
}

#[test]
fn its_count_falls_to_0_7000_ms_after_its_last_hit() {
    assert_eq!(first_hit_after_a_gap(60), 120, "3000 ms after its last hit the count stands");
    assert_eq!(first_hit_after_a_gap(150), 35, "7500 ms after its last hit the count is 0");
}

#[test]
fn a_zap_on_it_zeroes_its_count() {
    let (mut s, dragon, golem) = scene();
    let at = s.entity(golem).unwrap().pos;
    let mut hits = Vec::new();
    while hits.len() < 3 {
        hits.extend(golem_hits(&mut s, golem, at, None, 1));
    }
    assert_eq!(hits, vec![35, 120, 120], "the count reaches 6");
    // A red Zap on the dragon: its stun holds it (COMBAT_DISABLED), and the count is 0; the Golem stands outside the
    // Zap's radius.
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    let p = s.entity(dragon).unwrap().pos;
    s.deploy(Team::Red, "Zap", p).expect("the Zap");
    let after = golem_hits(&mut s, golem, at, None, 200);
    assert_eq!(after.first(), Some(&35), "the first hit after the Zap: {after:?}");
}
