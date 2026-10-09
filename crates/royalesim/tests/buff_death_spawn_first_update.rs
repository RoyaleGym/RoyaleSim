//! A CURSE HOG WHOSE PARENT DIED BEFORE THE MOVE PASS (item 319; status.BUFF_DEATH_SPAWN_FIRST_UPDATE, state.rs `phase_reap`'s
//! buff death spawn loop): under client15535_doomed_parent a buff's death spawn whose parent the move pass's doomed mask
//! held (movement.DYING_UNIT_VISIBILITY; killed by the damage buffered before the move pass) takes its first update on its
//! creation tick, with the dying parent a static blocker in its avoidance scan; any other death leaves it to the next tick.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-SuperWitch-s0 t408): a Knight cursed by the Super Witch and killed by a Bat's
//! direct hit left a Voodoo Hog 1100 ahead of it whose first frame, on t408, took an avoidance offset of -200 off the
//! Knight dead ahead (its walk t413..t427 fits the offset decaying -140..0, 15 of 15 steps); the engine's hog first moved on
//! t409, with the Knight gone, at offset 0. sweep-WitchMother: a Knight killed by a princess tower's arrow after the move
//! pass left its hog at offset 0.
//!
//! THE SCENE (tests/curse.rs's): a Blue Cannon, a Red Knight attacking it, a Blue Mother Witch 5500 south of the Knight, who
//! curses it; and a Blue Knight behind the Red one (on the side away from the hog), whose strikes are the kill. A probe
//! reads the Blue Knight's strike ticks with the Red Knight's hitpoints put back each tick; the scene is run again with them
//! set to 1 before a strike tick D past the curse, or to 0 before a tick on which nothing hits it (not doomed).
//!
//! WHAT IS PINNED, under movement.DYING_UNIT_VISIBILITY = client_doomed_static: (1) killed by the strike, the hog's avoidance
//! offset on D is not 0 under client15535_doomed_parent, and is 0 under the shipped next_tick; (2) set to 0, it is 0 on D
//! under client15535_doomed_parent too.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test buff_death_spawn_first_update`):
//!   * `buff_spawn_first_update_next_tick` -- the hog first moves on the next tick: (1) goes red;
//!   * `buff_spawn_first_update_any_parent` -- the hog steps on its birth tick whatever killed its parent: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, BuffDeathSpawnFirstUpdate, Calib, DyingUnitVisibility};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const CANNON: (i32, i32) = (8000, 20000);
const RED_KNIGHT: (i32, i32) = (9900, 20000);
const WITCH: (i32, i32) = (9900, 14500);
/// Behind the Red Knight as the hog sees it (the hog is born 1100 south of it and walks north), touching it.
const BLUE_KNIGHT: (i32, i32) = (9900, 21000);

fn cfg(arm: BuffDeathSpawnFirstUpdate) -> BattleConfig {
    let mut c = config();
    c.calib.dying_unit_visibility = DyingUnitVisibility::ClientDoomedStatic;
    c.calib.buff_death_spawn_first_update = arm;
    c
}

fn cursed(s: &BattleState, id: EntityId) -> bool {
    let b = card_stat(s, "WitchMother").attack_buff.expect("the Witch hangs a buff").buff;
    s.entity(id).is_some_and(|v| v.buffs.iter().any(|sl| sl.id == b + 1))
}

