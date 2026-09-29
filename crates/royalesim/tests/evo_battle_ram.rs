//! THE EVO BATTLE RAM (card.rs `RamDef`, `RamPushDef`, `HitRageDef`; state.rs `ram_pass`, `charge_pass`,
//! `evo_after_fire`), against client 15.535.29 at level 11.
//!
//! THE MEASUREMENTS (Oracle's scenes: sp-ram-alone, -bldg, -side+, -side-, -kill, the five sp-ram-v-* and the two
//! sp-ram-w-* runs; the ram put down at (3500, 12500) runs up the left lane at the red princess tower):
//!   - sp-ec-BattleRam: the third play of an evolved Battle Ram entry puts the form down (DarkElixirCost 2);
//!   - the push switches on 16 ticks after the charge starts (C: the first charged step comes on C + 1): a Knight
//!     walking into the ram, inside the area from t934, takes the 212 (83 on the ladder) on t937 = C + 16, once;
//!   - a Knight or a Musketeer 238 aside, struck, steps straight away from the ram's line from the next tick: 250 x 4,
//!     225, 200, ... (the knockback ladder of PushBackStrength 2500); the Giant (IgnorePushback) takes the 212 and
//!     never moves;
//!   - sp-ram-bldg-s0: the ram hits an Elixir Collector for 573 (224 on the ladder, its DamageSpecial), lives, recoils
//!     (249 x 3, 224, 199, ... 24: AttackPushBack 2000), charges on at 119-120 a tick from the tick after the ladder,
//!     and hits it again 29 ticks after the first;
//!   - sp-ram-alone-s0: the ram's death puts down two Barbarian_EV1 (716 hp); the first one's first hit lands on t1111
//!     (progress 1400) and its progress steps 65 from t1112 (50 x 130 / 100, Barbarian_EVO_Rage).
//!
//! THE SCENES here put the form down with spawn_unit at (3500, 12500) on Blue's side, both sides at level 11, Blue's
//! princess towers down so that nothing but the ram touches a red troop.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_battle_ram`):
//!   - ram_push_at_charge_start -> `the_push_switches_on_16_ticks_into_the_charge_and_strikes_once` red (struck on C);
//!   - ram_push_never -> `the_push_switches_on_16_ticks_into_the_charge_and_strikes_once` and
//!     `a_struck_troop_steps_straight_off_the_rams_line` red;
//!   - ram_charge_consumed and ram_recoil_clears_charge -> `the_ram_lives_through_its_hit_recoils_and_charges_on` red
//!     (it walks after its recoil);
//!   - hit_rage_never -> `its_barbarians_rage_from_the_tick_after_their_first_hit` red (their progress steps 50).
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

/// A battle whose Blue deck is the Battle Ram, evolved, and a Knight; both sides at level 11, past the opening lockout,
/// Blue's princess towers down.
fn battle() -> BattleState {
    let mut cfg = config();
    cfg.decks = [vec!["BattleRam".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    s
}

/// The form put down at (3500, 12500) and run until the tick its charge starts (the first frame reading charged): the
/// battle and the ram.
fn charging_ram() -> (BattleState, EntityId) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "BattleRam_EV1", n(3500, 12500), None).expect("the form");
    s.tick();
    let ram = find_live(&s, Team::Blue, "BattleRam_EV1")[0].id;
    for _ in 0..200 {
        if s.entity(ram).is_some_and(|e| e.charged) {
            return (s, ram);
        }
        s.tick();
    }
    panic!("the ram never charges");
}

fn step(a: Vec2, b: Vec2) -> (i32, i32) {
    let d = b.sub(a);
    (d.x / K, d.y / K)
}

#[test]
fn the_third_play_is_the_evolved_ram() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "BattleRam").expect("the ram in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    for _ in 0..2 {
        assert_eq!(next(&s), "BattleRam", "a basic play");
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "BattleRam", n(14500, 3500)).expect("the play");
        for _ in 0..20 {
            s.tick();
        }
    }
    assert_eq!(next(&s), "BattleRam_EV1", "the third play is the form");
}

