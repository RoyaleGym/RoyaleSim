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
//!
//! THE CATCH (combat.rs `step_projectiles`): a shot at him while his deflect is active lands on the tick its step brings it
//! within his deflect area's Radius (1500), not at his centre. Measured on client 15.535.29 (sp-champ-Monk-s0): a
//! Musketeer 5996 away, three shots of three a tick before a flight to his centre would land, their returns a tick
//! earlier too.
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError};
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
