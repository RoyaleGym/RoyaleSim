//! THE MIGHTY MINER'S BUTTON (tools/extract_cards.py `champion_lane_switch`; card.rs `LaneSwitchDef`, `UnitUse::LaneSwitchBomb`,
//! `CardDef::dropped_by_ability`; spell.rs `death_bomb_push`; state.rs `LaneRun`, `lane_pass`, `tunnel_step`, `surface`),
//! at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-champ-MightyMiner-s0; P the press's issue tick, t196): the cast holds
//! him P + 1 .. P + 9, he stands on the trigger P + 10 and drops his bomb there; under ground from P + 11 (the Knight that
//! held him took a tower that tick), (11314, 13178) to (10667, 13224), (10017, 13237), (9367, 13242), (8717, 13243),
//! (8067, 13244), (7417, 13244) on P + 11 .. P + 16 and his point (6686, 13178) on P + 17, the other lane's mirror; there
//! he deploys to P + 35 and attacks from P + 37; the bomb's 332 (130 at level 11) on a Knight 3486 from it on P + 30, and
//! the knockback ladder of 1800 from P + 31 (its first steps 245 and 247 on the diagonal: 250 capped).
//!
//! THE SCENE: the Mighty Miner held on the measured point 40 ticks, a red Knight held 2000 west of him (out of every crown
//! tower's reach) until the bomb's tick; then the press.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! mighty_miner`):
//!   - lane_switch_never -> `his_press_drops_a_bomb_and_takes_him_under_ground_to_the_other_lane` red;
//!   - lane_under_at_trigger -> the same red (under ground on P + 10);
//!   - lane_bomb_unpushed -> `his_bomb_strikes_and_pushes_twenty_ticks_after_the_trigger` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["MightyMiner", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (11314, 13178);
const KNIGHT: (i32, i32) = (9314, 13178);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// One row after each tick, indexed from P (the press's issue tick): the Miner's (x, y), whether he is under ground and
/// deploying; the Knight's (x, y), hitpoints and target.
#[derive(Clone, Copy, Debug)]
struct Row {
    miner: (i32, i32),
    under: bool,
    deploying: bool,
    knight: (i32, i32),
    knight_hp: i32,
    knight_target: Option<EntityId>,
}

/// The scene, `ticks` rows from P + 1; the Knight held on its point through P + `hold_knight`.
fn scene(ticks: u32, hold_knight: u32) -> (Vec<Row>, EntityId) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let mm = s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the Mighty Miner");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT), None).expect("a red Knight");
    for _ in 0..40 {
        assert!(s.debug_set_pos(mm, n(AT)));
        assert!(s.debug_set_pos(kn, n(KNIGHT)));
        s.tick();
    }
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = vec![];
    for k in 1..=ticks {
        if k <= hold_knight {
            assert!(s.debug_set_pos(kn, n(KNIGHT)));
        }
        s.tick();
        assert_eq!(s.tick_count() - 1 - p, k, "the row's tick");
        let m = s.entity(mm).expect("the Mighty Miner lives");
        let kv = s.entity(kn).expect("the Knight lives");
        rows.push(Row {
            miner: (m.pos.x / K, m.pos.y / K),
            under: m.tunnel_dest.is_some(),
            deploying: m.deploying,
            knight: (kv.pos.x / K, kv.pos.y / K),
            knight_hp: kv.hp,
            knight_target: kv.target,
        });
    }
    (rows, mm)
}

#[test]
fn his_press_drops_a_bomb_and_takes_him_under_ground_to_the_other_lane() {
    let (rows, mm) = scene(40, 40);
    let row = |k: usize| rows[k - 1];
    for k in 1..=10 {
        assert_eq!((row(k).miner, row(k).under), (AT, false), "P + {k}: he stands, above ground: {:?}", row(k));
    }
    assert!(row(10).knight_target == Some(mm), "the scene drifted: the Knight is not on him on the trigger: {:?}", row(10));
    let path = [(10667, 13224), (10017, 13237), (9367, 13242), (8717, 13243), (8067, 13244), (7417, 13244)];
    for (j, want) in path.iter().enumerate() {
        let k = 11 + j;
        assert!(row(k).under, "P + {k}: under ground: {:?}", row(k));
        assert_eq!(row(k).miner, *want, "P + {k}: his step under ground (measured)");
    }
    assert_ne!(row(11).knight_target, Some(mm), "the Knight drops him on P + 11");
    assert_eq!((row(17).miner, row(17).under), ((18000 - AT.0, AT.1), false), "P + 17: up on the other lane's mirror point");
    for k in 17..=35 {
        assert!(row(k).deploying, "P + {k}: deploying after he came up");
    }
    assert!(!row(36).deploying && row(36).miner == row(35).miner, "P + 36: deployed, not moved");
}

#[test]
fn his_bomb_strikes_and_pushes_twenty_ticks_after_the_trigger() {
    let (rows, _) = scene(34, 30);
    let row = |k: usize| rows[k - 1];
    // The Knight stands in a princess tower's reach: its arrows (109) land too.
    let lost: Vec<(usize, i32)> = (12..=34).map(|k| (k, row(k - 1).knight_hp - row(k).knight_hp)).filter(|(_, d)| *d != 0).collect();
    let at30 = lost.iter().find(|r| r.0 == 30).map(|r| r.1);
    assert!(matches!(at30, Some(332) | Some(441)), "the bomb's 332 (130 at level 11) on P + 30: {lost:?}");
    assert!(lost.iter().all(|&(k, d)| k >= 30 || d == 109), "nothing but the towers' arrows before P + 30: {lost:?}");
    let step = (row(31).knight.0 - row(30).knight.0, row(31).knight.1 - row(30).knight.1);
    assert_eq!(step, (-250, 0), "P + 31: the ladder of 1800's first step, 250 capped, away from the bomb");
}

/// His button is a champion's (state.rs `champion_button`): reported so, and charged, before the press.
#[test]
fn his_button_is_a_champions() {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the champion");
    for _ in 0..30 {
        s.tick();
    }
    let b = s.ability_buttons(Team::Blue);
    assert_eq!(b.len(), 1, "one button: his");
    assert!(b[0].champion && b[0].available, "a champion's button, charged: {:?}", b[0]);
}
