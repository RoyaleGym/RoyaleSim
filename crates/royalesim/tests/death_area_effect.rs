//! THE AREA EFFECT A DEATH LEAVES ON THE GROUND -- card.rs `death_area_effect` /
//! `convert_area_effect`, state.rs `phase_reap`, spell.rs `cast` / `step_spells`.
//!
//! WHAT THE CARD DATA SAYS (read here, never pasted; every number below comes off
//! the loaded CardDb or data/derived/cards.json):
//!   * `DeathAreaEffect` on a character or building row is the NAME of a row in
//!     cards.json's `area_effect_objects` table. The Ice Golem's is
//!     `FreezeIceGolemite`: a 2000-millitile disc, LifeDuration 1000 ms, no Damage
//!     column at all, HitSpeed blank (so a ONE-SHOT area, the shape a Zap has), and
//!     the buff `IceWizardSlowDown` -- -30 on SpeedMultiplier, HitSpeedMultiplier and
//!     SpawnSpeedMultiplier -- for BuffTime 2000 ms, enemies only, air and ground.
//!     It is a SLOW, not a hold: -30 composes to 70 % of the walk, where the Freeze
//!     row's -100 composes to 0.
//!   * The Ice Golem ALSO ships `DeathDamage` 33 over `DeathDamageRadius` 2000. The
//!     two are separate effects of one death, and the data is what settles it: the
//!     area's Damage column is blank, so reading the 33 as "the area's damage" would
//!     leave the area with nothing to deal; and the Super Ice Golem ships the two
//!     blocks with DIFFERENT radii, different damage and different crown-tower
//!     percents, which no single-effect reading can hold. The engine fires both.
//!
//! WHAT IS PINNED
//!   1. the loader turns `DeathAreaEffect` into the card's own `SpellDef`, with the
//!      radius, the buff row and the BuffTime cards.json carries -- and REFUSES the
//!      card, by name and with the area's reason, when the area's mechanic is one it
//!      does not read (the Rage Barbarian's and the Suspicious Bush's are spawn
//!      scripts) or when the named row is not in the file at all;
//!   2. the death releases the area as the engine's OWN area-effect object: after the
//!      death tick `spells()` holds one `Area` motion at the death point under the
//!      dying card's index, and it is consumed on its first update, exactly as a Zap
//!      is;
//!   3. an enemy inside the disc takes the death damage AND carries the area's buff
//!      for its BuffTime, and its walk drops to the composition of the buff;
//!   4. an enemy outside both discs takes neither;
//!   5. the buff runs out after ceil(BuffTime / TICK_MS) ticks and the walk comes
//!      back;
//!   6. the two effects land on the SAME tick -- the tick after the death, because
//!      Reap writes the damage into the buffer Resolve drains next tick and puts the
//!      area into the list the Projectile phase steps next tick;
//!   7. seat symmetry: the whole scene rotated 180 degrees gives the mirrored
//!      outcome, number for number.
//!
//! HOW TO PLANT A DEFECT AND SEE THESE FAIL (nothing here has a cfg plant of its
//! own, so a reviewer plants by hand):
//!   * drop the `self.spells.append(&mut released)` line in state.rs `phase_reap`:
//!     (2), (3), (5) and (7) go red, (1) and (4) stay green;
//!   * make `shape_of` in spell.rs read `def.spell` only: the same set goes red,
//!     because the released object is then discarded on its first step;
//!   * give the missing-record arm of `CardDb::from_json_str` a silent `None`
//!     instead of pushing to `unloadable`: the last case of (1) goes red;
//!   * move the release above the death-damage loop's `combat::splash` and into the
//!     PREVIOUS phase: (6)'s "nothing has landed yet" assertions go red.

mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellHit, SpellShape};
use royalesim::fixed::{milli, Vec2, SUBTILE_PER_MILLITILE};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, Calib};
use royalesim::status::{compose, BuffDef, Sel};
use royalesim::{EntityId, Team};

fn calib() -> Calib {
    Calib::shipped()
}

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(11, cfg)
}

/// ceil(a / b) for positive b.
fn ceil_div(a: i32, b: i32) -> i32 {
    (a + b - 1) / b
}

/// One tile, in subtiles.
fn tile() -> i32 {
    SUBTILE_PER_MILLITILE * 1000
}

