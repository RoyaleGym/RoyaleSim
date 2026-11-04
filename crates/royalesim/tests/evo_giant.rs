//! THE EVO GIANT (the 16.635 table; tools/extract_cards.py `rage_areas_block`; card.rs `RageAreasDef`, EVO_RAGE_AREAS;
//! state.rs `rage_area_arm`, `rage_area_pass`), on calibration cards.CARD_TABLE client160635007_20261103 at level 11.
//!
//! Read off the table, not measured (no 16.635 capture holds the form): the first area's tick, the point's sign and
//! rounding, the first application, the count's first hit, the clock under a freeze. Pinned here as the engine reads
//! the table:
//!   1. the 16.635 table loads the form, its rage as written, and no earlier table does;
//!   2. above half its hitpoints the giant puts down no area;
//!   3. the tick after the one its hitpoints reach half or below arms its clock: the first area comes on the tick after
//!      that, then every 30 ticks (29, then 30: the overshoot carried); an own Knight at the first area's edge is raged
//!      from one HitSpeed after the area is made, not before;
//!   4. each area is the one its hits since the rage began have reached (1, 2, 3); its hits before do not count;
//!   5. the area stands 2500 behind the giant along its facing, on both sides;
//!   6. a Freeze on the giant holds the clock: the next area is late by the frozen ticks;
//!   7. an area rages own units only and deals no damage;
//!   8. a Clone copies the giant as the base Giant;
//!   9. its hits are its row's own damage, before and after the rage;
//!  10. an area outlives the giant;
//!  11. a battle saved mid-rage restores to the same hashes.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_giant`):
//!   - evo_rage_never_armed -> 3, 4, 5, 6, 7, 10 and 11 red;
//!   - evo_rage_clock_late -> 3 red;
//!   - evo_rage_hits_uncounted -> 4 red;
//!   - evo_rage_largest_always -> 4 red;
//!   - evo_rage_offset_ahead -> 5 red;
//!   - evo_rage_clock_unheld -> 6 red;
//!   - evo_rage_area_at_once -> 3 red;
//!   - evo_rage_block_unread -> 1 red (the form is refused, so every scene of it is red too);
//!   - clone_copies_the_form -> 8 red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, EVO_RAGE_AREAS};
use royalesim::fixed::{isqrt, milli, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, CardTable};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn table() -> CardDb {
    CardDb::load_table(CardTable::Client160635007Nov03).expect("the 16.635 table")
}

