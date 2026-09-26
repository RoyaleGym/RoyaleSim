//! THE DEATH BOMB: the thing a dying Balloon, Giant Skeleton or Bomb Tower leaves
//! where it fell, which goes off a while later.
//!
//! card.rs `convert_death_bomb` and state.rs `phase_reap`. The three cards used to
//! be refused, because the loader read their DeathSpawnCharacter as a UNIT and the
//! row it names has no hitpoints. It is not a unit: it is one area hit on a timer,
//! at the point of death, and it now loads and runs as that.
//!
//! WHAT IS PINNED, every number read from the loaded CardDb (nothing pasted):
//!   1. the three cards load, their death-spawn record is a bomb carrying the file's
//!      DeployTime, DeathDamage and DeathDamageRadius, and no OTHER block of any
//!      card (`CardDb::unit_refs`: spawner, spell release, second summon) names one;
//!   2. THE FUSE. The damage lands `DeployTime / TICK_MS + 1` ticks after the death
//!      and NOT on the death tick;
//!   3. the amount: the bomb row's DeathDamage on its own ladder at the card's
//!      level, taken off a crown tower at the row's own crown percent;
//!   4. the bomb is not on the board -- no entity appears, the census is unchanged
//!      for the whole fuse, and a waiting enemy never holds a target that is not
//!      there;
//!   5. enemies only, and it really does fire: against a control run with no bomb at
//!      all, the friendly unit's hitpoints are identical and the enemy's are short
//!      by exactly the bomb's amount;
//!   6. both seats get the same fuse and the same amount, on either princess;
//!   7. a bomb still counting down survives a save and goes off on the same tick.
//!
//! HOW A REVIEWER PLANTS THE DEFECT EACH ONE CATCHES. This suite has no
//! `clash_plant` arm; these are edits to make by hand, run, and undo.
//!   (2) in state.rs `phase_reap`, pass `delay_ms: 0` -- which is what the MEASURED
//!       death-spawn deploy default (spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT = zero)
//!       would hand a spawned unit -- instead of `fuse_ms`. The bomb then goes off on
//!       the death tick: (2) and (4) go red and (3) stays green, which is exactly the
//!       pair of readings the corpus had to separate.
//!   (3) in card.rs `convert_death_bomb`, drop the `c.level_table = level_table`
//!       line: the tower loses the level-1 figure instead of the card-level one.
//!   (4) in `phase_reap`, delete the `death_bomb_fuse_ms` branch so a bomb falls
//!       through to the ordinary death spawn: a hitpoint-less BUILDING appears at the
//!       death point and the census grows.
//!   (5) set `only_enemies: false` in `convert_death_bomb`: the friendly loses
//!       hitpoints the control run says it should not.
//!   (6) give the impact an absolute-frame offset (aim at `pos.add(Vec2::new(0,
//!       SUBTILE))` in `phase_reap` instead of `pos`): the seats stop agreeing.

mod common;

use common::*;
use royalesim::card::{CardDb, CardDef, SpellDef, SpellHit, SpellShape, UnitRef};
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE};
use royalesim::state::{BattleConfig, BattleState, Calib};
use royalesim::{EntityId, Team};

/// The cards whose death leaves a bomb, in the shipped data.
const BOMB_CARDS: [&str; 3] = ["Balloon", "GiantSkeleton", "BombTower"];

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(11, cfg)
}

fn tick_ms() -> i32 {
    Calib::shipped().tick_ms
}

/// The bomb record a card's death leaves: (its CardDb index, the record).
fn bomb_of<'a>(db: &'a CardDb, card: &str) -> Option<(u16, &'a CardDef)> {
    let ds = db.get(db.index(card)?).death_spawn?;
    let u = db.get(ds.unit);
    u.death_bomb_fuse_ms().map(|_| (ds.unit, u))
}

/// The impact a bomb card carries (`convert_death_bomb` builds exactly one shape).
fn bomb_hit(c: &CardDef) -> SpellHit {
    match &c.spell {
        Some(SpellDef { shape: SpellShape::Projectile { hit: Some(h), .. }, .. }) => *h,
        _ => panic!("{}: a death bomb must carry one projectile impact", c.name),
    }
}

