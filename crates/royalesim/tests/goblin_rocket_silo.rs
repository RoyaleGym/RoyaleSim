//! THE GOBLIN ROCKET SILO EVENT CARD (item 302; card.rs `StageDef`, `timed_stages_of`, `deploy_w_tile_margin`,
//! `target_only_king_tower`; state.rs `spawn_with`'s stages and `building_placement`'s margin; target.rs `can_target`): a
//! building that changes twice on a clock, the last row shooting the enemy king alone and dying on its shot; its footprint
//! kept 5 tiles off each side edge.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-GoblinRocketSilo-s0, level 11, one play): a tap on (14500, 11500) put it on
//! (11500, 11500); GoblinRocketSilo1 on its first frame F + 124, GoblinRocketSilo2 on F + 240, one entity, its hitpoints
//! carried; the last row took side 1's king (17,678 off) on its change frame, fired 19 frames later and was gone on its fire
//! frame (F + 259); the rocket, 350 a tick from 1000 off the silo, took 1600 (625 x 256 %) off the king on F + 307.
//!
//! WHAT IS PINNED: a Blue silo tapped on (14500, 11500) stands on (11500, 11500), is GoblinRocketSilo1 from F + 124 and
//! GoblinRocketSilo2 from F + 240, targets the red king (nothing else) as that row, is gone on F + 259, and the red king
//! loses 1600 once, on F + 307.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test goblin_rocket_silo`):
//!   * `stages_refused` -- the card is refused again: red;
//!   * `stages_never_run` -- the silo never changes: red;
//!   * `king_only_unread` -- the last row takes another target: red;
//!   * `tile_margin_unread` -- it stands on the tap's tile: red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Plants: stages_refused, stages_never_run, king_only_unread, tile_margin_unread.
#[test]
fn the_silo_lands_five_tiles_in_changes_twice_and_shoots_the_king_once() {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(15, cfg);
    past_deploy_lockout(&mut s);
    let king: EntityId = s.entities().find(|e| e.team == Team::Red && e.kind == EntityKind::KingTower).expect("the red king").id;
    let king_hp = s.entity(king).expect("the red king").hp;
    s.spawn_unit_tapped(Team::Blue, "GoblinRocketSilo", at((14500, 11500)), None).expect("the Goblin Rocket Silo is played");
    let mut silo: Option<(EntityId, u32)> = None;
    let mut rows: Vec<(u32, String)> = Vec::new();
    let mut targets: Vec<Option<EntityId>> = Vec::new();
    let mut gone: Option<u32> = None;
    let mut king_losses: Vec<(u32, i32)> = Vec::new();
    let mut last_king = king_hp;
    for _ in 0..400 {
        s.tick();
        let t = s.tick_count() - 1;
        if silo.is_none() {
            silo = s.entities().find(|e| e.team == Team::Blue && e.card.ends_with("GoblinRocketSilo")).map(|e| (e.id, t));
            if let Some((id, _)) = silo {
                let e = s.entity(id).expect("the silo");
                assert_eq!((e.pos.x / K, e.pos.y / K), (11500, 11500), "a 3 x 3 kept 5 tiles off the right edge");
            }
        }
        if let Some((id, _)) = silo {
            match s.entity(id) {
                Some(e) => {
                    let row = e.card.trim_start_matches("units.").to_string();
                    if rows.last().is_none_or(|(_, r)| *r != row) {
                        rows.push((t, row.clone()));
                    }
                    if row == "GoblinRocketSilo2" {
                        targets.push(e.target);
                    }
                }
                None if gone.is_none() => gone = Some(t),
                None => {}
            }
        }
        let k = s.entity(king).map_or(0, |e| e.hp);
        if k < last_king {
            king_losses.push((t, last_king - k));
        }
        last_king = k;
    }
    let (_, f) = silo.expect("the silo never stood");
    let steps: Vec<(u32, &str)> = rows.iter().map(|(t, r)| (t - f, r.as_str())).collect();
    assert_eq!(steps, [(0, "GoblinRocketSilo"), (124, "GoblinRocketSilo1"), (240, "GoblinRocketSilo2")], "its rows: {rows:?}");
    assert!(!targets.is_empty() && targets.iter().all(|t| *t == Some(king)), "the last row's targets: {targets:?} (king {king:?})");
    assert_eq!(gone.map(|g| g - f), Some(259), "the silo goes on its fire tick, its change + 19");
    let losses: Vec<(u32, i32)> = king_losses.iter().map(|(t, d)| (t - f, *d)).collect();
    assert_eq!(losses, [(307, 1600)], "the red king takes one rocket of 1600, 48 ticks after the fire");
}
