//! targeting.DOOMED_LANE_TOWER, read off the engine: whether a princess tower the shots in flight will kill is still
//! the tower a unit walks to.
//!
//! THE READING (client 15.535.29's special-form scenes and the 16.402 corpus, with the engine's doomed set; the ledger's
//! provenance): on the tick after the engine's doomed set first holds a princess tower, every walker whose card fires a
//! projectile takes the king and walks to it, even where the other princess stands nearer, and every walker without
//! one keeps the princess. Attackers keep it. projectile_walkers_take_king is that reading; walkers_take_king is
//! parity's proposal as written (every walker takes the king), which the same records refute; standing, the engine
//! before the 2026-09-28 round 9 flip, walks a unit the doom drop makes let go of the princess on to it. The flip ships
//! projectile_walkers_take_king.
//!
//! The scene: the Red engine-right princess tower on 40 hp, a Blue Musketeer in its reach whose first shot dooms it,
//! and four Blue walkers in the right lane: a Baby Dragon (a projectile) and a Knight (none) in sight of the princess,
//! walking at it, and the same two far back beside their own tower, out of every enemy's sight, walking to their
//! default tower, where the other princess stands nearer than the king. The engine's doomed set (`doom_reading`) holds
//! the princess from the tick the shot leaves; the tick after is read, and the princess still stands on it. A walker's
//! GOAL is the Red crown tower its route's goal (route[0], the pathfinder's goal-first layout) stands nearest. WHAT IS
//! PINNED, each with those preconditions:
//!   1. projectile_walkers_take_king and walkers_take_king: both Baby Dragons have no target and walk to the king (the
//!      far one although the other princess is nearer); the Musketeer, attacking, keeps the princess. Under
//!      walkers_take_king the far Knight walks to the king too, under projectile_walkers_take_king to the princess.
//!      Under standing all four walk to the princess, and the Musketeer keeps it;
//!   2. a walker without a projectile at the doomed princess: the near Knight keeps it under standing and
//!      projectile_walkers_take_king, and lets go of it under walkers_take_king;
//!   3. the shipped value is projectile_walkers_take_king (since the 2026-09-28 round 9 flip).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test doomed_lane_tower`):
//!   * `doomed_lane_tower_standing` -- the new arms still walk on to a doomed princess tower: (1) goes red.
//!   * `doomed_lane_walker_keeps` -- under walkers_take_king a walking troop still keeps a doomed princess tower: (2)
//!     goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DoomReading, DoomedLaneTower};
use royalesim::{EntityId, Team};

const PRINCESS_HP: i32 = 40;
/// How long the scene may take to doom the princess (the Musketeer's first shot), ticks.
const MAX_TICKS: u32 = 120;

const MUSKETEER: usize = 0;
const NEAR_DRAGON: usize = 1;
const FAR_DRAGON: usize = 2;
const NEAR_KNIGHT: usize = 3;
const FAR_KNIGHT: usize = 4;
const SPAWNS: [(&str, (i32, i32)); 5] = [
    ("Musketeer", (14500, 18500)),
    ("BabyDragon", (13000, 18500)),
    ("BabyDragon", (16500, 4000)),
    ("Knight", (15500, 19500)),
    ("Knight", (16000, 5500)),
];

/// Red crown tower slots (engine lanes): 0 king, 1 left princess, 2 right princess.
const KING: usize = 0;
const OTHER_PRINCESS: usize = 1;
const PRINCESS: usize = 2;

fn native(v: Vec2) -> (i64, i64) {
    ((v.x / K) as i64, (v.y / K) as i64)
}

fn dist(a: (i64, i64), b: (i64, i64)) -> i64 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    isqrt(dx * dx + dy * dy)
}

/// One unit on one tick.
#[derive(Debug, Clone)]
struct Unit {
    pos: (i64, i64),
    target: Option<EntityId>,
    phase: AttackPhase,
    /// The Red crown tower slot its route's goal stands nearest, None with no route.
    goal: Option<usize>,
}

struct Scene {
    princess: EntityId,
    /// Native positions of the Red crown towers by slot.
    towers: [(i64, i64); 3],
    /// The engine's reading of the princess on the tick its doom first showed, and on the tick before.
    doomed_on: u32,
    reading: DoomReading,
    before: DoomReading,
    /// The units on the doom tick and on the tick after.
    at_doom: Vec<Unit>,
    after: Vec<Unit>,
}

fn scene(arm: DoomedLaneTower) -> Scene {
    let mut cfg = config();
    cfg.calib.doomed_lane_tower = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tower_hp(Team::Red, PRINCESS, PRINCESS_HP).expect("the Red right princess tower takes the scene's hp");
    let ids_red = s.tower_ids(Team::Red);
    let princess = ids_red[PRINCESS].expect("the Red right princess tower stands");
    let towers = [0, 1, 2].map(|k| native(s.entity(ids_red[k].expect("every Red crown tower stands")).expect("alive").pos));
    let specs: Vec<(Team, &str, Vec2, Option<i32>)> = SPAWNS.iter().map(|&(c, p)| (Team::Blue, c, Vec2::new(p.0 * K, p.1 * K), None)).collect();
    let ids = s.scenario_spawn_batch(&specs).unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let read = |s: &BattleState| -> Vec<Unit> {
        ids.iter()
            .map(|id| {
                let e = s.entity(*id).expect("every scene unit lives through the read");
                let goal = e.route.first().map(|g| {
                    let g = native(*g);
                    (0..3).min_by_key(|&k| dist(g, towers[k])).expect("three towers")
                });
                Unit { pos: native(e.pos), target: e.target, phase: e.attack_phase, goal }
            })
            .collect()
    };
    let mut before = s.doom_reading(princess).expect("the princess stands");
    let mut doomed_on = None;
    for tick in 1..=MAX_TICKS {
        s.tick();
        let r = s.doom_reading(princess).expect("the princess stands until its doom");
        if r.doomed {
            doomed_on = Some((tick, r));
            break;
        }
        before = r;
    }
    let (doomed_on, reading) = doomed_on.unwrap_or_else(|| panic!("the scene drifted: the princess was not doomed within {MAX_TICKS} ticks"));
    let at_doom = read(&s);
    // The tick after: its Target phase and its Path phase read the doom the tick above ended in.
    s.tick();
    assert!(s.entity(princess).is_some(), "the scene drifted: the princess fell on the tick read, so a walker's switch would be the fall's");
    let after = read(&s);
    Scene { princess, towers, doomed_on, reading, before, at_doom, after }
}

