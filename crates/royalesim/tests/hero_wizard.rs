//! THE HERO WIZARD (card.rs `AbilityEffect::GroundToAir`, `CardDef::projectile_area_ahead`; state.rs `LiftRun`,
//! `lift_pass`; status.rs `BuffDef::no_pushed_by_ally`), against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (sp-form-Wizard-hero-s0; the press's first frame P = t215; the Wizard standing at (9500, 11500),
//! attacking a red Knight about 6100 away at the press, which walked to about 5300 by its shot):
//!   - the cast holds it P .. P + 17 (CastTime 950); a red Knight attacking it takes a tower on P + 5: the Wizard is in
//!     the air. Here the Knight lets it go on P + 4, a tick sooner (open: the trigger's own tick, state.rs
//!     LEVEL_SET_EARLY_TICKS);
//!   - its first shot after the cast (an instant hit: its target in reach), first seen on P + 19, takes 238 (93 at
//!     level 1) off the Knight on P + 25, and on P + 26 every enemy within 4000 of the point 1000 ahead of the hit (the owner's forward) loses 43
//!     (17 at level 1): the Knight and a Musketeer beside it;
//!   - from P + 27 both are pulled toward that point (the Knight +91 a tick in y, the Musketeer -149).
//! Read off the table, not measured: the descent (the Wizard died on P + 80, in the air) and the 150 % walk.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_wizard`):
//!   - lift_never -> `a_ground_only_attacker_drops_it_on_the_trigger` red;
//!   - instant_hit_dropped -> `its_first_shot_after_the_cast_hits_on_p_plus_25` red;
//!   - ally_push_kept -> `the_unit_its_shot_strikes_is_not_pushed_by_its_own_side` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Wizard", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];
const WIZARD: (i32, i32) = (9500, 11500);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s
}

/// One frame: the Wizard's card name and target, and each red unit's point, hp and target.
struct Frame {
    card: String,
    reds: Vec<Option<(Vec2, i32, Option<EntityId>)>>,
}

/// The Wizard held at `at`, red `units` held at their points (their hp topped up when `full`) until frame `free`
/// (none: always), deployed; then the press and `frames` frames, frame k being P + k (P the press's first frame, as
/// the client's frames are counted). Returns the Wizard's id, the red units' ids and the frames.
fn scene(at: (i32, i32), units: &[(&str, (i32, i32))], frames: usize, free: Option<usize>, full: bool) -> (EntityId, Vec<EntityId>, Vec<Frame>) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Wizard_hero", n(at.0, at.1), None).expect("the hero");
    for (card, p) in units {
        s.spawn_unit_resolved(Team::Red, card, n(p.0, p.1), None).expect("a red unit");
    }
    s.tick();
    let hero = find_live(&s, Team::Blue, "Wizard_hero")[0].id;
    let ids: Vec<EntityId> = units
        .iter()
        .map(|(card, p)| s.entities().find(|e| e.team == Team::Red && e.card == *card && e.pos == n(p.0, p.1)).expect("the unit where it was put").id)
        .collect();
    let tops: Vec<i32> = ids.iter().map(|id| s.entity(*id).expect("the unit").max_hp).collect();
    let hold = |s: &mut BattleState, reds: bool| {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(at.0, at.1)));
        }
        for ((id, (_, p)), top) in ids.iter().zip(units).zip(&tops) {
            if s.entity(*id).is_some() {
                if reds {
                    assert!(s.debug_set_pos(*id, n(p.0, p.1)));
                }
                if full {
                    assert!(s.debug_set_hp(*id, *top));
                }
            }
        }
    };
    for _ in 0..40 {
        hold(&mut s, true);
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    hold(&mut s, true);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    for k in 0..frames {
        hold(&mut s, free.is_none_or(|f| k < f));
        s.tick();
        let card = s.entity(hero).map(|e| e.card.to_string()).unwrap_or_default();
        let reds = ids.iter().map(|id| s.entity(*id).map(|e| (e.pos, e.hp, e.target))).collect();
        out.push(Frame { card, reds });
    }
    (hero, ids, out)
}

/// The shot's scene moved west by SHOT_SHIFT: in place, the blue princess tower reaches its Knight.
const SHOT_SHIFT: i32 = 4000;

fn shifted(p: (i32, i32)) -> (i32, i32) {
    (p.0 - SHOT_SHIFT, p.1)
}

/// P + d is frame d.
fn at(d: usize) -> usize {
    d
}

