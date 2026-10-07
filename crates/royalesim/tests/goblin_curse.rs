//! THE GOBLIN CURSE: an area whose action makes another area (card.rs `area_spawns_area`, collapsed to a zero
//! `SpellShape::Fuse`), a circle whose hit hangs two buffs (card.rs `on_hit_buffs`, `SpellHit::buff2`), a buff whose
//! carrier leaves a unit when it dies (status.rs `BuffDeathSpawn`; state.rs `phase_reap`), and a crown-tower pulse
//! (status.rs `BuffDef::crown_hit`).
//!
//! THE LAW, measured on client 15.535.29 (20 runs), C the cast tick (k = 0 below, the first tick run after the play):
//!   - the parent area's own Damage never lands (spells.AREA_DAMAGE_WITHOUT_HIT_FLAGS = inert);
//!   - the curse circle applies on C + 1 to C + 120 (spells.AREA_SPAWNED_AREA_START = on_parent_first_update), each
//!     application hanging GoblinCurseDamage (14 a second, -15 % speed) and the mark GoblinCurse for 100 ms; a unit
//!     inside on tick t is slowed on its steps t + 1 and t + 2, the last slowed step C + 122;
//!   - the damage pulses every 1000 ms from the first application: C + 21 to C + 121, 35 at level 11, 39 at 12; a crown
//!     tower takes 10 a pulse instead (status.CROWN_TOWER_DAMAGE_PER_HIT_SCALING = level_scaled);
//!   - a unit that dies with the mark leaves a GoblinCurseGoblin for the caster on its own point, deploying its 20 ticks,
//!     targetable from the tick after it appears (status.BUFF_DEATH_SPAWN_*);
//!   - the mark never lands on a building (IgnoreBuildings) nor on a unit whose row lists it (the Battle Ram's
//!     IgnoreBuff), which still takes the damage and the slow.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the card as a zero fuse over its curse circle, and refuses the shapes it does not read;
//!   2. a Knight at the centre loses only the circle's pulses;
//!   3. the circle applies on C + 1 to C + 120: a walking Knight's first slowed step is C + 2, a standing Giant's last
//!      is C + 122;
//!   4. a standing Giant takes six pulses of 35 on C + 21 to C + 121 (39 under a level-12 cast);
//!   5. a crown tower takes six pulses of 10;
//!   6. three cursed Skeletons killed before any pulse become three goblins for the caster, on their points, on the
//!      death tick, at the curse's level, deploying 20 ticks, targetable by a Red tower from the next tick;
//!   7. the Battle Ram takes the damage and the slow and never the mark: its own Barbarians, no goblin;
//!   8. a building takes the damage and never the mark: no goblin;
//!   9. a slot's source level is state: a save edited only in it fails the load's hash self-check;
//!   10. KNOWN DIVERGENCE: two curses on one unit. Measured on client 15.535.29, they pulse on their own clocks and at
//!       their own levels (35 and 39 a pulse); the engine keeps one slot per buff row (status.BUFF_STACKING, whose entry
//!       records the refutation) and the latest application's level. This pins the engine's one slot until the key
//!       grows the arm; it does not assert the measurement.
//!
//! No measurement on disk: the goblin's level under a level-12 curse that kills, its damage, a non-crown building's
//! pulse, a unit with its own death spawn cursed, pulses 3-4 ticks after the last application, deaths inside a footprint.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test goblin_curse`):
//!   * `curse_parent_as_disc` -- the parent read as a disc of its own Damage, the circle lost: (2) and (4) go red.
//!   * `curse_child_with_parent` -- the circle made with its parent, one tick early: (3) goes red.
//!   * `crown_per_hit_ignored` -- the crown tower takes the percent route (35): (5) goes red.
//!   * `buff_death_spawn_dropped` -- no unit for a buff's death spawn: (6) goes red.
//!   * `curse_mark_dropped` -- the impact never hangs its second buff: (6) goes red.
//!   * `buff_death_spawn_acquire_delayed` -- the goblin waits out the death spawn's acquire delay: (6) goes red.
//!   * `ignore_buff_not_read` -- IgnoreBuff is not read: (7) goes red.
//!   * `curse_on_buildings` -- the mark lands on buildings: (8) goes red.
//!   * `hash_skips_buff_source` -- a slot's source level is not hashed: (9) goes red.
//!   * `curse_refresh_keeps_source` -- a refresh keeps the first application's level: (10) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellPlacement, SpellShape};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AreaSpawnedAreaStart, BattleConfig, BattleState, CrownPerHitScaling};
use royalesim::status::{compose, BuffSlot, Sel};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The shipped config, asserting the arms this file pins.
fn shipped() -> BattleConfig {
    let c = config();
    assert_eq!(c.calib.area_spawned_area_start, AreaSpawnedAreaStart::OnParentFirstUpdate, "the shipped spells.AREA_SPAWNED_AREA_START");
    assert_eq!(c.calib.crown_per_hit_scaling, CrownPerHitScaling::LevelScaled, "the shipped status.CROWN_TOWER_DAMAGE_PER_HIT_SCALING");
    c
}

