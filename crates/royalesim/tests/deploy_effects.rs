//! A CARD'S DEPLOY EFFECTS BELONG TO ITS PLAY. A UNIT'S SPAWN AREA BELONGS TO ITS CREATION.
//!
//! combat.DEPLOY_PROJECTILE = client_on_landing: a played Mega Knight lands its deploy projectile (MegaKnightAppear) on
//! its own position on the 6th tick after its first frame (combat.rs `DEPLOY_PROJECTILE_DELAY_TICKS`). Measured on client
//! 15.535.29: a Knight 560 away loses 430 at level 11. spells.DEPLOY_AREA_EFFECT (the Electro Wizard's zap) is the same
//! kind of effect. Both are what PLAYING the card does, so both fire where a play's units are created (state.rs
//! `phase_spawn`): a play from the hand, and `spawn_unit`, which queues the card as a play does (the replay harness
//! plays every deploy through it).
//!
//! A unit the scenario setup puts down (`scenario_spawn_now`, `scenario_spawn_batch`, the Python `reset(spawns)`)
//! stands as if it had been played earlier. It lands no blow and casts no zap. The blow used to fire from `spawn_now`,
//! which every creation passes, so a setup Mega Knight hit a Knight 2,400 away for 430 on tick 7 while a setup Electro
//! Wizard zapped nothing.
//!
//! spawner.SPAWN_AREA_OBJECT_SCOPE = every_row is a different thing: the Battle Healer's spawn heal belongs to her
//! CREATION, so a setup Battle Healer still heals.
//!
//! THE SCENES stand away from every crown tower's reach, so no tower hit can land on a counted tick.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test deploy_effects`):
//!   setup_spawn_lands_deploy_blow  a setup spawn lands its card's deploy projectile again:
//!                                  a_setup_mega_knight_lands_no_blow goes red.
//!   deploy_projectile_unfired      no play lands a blow (tests/test_deploy_projectile.py aims it too):
//!                                  a_played_mega_knight_lands_its_blow_on_its_sixth_tick goes red.
//!   setup_spawn_skips_spawn_area   a setup spawn puts no spawn area object down either:
//!                                  a_setup_battle_healer_still_heals goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

/// A native point, in subtiles.
fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The measured blow at level 11 (client 15.535.29): MegaKnightAppear's Damage on the card's ladder.
const BLOW: i32 = 430;

/// The Mega Knight's point, a tile centre so a play does not move it, on Blue's half and out of every crown
/// tower's reach.
fn mk_at() -> Vec2 {
    n(9500, 13500)
}

/// Whether a live spell object belongs to `card`.
fn spell_of(s: &BattleState, card: &str) -> bool {
    s.spells().iter().any(|sp| s.cards().get(sp.card).name == card)
}

/// How a played Mega Knight's units come to exist.
#[derive(Clone, Copy, Debug)]
enum Path {
    /// From Blue's hand, through `deploy`.
    Hand,
    /// Through `spawn_unit`, the path the replay harness plays every deploy through.
    SpawnUnit,
}

/// A Red Knight set up 560 left of the Mega Knight's point, then a Blue Mega Knight played there on tick `play`.
/// Returns (the Mega Knight's first frame, [(tick, hp the Knight lost on it)]) over the 12 ticks after the play.
fn played_scene(path: Path) -> (u32, u32, Vec<(u32, i32)>) {
    let mut cfg = config();
    let deck: Vec<String> = ["MegaKnight", "Knight", "Archer", "Giant", "Minions", "Valkyrie", "Musketeer", "HogRider"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let level = cfg.card_level[0];
    let mut s = BattleState::new(3, cfg);
    // The clock starts past the opening lockout, so the Knight is still where it was put when the card is played.
    let lockout = s.config().calib.deploy_lockout_ticks.max(0) as u32;
    s.scenario_set_tick(lockout);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(mk_at().x - 560 * K, mk_at().y), None).unwrap();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let play = s.tick_count();
    match path {
        Path::Hand => {
            let at = s.deploy(Team::Blue, "MegaKnight", mk_at()).unwrap();
            assert_eq!(at, mk_at(), "the play moved the Mega Knight: the scene is not the one described");
        }
        Path::SpawnUnit => s.spawn_unit(Team::Blue, "MegaKnight", mk_at(), None).unwrap(),
    }
    assert_eq!(level, 11, "the measured blow is at level 11; these battles run at {level}");
    let (mut first, mut last, mut lost) = (None, s.entity(knight).unwrap().hp, Vec::new());
    for _ in 0..12 {
        s.tick();
        if first.is_none() && !find_live(&s, Team::Blue, "MegaKnight").is_empty() {
            first = Some(s.tick_count());
        }
        let hp = s.entity(knight).expect("the Knight died").hp;
        if hp < last {
            lost.push((s.tick_count(), last - hp));
        }
        last = hp;
    }
    (play, first.expect("the Mega Knight never appeared"), lost)
}

