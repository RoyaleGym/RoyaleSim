//! A HOOK ON A UNIT THAT CHANGES INTO A BUILDING (combat.SPECIAL_HOOK = client_hook_drag; state.rs `special_step`,
//! `step_hook_drags`, `end_drag`, `rebind_unit` and the hook's landing in `apply_effects`).
//!
//! The Fisherman's hook starts only on an enemy ground troop (`hookable`). The Cannon Cart is one while it rolls, and
//! it becomes a building in place when it drops to half its hitpoints (the card MovingCannon becomes the unit
//! BrokenCannon; tests/transform.rs). Tower or defender fire can take a Cart across that line at any point of the hook:
//! while the Fisherman loads it, while it flies or while it drags. The client was not recorded hooking a Cart, so both
//! rules below are the engine's, built from the rules it already has.
//!
//! WHAT IS PINNED, each scene beside a control where the Cart stays above half and is dragged to the margin:
//!   1. `a_cart_that_breaks_mid_drag_ends_the_drag_and_frees_the_fisherman`: a Cart that becomes a building while it
//!      is dragged ends the drag the way a drag ends at its margin. The building stays where the change found it, the
//!      Fisherman's special is over on the change tick, and he walks on. Before, the change dropped only the victim's
//!      side of the drag, so the Fisherman stood in his windup until the building was gone.
//!   2. `a_hook_that_lands_on_a_broken_cart_drags_nothing`: a Cart that becomes a building while the hook flies is no
//!      longer a unit the hook can take, by the rule the special applies to a target when it starts. The hook lands as
//!      a miss: nothing is dragged, and the special ends on the landing tick. Before, the hook dragged the building.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test hook_transform`):
//!   * `hook_kept_through_transform` -- the change into a building leaves the thrower's special running: (1) red.
//!   * `hook_lands_on_any_kind` -- a landing hook drags whatever it lands on: (2) red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, HookBuildings, HookLanding, SpecialHook};
use royalesim::{EntityId, Team};

/// The Cannon Cart's card. Its broken form is another row, read from the card's transformation.
const CART: &str = "MovingCannon";
/// A Blue Fisherman and a Red Cannon Cart 6,500 apart on x 9000, out of every tower's reach. The Cart stands and
/// shoots from where it is placed, and the Fisherman hooks it from where he is placed.
const FISHERMAN_AT: (i32, i32) = (9000, 11000);
const CART_AT: (i32, i32) = (9000, 17500);
/// Ticks each scene runs: the hook is thrown on tick 27 and the unbroken drag ends on tick 46. Both units live
/// through them in every scene here.
const TICKS: usize = 70;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Native distance between two world points.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Every card and tower at level 11, the level the Cart's transformation was measured at.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    // The drag's ticks were pinned with the hook landing on its victim where the victim walked (combat.HOOK_LANDING =
    // on_victim); the shipped landing on the hook's point starts the drag a tick later.
    cfg.calib.hook_landing = HookLanding::OnVictim;
    // A victim that became a building ends the drag, as a hook on a building did (combat.HOOK_BUILDINGS =
    // troops_only); the shipped client_pull_self pulls the Fisherman to it instead.
    cfg.calib.hook_buildings = HookBuildings::TroopsOnly;
    cfg
}

/// When the scene takes the Cart below its line (by `debug_set_hp`, as a hit landed at the end of the tick before).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Break {
    /// Never: the control.
    Never,
    /// Before the first tick that starts with the hook in flight.
    InFlight,
    /// Before the tick after the drag's first step.
    MidDrag,
}

/// The two units after one tick.
#[derive(Clone, Debug)]
struct Frame {
    fisherman: Vec2,
    phase: AttackPhase,
    cart: Vec2,
    card: String,
    kind: EntityKind,
    /// A hook the Fisherman threw is in flight.
    hook: bool,
}

fn hook_in_flight(s: &BattleState, by: EntityId) -> bool {
    s.projectiles().iter().any(|p| p.hook == Some(by))
}

/// The scene, `TICKS` frames, the Cart taken below its line at `brk`. Also returns the broken form's name.
fn play(brk: Break) -> (Vec<Frame>, String) {
    let mut s = BattleState::new(0, level11(config()));
    assert_eq!(s.config().calib.special_hook, SpecialHook::ClientHookDrag, "the shipped combat.SPECIAL_HOOK");
    let fisherman = s.scenario_spawn_now(Team::Blue, "Fisherman", at(FISHERMAN_AT), None).expect("place the Fisherman");
    let cart = s.scenario_spawn_now(Team::Red, CART, at(CART_AT), None).expect("place the Cannon Cart");
    let tr = card_stat(&s, CART).transform_at_hp.expect("the Cannon Cart carries its transformation");
    let broken = s.cards().get(tr.unit).name.clone();
    assert_ne!(broken, CART, "vacuous: the Cart's broken form is read under the card's own name");
    let max_hp = s.entity(cart).expect("the Cart").max_hp;
    let below = max_hp * tr.pct / 100 - 50;
    let start = at(CART_AT);
    let mut done = false;
    let mut frames = Vec::with_capacity(TICKS);
    for _ in 0..TICKS {
        if !done {
            let due = match brk {
                Break::Never => false,
                Break::InFlight => hook_in_flight(&s, fisherman),
                Break::MidDrag => s.entity(cart).is_some_and(|c| c.pos != start),
            };
            if due {
                assert!(s.debug_set_hp(cart, below), "the Cart is gone");
                done = true;
            }
        }
        s.tick();
        let f = s.entity(fisherman).unwrap_or_else(|| panic!("the scene drifted: the Fisherman died on tick {}", s.tick_count()));
        let c = s.entity(cart).unwrap_or_else(|| panic!("the scene drifted: the Cart died on tick {}", s.tick_count()));
        frames.push(Frame {
            fisherman: f.pos,
            phase: f.attack_phase,
            cart: c.pos,
            card: c.card.to_string(),
            kind: c.kind,
            hook: hook_in_flight(&s, fisherman),
        });
    }
    assert!(brk == Break::Never || done, "the scene drifted: the Cart was never broken at {brk:?}");
    (frames, broken)
}

