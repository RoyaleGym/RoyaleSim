//! THE SUPER LAVA HOUND EVENT CARD (item 300; card.rs `HopDef`, `death_hop`, `CardDb::rarity_scaled`; spell.rs
//! `launch_hop`; state.rs `phase_reap`; spawner.DEATH_PROJECTILE_COPIES): its death leaves two fire walls on its point, each
//! landing as the Phoenix's death projectile does and then hopping on twice along its owner's forward axis, each hop on
//! the hop row's own Rare ladder and sparing what its chain already hit.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-SuperLavaHound-s0, level 11): the death on D + 1 (t679) left two carriers
//! and two SuperLavaHound2; D + 2 the Musketeer 336 off took 614 = 2 x 307 (Common ladder); hops landed 2000 on D + 6 and
//! 4000 on D + 10, the first taking 154 = 2 x 77 (ceil(254 x 30 %), Rare ladder) off a princess tower 3241 off, the second
//! sparing it; the Musketeer, 2328 from the first hop, was spared too.
//!
//! WHAT IS PINNED: a Blue Super Lava Hound killed on that point beside a Red Musketeer (held), its SuperLavaHound2 held far
//! off: the tick K it is gone, the Musketeer loses 614 on K + 1 and nothing more by K + 10, the red right princess tower
//! loses 154 on K + 5 and nothing on K + 9.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test super_lava_hound`):
//!   * `fire_wall_never_hops` -- the carriers land and launch nothing: the tower is never hit, red;
//!   * `death_projectile_single_copy` -- one carrier whatever the key: the Musketeer loses 307, red;
//!   * `hop_on_card_ladder` -- the hop on the dead unit's Common ladder: the tower loses 186, red;
//!   * `fire_wall_refused` -- the card is refused again: red (and tests/spawn_chain.rs (5)).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeathProjectileCopies, DeathSpawnProjectile};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const POINT: (i32, i32) = (14677, 20264);
const MUSK: (i32, i32) = (14601, 19937);

/// The Musketeer's and the red right princess tower's hp, from the tick before K to K + 10.
fn fire_wall() -> (Vec<i32>, Vec<i32>) {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.death_spawn_projectile = DeathSpawnProjectile::ClientProjectile;
    cfg.calib.death_projectile_copies = DeathProjectileCopies::Client15535PerDeathSpawnMember;
    let mut s = BattleState::new(15, cfg);
    past_deploy_lockout(&mut s);
    let hound = s.scenario_spawn_now(Team::Blue, "SuperLavaHound", at(POINT), None).expect("the Super Lava Hound loads and stands");
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", at(MUSK), None).expect("the Musketeer");
    let tower: EntityId = s
        .entities()
        .find(|e| e.team == Team::Red && e.kind == royalesim::entity::EntityKind::PrincessTower && e.pos.x > at((9000, 0)).x)
        .expect("the red right princess tower")
        .id;
    let hold = |s: &mut BattleState| {
        s.debug_set_pos(musk, at(MUSK));
        let far: Vec<EntityId> = s.entities().filter(|e| e.card == "SuperLavaHound2" || e.card == "LavaPups").map(|e| e.id).collect();
        for id in far {
            s.debug_set_pos(id, at((2000, 8000)));
        }
    };
    for _ in 0..30 {
        s.debug_set_pos(hound, at(POINT));
        hold(&mut s);
        s.tick();
    }
    let full = s.entity(musk).expect("the Musketeer").max_hp;
    assert!(s.debug_set_hp(musk, full) && s.debug_set_hp(hound, 0));
    let (mut m, mut t) = (Vec::new(), Vec::new());
    let read = |s: &BattleState, m: &mut Vec<i32>, t: &mut Vec<i32>| {
        m.push(s.entity(musk).map_or(-1, |e| e.hp));
        t.push(s.entity(tower).map_or(-1, |e| e.hp));
    };
    read(&s, &mut m, &mut t);
    for _ in 0..11 {
        s.debug_set_pos(hound, at(POINT));
        hold(&mut s);
        s.tick();
        if m.len() == 1 {
            assert!(s.entity(hound).is_none(), "the scene drifted: the hound at 0 hitpoints stood through its tick");
        }
        read(&s, &mut m, &mut t);
    }
    (m, t)
}

/// Plants: fire_wall_never_hops, death_projectile_single_copy, hop_on_card_ladder, fire_wall_refused.
#[test]
fn two_fire_walls_blast_then_hop_twice_on_the_rare_ladder() {
    let (m, t) = fire_wall();
    // v[0] before K, v[1] on K, v[k + 1] on K + k: the loss on K + k is v[k] - v[k + 1].
    let lost = |v: &[i32], k: usize| v[k] - v[k + 1];
    assert_eq!(lost(&m, 1), 614, "K + 1: two carrier blasts of 307 on the Musketeer: {m:?}");
    assert!((2..=10).all(|k| lost(&m, k) == 0), "the Musketeer is spared by the hops: {m:?}");
    assert_eq!(lost(&t, 5), 154, "K + 5: two first hops of 77 (ceil(254 x 30 %)) on the tower: {t:?}");
    assert_eq!(lost(&t, 9), 0, "K + 9: the second hops spare the tower their chains hit: {t:?}");
}
