//! INVISIBLE WHEN IDLE (card.rs `CardDef::invisible_when_idle`; target.rs `invisible`; targeting.INVISIBILITY): the
//! Royal Ghost.
//!
//! THE LAW, measured on client 15.535.29 (the corpus battle and four scenario runs):
//!   - invisible from its deploy: no enemy targets it before its first hit, however near;
//!   - its own hit reveals it: enemies may target it from the tick after the hit;
//!   - it hides again 46 ticks after its last hit, which drops an enemy's lock;
//!   - area damage lands on it while it is invisible.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Ghost's idle invisibility (2000 ms) and no hide timer;
//!   2. a red princess tower first targets a Blue Ghost walking at it on the tick after the Ghost's first hit, though
//!      the Ghost stood within the tower's reach before;
//!   3. the tower that took the Ghost on a hit holds it through hit + 45 and drops it on hit + 46, the Ghost walking
//!      and not hitting between;
//!   4. a Zap lands on an invisible Ghost;
//!   5. the Ghost's reveal is state: a save edited only in it fails the load's hash self-check;
//!   6. the export's `status_flags` bit 1 is that same targeting predicate: read after tick k it says whether an
//!      enemy may target the Ghost on tick k + 1 (set before the hit, clear from the hit through hit + 44, set again
//!      on hit + 45, the tower dropping it on hit + 46).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test invisibility`):
//!   * `invisible_targetable` -- invisibility not read: (2) goes red.
//!   * `reveal_never` -- a hit reveals nothing: (2), (3) and (6) go red.
//!   * `rehide_never` -- once revealed, visible for good: (3) and (6) go red.
//!   * `hash_skips_reveal` -- the reveal is not hashed: (5) goes red.
//!   * `invisible_area_immune` -- spell.rs `impact` skips a unit invisible when idle: (4) goes red.
//!   * `rehide_from_hit` -- client16402_idle_after_attack still counts from the hit: (7) goes red.
//!   * `reattack_reveals_at_start` -- client16402_hit_reveal_idle_after_attack: a swing after a re-hide shows it: (8) goes red.
//!
//! (7) targeting.INVISIBILITY = client16402_idle_after_attack (client 16.402, parity r75, live13: 26 of 26 let-gos on the
//! attack's end + 40): a second Skeleton held in the Ghost's reach after its first kill, killed in the Ghost's windup; the
//! tower holds the Ghost through its attack's end E + 40 and drops it on E + 41, where client_until_hit drops it on the
//! first hit + 46.
//!
//! (8) targeting.INVISIBILITY = client16402_hit_reveal_idle_after_attack (client 16.402, parity r78, live13: 0 of 94 re-attacks
//! after a re-hide held between their start and their hit): the Ghost held still after its first hit until it hides again,
//! then a red Skeleton held before it; the tower first holds it on its second hit + 1, where client16402_idle_after_attack
//! holds it from its second swing's start.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::entity::AttackPhase;
use royalesim::state::{BattleState, Invisibility};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

#[test]
fn the_loader_reads_the_ghosts_idle_invisibility() {
    let s = BattleState::new(0, config());
    let g = card_stat(&s, "Ghost");
    assert_eq!(g.invisible_when_idle, Some(2000), "BuffWhenNotAttackingTime");
}

/// The red princess tower nearest `p`.
fn red_tower_near(s: &BattleState, p: Vec2) -> (EntityId, Vec2) {
    s.entities().filter(|v| v.team == Team::Red && v.card == "PrincessTower").map(|v| (v.id, v.pos)).min_by_key(|v| dist(v.1, p)).expect("a red princess tower")
}

/// Per tick after the play (k = 0 the Ghost's creation tick): whether `tower` targets the Ghost, the Ghost's
/// distance to it, and whether the Ghost hit something (a red unit lost hp while the tower did not shoot the Ghost).
struct Tick {
    targets_ghost: bool,
    dist: i64,
    ghost_hit: bool,
    /// The Ghost's exported `status_flags` bit 1 after this tick.
    invisible_bit: bool,
}

