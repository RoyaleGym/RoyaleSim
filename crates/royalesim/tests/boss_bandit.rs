//! THE BOSS BANDIT'S BUTTON (tools/extract_cards.py `champion_warp_back`; card.rs `WarpBackDef`; state.rs `MagicRun`,
//! `magic_warps`, `HeroUnit::uses`, `charges_left`, EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-champ-BossBandit-s0; the press issued t175 mid-dash, the cast
//! t178..t194): charges 2 -> 1 and the cooldown 3000 on the press; 6000 back on t195 (the trigger t181, + 700 ms), its
//! target dropped; the button back on t238, the cast's start + 60, with one charge left.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! boss_bandit`): warp_back_never, champion_charges_unread, early_trigger_late, press_during_dash, warp_cloak_on_trigger
//! (`under_client16402_trigger_plus_1_her_holder_lets_her_go_a_tick_later` red), cloak_holders_drop_now
//! (`under_client16402_holders_drop_plus_1_her_holder_lets_her_go_a_tick_later` red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError, WarpBackCloak};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["BossBandit", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (3500, 12500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Boss Bandit put at AT, alone, and held there 40 ticks: the battle and his id.
fn scene() -> (BattleState, EntityId) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let bb = s.scenario_spawn_now(Team::Blue, "BossBandit", n(AT), None).expect("the Boss Bandit");
    for _ in 0..40 {
        assert!(s.debug_set_pos(bb, n(AT)));
        s.tick();
    }
    (s, bb)
}

#[test]
fn its_press_warps_it_6000_back_700_ms_after_its_trigger() {
    // A free press on P: the cast from P + 1, the trigger P + 4 (TriggerDelay 200 less a tick), the warp P + 18.
    let (mut s, bb) = scene();
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = Vec::new();
    for _ in 0..24 {
        s.tick();
        let e = s.entity(bb).expect("the Boss Bandit");
        rows.push((s.tick_count() - 1 - p, e.pos.y / K, e.target));
    }
    let moved = rows.iter().find(|r| r.1 < AT.1 - 3000).map(|r| r.0);
    assert_eq!(moved, Some(18), "the warp's tick from the press: {rows:?}");
    let r = rows.iter().find(|r| r.0 == 18).expect("row 18");
    assert!((AT.1 - 6000 - r.1).abs() <= 150 && r.2.is_none(), "6000 back, its target dropped: {rows:?}");
}

/// A point's distance from another, native.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
    royalesim::fixed::isqrt(dx * dx + dy * dy)
}

/// A PRESS MID-DASH waits for the dash's end: measured on client 15.535.29 (sp-champ-BossBandit-s0: the press issued t175
/// mid-dash, the dash's last frame t177, the cast from t178, the warp on t195, the cast's start + 17).
#[test]
fn a_press_mid_dash_waits_for_the_dash_to_end() {
    // tests/dash_attack.rs's geometry: a red Knight walking to the blue right princess tower, the Boss Bandit after it from
    // beyond its trigger distance.
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let bb = s.scenario_spawn_now(Team::Blue, "BossBandit", n((8200, 6200)), None).expect("the Boss Bandit");
    s.scenario_spawn_now(Team::Red, "Knight", n((14231, 9500)), None).expect("a red Knight");
    // Its dash under way: a tick it steps more than 300 (its walk is under 100 a tick).
    let mut at = s.entity(bb).expect("the Boss Bandit").pos;
    let mut k = 0;
    loop {
        s.tick();
        let p = s.entity(bb).expect("the Boss Bandit").pos;
        if dist(p, at) > 300 {
            break;
        }
        at = p;
        k += 1;
        assert!(k < 200, "the Boss Bandit never dashed");
    }
    // The dash's end with no press, on a twin: the first tick it steps 300 or less.
    let mut twin = s.clone();
    let mut end = None;
    let mut at = twin.entity(bb).expect("the Boss Bandit").pos;
    for _ in 0..40 {
        twin.tick();
        let p = twin.entity(bb).expect("the Boss Bandit").pos;
        if dist(p, at) <= 300 {
            end = Some(twin.tick_count() - 1);
            break;
        }
        at = p;
    }
    let end = end.expect("the dash's end");
    let p0 = s.tick_count() - 1;
    assert!(end > p0 + 1, "a press mid-dash: pressed on {p0}, the dash's end on {end}");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press, mid-dash");
    // The warp: the tick it moves 3000 or more.
    let mut warp = None;
    let mut at = s.entity(bb).expect("the Boss Bandit").pos;
    for _ in 0..60 {
        s.tick();
        let Some(e) = s.entity(bb) else { break };
        if warp.is_none() && dist(e.pos, at) >= 3000 {
            warp = Some(s.tick_count() - 1);
        }
        at = e.pos;
    }
    assert_eq!(warp, Some(end + 17), "the warp the cast's start + 17, the cast from the dash's end {end} (pressed on {p0})");
}

