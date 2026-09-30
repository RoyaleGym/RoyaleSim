//! THE EVO MINION HORDE (card.rs `EvoDef::first_hit`; status.rs `BuffDef::no_damage`; combat.rs `resolve`; state.rs
//! `first_hit`), at level 11.
//!
//! Read off the table (characters/minion_horde_ev1.toml): the second play of an evolved Minion Horde entry is the form
//! (DarkElixirCost 1); the first damage each minion survives lands MinionHorde_EV1_GhostBuff on it for 3000 ms, once in
//! its life: invisible, NO_DAMAGE, -33 % speed and hit speed.
//!
//! THE MEASUREMENT (sp-form-MinionHorde-evo-s0): every evolved minion a Musketeer's shot took from 230 to 13 took no
//! other damage for a stretch and died later. The ghost's length and its first tick are the table's, not measured.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_minion_horde`):
//!   - ghost_never -> `a_zapped_minion_takes_nothing_from_the_next_zap` and `the_ghost_ends_3000_ms_on_and_comes_once`
//!     red;
//!   - ghost_takes_damage -> the same two red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// On blue's own side, with no red unit anywhere: no tower and no minion shoots.
const AT: (i32, i32) = (9000, 10000);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["MinionHorde".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// One evolved play's six minions put down at AT and held on their first points, a blue Knight (the control: it
/// shows each Zap land) 1500 east of the first minion; everyone's hitpoints topped up every tick. A red Zap is cast on
/// the first minion's point before the tick of each frame in `zaps` (it lands on that tick). Returns, per frame, what
/// each minion lost on it and what the Knight lost.
fn scene(zaps: &[usize], frames: usize) -> Vec<(Vec<i32>, i32)> {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "MinionHorde_EV1", n(AT.0, AT.1), None).expect("the horde");
    s.tick();
    let minions: Vec<(EntityId, Vec2, i32)> = find_live(&s, Team::Blue, "MinionHorde_EV1").iter().map(|e| (e.id, e.pos, e.max_hp)).collect();
    assert_eq!(minions.len(), 6, "six minions");
    let first = minions[0].1;
    let kp = Vec2::new(first.x + 1500 * K, first.y);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", kp, None).expect("the control Knight");
    let ktop = s.entity(knight).expect("the Knight").max_hp;
    let mut out = Vec::new();
    for k in 0..frames {
        for (id, p, top) in &minions {
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, *top));
        }
        assert!(s.debug_set_pos(knight, kp));
        assert!(s.debug_set_hp(knight, ktop));
        if zaps.contains(&k) {
            s.spawn_unit(Team::Red, "Zap", first, None).expect("a Zap");
        }
        s.tick();
        let lost = minions.iter().map(|(id, _, top)| top - s.entity(*id).expect("a minion held alive").hp).collect();
        out.push((lost, ktop - s.entity(knight).expect("the Knight held alive").hp));
    }
    out
}

#[test]
fn the_second_play_is_the_evolved_horde() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 1, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "MinionHorde").expect("the horde in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    assert_eq!(next(&s), "MinionHorde", "the first play is basic");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "MinionHorde", n(3500, 5500)).expect("the play");
    for _ in 0..20 {
        s.tick();
    }
    assert_eq!(next(&s), "MinionHorde_EV1", "the second play is the form");
}

#[test]
fn a_zapped_minion_takes_nothing_from_the_next_zap() {
    // The first Zap lands in full on every minion in its reach (the hit that lands the ghost is not stopped); 20 ticks
    // on, the second takes the Knight again and none of them.
    let f = scene(&[0, 20], 30);
    let z = f[0].1;
    assert!(z > 0, "the first Zap on the Knight");
    let hit: Vec<usize> = (0..6).filter(|m| f[0].0[*m] > 0).collect();
    assert!(hit.len() >= 2, "the first Zap took two minions or more: {:?}", f[0].0);
    for m in &hit {
        assert_eq!(f[0].0[*m], f[0].0[hit[0]], "one Zap, one loss: {:?}", f[0].0);
    }
    assert_eq!(f[20].1, z, "the second Zap on the Knight");
    for (k, (lost, _)) in f.iter().enumerate().skip(1) {
        for m in &hit {
            assert_eq!(lost[*m], 0, "minion {m} took {} on frame {k}, in its ghost", lost[*m]);
        }
    }
}

#[test]
fn the_ghost_ends_3000_ms_on_and_comes_once() {
    // Zaps on frames 0, 55, 65 and 75: the one on 55 is inside the ghost (60 ticks from frame 0), the one on 65 after
    // it, and the one on 75 lands too (the ghost comes once in a minion's life).
    let f = scene(&[0, 55, 65, 75], 80);
    let hit: Vec<usize> = (0..6).filter(|m| f[0].0[*m] > 0).collect();
    assert!(hit.len() >= 2, "the first Zap took two minions or more: {:?}", f[0].0);
    let z = f[0].0[hit[0]];
    for k in [55, 65, 75] {
        assert!(f[k].1 > 0, "the Zap on frame {k} on the Knight");
    }
    for m in &hit {
        assert_eq!(f[55].0[*m], 0, "minion {m} inside its ghost on frame 55");
        assert_eq!(f[65].0[*m], z, "minion {m} after its ghost on frame 65");
        assert_eq!(f[75].0[*m], z, "minion {m} with no second ghost on frame 75");
    }
}