/// data/derived/cards.json as text, for the tests that doctor it.
fn cards_json() -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json");
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The death area effect of `card` on the loaded CardDb: its one-shot hit, its buff
/// row and the BuffTime that rides it.
fn death_area(s: &BattleState, card: &str) -> (SpellHit, BuffDef, i32) {
    let def = card_stat(s, card)
        .death_area_effect
        .clone()
        .unwrap_or_else(|| panic!("cards.json {card} carries no death area effect"));
    let hit = match &def.shape {
        SpellShape::AreaEffect { hit } => *hit,
        other => panic!("{card}: the death area is {other:?}, not a one-shot disc"),
    };
    let b = hit.buff.unwrap_or_else(|| panic!("cards.json {card}'s death area carries no buff"));
    (hit, s.cards().buffs[b.buff as usize], b.time_ms)
}

// ---------------------------------------------------------------------------
// (1): the loader

#[test]
fn the_loader_reads_the_named_area_effect_row_onto_the_card_and_refuses_the_rest_by_name() {
    let db = cards();
    let doc: serde_json::Value = serde_json::from_str(&cards_json()).expect("cards.json parses");
    let aeos = &doc["area_effect_objects"];
    let rows = doc["cards"].as_array().expect("cards.json has a cards array");
    let int = |v: &serde_json::Value| v.as_i64().unwrap_or_else(|| panic!("{v} is not a number")) as i32;

    let (mut named, mut loaded, mut refused) = (0, 0, 0);
    for row in rows {
        let Some(area) = row["death_area_effect"].as_str() else { continue };
        named += 1;
        let name = row["name"].as_str().expect("a card row has a name");
        let Some(idx) = db.index(name) else {
            // REFUSED: out loud, naming the area it could not read.
            refused += 1;
            let (_, why) = db.rejected.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} neither loaded nor rejected"));
            assert!(why.contains(area), "{name} was refused without naming its area {area}: {why}");
            continue;
        };
        // LOADED: the block is the file's row, field for field.
        loaded += 1;
        let a = &aeos[area];
        assert!(!a.is_null(), "{name}: loaded against an area_effect_objects row the file does not carry");
        let def = db.get(idx).death_area_effect.clone().unwrap_or_else(|| panic!("{name} loaded with no death area effect block"));
        let hit = match &def.shape {
            SpellShape::AreaEffect { hit } => {
                assert!(a["hit_speed_ms"].is_null(), "{name}: {area} ships a HitSpeed and loaded as a one-shot disc");
                hit
            }
            SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms } => {
                assert_eq!(*life_ms, int(&a["life_duration_ms"]), "{name}: the area's LifeDuration");
                assert_eq!(*hit_speed_ms, int(&a["hit_speed_ms"]), "{name}: the area's HitSpeed");
                hit
            }
            other => panic!("{name}: a death area loaded as {other:?}"),
        };
        assert_eq!(hit.radius, milli(int(&a["radius_milli"])), "{name}: the area's Radius");
        assert_eq!(hit.damage, a["damage"].as_i64().unwrap_or(0) as i32, "{name}: the area's Damage");
        assert_eq!(hit.crown_pct, int(&a["crown_tower_damage_percent"]), "{name}: the area's CrownTowerDamagePercent");
        assert_eq!(hit.only_enemies, a["only_enemies"].as_bool().unwrap_or(false), "{name}: OnlyEnemies");
        assert_eq!(hit.hits_air, a["hits_air"].as_bool().unwrap_or(false), "{name}: HitsAir");
        assert_eq!(hit.hits_ground, a["hits_ground"].as_bool().unwrap_or(false), "{name}: HitsGround");
        match (a["buff"].is_null(), hit.buff) {
            (true, None) => {}
            (false, Some(b)) => {
                assert_eq!(b.time_ms, int(&a["buff_time_ms"]), "{name}: the area's BuffTime");
                let carried = db.buffs[b.buff as usize];
                assert_eq!(carried.speed_pct, int(&a["buff"]["speed_multiplier_raw"]), "{name}: the buff's SpeedMultiplier");
                assert_eq!(carried.hit_speed_pct, int(&a["buff"]["hit_speed_multiplier_raw"]), "{name}: the buff's HitSpeedMultiplier");
            }
            (file_blank, got) => panic!("{name}: cards.json buff blank = {file_blank}, the card carries {got:?}"),
        }
    }
    assert!(named >= 2, "only {named} cards in cards.json carry a DeathAreaEffect; the scan proves nothing");
    assert!(loaded >= 1, "not one DeathAreaEffect card loaded (of {named})");
    assert!(refused >= 1, "every DeathAreaEffect card loaded, so the refusal arm is untested");

    // The Ice Golem is the card this file works on, and it loads.
    let ice = db.index("IceGolemite").unwrap_or_else(|| panic!("IceGolemite: {:?}", db.rejected.iter().find(|(n, _)| n == "IceGolemite")));
    assert!(db.get(ice).death_area_effect.is_some());
    // An area whose mechanic is a spawn script stays refused, card and all.
    for card in ["RageBarbarian", "SuspiciousBush"] {
        assert!(db.index(card).is_none(), "{card} must not be simulable: its area is a spawn script");
    }
}