fn run(s: &mut BattleState, ghost_at: (i32, i32), tower: (EntityId, Vec2), ticks: u32) -> Vec<Tick> {
    s.spawn_unit(Team::Blue, "Ghost", at(ghost_at), None).expect("play the Ghost");
    let mut out = Vec::new();
    for _ in 0..ticks {
        let red_hp: i64 = s.entities().filter(|v| v.team == Team::Red).map(|v| v.hp as i64).sum();
        s.tick();
        let Some((g, bit)) = s.entities().find(|v| v.card == "Ghost").map(|v| ((v.id, v.pos), v.status_flags & 2 != 0)) else { break };
        let red_after: i64 = s.entities().filter(|v| v.team == Team::Red).map(|v| v.hp as i64).sum();
        let t = s.entity(tower.0);
        out.push(Tick {
            targets_ghost: t.and_then(|t| t.target) == Some(g.0),
            dist: dist(g.1, tower.1),
            ghost_hit: red_after < red_hp,
            invisible_bit: bit,
        });
    }
    out
}

/// Plants: invisible_targetable, reveal_never.
#[test]
fn the_tower_first_targets_the_ghost_on_the_tick_after_its_first_hit() {
    let mut s = BattleState::new(0, config());
    let tower = red_tower_near(&s, at((14500, 26000)));
    let ticks = run(&mut s, (14500, 12000), tower, 400);
    let h = ticks.iter().position(|t| t.ghost_hit).expect("the scene drifted: the Ghost never hit");
    let t = ticks.iter().position(|t| t.targets_ghost).expect("the tower never targeted the Ghost");
    assert!(ticks[..h].iter().any(|t| t.dist <= 9000), "the scene drifted: the Ghost was never within the tower's reach before its first hit");
    assert_eq!(t, h + 1, "the tower first targets the Ghost on {t}; its first hit was on {h}");
}

/// Plants: reveal_never, rehide_never.
#[test]
fn the_tower_drops_the_ghost_46_ticks_after_its_last_hit() {
    let mut s = BattleState::new(0, config());
    let tower = red_tower_near(&s, at((14500, 26000)));
    // a red Skeleton behind the tower, on the Ghost's lane. It cannot see the invisible Ghost, so it walks out down
    // the lane toward Blue's towers. The Ghost's first hit kills it inside the tower's reach, and the Ghost then walks
    // on toward the tower for longer than 46 ticks without hitting. A Skeleton at (14500, 19000) walks to meet the
    // Ghost on Blue's side, 12003 from the tower, and the tower cannot take it there.
    s.scenario_spawn_now(Team::Red, "Skeleton", at((14500, 28500)), None).expect("spawn the Skeleton");
    let ticks = run(&mut s, (14500, 12000), tower, 400);
    let h = ticks.iter().position(|t| t.ghost_hit).expect("the scene drifted: the Ghost never hit");
    assert!(ticks.len() > h + 47, "the scene drifted: the Ghost died before hit + 46");
    assert!(ticks[h + 1..=h + 46].iter().all(|t| !t.ghost_hit), "the scene drifted: the Ghost hit again before hit + 46");
    assert!(ticks[h + 1].targets_ghost, "the scene drifted: the tower did not take the Ghost on hit + 1 (it stood {} away)", ticks[h + 1].dist);
    let held: Vec<usize> = (h + 1..=h + 46).filter(|&k| ticks[k].targets_ghost).map(|k| k - h).collect();
    assert_eq!(held, (1..=45).collect::<Vec<usize>>(), "the ticks after the hit on which the tower held the Ghost");
}