/// Tower `k` of `team` and the point it stands on.
fn tower_at(s: &BattleState, team: Team, k: usize) -> (EntityId, Vec2) {
    let id = s.tower_ids(team)[k].expect("the tower stands at setup");
    (id, s.entity(id).expect("a live tower").pos)
}

// ---------------------------------------------------------------------------
// (1) what the data loads as

#[test]
fn every_death_bomb_row_loads_as_a_timed_impact_and_only_a_death_releases_one() {
    let db = cards();
    let mut seen = 0;
    for card in BOMB_CARDS {
        let idx = db
            .index(card)
            .unwrap_or_else(|| panic!("{card} is not simulable: {:?}", db.rejected.iter().find(|(n, _)| n == card)));
        let ds = db.get(idx).death_spawn.unwrap_or_else(|| panic!("{card}: no death_spawn read"));
        let (unit, bomb) = bomb_of(&db, card).unwrap_or_else(|| panic!("{card}: its death-spawn record is not a bomb"));
        assert_eq!(unit, ds.unit);
        let hit = bomb_hit(bomb);
        // Every number is the file's, read back off the CardDef the loader built.
        assert_eq!(hit.damage, bomb.death_damage, "{card}: the impact is not the row's DeathDamage");
        assert_eq!(hit.radius, bomb.death_damage_radius, "{card}: the impact is not the row's DeathDamageRadius");
        assert_eq!(bomb.death_bomb_fuse_ms(), Some(bomb.deploy_time_ms), "{card}: the fuse is not the row's DeployTime");
        assert!(bomb.deploy_time_ms > 0, "{card}: a fuse of 0 would go off on the death tick");
        assert!(hit.damage > 0 && hit.radius > 0, "{card}: a bomb with no damage or no radius is not a bomb");
        assert!(hit.only_enemies, "{card}: a death bomb does not hit its own side");
        assert!(bomb.hitpoints == 0 && bomb.summon_only, "{card}: the bomb row is not a unit and must not be playable");
        // `phase_reap` leaves ONE bomb per death. That is only right while every row
        // asks for one; a row that asked for more would need a layout nothing has
        // measured, and this is the assertion that would say so.
        assert_eq!(ds.count, 1, "{card}: DeathSpawnCount {} -- the engine leaves one bomb, not {}", ds.count, ds.count);
        seen += 1;
    }
    assert_eq!(seen, BOMB_CARDS.len(), "vacuous: no bomb card loaded");
    // A bomb is released by a DEATH SPAWN and by nothing else: any other block that
    // named one (`CardDb::unit_refs`: a spawner, a spell release, a second summon)
    // would reach `spawn_now` with a hitpoint-less record (card.rs refuses such a card
    // instead).
    let mut others = 0;
    for idx in 0..db.cards.len() as u16 {
        for (path, u, _) in db.unit_refs(idx) {
            if path == UnitRef::DeathSpawn {
                continue;
            }
            others += 1;
            assert!(
                db.cards.get(u as usize).and_then(CardDef::death_bomb_fuse_ms).is_none(),
                "{}: a bomb reached the board through {}, which is not a death spawn",
                db.get(idx).name,
                path.block_name()
            );
        }
    }
    assert!(others > 0, "vacuous: no card puts a unit on the board by any block but a death spawn");
}

// ---------------------------------------------------------------------------
// (2) + (3) the fuse and the amount