/// A red Knight put down at `off` (aside, ahead) of the ram on frame `from` of its charge (frame 0: the first frame
/// reading charged), at that exact point (`spawn_unit_resolved`), and held there, relative to the ram, on every frame
/// through `hold`: the Knight's hp and point and the ram's point on each frame from 0 to `frames` (None before the
/// Knight exists).
#[allow(clippy::type_complexity)]
fn knight_beside(off: (i32, i32), from: u32, hold: u32, frames: u32) -> Vec<Option<(i32, Vec2, Vec2)>> {
    let (mut s, ram) = charging_ram();
    let at = |s: &BattleState| {
        let p = s.entity(ram).expect("the ram lives").pos;
        Vec2::new(p.x + off.0 * K, p.y + off.1 * K)
    };
    let mut knight: Option<EntityId> = None;
    let mut out = vec![None];
    for k in 1..=frames {
        if k == from {
            s.spawn_unit_resolved(Team::Red, "Knight", at(&s), None).expect("the Knight");
        }
        if let Some(id) = knight.filter(|_| k <= hold) {
            let p = at(&s);
            assert!(s.debug_set_pos(id, p));
        }
        s.tick();
        if knight.is_none() && k >= from {
            knight = find_live(&s, Team::Red, "Knight").first().map(|e| e.id);
        }
        let rp = s.entity(ram).expect("the ram lives").pos;
        out.push(knight.map(|id| {
            let e = s.entity(id).expect("the Knight lives");
            (e.hp, e.pos, rp)
        }));
    }
    out
}

#[test]
fn the_push_switches_on_16_ticks_into_the_charge_and_strikes_once() {
    // Held 1200 aside and 700 ahead from the charge's start to frame 30 (inside the area: 1204 from the point 800 ahead,
    // against 1000 and its radius 500; clear of the ram's body and its path): struck on frame 16 alone.
    let fr = knight_beside((1200, 700), 1, 30, 30);
    let hp: Vec<i32> = fr.iter().flatten().map(|f| f.0).collect();
    assert_eq!(hp[0], 1766, "the Knight is created whole");
    let drops: Vec<(usize, i32)> = (1..fr.len())
        .filter_map(|k| match (fr[k - 1], fr[k]) {
            (Some(a), Some(b)) if b.0 < a.0 => Some((k, a.0 - b.0)),
            _ => None,
        })
        .collect();
    assert_eq!(drops, vec![(16, 212)], "one strike of 212 (83 x 256 / 100), 16 ticks into the charge");
}

#[test]
fn a_struck_troop_steps_straight_off_the_rams_line() {
    // Oracle's sp-ram-v-Knight-3500-s0: a Knight put down 238 aside and about 1,965 ahead of the charging ram, the push
    // already on, is struck on its first frame and steps straight off the ram's line from the next.
    let fr = knight_beside((238, 1965), 20, 0, 30);
    let first = (0..fr.len()).find(|&k| fr[k].is_some()).expect("the Knight is created");
    assert_eq!(fr[first].unwrap().0, 1766 - 212, "struck on its first frame");
    let at = |k: usize| fr[k].unwrap();
    let steps: Vec<(i32, i32)> = (first + 1..=first + 6).map(|k| step(at(k - 1).1, at(k).1)).collect();
    let ram_dx: Vec<i32> = (first + 1..=first + 6).map(|k| step(at(k - 1).2, at(k).2).0).collect();
    assert!(ram_dx.iter().all(|d| d.abs() <= 2), "the ram holds its line: {ram_dx:?}");
    assert!(steps.iter().all(|s| s.1.abs() <= 5), "straight off the line: {steps:?}");
    let dx: Vec<i32> = steps.iter().map(|s| s.0).collect();
    for (got, want) in dx.iter().zip([250, 250, 250, 250, 225, 200]) {
        assert!((got - want).abs() <= 2, "the ladder's steps: {dx:?}");
    }
}