#[test]
fn a_cards_json_whose_area_effect_table_is_missing_the_named_row_refuses_the_card() {
    // The positive control first: the file as it ships loads the card WITH the block.
    let text = cards_json();
    let good = CardDb::from_json_str(&text, CardSource::DerivedJson).expect("cards.json loads");
    let idx = good.index("IceGolemite").expect("the Ice Golem loads from the shipped file");
    assert!(good.get(idx).death_area_effect.is_some());

    // The same file with the named row taken out of the table. cards.json is
    // generated: a regenerated one that dropped the table, or renamed a row, must not
    // quietly produce a card whose death does nothing.
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    let named = doc["cards"]
        .as_array()
        .expect("a cards array")
        .iter()
        .find(|c| c["name"] == "IceGolemite")
        .and_then(|c| c["death_area_effect"].as_str())
        .expect("the Ice Golem names a death area effect")
        .to_string();
    doc["area_effect_objects"].as_object_mut().expect("the table is an object").remove(&named);
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the doctored file still parses");
    assert!(db.index("IceGolemite").is_none(), "a missing {named} row loaded the Ice Golem anyway");
    let (_, why) = db.rejected.iter().find(|(n, _)| n == "IceGolemite").expect("the Ice Golem is not even listed as rejected");
    assert!(why.contains(&named), "the refusal does not name the missing row: {why}");
}

// ---------------------------------------------------------------------------
// the scene
//
// THE ICE GOLEM IS THE ATTACKER'S AND THE VICTIMS ARE THE DEFENDER'S, ON THE
// VICTIMS' OWN HALF. No crown tower can then shoot a victim -- the only tower within
// reach belongs to the victims' own side, and the only enemy in its reach is the Ice
// Golem, which these tests kill themselves -- so an hp difference inside the window
// is the death's and nothing else's.

/// A spot on Blue's half, clear of the river and of every tower footprint.
fn blue_scene() -> Vec2 {
    t(900, 1000)
}

struct Scene {
    golem: EntityId,
    inside: EntityId,
    outside: EntityId,
    death_at: Vec2,
}

/// `attacker`'s Ice Golem and two of the other side's Knights: one a half-radius
/// from the golem, one clear of BOTH discs by a tile. Both Knights are deploying,
/// so they stand where they were put.
fn scene(s: &mut BattleState, attacker: Team) -> Scene {
    let victim = attacker.other();
    let (hit, _, _) = death_area(s, "IceGolemite");
    let ig = card_stat(s, "IceGolemite").clone();
    let knight_radius = card_stat(s, "Knight").collision_radius;
    assert!(ig.death_damage > 0 && ig.death_damage_radius > 0, "data: the Ice Golem carries a death damage disc too");
    let reach = hit.radius.max(ig.death_damage_radius);

    // Laid out in Blue's frame, then rotated when the victims are Red: the scene
    // always stands on the victims' half.
    //
    // THE NEAR KNIGHT STANDS CLEAR OF THE GOLEM'S BODY, in the middle of the band
    // where a Knight is wholly inside the disc and wholly outside the golem. Touching
    // it would hand the scene a contact push every tick, and the geometry under test
    // would then be the separation law's.
    let at0 = blue_scene();
    let (lo, hi) = (ig.collision_radius + knight_radius, hit.radius - knight_radius);
    assert!(lo < hi, "data: the area's disc is too narrow to stand a Knight inside it and clear of the golem");
    let near0 = Vec2::new(at0.x + (lo + hi) / 2, at0.y);
    let far0 = Vec2::new(at0.x + reach + knight_radius + tile(), at0.y);
    let (at, near_at, far_at) = match victim {
        Team::Blue => (at0, near0, far0),
        Team::Red => (mirror(s, at0), mirror(s, near0), mirror(s, far0)),
    };
    for p in [at, near_at, far_at] {
        assert!(s.arena().is_passable_ground(p), "scene: {p:?} is not dry ground");
    }

    let golem = s.scenario_spawn_now(attacker, "IceGolemite", at, None).expect("the Ice Golem stands up");
    s.spawn_unit(victim, "Knight", near_at, None).expect("the near Knight is placed");
    s.spawn_unit(victim, "Knight", far_at, None).expect("the far Knight is placed");
    s.tick();
    let death_at = s.entity(golem).expect("the golem is alive").pos;
    let (inside, outside) = {
        let mut knights = find_live(s, victim, "Knight");
        assert_eq!(knights.len(), 2, "scene: two Knights");
        knights.sort_by_key(|k| k.pos.dist2(death_at));
        (knights[0].id, knights[1].id)
    };
    let d_in = s.entity(inside).expect("the near Knight").pos.dist(death_at);
    let d_out = s.entity(outside).expect("the far Knight").pos.dist(death_at);
    assert!(d_in + knight_radius < hit.radius, "scene: the near Knight at {d_in} is not inside the {} disc", hit.radius);
    assert!(d_in > ig.collision_radius + knight_radius, "scene: the near Knight at {d_in} is touching the golem");
    assert!(d_out > reach + knight_radius, "scene: the far Knight at {d_out} is not clear of the {reach} discs");
    Scene { golem, inside, outside, death_at }
}

