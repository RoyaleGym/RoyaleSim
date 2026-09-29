//! THE HERO BALLOON (card.rs `AbilityEffect::Throw`; state.rs `throw_pass`, `throw_pick`), against client 15.535.29 at
//! level 11.
//!
//! THE MEASUREMENTS (Oracle's sp-balloon-g6000-s0, -g4500-s0, -g2500d-s0 and -failsafe-s0; the press's first frame P):
//!   - the button's throw appears on P + 3, 250 ahead of the Balloon at the closest enemy ground troop (g6000: the
//!     Balloon at (10060, 8820) on P + 3, an Ice Golemite standing at (11722, 14451): the throw at (10130, 9059));
//!   - it moves from P + 4 at 201 native a tick for 12 ticks, then 243, 274, 299 and 320 for 3 ticks each, each step
//!     truncated in native units ((10186, 9251) on P + 4);
//!   - it lands on P + 27 there (5,990 away): the Skeletrooper appears on the golem's point, hp 473, deploying 10
//!     frames, and the golem loses 263 on that frame (the trooper's landing area, 103 at level 1);
//!   - a pick killed in flight leaves its last point as the aim (g2500d: the trooper there on schedule);
//!   - with no enemy ground troop within 6500 one throw appears on P + 4 on the Balloon's own point and the trooper
//!     lands there on P + 5 (failsafe).
//!
//! THE SCENES hold the Balloon at the g6000 point it had on P + 3, and the golem where it stood.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_balloon`):
//!   - throw_never -> `the_throw_lands_on_the_golem_on_p_plus_27` red;
//!   - throw_speed_flat -> `the_throw_lands_on_the_golem_on_p_plus_27` red;
//!   - throw_failsafe_on_time -> `with_nothing_in_reach_the_trooper_drops_on_the_balloon_on_p_plus_5` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Balloon", "Knight", "Archer", "Giant", "Valkyrie", "HogRider", "Fireball", "Zap"];
/// The Balloon on P + 3 in sp-balloon-g6000-s0, and the golem there.
const BALLOON: (i32, i32) = (10060, 8820);
const GOLEM: (i32, i32) = (11722, 14451);

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s
}

/// One frame: every Skeletrooper (point, hp, deploying), the first golem's hp and point, each golem's point, and the
/// Balloon's point (each where its own step of the tick left it: they are put back before every tick).
struct Frame {
    troopers: Vec<(Vec2, i32, bool)>,
    golem: Option<i32>,
    golems: Vec<Option<Vec2>>,
    balloon: Option<Vec2>,
}

/// The Balloon held at `balloon`, red Ice Golemites held at `golems` (none: nothing in reach), deployed; then the press
/// and `frames` frames from k = 0 (P + 1). `kill_at`: the frame before whose tick the first golem is struck to 0.
fn scene(balloon: (i32, i32), golems: &[(i32, i32)], frames: u32, kill_at: Option<u32>) -> Vec<Frame> {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Balloon_hero", n(balloon.0, balloon.1), None).expect("the hero");
    for g in golems {
        s.spawn_unit_resolved(Team::Red, "IceGolemite", n(g.0, g.1), None).expect("a golem");
    }
    s.tick();
    let hero = find_live(&s, Team::Blue, "Balloon_hero")[0].id;
    let ids: Vec<(EntityId, Vec2)> = golems
        .iter()
        .map(|g| (s.entities().find(|e| e.team == Team::Red && e.card == "IceGolemite" && e.pos == n(g.0, g.1)).expect("the golem where it was put").id, n(g.0, g.1)))
        .collect();
    let hold = |s: &mut BattleState| {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(balloon.0, balloon.1)));
        }
        for (id, p) in &ids {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
            }
        }
    };
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    for k in 0..frames {
        if kill_at == Some(k) {
            assert!(s.debug_set_hp(ids[0].0, 0));
        }
        hold(&mut s);
        s.tick();
        let troopers = s.entities().filter(|e| e.card == "SkeletonTrooper").map(|e| (e.pos, e.hp, e.deploying)).collect();
        let golems = ids.iter().map(|(id, _)| s.entity(*id).map(|e| e.pos)).collect();
        out.push(Frame { troopers, golem: ids.first().and_then(|(id, _)| s.entity(*id)).map(|e| e.hp), golems, balloon: s.entity(hero).map(|e| e.pos) });
    }
    out
}

/// P + d is frame k = d - 1.
fn at(d: usize) -> usize {
    d - 1
}

