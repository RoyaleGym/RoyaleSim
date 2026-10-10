//! THE MONK'S DEFLECT: a champion button (card.rs `AbilityEffect::Deflect`, state.rs `fire_ability` and `deflects`,
//! combat.rs `step_projectiles`).
//!
//! The law, measured on client 15.535.29 (sp-champ-Monk-s0 against its no-press twin, and sp-champ-Monk-recharge-q20):
//!   - the press is taken, 1 elixir; he casts from the press's first frame P for 17 ticks, then his Deflect is active
//!     for 79 (P + 17 .. P + 95); he stands, and attacks nothing, through both;
//!   - every hit on him while it is active lands at 35 % (his ShieldBoostMonk's DamageReduction 65, truncated: a
//!     Musketeer's 217 -> 75);
//!   - every enemy shot landing on him while it is active also goes back at its firer for its full damage (a
//!     Musketeer lost 217 six ticks after each 75 the Monk took, three of three);
//!   - one charge: no second press is taken within 60 s.
//!
//! The scene: Blue's level-11 Monk at (3500, 12500) and Red's level-11 Musketeer at (3500, 18000), both deployed; the
//! Monk's button pressed on the fifth tick. Pinned:
//!   1. the Monk has one button, a champion's, and the press is taken;
//!   2. he stands through the cast and the active window, and walks again after them;
//!   3. in the active window every Musketeer shot on him lands at 75, and the Musketeer takes 217 for each;
//!   4. before the active window and after it, her shots land at 217 and nothing comes back;
//!   5. a second press is refused, the charge spent.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test monk_deflect`):
//!   deflect_returns_nothing   the deflect sends nothing back: (3) goes red.
//!   deflect_walks             the active window does not hold him: (2) goes red.
//!   ability_time_rounds_up    his 933 ms cast and 933 ms trigger keep their part-tick: (2) goes red (row 98).
//!   deflect_catches_at_body   a shot at him flies to his centre: `a_shot_at_him_is_caught_at_his_deflect_areas_edge`
//!                             goes red.
//!   deflect_pushed            the active window leaves him to his neighbours' pushes:
//!                             `a_giant_walking_into_him_does_not_move_him_while_it_is_active` goes red.
//!   deflect_contact_kept      the active window leaves him in his neighbours' separation scans (and their avoidance
//!                             meets him as a mover): `a_giant_on_him_is_not_pushed_by_him_while_it_is_active` red.
//!   deflect_buff_at_trigger   status.DEFLECT_BUFF_LANDING's new arm lands the buff at the trigger for its whole time:
//!                             `a_hit_on_the_trigger_tick_and_one_past_the_window_land_in_full` red.
//!
//! THE CATCH (combat.rs `step_projectiles`): a shot at him while his deflect is active lands on the tick its step brings it
//! within his deflect area's Radius (1500), not at his centre. Measured on client 15.535.29 (sp-champ-Monk-s0): a
//! Musketeer 5996 away, three shots of three a tick before a flight to his centre would land, their returns a tick
//! earlier too.
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeflectBuffLanding, DeployError};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Monk", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Per tick after the press: the Monk's point and hp, and the Musketeer's hp.
struct Row {
    at: Vec2,
    hp: i32,
    musk: i32,
}

fn scene() -> (BattleState, EntityId, EntityId) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n((3500, 12500)), None).expect("the Monk");
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", n((3500, 18000)), None).expect("the Musketeer");
    (s, monk, musk)
}

fn run(s: &mut BattleState, monk: EntityId, musk: EntityId, ticks: u32) -> Vec<Row> {
    (0..ticks)
        .map(|_| {
            s.tick();
            Row { at: s.entity(monk).map_or(Vec2::default(), |e| e.pos), hp: s.entity(monk).map_or(0, |e| e.hp), musk: s.entity(musk).map_or(0, |e| e.hp) }
        })
        .collect()
}

/// (row, drop) for every hp drop of the Monk (`monk` true) or of the Musketeer.
fn drops(rows: &[Row], start: (i32, i32), monk: bool) -> Vec<(usize, i32)> {
    let mut prev = if monk { start.0 } else { start.1 };
    let mut out = Vec::new();
    for (k, r) in rows.iter().enumerate() {
        let hp = if monk { r.hp } else { r.musk };
        if hp < prev {
            out.push((k, prev - hp));
        }
        prev = hp;
    }
    out
}

