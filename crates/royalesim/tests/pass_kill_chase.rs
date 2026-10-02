//! combat.PASS_KILL_CHASE: what an attacker chasing its target OUT of its attack reach reads of that target when a
//! strike earlier in the sequential pass (match.TICK_ORDER = client_sequential_strike) has felled it.
//!
//! THE LAW, measured on client 15.535.29 (every unit in its attack whose target died, with one unit's direct strike on
//! the victim on the death frame): out of reach (centre distance past Range plus both radii), 19 of 25 attackers created
//! after the striker took their next target on the kill frame, their attack progress 0, and walked; 17 of 19 created
//! before it waited (no target for five frames, the next on the sixth). In reach, 131 of 131 created after the striker
//! waited. sp-il-b5e2 t3578: three Skeleton Army members 28 to 158 past their reach of a Hog Rider another member killed
//! took the princess tower on the kill frame.
//!
//! WHAT IS PINNED, on two Blue Knights at a Red Knight of 100 hitpoints that the first-created Blue Knight kills with
//! its first blow, the second Knight attacking it too and moved 1,000 back on each axis on the tick before the kill (out
//! of its attack reach, inside its locked hold):
//!   1. under client15535_chaser_reads_pass the second Knight lets go of the victim on the kill tick, its attack
//!      progress 0, and walks on the next five ticks;
//!   2. under start_of_pass it holds the corpse through the kill tick and then waits, standing, with no target;
//!   3. the arms part only out of reach: left in reach, the second Knight waits under both, tick for tick.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! pass_kill_chase`):
//!   * `chaser_reads_pass_start` -- the chaser reads the pass as it began under the new arm too: (1) goes red;
//!   * `chaser_waits` -- the chaser that reads the kill still starts the wait: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, PassKillChase, TickOrder};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle(arm: PassKillChase) -> BattleState {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    cfg.calib.pass_kill_chase = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s
}

fn knight(s: &mut BattleState, team: Team, p: (i32, i32), hp: Option<i32>) -> EntityId {
    s.scenario_spawn_now(team, "Knight", at(p.0, p.1), hp).expect("a Knight")
}

/// The scene: (battle, the second Knight, the victim, the kill's tick counted from the second Knight's creation). With
/// `ticks` None it is a dry run, ticked through the kill to find that count; with Some(n) it stops on the tick before
/// the kill.
fn scene(arm: PassKillChase, ticks: Option<u32>) -> (BattleState, EntityId, EntityId, u32) {
    let mut s = battle(arm);
    knight(&mut s, Team::Blue, (8300, 12200), None);
    let v = knight(&mut s, Team::Red, (9000, 13300), Some(100));
    for _ in 0..8 {
        s.tick();
    }
    let k2 = knight(&mut s, Team::Blue, (9700, 12200), None);
    let n = match ticks {
        Some(n) => {
            for _ in 0..n - 1 {
                s.tick();
            }
            n
        }
        None => (1..=80)
            .find(|_| {
                s.tick();
                s.entity(v).is_none()
            })
            .expect("the scene drifted: the victim stood 80 ticks"),
    };
    (s, k2, v, n)
}

/// The second Knight's (pos, target, attack phase, progress) on the kill tick and the 6 after, moved `away` (native)
/// straight back from the victim on the tick before the kill.
fn second_knight(arm: PassKillChase, away: i32) -> Vec<(Vec2, Option<EntityId>, AttackPhase, i32)> {
    let (_, _, _, n) = scene(arm, None);
    let (mut s, k2, v, _) = scene(arm, Some(n));
    let e = s.entity(k2).expect("the second Knight");
    assert_eq!(e.target, Some(v), "the scene drifted: the second Knight was not on the victim before the kill");
    assert_ne!(e.attack_phase, AttackPhase::Idle, "the scene drifted: the second Knight was not attacking before the kill");
    let p = e.pos;
    assert!(s.debug_set_pos(k2, Vec2::new(p.x + away * K, p.y - away * K)));
    s.tick();
    assert!(s.entity(v).is_none(), "the scene drifted: the kill did not come on the tick the dry run found");
    let mut out = Vec::new();
    for _ in 0..7 {
        let e = s.entity(k2).expect("the second Knight");
        out.push((e.pos, e.target, e.attack_phase, e.attack_ms));
        s.tick();
    }
    out
}

#[test]
fn a_chaser_out_of_reach_takes_its_next_target_on_the_kill_tick() {
    let v = scene(PassKillChase::Client15535ChaserReadsPass, None).2;
    let new = second_knight(PassKillChase::Client15535ChaserReadsPass, 1000);
    let old = second_knight(PassKillChase::StartOfPass, 1000);
    // (1) the kill tick: the corpse let go and the swing dropped; then it walks
    assert_ne!(new[0].1, Some(v), "client15535_chaser_reads_pass: the chaser still holds the corpse on the kill tick");
    assert_eq!(new[0].3, 0, "client15535_chaser_reads_pass: the chaser's attack progress on the kill tick");
    for w in new.windows(2).take(5) {
        assert_ne!(w[0].0, w[1].0, "client15535_chaser_reads_pass: the chaser stood still after the kill: {new:?}");
    }
    // (2) start_of_pass: the corpse held through the kill tick, then the wait, standing
    assert_eq!(old[0].1, Some(v), "start_of_pass: the chaser let go of the corpse on the kill tick");
    assert!(old[1..5].iter().all(|r| r.1.is_none()), "start_of_pass: the chaser did not wait: {old:?}");
    assert!(old[1..5].windows(2).all(|w| w[0].0 == w[1].0), "start_of_pass: the chaser walked during its wait: {old:?}");
}

#[test]
fn in_reach_both_arms_wait() {
    let new = second_knight(PassKillChase::Client15535ChaserReadsPass, 0);
    let old = second_knight(PassKillChase::StartOfPass, 0);
    assert!(new[1..5].iter().all(|r| r.1.is_none()), "the scene drifted: the second Knight in reach did not wait: {new:?}");
    assert_eq!(new, old, "an attacker in reach: the arms part");
}
