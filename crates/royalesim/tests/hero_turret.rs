//! THE TURRET A HERO'S BUTTON PUTS DOWN, read off the engine: spawner.ABILITY_UNIT_FIRST_UPDATE (state.rs
//! `phase_spawn`) and targeting.SPAWNED_UNIT_ACQUIRE_DELAY = client_8th_frame_action_buildings (`delay_acquisition`),
//! both reading the release's `action_made`, which `fire_ability` alone sets. Both ship at their old arms,
//! creation_tick and client_8th_frame; every scene names its arms.
//!
//! THE EVIDENCE, measured on client 15.535.29 over the hero scenes (Parity's round 8 item 9; the ledger has the rows).
//! The Hero Musketeer's turret, first seen on frame c, leaves its deploy on c + 20 and first loses lifetime hp on c + 21
//! (13 of 13 turrets), and no enemy targets it before c + 7 (13 lookers in 5 scenes, every one on c + 7). A played
//! Cannon leaves its deploy on F + 19, first loses lifetime hp on F + 20 and is targeted on F + 0, F being its first
//! frame, in the client and in the engine alike.
//!
//! FRAMES ARE COUNTED AS THE MEASUREMENT COUNTS THEM: a unit's first frame is the state after the tick that created it;
//! it "leaves its deploy" on the first frame whose deploy timer reads 0, "first loses lifetime hp" on the first frame
//! whose hp is below the frame before's, and is "targeted" on the first frame whose looker's target is it.
//!
//! THE SCENES. A blue Hero Musketeer on (9000, 10000) presses her button on her deploy end; her turret appears 2500
//! ahead of her, on (9000, 12500). Blue's princess towers are down and nothing red comes near her, so the turret's hp moves
//! only by its lifetime drain. In the targeting scenes an idle red Cannon stands on (9000, 18000), 5500 from the turret
//! and 8000 from her: inside its reach of the turret, outside its reach of her (checked, not assumed: it holds no target
//! before the turret appears, and under the old arm it takes the turret on the turret's first frame). The played blue
//! Cannon goes on (9000, 14000), 4000 from the red one.
//!
//! WHAT IS PINNED, and the plant that turns each check red (`RUSTFLAGS='--cfg clash_plant="NAME"'
//! CARGO_TARGET_DIR=target/plant cargo test --profile gate --test hero_turret`):
//!   1. `the_turret_takes_no_update_on_its_creation_tick_under_client_next_tick`: under client_next_tick the turret
//!      leaves its deploy on c + 20 and first loses lifetime hp on c + 21, with the acquire key at either arm; under
//!      creation_tick on c + 19 and c + 20; a played Cannon on F + 19 and F + 20 under both
//!      -- ability_unit_counts_down_at_creation (the new arm counts the turret down on its creation tick);
//!   2. `no_enemy_targets_the_turret_before_c7_under_client_8th_frame_action_buildings`: under the new arm the red
//!      Cannon first targets the turret on c + 7, whichever arm spawner.ABILITY_UNIT_FIRST_UPDATE is at; under
//!      client_8th_frame on c + 0; a played Cannon on F + 0 under both
//!      -- action_building_acquired_at_once (the new arm exempts the turret as client_8th_frame does);
//!   3. `both_keys_ship_their_old_arms`;
//!   4. `the_turrets_blow_lands_on_c2_under_client_on_landing_action_at_2`: combat.DEPLOY_PROJECTILE's action arm
//!      lands the turret's deploy blow on a red Knight 1500 ahead of it on c + 2, the old arm client_on_landing on
//!      c + 6, the same damage under both (read on client 15.535.29: every enemy hp drop within 2,600 of a new
//!      turret falls on c + 2) -- action_blow_at_play_delay (the action arm keeps the play's 6 ticks).
//!
//! The plant acquire_delay_on_buildings (tests/spawn_acquire_delay.rs) makes every flagged building wait under
//! client_8th_frame too, the turret included: (2)'s old arm goes red under it as well.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AbilityUnitFirstUpdate, BattleConfig, BattleState, Calib, DeployProjectile, SpawnedUnitAcquireDelay};
use royalesim::{EntityId, Team};