#[test]
fn the_deflect_holds_the_monk_cuts_every_hit_to_35_percent_and_sends_the_shots_back() {
    let (mut s, monk, musk) = scene();
    for _ in 0..5 {
        s.tick();
    }
    let b = s.ability_buttons(Team::Blue);
    assert_eq!(b.len(), 1, "one button: the Monk's");
    assert!(b[0].champion && b[0].available, "a champion's button, charged: {:?}", b[0]);
    s.press_ability_button(Team::Blue, 0).expect("the press is taken");
    let start = (s.entity(monk).unwrap().hp, s.entity(musk).unwrap().hp);
    let rows = run(&mut s, monk, musk, 160);
    // The hold: the first row he moves again after the press, and the rows he stood through.
    let still: Vec<usize> = (1..rows.len()).filter(|&k| rows[k].at == rows[k - 1].at).collect();
    let walks_again = (20..rows.len()).find(|&k| rows[k].at != rows[k - 1].at).expect("he walks again after the Deflect");
    assert!((1..walks_again).all(|k| still.contains(&k)), "he stands from the press to the Deflect's end (walks again on row {walks_again})");
    // Row k is the press's first frame P + k. Measured: state 0 on P + 96, attacking on P + 97; his cast is 17 frames
    // and his state 79 (`whole_ticks_ms`).
    assert_eq!(walks_again, 97, "he moves again on P + 97, the client's first frame after his hold");
    let on_monk = drops(&rows, start, true);
    let on_musk = drops(&rows, start, false);
    let reduced: Vec<usize> = on_monk.iter().filter(|d| d.1 == 75).map(|d| d.0).collect();
    let full: Vec<usize> = on_monk.iter().filter(|d| d.1 == 217).map(|d| d.0).collect();
    assert!(reduced.len() >= 3, "the active window cuts her shots to 75: {on_monk:?}");
    assert!(full.iter().all(|&k| k < reduced[0] || k > *reduced.last().unwrap()), "no full shot inside the active window: {on_monk:?}");
    // Each reduced shot comes back at her for its full 217 (the last may find less hp left than that), six ticks after
    // it lands on him, as measured.
    let mut left = start.1;
    let back: Vec<(usize, i32)> = on_musk.iter().copied().filter(|d| d.0 > reduced[0]).collect();
    assert_eq!(back.len(), reduced.len(), "each reduced shot comes back at her: on her {on_musk:?}, on him {on_monk:?}");
    for (r, (b, dmg)) in reduced.iter().zip(&back) {
        assert_eq!(b - r, 6, "her shot comes back six ticks after it lands on him: {r} -> {b}");
        assert_eq!(*dmg, 217.min(left), "the return deals her shot's full damage: {on_musk:?}");
        left -= dmg;
    }
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilitySpent), "one charge: a second press is refused");
    let b = s.ability_buttons(Team::Blue)[0];
    assert!(!b.available && b.spent, "the charge is spent: {b:?}");
    eprintln!("walks again {walks_again}; on him {on_monk:?}; on her {on_musk:?}");
}