/// One victim's hp, its unbuffed walk, its walk now, and the one buff slot it
/// carries (row id + 1, ms left).
fn probe(s: &BattleState, id: EntityId) -> (i32, i32, i32, Option<(u16, i32)>) {
    let v = s.entity(id).unwrap_or_else(|| panic!("{id:?} is gone"));
    let slot = v.buffs.iter().find(|b| !b.is_empty()).map(|b| (b.id, b.ms));
    (v.hp, v.speed, v.speed_now, slot)
}

// ---------------------------------------------------------------------------
// (2), (3), (4), (6): the release, the two effects, and who is out of reach

#[test]
fn an_ice_golems_death_slows_the_enemy_inside_its_area_damages_it_with_its_disc_and_leaves_the_one_outside_alone() {
    let mut s = bare(config());
    let (hit, buff, buff_ms) = death_area(&s, "IceGolemite");
    assert_eq!(hit.damage, 0, "data: the Ice Golem's area carries no Damage of its own");
    assert!(hit.only_enemies, "data: the area is enemies only");
    let left = compose([buff].iter(), Sel::Speed, 100);
    assert!(left > 0 && left < 100, "data: the area's buff is a SLOW ({left} % of the walk), not a hold and not nothing");

    let sc = scene(&mut s, Team::Red);
    let ice_idx = s.cards().index("IceGolemite").expect("the Ice Golem is simulable");
    let level = s.config().card_level[Team::Red as usize];
    let dd = s.cards().scaled(ice_idx, level, card_stat(&s, "IceGolemite").death_damage).expect("the death damage scales");
    assert!(dd > 0);

    let (hp_in, speed_in, walk_in, slot_in) = probe(&s, sc.inside);
    let (hp_out, speed_out, walk_out, slot_out) = probe(&s, sc.outside);
    assert_eq!((slot_in, slot_out), (None, None), "scene: a Knight was already buffed");
    assert_eq!((walk_in, walk_out), (speed_in, speed_out), "scene: a Knight was already slowed");
    assert!(s.spells().is_empty(), "scene: a spell object was already on the board");

    // THE DEATH TICK. Resolve queues the death; Reap releases the area into the spell
    // list and buffers the death damage. Neither has landed yet.
    assert!(s.debug_set_hp(sc.golem, 0));
    s.tick();
    let death_tick = s.tick_count();
    assert!(s.entity(sc.golem).is_none(), "the Ice Golem is still alive after tick {death_tick}");
    assert_eq!(s.spells().len(), 1, "post-tick {death_tick}: the death left exactly one area object");
    {
        let spell = &s.spells()[0];
        assert_eq!(spell.card, ice_idx, "the object runs under the DYING card's index, not a spell card's");
        assert_eq!(spell.team, Team::Red, "the area belongs to the side whose unit died");
        assert_eq!(spell.damage, 0, "the area deals its own damage, not the death disc's");
        match &spell.motion {
            SpellMotion::Area { pos } => {
                let drift = pos.dist(sc.death_at);
                assert!(drift <= tile() / 4, "the area stands {drift} subtiles off the death point");
            }
            other => panic!("the death left {other:?}, not a one-shot area"),
        }
    }
    assert_eq!(probe(&s, sc.inside).0, hp_in, "post-tick {death_tick}: the death damage has not resolved yet");
    assert_eq!(probe(&s, sc.inside).3, None, "post-tick {death_tick}: the area has not applied yet");

    // THE TICK AFTER. The area applies in the Projectile phase and the death damage
    // resolves in the same Resolve: one death, two effects, one tick.
    s.tick();
    assert!(s.spells().is_empty(), "post-tick {}: a one-shot area is consumed on its first update", death_tick + 1);
    let (hp, speed, walk, slot) = probe(&s, sc.inside);
    assert_eq!(hp, hp_in - dd, "the death damage disc did not land on the near Knight");
    let (slot_id, ms) = slot.expect("the near Knight carries no buff: the area never reached it");
    let want = hit.buff.expect("the area's buff").buff + 1;
    assert_eq!((slot_id, ms), (want, buff_ms), "the slot is not the area's own buff row for its own BuffTime");
    let spt = calib().speed_to_subtiles_per_tick;
    assert_eq!(walk, compose([buff].iter(), Sel::Speed, speed / spt) * spt, "the slowed walk is the composition of the free one");
    assert!(walk < speed, "the near Knight walks at {walk}, unslowed it walks at {speed}");

    // (4) The one outside both discs: no damage, no buff, the walk it had.
    let (hp2, speed2, walk2, slot2) = probe(&s, sc.outside);
    assert_eq!(hp2, hp_out, "the far Knight took damage");
    assert_eq!(slot2, None, "the far Knight was buffed through its clearance of the {} disc", hit.radius);
    assert_eq!(walk2, speed2, "the far Knight's walk moved");
}

