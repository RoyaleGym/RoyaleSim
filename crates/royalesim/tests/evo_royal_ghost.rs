//! THE EVO ROYAL GHOST (card.rs `GhostDef`; state.rs `ghost_pair`), against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (oracle scenes sp-ghost-summons-s0, sp-ghost-ab-s0 and sp-form-Ghost-evo-s0):
//!   - sp-ec-Ghost: the third play of an evolved Royal Ghost entry puts the form down (DarkElixirCost 2);
//!   - a hit it makes while hidden puts two small ghosts down on the next tick, 2000 either side of what it hit, across
//!     the line from the ghost to it. sp-ghost-ab-s0: the ghost at (3261, 22739) hit the red left princess tower at
//!     (3500, 25500) on t1056; the pair stood at (5492, 25328) and (1508, 25672) from t1057, deploying through t1072,
//!     hp 81; its damage area took 81 off the tower on t1062, once;
//!   - hits 35 and 36 ticks after the last made no pair; hits 82 and 83 ticks after it did.
//!
//! THE SCENES put the form where the oracle's stood and hold it there (it would walk), its hp kept whole against the
//! tower's shots; a hit is the tick its attack progress passes a multiple of its HitSpeed.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_royal_ghost`):
//!   - ghost_pair_never -> `a_hit_while_hidden_puts_the_pair_down_on_the_next_tick` red;
//!   - ghost_pair_every_hit -> `its_next_hits_make_none_until_it_has_hidden_again` red;
//!   - ghost_strike_dropped -> `its_blow_strikes_what_it_hit_once_on_the_hits_tick_plus_6` red;
//!   - pair_starts_hidden -> `the_pair_starts_visible` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const GHOST: (i32, i32) = (3261, 22739);
const TOWER: (i32, i32) = (3500, 25500);

fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

fn battle() -> BattleState {
    let mut cfg = config();
    cfg.decks = [vec!["Ghost".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    s
}

/// One frame: the red tower's hp, whether the form fired on it, whether the form is hidden, and each small ghost
/// (id, point, hp, deploying, hidden).
struct Frame {
    tower: i32,
    fired: bool,
    hidden: bool,
    pair: Vec<(EntityId, Vec2, i32, bool, bool)>,
}

/// The form at GHOST against the red tower at TOWER for `frames` frames; after its `after`-th hit it is held `ticks`
/// ticks away out of reach, (3261, 17000), then put back.
fn scene(frames: u32, away: Option<(usize, u32)>) -> Vec<Frame> {
    let mut s = battle();
    s.spawn_unit_resolved(Team::Blue, "Ghost_EV1", n(GHOST.0, GHOST.1), None).expect("the form");
    s.tick();
    let ghost = find_live(&s, Team::Blue, "Ghost_EV1")[0].id;
    let tower = s.entities().find(|e| e.team == Team::Red && e.pos == n(TOWER.0, TOWER.1)).expect("the red left princess tower").id;
    let hs = s.cards().get(s.entity(ghost).unwrap().card_idx).hit_speed_ms;
    let (mut out, mut prog, mut hits, mut gone) = (Vec::new(), 0, 0, 0u32);
    for _ in 0..frames {
        let at = if gone > 0 { n(GHOST.0, 17000) } else { n(GHOST.0, GHOST.1) };
        gone = gone.saturating_sub(1);
        assert!(s.debug_set_pos(ghost, at));
        assert!(s.debug_set_hp(ghost, 1210));
        s.tick();
        let g = s.entity(ghost).expect("the form lives");
        let fired = g.attack_ms / hs > prog / hs;
        prog = g.attack_ms;
        let hidden = g.status_flags & 2 != 0;
        if fired {
            hits += 1;
            if let Some((after, ticks)) = away.filter(|(after, _)| *after == hits) {
                let _ = after;
                gone = ticks;
            }
        }
        let pair = s.entities().filter(|e| e.card.starts_with("Ghost_EV1_Summon")).map(|e| (e.id, e.pos, e.hp, e.deploying, e.status_flags & 2 != 0)).collect();
        out.push(Frame { tower: s.entity(tower).expect("the tower stands").hp, fired, hidden, pair });
    }
    out
}

#[test]
fn the_third_play_is_the_evolved_ghost() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "Ghost").expect("the ghost in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for _ in 0..2 {
        assert_eq!(next(&s), "Ghost", "a basic play");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "Ghost", n(14500, 3500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "Ghost_EV1", "the third play is the form");
}

#[test]
fn a_hit_while_hidden_puts_the_pair_down_on_the_next_tick() {
    let f = scene(80, None);
    let h = f.iter().position(|x| x.fired).expect("the form hits the tower");
    assert!(f[h - 1].hidden, "hidden up to its hit");
    assert_eq!(f[h - 1].tower - f[h].tower, 261, "the form's own hit");
    assert!(f[h].pair.is_empty(), "no pair on the hit's own tick");
    let p = &f[h + 1].pair;
    let points: Vec<Vec2> = p.iter().map(|x| x.1).collect();
    assert_eq!(points, vec![n(5492, 25328), n(1508, 25672)], "2000 either side of the tower, across the ghost's line");
    assert!(p.iter().all(|x| x.2 == 81), "hp 81 at level 11: {:?}", p.iter().map(|x| x.2).collect::<Vec<_>>());
    for k in 1..=16 {
        assert!(f[h + k].pair.iter().all(|x| x.3), "deploying on the hit + {k}");
    }
    assert!(f[h + 17].pair.iter().all(|x| !x.3), "deployed on the hit + 17");
}

#[test]
fn the_pair_starts_visible() {
    let f = scene(40, None);
    let h = f.iter().position(|x| x.fired).expect("the form hits the tower");
    assert!(f[h + 1].pair.iter().all(|x| !x.4), "visible from its first frame (StartWithBuffWhenNotAttacking false)");
}

#[test]
fn its_blow_strikes_what_it_hit_once_on_the_hits_tick_plus_6() {
    let f = scene(80, None);
    let h = f.iter().position(|x| x.fired).expect("the form hits the tower");
    let drops: Vec<i32> = (h + 1..h + 12).map(|k| f[k - 1].tower - f[k].tower).collect();
    assert_eq!(drops, vec![0, 0, 0, 0, 0, 81, 0, 0, 0, 0, 0], "from the hit + 1: the blow alone, 81, on the hit + 6");
}

#[test]
fn its_next_hits_make_none_until_it_has_hidden_again() {
    // After its second hit it stands out of reach 60 ticks, past its idle window (40 + 5 ticks), and comes back.
    let f = scene(220, Some((2, 60)));
    let hits: Vec<usize> = f.iter().enumerate().filter(|(_, x)| x.fired).map(|(k, _)| k).collect();
    assert!(hits.len() >= 3, "three hits: {hits:?}");
    assert_eq!(hits[1] - hits[0], 36, "its second hit one HitSpeed after the first");
    assert!(hits[2] - hits[1] > 45, "its third past its idle window: {hits:?}");
    let pairs_after = |k: usize| f[k + 1].pair.iter().filter(|x| !f[k].pair.iter().any(|y| y.0 == x.0)).count();
    assert_eq!((pairs_after(hits[0]), pairs_after(hits[1]), pairs_after(hits[2])), (2, 0, 2), "a pair on the first and the third hits");
}
