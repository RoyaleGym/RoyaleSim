//! A facing's angle ROUNDED to a whole degree (formation.rs `rounded_degree`), and the two arms that read it:
//! spawner.SPAWN_POINT = client_rounded_facing_degree (a set SpawnAngleShift's ring) and spawner.DEATH_SPAWN_LAYOUT =
//! facing_ring_rounded (a death ring, whose members keep the dying unit's heading). Both keys ship their older values.
//!
//! THE LAW, measured on client 15.535.29: a Night Witch facing (255, 22), at 4.93 degrees, lays her Bat ring at 5
//! (7 of 7 two-Bat emissions exact; the 1024 table's own argmax picks 4 there and gives 6 of 7); a Battle Ram heading
//! (28, -254) lays its death ring at -84, and both Barbarians start with that heading (13 of 17 rings exact, 17 of 17
//! headings). The end-to-end scenes are tests/test_ring_facing_degree.py and tests/test_death_ring_degree.py.
//!
//! WHAT IS PINNED:
//!   1. `rounded_degree` rounds (255, 22) to 5 and (28, -254) to 276, gives every table direction its own degree, and
//!      gives the zero vector 0;
//!   2. a red Battle Ram killed while it heads off-axis for the blue left princess tower lays its two Barbarians at the
//!      rounded degree of that heading, and both face the heading normalized to 256; under facing_ring they stand on
//!      the exact rotation and face their side's forward, so the scene separates the arms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test ring_degree`):
//!   * `rounded_degree_coarse` -- the helper takes the 1024 table's argmax: (1) goes red.
//!   * `death_ring_unrounded` -- the ring lies a degree off the rounding: (2) goes red on the positions.
//!   * `death_ring_members_face_forward` -- the members face their side's forward: (2) goes red on the heading.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::formation::{rounded_degree, sin1024};
use royalesim::move16402::normalize_to;
use royalesim::state::{BattleState, DeathSpawnLayout, SpawnedFirstStep};
use royalesim::Team;

#[test]
fn rounded_degree_picks_the_nearest_whole_degree() {
    assert_eq!(rounded_degree(Vec2::new(255, 22)), 5, "(255, 22) is 4.93 degrees");
    assert_eq!(rounded_degree(Vec2::new(28, -254)), 276, "(28, -254) is -83.71 degrees");
    for d in 0..360 {
        let v = Vec2::new(sin1024(d + 90) * 1000, sin1024(d) * 1000);
        assert_eq!(rounded_degree(v), d, "the table direction of {d} degrees");
    }
    assert_eq!(rounded_degree(Vec2::default()), 0);
}

/// Where the blue left princess tower stands, native.
const TOWER: (i32, i32) = (3500, 6500);
/// The Ram starts on the line from the tower along (-28, 254) x 15, so it heads at about -84 degrees.
const RAM_AT: (i32, i32) = (3080, 10310);

/// A red Battle Ram heading for the blue left princess tower, killed on the tick after it takes the tower: the death
/// point, the heading it died on (tower - death point, subtiles), and its Barbarians (position, facing) on their first
/// frame. The Ram still moves on the tick it is killed, so the death point is read off its ring: two members 180
/// degrees apart stand on exactly opposite offsets, truncation included, so their midpoint is the death point.
fn ram_death(layout: DeathSpawnLayout) -> (Vec2, Vec2, Vec<(Vec2, Vec2)>) {
    let mut cfg = config();
    cfg.calib.death_spawn_layout = layout;
    // spawner.SPAWNED_FIRST_STEP at none: the scene reads where the Barbarians are CREATED and the heading they are
    // created with; under the shipped client16402_same_tick they have stepped on their first frame.
    cfg.calib.spawned_first_step = SpawnedFirstStep::None;
    let mut s = BattleState::new(0, cfg);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let ram = s.scenario_spawn_now(Team::Red, "BattleRam", at(RAM_AT), None).unwrap();
    let tower = s.entities().find(|e| e.team == Team::Blue && e.pos == at(TOWER)).expect("the blue left princess tower").id;
    for _ in 0..20 {
        s.tick();
        if s.entity(ram).and_then(|e| e.target) == Some(tower) {
            break;
        }
    }
    let rv = s.entity(ram).expect("scene: the Ram died before it took the tower");
    assert_eq!(rv.target, Some(tower), "scene: the Ram never took the tower");
    let before: Vec<_> = s.entities().map(|e| e.id).collect();
    assert!(s.debug_set_hp(ram, 0));
    s.tick();
    assert!(s.entity(ram).is_none(), "scene: the Ram did not die");
    let kids: Vec<(Vec2, Vec2)> = s.entities().filter(|e| !before.contains(&e.id)).map(|e| (e.pos, e.facing)).collect();
    assert_eq!(kids.len(), 2, "scene: the Ram's death released {} units", kids.len());
    let death = Vec2::new((kids[0].0.x + kids[1].0.x) / 2, (kids[0].0.y + kids[1].0.y) / 2);
    (death, at(TOWER).sub(death), kids)
}

#[test]
fn a_death_ring_lies_at_the_rounded_degree_and_its_members_keep_the_heading() {
    let s = BattleState::new(0, config());
    let ram = card_stat(&s, "BattleRam");
    let ds = ram.death_spawn.as_ref().expect("data: the Battle Ram has a death spawn");
    let r = ds.radius.expect("data: the Battle Ram's DeathSpawnRadius") as i64;
    let (shift, n) = (ram.formation.spawn_angle_shift_deg, ds.count);
    assert_eq!(n, 2, "data: the Battle Ram releases two");

    let (death, heading, kids) = ram_death(DeathSpawnLayout::FacingRingRounded);
    let a = rounded_degree(heading);
    let want: Vec<Vec2> = (0..n)
        .map(|k| {
            let deg = a + shift + k * 360 / n;
            death.add(Vec2::new((r * sin1024(deg + 90) as i64 / 1024) as i32, (r * sin1024(deg) as i64 / 1024) as i32))
        })
        .collect();
    let mut got: Vec<Vec2> = kids.iter().map(|k| k.0).collect();
    let mut want_sorted = want.clone();
    got.sort_by_key(|p| (p.x, p.y));
    want_sorted.sort_by_key(|p| (p.x, p.y));
    assert_eq!(got, want_sorted, "the Barbarians do not stand at the rounded degree {a} of the heading {heading:?}");
    let mut h = (heading.x / K, heading.y / K);
    normalize_to(&mut h, 256);
    for (_, f) in &kids {
        assert_eq!((f.x, f.y), h, "a Barbarian does not keep the Ram's heading");
    }
    // facing_ring, the shipped value, differs in both, or the scene separates nothing
    let (_, _, old) = ram_death(DeathSpawnLayout::FacingRing);
    let mut old_pos: Vec<Vec2> = old.iter().map(|k| k.0).collect();
    old_pos.sort_by_key(|p| (p.x, p.y));
    assert_ne!(old_pos, want_sorted, "precondition: the exact rotation lands on the rounded degree's points here");
    assert!(old.iter().all(|(_, f)| (f.x, f.y) == (0, -256)), "precondition: under facing_ring the Barbarians face Red's forward");
}