/// The frame the Cart is first the broken form.
fn change_frame(frames: &[Frame], broken: &str) -> usize {
    frames.iter().position(|f| f.card == broken).unwrap_or_else(|| panic!("the Cart never became {broken}"))
}

/// The control: the Cart stays above half, is dragged straight at the Fisherman to the margin, and his special ends
/// only then. Returns (the first frame of the drag, the frame the special is over).
fn control_drag(frames: &[Frame]) -> (usize, usize) {
    let start = at(CART_AT);
    let first = frames.iter().position(|f| f.cart != start).expect("the scene drifted: the control Cart was never dragged");
    let over = frames[first..].iter().position(|f| f.phase != AttackPhase::Windup).map(|k| first + k).expect("the control drag never ended");
    assert!(over > first + 5, "the scene drifted: the control drag ran {} frames", over - first);
    assert!(frames[first..over].iter().all(|f| f.card == CART && f.fisherman == at(FISHERMAN_AT)), "the Fisherman walked during the control drag");
    assert!(dist(frames[over].cart, start) > 4000, "the scene drifted: the control drag moved the Cart {}", dist(frames[over].cart, start));
    (first, over)
}

// ---------------------------------------------------------------------------
// (1)

/// Plant: hook_kept_through_transform.
#[test]
fn a_cart_that_breaks_mid_drag_ends_the_drag_and_frees_the_fisherman() {
    let (control, _) = play(Break::Never);
    let (drag, over) = control_drag(&control);
    let (frames, broken) = play(Break::MidDrag);
    let c = change_frame(&frames, &broken);
    assert_eq!(c, drag + 1, "the scene drifted: the change is not on the tick after the drag's first step");
    assert_eq!(frames[c].kind, EntityKind::Building, "{broken} is a building");
    assert!(c < over, "the scene drifted: the control drag was over by the change");
    // the control on the same tick: still dragged, the Fisherman standing in his special
    assert_ne!(control[c].cart, control[c - 1].cart, "the control Cart was not dragged on the change tick");
    assert_eq!(control[c].phase, AttackPhase::Windup, "the control Fisherman is out of his special on the change tick");
    // the drag is over where the change found it
    let q = frames[c].cart;
    assert_eq!(q, frames[c - 1].cart, "the building was dragged on its change tick");
    assert!(frames[c..].iter().all(|f| f.cart == q), "the building was dragged after the change");
    // and so is the special: out of the windup on the change tick, walking at the building from the next
    assert_ne!(
        frames[c].phase,
        AttackPhase::Windup,
        "the Fisherman is still in his special on the change tick: nothing ended it when the drag's victim became a building"
    );
    let walked = frames[c + 1..c + 4].iter().any(|f| f.fisherman != at(FISHERMAN_AT));
    assert!(walked, "the Fisherman stood for 3 ticks after the drag ended: {:?}", frames[c..c + 4].iter().map(|f| f.phase).collect::<Vec<_>>());
    assert!(dist(frames[c + 10].fisherman, q) < dist(at(FISHERMAN_AT), q), "the Fisherman did not walk at the building");
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: hook_lands_on_any_kind.
#[test]
fn a_hook_that_lands_on_a_broken_cart_drags_nothing() {
    let (control, _) = play(Break::Never);
    let (drag, _) = control_drag(&control);
    let (frames, broken) = play(Break::InFlight);
    let c = change_frame(&frames, &broken);
    assert!(frames[c].hook, "the scene drifted: the hook was not in flight when the Cart changed");
    assert_eq!(frames[c].kind, EntityKind::Building, "{broken} is a building");
    let landed = frames[c..].iter().position(|f| !f.hook).map(|k| c + k).expect("the hook never landed");
    // the control's hook lands on the same tick and drags on the next
    assert_eq!(landed + 1, drag, "the scene drifted: the control drag does not start the tick after this landing");
    // nothing is dragged
    let q = frames[c].cart;
    assert!(
        frames[c..].iter().all(|f| f.cart == q),
        "the hook dragged the building: {:?} after its landing",
        frames[landed..landed + 4].iter().map(|f| f.cart).collect::<Vec<_>>()
    );
    // the special is over on the landing tick, and the Fisherman walks on
    assert_eq!(frames[landed - 1].phase, AttackPhase::Windup, "the scene drifted: the Fisherman was not in his special while the hook flew");
    assert_ne!(frames[landed].phase, AttackPhase::Windup, "the Fisherman is still in his special after his hook landed as a miss");
    assert!(frames[landed + 1..landed + 4].iter().any(|f| f.fisherman != at(FISHERMAN_AT)), "the Fisherman stood after his hook missed");
}
