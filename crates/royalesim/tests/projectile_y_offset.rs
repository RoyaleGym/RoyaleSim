//! A KING TOWER'S SHOT IS BORN ITS PROJECTILEYOFFSET FURTHER ALONG ITS OWN FORWARD Y (calibration
//! combat.PROJECTILE_Y_OFFSET; combat.rs `launch_point`, card.rs `CardDef::projectile_y_offset`).
//!
//! MEASURED on the live 16.402 corpus (one seat per battle, 41 battles): every king-tower shot's first frame
//! sits 400 past the point combat.PROJECTILE_LAUNCH gives (ProjectileStartRadius 750 from the centre toward the
//! target), along the owner's own forward y: +400 for the Blue king on 191 of 191 shots, -400 for the Red king on
//! 222 of 222. 400 is the King Tower row's ProjectileYOffset. Flown from there a shot lands on the client's tick on
//! 251 of 251 followed flights; flown from the plain point, 58 land late. The princess towers, whose rows set no
//! ProjectileYOffset, sit on the plain point.
//!
//! WHAT IS PINNED, every number read from the loaded CardDb:
//!   1. under client_forward_y a king's first shot is born at the plain point plus the row's offset along the
//!      king's own forward y, for both seats (a Blue Knight walking at the Red king, a Red Knight at the Blue
//!      king; each king is made to shoot by destroying its own princess towers before the battle);
//!   2. under not_read it is born at the plain point;
//!   3. a princess tower's shot is born at the plain point under both arms (the null);
//!   4. flown from the moved point the first king shot at the walking Knight lands one tick earlier, the Knight
//!      having walked the same ticks under both arms;
//!   5. the shipped value is client_forward_y (not_read, the old arm, shipped until parity scored the flip), and the
//!      King Tower is the only loaded row outside the hero pass that sets the column; the hero pass's rows that set it
//!      are the Hero Musketeer and her turret;
//!   6. a row the hero pass loaded applies its offset under both arms, once: the Hero Musketeer's first shot is born
//!      300 past her plain point along Blue's forward y, and the base Musketeer's on the plain point (the null).
//!
//! PLANTS (regression):
//!   * `projectile_y_offset_unread` -- the loader drops the column, so the new arm moves nothing: 1 and 4 go red.
//!     RUSTFLAGS='--cfg clash_plant="projectile_y_offset_unread"' CARGO_TARGET_DIR=target/plant cargo test --test projectile_y_offset
//!   * `projectile_y_offset_arena_frame` -- the offset goes up the arena for both seats, so the Red king's shot is
//!     born behind it: 1 goes red (its Red-king half).
//!     RUSTFLAGS='--cfg clash_plant="projectile_y_offset_arena_frame"' CARGO_TARGET_DIR=target/plant cargo test --test projectile_y_offset

mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE};
use royalesim::path::native_in_frame;
use royalesim::state::{BattleConfig, BattleState, Calib, ProjectileYOffset};
use royalesim::Team;

/// Engine subtiles per native millitile (the recordings' unit).
const K: i32 = SUBTILE_PER_MILLITILE;

fn cfg(arm: ProjectileYOffset) -> BattleConfig {
    let mut c = config();
    c.calib.projectile_y_offset = arm;
    c
}

fn forward(team: Team) -> i64 {
    match team {
        Team::Blue => 1,
        Team::Red => -1,
    }
}

/// One scene: a Knight of `knight_team` put down at `at` (native), the other team's towers shooting it.
struct Run {
    /// (tick, position, aim) of the shooter team's first projectile, on the tick it first appears.
    first: (u32, Vec2, Vec2),
    /// The first tick the Knight's hp drops.
    hit: u32,
    /// The Knight's position after every tick, from the first.
    walk: Vec<(u32, Vec2)>,
    /// The shooter's centre, ProjectileStartRadius and ProjectileYOffset, native.
    centre: Vec2,
    radius: i64,
    offset: i64,
}