/// Kill a `team` `card` standing on the enemy's tower `k`: (ticks from the death
/// tick to the hit, hitpoints the tower lost).
fn fuse_and_amount(card: &str, team: Team, k: usize) -> (u32, i32) {
    let mut s = bare(config());
    let (tower, p) = tower_at(&s, team.other(), k);
    let before = s.entity(tower).expect("a live tower").hp;
    let id = s
        .scenario_spawn_now(team, card, p, None)
        .unwrap_or_else(|e| panic!("{card} could not be placed on the tower: {e:?}"));
    assert!(s.debug_set_hp(id, 0), "{card}: could not kill it");
    s.tick(); // the death tick: Resolve sees hp 0, Reap leaves the bomb
    let death_tick = s.tick_count();
    assert!(s.entity(id).is_none(), "{card}: it did not die");
    assert_eq!(s.spells().len(), 1, "{card}: the death left {} spell objects, not the one bomb", s.spells().len());
    let mut waited = 0;
    while s.entity(tower).expect("the tower outlives the bomb").hp == before {
        assert!(waited < 400, "{card}: the bomb never went off");
        s.tick();
        waited += 1;
    }
    (s.tick_count() - death_tick, before - s.entity(tower).expect("a live tower").hp)
}

#[test]
fn a_death_bomb_lands_a_fuse_after_the_death_for_its_own_rows_damage() {
    let db = cards();
    let calib = Calib::shipped();
    let level = config().card_level[0];
    for card in BOMB_CARDS {
        let (bomb_idx, bomb) = bomb_of(&db, card).unwrap_or_else(|| panic!("{card}: no bomb"));
        let fuse = bomb.deploy_time_ms;
        let (ticks, lost) = fuse_and_amount(card, Team::Blue, 1);
        // THE FUSE. The impact counts down one tick at a time and arrives on the
        // first tick after it reaches zero: fuse / TICK_MS + 1 ticks past the Reap
        // that left it, never on that Reap.
        assert_eq!(ticks, (fuse / calib.tick_ms) as u32 + 1, "{card}: the bomb landed {ticks} ticks after the death (fuse {fuse} ms)");
        assert!(ticks > 1, "{card}: the bomb went off on the death tick");
        // THE AMOUNT: the bomb row's DeathDamage on its own ladder at this level,
        // through the crown-tower reduction the row asks for (the assertion reads the
        // row's percent, it does not assume 100).
        let scaled = db.scaled(bomb_idx, level, bomb.death_damage).expect("the bomb's level is the card's");
        let want = royalesim::combat::damage_against(EntityKind::PrincessTower, scaled, bomb_hit(bomb).crown_pct, calib.crown_rounding);
        assert_eq!(lost, want, "{card}: the tower lost {lost}, not its bomb's {want}");
        assert!(lost > 0, "{card}: a bomb that costs nothing is not a bomb");
    }
}

// ---------------------------------------------------------------------------
// (4) it is not on the board

#[test]
fn a_death_bomb_never_becomes_an_entity_and_leaves_no_phantom_target() {
    let db = cards();
    let mut s = bare(config());
    let (tower, p) = tower_at(&s, Team::Red, 1);
    let max = s.entity(tower).expect("a live tower").max_hp;
    // A Red Knight two tiles off the death point: something that picks a target
    // every tick and is inside the bomb's radius.
    let knight = s
        .scenario_spawn_now(Team::Red, "Knight", Vec2::new(p.x, p.y - 2 * SUBTILE), None)
        .expect("a Knight on Red's own half");
    let before = census(&s);
    let id = s.scenario_spawn_now(Team::Blue, "Balloon", p, None).expect("a Balloon over the tower");
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    let fuse = bomb_of(&db, "Balloon").expect("a bomb").1.deploy_time_ms;
    for _ in 0..(fuse / tick_ms()) as u32 {
        // The census IS the board: a bomb that became an entity would show up here.
        assert_eq!(census(&s), before, "a death bomb put an entity on the board");
        assert_eq!(s.spells().len(), 1, "the bomb left the spell list early");
        if let Some(t) = s.entity(knight).and_then(|k| k.target) {
            assert!(s.entity(t).is_some(), "the Knight holds a target that is not on the board");
        }
        s.tick();
    }
    s.tick(); // the impact tick
    assert!(s.spells().is_empty(), "the bomb did not clear after it went off");
    assert!(s.entity(tower).expect("a live tower").hp < max, "the bomb never landed");
}

