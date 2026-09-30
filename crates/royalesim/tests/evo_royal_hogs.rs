//! THE EVO ROYAL HOGS (card.rs `FallDef`; state.rs `FallRun`, `fall_pass`, `evo_after_fire`), against client 15.535.29
//! at level 11.
//!
//! THE MEASUREMENTS (sp-form-RoyalHogs-evo-s0):
//!   - the third play of an evolved Royal Hogs entry is the form (DarkElixirCost 2); its four hogs fly (straight over
//!     the river) until they fall;
//!   - a hog hit under 99 % of its 837 on t1268 landed on t1280: its blow took 43 (17 at level 1) off the princess
//!     tower on t1281, and it walked a new path from t1282 (another, hit on t1243, landed on t1255, a new path t1257);
//!   - a hog whose first hit on the tower (74: 29 at level 1) fell on t1278 took a new path on t1289 and its blow took 43
//!     off the tower on t1290.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_royal_hogs`):
//!   - fall_never -> `under_99_percent_a_hog_lands_12_ticks_on_and_its_blow_takes_43` and
//!     `a_hog_that_strikes_lands_11_ticks_after_its_first_hit` red;
//!   - landing_blow_dropped -> both red (their blow);
//!   - fall_on_attack_unread -> `a_hog_that_strikes_lands_11_ticks_after_its_first_hit` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Over the river, out of every crown tower's reach (no tower shot trips the health line).
const AT: (i32, i32) = (9500, 15500);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["RoyalHogs".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// One frame: the first hog's card, whether it is in the air, and its hitpoints; what each red unit lost on the tick.
struct Frame {
    card: String,
    flying: bool,
    losses: Vec<i32>,
}

/// The four hogs of one evolved play put down at AT and held on their first points (all but the first moved west along
/// AT's line when `park`); red `units` put (dx, dy) from the first hog and held there, their hitpoints topped up;
/// `before(k, s, hog)` runs before frame k's tick.
fn scene(units: &[(&str, (i32, i32))], park: bool, frames: usize, mut before: impl FnMut(usize, &mut BattleState, EntityId)) -> Vec<Frame> {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "RoyalHogs_EV1", n(AT.0, AT.1), None).expect("the hogs");
    s.tick();
    let mut hogs: Vec<(EntityId, Vec2)> = find_live(&s, Team::Blue, "RoyalHogs_EV1").iter().map(|e| (e.id, e.pos)).collect();
    assert_eq!(hogs.len(), 4, "four hogs");
    if park {
        for (k, h) in hogs.iter_mut().enumerate().skip(1) {
            h.1 = n(2500 + 1500 * k as i32, AT.1);
        }
    }
    let first = hogs[0].1;
    let reds: Vec<(EntityId, Vec2, i32)> = units
        .iter()
        .map(|(card, (dx, dy))| {
            let p = Vec2::new(first.x + dx * K, first.y + dy * K);
            let id = s.scenario_spawn_now(Team::Red, card, p, None).expect("a red unit");
            (id, p, s.entity(id).expect("the red unit").max_hp)
        })
        .collect();
    let mut out = Vec::new();
    for k in 0..frames {
        for (id, p) in &hogs {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
            }
        }
        for (id, p, top) in &reds {
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, *top));
        }
        before(k, &mut s, hogs[0].0);
        s.tick();
        let h = s.entity(hogs[0].0).expect("the first hog");
        out.push(Frame {
            card: h.card.to_string(),
            flying: h.flying,
            losses: reds.iter().map(|(id, _, top)| top - s.entity(*id).expect("a red unit held alive").hp).collect(),
        });
    }
    out
}

#[test]
fn the_third_play_is_the_evolved_hogs() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "RoyalHogs").expect("the hogs in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for (k, x) in [3500, 14500].into_iter().enumerate() {
        assert_eq!(next(&s), "RoyalHogs", "a basic play {k}");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "RoyalHogs", n(x, 5500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "RoyalHogs_EV1", "the third play is the form");
}

#[test]
fn under_99_percent_a_hog_lands_12_ticks_on_and_its_blow_takes_43() {
    // A red Knight 1700 from the first hog (it cannot reach a flier). The hog set to 828 of its 837 before frame 30's
    // tick, as a blow in the tick before would leave it: the client's hog hit on t1268 landed on t1280.
    let f = scene(&[("Knight", (0, 1700))], false, 50, |k, s, h| {
        if k == 30 {
            assert!(s.debug_set_hp(h, 828));
        }
    });
    for (k, x) in f.iter().enumerate().take(41) {
        assert_eq!((x.card.as_str(), x.flying), ("RoyalHogs_EV1", true), "in the air on frame {k}");
    }
    for (k, x) in f.iter().enumerate().skip(41) {
        assert_eq!((x.card.as_str(), x.flying), ("RoyalHog_EV1_Grounded", false), "on the ground on frame {k}");
    }
    assert!(f[..42].iter().all(|x| x.losses[0] == 0), "nothing on the Knight before the blow");
    assert_eq!(f[42].losses[0], 43, "the blow on the tick after the landing (17 at level 1)");
}

#[test]
fn a_hog_that_strikes_lands_11_ticks_after_its_first_hit() {
    // A red Cannon behind the first hog, off the river (a Cannon shoots no flier), the other three parked away from it.
    let f = scene(&[("Cannon", (0, -1400))], true, 90, |_, _, _| {});
    let d = f.iter().position(|x| x.losses[0] == 74).expect("the first hog's first hit on the Cannon (29 at level 1)");
    assert!(f[..d + 11].iter().all(|x| x.flying), "in the air to frame {}", d + 10);
    assert_eq!((f[d + 11].card.as_str(), f[d + 11].flying), ("RoyalHog_EV1_Grounded", false), "landed 11 ticks after its hit (the client's t1278 -> t1289)");
    assert_eq!(f[d + 12].losses[0], 43, "the blow on the Cannon the tick after");
}
