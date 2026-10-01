//! THE LITTLE PRINCE (tools/extract_cards.py `champion_ramp`, `champion_guard`; card.rs `RampDef`, `GuardDef`,
//! GUARD_SPAWN_OFFSET, GUARD_END_OFFSET, GUARD_DASH_START_TICKS, GUARD_STEP, GUARD_LANDING_TICKS; state.rs `RampRun`,
//! `ramp_pass`, `swing_started`, `ramp_shot`, `GuardRun`, `guard_release`, `guard_bind`, `guard_moves`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29; sp-champ-LittlePrince-s0 and Oracle's sp-lp-*-s0): his shots 24, 24, 12, 12,
//! 12, 8, 8 ... ticks apart, a Zap's stun starting the ramp again; the guard's first frame the press + 18 at his point +
//! (18, -1944), its first step + 25, 398 a tick, its last + 37 at his point + (239, 3083); a Knight 2962 from its path
//! losing 256 once.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! little_prince`): ramp_never, ramp_never_resets, guard_never, guard_never_charges, guard_collides,
//! early_trigger_late, guard_lands_loaded, guard_push_once (`his_guards_charge_pushes_by_a_ladder_rearmed_every_tick`
//! red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["LittlePrince", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (14500, 10500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Little Prince put at AT and held there 30 ticks, red `reds` put and held: the battle, his id and theirs.
fn scene(reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<(EntityId, Vec2)>) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let lp = s.scenario_spawn_now(Team::Blue, "LittlePrince", n(AT), None).expect("the Little Prince");
    let reds: Vec<(EntityId, Vec2)> = reds.iter().map(|(c, p)| (s.scenario_spawn_now(Team::Red, c, n(*p), None).expect("a red"), n(*p))).collect();
    for _ in 0..30 {
        hold(&mut s, lp, &reds);
        s.tick();
    }
    (s, lp, reds)
}

fn hold(s: &mut BattleState, lp: EntityId, reds: &[(EntityId, Vec2)]) {
    if s.entity(lp).is_some() {
        assert!(s.debug_set_pos(lp, n(AT)));
    }
    for (id, p) in reds {
        if let Some(e) = s.entity(*id) {
            let full = e.max_hp;
            assert!(s.debug_set_pos(*id, *p));
            assert!(s.debug_set_hp(*id, full));
        }
    }
}

/// The ticks on which the Little Prince's shots are made, over `ticks`, `zap_at` ticks in a red Zap on him.
fn shots(s: &mut BattleState, lp: EntityId, reds: &[(EntityId, Vec2)], ticks: u32, zap_at: Option<u32>) -> Vec<u32> {
    let mine = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(lp)).count();
    let mut known = mine(s);
    let mut out = Vec::new();
    for k in 0..ticks {
        hold(s, lp, reds);
        if zap_at == Some(k) {
            s.spawn_unit(Team::Red, "Zap", n(AT), None).expect("the Zap");
        }
        s.tick();
        let m = mine(s);
        if m > known {
            out.push(s.tick_count() - 1);
        }
        known = m;
    }
    out
}

fn gaps(t: &[u32]) -> Vec<u32> {
    t.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn his_shots_quicken_twice_after_his_third_and_sixth() {
    // A red Golem held 5000 ahead (in his reach, too big to die), put down after his hold: his first shot is counted.
    let (mut s, lp, _) = scene(&[]);
    let at = (AT.0, AT.1 + 5000);
    let reds = vec![(s.scenario_spawn_now(Team::Red, "Golem", n(at), None).expect("a red Golem"), n(at))];
    let t = shots(&mut s, lp, &reds, 220, None);
    let g = gaps(&t);
    assert!(g.len() >= 8, "his shots: {t:?}");
    assert_eq!(&g[..8], &[24, 24, 12, 12, 12, 8, 8, 8], "his shots' gaps: {t:?}");
}

#[test]
fn a_stun_starts_his_ramp_again() {
    let (mut s, lp, reds) = scene(&[("Golem", (AT.0, AT.1 + 5000))]);
    let p0 = s.tick_count();
    let t = shots(&mut s, lp, &reds, 260, Some(120));
    let g = gaps(&t);
    // From the first shot after the Zap's tick (p0 + 120; its stun holds the swing under way), the gaps run 24, 24, 12
    // again: measured, 363, 387, 411, 423 after a Zap on t336.
    let after = t.iter().position(|x| *x > p0 + 120).expect("shots after the Zap");
    let g2 = gaps(&t[after..]);
    assert!(g2.len() >= 3 && g2[..3] == [24, 24, 12], "after the Zap: {g2:?} (all: {g:?})");
}

#[test]
fn his_guard_appears_behind_him_and_charges_to_a_fixed_point_ahead() {
    let (mut s, lp, _) = scene(&[]);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows: Vec<(u32, Vec2, bool)> = Vec::new();
    for _ in 0..50 {
        assert!(s.debug_set_pos(lp, n(AT)));
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, "ChampionGuard").first() {
            rows.push((s.tick_count() - 1 - p, e.pos, e.deploying));
        }
    }
    let first = rows.first().expect("a guard");
    assert_eq!(first.0, 18, "its first frame, from the press: {rows:?}");
    assert_eq!(first.1, n((AT.0 + 18, AT.1 - 1944)), "its first point: {rows:?}");
    let at = |k: u32| rows.iter().find(|r| r.0 == k).map(|r| r.1).expect("a row");
    assert_eq!(at(24), first.1, "still on the press + 24: {rows:?}");
    assert!(at(25).y > first.1.y, "its first step on the press + 25: {rows:?}");
    assert_eq!(at(37), n((AT.0 + 239, AT.1 + 3083)), "its end on the press + 37: {rows:?}");
    assert_ne!(at(36), at(37), "its last step on the press + 37: {rows:?}");
}