/// The preconditions every reading below stands on.
fn checked(arm: DoomedLaneTower) -> Scene {
    let sc = scene(arm);
    let r = sc.reading;
    assert!(r.pending >= r.hp as i64 && r.last_ms <= 600 && r.hp == PRINCESS_HP, "{arm:?}: the doom tick {} reads {r:?}", sc.doomed_on);
    assert!(!sc.before.doomed, "{arm:?}: the princess was doomed before the tick found");
    for (k, u) in sc.at_doom.iter().enumerate() {
        if k == MUSKETEER {
            assert_eq!(u.target, Some(sc.princess), "{arm:?}: on the doom tick the Musketeer's target is not the princess: {u:?}");
            continue;
        }
        assert_eq!(u.phase, AttackPhase::Idle, "{arm:?}: walker {k} is in its attack on the doom tick: {u:?}");
        assert_eq!(u.goal, Some(PRINCESS), "{arm:?}: walker {k} does not walk to the princess on the doom tick: {u:?}");
    }
    for k in [NEAR_DRAGON, NEAR_KNIGHT] {
        assert_eq!(sc.at_doom[k].target, Some(sc.princess), "{arm:?}: near walker {k} has not taken the princess in sight: {:?}", sc.at_doom[k]);
    }
    for k in [FAR_DRAGON, FAR_KNIGHT] {
        let u = &sc.at_doom[k];
        assert_eq!(u.target, None, "{arm:?}: far walker {k} has a target: {u:?}");
        let (other, king) = (dist(u.pos, sc.towers[OTHER_PRINCESS]), dist(u.pos, sc.towers[KING]));
        assert!(other < king, "{arm:?}: far walker {k} at {:?} stands {other} from the other princess and {king} from the king", u.pos);
    }
    // An attacker keeps the doomed princess under every arm (the Musketeer has shot at it).
    let m = &sc.after[MUSKETEER];
    assert_eq!(m.target, Some(sc.princess), "{arm:?}: the attacking Musketeer let go of the doomed princess: {m:?}");
    sc
}

#[test]
fn a_projectile_walker_walks_to_the_king_once_its_princess_is_doomed() {
    for arm in [DoomedLaneTower::ProjectileWalkersTakeKing, DoomedLaneTower::WalkersTakeKing] {
        let sc = checked(arm);
        for k in [NEAR_DRAGON, FAR_DRAGON] {
            let u = &sc.after[k];
            assert_eq!(u.target, None, "{arm:?}: dragon {k} on the tick after the doom: {u:?}");
            assert_eq!(u.goal, Some(KING), "{arm:?}: dragon {k} does not walk to the king on the tick after the doom: {u:?}");
        }
        let want = if arm == DoomedLaneTower::WalkersTakeKing { KING } else { PRINCESS };
        assert_eq!(sc.after[FAR_KNIGHT].goal, Some(want), "{arm:?}: the far Knight on the tick after the doom: {:?}", sc.after[FAR_KNIGHT]);
    }
    let sc = checked(DoomedLaneTower::Standing);
    for k in [NEAR_DRAGON, FAR_DRAGON, NEAR_KNIGHT, FAR_KNIGHT] {
        assert_eq!(sc.after[k].goal, Some(PRINCESS), "standing: walker {k} left the doomed princess: {:?}", sc.after[k]);
    }
}

#[test]
fn a_walker_without_a_projectile_keeps_a_doomed_princess_unless_every_walker_lets_go() {
    for arm in [DoomedLaneTower::Standing, DoomedLaneTower::ProjectileWalkersTakeKing] {
        let sc = checked(arm);
        assert_eq!(sc.after[NEAR_KNIGHT].target, Some(sc.princess), "{arm:?}: the near Knight let go of the doomed princess: {:?}", sc.after[NEAR_KNIGHT]);
    }
    let sc = checked(DoomedLaneTower::WalkersTakeKing);
    let u = &sc.after[NEAR_KNIGHT];
    assert_ne!(u.target, Some(sc.princess), "walkers_take_king: the near Knight kept the doomed princess: {u:?}");
}

#[test]
fn the_shipped_value_is_projectile_walkers_take_king() {
    assert_eq!(Calib::shipped().doomed_lane_tower, DoomedLaneTower::ProjectileWalkersTakeKing);
    assert_eq!(config().calib.doomed_lane_tower, DoomedLaneTower::ProjectileWalkersTakeKing);
}
