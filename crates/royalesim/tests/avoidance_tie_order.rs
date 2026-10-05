//! A TIE INSIDE ONE GRID GROUP GOES BY CREATION ORDER -- move16402.rs `Index::query` (the hits' key: first group
//! column, first group row, `Body::seq`, index), state.rs `phase_path16402_for` (every live body's `seq` is its
//! `Entities::creation_seq`). A fix: the query's own statement was that bodies are met in update order, which is
//! creation order (calibration match.TICK_ORDER, collision.CONTACT_LAW), and it met them by their index into `bodies`,
//! the entity SLOT, which a LIFO free list reuses.
//!
//! THE LAW. The avoidance scan lets the last static blocker it meets decide the sign. Its candidates come in the
//! grid's order: by the first group column, then row, of the query box each is sighted in, and inside one group in
//! creation order. Read on client 16.402, 20260918-130203.b2 t1568: a deploying Ice Spirit at (4499, 14500) facing
//! (0, 256), whose look circle lies in one group, (4, 14), with two waiting Goblins created that tick on (3738, 14500)
//! and (5260, 14500), mirror images across its heading. The left one alone gives +200, the right one -200. The client
//! meets the right one (created later) last: -190, held until the spirit walks. The engine met the left one last: +190.
//!
//! THE SCENES. Two Blue Cannons on the Goblins' points (a building is a static blocker) and a Blue Ice Spirit on the
//! spirit's, its look circle over both. Before them two fillers are set down and killed, so the Cannons take reused
//! slots in the reverse of their creation order: the later-created Cannon takes the LOWER slot.
//!
//! WHAT IS PINNED:
//!   1. the scan itself (a hand-built body array): a walker between two static blockers first sighted in one group
//!      takes -200 when the right one is the later-created and +200 when the left one is, whatever their indices;
//!   2. the right Cannon created last (and in the lower slot): the Ice Spirit's offset is -190 on the first tick;
//!   3. the left Cannon created last (and in the lower slot): +190 on the first tick, so the sign follows creation
//!      order both ways (2 and 3 together are the non-vacuity check: the slots are reversed in both);
//!   4. both scenes: the later-created Cannon does hold the lower slot (the precondition that makes 2 and 3 a test).
//!
//!   5. a static blocker grouped as a walker (`Body::group_walker`, spawner.SPECTRAL_PARENT_BLOCKER =
//!      client15535_static_walker_group) is sighted in the group its circle grown by 250 reaches: a walker whose look
//!      circle spans two columns meets a right blocker in the right column after a left one in the left column (-200),
//!      and the same blocker grouped as a walker in the left column, before a left blocker sighted a row down (+200).
//!
//! PLANT (regression): `slot_order_group_tie` keys a tie inside one group by index (the slot) again: (1), (2) and (3)
//! go red. `group_walker_unread` spans a static body grouped as a walker by its radius: (5) goes red.
//!     RUSTFLAGS='--cfg clash_plant="slot_order_group_tie"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test avoidance_tie_order

mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::move16402::{avoidance_scan, Body, Contact, Index};
use royalesim::state::BattleState;
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * SUBTILE_PER_MILLITILE, y * SUBTILE_PER_MILLITILE)
}

fn body(x: i32, y: i32, r: i32, mover: bool, seq: u32) -> Body {
    Body {
        x,
        y,
        start_x: x,
        start_y: y,
        side: 0,
        r,
        mass: 3,
        air: false,
        mover,
        alive: true,
        collidable: true,
        offset: 0,
        dir: (0, 256),
        heading_counts: true,
        avoid_static: false,
        group_walker: false,
        seq,
    }
}

const LEFT: (i32, i32) = (3738, 14500);
const RIGHT: (i32, i32) = (5260, 14500);
const SPIRIT: (i32, i32) = (4499, 14500);

