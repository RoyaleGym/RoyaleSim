//! THE EVO MUSKETEER'S SNIPE against client 15.535.29, level 11 (the oracle's runs sp-m6a, sp-m6b1, sp-m6b2 and sp-m6c).
//!
//! Each scene puts her and her enemies down where the recorded run had them once their deploys ended, the enemies as
//! many ticks after her as they were played after her. Ticks count from her deploy end: the first tick she is no
//! longer deploying. Distances are centre to centre, native units, rounded, taken on the tick a shot leaves.
//!
//! What the runs measured, one test each:
//!   - a Giant coming down from its bridge: the first snipe 15 ticks after her deploy end, then every 19; three snipes
//!     at 11487, 10486 and 9594; Speed 2650 and 391 damage each; she stepped between them; then plain shots (Speed
//!     1000, 217), the first 40 ticks after the third snipe, once the Giant was inside her ordinary reach (6568);
//!   - a Knight 12 tiles ahead and 1000 to her side: three snipes at +15, +34, +53, at 11986, 11106 and 9928;
//!   - a Knight 12 tiles ahead and 2000 to her side: no snipe target until its offset fell to SnipeSideClip plus its
//!     radius (1767 refused, 1727 taken), then three snipes from +45;
//!   - a Knight at 5 tiles and a Giant at 15: plain shots at the Knight until it died, then three snipes at the Giant;
//!   - Goblins at 9 tiles and a Knight at 13, with her own Fireball cast on the Goblins (sp-m6d): her second snipe
//!     took a goblin 1815 from the Fireball's centre while the Fireball was in flight (it landed 13 ticks later, and
//!     the recorded pending damage of the units it killed stayed 0): a spell in flight is not pending damage to
//!     IgnorePendingDamageTargets, only shots are.
//!
//! Not pinned here: the tick after an ordinary target dies (sp-m6c: she stands through it, TryToFinishAttackAnimation,
//! which the engine does not read; there she walks one step, so her three snipes at the Giant leave about 57 closer
//! than the recorded 10139, 9164 and 8228, and her first plain shot after them one tick early); and in sp-m6d the
//! Fireball put down 39 ticks after her lands on the 46th tick after her deploy end here, the 47th in the run, so her
//! third snipe, at the Knight, leaves at +61 here and at +62 there.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

/// A native point, in subtiles.
fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A native position, from subtiles.
fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

fn dist(a: (i32, i32), b: (i32, i32)) -> i32 {
    ((b.0 - a.0) as f64).hypot((b.1 - a.1) as f64).round() as i32
}

/// One shot of hers: the tick it left (from her deploy end), its speed (raw) and damage, its target's card, her
/// position and the target's.
#[derive(Clone, Debug)]
struct Shot {
    tick: i32,
    speed: i32,
    damage: i32,
    target: String,
    me: (i32, i32),
    at: (i32, i32),
}

impl Shot {
    fn dist(&self) -> i32 {
        dist(self.me, self.at)
    }
}

/// One tick of hers after her deploy end: her position, her target's card, and the first enemy troop's position.
#[derive(Clone, Debug)]
struct Frame {
    tick: i32,
    me: (i32, i32),
    target: Option<String>,
    foe: Option<(i32, i32)>,
}

/// Her shots and her frames, `ticks` ticks after she is put down at `me`; each foe `(card, point, delay)` is put down
/// `delay` ticks after her.
fn scene(me: (i32, i32), foes: &[(&str, (i32, i32), u32)], ticks: u32) -> (Vec<Shot>, Vec<Frame>) {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let form = s.cards().index("Musketeer_EV1").expect("the evolved Musketeer loads");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    s.spawn_unit(Team::Blue, "Musketeer_EV1", n(me.0, me.1), None).unwrap();
    let (mut shots, mut frames) = (Vec::new(), Vec::new());
    let mut deploy_end: Option<u32> = None;
    for k in 0..ticks {
        for (card, at, delay) in foes {
            if *delay == k {
                s.spawn_unit(Team::Red, card, n(at.0, at.1), None).unwrap();
            }
        }
        s.tick();
        let Some(m) = s.entities().find(|e| e.card_idx == form) else { continue };
        if m.deploying {
            continue;
        }
        let de = *deploy_end.get_or_insert(s.tick_count());
        let tick = (s.tick_count() - de) as i32;
        let target = m.target.and_then(|t| s.entity(t)).map(|t| t.card.to_string());
        let foe = s.entities().find(|e| e.team == Team::Red && !e.kind.is_crown_tower()).map(|e| native(e.pos));
        frames.push(Frame { tick, me: native(m.pos), target, foe });
        // Her hit tick: the shot she just launched is the newest of hers.
        if m.attack_phase == AttackPhase::Cooldown {
            let p = s.projectiles().iter().rev().find(|p| p.firer_card == Some(form)).expect("her shot");
            let t = s.entity(p.target).expect("a shot's target lives as it leaves");
            shots.push(Shot { tick, speed: p.speed / mult, damage: p.damage, target: t.card.to_string(), me: native(m.pos), at: native(t.pos) });
        }
    }
    (shots, frames)
}

