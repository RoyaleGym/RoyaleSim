//! combat.SPIN_BLOWS: when and from where a Hero Valkyrie's spin makes its blows (state.rs `spin_seek`, `spin_pass`).
//!
//! THE LAW, measured on client 16.635 (parity r98, the oracle's il-mg-16635 set: 12 of 12 first-blow victims hit on the
//! begin + 1, none on the begin; of 15 victims near the edge, her start-of-tick point decides 15, her post-move point 1):
//! her blows are made before her move, on her point at the tick's start, the first on the begin + 1 and the rest on the
//! begin + 5k; each strikes in that tick's spells step, its victims where the tick's moves left them. The engine's arm,
//! after_move, makes them after her move on her post-move point, the first on the begin.
//!
//! THE SCENE: blue's princess towers down; the hero put back on her point before every tick (she steps 150 within each
//! tick of her chain); a red Knight held 2600 to her left (her chain's first target, never reached: 2200 its reach) and a
//! red Giant held 3200 to her right, inside 2500 + its 750 of her point at the tick's start and 3350 from where her step
//! leaves her; their hp left to fall; the press (combat.SPIN_BEGIN = client15535_next_tick, combat.SPIN_TARGETS = troops,
//! combat.SPIN_CHAIN_END = ends). b is the first frame she steps 140 or more (her begin).
//!
//! WHAT IS PINNED: under client16635_before_move the Knight's first two hits on b + 1 and b + 5, the Giant's first on
//! b + 1; under after_move (the vacuity check) the Knight's on b and b + 5 and none on the Giant within 30 frames.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! spin_blows`):
//!   * `spin_blows_after_move` -- the new arm still blows after the move, the first on the begin:
//!     `under_client16635_before_move_blows_come_from_her_start_point_the_first_a_tick_late` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpinBegin, SpinBlows, SpinChainEnd, SpinTargets};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Valkyrie", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];

/// A level-11 battle past the opening lockout, the Hero Valkyrie in blue's deck, `f` applied to its config, and blue's
/// princess towers down (their shots would reach the scene; blue's King stays out of its reach).
fn battle(f: impl FnOnce(&mut BattleConfig)) -> BattleState {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    f(&mut cfg);
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s
}

const HERO: (i32, i32) = (9000, 14000);
const KNIGHT: (i32, i32) = (6400, 14000);
const GIANT: (i32, i32) = (12200, 14000);

/// Under combat.SPIN_BLOWS = `arm`, over the press's first 30 frames (0 its first): b, the first frame she steps 140 or
/// more from her held point; the Knight's first two hp-drop frames; the Giant's first hp-drop frame.
fn blows(arm: SpinBlows) -> (usize, Vec<usize>, Option<usize>) {
    let mut s = battle(|c| {
        c.calib.spin_blows = arm;
        c.calib.spin_targets = SpinTargets::Troops;
        c.calib.spin_chain_end = SpinChainEnd::Ends;
        c.calib.spin_begin = SpinBegin::Client15535NextTick;
    });
    s.spawn_unit(Team::Blue, "Valkyrie_hero", n(HERO.0, HERO.1), None).expect("the hero");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT.0, KNIGHT.1), None).expect("a red Knight");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n(GIANT.0, GIANT.1), None).expect("a red Giant");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Valkyrie_hero")[0].id;
    let hold = |s: &mut BattleState| {
        assert!(s.debug_set_pos(hero, n(HERO.0, HERO.1)));
        assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)) && s.debug_set_pos(giant, n(GIANT.0, GIANT.1)));
    };
    let hp = |s: &BattleState, id: EntityId| s.entity(id).expect("a red unit").hp;
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    hold(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut b, mut knight_drops, mut giant_drop) = (None, Vec::new(), None);
    for k in 0..30usize {
        hold(&mut s);
        let (kh, gh) = (hp(&s, knight), hp(&s, giant));
        s.tick();
        let step = s.entity(hero).expect("the hero").pos.dist(n(HERO.0, HERO.1)) / K;
        if b.is_none() && step >= 140 {
            b = Some(k);
        }
        if hp(&s, knight) < kh && knight_drops.len() < 2 {
            knight_drops.push(k);
        }
        if hp(&s, giant) < gh && giant_drop.is_none() {
            giant_drop = Some(k);
        }
    }
    (b.expect("the scene drifted: her spin never began"), knight_drops, giant_drop)
}

/// Plant: spin_blows_after_move.
#[test]
fn under_client16635_before_move_blows_come_from_her_start_point_the_first_a_tick_late() {
    let (b, knight, giant) = blows(SpinBlows::AfterMove);
    // NOT VACUOUS: the engine's first blow falls on her begin, and from her post-move point the Giant is out of reach.
    assert_eq!(knight, vec![b, b + 5], "after_move: the Knight's first two hits on the begin and the begin + 5");
    assert_eq!(giant, None, "after_move: the Giant is never hit from her post-move point");
    let (b2, knight, giant) = blows(SpinBlows::Client16635BeforeMove);
    assert_eq!(b2, b, "her spin begins on the same frame under both arms");
    assert_eq!(knight, vec![b + 1, b + 5], "client16635_before_move: the Knight's first two hits on the begin + 1 and the begin + 5");
    assert_eq!(giant, Some(b + 1), "client16635_before_move: the Giant hit from her start-of-tick point on the begin + 1");
}

/// Since parity r98 for clients 16.402 and 16.635 (after_move before it; client 15.535.29 keeps after_move).
#[test]
fn the_shipped_arm_is_client16635_before_move() {
    assert_eq!(royalesim::state::Calib::shipped().spin_blows, royalesim::state::SpinBlows::Client16635BeforeMove);
    assert_eq!(config().calib.spin_blows, royalesim::state::SpinBlows::Client16635BeforeMove);
}