/// Per tick k: (the Red Knight's hitpoints drop on k, it carries the curse after k). `kill`: (the tick before which its
/// hitpoints are set, to what); then the hog it leaves and the hog's avoidance offset on its creation tick.
fn scene(arm: BuffDeathSpawnFirstUpdate, kill: Option<(usize, i32)>) -> (Vec<(i32, bool)>, Option<i32>) {
    let mut s = BattleState::new(0, cfg(arm));
    let _cannon = s.scenario_spawn_now(Team::Blue, "Cannon", at(CANNON), None).expect("the Cannon");
    let red = s.scenario_spawn_now(Team::Red, "Knight", at(RED_KNIGHT), None).expect("the Red Knight");
    let _witch = s.scenario_spawn_now(Team::Blue, "WitchMother", at(WITCH), None).expect("the Witch");
    let blue = s.scenario_spawn_now(Team::Blue, "Knight", at(BLUE_KNIGHT), None).expect("the Blue Knight");
    let full = s.entity(red).expect("the Red Knight").max_hp;
    let mut rows = Vec::new();
    for k in 0..400 {
        let Some(_) = s.entity(red) else { break };
        assert!(s.debug_set_pos(red, at(RED_KNIGHT)) && s.debug_set_pos(blue, at(BLUE_KNIGHT)));
        assert!(s.debug_set_hp(blue, s.entity(blue).expect("the Blue Knight").max_hp));
        let hp = match kill {
            Some((t, h)) if t == k => h,
            _ => full,
        };
        assert!(s.debug_set_hp(red, hp));
        let hogs: Vec<EntityId> = s.entities().filter(|e| e.card == "VoodooHog").map(|e| e.id).collect();
        s.tick();
        let after = s.entity(red).map_or(0, |v| v.hp.max(0));
        rows.push((if hp > 0 { hp - after } else { 0 }, cursed(&s, red)));
        if kill.is_some_and(|(t, _)| t == k) {
            let new = s.entities().find(|e| e.card == "VoodooHog" && !hogs.contains(&e.id)).map(|e| e.avoid_offset);
            return (rows, new);
        }
    }
    (rows, None)
}

/// The probe: a tick D past tick 100, the Red Knight cursed before it, on which the Blue Knight's strike lands alone
/// (the drop equal to a Knight's damage).
fn strike_tick() -> usize {
    let (rows, _) = scene(BuffDeathSpawnFirstUpdate::NextTick, None);
    let s = BattleState::new(0, cfg(BuffDeathSpawnFirstUpdate::NextTick));
    let dmg = s.cards().scaled(s.cards().index("Knight").expect("the Knight"), s.config().card_level[0], card_stat(&s, "Knight").damage).expect("a valid level");
    (101..rows.len()).find(|&k| rows[k].0 == dmg && rows[k - 1].1).unwrap_or_else(|| panic!("the scene drifted: no strike of {dmg} on the cursed Knight: {:?}", &rows[100..]))
}

/// A tick past tick 100, the Red Knight cursed before it, on which nothing hits it (no hit in the buffer: not doomed).
fn quiet_tick() -> usize {
    let (rows, _) = scene(BuffDeathSpawnFirstUpdate::NextTick, None);
    (101..rows.len()).find(|&k| rows[k].0 == 0 && rows[k - 1].1).expect("the scene drifted: no quiet tick on the cursed Knight")
}

/// (1) Plant: buff_spawn_first_update_next_tick.
#[test]
fn a_hog_whose_parent_a_strike_killed_steps_on_its_birth_tick() {
    let d = strike_tick();
    let (_, new) = scene(BuffDeathSpawnFirstUpdate::Client15535DoomedParent, Some((d, 1)));
    let off = new.expect("the scene drifted: the struck Knight left no hog");
    assert_ne!(off, 0, "client15535_doomed_parent: the hog took no avoidance offset off its dying parent on its birth tick");
    // NOT VACUOUS: the shipped arm leaves its first update to the next tick.
    let (_, old) = scene(BuffDeathSpawnFirstUpdate::NextTick, Some((d, 1)));
    assert_eq!(old, Some(0), "next_tick: the hog's offset on its birth tick");
}

/// (2) Plant: buff_spawn_first_update_any_parent.
#[test]
fn a_hog_whose_parent_died_of_no_hit_waits_for_the_next_tick() {
    let d = quiet_tick();
    let (_, new) = scene(BuffDeathSpawnFirstUpdate::Client15535DoomedParent, Some((d, 0)));
    assert_eq!(new, Some(0), "client15535_doomed_parent: a hog whose parent the doomed mask did not hold stepped on its birth tick");
}

#[test]
fn the_shipped_arm_is_the_next_tick() {
    assert_eq!(Calib::shipped().buff_death_spawn_first_update, BuffDeathSpawnFirstUpdate::NextTick);
}