// ---------------------------------------------------------------------------
// (5) enemies only, against a control run with no bomb

/// Two Knights a tile either side of `mid`, ticked `n` times, with a Blue Balloon
/// killed on `mid` when `bomb` is set. Returns (Blue's hitpoints, Red's).
///
/// A CONTROL RUN, not a bare assertion: the two Knights fight each other, and the
/// only honest way to read the bomb's contribution out of that is to run the
/// identical battle without it. The Balloon is spawned LAST, so it cannot move
/// anyone's `team_seq`, and it is flying, so no Knight can target it in the one
/// tick it lives.
fn two_knights(bomb: bool, n: u32) -> (i32, i32) {
    let mut s = bare(config());
    // Mid-board, out of every crown tower's range, so the only damage in the battle
    // is the Knights' and the bomb's.
    let mid = t(900, 1300);
    let one = Vec2::new(SUBTILE, 0);
    let friend = s.scenario_spawn_now(Team::Blue, "Knight", mid.add(one), None).expect("a Blue Knight");
    let foe = s.scenario_spawn_now(Team::Red, "Knight", mid.sub(one), None).expect("a Red Knight");
    if bomb {
        let id = s.scenario_spawn_now(Team::Blue, "Balloon", mid, None).expect("a Balloon");
        assert!(s.debug_set_hp(id, 0));
    }
    for _ in 0..n {
        s.tick();
    }
    (s.entity(friend).map_or(0, |e| e.hp), s.entity(foe).map_or(0, |e| e.hp))
}

#[test]
fn a_death_bomb_spares_its_own_side_and_costs_the_enemy_exactly_its_amount() {
    let db = cards();
    let (bomb_idx, bomb) = bomb_of(&db, "Balloon").expect("a bomb");
    let n = (bomb.deploy_time_ms / tick_ms()) as u32 + 2;
    let (friend_with, foe_with) = two_knights(true, n);
    let (friend_without, foe_without) = two_knights(false, n);
    assert_eq!(friend_with, friend_without, "the bomb hit its own side");
    let want = db.scaled(bomb_idx, config().card_level[0], bomb.death_damage).expect("a level");
    assert_eq!(foe_without - foe_with, want, "the enemy Knight is not exactly one bomb short");
    assert!(want > 0 && foe_with > 0, "vacuous: the bomb cost nothing or killed the Knight");
}

// ---------------------------------------------------------------------------
// (6) both seats

#[test]
fn the_bomb_is_the_same_on_both_seats() {
    for card in BOMB_CARDS {
        for k in [1usize, 2] {
            let blue = fuse_and_amount(card, Team::Blue, k);
            let red = fuse_and_amount(card, Team::Red, k);
            assert_eq!(blue, red, "{card}: on tower {k} Blue got {blue:?} and Red {red:?}");
            assert!(blue.1 > 0, "{card}: vacuous, the bomb cost nothing");
        }
    }
}

// ---------------------------------------------------------------------------
// (7) a bomb in the air survives a save

#[test]
fn a_bomb_still_counting_down_survives_a_save_and_goes_off_on_the_same_tick() {
    let db = cards();
    let mut s = bare(config());
    let (tower, p) = tower_at(&s, Team::Red, 1);
    let before = s.entity(tower).expect("a live tower").hp;
    let id = s.scenario_spawn_now(Team::Blue, "Balloon", p, None).expect("a Balloon");
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert_eq!(s.spells().len(), 1);
    let blob = s.save();
    let mut back = BattleState::load(&blob).expect("the bomb reloads");
    assert_eq!(back.state_hash(), s.state_hash(), "the saved bomb is not in the hash");
    let fuse = bomb_of(&db, "Balloon").expect("a bomb").1.deploy_time_ms;
    for _ in 0..=(fuse / tick_ms()) as u32 {
        s.tick();
        back.tick();
        assert_eq!(back.state_hash(), s.state_hash(), "the resumed battle diverged");
    }
    assert!(s.entity(tower).expect("a live tower").hp < before, "the bomb never landed");
}