/// Plants: reveal_never, rehide_never.
#[test]
fn the_exported_invisible_bit_is_the_targeting_predicate() {
    let mut s = BattleState::new(0, config());
    let tower = red_tower_near(&s, at((14500, 26000)));
    s.scenario_spawn_now(Team::Red, "Skeleton", at((14500, 28500)), None).expect("spawn the Skeleton");
    let ticks = run(&mut s, (14500, 12000), tower, 400);
    let h = ticks.iter().position(|t| t.ghost_hit).expect("the scene drifted: the Ghost never hit");
    assert!(ticks.len() > h + 47, "the scene drifted: the Ghost died before hit + 46");
    assert!(ticks[h + 1..=h + 46].iter().all(|t| !t.ghost_hit), "the scene drifted: the Ghost hit again before hit + 46");
    assert!(ticks[..h].iter().all(|t| t.invisible_bit), "bit 1 was clear before the Ghost's first hit");
    let clear: Vec<usize> = (h..=h + 46).filter(|&k| !ticks[k].invisible_bit).map(|k| k - h).collect();
    assert_eq!(clear, (0..=44).collect::<Vec<usize>>(), "the ticks after the hit on which bit 1 was clear");
    // The same predicate, as targeting acted on it: the tower holds the Ghost on hit + 1 ..= hit + 45 and not on + 46.
    assert!(ticks[h + 45].targets_ghost && !ticks[h + 46].targets_ghost, "the tower's hold moved: the bit is not the one targeting read");
}

#[test]
fn a_zap_lands_on_an_invisible_ghost() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "Ghost", at((9000, 8000)), None).expect("play the Ghost");
    for _ in 0..30 {
        s.tick();
    }
    let (g, p, hp) = s.entities().find(|v| v.card == "Ghost").map(|v| (v.id, v.pos, v.hp)).expect("the Ghost stands");
    s.spawn_unit(Team::Red, "Zap", p, None).expect("cast Zap");
    for _ in 0..3 {
        s.tick();
    }
    assert!(s.entity(g).map_or(0, |v| v.hp) < hp, "the Zap did not land on the invisible Ghost");
}

/// Plant: hash_skips_reveal.
#[test]
fn the_ghosts_reveal_is_state() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "Ghost", at((9000, 8000)), None).expect("play the Ghost");
    for _ in 0..3 {
        s.tick();
    }
    let g = s.entities().find(|v| v.card == "Ghost").map(|v| v.id).expect("the Ghost stands");
    let hashed = edit_is_hashed(&s, |v| {
        let col = v["ents"]["reveal_from"].as_array_mut().expect("the snapshot carries the reveal");
        col[g.index as usize] = serde_json::Value::from(7);
    });
    assert!(hashed, "a save edited only in the Ghost's reveal loads under the old hash: the reveal is not hashed");
}