fn scene(arm: ProjectileYOffset, knight_team: Team, at: (i32, i32), king: bool) -> Run {
    let shooter = match knight_team {
        Team::Blue => Team::Red,
        Team::Red => Team::Blue,
    };
    let mut s = BattleState::new(7, cfg(arm));
    if king {
        for k in [1, 2] {
            s.scenario_set_tower_hp(shooter, k, 0).expect("destroy a princess tower before the battle");
        }
    }
    let tower = s.tower_ids(shooter)[if king { 0 } else { 1 }].expect("the shooting tower stands");
    let centre = s.entity(tower).unwrap().pos;
    let stat = card_stat(&s, if king { "KingTower" } else { "PrincessTower" }).clone();
    let knight = s.scenario_spawn_now(knight_team, "Knight", Vec2::new(at.0 * K, at.1 * K), None).expect("put the Knight down");
    let hp0 = s.entity(knight).unwrap().hp;
    let (mut first, mut hit, mut walk) = (None, None, Vec::new());
    for _ in 0..200 {
        s.tick();
        let t = s.tick_count();
        let e = s.entity(knight).expect("the scene drifted: the Knight died before its first hit");
        walk.push((t, e.pos));
        if first.is_none() {
            let shots: Vec<_> = s.projectiles().iter().filter(|p| p.team == shooter).collect();
            if !shots.is_empty() {
                assert_eq!(shots.len(), 1, "the scene drifted: {} shots on the first tick", shots.len());
                first = Some((t, shots[0].pos, shots[0].aim));
            }
        }
        if e.hp < hp0 {
            hit = Some(t);
            break;
        }
    }
    Run {
        first: first.expect("the scene drifted: the tower never shot"),
        hit: hit.expect("the scene drifted: the Knight was never hit"),
        walk,
        centre,
        radius: (stat.projectile_start_radius / K) as i64,
        offset: (stat.projectile_y_offset / K) as i64,
    }
}

/// combat.PROJECTILE_LAUNCH's point in native units, the engine's arithmetic: the centre + trunc0(v * R /
/// isqrt(v.v)), v = the aim - the centre, both read on the native grid in the shooter's frame.
fn plain_point(r: &Run, shooter: Team) -> (i64, i64) {
    let (sx, sy) = (native_in_frame(r.centre.x, shooter), native_in_frame(r.centre.y, shooter));
    let aim = r.first.2;
    let (vx, vy) = (native_in_frame(aim.x, shooter) - sx, native_in_frame(aim.y, shooter) - sy);
    let n = isqrt(vx * vx + vy * vy);
    assert!(n > 0, "the scene drifted: the shot's aim is the tower's centre");
    (sx + vx * r.radius / n, sy + vy * r.radius / n)
}

fn born(r: &Run) -> (i64, i64) {
    let p = r.first.1;
    assert!(p.x % K == 0 && p.y % K == 0, "a shot is born on the native grid (combat.PROJECTILE_STEP), got {p:?}");
    ((p.x / K) as i64, (p.y / K) as i64)
}

const SCENES: [(Team, (i32, i32)); 2] = [(Team::Blue, (9000, 20000)), (Team::Red, (9000, 12000))];

#[test]
fn a_king_shot_is_born_past_its_start_radius_point_along_its_own_forward_y() {
    for (knight_team, at) in SCENES {
        let shooter = if knight_team == Team::Blue { Team::Red } else { Team::Blue };
        let r = scene(ProjectileYOffset::ClientForwardY, knight_team, at, true);
        assert!(r.offset != 0, "data: the King Tower's ProjectileYOffset is blank");
        let (px, py) = plain_point(&r, shooter);
        let want = (px, py + forward(shooter) * r.offset);
        assert_eq!(
            born(&r),
            want,
            "client_forward_y, the {shooter:?} king: its first shot was born at {:?}, want the ProjectileStartRadius point ({px}, {py}) plus {} along its own forward y",
            born(&r),
            r.offset
        );
    }
}

#[test]
fn the_old_arm_bears_a_king_shot_at_the_start_radius_point() {
    for (knight_team, at) in SCENES {
        let shooter = if knight_team == Team::Blue { Team::Red } else { Team::Blue };
        let r = scene(ProjectileYOffset::NotRead, knight_team, at, true);
        assert_eq!(born(&r), plain_point(&r, shooter), "not_read, the {shooter:?} king: its first shot is not on the plain point");
    }
}

#[test]
fn a_princess_shot_is_born_at_the_start_radius_point_under_both_arms() {
    for arm in [ProjectileYOffset::ClientForwardY, ProjectileYOffset::NotRead] {
        let r = scene(arm, Team::Blue, (3500, 17000), false);
        assert_eq!(r.offset, 0, "data: the princess tower's row now sets a ProjectileYOffset; this null needs another tower");
        assert_eq!(born(&r), plain_point(&r, Team::Red), "{arm:?}: the princess tower's first shot is not on the plain point");
    }
}