#[test]
fn it_has_two_charges_the_second_after_a_3000_ms_cooldown() {
    let (mut s, bb) = scene();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the first press");
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilityNotReady), "a charge left, behind its cooldown");
    // The cast starts on P + 1; its cooldown gives the button back 60 ticks on.
    let mut back = None;
    for _ in 0..80 {
        assert!(s.debug_set_pos(bb, n(AT)));
        s.tick();
        if back.is_none() && s.check_ability_button(Team::Blue, 0).is_ok() {
            back = Some(s.tick_count() - p);
        }
    }
    assert_eq!(back, Some(62), "the button back after the tick P + 61 ran (the cast's start P + 1, + 60)");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the second press");
    for _ in 0..80 {
        s.tick();
    }
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilitySpent), "two charges, both used");
}

/// The Boss Bandit at AT under status.WARP_BACK_CLOAK = `arm`, a Red Knight held 1500 ahead of her and holding her, pressed
/// free: (the tick, from the press, the Knight first holds another target or none, the tick she warps).
fn holder_lets_go(arm: WarpBackCloak) -> (u32, u32) {
    let mut cfg = config();
    cfg.calib.warp_back_cloak = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let bb = s.scenario_spawn_now(Team::Blue, "BossBandit", n(AT), None).expect("the Boss Bandit");
    let at_k = (AT.0, AT.1 + 1500);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(at_k), None).expect("the Knight");
    for _ in 0..40 {
        assert!(s.debug_set_pos(bb, n(AT)) && s.debug_set_pos(knight, n(at_k)));
        s.tick();
    }
    assert_eq!(s.entity(knight).expect("the Knight").target, Some(bb), "the scene: the Knight holds the Boss Bandit at the press");
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut let_go, mut warp) = (None, None);
    for _ in 0..24 {
        if warp.is_none() {
            assert!(s.debug_set_pos(knight, n(at_k)));
        }
        s.tick();
        let age = s.tick_count() - 1 - p;
        if let_go.is_none() && s.entity(knight).expect("the Knight").target != Some(bb) {
            let_go = Some(age);
        }
        if warp.is_none() && s.entity(bb).expect("the Boss Bandit").pos.y / K < AT.1 - 3000 {
            warp = Some(age);
        }
    }
    (let_go.expect("the Knight never let her go"), warp.expect("she never warped"))
}

/// status.WARP_BACK_CLOAK = client16402_trigger_plus_1 (client 16.402, parity's r71 census: her holders let her go on the
/// press + 4, 19 of 21, the engine's on + 3; her warp on + 17 in both): the Knight holding her lets her go a tick later
/// than under with_the_trigger, and she warps on the same tick. Plant: warp_cloak_on_trigger.
#[test]
fn under_client16402_trigger_plus_1_her_holder_lets_her_go_a_tick_later() {
    let (old_go, old_warp) = holder_lets_go(WarpBackCloak::WithTheTrigger);
    let (new_go, new_warp) = holder_lets_go(WarpBackCloak::Client16402TriggerPlus1);
    assert_eq!(new_go, old_go + 1, "client16402_trigger_plus_1: the holder lets her go a tick later ({old_go} -> {new_go})");
    assert_eq!(new_warp, old_warp, "the warp's tick is the same under both arms");
    assert!(new_go < new_warp, "the cloak drops the holder before the warp ({new_go}, {new_warp})");
}

/// status.WARP_BACK_CLOAK = client16402_holders_drop_plus_1 (client 16.402, parity's r73 census: her holders let her go on the
/// press + 4, 10 of 10, the engine's on + 3): the Knight holding her lets her go a tick later than under with_the_trigger,
/// and she warps on the same tick. Plant: cloak_holders_drop_now.
#[test]
fn under_client16402_holders_drop_plus_1_her_holder_lets_her_go_a_tick_later() {
    let (old_go, old_warp) = holder_lets_go(WarpBackCloak::WithTheTrigger);
    let (new_go, new_warp) = holder_lets_go(WarpBackCloak::Client16402HoldersDropPlus1);
    assert_eq!(new_go, old_go + 1, "client16402_holders_drop_plus_1: the holder lets her go a tick later ({old_go} -> {new_go})");
    assert_eq!(new_warp, old_warp, "the warp's tick is the same under both arms");
}