/// (the damage buff, the mark): the curse circle's two buffs, as `CardDb::buffs` indices.
fn curse_buffs(s: &BattleState) -> (u16, u16) {
    let SpellShape::Fuse { then, .. } = &card_stat(s, "GoblinCurse").spell.as_ref().expect("GoblinCurse is a spell").shape else { panic!("the curse is not a Fuse") };
    let SpellShape::PulsingAreaEffect { hit, .. } = then.as_ref() else { panic!("the curse's fuse releases no pulsing area: {then:?}") };
    (hit.buff.expect("the circle's damage buff").buff, hit.buff2.expect("the circle's mark").buff)
}

/// Entity `id`'s slot of buff `buff`, if it carries one.
fn slot(s: &BattleState, id: EntityId, buff: u16) -> Option<BuffSlot> {
    s.entity(id).and_then(|v| v.buffs.iter().copied().find(|b| b.id == buff + 1))
}

/// One pulse of the damage buff at `level` (status.BUFF_PULSE_AMOUNT).
fn pulse(s: &BattleState, level: i32) -> i32 {
    let (damage, _) = curse_buffs(s);
    let idx = s.cards().index("GoblinCurse").unwrap();
    s.cards().buffs[damage as usize].pulse_amount(s.config().calib.buff_pulse_amount, |m| s.cards().scaled(idx, level, m)).expect("a valid level")
}

/// Every (k, hp lost) of `id` from the cast tick, against `before` hp read each tick.
fn losses(s: &mut BattleState, id: EntityId, ticks: u32) -> Vec<(u32, i32)> {
    let mut out = Vec::new();
    for k in 0..ticks {
        let b = s.entity(id).map_or(0, |v| v.hp);
        s.tick();
        let now = s.entity(id).map_or(0, |v| v.hp);
        if now < b {
            out.push((k, b - now));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// (1)

/// A synthetic Goblin Curse: the parent area (`hits_ground`), whose action spawns the circle (its spawn entry carries
/// `spawn_unread`), the circle's hit entries `on_hit`, and the goblin. `MARK` and `DAMAGE` below are the two entries
/// the real circle hangs.
fn synthetic(hits_ground: bool, spawn_unread: &str, on_hit: &str) -> String {
    format!(
        r#"{{"cards":[{{"name":"Hex","kind":"spell","elixir":2,"rarity":"Epic",
        "spell":{{"area_effect_object":{{"name":"HexParent","life_duration_ms":6000,"radius_milli":3000,"hit_speed_ms":-1,"damage":100,
        "only_enemies":true,"hits_ground":{hits_ground},"hits_air":false,
        "action_graph":{{"roots":{{"OnStartingAction":"HexSet"}},"class_types":["ActionGroup","ActionPlayEffect","ActionSpawn"],
        "spawns":["AreaEffectType:HexCircle"],"mechanic":true}},
        "schedule":{{"root":"HexSet","entries":[{{"delay_ms":0,"class":"ActionPlayEffect","cosmetic":true}},
        {{"delay_ms":0,"class":"ActionSpawn","spawn_type":"AreaEffectType","spawn":"HexCircle","unread":[{spawn_unread}]}},
        {{"delay_ms":5950,"class":"ActionPlayEffect","cosmetic":true}}]}}}}}}}}],
        "area_effect_objects":{{"HexCircle":{{"name":"HexCircle","life_duration_ms":6000,"radius_milli":3000,"hit_speed_ms":50,
        "only_enemies":true,"hits_ground":true,"hits_air":true,
        "action_graph":{{"roots":{{"OnHitAction":"HexBuffs"}},"class_types":["ActionGroup"],"spawns":[],"mechanic":false}},
        "on_hit":{{"root":"HexBuffs","entries":[{on_hit}]}}}}}},
        "units":{{"Hexling":{{"name":"Hexling","rarity":"Common","hitpoints":79,"hit_speed_ms":1100,"range_milli":500,
        "collision_radius_milli":500,"speed":120,"deploy_time_ms":1000}}}}}}"#
    )
}