#[test]
fn the_ram_lives_through_its_hit_recoils_and_charges_on() {
    let mut s = battle();
    s.scenario_spawn_now(Team::Red, "Elixir Collector", n(3500, 20500), None).expect("the Collector");
    s.spawn_unit(Team::Blue, "BattleRam_EV1", n(3500, 12500), None).expect("the form");
    s.tick();
    let ram = find_live(&s, Team::Blue, "BattleRam_EV1")[0].id;
    let col = find_live(&s, Team::Red, "Elixir Collector")[0].id;
    let mut fr: Vec<(Option<Vec2>, i32)> = Vec::new();
    for _ in 0..220 {
        s.tick();
        fr.push((s.entity(ram).map(|e| e.pos), s.entity(col).map_or(0, |e| e.hp)));
    }
    // The Collector's drops of more than its own decay: the ram's hits.
    let hits: Vec<usize> = (1..fr.len()).filter(|&k| fr[k - 1].1 - fr[k].1 > 100).collect();
    assert!(hits.len() >= 2, "the ram hits the Collector twice: {hits:?}");
    let h = hits[0];
    // 573 (224 x 256 / 100) and the Collector's own decay of 1 on some ticks: 574 in sp-ram-bldg-s0.
    assert!((573..=574).contains(&(fr[h - 1].1 - fr[h].1)), "a charged hit: its DamageSpecial, 224 on the ladder");
    assert!(fr[h..hits[1]].iter().all(|f| f.0.is_some()), "the ram lives through its hit");
    let st: Vec<i32> = (h + 1..hits[1]).map(|k| {
        let (a, b) = (fr[k - 1].0.unwrap(), fr[k].0.unwrap());
        let (dx, dy) = step(a, b);
        ((dx as f64).hypot(dy as f64)) as i32
    }).collect();
    // The recoil's ladder, then the charge at once: every step after the ladder's back-step is a charged one.
    // The recoil's ladder (AttackPushBack 2000), its first step on the hit's own frame: from the next frame the recorded
    // steps (sp-ram-bldg-s0, t957 on) read 249, 249, 224, 199, 174, 149, 125, 100, 74, 49, 24.
    let recorded = [249, 249, 224, 199, 174, 149, 125, 100, 74, 49, 24];
    assert!(st.len() > recorded.len() && st.iter().zip(recorded).all(|(g, w)| (g - w).abs() <= 2), "the recoil's ladder (2000): {st:?}");
    let back = st.iter().rposition(|v| *v <= 30).expect("the recoil's ladder ends");
    let after: Vec<i32> = st[back + 1..].to_vec();
    assert!(!after.is_empty() && after.iter().all(|v| (110..=125).contains(v)), "charged steps after the recoil: {st:?}");
}

#[test]
fn its_barbarians_rage_from_the_tick_after_their_first_hit() {
    let (mut s, ram) = charging_ram();
    // The ram at 1 hp: the red princess tower's first shot kills it on the way.
    assert!(s.debug_set_hp(ram, 1));
    let mut barbs: Vec<EntityId> = Vec::new();
    let mut prog: Vec<Vec<i32>> = Vec::new();
    for _ in 0..400 {
        s.tick();
        if barbs.is_empty() {
            barbs = find_live(&s, Team::Blue, "Barbarian_EV1").iter().map(|e| e.id).collect();
            if !barbs.is_empty() {
                assert_eq!(barbs.len(), 2, "two Barbarian_EV1");
                assert!(barbs.iter().all(|b| s.entity(*b).unwrap().max_hp == 716), "716 hp at level 11");
                prog = vec![Vec::new(); 2];
            }
            continue;
        }
        for (m, b) in barbs.iter().enumerate() {
            prog[m].push(s.entity(*b).map_or(-1, |e| e.attack_ms));
        }
    }
    assert!(!barbs.is_empty(), "the ram dies into its Barbarians");
    let p = &prog[0];
    let hit = p.iter().position(|v| *v >= 1400).expect("the first Barbarian hits");
    assert_eq!(p[hit + 1] - p[hit], 65, "raged from the tick after its first hit (50 x 130 / 100): {:?}", &p[hit.saturating_sub(3)..hit + 3]);
    assert!(p[hit.saturating_sub(4)..hit].windows(2).all(|w| w[1] - w[0] == 50), "unraged before it: {:?}", &p[hit.saturating_sub(4)..hit + 1]);
}