/// The Boss Bandit put at `me` with a Red Knight held 1,200 behind her (her target, so she never dashes at a Red princess
/// tower), held 40 ticks, under status.WARP_BACK_LANDING = `landing`, pressed: her point on the tick she warps (native).
fn warped_to(landing: royalesim::state::WarpBackLanding, me: (i32, i32)) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.warp_back_landing = landing;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let bb = s.scenario_spawn_now(Team::Blue, "BossBandit", n(me), None).expect("the Boss Bandit");
    let kn_at = (me.0, me.1 - 1200);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(kn_at), None).expect("the Knight");
    let hold = |s: &mut BattleState| {
        let (hb, hk) = (s.entity(bb).expect("her").max_hp, s.entity(knight).expect("the Knight").max_hp);
        assert!(s.debug_set_pos(knight, n(kn_at)) && s.debug_set_hp(knight, hk) && s.debug_set_hp(bb, hb));
    };
    for _ in 0..40 {
        hold(&mut s);
        assert!(s.debug_set_pos(bb, n(me)));
        s.tick();
    }
    assert_eq!(s.entity(bb).expect("her").target, Some(knight), "the scene: she holds the Knight at the press");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..30 {
        hold(&mut s);
        assert!(s.debug_set_pos(bb, n(me)));
        let before = s.entity(bb).expect("the Boss Bandit").pos;
        s.tick();
        let p = s.entity(bb).expect("the Boss Bandit").pos;
        if (before.y - p.y) / K > 3000 {
            return (p.x / K, p.y / K);
        }
    }
    panic!("she never warped");
}

/// status.WARP_BACK_LANDING = client16402_nearest_bank_half_row (client 16.402, parity's r74 census: 3 of 3 warps onto the river
/// stood on the nearer bank's first land half-row centre): pressed at y 22200, her 6,000 back falls on the river (16200),
/// and she stands on (x, 17250); under nominal_point (the vacuity check) on the water. Plant: warp_lands_on_water.
#[test]
fn under_client16402_nearest_bank_half_row_her_warp_onto_the_river_lands_on_the_bank() {
    use royalesim::state::WarpBackLanding as W;
    let old = warped_to(W::NominalPoint, (9000, 22200));
    assert!((15000..17000).contains(&old.1), "nominal_point (the vacuity check): she is not on the river ({old:?})");
    let new = warped_to(W::Client16402NearestBankHalfRow, (9000, 22200));
    assert_eq!(new, (old.0, 17250), "client16402_nearest_bank_half_row: not on the nearer bank's half-row centre");
}

/// The Boss Bandit `d` south of the Red left princess tower's centre (its lane, the bridge's column), a Red Knight held 1,200
/// behind her (her target), both held, under status.WARP_BACK_CLOAK = `arm`, pressed on the (40 + `wait`)th tick: whether a
/// tower arrow flies at her on the tick of her warp, after it.
fn shot_after_warp(arm: WarpBackCloak, d: i32, wait: u32) -> bool {
    use royalesim::entity::EntityKind;
    let mut cfg = config();
    cfg.calib.warp_back_cloak = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let tower = s.entities().filter(|e| e.team == Team::Red && e.kind == EntityKind::PrincessTower).min_by_key(|e| e.pos.x).expect("a Red princess tower").pos;
    let me = (tower.x / K, tower.y / K - d);
    let kn_at = (me.0, me.1 - 1200);
    let bb = s.scenario_spawn_now(Team::Blue, "BossBandit", n(me), None).expect("the Boss Bandit");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(kn_at), None).expect("the Knight");
    let hold = |s: &mut BattleState| {
        let (hb, hk) = (s.entity(bb).expect("her").max_hp, s.entity(knight).expect("the Knight").max_hp);
        assert!(s.debug_set_pos(knight, n(kn_at)) && s.debug_set_hp(knight, hk) && s.debug_set_hp(bb, hb));
    };
    for _ in 0..40 + wait {
        hold(&mut s);
        assert!(s.debug_set_pos(bb, n(me)));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..30 {
        hold(&mut s);
        let before = s.entity(bb).expect("her").pos;
        s.tick();
        let p = s.entity(bb).expect("her").pos;
        if (before.y - p.y) / K > 3000 {
            return s.projectiles().iter().any(|q| q.target == bb);
        }
        assert!(s.debug_set_pos(bb, n(me)));
    }
    panic!("she never warped");
}

/// status.WARP_BACK_CLOAK = client16402_holders_drop_plus_1_shots_lost_at_warp (client 16.402, parity's r74 census: a tower's
/// arrow released on the press + 3, whose flight crossed her warp on the press + 17, never landed): over her distances from the
/// tower 8,000 to 10,500 and the press's 16 phases of its swing, no arrow flies at her after her warp; under
/// client16402_holders_drop_plus_1 (the vacuity check) one does at some. Plant: warp_keeps_shots.
#[test]
fn under_client16402_holders_drop_plus_1_shots_lost_at_warp_no_shot_follows_her_warp() {
    let (mut old, mut new) = (0, 0);
    for d in (8000..=10500).step_by(250) {
        for wait in 0..16 {
            old += usize::from(shot_after_warp(WarpBackCloak::Client16402HoldersDropPlus1, d, wait));
            new += usize::from(shot_after_warp(WarpBackCloak::Client16402HoldersDropPlus1ShotsLostAtWarp, d, wait));
        }
    }
    assert!(old > 0, "client16402_holders_drop_plus_1 (the vacuity check): no shot was flying at her when she warped");
    assert_eq!(new, 0, "client16402_holders_drop_plus_1_shots_lost_at_warp: a shot flew at her after her warp");
}
