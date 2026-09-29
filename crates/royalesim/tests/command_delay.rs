//! THE COMMAND DELAY (state.rs `BattleConfig::command_delay_ticks`, `run_due_commands`, `PendingCommand`).
//!
//! Measured on the live client (device clock, 238 taps; one Training Camp run of pending plays): a tap runs 1072..1099
//! ms later (21-22 ticks), the same for every player; the hand, the elixir and the unit change only when it runs;
//! several plays may wait at once; a waiting card's slot empties at the touch, so it cannot be played again; a second
//! tap the elixir covers only without the first's cost is never sent (nothing spent); a building whose tile is taken
//! when it runs goes where a play then would put it, paid.
//!
//! Pinned, with Blue's k = 22 and Red's 0:
//!   1. a play waits: nothing spawns, and Blue's hand and elixir stand still, until T + 22, when it runs as the same
//!      play at once would have run on T + 22 (the unit's first frame 22 ticks after the undelayed play's);
//!   2. the waiting card cannot be played again (CardPending) and its cost is spoken for (NotEnoughElixir);
//!   3. a second building tapped on the first's point 0.1 s later runs too, relocated, and is paid;
//!   4. Red, at k = 0, plays at once;
//!   5. a snapshot taken with a play waiting restores it: the same hash, and the play runs on time;
//!   6. a champion's button press waits too.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test command_delay`):
//!   pending_command_ignored   a waiting card and its cost are not held: (2) goes red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CommandOutcome, DeployError};
use royalesim::Team;

const DECK: [&str; 8] = ["Knight", "Cannon", "Tombstone", "Musketeer", "Fireball", "Arrows", "Monk", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn battle(delay: [u32; 2]) -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.command_delay_ticks = delay;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s
}

fn slot_of(s: &BattleState, team: Team, card: &str) -> usize {
    s.hand(team).iter().position(|c| *c == card).unwrap_or_else(|| panic!("{card} in hand: {:?}", s.hand(team)))
}

/// Ticks from now until `team` has a live `card` unit, up to `limit`.
fn first_frame(s: &mut BattleState, team: Team, card: &str, limit: u32) -> Option<u32> {
    for k in 1..=limit {
        s.tick();
        if s.entities().any(|e| e.team == team && e.card == card) {
            return Some(k);
        }
    }
    None
}

#[test]
fn a_play_waits_k_ticks_and_then_runs_as_a_play_at_once() {
    // The undelayed play, for the unit's first frame.
    let mut now = battle([0, 0]);
    let slot = slot_of(&now, Team::Blue, "Knight");
    now.deploy_slot(Team::Blue, slot, n((9500, 8500))).expect("the play");
    let at_once = first_frame(&mut now, Team::Blue, "Knight", 60).expect("the Knight comes");

    let mut s = battle([22, 0]);
    let hand = s.hand(Team::Blue).iter().map(|c| c.to_string()).collect::<Vec<_>>();
    let elixir = s.elixir_raw(Team::Blue).0;
    let slot = slot_of(&s, Team::Blue, "Knight");
    s.deploy_slot(Team::Blue, slot, n((9500, 8500))).expect("the play is accepted");
    assert_eq!(s.pending_commands(Team::Blue).len(), 1, "one command waits");
    // (2) The waiting card cannot be played again; its cost (3) is spoken for.
    assert_eq!(s.check_deploy_slot(Team::Blue, slot, n((9500, 9500))), Err(DeployError::CardPending), "the waiting card is held");
    let musk = slot_of(&s, Team::Blue, "Musketeer");
    assert!(matches!(s.check_deploy_slot(Team::Blue, musk, n((9500, 9500))), Ok(())), "4 on 10 less 3 is covered");
    s.scenario_set_elixir_milli(Team::Blue, 6_000);
    assert!(
        matches!(s.check_deploy_slot(Team::Blue, musk, n((9500, 9500))), Err(DeployError::NotEnoughElixir { .. })),
        "4 on 6 less the Knight's 3 is not covered"
    );
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let elixir = elixir.max(s.elixir_raw(Team::Blue).0);
    // (1) Nothing moves until it runs.
    for k in 1..at_once + 22 {
        s.tick();
        let has = s.entities().any(|e| e.team == Team::Blue && e.card == "Knight");
        assert!(!has, "no Knight before T + 22 + its own lead (tick {k})");
        if k < 22 {
            let h: Vec<String> = s.hand(Team::Blue).iter().map(|c| c.to_string()).collect();
            assert_eq!(h, hand, "the hand stands still while it waits (tick {k})");
            assert!(s.elixir_raw(Team::Blue).0 >= elixir, "nothing is spent while it waits (tick {k})");
        }
    }
    s.tick();
    assert!(s.entities().any(|e| e.team == Team::Blue && e.card == "Knight"), "the Knight's first frame is 22 after the play at once's");
    assert!(s.pending_commands(Team::Blue).is_empty(), "nothing waits after it ran");
}