/// Under targeting.INVISIBILITY = `arm`: the 46-tick scene (a red Skeleton behind the tower meeting the Ghost); after the
/// Ghost's first hit h a second red Skeleton held (topped up) 700 ahead of it; once the Ghost has attacked it 8 ticks (its
/// windup, no hit yet) the Skeleton is set to 0. Returns (h, E: the first tick after the kill on which the Ghost's attack
/// phase reads Idle, per tick whether the tower targets the Ghost).
fn lost_swing(arm: Invisibility) -> (usize, usize, Vec<bool>) {
    let mut cfg = config();
    cfg.calib.invisibility = arm;
    let mut s = BattleState::new(0, cfg);
    let tower = red_tower_near(&s, at((14500, 26000)));
    s.scenario_spawn_now(Team::Red, "Skeleton", at((14500, 28500)), None).expect("spawn the Skeleton");
    s.spawn_unit(Team::Blue, "Ghost", at((14500, 12000)), None).expect("play the Ghost");
    let (mut h, mut second, mut attacking, mut killed, mut idle) = (None, None, 0, None, None);
    let mut holds = Vec::new();
    for k in 0..400 {
        let red_hp: i64 = s.entities().filter(|v| v.team == Team::Red && v.card != "PrincessTower" && v.card != "KingTower").map(|v| v.hp as i64).sum();
        if let Some((sk, p)) = second {
            if killed.is_none() {
                if let Some(e) = s.entity(sk) {
                    let full = e.max_hp;
                    assert!(s.debug_set_pos(sk, p) && s.debug_set_hp(sk, full));
                }
            }
        }
        let ghost = s.entities().find(|v| v.card == "Ghost").map(|v| (v.id, v.max_hp));
        if let Some(g) = ghost {
            assert!(s.debug_set_hp(g.0, g.1));
        }
        s.tick();
        let Some((g, gpos, phase)) = s.entities().find(|v| v.card == "Ghost").map(|v| (v.id, v.pos, v.attack_phase)) else { break };
        let red_after: i64 = s.entities().filter(|v| v.team == Team::Red && v.card != "PrincessTower" && v.card != "KingTower").map(|v| v.hp as i64).sum();
        holds.push(s.entity(tower.0).and_then(|t| t.target) == Some(g));
        if h.is_none() && red_after < red_hp {
            h = Some(k);
            let p = Vec2::new(gpos.x, gpos.y + 700 * K);
            second = Some((s.scenario_spawn_now(Team::Red, "Skeleton", p, None).expect("the second Skeleton"), p));
            continue;
        }
        let Some(hit) = h else { continue };
        if killed.is_none() && k > hit + 6 {
            assert!(red_after >= red_hp, "the scene drifted: the Ghost hit the second Skeleton on {k}");
            attacking = if phase != AttackPhase::Idle { attacking + 1 } else { 0 };
            if attacking == 8 {
                let sk = second.expect("the second Skeleton").0;
                assert!(s.debug_set_hp(sk, 0));
                killed = Some(k);
            }
        } else if let Some(kk) = killed {
            if idle.is_none() && k > kk && phase == AttackPhase::Idle {
                idle = Some(k);
            }
            if idle.is_some_and(|e| k > e + 45) {
                break;
            }
        }
    }
    let h = h.expect("the scene drifted: the Ghost never hit");
    let e = idle.unwrap_or_else(|| panic!("the scene drifted: the Ghost's second attack never ended (killed {killed:?})"));
    (h, e, holds)
}

/// (7) Plant: rehide_from_hit.
#[test]
fn under_client16402_idle_after_attack_the_tower_holds_the_ghost_40_ticks_past_its_attacks_end() {
    let (h, e, held) = lost_swing(Invisibility::Client16402IdleAfterAttack);
    assert!(e > h + 5, "the scene drifted: its attack ended on {e}, the hit on {h}");
    assert!(held.len() > e + 41, "the scene drifted: the run ended on {}", held.len());
    assert!((e + 1..=e + 40).all(|k| held[k]), "client16402_idle_after_attack: the tower let the Ghost go before its attack's end + 40 ({h}, {e}): {:?}", &held[e..]);
    assert!(!held[e + 41], "client16402_idle_after_attack: the tower held the Ghost on its attack's end + 41");
    // NOT VACUOUS: client_until_hit drops it on the hit + 46, before the attack's end + 41.
    let (h0, e0, old) = lost_swing(Invisibility::ClientUntilHit);
    assert!(h0 + 46 < e0 + 41 && old[h0 + 45] && !old[h0 + 46], "client_until_hit: the tower's hold ({h0}, {e0})");
}

