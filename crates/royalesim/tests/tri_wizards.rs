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
//!   - nothing of the card before C + 5;
//!   - no unit waits for its 8th frame to be targeted: the enemy Knight (played at (9500, 18499) 117 ticks before)
//!     takes the TriWizard on C + 6, one frame after its first, and the Ice Wizard on C + 8, one frame after its own.
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
//!   5. a battle saved on any frame from C to C + 10 resumes as the same battle: among them the frame the TriWizard's
//!      entry has fired and the wizards' have not (C + 5), and the one frame the two wizards wait in the spawn queue
//!      (C + 6); a save that lost either fails the load's own check. The area stands on C .. C + 8, nine frames of its
//!      500 ms;
//!   6. an idle enemy Cannon that reaches one unit alone first targets the TriWizard on C + 6, its next frame, and each
//!      wizard on C + 7, its first (a played troop's frame); none waits for its 8th frame, as a Graveyard's Skeleton
//!      does. In the client's own scene the Knight takes the TriWizard on C + 6, as the client's did, and the Ice
//!      Wizard on C + 7: A FRAME BEFORE THE CLIENT'S C + 8, a gap this file pins rather than hides (a wizard is created in
//!      the Spawn phase, before the Target phase of its first tick).
//!
//! The owner's fast lane for new cards: no ledger arm (one behaviour per card, a plain test that the card puts down
//! what the client puts down). One plant, for the rule an enemy reads, compiled in with `--cfg clash_plant="..."`
//! (docs/contributing.md):
//!   * `deploy_spawn_area_acquire_delayed` -- the three units carry the Graveyard's acquire delay (state.rs
//!     `phase_projectile`): (6) goes red, each unit first targeted 7 frames after its first.
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpawnOffset, SpawnVia, SpellShape, UnitRef};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};
use std::collections::BTreeMap;

/// The level of the client's scene.
const LEVEL: i32 = 11;
/// The client's tap, side 0.
const TAP: (i32, i32) = (9500, 11500);

/// An idle Red Cannon for each unit (native), 6400 straight across the river from the point the unit first stands on:
/// inside a Cannon's reach of a 500-radius unit (5500 + 600 + 500 = 6600) and outside it for the other two units
/// (6871 and more), whose points are 2500 apart. Out of every crown tower's range.
const LOOKERS: [(&str, (i32, i32)); 3] = [("TriWizards", (9500, 17900)), ("ElectroWizard", (7000, 17900)), ("IceWizard", (12000, 17900))];

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

/// The area and the queue on frame C + f: the area's `fired` bits while it stands (None once it has ended), and the
/// cards of the units waiting in the spawn queue.
fn area_and_queue(s: &BattleState) -> (Option<u32>, Vec<String>) {
    let fired = s.spells().iter().find_map(|sp| match sp.motion {
        SpellMotion::Scheduled { fired, .. } => Some(fired),
        _ => None,
    });
    let queued = s.pending_spawns().iter().map(|(_, card, _)| s.cards().get(*card).name.clone()).collect();
    (fired, queued)
}

#[test]
fn a_battle_saved_on_any_frame_of_the_area_resumes_as_the_same_battle() {
    let mut s = BattleState::new(7, config());
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "TriWizards", at(TAP), Some(LEVEL)).expect("the play is taken");
    let c = s.tick_count() + 1;
    let mut seen: Vec<(u32, Option<u32>, Vec<String>)> = Vec::new();
    for _ in 0..11 {
        s.tick();
        let f = s.tick_count() - c;
        let (fired, queued) = area_and_queue(&s);
        // Saved on this frame: the resumed battle and the original go on together.
        let mut original = s.clone();
        let mut resumed = BattleState::load(&s.save()).unwrap_or_else(|e| panic!("the save on C + {f} loads: {e}"));
        for t in 0..12 {
            assert_eq!(resumed.state_hash(), original.state_hash(), "saved on C + {f}: the resumed battle parted {t} ticks after the save");
            original.tick();
            resumed.tick();
        }
        // A save that lost the area's released entries, or the wizards waiting in the queue, fails the load's own check.
        if fired.is_some_and(|b| b != 0) {
            let lost = edit_is_hashed(&s, |v| {
                let i = v["spells"].as_array().expect("the spells").iter().position(|sp| sp["motion"].get("Scheduled").is_some()).expect("the saved area");
                v["spells"][i]["motion"]["Scheduled"]["fired"] = serde_json::json!(0);
            });
            assert!(lost, "C + {f}: the area's released entries are not hashed");
        }
        if !queued.is_empty() {
            assert!(edit_is_hashed(&s, |v| v["spawn_queue"] = serde_json::json!([])), "C + {f}: the queued wizards are not hashed");
        }
        seen.push((f, fired, queued));
    }
    let q = |v: &[&str]| v.iter().map(|n| n.to_string()).collect::<Vec<_>>();
    assert_eq!(
        seen,
        vec![
            (0, Some(0), q(&[])),
            (1, Some(0), q(&[])),
            (2, Some(0), q(&[])),
            (3, Some(0), q(&[])),
            (4, Some(0), q(&[])),
            (5, Some(0b001), q(&[])),
            (6, Some(0b111), q(&["ElectroWizard", "IceWizard"])),
            (7, Some(0b111), q(&[])),
            (8, Some(0b111), q(&[])),
            (9, None, q(&[])),
            (10, None, q(&[])),
        ],
        "the saves' frames: the area on the board from C to C + 8 (nine frames of its 500 ms), the TriWizard's entry \
         released on C + 5, the wizards' on C + 6 and waiting in the queue on that frame alone"
    );
}