#[test]
fn a_played_mega_knight_lands_its_blow_on_its_sixth_tick() {
    for path in [Path::Hand, Path::SpawnUnit] {
        let (play, first, lost) = played_scene(path);
        // Its first frame is the tick after the play, and the blow lands 6 ticks later (combat.rs
        // DEPLOY_PROJECTILE_DELAY_TICKS): the same ticks as when the blow was cast inside `spawn_now`.
        assert_eq!(first, play + 1, "{path:?}: the Mega Knight's first frame");
        assert_eq!(lost, vec![(first + royalesim::combat::DEPLOY_PROJECTILE_DELAY_TICKS as u32, BLOW)], "{path:?}: the Knight's losses as (tick, hp)");
    }
}

#[test]
fn a_setup_mega_knight_lands_no_blow() {
    // The scene the defect was found on: the Knight 2,400 away, inside the blow's radius, and lost 430 on tick 7.
    // `scenario_spawn_now` and `scenario_spawn_batch` (the Python `reset(spawns)`) are the two setup paths.
    let far = Vec2::new(mk_at().x - 2400 * K, mk_at().y);
    for batch in [false, true] {
        let mut s = BattleState::new(3, config());
        if batch {
            s.scenario_spawn_batch(&[(Team::Blue, "MegaKnight", mk_at(), None), (Team::Red, "Knight", far, None), (Team::Blue, "ElectroWizard", n(12500, 13500), None)]).unwrap();
        } else {
            s.scenario_spawn_now(Team::Blue, "MegaKnight", mk_at(), None).unwrap();
            s.scenario_spawn_now(Team::Red, "Knight", far, None).unwrap();
            s.scenario_spawn_now(Team::Blue, "ElectroWizard", n(12500, 13500), None).unwrap();
        }
        let knight = find_live(&s, Team::Red, "Knight")[0].id;
        let full = s.entity(knight).unwrap().max_hp;
        for _ in 0..9 {
            assert!(!spell_of(&s, "MegaKnight"), "batch {batch}, tick {}: a setup Mega Knight's deploy projectile is in flight", s.tick_count());
            assert!(!spell_of(&s, "ElectroWizard"), "batch {batch}, tick {}: a setup Electro Wizard cast its zap", s.tick_count());
            s.tick();
            assert_eq!(s.entity(knight).unwrap().hp, full, "batch {batch}, tick {}: the Knight was hit", s.tick_count());
        }
    }
}

#[test]
fn a_setup_battle_healer_still_heals() {
    // A Blue Knight at 1,000 hp and a Blue Battle Healer 2,000 away, both set up; no enemy near. Her spawn heal is
    // a property of her creation (spawner.SPAWN_AREA_OBJECT_SCOPE = every_row), so the Knight gains hp. The control
    // without her gains nothing.
    let gains = |healer: bool| -> i32 {
        let mut s = BattleState::new(3, config());
        let knight = s.scenario_spawn_now(Team::Blue, "Knight", n(9000, 9000), Some(1000)).unwrap();
        if healer {
            s.scenario_spawn_now(Team::Blue, "BattleHealer", n(11000, 9000), None).unwrap();
        }
        let mut total = 0;
        let mut last = s.entity(knight).unwrap().hp;
        for _ in 0..40 {
            s.tick();
            let hp = s.entity(knight).unwrap().hp;
            total += (hp - last).max(0);
            last = hp;
        }
        total
    };
    assert_eq!(gains(false), 0, "the control Knight gained hp with no healer");
    assert!(gains(true) > 0, "a setup Battle Healer healed nobody");
}