/// The mark: a buff whose carrier leaves a Hexling when it dies. `allowed` is its OtherBuffDeathSpawnAllowed.
fn mark(allowed: &str) -> String {
    format!(
        r#"{{"delay_ms":0,"class":"ActionSpawn","spawn_type":"BuffType","spawn":"HexMark","spawn_time_ms":100,
        "buff":{{"name":"HexMark","death_spawn":{{"character":"Hexling","count":1,"is_enemy":true,"deploy_delay":true,
        "same_location":true{allowed}}},"ignore_buildings":true}}}}"#
    )
}

/// A damage-over-time buff of `dps` named `name`.
fn damage(name: &str, dps: i32) -> String {
    format!(
        r#"{{"delay_ms":0,"class":"ActionSpawn","spawn_type":"BuffType","spawn":"{name}","spawn_time_ms":100,
        "buff":{{"name":"{name}","damage_per_second":{dps},"hit_frequency_ms":1000,"speed_multiplier_raw":-15,"crown_tower_damage_per_hit":4}}}}"#
    )
}

/// A plain slow named `name`: it neither pulses nor releases anything.
fn slow(name: &str) -> String {
    format!(
        r#"{{"delay_ms":0,"class":"ActionSpawn","spawn_type":"BuffType","spawn":"{name}","spawn_time_ms":100,
        "buff":{{"name":"{name}","speed_multiplier_raw":-30}}}}"#
    )
}

/// Ok when `card` loads from `text`, else the reason it is refused.
fn refusal(text: &str, card: &str) -> Result<(), String> {
    let db = CardDb::from_json_str(text, CardSource::DerivedJson).map_err(|e| format!("the file does not parse: {e}"))?;
    match db.rejected.iter().find(|(n, _)| n == card) {
        Some((_, why)) => Err(why.clone()),
        None if db.index(card).is_some() => Ok(()),
        None => Err(format!("{card} neither loads nor is refused")),
    }
}