// ---------------------------------------------------------------------------
// (5): the buff runs out

#[test]
fn the_slow_runs_out_after_its_bufftime_and_the_walk_comes_back() {
    let mut s = bare(config());
    let (_, _, buff_ms) = death_area(&s, "IceGolemite");
    let sc = scene(&mut s, Team::Red);
    assert!(s.debug_set_hp(sc.golem, 0));
    s.tick();
    s.tick();
    let (_, speed, slowed, slot) = probe(&s, sc.inside);
    assert_eq!(slot.map(|(_, ms)| ms), Some(buff_ms), "the slow did not land");
    assert!(slowed < speed);

    // status.BUFF_EXPIRY_TICK_ALIGNMENT: the buff's own timer runs on the stun's
    // alignment -- ceil(BuffTime / TICK_MS) ticks from the one it landed on.
    let want_ticks = ceil_div(buff_ms, calib().tick_ms);
    for k in 0..want_ticks {
        let (_, _, now, carried) = probe(&s, sc.inside);
        assert!(carried.is_some(), "tick {k} of the slow: the buff is already gone");
        assert_eq!(now, slowed, "tick {k} of the slow: the walk moved");
        s.tick();
    }
    let (_, speed2, back, gone) = probe(&s, sc.inside);
    assert_eq!(gone, None, "the slot outlived ceil(BuffTime / TICK_MS) = {want_ticks} ticks");
    assert_eq!(back, speed2, "the walk did not come back");
}

// ---------------------------------------------------------------------------
// (7): seat symmetry

#[test]
fn both_seats_release_the_same_area_and_it_does_the_same_thing() {
    // The same scene rotated 180 degrees: a Red Ice Golem on Blue's half against
    // Blue Knights, and a Blue one on Red's half against Red Knights. Every
    // observable a victim carries must agree, number for number. Distinct
    // quantities, so a swap between the two victims cannot pass: the inside one
    // loses hp and is slowed, the outside one loses nothing and is not.
    let run = |attacker: Team| {
        let mut s = bare(symmetric_config());
        let sc = scene(&mut s, attacker);
        let (hp_in, _, _, _) = probe(&s, sc.inside);
        let (hp_out, _, _, _) = probe(&s, sc.outside);
        assert!(s.debug_set_hp(sc.golem, 0));
        s.tick();
        let released = (s.spells().len(), s.spells().first().map(|x| (x.team == attacker, x.card, x.damage)));
        s.tick();
        let (hp, speed, walk, slot) = probe(&s, sc.inside);
        let (hp2, speed2, walk2, slot2) = probe(&s, sc.outside);
        (released, hp_in - hp, speed, walk, slot, hp_out - hp2, speed2, walk2, slot2)
    };
    let red = run(Team::Red);
    let blue = run(Team::Blue);
    assert!(red.1 > 0 && red.3 < red.2, "the Red-seat run did nothing to measure");
    assert_eq!(red.5, 0, "the Red-seat outside Knight was hit");
    assert_eq!(red, blue, "the two seats disagree about the Ice Golem's death area");
}