/// Both decks; the first four start in hand (unshuffled).
const DECK: [&str; 8] = ["Musketeer", "Cannon", "Knight", "Archer", "Giant", "Valkyrie", "HogRider", "Fireball"];
const HERO_AT: (i32, i32) = (9000, 10000);
const RED_CANNON_AT: (i32, i32) = (9000, 18000);
const BLUE_CANNON_AT: (i32, i32) = (9000, 14000);

/// The measured frames, from the unit's first frame: the deploy end and the first lifetime hp loss.
const TURRET_CLOCK: (u32, u32) = (20, 21);
const PLAYED_CLOCK: (u32, u32) = (19, 20);
/// The first frame an enemy may target the turret (client 15.535.29), and a played building.
const TURRET_FIRST_LOOK: u32 = 7;
const PLAYED_FIRST_LOOK: u32 = 0;

const OLD_UPDATE: AbilityUnitFirstUpdate = AbilityUnitFirstUpdate::CreationTick;
const NEW_UPDATE: AbilityUnitFirstUpdate = AbilityUnitFirstUpdate::NextTick;
const OLD_DELAY: SpawnedUnitAcquireDelay = SpawnedUnitAcquireDelay::Client8thFrame;
const NEW_DELAY: SpawnedUnitAcquireDelay = SpawnedUnitAcquireDelay::Client8thFrameActionBuildings;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn with_arms(update: AbilityUnitFirstUpdate, delay: SpawnedUnitAcquireDelay) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.ability_unit_first_update = update;
    cfg.calib.spawned_unit_acquire_delay = delay;
    cfg
}

/// A battle of DECK against DECK, Blue's Musketeer entry its hero form, at the end of the opening lockout, both sides
/// at 10 elixir, Blue's princess towers down; the idle red Cannon put down first, and past its deploy, when
/// `red_cannon`.
fn battle(cfg: BattleConfig, red_cannon: bool) -> BattleState {
    let mut cfg = cfg;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    if red_cannon {
        s.spawn_unit(Team::Red, "Cannon", at(RED_CANNON_AT), None).expect("the red Cannon");
        let deployed = |s: &BattleState| find_live(s, Team::Red, "Cannon").first().is_some_and(|e| !e.deploying);
        run_until(&mut s, 40, deployed);
        assert!(deployed(&s), "the red Cannon is still deploying");
    }
    s
}

/// The red Cannon of a targeting scene.
fn red_cannon(s: &BattleState) -> EntityId {
    let c = find_live(s, Team::Red, "Cannon");
    assert_eq!(c.len(), 1, "the scene has one red Cannon");
    c[0].id
}

/// One unit watched from its first frame: per frame, its deploy timer, its hp and whether the watcher targets it.
struct Watched {
    deploy_ms: Vec<i32>,
    hp: Vec<i32>,
    looked: Vec<bool>,
}

impl Watched {
    /// The first frame whose deploy timer reads 0, from the first frame.
    fn deploy_end(&self) -> Option<u32> {
        self.deploy_ms.iter().position(|ms| *ms == 0).map(|k| k as u32)
    }

    /// The first frame whose hp is below the frame before's.
    fn first_loss(&self) -> Option<u32> {
        self.hp.windows(2).position(|w| w[1] < w[0]).map(|k| k as u32 + 1)
    }

    fn first_look(&self) -> Option<u32> {
        self.looked.iter().position(|l| *l).map(|k| k as u32)
    }
}

/// Watch `id` from the current frame (its first) for `frames` frames, the watcher `looker` (if any) reading its target.
fn watch(s: &mut BattleState, id: EntityId, looker: Option<EntityId>, frames: u32) -> Watched {
    let mut w = Watched { deploy_ms: Vec::new(), hp: Vec::new(), looked: Vec::new() };
    for k in 0..frames {
        if k > 0 {
            s.tick();
        }
        let e = s.entity(id).unwrap_or_else(|| panic!("the watched unit is gone on frame {k}"));
        w.deploy_ms.push(e.deploy_ms);
        w.hp.push(e.hp);
        w.looked.push(looker.is_some_and(|l| s.entity(l).and_then(|e| e.target) == Some(id)));
    }
    w
}