/// Plant: curse_parent_as_disc.
#[test]
fn goblin_curse_loads_as_a_zero_fuse_over_its_curse_circle() {
    let s = BattleState::new(0, shipped());
    let spell = card_stat(&s, "GoblinCurse").spell.as_ref().expect("GoblinCurse loads as a spell");
    assert_eq!(spell.placement, SpellPlacement::Anywhere);
    let SpellShape::Fuse { fuse_ms, then } = &spell.shape else { panic!("GoblinCurse: {:?}", spell.shape) };
    assert_eq!(*fuse_ms, 0, "a zero fuse: the parent area collapses");
    let SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms, child } = then.as_ref() else { panic!("{then:?}") };
    assert_eq!((*life_ms, *hit_speed_ms, hit.radius, child.is_none()), (6000, 50, 3000 * K, true));
    assert!(hit.only_enemies && hit.hits_air && hit.hits_ground && !hit.ignore_buildings, "the circle's filters: {hit:?}");
    let (damage_buff, mark_buff) = curse_buffs(&s);
    let (bd, bm) = (hit.buff.unwrap(), hit.buff2.unwrap());
    assert_eq!((bd.time_ms, bm.time_ms), (100, 100), "each application lasts its entry's SpawnTime");
    let d = s.cards().buffs[damage_buff as usize];
    assert_eq!((d.damage_per_second, d.hit_frequency_ms, d.speed_pct, d.crown_hit), (14, 1000, -15, 4), "GoblinCurseDamage");
    assert!(d.death_spawn.is_none() && !d.ignore_buildings);
    let m = s.cards().buffs[mark_buff as usize];
    assert!(m.ignore_buildings && !m.pulses(), "the mark: {m:?}");
    let ds = m.death_spawn.expect("the mark releases a goblin");
    assert_eq!((ds.count, ds.for_other_side, ds.deploy_delay, ds.same_location), (1, true, true, true));
    let unit = s.cards().get(ds.unit);
    assert_eq!((unit.name.as_str(), unit.summon_only), ("GoblinCurseGoblin", true), "the mark's unit");
    // The goblin reports the card that cursed its victim (py.rs `ids_of_indices`).
    let catalogue: Vec<u16> = ["Knight", "Zap", "GoblinCurse"].iter().map(|n| s.cards().index(n).expect("loads")).collect();
    assert_eq!(royalesim::py::ids_of_indices(s.cards(), &catalogue)[ds.unit as usize], 2, "the goblin reports the Goblin Curse's catalogue id");
    // The shapes it refuses, on a synthetic copy that loads.
    let both = format!("{},{}", mark(r#","other_buff_death_spawn_allowed":true"#), damage("HexDamage", 14));
    assert_eq!(refusal(&synthetic(false, "", &both), "Hex"), Ok(()), "the synthetic copy must load");
    let why = refusal(&synthetic(true, "", &both), "Hex").unwrap_err();
    assert!(why.contains("runs an action graph this loader does not read"), "a parent that hits the ground keeps today's refusal: {why}");
    let why = refusal(&synthetic(false, r#""columns ActionDelay""#, &both), "Hex").unwrap_err();
    assert!(why.contains("runs an action graph this loader does not read"), "a delayed spawn (the global Lightning's) keeps today's refusal: {why}");
    let three = format!("{both},{}", slow("HexSlow"));
    let why = refusal(&synthetic(false, "", &three), "Hex").unwrap_err();
    assert!(why.contains("hangs 3 buffs"), "a third buff: {why}");
    let pulsing_pair = format!("{},{}", damage("HexDamage", 14), damage("HexBurn", 20));
    let why = refusal(&synthetic(false, "", &pulsing_pair), "Hex").unwrap_err();
    assert!(why.contains("two buffs that pulse"), "two pulsing buffs: {why}");
    // A mark without OtherBuffDeathSpawnAllowed loads (item 296): it takes the place of its carrier's own death spawn.
    assert!(!ds.suppresses_own, "the Goblin Curse's mark sets OtherBuffDeathSpawnAllowed: its victim's own death spawn comes");
    let unallowed = format!("{},{}", mark(""), damage("HexDamage", 14));
    assert_eq!(refusal(&synthetic(false, "", &unallowed), "Hex"), Ok(()), "a mark without OtherBuffDeathSpawnAllowed loads");
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: curse_parent_as_disc.
#[test]
fn the_parent_damage_never_lands() {
    let mut s = BattleState::new(0, shipped());
    // A Red Knight on the Red half, where no tower reaches it; it walks, and stays inside the circle for 25 ticks.
    let k = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 22000)), None).expect("spawn");
    s.spawn_unit(Team::Blue, "GoblinCurse", s.entity(k).unwrap().pos, None).expect("cast the curse");
    let got = losses(&mut s, k, 26);
    let p = pulse(&s, s.config().card_level[0]);
    assert_eq!(got, vec![(21, p)], "the Knight loses the circle's first pulse on C + 21 and nothing else (the parent's Damage never lands)");
}

// ---------------------------------------------------------------------------
// (3)

/// A Red Giant attacking the Blue left princess tower: it stands for as long as the test runs. Its id and the battle.
fn standing_giant(cfg: BattleConfig) -> (BattleState, EntityId) {
    let mut s = BattleState::new(0, cfg);
    let tower = s.entities().find(|v| v.team == Team::Blue && v.card == "PrincessTower" && v.pos.x < s.arena().width / 2).map(|v| v.pos).expect("the Blue left princess tower");
    let g = s.scenario_spawn_now(Team::Red, "Giant", Vec2::new(tower.x, tower.y + 2500 * K), None).expect("spawn a Giant");
    (s, g)
}

/// Plant: curse_child_with_parent.
#[test]
fn the_circle_applies_from_c_plus_1_to_c_plus_120() {
    // (a) A walking Knight's first slowed step is C + 2: its speed is slowed from the end of C + 1.
    let mut s = BattleState::new(0, shipped());
    let (damage_buff, _) = curse_buffs(&s);
    let def = s.cards().buffs[damage_buff as usize];
    let spt = s.config().calib.speed_to_subtiles_per_tick;
    let slowed = |v: i32| compose([def].iter(), Sel::Speed, v / spt) * spt;
    let k = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 22000)), None).expect("spawn");
    let base = s.entity(k).unwrap().speed;
    assert!(slowed(base) < base, "the scene drifted: the slow slows nothing");
    s.spawn_unit(Team::Blue, "GoblinCurse", s.entity(k).unwrap().pos, None).expect("cast the curse");
    let mut speeds = Vec::new();
    for _ in 0..4u32 {
        s.tick();
        speeds.push(s.entity(k).unwrap().speed_now);
    }
    assert_eq!(speeds, vec![base, slowed(base), slowed(base), slowed(base)], "the walking Knight's speed after C + 0 to C + 3: the first slowed step is C + 2");
    // (b) A standing Giant: an application on every tick C + 1 to C + 120 (its slot refreshed to 100 ms), the last slowed
    // step C + 122.
    let (mut s, g) = standing_giant(shipped());
    s.spawn_unit(Team::Blue, "GoblinCurse", s.entity(g).unwrap().pos, None).expect("cast the curse");
    let gbase = s.entity(g).unwrap().speed;
    let (mut applied, mut slow) = (Vec::new(), Vec::new());
    for t in 0..124u32 {
        s.tick();
        let v = s.entity(g).unwrap_or_else(|| panic!("the scene drifted: the Giant died on C + {t}"));
        if slot(&s, g, damage_buff).is_some_and(|b| b.ms == 100) {
            applied.push(t);
        }
        if v.speed_now < gbase {
            slow.push(t);
        }
    }
    assert_eq!(applied, (1..=120).collect::<Vec<u32>>(), "the circle applies on C + 1 to C + 120");
    assert_eq!(slow, (1..=121).collect::<Vec<u32>>(), "slowed from the end of C + 1 to the end of C + 121: the steps C + 2 to C + 122");
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: curse_parent_as_disc.
#[test]
fn a_standing_giant_takes_six_pulses_of_35() {
    for level in [11, 12] {
        let (mut cast, g) = standing_giant(shipped());
        let (mut control, g2) = standing_giant(shipped());
        assert_eq!(g, g2);
        cast.spawn_unit(Team::Blue, "GoblinCurse", cast.entity(g).unwrap().pos, Some(level)).expect("cast the curse");
        let mut got = Vec::new();
        let mut last = 0;
        for t in 0..125u32 {
            cast.tick();
            control.tick();
            let diff = control.entity(g).map_or(0, |v| v.hp) - cast.entity(g).map_or(0, |v| v.hp);
            if diff != last {
                got.push((t, diff - last));
                last = diff;
            }
        }
        let p = pulse(&cast, level);
        match level {
            11 => assert_eq!(p, 35, "14 a second at level 11"),
            _ => assert_eq!(p, 39, "14 a second at level 12"),
        }
        let want: Vec<(u32, i32)> = [21, 41, 61, 81, 101, 121].iter().map(|t| (*t, p)).collect();
        assert_eq!(got, want, "a level-{level} curse: the Giant's losses against the control");
    }
}