/// Every Musketeer shot that lands on him in his active window was last seen beyond his deflect area's 1500, and takes 35 %
/// of its 217 off him.
#[test]
fn a_shot_at_him_is_caught_at_his_deflect_areas_edge() {
    let (mut s, monk, _musk) = scene();
    for _ in 0..5 {
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut seen: Option<i64> = None;
    let mut caught = Vec::new();
    let mut hp = s.entity(monk).expect("the Monk").hp;
    for k in 1..=95u32 {
        s.tick();
        let m = s.entity(monk).expect("the Monk lives");
        if m.hp < hp && (20..=90).contains(&k) {
            if let Some(d) = seen {
                caught.push((k, hp - m.hp, d));
            }
        }
        hp = m.hp;
        seen = s.projectiles().iter().filter(|p| p.target == monk && !p.deflected).map(|p| isqrt(p.pos.dist2(m.pos)) / K as i64).min();
    }
    assert!(caught.len() >= 2, "the scene drifted: {caught:?}");
    for (k, lost, d) in &caught {
        assert_eq!(*lost, 75, "row {k}: 35 % of the Musketeer's 217");
        assert!(*d > 1500, "row {k}: the shot was last seen {d} from him, inside his deflect area, before it landed");
    }
}

/// HE IS PUSHED BY NO ONE WHILE IT IS ACTIVE (card.rs `AbilityEffect::Deflect::stay`, his ability's
/// NO_MOVE_ALLOW_ATTRACT): measured on client 15.535.29 (sp-champ-Monk-recharge-q20-s0), a Giant walking into him left
/// him on his point to the end of his deflect. Here a red Giant walks down his lane at him: he is held on his point
/// until it is near, pressed, and from the trigger (P + 17) to P + 90 he stands where the active window began, the
/// Giant on him.
#[test]
fn a_giant_walking_into_him_does_not_move_him_while_it_is_active() {
    const RED: [&str; 8] = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = n((3500, 12500));
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", at, None).expect("the Monk");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n((3500, 17500)), None).expect("the Giant");
    let gap = |s: &BattleState| {
        let (a, b) = (s.entity(monk).expect("the Monk").pos, s.entity(giant).expect("the Giant").pos);
        let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
        isqrt(dx * dx + dy * dy)
    };
    let mut waited = 0;
    while gap(&s) > 2200 {
        assert!(s.debug_set_pos(monk, at));
        s.tick();
        waited += 1;
        assert!(waited < 400, "the scene drifted: the Giant never came near ({})", gap(&s));
    }
    assert!(s.debug_set_pos(monk, at));
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = Vec::new();
    let mut nearest = i64::MAX;
    while s.tick_count() - 1 < p + 90 {
        s.tick();
        let k = s.tick_count() - 1 - p;
        let m = s.entity(monk).expect("the Monk").pos;
        if k >= 17 {
            rows.push((k, m.x / K, m.y / K));
            nearest = nearest.min(gap(&s));
        }
    }
    assert!(nearest < 1300, "the scene drifted: the Giant never stood on him in the active window ({nearest})");
    let first = (rows[0].1, rows[0].2);
    for r in &rows {
        assert_eq!((r.1, r.2), first, "P + {}: he stands where the active window began: {rows:?}", r.0);
    }
}

/// NOR DOES HE PUSH ANYONE WHILE IT IS ACTIVE under movement.DEFLECT_CONTACT's shipped arm, hidden (state.rs
/// `deflect_off`, the engine's reading of his ability's AVOIDANCE_AS_OBSTACLE). Client 15.535.29 pushes
/// (tests/deflect_contact.rs). The scene above: on every tick of the active window that starts with the Giant inside both
/// radii and 20 of him (its only neighbour), the Giant takes no contact push.
#[test]
fn a_giant_on_him_is_not_pushed_by_him_while_it_is_active() {
    const RED: [&str; 8] = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let at = n((3500, 12500));
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", at, None).expect("the Monk");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n((3500, 17500)), None).expect("the Giant");
    let gap = |s: &BattleState| {
        let (a, b) = (s.entity(monk).expect("the Monk").pos, s.entity(giant).expect("the Giant").pos);
        let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
        isqrt(dx * dx + dy * dy)
    };
    let reach = (s.entity(monk).expect("the Monk").radius + s.entity(giant).expect("the Giant").radius) as i64 / K as i64 + 20;
    let mut waited = 0;
    while gap(&s) > 2200 {
        assert!(s.debug_set_pos(monk, at));
        s.tick();
        waited += 1;
        assert!(waited < 400, "the scene drifted: the Giant never came near ({})", gap(&s));
    }
    assert!(s.debug_set_pos(monk, at));
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut on_him = Vec::new();
    while s.tick_count() - 1 < p + 90 {
        let before = gap(&s);
        s.tick();
        let k = s.tick_count() - 1 - p;
        let g = s.entity(giant).expect("the Giant");
        if k >= 18 && before <= reach {
            on_him.push((k, before, g.push_neighbours, g.push_applied.x / K, g.push_applied.y / K));
        }
    }
    assert!(on_him.len() >= 3, "the scene drifted: the Giant stood inside {reach} of him on {} ticks of the active window", on_him.len());
    for r in &on_him {
        assert!(r.2 == 0 && (r.3, r.4) == (0, 0), "P + {}: the Giant, {} from him, was pushed: {on_him:?}", r.0, r.1);
    }
}

/// The Monk alone at (3500, 12500), under status.DEFLECT_BUFF_LANDING = `arm`, his button pressed on the fifth tick: the
/// battle, his id, and the tick his Deflect triggers (T: the first tick after the press with a buff on him).
fn lone_monk(arm: DeflectBuffLanding) -> (BattleState, EntityId, u32) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.deflect_buff_landing = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n((3500, 12500)), None).expect("the Monk");
    for _ in 0..5 {
        s.tick();
    }
    let mut twin = s.clone();
    twin.press_ability_button(Team::Blue, 0).expect("the press");
    let mut t = None;
    for _ in 0..60 {
        twin.tick();
        if twin.entity(monk).is_some_and(|e| e.buffs.iter().any(|b| !b.is_empty())) {
            t = Some(twin.tick_count() - 1);
            break;
        }
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    (s, monk, t.expect("the Deflect never landed"))
}

