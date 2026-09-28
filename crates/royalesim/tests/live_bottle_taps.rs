//! placement.LIVE_BOTTLE_TAPS: a troop tap on a live bottle of the placer's own side, read off `resolve_point` and
//! `spawn_unit` (the play path and the replay harness share that resolution).
//!
//! THE LAW, read off the client 16.402 corpus (3 battles, both seats): a troop group tapped on the tile of its own
//! side's Rage, cast 1 to 4 ticks earlier, is laid a tile over; in the placer's own frame every one goes (3500, 1500)
//! -> (2500, 1500), the first tile of the building ring search from the snapped tile (ring 1, nearest to the tap, the
//! column-major tie in the placer's frame). The Rage's bottle stands from its cast C until its release on C + 9
//! (spells.SUMMON_FUSE_START); taps on the tile at C + 27 or later are laid as tapped. Side 1's corpus tap (14500,
//! 30500) is laid on (15500, 30500). Today's engine (not_blocked) runs the bottle as a spell object nothing collides
//! with, so the tap stays on the bottle's tile.
//!
//! WHAT IS PINNED, each scene a Rage cast on own (3500, 1500), then Goblins tapped on that tile:
//!   1. client16402_relocate, C + 1: the Goblins are laid exactly as a tap on own (2500, 1500) is, both seats; side 1
//!      in arena coordinates is the corpus's (14500, 30500) -> (15500, 30500);
//!   2. not_blocked, C + 1: laid exactly as a tap on own (3500, 1500) with no Rage, both seats;
//!   3. both values, C + 27 (the bottle released): not moved;
//!   4. client16402_relocate: a candidate on another own live bottle does not fit (a second Rage on own (2500, 1500)
//!      sends the tap to the next fitting tile of the ring, own (3500, 2500), both seats);
//!   5. client16402_relocate: the other side's bottle is not read (the measured scope is the placer's own);
//!   6. the shipped value is client16402_relocate (the 2026-09-28 bottle flip).
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleState, Calib, LiveBottleTaps};
use royalesim::Team;

const NEW: LiveBottleTaps = LiveBottleTaps::Client16402Relocate;
const OLD: LiveBottleTaps = LiveBottleTaps::NotBlocked;

/// Own-frame native point `p` of `team`, in arena subtiles.
fn own(s: &BattleState, team: Team, p: (i32, i32)) -> Vec2 {
    s.config().arena.from_frame(team, Vec2::new(p.0 * K, p.1 * K))
}

fn battle(arm: LiveBottleTaps) -> BattleState {
    let mut cfg = config();
    cfg.calib.placement_live_bottle_taps = arm;
    BattleState::new(1, cfg)
}

/// The live bottles on the board (their points).
fn bottles(s: &BattleState) -> Vec<Vec2> {
    s.spells().iter().filter_map(|sp| if let SpellMotion::Fuse { pos, .. } = sp.motion { Some(pos) } else { None }).collect()
}

/// Goblins tapped by `team` on own `tap` on the board of tick C + `k`, where `rages` are the (caster, own-frame point)
/// Rages cast on tick C (none: the same ticks with no cast). The resolved point and the laid members, sorted.
fn goblins(arm: LiveBottleTaps, team: Team, rages: &[(Team, (i32, i32))], k: u32, tap: (i32, i32)) -> (Vec2, Vec<Vec2>) {
    let mut s = battle(arm);
    for &(caster, p) in rages {
        let at = own(&s, caster, p);
        s.spawn_unit(caster, "Rage", at, None).expect("cast Rage");
    }
    for _ in 0..=k {
        s.tick();
    }
    if !rages.is_empty() {
        let live = bottles(&s).len();
        if k < 9 {
            assert_eq!(live, rages.len(), "the scene drifted: C + {k} holds {live} live bottles");
        } else {
            assert_eq!(live, 0, "the scene drifted: a bottle is still live on C + {k}");
        }
    }
    let idx = s.cards().index("Goblins").expect("data: Goblins loads");
    let at = own(&s, team, tap);
    let resolved = s.resolve_point(team, idx, at);
    s.spawn_unit(team, "Goblins", at, None).expect("spawn_unit takes the tap");
    let mut laid: Vec<Vec2> = s.pending_spawns().into_iter().filter(|(t, c, _)| *t == team && s.cards().get(*c).name == "Goblins").map(|(_, _, p)| p).collect();
    assert!(!laid.is_empty(), "nothing was laid");
    laid.sort_by_key(|p| (p.x, p.y));
    (resolved, laid)
}