// ---------------------------------------------------------------------------
// (5)

/// Plant: crown_per_hit_ignored.
#[test]
fn a_crown_tower_takes_ten_a_pulse() {
    let mut s = BattleState::new(0, shipped());
    let (damage_buff, mark_buff) = curse_buffs(&s);
    let (tower, pos) = s.entities().find(|v| v.team == Team::Red && v.card == "PrincessTower").map(|v| (v.id, v.pos)).expect("a Red princess tower");
    s.spawn_unit(Team::Blue, "GoblinCurse", pos, None).expect("cast the curse");
    let level = s.config().card_level[0];
    let idx = s.cards().index("GoblinCurse").unwrap();
    let per_hit = s.cards().scaled(idx, level, 4).expect("a valid level");
    if level == 11 {
        assert_eq!(per_hit, 10, "floor(4 x 256 / 100)");
    }
    let got = losses(&mut s, tower, 125);
    let want: Vec<(u32, i32)> = [21, 41, 61, 81, 101, 121].iter().map(|t| (*t, per_hit)).collect();
    assert_eq!(got, want, "the crown tower's losses: CrownTowerDamagePerHit scaled, six pulses");
    assert!(slot(&s, tower, mark_buff).is_none(), "the mark never lands on a crown tower");
    let _ = damage_buff;
}

