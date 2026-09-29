//! THE TRI WIZARDS PUT DOWN THREE UNITS (card.rs `CardDef::deploy_spawn_area`, `convert_deploy_spawn_area`,
//! `SpawnVia`; spell.rs `step_spells`, the Scheduled arm; state.rs `enqueue_with`, `phase_projectile`, `phase_spawn`).
//!
//! THE ROWS (the 15.535.29 tables): the card's AreaEffectObject TriWizardSpawn (LifeDuration 500, no hit) puts its own
//! SpawnCharacter, the TriWizard, down every SpawnInterval 300 with SpawnTime 100, and its OnStartingAction group runs
//! ElectroWizardAOE and IceWizardAOE at SubActionsDelay 300 and 300: each an ActionSpawnToLocation, RelativeX +5 and -5,
//! of an area (ElectroWizardZap, IceWizardCold) whose own OnStartingAction puts its wizard down. Those two areas are the
//! Electro Wizard's and the Ice Wizard's own cards' areas, and they deal damage where the wizard appears.
//!
//! WHAT THE CLIENT PUTS DOWN, client 15.535.29 scenario sweep-TriWizards (side 0 taps (9500, 11500) at level 11; C the
//! play's cast tick, the frame a played troop would first stand on):
//!   - the TriWizard, 755 hp, first on C + 5, leaving its deploy on C + 7;
//!   - the Electro Wizard, 714 hp, at (7000, 11500), and the Ice Wizard, 688 hp, at (12000, 11500), both first on C + 7,
//!     leaving their deploy on C + 27;
//!   - nothing of the card before C + 5.
//!
//! The TriWizard stood at (9500, 11527), (0, +27) from the tap, an offset no column of the area names; the engine stands
//! it on the play's point.
//!
//! WHAT IS PINNED:
//!   1. the loader reads TriWizardSpawn as a scheduled area of three entries (the TriWizard at 200 ms, deploying 100;
//!      the Electro Wizard and the Ice Wizard at 300 ms through their cards' own areas, RelativeX +5 and -5), and
//!      `unit_refs` names all three;
//!   2. a side-0 play at (9500, 11500) puts down exactly what the client put down, on the client's ticks and points,
//!      at the client's hitpoints; a side-1 play is its rotation;
//!   3. the Electro Wizard's own area lands where it appears, on its first frame: a Red Cannon 2236 from it loses 192
//!      (ElectroWizardZap's Damage 75 at level 11, the Electro Wizard card's measured zap) on C + 7 and not before;
//!   4. not vacuous: the same table without `deploy_spawn_area` puts the TriWizard down alone, on C, where 2 reads
//!      nothing before C + 5;
//!   5. a battle saved while the area is still on the board resumes as the same battle.
//!
//! No plant: the owner's fast lane for new cards (one behaviour per card, a plain test that the card puts down what the
//! client puts down).
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpawnOffset, SpawnVia, SpellShape, UnitRef};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;
use std::collections::BTreeMap;