#[test]
fn a_ground_only_attacker_drops_it_on_the_trigger() {
    // A red Knight beside the Wizard, swinging at it, held there with its hp topped up.
    let (hero, _, f) = scene(WIZARD, &[("Knight", (9500, 12800))], 20, None, true);
    for d in 0..4 {
        assert_eq!(f[at(d)].reds[0].unwrap().2, Some(hero), "the Knight keeps the Wizard on P + {d}");
    }
    // (P + 4 unasserted: the client's Knight still has it, this one has let it go; see the header.)
    for d in 5..20 {
        assert_ne!(f[at(d)].reds[0].unwrap().2, Some(hero), "the Wizard in the air on P + {d}");
    }
    assert_eq!(f[at(10)].card, "WizardHero_air", "its air form by P + 10 (the trigger + 200 ms)");
}

#[test]
fn its_first_shot_after_the_cast_hits_on_p_plus_25() {
    // The scene's Knight held where it walked by the shot (about 5300 away, in the Wizard's reach), the Musketeer
    // where it stood, both held through P + 26: the scene 4000 west (SHOT_SHIFT), out of every crown tower's reach.
    let (_, _, f) = scene(shifted(WIZARD), &[("Knight", shifted((13640, 14790))), ("Musketeer", shifted((13770, 17015)))], 40, Some(27), false);
    let hp = |d: usize, u: usize| f[at(d)].reds[u].unwrap().1;
    // (a shot it fired before the press lands by P + 8; from its cast's end the Knight is untouched until the air shot)
    assert!((19..25).all(|d| hp(d, 0) == hp(18, 0)), "nothing on the Knight P + 19 .. P + 24");
    assert_eq!(hp(24, 0) - hp(25, 0), 238, "the air shot, 93 at level 1, on P + 25");
    assert_eq!(hp(25, 0) - hp(26, 0), 43, "the damage area on P + 26 (17 at level 1)");
    assert_eq!(hp(25, 1) - hp(26, 1), 43, "on the Musketeer beside it too");
}

#[test]
fn its_areas_pull_toward_the_point_1000_ahead_of_the_hit() {
    let (_, _, f) = scene(shifted(WIZARD), &[("Knight", shifted((13640, 14790))), ("Musketeer", shifted((13770, 17015)))], 40, Some(27), false);
    let pos = |d: usize, u: usize| f[at(d)].reds[u].unwrap().0;
    // Held through P + 26; the pull moves them from P + 27: the Knight up the arena, the Musketeer down, toward the
    // point 1000 ahead of the Knight (y 15790).
    let (k0, k1) = (pos(26, 0), pos(27, 0));
    let (m0, m1) = (pos(26, 1), pos(27, 1));
    assert!(k1.y > k0.y, "the Knight pulled up the arena: {k0:?} -> {k1:?}");
    assert!(m1.y < m0.y, "the Musketeer pulled down: {m0:?} -> {m1:?}");
}

#[test]
fn the_unit_its_shot_strikes_is_not_pushed_by_its_own_side() {
    // The table's reading (the air shot's target buff HeroWizardNoMove sets NO_PUSHED_BY_ALLY), unmeasured: the Knight
    // the shot strikes walks the same pull whether or not a red Skeleton is put down on its point after the hit. The
    // shot's scene, west of the towers; the Knight held through P + 26, the Skeleton put down on P + 27.
    let run = |with_ally: bool| -> Vec<Vec2> {
        let mut s = battle();
        let (w, kp) = (shifted(WIZARD), shifted((13640, 14790)));
        s.spawn_unit(Team::Blue, "Wizard_hero", n(w.0, w.1), None).expect("the hero");
        s.spawn_unit_resolved(Team::Red, "Knight", n(kp.0, kp.1), None).expect("the Knight");
        s.tick();
        let hero = find_live(&s, Team::Blue, "Wizard_hero")[0].id;
        let knight = find_live(&s, Team::Red, "Knight")[0].id;
        let hold = |s: &mut BattleState, knight_too: bool| {
            if s.entity(hero).is_some() {
                assert!(s.debug_set_pos(hero, n(w.0, w.1)));
            }
            if knight_too {
                assert!(s.debug_set_pos(knight, n(kp.0, kp.1)));
            }
        };
        for _ in 0..40 {
            hold(&mut s, true);
            s.tick();
        }
        hold(&mut s, true);
        s.press_ability_button(Team::Blue, 0).expect("the press");
        let mut out = Vec::new();
        for k in 0..40 {
            hold(&mut s, k < 27);
            if with_ally && k == 27 {
                let at = s.entity(knight).expect("the Knight").pos;
                s.scenario_spawn_now(Team::Red, "Skeleton", at, None).expect("a red Skeleton on the Knight");
            }
            s.tick();
            out.push(s.entity(knight).map(|e| e.pos).unwrap_or_default());
        }
        out
    };
    let (alone, beside) = (run(false), run(true));
    assert_ne!(alone[at(27)], alone[at(34)], "the Knight is pulled");
    assert_eq!(alone[at(27)..at(35)], beside[at(27)..at(35)], "its own side does not push it while the buff lasts");
}