#[test]
fn a_second_building_on_a_waiting_tile_runs_relocated_and_paid() {
    let mut s = battle([22, 0]);
    let at = n((8500, 9500));
    let tomb = slot_of(&s, Team::Blue, "Tombstone");
    s.deploy_slot(Team::Blue, tomb, at).expect("the Tombstone");
    // Tapped 0.1 s after it, as measured; both ran, the second after the first stood.
    s.tick();
    s.tick();
    let cannon = slot_of(&s, Team::Blue, "Cannon");
    s.deploy_slot(Team::Blue, cannon, at).expect("the Cannon, on the waiting tile, is accepted");
    let mut ran = Vec::new();
    for _ in 0..30 {
        s.tick();
        ran.extend(s.commands_run().iter().cloned());
    }
    assert_eq!(ran.len(), 2, "both ran: {ran:?}");
    let points: Vec<Vec2> = ran.iter().map(|r| match r.result {
        Ok(CommandOutcome::Deployed(p)) => p,
        ref other => panic!("both went down: {other:?}"),
    }).collect();
    assert_ne!(points[0], points[1], "the second went where a play then would put it: {points:?}");
    assert!(s.entities().any(|e| e.team == Team::Blue && e.card == "Cannon"), "the Cannon stands");
}

#[test]
fn red_at_zero_plays_at_once_while_blue_waits() {
    let mut s = battle([22, 0]);
    let slot = slot_of(&s, Team::Red, "Knight");
    s.deploy_slot(Team::Red, slot, n((9500, 23500))).expect("Red's play");
    assert!(s.pending_commands(Team::Red).is_empty(), "Red's play does not wait");
    let mut now = battle([0, 0]);
    let slot = slot_of(&now, Team::Red, "Knight");
    now.deploy_slot(Team::Red, slot, n((9500, 23500))).expect("Red's play");
    let a = first_frame(&mut s, Team::Red, "Knight", 60);
    let b = first_frame(&mut now, Team::Red, "Knight", 60);
    assert_eq!(a, b, "Red at k = 0 runs as it always has");
}

#[test]
fn a_snapshot_keeps_a_waiting_play() {
    let mut s = battle([22, 22]);
    let slot = slot_of(&s, Team::Blue, "Knight");
    s.deploy_slot(Team::Blue, slot, n((9500, 8500))).expect("the play");
    for _ in 0..5 {
        s.tick();
    }
    let bytes = s.save();
    let mut r = battle([0, 0]);
    r.restore(&bytes).expect("restores");
    assert_eq!(r.state_hash(), s.state_hash(), "the same state, the waiting play included");
    assert_eq!(r.pending_commands(Team::Blue), s.pending_commands(Team::Blue));
    for _ in 0..40 {
        s.tick();
        r.tick();
        assert_eq!(r.state_hash(), s.state_hash(), "they run the same");
    }
    assert!(r.entities().any(|e| e.team == Team::Blue && e.card == "Knight"), "the restored play ran");
}

#[test]
fn a_button_press_waits_too() {
    let mut s = battle([22, 0]);
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n((3500, 12500)), None).expect("the Monk");
    for _ in 0..5 {
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press is accepted");
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::CardPending), "the waiting button is held");
    // It runs at the top of the tick whose index is T + 22, the 23rd from the press, where a press at once runs in the
    // 1st.
    for k in 1..=22 {
        s.tick();
        assert_eq!(s.entity(monk).unwrap().stun_ms, 0, "no cast while it waits (tick {k})");
    }
    s.tick();
    assert!(s.entity(monk).unwrap().stun_ms > 0, "the cast starts once it ran");
}