fn battle() -> BattleState {
    let mut cfg = BattleConfig::with_cards(table());
    cfg.calib.card_table = CardTable::Client160635007Nov03;
    cfg.decks = [vec!["Giant".into(), "Knight".into(), "Clone".into()], vec!["Knight".into(), "Cannon".into(), "Freeze".into()]];
    cfg.forms = [vec![1, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// An Evo Giant of `team` put down at `at` (native), the ticks run until it stands deployed; its id.
fn giant(s: &mut BattleState, team: Team, at: (i32, i32)) -> EntityId {
    s.spawn_unit(team, "Giant_EV1", n(at.0, at.1), None).expect("the giant");
    for _ in 0..80 {
        assert!(find_live(s, team, "Giant_EV1").len() <= 1);
        if let Some(g) = find_live(s, team, "Giant_EV1").first().filter(|g| !g.deploying).map(|g| g.id) {
            return g;
        }
        if let Some(g) = find_live(s, team, "Giant_EV1").first().map(|g| g.id) {
            assert!(s.debug_set_pos(g, n(at.0, at.1)));
        }
        s.tick();
    }
    panic!("the giant never stood");
}

/// The rage areas standing now: (the index into EVO_RAGE_AREAS, the centre, the giant that made it, its life left).
fn areas(s: &BattleState) -> Vec<(usize, Vec2, EntityId, i32)> {
    s.spells()
        .iter()
        .filter_map(|sp| match &sp.motion {
            SpellMotion::Attached { parent, pos, part, life_ms, .. } => EVO_RAGE_AREAS.iter().position(|p| p == part).map(|k| (k, *pos, *parent, *life_ms)),
            _ => None,
        })
        .collect()
}

/// The areas made on the tick just run: their life is still whole (an area first updates on the tick after it is made).
fn made(s: &BattleState) -> Vec<(usize, Vec2, EntityId)> {
    areas(s).into_iter().filter(|a| a.3 == 1500).map(|(k, at, g, _)| (k, at, g)).collect()
}

/// Whether unit `id` carries the Rage buff (+30 % speed and hit speed).
fn raged(s: &BattleState, id: EntityId) -> bool {
    let e = s.entity(id).expect("the unit");
    e.buffs.iter().any(|b| b.id > 0 && s.cards().buffs[(b.id - 1) as usize].speed_pct == 130 && s.cards().buffs[(b.id - 1) as usize].hit_speed_pct == 130)
}

/// Whether unit `id` is held now (stunned, or frozen by a buff): the clock's PauseTag COMBAT_DISABLED.
fn frozen(s: &BattleState, id: EntityId) -> bool {
    let e = s.entity(id).expect("the unit");
    e.stun_ms > 0 || e.buffs.iter().any(|b| b.id > 0 && s.cards().buffs[(b.id - 1) as usize].speed_pct == -100)
}

/// The Cannon's own LifeTime drain, whole hitpoints a tick (lifetime.HP_DECAY linear_drain: 137 hundredths a tick at
/// level 11, so 1 or 2): a loss above it is a hit.
const DRAIN_MAX: i32 = 2;

/// The scene most tests share: an Evo Giant of `team` held on its own half against an enemy Cannon held 1500 ahead of
/// it (it stands and hits it, facing it), both topped up; the giant side's princess towers down, so no tower shot
/// reaches the Cannon (the king, 9000 from it, is out of its reach): (the battle, the giant, the Cannon).
fn against_a_cannon(team: Team) -> (BattleState, EntityId, EntityId) {
    let mut s = battle();
    let (g_at, c_at) = if team == Team::Blue { ((9000, 10500), (9000, 12000)) } else { ((9000, 21500), (9000, 20000)) };
    for k in [1, 2] {
        s.scenario_set_tower_hp(team, k, 0).expect("a princess tower down");
    }
    let g = giant(&mut s, team, g_at);
    let c = s.scenario_spawn_now(team.other(), "Cannon", n(c_at.0, c_at.1), None).expect("the Cannon");
    (s, g, c)
}

/// One tick of the shared scene: both held, the Cannon topped up and the giant's hitpoints set to `hp_pct` of its
/// maximum; the Cannon's loss on the tick (the giant's hit, if any, and its own drain).
fn step_loss(s: &mut BattleState, g: EntityId, c: EntityId, g_at: Vec2, hp_pct: i32) -> i32 {
    let top = s.entity(c).expect("the Cannon").max_hp;
    let g_max = s.entity(g).expect("the giant").max_hp;
    let c_at = s.entity(c).expect("the Cannon").pos;
    assert!(s.debug_set_pos(g, g_at) && s.debug_set_hp(g, g_max * hp_pct / 100));
    assert!(s.debug_set_pos(c, c_at) && s.debug_set_hp(c, top));
    s.tick();
    let loss = top - s.entity(c).expect("the Cannon held alive").hp;
    assert!(loss >= 0, "the Cannon gained {}", -loss);
    loss
}

/// `step_loss`: whether the giant hit the Cannon this tick (a loss above the drain).
fn step(s: &mut BattleState, g: EntityId, c: EntityId, g_at: Vec2, hp_pct: i32) -> bool {
    step_loss(s, g, c, g_at, hp_pct) > DRAIN_MAX
}

#[test]
fn the_16635_table_loads_the_evo_giant() {
    let db = table();
    let i = db.index("Giant_EV1").unwrap_or_else(|| panic!("refused: {:?}", db.rejected_evolutions));
    let ev = db.get(i).evo.as_ref().expect("the form");
    let r = ev.rage_areas.expect("its rage");
    assert_eq!((r.at_hp_pct, r.first_ms, r.every_ms, r.thresholds, r.offset), (50, 0, 1500, [1, 2, 3], milli(-2500)));
    assert_eq!(r.areas.map(|a| a.hit.radius), [2000, 2500, 3000, 3500].map(milli), "the four radii");
    for a in r.areas {
        assert_eq!((a.life_ms, a.hit_speed_ms, a.first_ms, a.follow, a.stay, a.end), (1500, 300, 300, false, true, None));
        assert!(a.hit.buff.is_some() && a.hit.damage == 0 && a.hit.only_own_troops && !a.hit.only_enemies, "{:?}", a.hit);
    }
    assert!(!db.rejected_evolutions.iter().any(|(n, _)| n == "Giant_EV1"), "{:?}", db.rejected_evolutions);
    assert_eq!(ev.cloned_as, db.index("Giant"), "a Clone copies it as the Giant");
    assert_eq!(ev.cycles, 1, "one Giant play before each evolved one");
    let card = db.get(i);
    assert_eq!((card.stop_movement_after_ms, card.wait_ms), (0, 0), "no stomp");
    for t in [CardTable::Client15535, CardTable::Client160402017Install, CardTable::Client160402017, CardTable::Client160402017Oct19] {
        let old = CardDb::load_table(t).expect("an earlier table");
        assert!(old.index("Giant_EV1").is_none(), "{t:?} carries no Evo Giant");
    }
}

#[test]
fn no_area_above_half() {
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    for k in 1..=200 {
        step(&mut s, g, c, at, 51);
        assert!(areas(&s).is_empty(), "an area at 51 % on tick {k}: {:?}", areas(&s));
    }
}

#[test]
fn the_first_area_on_c_plus_2_then_every_30() {
    // Its hitpoints set to half after tick C: armed in tick C + 1's Status, its clock runs from C + 2 (the first area
    // at once), then 1450 left, 50 a tick: C + 31, then C + 61 and C + 91. A blue Knight held on the first area's edge
    // (2000 + its radius from the centre, 2500 behind the giant) is raged from one HitSpeed (300 ms) after the area is
    // made: its first update (C + 3) is 50 old, so it applies on C + 8.
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    for _ in 0..40 {
        step(&mut s, g, c, at, 100);
    }
    assert!(made(&s).is_empty() && areas(&s).is_empty());
    let f = s.entity(g).expect("the giant").facing;
    let flen = isqrt(f.len2());
    let centre = Vec2::new(at.x + (f.x as i64 * milli(-2500) as i64 / flen) as i32, at.y + (f.y as i64 * milli(-2500) as i64 / flen) as i32);
    let kr = card_stat(&s, "Knight").collision_radius;
    let edge = Vec2::new(centre.x + milli(2000) + kr - K, centre.y);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", edge, None).expect("the Knight");
    let mut ticks = Vec::new();
    let mut raged_from = None;
    for k in 1..=100u32 {
        assert!(s.debug_set_pos(knight, edge));
        step(&mut s, g, c, at, 50);
        if !made(&s).is_empty() {
            ticks.push(k);
        }
        if raged_from.is_none() && raged(&s, knight) {
            raged_from = Some(k);
        }
    }
    assert_eq!(ticks, [2, 31, 61, 91], "the areas' ticks");
    assert_eq!(raged_from, Some(8), "the Knight on the first area's edge");
}

#[test]
fn hits_after_the_rage_grow_the_area() {
    // The giant hits the Cannon three times or more at full hitpoints, then is set to 40 %: each area made after is
    // the one its hits from the arming tick on (thresholds 1, 2, 3) have reached, its earlier hits not counted.
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    let mut before = 0;
    for _ in 0..200 {
        before += u32::from(step(&mut s, g, c, at, 100));
        if before >= 3 {
            break;
        }
    }
    assert!(before >= 3, "the scene drifted: the giant did not hit the Cannon");
    let (mut hits, mut parts, mut want) = (0u32, Vec::new(), Vec::new());
    for _ in 1..=200 {
        hits += u32::from(step(&mut s, g, c, at, 40));
        for (k, _, _) in made(&s) {
            parts.push(k);
            want.push([1, 2, 3].iter().filter(|t| hits >= **t).count());
        }
    }
    assert_eq!(parts, want, "each area by the hits since the rage began ({hits} in all)");
    assert_eq!((parts.first(), parts.last()), (Some(&0), Some(&3)), "the first area on no hit since, the last the largest: {parts:?}");
}

#[test]
fn the_area_stands_2500_behind_along_its_facing() {
    for team in [Team::Blue, Team::Red] {
        let (mut s, g, c) = against_a_cannon(team);
        let at = s.entity(g).expect("the giant").pos;
        let cannon = s.entity(c).expect("the Cannon").pos;
        for _ in 0..40 {
            step(&mut s, g, c, at, 100);
        }
        let mut first = None;
        for _ in 0..10 {
            step(&mut s, g, c, at, 50);
            if let Some(a) = made(&s).first() {
                first = Some((a.1, s.entity(g).expect("the giant").facing));
                break;
            }
        }
        let (centre, f) = first.expect("the first area");
        let flen = isqrt(f.len2());
        let want = Vec2::new(at.x + (f.x as i64 * milli(-2500) as i64 / flen) as i32, at.y + (f.y as i64 * milli(-2500) as i64 / flen) as i32);
        assert_eq!(centre, want, "{team:?}: 2500 along the facing {f:?}, backwards");
        let d = |p: Vec2| ((p.x - cannon.x) as i64).pow(2) + ((p.y - cannon.y) as i64).pow(2);
        assert!(d(centre) > d(at), "{team:?}: the area behind the giant, away from what it hits");
        let back = if team == Team::Blue { centre.y < at.y } else { centre.y > at.y };
        assert!(back, "{team:?}: toward its own side");
    }
}

#[test]
fn the_clock_holds_while_frozen() {
    // A red Freeze on the giant after its first area: the second, due 29 ticks on, comes as many ticks late as the giant
    // is held.
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    for _ in 0..40 {
        step(&mut s, g, c, at, 100);
    }
    let (mut ticks, mut held) = (Vec::new(), 0u32);
    for k in 1..=120u32 {
        if k == 6 {
            s.spawn_unit(Team::Red, "Freeze", at, None).expect("cast the Freeze");
        }
        step(&mut s, g, c, at, 50);
        if !made(&s).is_empty() {
            ticks.push(k);
        }
        if ticks.len() == 1 && frozen(&s, g) {
            held += 1;
        }
    }
    assert!(held > 0, "the scene drifted: the Freeze never held the giant");
    assert_eq!(&ticks[..2], [2, 31 + held], "the second area late by the {held} frozen ticks");
}

#[test]
fn own_side_only_and_no_damage() {
    // A blue Knight and a red Knight held inside the first area, out of each other's reach: the blue one is raged, the red
    // one never; the areas deal no damage and pulse none.
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    for _ in 0..40 {
        step(&mut s, g, c, at, 100);
    }
    let centre = Vec2::new(at.x, at.y - milli(2500));
    let (own_at, foe_at) = (Vec2::new(centre.x + milli(2000), centre.y + milli(500)), Vec2::new(centre.x, centre.y - milli(1500)));
    let own = s.scenario_spawn_now(Team::Blue, "Knight", own_at, None).expect("the blue Knight");
    let foe = s.scenario_spawn_now(Team::Red, "Knight", foe_at, None).expect("the red Knight");
    let (mut own_raged, mut foe_raged, mut seen) = (false, false, 0);
    for _ in 1..=60 {
        assert!(s.debug_set_pos(own, own_at) && s.debug_set_pos(foe, foe_at));
        step(&mut s, g, c, at, 50);
        own_raged |= raged(&s, own);
        foe_raged |= raged(&s, foe);
        for sp in s.spells().iter().filter(|sp| matches!(&sp.motion, SpellMotion::Attached { part, .. } if EVO_RAGE_AREAS.contains(part))) {
            assert_eq!((sp.damage, sp.pulse), (0, 0), "an area deals nothing");
            seen += 1;
        }
    }
    assert!(seen > 0, "the scene drifted: no area");
    assert!(own_raged && !foe_raged, "own side only: blue {own_raged}, red {foe_raged}");
}

#[test]
fn a_clone_is_a_plain_giant() {
    // The Clone cast on a giant whose rage runs: its copy is the base Giant (no rage of its own, the Giant's stomp), and
    // every area after it is the original's.
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    for _ in 0..40 {
        step(&mut s, g, c, at, 100);
    }
    for _ in 0..5 {
        step(&mut s, g, c, at, 50);
    }
    s.spawn_unit(Team::Blue, "Clone", at, None).expect("cast the Clone");
    let mut copy = None;
    for _ in 0..60 {
        step(&mut s, g, c, at, 50);
        if copy.is_none() {
            copy = s.entities().find(|v| v.team == Team::Blue && v.cloned).map(|v| (v.id, v.card.to_string()));
        }
        for (_, _, parent) in made(&s) {
            assert_eq!(parent, g, "an area of the original alone");
        }
    }
    let (_, card) = copy.expect("the scene drifted: the Clone made no copy");
    assert_eq!(card, "Giant", "the copy is the base Giant");
    let base = card_stat(&s, "Giant");
    assert!(base.evo.is_none() && base.stop_movement_after_ms > 0, "the Giant: no rage, a stomp");
}

#[test]
fn the_attack_entries_change_nothing() {
    // Every hit on the Cannon is the row's level-11 damage, at full hitpoints and in the rage.
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    let db = s.cards().clone();
    let i = db.index("Giant_EV1").expect("the form");
    let want = db.scaled(i, 11, db.get(i).damage).expect("level 11");
    let mut seen = [0, 0];
    for (half, pct) in [100, 40].into_iter().enumerate() {
        for _ in 0..150 {
            let loss = step_loss(&mut s, g, c, at, pct);
            if loss > DRAIN_MAX {
                assert!((1..=DRAIN_MAX).contains(&(loss - want)), "a hit at {pct} %: {loss}, the row's {want} and the drain");
                seen[half] += 1;
            }
        }
    }
    assert!(seen[0] >= 2 && seen[1] >= 2, "the scene drifted: hits {seen:?}");
}

#[test]
fn areas_outlive_the_giant() {
    // The giant dies on the tick after its first area: the area lives out its 1500 ms and no other comes.
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    for _ in 0..40 {
        step(&mut s, g, c, at, 100);
    }
    for _ in 0..2 {
        step(&mut s, g, c, at, 50);
    }
    assert_eq!(made(&s).len(), 1, "the first area");
    assert!(s.debug_set_hp(g, 0));
    let mut alive = Vec::new();
    for k in 1..=40u32 {
        s.tick();
        if k == 1 {
            assert!(!s.entity(g).is_some_and(|v| v.hp > 0), "the giant is dead");
        }
        assert!(made(&s).is_empty(), "no area after its death (tick {k})");
        if !areas(&s).is_empty() {
            alive.push(k);
        }
    }
    assert_eq!(alive, (1..=29).collect::<Vec<_>>(), "its area lives out its life");
}

#[test]
fn save_and_restore_mid_rage() {
    let (mut s, g, c) = against_a_cannon(Team::Blue);
    let at = s.entity(g).expect("the giant").pos;
    for _ in 0..40 {
        step(&mut s, g, c, at, 100);
    }
    for _ in 0..5 {
        step(&mut s, g, c, at, 50);
    }
    assert!(!areas(&s).is_empty(), "an area stands");
    let bytes = s.save();
    assert!(String::from_utf8_lossy(&bytes).contains("\"rage_areas\""), "the run is saved");
    let mut back = BattleState::load_with(&bytes, s.config().cards.clone(), s.config().arena.clone()).expect("a mid-rage snapshot loads");
    assert_eq!(back.state_hash(), s.state_hash());
    for k in 0..120 {
        s.tick();
        back.tick();
        assert_eq!(back.state_hash(), s.state_hash(), "tick {k} after the restore");
    }
    assert!(!areas(&s).is_empty() || find_live(&s, Team::Blue, "Giant_EV1").is_empty(), "the rage ran on");
    let plain = battle().save();
    assert!(!String::from_utf8_lossy(&plain).contains("rage_areas"), "a battle with no Evo Giant saves no rage");
}