/// sp-m6a: her play at (4500, 6000) lands at (5499, 5500) (the princess tower's footprint moves it); the Giant, played
/// on its bridge at (3500, 17500) on the same tick, stands at (3499, 17499).
fn scene_a() -> Vec<Shot> {
    scene((5499, 5500), &[("Giant", (3499, 17499), 0)], 260).0
}

fn snipes(shots: &[Shot]) -> Vec<&Shot> {
    shots.iter().filter(|s| s.speed == 2650).collect()
}

#[test]
fn a_the_first_snipe_leaves_15_ticks_after_her_deploy_end_then_every_19() {
    let shots = scene_a();
    let ticks: Vec<i32> = snipes(&shots).iter().map(|s| s.tick).collect();
    assert_eq!(ticks, [15, 34, 53], "{shots:?}");
}

#[test]
fn a_three_snipes_of_391_at_speed_2650_at_the_measured_distances() {
    let shots = scene_a();
    let three: Vec<(i32, i32, &str, i32)> = shots[..3].iter().map(|s| (s.speed, s.damage, s.target.as_str(), s.dist())).collect();
    assert_eq!(three, [(2650, 391, "Giant", 11487), (2650, 391, "Giant", 10486), (2650, 391, "Giant", 9594)], "{shots:?}");
    assert_eq!(snipes(&shots).len(), 3, "AmmoCount 3: {shots:?}");
}

#[test]
fn a_she_steps_between_snipes() {
    // Her position at each snipe, as recorded: a step on each tick she took the Giant (the first after her deploy, the
    // others two ticks after a snipe), and steps toward the tower after the first snipe, while the Giant stood 2031
    // and 2006 to her side (past SnipeSideClip 1250 plus its radius 750).
    let shots = scene_a();
    let at: Vec<(i32, i32)> = snipes(&shots).iter().map(|s| s.me).collect();
    assert_eq!(at, [(5457, 5542), (5357, 5688), (5314, 5728)], "{shots:?}");
}

#[test]
fn a_plain_shots_start_40_ticks_after_the_third_snipe_inside_her_ordinary_reach() {
    let shots = scene_a();
    let plain: Vec<&Shot> = shots.iter().filter(|s| s.speed != 2650).collect();
    assert!(plain.len() >= 2, "{shots:?}");
    assert_eq!((plain[0].tick - shots[2].tick, plain[0].speed, plain[0].damage, plain[0].dist()), (40, 1000, 217, 6568), "{shots:?}");
    // Every 20 ticks after it (HitSpeed 1000), 217 each.
    let gaps: Vec<i32> = plain.windows(2).map(|w| w[1].tick - w[0].tick).collect();
    assert!(gaps.iter().all(|g| *g == 20) && plain.iter().all(|s| s.damage == 217), "{plain:?}");
}

#[test]
fn b1_a_knight_1000_to_her_side_is_sniped_three_times() {
    // sp-m6b1: she stands at (3499, 9500); the Knight, played 20 ticks after her at (4500, 21500), stands at
    // (4499, 21499), still deploying when she takes it.
    let shots = scene((3499, 9500), &[("Knight", (4499, 21499), 20)], 160).0;
    let three: Vec<(i32, i32, i32)> = snipes(&shots).iter().map(|s| (s.tick, s.damage, s.dist())).collect();
    assert_eq!(three, [(15, 391, 11986), (34, 391, 11106), (53, 391, 9928)], "{shots:?}");
}

#[test]
fn b2_a_knight_2000_to_her_side_is_taken_once_inside_the_side_clip_plus_its_radius() {
    // sp-m6b2: the Knight at (5499, 21499), 2000 to her side. She walks; it walks in; she takes it on the tick its
    // offset fell from 1767 to 1727 (SnipeSideClip 1250 + radius 500 = 1750; LockedTargetSnipeSideClip 1500 + 500
    // would have taken it at 1962, six ticks earlier). Then three snipes, the first at +45.
    let (shots, frames) = scene((3499, 9500), &[("Knight", (5499, 21499), 20)], 160);
    let taken = frames.iter().position(|f| f.target.as_deref() == Some("Knight")).expect("she takes the Knight");
    let side = |f: &Frame| f.foe.expect("the Knight stands").0 - f.me.0;
    assert_eq!((frames[taken].tick, side(&frames[taken - 1]), side(&frames[taken])), (31, 1767, 1727), "{frames:?}");
    let ticks: Vec<i32> = snipes(&shots).iter().map(|s| s.tick).collect();
    assert_eq!(ticks, [45, 64, 83], "{shots:?}");
    assert_eq!(snipes(&shots)[0].dist(), 9092, "{shots:?}");
}

