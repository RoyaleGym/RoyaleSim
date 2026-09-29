//! THE ARCHER QUEEN'S CAPE: a champion button (card.rs `AbilityEffect::SelfBuff`, state.rs `fire_ability`), and a
//! carried buff's invisibility (status.rs `BuffDef::invisible`, target.rs `invisible_at`).
//!
//! The law, measured on client 15.535.29 (sp-champ-ArcherQueen-s0 against its no-press twin; P the press's first
//! frame, t215):
//!   - the press is taken, 1 elixir; she casts P..P + 16 (CastTime 933 in whole ticks, state.rs `whole_ticks_ms`),
//!     her attack progress 0 through it, and attacks again from P + 18;
//!   - her buff ArcherQueenRapid lands on P + 4 (TriggerDelay 200): every enemy that held her (a Knight, a Skeleton,
//!     a Musketeer) took another target on P + 4;
//!   - it lasts 3500 ms: her attack runs at 2.8 times its rate (HitSpeedMultiplier 280) through P + 73, her hits on
//!     the Knight 8 or 9 ticks apart (1200 / 2.8 = 428.6 ms);
//!   - one charge.
//!
//! The scene: Blue's level-11 Archer Queen at (3500, 12500), Red's level-11 Knight at (3500, 16000) and Musketeer at
//! (3500, 18000), both walking at her. Her button is pressed once both hold her. Pinned:
//!   1. one button, a champion's, and the press is taken;
//!   2. no attack progress through her cast (P..P + 16), and she attacks again by P + 18;
//!   3. from P + 4 no enemy targets her, through the buff's last frame;
//!   4. her attack progress, the client's own column: 1040 on P + 18 (a fresh entry, LoadTime 900 + 140), 140 a
//!      tick through P + 73 (8740), 50 a tick from P + 74 (8790), as sp-champ-ArcherQueen-s0 reads frame for frame;
//!   5. a second press is refused, the charge spent.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test archer_queen_cape`):
//!   buff_invisible_targetable   a carried Invisible buff hides nobody: (3) goes red.
//!   ability_time_rounds_up      her 933 ms cast keeps its part-tick: (2) goes red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["ArcherQueen", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn scene() -> (BattleState, EntityId, [EntityId; 2]) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let aq = s.scenario_spawn_now(Team::Blue, "ArcherQueen", n((3500, 12500)), None).expect("the Archer Queen");
    let k = s.scenario_spawn_now(Team::Red, "Knight", n((3500, 16000)), None).expect("the Knight");
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", n((3500, 18000)), None).expect("the Musketeer");
    (s, aq, [k, m])
}

/// Per frame from the press's first: her attack progress, and whether any live enemy targets her.
struct Row {
    attack_ms: i32,
    held: bool,
}

#[test]
fn the_cape_hides_the_archer_queen_and_speeds_her_attack_for_3500_ms() {
    let (mut s, aq, enemies) = scene();
    // Both enemies hold her before the press.
    let mut waited = 0;
    while !enemies.iter().all(|e| s.entity(*e).is_some_and(|v| v.target == Some(aq))) {
        s.tick();
        waited += 1;
        assert!(waited < 200, "both enemies take her as their target");
    }
    let b = s.ability_buttons(Team::Blue);
    assert_eq!(b.len(), 1, "one button: the Archer Queen's");
    assert!(b[0].champion && b[0].available, "a champion's button, charged: {:?}", b[0]);
    s.press_ability_button(Team::Blue, 0).expect("the press is taken");
    let mut rows: Vec<Row> = Vec::new();
    for _ in 0..100 {
        s.tick();
        let Some(v) = s.entity(aq) else { break };
        let held = s.entities().any(|e| e.team == Team::Red && e.target == Some(aq));
        rows.push(Row { attack_ms: v.attack_ms, held });
    }
    let trace: Vec<(usize, i32, bool)> = rows.iter().enumerate().map(|(k, r)| (k, r.attack_ms, r.held)).collect();
    eprintln!("{trace:?}");
    // (2) Row k is frame P + k.
    assert!(rows[..17].iter().all(|r| r.attack_ms == 0), "no attack progress through her cast P..P + 16: {trace:?}");
    assert!(rows[18].attack_ms > 0, "she attacks again by P + 18: {trace:?}");
    // (3)
    assert!(rows[..4].iter().any(|r| r.held), "an enemy still holds her before the cape lands: {trace:?}");
    assert!(rows.iter().take(74).skip(4).all(|r| !r.held), "no enemy targets her P + 4..P + 73: {trace:?}");
    assert_eq!(rows.len(), 100, "she lives through the scene");
    // (4) Her progress column, as the client's reads (sp-champ-ArcherQueen-s0, t233..t324).
    assert_eq!(rows[18].attack_ms, 1040, "a fresh entry at the raged rate on P + 18: {trace:?}");
    assert!((19..=73).all(|k| rows[k].attack_ms - rows[k - 1].attack_ms == 140), "2.8 times her rate through P + 73: {trace:?}");
    assert!((74..rows.len()).all(|k| rows[k].attack_ms - rows[k - 1].attack_ms == 50), "her own rate from P + 74: {trace:?}");
    // (5)
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilitySpent), "one charge: a second press is refused");
}