/// Under targeting.INVISIBILITY = `arm`: the 46-tick scene; after the Ghost's first hit h it is held still (no red unit in its
/// reach) and topped up until it hides again; on h + 60 a red Skeleton is spawned 700 ahead of it, held. Returns (S2: the first
/// tick after h + 60 its attack phase leaves Idle, H2: the first tick after S2 a red unit loses hp, A2: the first tick after
/// h + 60 the tower targets it).
fn second_reveal(arm: Invisibility) -> (usize, usize, usize) {
    let mut cfg = config();
    cfg.calib.invisibility = arm;
    let mut s = BattleState::new(0, cfg);
    let tower = red_tower_near(&s, at((14500, 26000)));
    s.scenario_spawn_now(Team::Red, "Skeleton", at((14500, 28500)), None).expect("spawn the Skeleton");
    s.spawn_unit(Team::Blue, "Ghost", at((14500, 12000)), None).expect("play the Ghost");
    let (mut h, mut still, mut second): (Option<usize>, Option<Vec2>, Option<(EntityId, Vec2)>) = (None, None, None);
    let (mut s2, mut h2, mut a2) = (None, None, None);
    let reds = |s: &BattleState| -> i64 { s.entities().filter(|v| v.team == Team::Red && v.card != "PrincessTower" && v.card != "KingTower").map(|v| v.hp as i64).sum() };
    for k in 0..500 {
        let ghost = s.entities().find(|v| v.card == "Ghost").map(|v| (v.id, v.max_hp));
        if let Some((g, full)) = ghost {
            assert!(s.debug_set_hp(g, full));
            if let Some(p) = still {
                assert!(s.debug_set_pos(g, p));
            }
        }
        if let Some((sk, p)) = second {
            let full = s.entity(sk).map(|e| e.max_hp);
            if let Some(full) = full {
                assert!(s.debug_set_pos(sk, p) && s.debug_set_hp(sk, full));
            }
        }
        let red_hp = reds(&s);
        s.tick();
        let Some((g, gpos, phase)) = s.entities().find(|v| v.card == "Ghost").map(|v| (v.id, v.pos, v.attack_phase)) else { break };
        let red_after = reds(&s);
        let held = s.entity(tower.0).and_then(|t| t.target) == Some(g);
        let Some(hit) = h else {
            if red_after < red_hp {
                h = Some(k);
                still = Some(gpos);
            }
            continue;
        };
        if k == hit + 60 {
            assert!(!held, "the scene drifted: the tower still holds the Ghost on its first hit + 60");
            let p = Vec2::new(gpos.x, gpos.y + 700 * K);
            second = Some((s.scenario_spawn_now(Team::Red, "Skeleton", p, None).expect("the second Skeleton"), p));
        } else if k > hit + 60 {
            if s2.is_none() && phase != AttackPhase::Idle {
                s2 = Some(k);
            }
            if s2.is_some() && h2.is_none() && red_after < red_hp {
                h2 = Some(k);
            }
            if a2.is_none() && held {
                a2 = Some(k);
            }
            if h2.is_some() && a2.is_some() {
                break;
            }
        }
    }
    let f = |v: Option<usize>, what: &str| v.unwrap_or_else(|| panic!("the scene drifted: no {what} (h {h:?})"));
    (f(s2, "second swing"), f(h2, "second hit"), f(a2, "second hold"))
}

/// (8) Plant: reattack_reveals_at_start.
#[test]
fn under_client16402_hit_reveal_a_rehidden_ghost_is_shown_by_its_hit_not_its_swing() {
    let (s2, h2, a2) = second_reveal(Invisibility::Client16402HitRevealIdleAfterAttack);
    assert!(h2 > s2 + 5, "the scene drifted: its second swing started on {s2}, hit on {h2}");
    assert_eq!(a2, h2 + 1, "client16402_hit_reveal_idle_after_attack: the tower first held the re-hidden Ghost on {a2} (swing {s2}, hit {h2})");
    // NOT VACUOUS: client16402_idle_after_attack shows it from its swing's start.
    let (s0, h0, a0) = second_reveal(Invisibility::Client16402IdleAfterAttack);
    assert!(a0 <= s0 + 1 && a0 < h0 + 1, "client16402_idle_after_attack shows it from its swing: {s0} {h0} {a0}");
}

/// (7) under the corrected arm: its first attack's hold is client16402_idle_after_attack's (through its end + 40).
#[test]
fn under_client16402_hit_reveal_the_tower_still_holds_the_ghost_40_ticks_past_its_attacks_end() {
    let (h, e, held) = lost_swing(Invisibility::Client16402HitRevealIdleAfterAttack);
    assert!(e > h + 5 && held.len() > e + 41, "the scene drifted: hit {h}, end {e}, run {}", held.len());
    assert!((e + 1..=e + 40).all(|k| held[k]) && !held[e + 41], "client16402_hit_reveal_idle_after_attack: the hold after its attack's end ({h}, {e}): {:?}", &held[e..]);
}