/// Plant: slot_order_group_tie.
#[test]
fn the_scan_meets_a_tie_in_creation_order() {
    let index = Index::new(36, 64);
    let walker = body(SPIRIT.0, SPIRIT.1, 400, true, 0);
    // the right blocker at the lower index in both arrays; only the creation order differs
    for (left_seq, right_seq, want) in [(40, 41, -200), (41, 40, 200)] {
        let bodies = [walker, body(RIGHT.0, RIGHT.1, 500, false, right_seq), body(LEFT.0, LEFT.1, 500, false, left_seq)];
        let mut scratch = Vec::new();
        let mut con = Contact::default();
        avoidance_scan(&index, &bodies, 0, &mut con, None, false, &mut scratch);
        assert_eq!(con.offset, want, "left created {left_seq}, right created {right_seq}: the later-created blocker decides");
    }
}

/// The Ice Spirit's offset after its first tick, and the two Cannons' slots, with the Cannon on `last` created last.
fn cannon_scene(last: (i32, i32)) -> (i32, u32, u32) {
    let first = if last == RIGHT { LEFT } else { RIGHT };
    let mut s = BattleState::new(5, config());
    // two fillers, killed: their slots go on the free list, and the next two creations take them last-freed first
    let fillers: Vec<_> = [(15000, 9000), (16000, 9000)]
        .iter()
        .map(|p| s.scenario_spawn_now(Team::Blue, "Knight", at(p.0, p.1), None).expect("a filler"))
        .collect();
    for f in &fillers {
        assert!(s.debug_set_hp(*f, 0), "the filler is gone already");
    }
    s.tick();
    assert!(fillers.iter().all(|f| s.entity(*f).is_none()), "the scene drifted: a filler survived");
    let c_first = s.scenario_spawn_now(Team::Blue, "Cannon", at(first.0, first.1), None).expect("the first Cannon");
    let c_last = s.scenario_spawn_now(Team::Blue, "Cannon", at(last.0, last.1), None).expect("the last Cannon");
    let spirit = s.scenario_spawn_now(Team::Blue, "IceSpirits", at(SPIRIT.0, SPIRIT.1), None).expect("the Ice Spirit");
    s.tick();
    let offset = s.entity(spirit).expect("the Ice Spirit is gone").avoid_offset;
    (offset, c_first.index, c_last.index)
}

/// Plant: slot_order_group_tie.
#[test]
fn the_later_created_right_blocker_decides_from_the_lower_slot() {
    let (offset, first_slot, last_slot) = cannon_scene(RIGHT);
    assert!(last_slot < first_slot, "the scene drifted: the later-created Cannon holds slot {last_slot}, the earlier {first_slot}");
    assert_eq!(offset, -190, "the right Cannon, created last, is met last");
}

/// Plant: slot_order_group_tie.
#[test]
fn the_later_created_left_blocker_decides_from_the_lower_slot() {
    let (offset, first_slot, last_slot) = cannon_scene(LEFT);
    assert!(last_slot < first_slot, "the scene drifted: the later-created Cannon holds slot {last_slot}, the earlier {first_slot}");
    assert_eq!(offset, 190, "the left Cannon, created last, is met last");
}

/// Plant: group_walker_unread. The walker at (5000, 11900) facing (0, 256): its look circle (5000, 12156) r 500 spans
/// columns 4 and 5, row 11 (and 12). The right blocker at (5745, 12156): its radius span starts in column 5, its walker
/// span (r + 250) in column 4. The left blocker at (4700, 13000), r 600: sighted in column 4, row 12.
#[test]
fn a_static_blocker_grouped_as_a_walker_is_sighted_where_its_walker_span_reaches() {
    let index = Index::new(36, 64);
    let walker = body(5000, 11900, 500, true, 0);
    let mut right = body(5745, 12156, 500, false, 1);
    let left = body(4700, 13000, 600, false, 2);
    for (grouped, want) in [(false, -200), (true, 200)] {
        right.group_walker = grouped;
        let bodies = [walker, right, left];
        let mut scratch = Vec::new();
        let mut con = Contact::default();
        avoidance_scan(&index, &bodies, 0, &mut con, None, false, &mut scratch);
        assert_eq!(con.offset, want, "group_walker {grouped}: the blocker met last decides (right -200, left +200)");
    }
}
