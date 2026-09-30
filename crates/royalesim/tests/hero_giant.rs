//! THE HERO GIANT (card.rs `SlapDef`, `AbilityEffect::Slap`, SLAP_FLIGHT_STEP, SLAP_HOLD_EXTRA_TICKS; spell.rs `shape_of`;
//! state.rs `SlapBoard`, `slap_pick`, `slap_pass`), at level 11.
//!
//! THE MEASUREMENTS (sp-form-Giant-hero-s0; the press issued t200):
//!   - the Giant took the Skeleton 2868 from him on t204 (2500 + its 500; 3008 on t203), stood t205-t222 (18 frames for
//!     the table's 800 ms) and walked on t223;
//!   - the Skeleton's target went on t212 (the pick + 8: PushbackDelay 400) and it moved -250 along x, y unchanged, on
//!     every tick from t213 (toward the arena's centre, past the Giant) until a tower's arrow killed it on t233.
//! THE MEASUREMENTS (sp-slap-*, five scenes: Knights and a Golem, either lane, either side of the Giant): the target
//! moved 250 along x toward the centre on each of 33 ticks from the throw's next, took 135 (53 on the ladder) on the
//! 32nd, moved 225, 200, ... 25, stood a tick, stepped back 25 and walked on the next: the table's 23000 as a pushback
//! ladder capped at 250.
//! Read off the table, not measured: the 2000 ms stun (inside the hold), the pick among several (the most hitpoints and
//! shield), the refusal of a unit that ignores pushback and the seek again 400 ms on.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_giant`):
//!   - slap_never -> both tests red;
//!   - slap_hold_table_only -> `the_slap_holds_the_giant_and_throws_its_pick_toward_the_centre` red;
//!   - slap_flight_never -> both red;
//!   - slap_landing_dropped -> `the_slap_holds_the_giant_and_throws_its_pick_toward_the_centre` red.
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

const DECK: [&str; 8] = ["Giant", "Knight", "Archer", "Musketeer", "MiniPekka", "HogRider", "Fireball", "Zap"];
/// Right of the centre line on blue's side; the throw runs along y 14800, out of every crown tower's reach.
const AT: (i32, i32) = (13000, 12000);

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

/// Red `units` put on their points, then the Giant at AT; everyone held 40 ticks; then the press. Returns the battle,
/// the Giant and the red units.
fn start(units: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<EntityId>) {
    let mut s = battle();
    let reds: Vec<(EntityId, Vec2)> = units
        .iter()
        .map(|(card, p)| (s.scenario_spawn_now(Team::Red, card, n(p.0, p.1), None).expect("a red unit"), n(p.0, p.1)))
        .collect();
    s.spawn_unit(Team::Blue, "Giant_hero", n(AT.0, AT.1), None).expect("the hero");
    s.tick();
    let giant = find_live(&s, Team::Blue, "Giant_hero")[0].id;
    for _ in 0..40 {
        assert!(s.debug_set_pos(giant, n(AT.0, AT.1)));
        for (id, p) in &reds {
            assert!(s.debug_set_pos(*id, *p));
        }
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    (s, giant, reds.into_iter().map(|(id, _)| id).collect())
}

/// One frame: the Giant's point; each red unit's point, hitpoints and whether it has a target (None once gone).
type Frame = (Vec2, Vec<Option<(Vec2, i32, bool)>>);

fn run(s: &mut BattleState, giant: EntityId, reds: &[EntityId], frames: usize) -> Vec<Frame> {
    (0..frames)
        .map(|_| {
            s.tick();
            let g = s.entity(giant).expect("the Giant").pos;
            (g, reds.iter().map(|id| s.entity(*id).map(|e| (e.pos, e.hp, e.target.is_some()))).collect())
        })
        .collect()
}

#[test]
fn the_slap_holds_the_giant_and_throws_its_pick_toward_the_centre() {
    // A red Knight 2800 ahead of the Giant (within 2500 + its 500): the pick on the first pass. The Giant stands 18
    // frames from the next; 8 frames after the pick the Knight loses its target and from the next it moves along x
    // (toward the centre line, x 9000), y unchanged: -250 on 33 frames, -225, -200, ... -25, 0, then +25; the blow takes
    // 135 off it on the 32nd.
    let (mut s, giant, reds) = start(&[("Knight", (AT.0, AT.1 + 2800))]);
    let f = run(&mut s, giant, &reds, 70);
    let knight = |k: usize| f[k].1[0].expect("the Knight alive");
    let first = (1..f.len()).find(|&k| knight(k).0.x - knight(k - 1).0.x == -250 * K).expect("the Knight thrown");
    let pick = first - 9;
    assert!(!knight(pick + 8).2, "its target gone on the pick + 8");
    let mut ladder: Vec<i32> = vec![-250; 33];
    ladder.extend((1..=9).map(|j| -250 + 25 * j));
    ladder.extend([0, 25]);
    for (j, want) in ladder.iter().enumerate() {
        let k = first + j;
        let (dx, dy) = (knight(k).0.x - knight(k - 1).0.x, knight(k).0.y - knight(k - 1).0.y);
        assert_eq!((dx / K, dy / K), (*want, 0), "frame {k}: step {j} of the throw");
    }
    for k in pick + 1..=pick + 18 {
        assert_eq!(f[k].0, f[pick].0, "the Giant stands on frame {k} (the pick + {})", k - pick);
    }
    assert_ne!(f[pick + 19].0, f[pick + 18].0, "and walks on the pick + 19");
    let losses: Vec<(usize, i32)> = (1..f.len()).map(|k| (k, knight(k - 1).1 - knight(k).1)).filter(|(_, d)| *d > 0).collect();
    assert_eq!(losses.first(), Some(&(first + 31, 135)), "the landing blow's 135 on the 32nd step: {losses:?}");
}

#[test]
fn a_unit_that_ignores_pushback_is_not_thrown_and_the_slap_seeks_again() {
    // A red Giant (IgnorePushback, the most hitpoints) 2600 ahead and a red Knight 2800 ahead: the red Giant is the pick
    // and is refused; 400 ms on the seek runs again and takes the Knight, which is thrown.
    let (mut s, giant, reds) = start(&[("Giant", (AT.0 - 1000, AT.1 + 2400)), ("Knight", (AT.0 + 800, AT.1 + 2680))]);
    let f = run(&mut s, giant, &reds, 90);
    let steps = |u: usize| {
        (1..f.len())
            .filter(|&k| matches!((f[k].1[u], f[k - 1].1[u]), (Some(a), Some(b)) if a.0.x - b.0.x == -250 * K && a.0.y == b.0.y))
            .count()
    };
    assert_eq!(steps(0), 0, "the red Giant is never thrown");
    assert!(steps(1) >= 20, "the Knight is thrown: {} steps", steps(1));
}