/// THE GUARD'S FIRST HIT after its charge: a whole swing on, its load timer full as it comes free. Measured on client
/// 15.535.29 (Oracle's sp-lp-far-s0: the press P = t122, the guard free P + 39, a Knight at its end losing 232 (91 at level
/// 11) first on P + 62, then every 24 ticks).
#[test]
fn his_guards_first_hit_after_its_charge_is_a_whole_swing_on() {
    // A red Knight held 1500 beyond the guard's end (in its reach); Blue's princess towers down.
    let at = (AT.0 + 239, AT.1 + 3083 + 1500);
    let (mut s, lp, reds) = scene(&[("Knight", at)]);
    let knight = reds[0].0;
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut losses = Vec::new();
    for _ in 0..100 {
        assert!(s.debug_set_pos(lp, n(AT)));
        let full = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(knight, n(at)));
        assert!(s.debug_set_hp(knight, full));
        s.tick();
        let lost = full - s.entity(knight).expect("the Knight").hp;
        if lost > 0 {
            losses.push((s.tick_count() - 1 - p, lost));
        }
    }
    // The guard's hits are 232; his own shots at the Knight the other losses (232 with one of them on a tick).
    let shots: Vec<i32> = losses.iter().map(|l| l.1).filter(|l| *l < 232).collect();
    let guard: Vec<u32> = losses.iter().filter(|l| l.1 == 232 || shots.contains(&(l.1 - 232))).map(|l| l.0).collect();
    assert_eq!(guard.first(), Some(&62), "the guard's first hit, from the press: {losses:?}");
}

#[test]
fn his_guards_charge_hits_a_knight_near_its_path_once() {
    // A red Knight held 2400 to the right of the guard's path.
    let (mut s, lp, reds) = scene(&[("Knight", (AT.0 + 2400, AT.1 + 1000))]);
    let knight = reds[0].0;
    // Blue's princess towers down (their arrows reach the Knight): his own shots are its other losses.
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut losses = Vec::new();
    for _ in 0..45 {
        assert!(s.debug_set_pos(lp, n(AT)));
        assert!(s.debug_set_pos(knight, reds[0].1));
        let before = s.entity(knight).expect("the Knight").hp;
        s.tick();
        let after = s.entity(knight).expect("the Knight").hp;
        if after < before {
            losses.push(before - after);
        }
    }
    // The charge's hit is the one loss of 256 or more: 256, or 256 and one of his shots on its tick.
    let (hits, shots): (Vec<i32>, Vec<i32>) = losses.iter().partition(|l| **l >= 256);
    assert_eq!(hits.len(), 1, "one charge hit: {losses:?}");
    assert!(hits[0] == 256 || shots.contains(&(hits[0] - 256)), "a charge hit of 256: {losses:?}");
}

/// THE GUARD'S PUSH (state.rs `guard_moves`, `rearm_ladder`): the knockback ladder, re-armed from the guard's point on
/// every tick of its charge for a troop whose centre is within 2500 of it, m = 2500 - d, the new ladder replacing the
/// running one unless its start is the smaller; after the charge the last ladder runs out. Read off the client code by
/// Oracle and checked tick for tick on client 15.535.29 (sp-lp-live-s0: 25 on t213, then 125, 150, 174, 199, 224 ...).
/// Here a red Knight beside the guard's path is held until the guard is first within 2500 of it, then each of its steps
/// is the ladder's from the frame before, to 3.
#[test]
fn his_guards_charge_pushes_by_a_ladder_rearmed_every_tick() {
    let (mut s, lp, reds) = scene(&[("Knight", (AT.0 + 800, AT.1 + 1500))]);
    let knight = reds[0].0;
    let dist = |a: Vec2, b: Vec2| -> i64 {
        let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
        royalesim::fixed::isqrt(dx * dx + dy * dy)
    };
    s.press_ability_button(Team::Blue, 0).expect("the press");
    // (the Knight, the guard) at the end of each tick from the one the guard first stands within 2500 of it.
    let mut rows: Vec<(Vec2, Vec2)> = Vec::new();
    let mut held = true;
    for _ in 0..70 {
        if s.entity(lp).is_some() {
            assert!(s.debug_set_pos(lp, n(AT)));
        }
        if held {
            assert!(s.debug_set_pos(knight, reds[0].1));
        }
        s.tick();
        let Some(g) = find_live(&s, Team::Blue, "ChampionGuard").first().map(|e| e.pos) else { continue };
        let k = s.entity(knight).expect("the Knight").pos;
        if held && dist(g, k) < 2500 {
            held = false;
        }
        if !held {
            rows.push((k, g));
        }
    }
    assert!(rows.len() > 20, "the scene drifted: the guard never came within 2500 of the Knight");
    let mut cur = 0;
    let mut checked = Vec::new();
    for i in 1..rows.len() {
        let (k0, g0) = rows[i - 1];
        // The guard charged on frame i - 1 when it moved on it (the first row's frame it moved into reach).
        let charging = i == 1 || rows[i - 2].1 != g0;
        if charging && dist(g0, k0) < 2500 {
            let start = royalesim::move16402::ladder_speed(2500 - dist(g0, k0) as i32);
            if start >= cur {
                cur = start;
            }
        }
        cur -= 25;
        if cur < 0 {
            break;
        }
        let step = dist(rows[i].0, k0) as i32;
        assert!((step - cur).abs() <= 3, "row {i}: a step of {step}, the ladder's {cur}; checked {checked:?}");
        checked.push(step);
    }
    assert!(checked.len() >= 8, "the scene drifted: {} ladder steps checked: {checked:?}", checked.len());
}