#[test]
fn the_throw_lands_on_the_golem_on_p_plus_27() {
    let f = scene(BALLOON, &[GOLEM], 45, None);
    let land = at(27);
    assert!(f[land - 1].troopers.is_empty(), "nothing before P + 27");
    // The throw homes on the golem's point where its step of the landing tick left it (it stood still in the client,
    // attacking; here it walks a step each tick before it is put back).
    let at = f[land].golems[0].expect("the golem stands");
    assert!(at.dist(n(GOLEM.0, GOLEM.1)) < 100 * K, "one golem step from its point");
    assert_eq!(f[land].troopers, vec![(at, 473, true)], "the Skeletrooper on the golem's point, deploying");
    assert_eq!(f[land - 1].golem.unwrap() - f[land].golem.unwrap(), 263, "the landing blow on its own frame");
    for (k, fr) in f.iter().enumerate().skip(land).take(10) {
        assert!(fr.troopers[0].2, "deploying on its frame {}", k - land + 1);
    }
    assert!(!f[land + 10].troopers[0].2, "deployed on its 11th frame");
}

#[test]
fn a_pick_killed_in_flight_leaves_its_point_as_the_aim() {
    let f = scene(BALLOON, &[GOLEM], 32, Some(10));
    assert!(f[at(27)].golem.is_none() && f[at(27) - 1].troopers.is_empty());
    // Struck to 0 before frame 10's tick, the golem takes no step on it and dies where it was put back: its last point.
    assert_eq!(f[at(27)].troopers.iter().map(|t| t.0).collect::<Vec<_>>(), vec![n(GOLEM.0, GOLEM.1)], "on the point the golem died on, on schedule");
}

#[test]
fn the_nearer_troop_is_picked() {
    // A second golem 2000 nearer than the first, beside the line: the throw goes to it, and lands sooner.
    let near = (GOLEM.0 - 2000, GOLEM.1 - 1500);
    let f = scene(BALLOON, &[GOLEM, near], 45, None);
    let k = f.iter().position(|x| !x.troopers.is_empty()).expect("a landing");
    assert_eq!(Some(f[k].troopers[0].0), f[k].golems[1], "on the nearer golem");
    assert!(k < at(27), "sooner than the far one's P + 27: frame {k}");
}

#[test]
fn with_nothing_in_reach_the_trooper_drops_on_the_balloon_on_p_plus_5() {
    let f = scene(BALLOON, &[], 12, None);
    assert!(f[at(5) - 1].troopers.is_empty(), "nothing before P + 5");
    // The failsafe's throw appears on P + 4 where the Balloon's step of that tick left it, and lands there.
    let at4 = f[at(4)].balloon.expect("the Balloon flies");
    assert!(at4.dist(n(BALLOON.0, BALLOON.1)) < 100 * K, "one Balloon step from its point");
    assert_eq!(f[at(5)].troopers.iter().map(|t| t.0).collect::<Vec<_>>(), vec![at4], "on the Balloon's point");
    assert!(f[at(5)..].iter().all(|x| x.troopers.len() == 1), "one throw only");
}

#[test]
fn the_hero_balloon_drops_the_base_balloons_bomb() {
    // Its DeathSpawnCharacter, BalloonHero_Bomb, is a BalloonBomb row that changes display columns only: a red Knight held
    // under the Balloon takes the same blow on the same tick after either Balloon's death.
    let blow = |card: &str| -> Vec<i32> {
        let mut s = battle();
        s.spawn_unit_resolved(Team::Blue, card, n(BALLOON.0, BALLOON.1), None).expect("the Balloon");
        s.spawn_unit_resolved(Team::Red, "Knight", n(BALLOON.0, BALLOON.1 + 600), None).expect("a Knight");
        s.tick();
        let b = find_live(&s, Team::Blue, card)[0].id;
        let k = find_live(&s, Team::Red, "Knight")[0].id;
        for _ in 0..30 {
            assert!(s.debug_set_pos(b, n(BALLOON.0, BALLOON.1)));
            assert!(s.debug_set_pos(k, n(BALLOON.0, BALLOON.1 + 600)));
            s.tick();
        }
        assert!(s.debug_set_hp(b, 0));
        let mut hp = Vec::new();
        for _ in 0..90 {
            if s.entity(k).is_some() {
                assert!(s.debug_set_pos(k, n(BALLOON.0, BALLOON.1 + 600)));
            }
            s.tick();
            hp.push(s.entity(k).map_or(0, |e| e.hp));
        }
        hp
    };
    let (hero, base) = (blow("Balloon_hero"), blow("Balloon"));
    assert!(base.windows(2).any(|w| w[1] < w[0]), "the base Balloon's bomb lands on the Knight: {base:?}");
    assert_eq!(hero, base, "the hero's bomb is the base's, tick for tick");
}