// ---------------------------------------------------------------------------
// (6)

/// Red Skeletons deployed from hand at `at_p` (a deploying unit stands still), cursed by Blue on C, zapped by Blue on
/// C + 2, before any pulse. Returns the battle after C + 2 (S, the death tick), the skeletons' points, and the goblins
/// that appeared on S.
fn cursed_and_zapped(mut s: BattleState, at_p: Vec2) -> (BattleState, Vec<Vec2>, Vec<EntityId>) {
    s.spawn_unit(Team::Red, "Skeletons", at_p, None).expect("deploy the Skeletons");
    s.tick();
    let skeletons: Vec<(EntityId, Vec2)> = s.entities().filter(|e| e.team == Team::Red && e.card == "Skeletons" && e.deploying).map(|e| (e.id, e.pos)).collect();
    assert_eq!(skeletons.len(), 3, "the scene drifted: the card deployed {} Skeletons", skeletons.len());
    s.spawn_unit(Team::Blue, "GoblinCurse", at_p, None).expect("cast the curse");
    s.tick(); // C: the circle is made
    s.tick(); // C + 1: it applies
    let (_, mark_buff) = curse_buffs(&s);
    for (id, _) in &skeletons {
        assert!(slot(&s, *id, mark_buff).is_some(), "the scene drifted: a Skeleton carries no mark after C + 1");
    }
    // Their points as they die: a deploying unit stands, so the points after C + 1 are the points on S.
    let skeletons: Vec<(EntityId, Vec2)> = skeletons.iter().map(|(id, _)| (*id, s.entity(*id).expect("a Skeleton died early").pos)).collect();
    s.spawn_unit(Team::Blue, "Zap", at_p, None).expect("cast the Zap");
    s.tick(); // C + 2 = S: the Zap kills them
    for (id, _) in &skeletons {
        assert!(s.entity(*id).is_none(), "the scene drifted: a Skeleton outlived the Zap");
    }
    let goblins: Vec<EntityId> = s.entities().filter(|e| e.card == "GoblinCurseGoblin").map(|e| e.id).collect();
    (s, skeletons.into_iter().map(|(_, p)| p).collect(), goblins)
}

