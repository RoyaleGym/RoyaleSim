//! THE GOBLIN PARTY ROCKET EVENT CARD (item 296; card.rs `SpellShape::Projectile::area` and the one-shot chain of
//! `area_effect_shape_hits`, spell.rs `step_spells`'s landing, status.rs `BuffDeathSpawn::suppresses_own`, state.rs
//! `phase_reap`; status.BUFF_DEATH_SPAWN_UNDELAYED_DEPLOY): the Rocket's flight, whose landing makes a chain of three one-shot
//! areas: a curse on the caster's own troops, a curse on the enemy's, and a 9999 hit on both sides (0 on crown towers). Each
//! cursed unit it kills leaves a GoblinParty for the caster, in place of its own death spawn.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-GoblinPartyRocket-s0, level 11, one cast): the projectile flew 350 a tick from
//! the caster's king and was gone on T; the Knight and three Skeletons within 4000 of the aim stood at full hp through T + 2
//! and were gone on T + 3; four caster units of 202 hp hitting for 119 (GoblinParty at level 11) came out of them, the first
//! in a tower's reach 40 ticks after the kill, which a zero deploy misses by 20 or more.
//!
//! WHAT IS PINNED: (1) a Blue cast over three Red troops held in place: nothing hit on T .. T + 2, all three gone on T + 3,
//! three Blue GoblinParty of 202 hp made on T + 3 and none Red, no crown tower hit, and under the 15.535 arm the goblins
//! deploy 20 ticks. (2) The rows' reading on a mixed crowd: a Red Knight, a Blue Knight, a Red Golem and a Red Cannon in
//! reach: three Blue goblins (one per cursed troop of either side), no Golemite, the Cannon gone without a goblin.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test goblin_party_rocket`):
//!   * `party_rocket_lands_nothing` -- the landing makes no area: (1) and (2) go red;
//!   * `own_death_spawn_kept` -- the Golem's Golemites come beside its goblin: (2) goes red;
//!   * `party_goblin_deploys_at_once` -- the 15.535 arm deploys the goblins at once: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, BuffDeathSpawnUndelayed};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn level11(undelayed: BuffDeathSpawnUndelayed) -> BattleConfig {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.buff_death_spawn_undelayed = undelayed;
    cfg
}

fn crown_hp(s: &BattleState) -> Vec<i32> {
    s.entities().filter(|e| e.kind.is_crown_tower()).map(|e| e.hp).collect()
}

/// What a cast over `crowd` (team, card, point; each held there) did: the landing tick T (the first tick the flight is
/// gone), the tick each of the crowd was gone, whether one was hit while it stood after the landing, the goblins made
/// (tick, team, max hp, still deploying 10 ticks on),
/// the units of `extra` seen after the cast, and whether a crown tower lost hp.
struct Run {
    landed: u32,
    gone: Vec<Option<u32>>,
    hit_early: bool,
    goblins: Vec<(u32, Team, i32, bool)>,
    extra_seen: usize,
    crown_hit: bool,
}

