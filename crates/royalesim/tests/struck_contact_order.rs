//! movement.STRUCK_CONTACT_ORDER: which movers a troop struck down in the sequential pass (match.TICK_ORDER =
//! client_sequential_strike) still meets in the move pass (state.rs `phase_path16402_for`, `Scratch::strike_lethal`).
//!
//! THE LAW, read on client 15.535.29 as if each unit ran its target, attack and move in one turn in creation order: the
//! struck troop is a body to the separation of the movers created before its striker only, and takes no update of its
//! own when its striker was created before it. Golems struck by an earlier unit laid their Golemites on their last point
//! (ub-ds1, ub-ds2); an Elixir Golem struck by a later one, one walk step on (ub-b1-elg1, -elg3); Skeleton Army members
//! created after the striker moved into a struck Musketeer's place (sp-il-04cb t1221).
//!
//! WHAT IS PINNED, on two Blue Knights at a Red Knight of 100 hitpoints that the first-created Blue Knight kills with its
//! first blow, the second Knight (created after the striker) set 700 beside the victim, inside both radii, on the tick
//! before the kill:
//!   1. under whole_pass the dying Knight pushes the second Knight away on the kill tick;
//!   2. under client15535_after_striker it does not: the second Knight's move on the kill tick has no part away from it;
//!   3. the same for the striker itself, set 700 beside its victim on the tick before: pushed under whole_pass, not under
//!      client15535_after_striker (it strikes, then moves). Client 15.535.29 sweep-SkeletonArmy t275: the Knight that
//!      struck a Skeleton overlapping it by 61 stood still on the kill tick.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! struck_contact_order`):
//!   * `struck_contact_whole_pass` -- the struck troop is a body to every mover under the new arm too: (2) and (3) go red.
//!   * `struck_meets_its_striker` -- the striker's own move still meets its victim: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, StruckContactOrder, TickOrder};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle(arm: StruckContactOrder) -> BattleState {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    cfg.calib.struck_contact_order = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s
}

fn knight(s: &mut BattleState, team: Team, p: (i32, i32), hp: Option<i32>) -> EntityId {
    s.scenario_spawn_now(team, "Knight", at(p.0, p.1), hp).expect("a Knight")
}

/// The second Knight's move (or, `striker`, the striker's) on the kill tick, projected on the line out of the victim's
/// point (native, outward positive). The kill tick is found on a dry run (`ticks` None), which the scene repeats.
fn push_out(arm: StruckContactOrder, striker: bool) -> i32 {
    let run = |ticks: Option<u32>| -> (BattleState, EntityId, EntityId, EntityId, u32) {
        let mut s = battle(arm);
        let k1 = knight(&mut s, Team::Blue, (8300, 12200), None);
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
        (s, k1, k2, v, n)
    };
    let (_, _, _, _, n) = run(None);
    let (mut s, k1, k2, v, _) = run(Some(n));
    let vp = s.entity(v).expect("the victim before the kill").pos;
    let (who, side) = if striker { (k1, -1) } else { (k2, 1) };
    assert!(s.debug_set_pos(who, Vec2::new(vp.x + side * 700 * K, vp.y)));
    let p0 = s.entity(who).expect("the Knight").pos;
    s.tick();
    assert!(s.entity(v).is_none(), "the scene drifted: the kill did not come on the tick the dry run found");
    let p1 = s.entity(who).expect("the Knight").pos;
    side * (p1.x - p0.x) / K
}

#[test]
fn a_struck_troop_pushes_only_the_movers_before_its_striker() {
    let old = push_out(StruckContactOrder::WholePass, false);
    assert!(old > 0, "whole_pass: the dying Knight did not push the second Knight ({old}): the scene separates nothing");
    let new = push_out(StruckContactOrder::Client15535AfterStriker, false);
    assert!(new <= 0, "client15535_after_striker: the dying Knight pushed a mover created after its striker ({new})");
}

/// Plant: struck_contact_whole_pass (and the striker's own turn: struck_meets_its_striker).
#[test]
fn a_struck_troop_does_not_push_its_striker() {
    let old = push_out(StruckContactOrder::WholePass, true);
    assert!(old > 0, "whole_pass: the dying Knight did not push its striker ({old}): the scene separates nothing");
    let new = push_out(StruckContactOrder::Client15535AfterStriker, true);
    assert!(new <= 0, "client15535_after_striker: the dying Knight pushed its own striker ({new})");
}