const BOTTLE: (i32, i32) = (3500, 1500);
const OVER: (i32, i32) = (2500, 1500);

#[test]
fn a_tap_on_a_live_own_bottle_is_laid_a_tile_over() {
    for team in [Team::Blue, Team::Red] {
        let s = battle(NEW);
        let (resolved, laid) = goblins(NEW, team, &[(team, BOTTLE)], 1, BOTTLE);
        assert_eq!(resolved, own(&s, team, OVER), "{team:?}: the tap on the bottle's tile");
        let (_, want) = goblins(NEW, team, &[], 1, OVER);
        assert_eq!(laid, want, "{team:?}: the Goblins are not laid as a tap on own (2500, 1500)");
        let (_, stay) = goblins(NEW, team, &[], 1, BOTTLE);
        assert_ne!(laid, stay, "{team:?}: vacuous, the two taps lay one formation");
    }
    // Side 1 in arena coordinates, as the corpus recorded it (20260918-133849).
    let native = |x: i32, y: i32| Vec2::new(x * K, y * K);
    let (resolved, _) = goblins(NEW, Team::Red, &[(Team::Red, BOTTLE)], 4, BOTTLE);
    assert_eq!(own(&battle(NEW), Team::Red, BOTTLE), native(14500, 30500));
    assert_eq!(resolved, native(15500, 30500), "side 1's C + 4 tap");
}

#[test]
fn the_old_value_lays_the_tap_on_the_bottle() {
    for team in [Team::Blue, Team::Red] {
        let s = battle(OLD);
        let (resolved, laid) = goblins(OLD, team, &[(team, BOTTLE)], 1, BOTTLE);
        assert_eq!(resolved, own(&s, team, BOTTLE), "{team:?}: the old value moved the tap");
        let (_, want) = goblins(OLD, team, &[], 1, BOTTLE);
        assert_eq!(laid, want, "{team:?}: the Goblins are not laid as a tap with no Rage");
    }
}

#[test]
fn a_tap_after_the_release_is_not_moved() {
    for arm in [NEW, OLD] {
        for team in [Team::Blue, Team::Red] {
            let s = battle(arm);
            let (resolved, laid) = goblins(arm, team, &[(team, BOTTLE)], 27, BOTTLE);
            assert_eq!(resolved, own(&s, team, BOTTLE), "{arm:?} {team:?}: the C + 27 tap moved");
            let (_, want) = goblins(arm, team, &[], 27, BOTTLE);
            assert_eq!(laid, want, "{arm:?} {team:?}");
        }
    }
}

#[test]
fn a_candidate_on_another_own_bottle_does_not_fit() {
    for team in [Team::Blue, Team::Red] {
        let s = battle(NEW);
        let (resolved, _) = goblins(NEW, team, &[(team, BOTTLE), (team, OVER)], 1, BOTTLE);
        // Ring 1 in column-major order: own (2500, 1500) is on the second bottle, own (3500, 500) on the back row's
        // no-deploy cells (the row is open only behind the King), so the next fit is own (3500, 2500).
        assert_eq!(resolved, own(&s, team, (3500, 2500)), "{team:?}: the next tile of the ring");
    }
}

#[test]
fn the_other_sides_bottle_is_not_read() {
    for team in [Team::Blue, Team::Red] {
        let s = battle(NEW);
        let enemy = team.other();
        let p = s.config().arena.to_frame(enemy, own(&s, team, BOTTLE));
        let (resolved, _) = goblins(NEW, team, &[(enemy, (p.x / K, p.y / K))], 1, BOTTLE);
        assert_eq!(resolved, own(&s, team, BOTTLE), "{team:?}: moved off the enemy's bottle");
    }
}

#[test]
fn the_shipped_value_is_the_new_one() {
    assert_eq!(Calib::shipped().placement_live_bottle_taps, NEW, "client16402_relocate ships since the 2026-09-28 bottle flip");
}