fn cast(cfg: BattleConfig, aim: (i32, i32), crowd: &[(Team, &str, (i32, i32))], extra: &str) -> Run {
    let mut s = BattleState::new(15, cfg);
    past_deploy_lockout(&mut s);
    let ids: Vec<(EntityId, Vec2)> = crowd
        .iter()
        .map(|&(team, card, p)| (s.scenario_spawn_now(team, card, at(p), None).unwrap_or_else(|e| panic!("{card}: {e:?}")), at(p)))
        .collect();
    for _ in 0..30 {
        for &(id, p) in &ids {
            s.debug_set_pos(id, p);
        }
        s.tick();
    }
    let hp0: Vec<i32> = ids.iter().map(|&(id, _)| s.entity(id).map_or(0, |e| e.hp)).collect();
    let crowns = crown_hp(&s);
    s.spawn_unit(Team::Blue, "GoblinPartyRocket", at(aim), None).expect("the Goblin Party Rocket is cast");
    let mut flying = false;
    let mut landed: Option<u32> = None;
    let mut gone: Vec<Option<u32>> = vec![None; ids.len()];
    let mut hit_early = false;
    let mut goblins: Vec<(EntityId, u32, Team, i32)> = Vec::new();
    let mut deploying_10: Vec<bool> = Vec::new();
    let mut extra_seen = 0;
    for _ in 0..120 {
        for &(id, p) in &ids {
            s.debug_set_pos(id, p);
        }
        s.tick();
        let t = s.tick_count() - 1;
        let in_flight = s.spells().iter().any(|sp| matches!(sp.motion, SpellMotion::Flight { .. }));
        if in_flight {
            flying = true;
        } else if flying && landed.is_none() {
            landed = Some(t);
        }
        for (k, &(id, _)) in ids.iter().enumerate() {
            match s.entity(id) {
                None => {
                    if gone[k].is_none() {
                        gone[k] = Some(t);
                    }
                }
                Some(e) => {
                    if landed.is_some() && e.hp < hp0[k] {
                        hit_early = true;
                    }
                }
            }
        }
        for e in s.entities().filter(|e| e.card == "GoblinParty") {
            if !goblins.iter().any(|g| g.0 == e.id) {
                goblins.push((e.id, t, e.team, e.max_hp));
                deploying_10.push(false);
            }
        }
        for (g, d) in goblins.iter().zip(deploying_10.iter_mut()) {
            if t == g.1 + 10 {
                *d = s.entity(g.0).is_some_and(|e| e.deploying);
            }
        }
        extra_seen = extra_seen.max(s.entities().filter(|e| e.card == extra).count());
    }
    let landed = landed.expect("the flight landed");
    Run {
        landed,
        gone,
        hit_early,
        goblins: goblins.iter().zip(deploying_10).map(|(g, d)| (g.1, g.2, g.3, d)).collect(),
        extra_seen,
        crown_hit: crown_hp(&s).iter().zip(crowns.iter()).any(|(a, b)| a < b),
    }
}

/// (1) The capture's timing and its goblins. Plants: party_rocket_lands_nothing, party_goblin_deploys_at_once.
#[test]
fn the_chain_kills_on_the_landing_plus_3_and_each_victim_leaves_a_caster_goblin() {
    let crowd = [(Team::Red, "Knight", (14500, 17500)), (Team::Red, "Musketeer", (13000, 19000)), (Team::Red, "Valkyrie", (16500, 18500))];
    let r = cast(level11(BuffDeathSpawnUndelayed::Client15535UnitDeployTime), (14500, 17500), &crowd, "");
    let t = r.landed;
    assert_eq!(r.gone, vec![Some(t + 3); 3], "the three, at full hp to T + 2, are gone on T + 3 (T = {t})");
    assert!(!r.hit_early, "nothing is hit between the landing and the kill");
    let made: Vec<(u32, Team, i32)> = r.goblins.iter().map(|g| (g.0, g.1, g.2)).collect();
    assert_eq!(made, vec![(t + 3, Team::Blue, 202); 3], "one Blue GoblinParty of 202 hp per victim, made on T + 3, none Red");
    assert!(r.goblins.iter().all(|g| g.3), "the 15.535 arm: each goblin is still deploying 10 ticks on (DeployTime 1000)");
    assert!(!r.crown_hit, "no crown tower is hit");
    // NOT VACUOUS: the shipped arm's goblins act at once.
    let z = cast(level11(BuffDeathSpawnUndelayed::Zero), (14500, 17500), &crowd, "");
    assert!(z.goblins.len() == 3 && z.goblins.iter().all(|g| !g.3), "zero: the goblins act at once: {:?}", z.goblins);
}

/// (2) Both sides' troops, a building and a death spawn: one goblin per cursed troop, all for the caster; no Golemite (the
/// curse takes the place of the Golem's own death spawn); the Cannon, uncursed, destroyed and leaving nothing. Plants:
/// party_rocket_lands_nothing, own_death_spawn_kept.
#[test]
fn both_sides_die_the_cursed_troops_leave_caster_goblins_and_no_golemite() {
    let crowd = [
        (Team::Red, "Knight", (9000, 20000)),
        (Team::Blue, "Knight", (6500, 19500)),
        (Team::Red, "Golem", (10500, 20000)),
        (Team::Red, "Cannon", (12000, 22500)),
    ];
    let r = cast(level11(BuffDeathSpawnUndelayed::Client15535UnitDeployTime), (9000, 20000), &crowd, "Golemite");
    let t = r.landed;
    assert_eq!(r.gone, vec![Some(t + 3); 4], "all four are gone on T + 3 (T = {t})");
    let teams: Vec<Team> = r.goblins.iter().map(|g| g.1).collect();
    assert_eq!(teams, vec![Team::Blue; 3], "three goblins, all the caster's: {:?}", r.goblins);
    assert_eq!(r.extra_seen, 0, "no Golemite: the Golem's curse takes the place of its own death spawn");
}