/// The level of the client's scene.
const LEVEL: i32 = 11;
/// The client's tap, side 0.
const TAP: (i32, i32) = (9500, 11500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The three units a play puts down, by card name: (first frame, first point in native units, max hp, the first frame
/// it no longer deploys), each frame counted from the play's cast tick C.
type Seen = BTreeMap<String, (u32, (i32, i32), i32, Option<u32>)>;

/// Play TriWizards for `team` at `tap` (native) at level 11 in a battle on `cfg`, run 40 ticks, and report what of the
/// card came on the board (every unit of the play's side but the crown towers).
fn play(cfg: BattleConfig, team: Team, tap: (i32, i32)) -> Seen {
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let before: Vec<_> = s.entities().map(|e| e.id).collect();
    s.spawn_unit(team, "TriWizards", at(tap), Some(LEVEL)).expect("the play is taken");
    let c = s.tick_count() + 1;
    let mut seen: Seen = BTreeMap::new();
    for _ in 0..40 {
        s.tick();
        let f = s.tick_count() - c;
        for e in s.entities().filter(|e| e.team == team && !before.contains(&e.id)) {
            let row = seen.entry(e.card.to_string()).or_insert((f, (e.pos.x / K, e.pos.y / K), e.max_hp, None));
            if !e.deploying && row.3.is_none() {
                row.3 = Some(f);
            }
        }
    }
    seen
}

/// One expected unit: (card name, first frame, first point, max hp, first frame out of deploy).
type Want<'a> = (&'a str, u32, (i32, i32), i32, u32);

fn want(rows: &[Want]) -> Seen {
    rows.iter().map(|(n, f, p, hp, d)| (n.to_string(), (*f, *p, *hp, Some(*d)))).collect()
}

// ---------------------------------------------------------------------------
// 1. the loader

#[test]
fn the_loader_reads_the_area_as_three_entries() {
    let db = cards();
    let idx = |n: &str| db.index(n).unwrap_or_else(|| panic!("{n} loads"));
    let tri = idx("TriWizards");
    let c = db.get(tri);
    assert_eq!(c.unit_name, "TriWizard", "the card's own unit is the area's SpawnCharacter");
    let d = c.deploy_spawn_area.as_ref().expect("TriWizards carries its deploy spawn area");
    let SpellShape::ScheduledArea { life_ms, schedule } = &d.shape else { panic!("TriWizardSpawn: {:?}", d.shape) };
    assert_eq!(*life_ms, 500, "TriWizardSpawn's LifeDuration");
    let got: Vec<(SpawnVia, u16, i32, Option<i32>, SpawnOffset)> = schedule.iter().map(|e| (e.via, e.unit, e.delay_ms, e.deploy_time_ms, e.offset)).collect();
    assert_eq!(
        got,
        vec![
            (SpawnVia::OwnSpawn, tri, 200, Some(100), SpawnOffset::Relative { x: 0, y: 0 }),
            (SpawnVia::DeployArea, idx("ElectroWizard"), 300, None, SpawnOffset::Relative { x: 5, y: 0 }),
            (SpawnVia::DeployArea, idx("IceWizard"), 300, None, SpawnOffset::Relative { x: -5, y: 0 }),
        ],
        "the TriWizard at SpawnInterval 300 - SpawnTime 100, then the two wizards' areas"
    );
    for w in ["ElectroWizard", "IceWizard"] {
        assert!(db.get(idx(w)).deploy_area_effect.is_some(), "the {w} card deploys through its own area");
    }
    assert_eq!(
        db.unit_refs(tri),
        vec![(UnitRef::DeploySpawn(0), tri, None), (UnitRef::DeploySpawn(1), idx("ElectroWizard"), None), (UnitRef::DeploySpawn(2), idx("IceWizard"), None)],
        "unit_refs names every entry"
    );
    assert!(c.spell.is_none() && c.deploy_area_effect.is_none() && c.formation.second_summon.is_none(), "the area is the card's one spell object and its whole deploy");
}

// ---------------------------------------------------------------------------
// 2. what the play puts down

#[test]
fn a_play_puts_down_what_the_client_put_down() {
    let got = play(config(), Team::Blue, TAP);
    assert_eq!(
        got,
        want(&[
            ("TriWizards", 5, TAP, 755, 7),
            ("ElectroWizard", 7, (7000, 11500), 714, 27),
            ("IceWizard", 7, (12000, 11500), 688, 27),
        ]),
        "(first frame from C, first point, max hp, first frame out of deploy) of every unit the play put down"
    );
}

#[test]
fn a_side_1_play_is_the_rotation() {
    let got = play(config(), Team::Red, (18000 - TAP.0, 32000 - TAP.1));
    assert_eq!(
        got,
        want(&[
            ("TriWizards", 5, (8500, 20500), 755, 7),
            ("ElectroWizard", 7, (11000, 20500), 714, 27),
            ("IceWizard", 7, (6000, 20500), 688, 27),
        ]),
        "side 1: each point turned 180 degrees about the arena's centre"
    );
}

// ---------------------------------------------------------------------------
// 3. the wizard's own area

/// The Red Cannon's hp on frames C .. C + 8, with the play or without it (the Cannon's own lifetime drain is in both).
fn cannon_hp(played: bool) -> Vec<i32> {
    let mut s = BattleState::new(7, config());
    past_deploy_lockout(&mut s);
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at((8000, 13500)), None).expect("scene: the Red Cannon");
    if played {
        s.spawn_unit(Team::Blue, "TriWizards", at(TAP), Some(LEVEL)).expect("the play is taken");
    }
    (0..9)
        .map(|_| {
            s.tick();
            s.entity(cannon).expect("the Cannon stands").hp
        })
        .collect()
}

#[test]
fn the_electro_wizards_area_lands_where_it_appears() {
    let (with, without) = (cannon_hp(true), cannon_hp(false));
    let lost: Vec<i32> = with.iter().zip(&without).map(|(w, o)| o - w).collect();
    assert_eq!(lost, vec![0, 0, 0, 0, 0, 0, 0, 192, 192], "what the play took off the Red Cannon, frame by frame from C: the zap's 192 on C + 7");
}

// ---------------------------------------------------------------------------
// 4. not vacuous

#[test]
fn without_its_area_the_card_puts_the_triwizard_down_alone() {
    let text = std::fs::read_to_string(format!("{}/../../data/derived/cards.json", env!("CARGO_MANIFEST_DIR"))).expect("cards.json");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("cards.json is JSON");
    let tri = v["cards"].as_array_mut().expect("cards").iter_mut().find(|c| c["name"] == "TriWizards").expect("the TriWizards row");
    assert!(tri.as_object_mut().expect("a row").remove("deploy_spawn_area").is_some(), "the row carries deploy_spawn_area");
    let db = CardDb::from_json_str(&serde_json::to_string(&v).unwrap(), CardSource::DerivedJson).expect("the table loads without it");
    let got = play(BattleConfig::with_cards(db), Team::Blue, TAP);
    // A troop played at C first stands on C and leaves its deploy on C + 19 (DeployTime 1000): the TriWizard alone.
    assert_eq!(got, want(&[("TriWizards", 0, TAP, 755, 19)]), "the card without its area");
}

// ---------------------------------------------------------------------------
// 5. state

#[test]
fn a_battle_saved_with_the_area_on_the_board_resumes_as_the_same_battle() {
    let mut s = BattleState::new(7, config());
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "TriWizards", at(TAP), Some(LEVEL)).expect("the play is taken");
    for _ in 0..3 {
        s.tick();
    }
    assert!(!s.spells().is_empty(), "scene: the area is on the board");
    let mut resumed = BattleState::load(&s.save()).expect("the save loads");
    for _ in 0..10 {
        s.tick();
        resumed.tick();
        assert_eq!(resumed.state_hash(), s.state_hash(), "the resumed battle parted on tick {}", s.tick_count());
    }
    assert_eq!(find_live(&s, Team::Blue, "ElectroWizard").len(), 1, "scene: the wizards came after the save");
}