/// Plants: buff_death_spawn_dropped, curse_mark_dropped, buff_death_spawn_acquire_delayed.
#[test]
fn a_cursed_troop_that_dies_becomes_a_goblin_for_the_caster() {
    // Away from every tower: both Red princess towers down (the Red king is then awake, and out of reach).
    let mut s = BattleState::new(0, shipped());
    for k in [1, 2] {
        s.scenario_set_tower_hp(Team::Red, k, 0).expect("a Red princess tower down");
    }
    let (mut s, points, goblins) = cursed_and_zapped(s, at((9000, 18500)));
    assert_eq!(goblins.len(), 3, "one goblin per cursed Skeleton, on the death tick");
    let level = s.config().card_level[0];
    let unit = s.cards().index("GoblinCurseGoblin").expect("the goblin loads as a unit");
    let hp = s.cards().scaled(unit, level, s.cards().get(unit).hitpoints).expect("a valid level");
    if level == 11 {
        assert_eq!(hp, 202, "the goblin's 79 at level 11");
    }
    let mut stood: Vec<Vec2> = Vec::new();
    for id in &goblins {
        let v = s.entity(*id).unwrap();
        assert_eq!(v.team, Team::Blue, "the goblin is the caster's");
        assert_eq!((v.hp, v.max_hp), (hp, hp), "the goblin at the curse's level");
        assert!(v.deploying, "the goblin deploys");
        stood.push(v.pos);
    }
    let mut want = points.clone();
    want.sort_by_key(|p| (p.x, p.y));
    stood.sort_by_key(|p| (p.x, p.y));
    assert_eq!(stood, want, "each goblin stands on its Skeleton's point (DeathSpawnSameLocation)");
    // 20 deploying ticks, first step S + 21.
    let mut deploying = Vec::new();
    let mut moved = None;
    for t in 1..=21u32 {
        s.tick();
        let v = s.entity(goblins[0]).unwrap_or_else(|| panic!("the scene drifted: the goblin died on S + {t}"));
        deploying.push(v.deploying);
        if moved.is_none() && !stood.contains(&v.pos) {
            moved = Some(t);
        }
    }
    assert_eq!(deploying, (1..=21u32).map(|t| t < 20).collect::<Vec<_>>(), "deploying through S + 19");
    assert_eq!(moved, Some(21), "the goblin's first step is S + 21");
    // Near a Red princess tower: the tower targets a goblin on the tick after it appears.
    let s = BattleState::new(0, shipped());
    let tower = s.entities().find(|v| v.team == Team::Red && v.card == "PrincessTower" && v.pos.x < s.arena().width / 2).map(|v| (v.id, v.pos)).expect("the Red left princess tower");
    let (mut s, _, goblins) = cursed_and_zapped(s, Vec2::new(tower.1.x + 2500 * K, tower.1.y - 3500 * K));
    assert_eq!(goblins.len(), 3, "the scene drifted: {} goblins near the tower", goblins.len());
    s.tick();
    let target = s.entity(tower.0).and_then(|v| v.target);
    assert!(target.is_some_and(|t| goblins.contains(&t)), "the Red tower does not target a goblin on S + 1 (target {target:?})");
}

// ---------------------------------------------------------------------------
// (7)

/// Plant: ignore_buff_not_read.
#[test]
fn the_battle_ram_takes_the_damage_and_the_slow_but_never_the_mark() {
    let mut s = BattleState::new(0, shipped());
    let (damage_buff, mark_buff) = curse_buffs(&s);
    let ram = s.scenario_spawn_now(Team::Red, "BattleRam", at((9000, 22000)), None).expect("spawn a Battle Ram");
    assert!(card_stat(&s, "BattleRam").ignore_buffs.contains(&mark_buff), "the scene drifted: the Battle Ram's row does not list the mark");
    s.spawn_unit(Team::Blue, "GoblinCurse", s.entity(ram).unwrap().pos, None).expect("cast the curse");
    s.tick();
    s.tick();
    assert!(slot(&s, ram, damage_buff).is_some(), "the Battle Ram takes the damage and the slow");
    assert!(slot(&s, ram, mark_buff).is_none(), "the Battle Ram never takes the mark (IgnoreBuff)");
    let v = s.entity(ram).unwrap();
    assert!(v.speed_now < v.speed, "the Battle Ram is slowed");
    let before: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    assert!(s.debug_set_hp(ram, 0));
    s.tick();
    let fresh: Vec<_> = s.entities().filter(|e| !before.contains(&e.id)).map(|e| (e.team, e.card.to_string())).collect();
    let ds = card_stat(&s, "BattleRam").death_spawn.expect("the Battle Ram's death spawn");
    let barbarian = s.cards().get(ds.unit).name.clone();
    assert_eq!(fresh, vec![(Team::Red, barbarian.clone()); ds.count as usize], "its own {} and no goblin", barbarian);
}