// ---------------------------------------------------------------------------
// 6. when an enemy first targets each unit

/// For each of the three units, the unit's first frame and the frame an idle Red Cannon that reaches it alone
/// (`LOOKERS`) first targets it, both from C. Each Cannon's target is checked on every frame to be its own unit or none.
fn first_targeted(cfg: BattleConfig) -> BTreeMap<String, (u32, u32)> {
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let lookers: Vec<(&str, EntityId)> = LOOKERS
        .iter()
        .map(|(unit, p)| (*unit, s.scenario_spawn_now(Team::Red, "Cannon", at(*p), None).unwrap_or_else(|e| panic!("scene: the Cannon for the {unit}: {e:?}"))))
        .collect();
    let before: Vec<_> = s.entities().map(|e| e.id).collect();
    s.spawn_unit(Team::Blue, "TriWizards", at(TAP), Some(LEVEL)).expect("the play is taken");
    let c = s.tick_count() + 1;
    let mut first: BTreeMap<String, u32> = BTreeMap::new();
    let mut taken: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for _ in 0..30 {
        s.tick();
        let f = s.tick_count() - c;
        for e in s.entities().filter(|e| e.team == Team::Blue && !before.contains(&e.id)) {
            first.entry(e.card.to_string()).or_insert(f);
        }
        for (unit, cannon) in &lookers {
            let Some(t) = s.entity(*cannon).expect("the Cannon stands").target else { continue };
            let card = s.entity(t).map(|e| e.card.to_string()).unwrap_or_default();
            assert_eq!(card, *unit, "scene: the Cannon for the {unit} took the {card} on C + {f}");
            taken.entry(card.clone()).or_insert((first[&card], f));
        }
    }
    taken
}

/// Plant: deploy_spawn_area_acquire_delayed.
#[test]
fn no_unit_of_the_area_waits_for_its_eighth_frame() {
    let got = first_targeted(config());
    let want: BTreeMap<String, (u32, u32)> = [("TriWizards", (5, 6)), ("ElectroWizard", (7, 7)), ("IceWizard", (7, 7))].into_iter().map(|(n, v)| (n.to_string(), v)).collect();
    assert_eq!(
        got, want,
        "(first frame, first frame an idle enemy Cannon targets it), from C: the TriWizard on its next frame, each wizard on its first; \
         a death spawn's or a Graveyard Skeleton's delay would put each one 7 frames after its first"
    );
}

/// The client's scene, sweep-TriWizards (client 15.535.29): a Red Knight played at (9500, 18499) on tick 100 walks the
/// right lane; the Tri Wizards are played at the tap on tick 217, so C is tick 218. The Knight's target on each frame
/// from C + 4, as the card name of the unit it targets.
fn the_knights_targets() -> Vec<(u32, String)> {
    let mut s = BattleState::new(15, config());
    while s.tick_count() < 100 {
        s.tick();
    }
    let before: Vec<_> = s.entities().map(|e| e.id).collect();
    s.spawn_unit(Team::Red, "Knight", at((9500, 18499)), Some(LEVEL)).expect("the Knight is taken");
    s.tick();
    let knight = s.entities().find(|e| !before.contains(&e.id)).map(|e| e.id).expect("the Knight stands");
    while s.tick_count() < 217 {
        s.tick();
    }
    s.spawn_unit(Team::Blue, "TriWizards", at(TAP), Some(LEVEL)).expect("the play is taken");
    let c = s.tick_count() + 1;
    let mut out = Vec::new();
    for _ in 0..10 {
        s.tick();
        let f = s.tick_count() - c;
        let k = s.entity(knight).expect("the Knight stands");
        if f >= 4 {
            out.push((f, k.target.and_then(|t| s.entity(t)).map(|e| e.card.to_string()).unwrap_or_default()));
        }
    }
    out
}

/// Plant: deploy_spawn_area_acquire_delayed.
#[test]
fn in_the_clients_scene_the_knight_takes_the_triwizard_on_c_plus_6_and_the_ice_wizard_on_c_plus_7() {
    let got = the_knights_targets();
    let want: Vec<(u32, String)> = [(4, "PrincessTower"), (5, "PrincessTower"), (6, "TriWizards"), (7, "IceWizard"), (8, "IceWizard"), (9, "IceWizard")].into_iter().map(|(f, n)| (f, n.to_string())).collect();
    assert_eq!(
        got, want,
        "the Knight's target from C + 4: the TriWizard on C + 6, as the client's Knight; the Ice Wizard on C + 7, a frame before the client's C + 8"
    );
}