#[test]
fn c_the_knight_in_reach_first_then_three_snipes_at_the_giant() {
    // sp-m6c: she stands at (3499, 12500); a Knight at (3499, 17499), 5 tiles ahead, and a Giant at (3499, 27499),
    // 15 ahead, both played 20 ticks after her. Plain shots at the Knight, from +14 every 20, until it died; then her
    // three snipes, kept from her deploy, at the Giant at +153, +172, +191.
    let shots = scene((3499, 12500), &[("Knight", (3499, 17499), 20), ("Giant", (3499, 27499), 20)], 240).0;
    let seen: Vec<(i32, i32, &str)> = shots.iter().take(10).map(|s| (s.tick, s.damage, s.target.as_str())).collect();
    let knight: Vec<(i32, i32, &str)> = (0..7).map(|k| (14 + 20 * k, 217, "Knight")).collect();
    let giant = [(153, 391, "Giant"), (172, 391, "Giant"), (191, 391, "Giant")];
    assert_eq!(seen, [knight.as_slice(), giant.as_slice()].concat(), "{shots:?}");
}

#[test]
fn d_a_spell_in_flight_is_not_pending_damage() {
    // sp-m6d: she stands at (3499, 9500); Goblins played at (3500, 18500) and a Knight at (3500, 22500) 20 ticks after
    // her; her side's Fireball cast at (3753, 18245) 39 ticks after her. Snipes at +15 and +34 at goblins, the second
    // at the goblin then at (3015, 16587), inside the Fireball's area while it flies; then the Knight, 1078 hp after
    // the Fireball's 688.
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let form = s.cards().index("Musketeer_EV1").expect("the evolved Musketeer loads");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    s.spawn_unit(Team::Blue, "Musketeer_EV1", n(3499, 9500), None).unwrap();
    let mut deploy_end: Option<u32> = None;
    // At each snipe: its tick, its target's card and position, the Knight's hp, the goblins alive.
    #[derive(Debug)]
    struct Snipe(i32, String, (i32, i32), i32, usize);
    let mut snipes: Vec<Snipe> = Vec::new();
    for k in 0..110u32 {
        if k == 20 {
            s.spawn_unit(Team::Red, "Goblins", n(3500, 18500), None).unwrap();
            s.spawn_unit(Team::Red, "Knight", n(3500, 22500), None).unwrap();
        }
        if k == 39 {
            s.spawn_unit(Team::Blue, "Fireball", n(3753, 18245), None).unwrap();
        }
        s.tick();
        let Some(m) = s.entities().find(|e| e.card_idx == form) else { continue };
        if m.deploying {
            continue;
        }
        let de = *deploy_end.get_or_insert(s.tick_count());
        let tick = (s.tick_count() - de) as i32;
        if m.attack_phase == AttackPhase::Cooldown {
            let p = s.projectiles().iter().rev().find(|p| p.firer_card == Some(form)).expect("her shot");
            assert_eq!(p.speed / mult, 2650, "a plain shot at +{tick}");
            let t = s.entity(p.target).expect("a shot's target lives as it leaves");
            let knight = s.entities().find(|e| e.team == Team::Red && e.card == "Knight").map_or(0, |e| e.hp);
            let goblins = s.entities().filter(|e| e.team == Team::Red && e.card == "Goblins").count();
            snipes.push(Snipe(tick, t.card.to_string(), native(t.pos), knight, goblins));
        }
    }
    assert_eq!(snipes.len(), 3, "{snipes:?}");
    assert_eq!((snipes[0].0, snipes[0].1.as_str()), (15, "Goblins"), "{snipes:?}");
    assert_eq!((snipes[1].0, snipes[1].1.as_str(), snipes[1].2), (34, "Goblins", (3015, 16587)), "{snipes:?}");
    // The Fireball had not landed at the second snipe (three goblins alive: her target and the two it killed), and had
    // by the third (no goblin left, 688 off the Knight).
    assert_eq!((snipes[1].4, snipes[2].4, snipes[2].1.as_str(), snipes[2].3), (3, 0, "Knight", 1078), "{snipes:?}");
}