/// The hp the Monk loses to a Red Zap landing on tick `at` (after `lone_monk`'s press), and the Zap's full damage on him
/// unpressed. The Zap's flight read off a twin: cast before tick c, it lands on c + d.
fn zap_at(arm: DeflectBuffLanding, at: u32) -> (i32, i32) {
    let (mut s, monk, _) = lone_monk(arm);
    // the Zap's flight and full damage, on a battle where the Monk is never pressed
    let mut unpressed = {
        let mut cfg = config();
        cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
        cfg.card_level = [11, 11];
        cfg.tower_level = [11, 11];
        let mut u = BattleState::try_new(0, cfg).expect("the decks load");
        past_deploy_lockout(&mut u);
        u
    };
    let um = unpressed.scenario_spawn_now(Team::Blue, "Monk", n((3500, 12500)), None).expect("the Monk");
    for _ in 0..5 {
        unpressed.tick();
    }
    let c = unpressed.tick_count();
    let hp0 = unpressed.entity(um).expect("the Monk").hp;
    unpressed.spawn_unit(Team::Red, "Zap", n((3500, 12500)), None).expect("the Zap");
    let mut d = None;
    for _ in 0..20 {
        let before = unpressed.entity(um).expect("the Monk").hp;
        unpressed.tick();
        if unpressed.entity(um).expect("the Monk").hp < before {
            d = Some(unpressed.tick_count() - 1 - c);
            break;
        }
    }
    let d = d.expect("the Zap never landed");
    let full = hp0 - unpressed.entity(um).expect("the Monk").hp;
    while s.tick_count() < at - d {
        s.tick();
    }
    s.spawn_unit(Team::Red, "Zap", n((3500, 12500)), None).expect("the Zap");
    while s.tick_count() < at {
        s.tick();
    }
    let before = s.entity(monk).expect("the Monk").hp;
    s.tick();
    assert_eq!(s.tick_count() - 1, at, "the row's tick");
    (before - s.entity(monk).expect("the Monk").hp, full)
}

/// status.DEFLECT_BUFF_LANDING = client_after_hits_tick_short (client 16.402, parity's r63 census over 12 presses: the
/// trigger tick's hits full 2 of 2, T + 1 .. T + 79 reduced 78 of 78, T + 80 on full 4 of 4): Zaps landing on T, T + 1,
/// T + 79 and T + 80. Plant: deflect_buff_at_trigger.
#[test]
fn a_hit_on_the_trigger_tick_and_one_past_the_window_land_in_full() {
    let (_, _, t) = lone_monk(DeflectBuffLanding::ClientAfterHitsTickShort);
    let row = |k: u32| zap_at(DeflectBuffLanding::ClientAfterHitsTickShort, t + k);
    let (on_t, full) = row(0);
    assert!(full > 0, "the scene drifted: the Zap took nothing off the unpressed Monk");
    assert_eq!(on_t, full, "client_after_hits_tick_short: the Zap on the trigger tick T = {t} lands in full");
    let (on_1, _) = row(1);
    assert!(on_1 > 0 && on_1 < full, "client_after_hits_tick_short: the Zap on T + 1 lands at 35 % ({on_1} of {full})");
    let (on_79, _) = row(79);
    assert!(on_79 > 0 && on_79 < full, "client_after_hits_tick_short: the Zap on T + 79 lands at 35 % ({on_79} of {full})");
    let (on_80, _) = row(80);
    assert_eq!(on_80, full, "client_after_hits_tick_short: the Zap on T + 80 lands in full");
    // NOT VACUOUS: at_trigger cuts the trigger tick's hit or the one past the window.
    let (_, _, t0) = lone_monk(DeflectBuffLanding::AtTrigger);
    let (a0, f0) = zap_at(DeflectBuffLanding::AtTrigger, t0);
    let (a80, _) = zap_at(DeflectBuffLanding::AtTrigger, t0 + 80);
    assert!(a0 < f0 || a80 < f0, "at_trigger: the same window as the new arm ({a0}, {a80} of {f0})");
}