#[test]
fn the_moved_start_lands_the_first_hit_a_tick_earlier() {
    let new = scene(ProjectileYOffset::ClientForwardY, Team::Blue, (9000, 20000), true);
    let old = scene(ProjectileYOffset::NotRead, Team::Blue, (9000, 20000), true);
    let walked: Vec<u32> = new.walk.iter().zip(&old.walk).filter(|(a, b)| a.0 < old.hit && a.1 != b.1).map(|(a, _)| a.0).collect();
    assert!(walked.is_empty(), "the scene drifted: the Knight walked differently under the two arms on {walked:?}");
    assert_eq!(
        new.hit + 1,
        old.hit,
        "the first king shot landed on {} under client_forward_y and on {} under not_read; the shot born {} nearer should land one tick earlier",
        new.hit,
        old.hit,
        new.offset
    );
}

#[test]
fn the_shipped_value_is_client_forward_y_and_the_king_tower_is_the_only_loaded_row_with_the_column() {
    assert_eq!(Calib::shipped().projectile_y_offset, ProjectileYOffset::ClientForwardY);
    let s = BattleState::new(7, config());
    assert_eq!(card_stat(&s, "KingTower").projectile_y_offset, 400 * K, "the King Tower row's ProjectileYOffset");
    assert_eq!(card_stat(&s, "PrincessTower").projectile_y_offset, 0);
    let db = s.cards();
    let set = |hero: bool| -> Vec<&str> {
        let rows = db.cards.iter().enumerate().filter(|(k, c)| c.projectile_y_offset != 0 && db.is_hero_record(*k as u16) == hero);
        rows.map(|(_, c)| c.name.as_str()).collect()
    };
    assert_eq!(set(false), ["KingTower"], "the loaded rows outside the hero pass that set ProjectileYOffset");
    assert_eq!(set(true), ["Musketeer_hero", "MusketeerTurret"], "the hero pass's rows that set ProjectileYOffset");
}

/// A Blue `name` at (9000, 9000) native with a Red Knight 5000 ahead of it: (where its first shot is born, the plain
/// point, its ProjectileYOffset), native.
fn troop_shot(arm: ProjectileYOffset, name: &str) -> ((i64, i64), (i64, i64), i64) {
    let mut s = BattleState::new(7, cfg(arm));
    let idx = s.cards().index(name).unwrap_or_else(|| panic!("{name} does not load"));
    let stat = card_stat(&s, name).clone();
    let shooter = s.scenario_spawn_now(Team::Blue, name, Vec2::new(9000 * K, 9000 * K), None).expect("put the shooter down");
    s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(9000 * K, 14000 * K), None).expect("put the Knight down");
    for _ in 0..200 {
        s.tick();
        let shots: Vec<_> = s.projectiles().iter().filter(|p| p.team == Team::Blue && p.firer_card == Some(idx)).collect();
        if let Some(p) = shots.first() {
            assert_eq!(shots.len(), 1, "the scene drifted: {} shots on the first tick", shots.len());
            let centre = s.entity(shooter).expect("the scene drifted: the shooter died").pos;
            let r = Run { first: (s.tick_count(), p.pos, p.aim), hit: 0, walk: Vec::new(), centre, radius: (stat.projectile_start_radius / K) as i64, offset: (stat.projectile_y_offset / K) as i64 };
            return (born(&r), plain_point(&r, Team::Blue), r.offset);
        }
    }
    panic!("the scene drifted: {name} never shot");
}

#[test]
fn a_hero_row_applies_its_offset_under_both_arms_once() {
    for arm in [ProjectileYOffset::NotRead, ProjectileYOffset::ClientForwardY] {
        let (at, (px, py), offset) = troop_shot(arm, "Musketeer_hero");
        assert_eq!(offset, 300, "data: the Hero Musketeer row's ProjectileYOffset");
        assert_eq!(at, (px, py + offset), "{arm:?}: the Hero Musketeer's first shot is not {offset} past her plain point ({px}, {py})");
        let (at, plain, offset) = troop_shot(arm, "Musketeer");
        assert_eq!(offset, 0, "data: the base Musketeer's row now sets a ProjectileYOffset; this null needs another troop");
        assert_eq!(at, plain, "{arm:?}: the base Musketeer's first shot is not on her plain point");
    }
}