/// The hero scene under `cfg`: her turret, watched from its first frame c (and by the red Cannon when there is one).
fn turret_scene(cfg: BattleConfig, red: bool) -> Watched {
    let mut s = battle(cfg, red);
    s.deploy(Team::Blue, "Musketeer", at(HERO_AT)).expect("the play");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Musketeer_hero");
    assert_eq!(hero.len(), 1, "the play put the hero form down");
    let hid = hero[0].id;
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let looker = red.then(|| red_cannon(&s));
    if let Some(l) = looker {
        assert!(s.entity(l).is_some_and(|e| e.target.is_none()), "the red Cannon holds a target before the press: it reaches her");
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..10 {
        s.tick();
        if !find_live(&s, Team::Blue, "MusketeerTurret").is_empty() {
            break;
        }
        if let Some(l) = looker {
            assert_eq!(s.entity(l).unwrap().target, None, "the red Cannon took a target before the turret appeared");
        }
    }
    let t = find_live(&s, Team::Blue, "MusketeerTurret");
    assert_eq!(t.len(), 1, "no turret 10 ticks after the press");
    let id = t[0].id;
    assert_eq!(t[0].pos, s.entity(hid).unwrap().pos.add(Vec2::new(0, 2500 * K)), "the scene drifted: the turret is not 2500 ahead of her");
    watch(&mut s, id, looker, 30)
}

/// A blue Cannon played on BLUE_CANNON_AT under `cfg`, watched from its first frame F (by the red Cannon when there is
/// one).
fn played_cannon_scene(cfg: BattleConfig, red: bool) -> Watched {
    let mut s = battle(cfg, red);
    let looker = red.then(|| red_cannon(&s));
    s.deploy(Team::Blue, "Cannon", at(BLUE_CANNON_AT)).expect("the play");
    s.tick();
    let c: Vec<EntityId> = find_live(&s, Team::Blue, "Cannon").iter().map(|e| e.id).collect();
    assert_eq!(c.len(), 1, "the play put the Cannon down");
    watch(&mut s, c[0], looker, 30)
}

#[test]
fn the_turret_takes_no_update_on_its_creation_tick_under_client_next_tick() {
    for delay in [OLD_DELAY, NEW_DELAY] {
        let new = turret_scene(with_arms(NEW_UPDATE, delay), false);
        assert_eq!((new.deploy_end(), new.first_loss()), (Some(TURRET_CLOCK.0), Some(TURRET_CLOCK.1)), "client_next_tick ({delay:?}): the turret's deploy end and first lifetime loss, from c");
        // The loss is the drain, one step, not a hit.
        let step = new.hp[TURRET_CLOCK.1 as usize - 1] - new.hp[TURRET_CLOCK.1 as usize];
        assert!((1..=10).contains(&step), "client_next_tick: the first loss is {step}, not one drain step");
        // NOT VACUOUS: the old arm gives the played building's clock.
        let old = turret_scene(with_arms(OLD_UPDATE, delay), false);
        assert_eq!((old.deploy_end(), old.first_loss()), (Some(PLAYED_CLOCK.0), Some(PLAYED_CLOCK.1)), "creation_tick ({delay:?}): the turret's deploy end and first lifetime loss, from c");
    }
    // A played Cannon keeps its clock under both arms.
    for update in [OLD_UPDATE, NEW_UPDATE] {
        let cannon = played_cannon_scene(with_arms(update, OLD_DELAY), false);
        assert_eq!((cannon.deploy_end(), cannon.first_loss()), (Some(PLAYED_CLOCK.0), Some(PLAYED_CLOCK.1)), "{update:?}: a played Cannon's deploy end and first lifetime loss, from F");
    }
}

#[test]
fn no_enemy_targets_the_turret_before_c7_under_client_8th_frame_action_buildings() {
    for update in [OLD_UPDATE, NEW_UPDATE] {
        let new = turret_scene(with_arms(update, NEW_DELAY), true);
        assert_eq!(new.first_look(), Some(TURRET_FIRST_LOOK), "client_8th_frame_action_buildings ({update:?}): the red Cannon's first look at the turret, from c");
        // NOT VACUOUS: under client_8th_frame the same Cannon takes it on its first frame, so the turret is in reach at
        // once and nothing but the delay holds the Cannon off.
        let old = turret_scene(with_arms(update, OLD_DELAY), true);
        assert_eq!(old.first_look(), Some(0), "client_8th_frame ({update:?}): the red Cannon's first look at the turret, from c");
    }
    // A played Cannon is targeted on its first frame under both arms.
    for delay in [OLD_DELAY, NEW_DELAY] {
        let cannon = played_cannon_scene(with_arms(OLD_UPDATE, delay), true);
        assert_eq!(cannon.first_look(), Some(PLAYED_FIRST_LOOK), "{delay:?}: the red Cannon's first look at a played Cannon, from F");
    }
}

/// The hero scene under `blow`, a red Knight put down 1500 ahead of the turret's point at the press: per frame from
/// the turret's first (c), the Knight's hp.
fn blow_scene(blow: DeployProjectile) -> Vec<i32> {
    let mut cfg = with_arms(OLD_UPDATE, OLD_DELAY);
    cfg.calib.deploy_projectile = blow;
    let mut s = battle(cfg, false);
    s.deploy(Team::Blue, "Musketeer", at(HERO_AT)).expect("the play");
    s.tick();
    let hid = find_live(&s, Team::Blue, "Musketeer_hero")[0].id;
    run_until(&mut s, 40, |s| s.entity(hid).is_some_and(|e| e.deploy_ms == 0));
    let turret_at = s.entity(hid).unwrap().pos.add(Vec2::new(0, 2500 * K));
    s.press_ability_button(Team::Blue, 0).expect("the press");
    s.spawn_unit(Team::Red, "Knight", turret_at.add(Vec2::new(0, 1500 * K)), None).expect("the red Knight");
    for _ in 0..10 {
        s.tick();
        if !find_live(&s, Team::Blue, "MusketeerTurret").is_empty() {
            break;
        }
    }
    assert_eq!(find_live(&s, Team::Blue, "MusketeerTurret").len(), 1, "no turret 10 ticks after the press");
    let knight = find_live(&s, Team::Red, "Knight");
    assert_eq!(knight.len(), 1, "the red Knight is not on the board when the turret appears");
    let kid = knight[0].id;
    let mut hp = Vec::new();
    for k in 0..12 {
        if k > 0 {
            s.tick();
        }
        hp.push(s.entity(kid).map_or(0, |e| e.hp));
    }
    hp
}

/// The frame (from c) on which the Knight first loses hp, and how much.
fn first_hit(hp: &[i32]) -> Option<(u32, i32)> {
    hp.windows(2).position(|w| w[1] < w[0]).map(|k| (k as u32 + 1, hp[k] - hp[k + 1]))
}

#[test]
fn the_turrets_blow_lands_on_c2_under_client_on_landing_action_at_2() {
    let old = first_hit(&blow_scene(DeployProjectile::ClientOnLanding)).expect("client_on_landing: the Knight takes no blow");
    let new = first_hit(&blow_scene(DeployProjectile::ClientOnLandingActionAt2)).expect("the action arm: the Knight takes no blow");
    assert_eq!(new.0, 2, "client_on_landing_action_at_2: the blow lands on c + {} (loss {})", new.0, new.1);
    // NOT VACUOUS: the old arm lands the same blow on c + 6.
    assert_eq!(old.0, 6, "client_on_landing: the blow lands on c + {} (loss {})", old.0, old.1);
    assert_eq!(new.1, old.1, "the two arms' blows differ in damage");
}

#[test]
fn both_keys_ship_their_old_arms() {
    let c = Calib::shipped();
    assert_eq!(c.ability_unit_first_update, OLD_UPDATE, "spawner.ABILITY_UNIT_FIRST_UPDATE ships creation_tick");
    assert_eq!(c.spawned_unit_acquire_delay, OLD_DELAY, "targeting.SPAWNED_UNIT_ACQUIRE_DELAY ships client_8th_frame");
    assert_eq!(config().calib.ability_unit_first_update, OLD_UPDATE);
    assert_eq!(c.deploy_projectile, DeployProjectile::ClientOnLanding, "combat.DEPLOY_PROJECTILE ships client_on_landing");
}