// ---------------------------------------------------------------------------
// (8)

/// Plant: curse_on_buildings.
#[test]
fn a_building_is_never_marked() {
    let mut s = BattleState::new(0, shipped());
    let (damage_buff, mark_buff) = curse_buffs(&s);
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at((9000, 22000)), None).expect("spawn a Red Cannon");
    s.spawn_unit(Team::Blue, "GoblinCurse", s.entity(cannon).unwrap().pos, None).expect("cast the curse");
    s.tick();
    s.tick();
    assert!(slot(&s, cannon, damage_buff).is_some(), "the Cannon takes the damage buff");
    assert!(slot(&s, cannon, mark_buff).is_none(), "the Cannon never takes the mark (IgnoreBuildings)");
    assert!(s.debug_set_hp(cannon, 0));
    s.tick();
    assert_eq!(s.entities().filter(|e| e.card == "GoblinCurseGoblin").count(), 0, "a building leaves no goblin");
}

// ---------------------------------------------------------------------------
// (9)

/// Plant: hash_skips_buff_source.
#[test]
fn a_curse_survives_a_save_with_its_source_level() {
    let mut s = BattleState::new(0, shipped());
    let (_, mark_buff) = curse_buffs(&s);
    let k = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 22000)), None).expect("spawn");
    s.spawn_unit(Team::Blue, "GoblinCurse", s.entity(k).unwrap().pos, None).expect("cast the curse");
    s.tick();
    s.tick();
    let level = slot(&s, k, mark_buff).expect("the scene drifted: the Knight carries no mark").src_level;
    assert_eq!(level, s.config().card_level[0], "the mark carries the curse's level");
    let hashed = edit_is_hashed(&s, |v| {
        let slots = v["ents"]["buffs"].as_array_mut().expect("the snapshot carries the buff slots");
        let sl = slots.iter_mut().find(|b| b["id"].as_u64() == Some(mark_buff as u64 + 1)).expect("the mark's slot is saved");
        sl["src_level"] = serde_json::Value::from(level + 1);
    });
    assert!(hashed, "a save edited only in a mark's source level loads under the old hash: the level is not hashed");
}

// ---------------------------------------------------------------------------
// (10)

/// Plant: curse_refresh_keeps_source.
#[test]
fn two_curses_on_one_unit_share_one_slot_a_known_divergence() {
    let (mut s, g) = standing_giant(shipped());
    let (damage_buff, _) = curse_buffs(&s);
    let pos = s.entity(g).unwrap().pos;
    s.spawn_unit(Team::Blue, "GoblinCurse", pos, Some(11)).expect("cast a level-11 curse");
    for _ in 0..20u32 {
        s.tick();
    }
    s.spawn_unit(Team::Blue, "GoblinCurse", pos, Some(12)).expect("cast a level-12 curse");
    for _ in 0..3u32 {
        s.tick();
    }
    let slots: Vec<BuffSlot> = s.entity(g).unwrap().buffs.iter().copied().filter(|b| b.id == damage_buff + 1).collect();
    const MEASURED: &str = "measured on client 15.535.29: two curses on one unit pulse on their own clocks and at their own levels \
                            (35 and 39 a pulse); the engine keeps one slot per buff row (status.BUFF_STACKING, a named gap)";
    assert_eq!(slots.len(), 1, "one slot for the two curses' damage buff ({MEASURED})");
    assert_eq!((slots[0].src_level, slots[0].pulse_amount), (12, pulse(&s, 12)), "the slot takes the latest application's level ({MEASURED})");
}
