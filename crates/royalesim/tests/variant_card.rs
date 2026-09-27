//! THE VARIANT CARD: the Spirit Empress (card.rs `SpellShape::Variant`, `VariantOption`; state.rs `resolve_play`).
//!
//! THE LAW, measured on client 15.535.29:
//!   - the card has two forms, each a card of its own: MergeMaiden_Mounted (6 elixir, AvailableManaTrigger 6000) and
//!     MergeMaiden_Normal (3 elixir, 3000); the table's display row is the first, and the card's own cost is its 6;
//!   - the elixir held when the play is applied picks the form: taps at 30,114 to 59,936 (units of 1/10000 elixir,
//!     single and double elixir) played the Normal form for 30,000, taps at 60,114 and 100,000 the Mounted form for
//!     60,000, with no wait (match.VARIANT_ELIXIR_MOMENT = command); whether exactly 6.000 is >= or > is unmeasured
//!     (match.VARIANT_TRIGGER_COMPARE, a guess);
//!   - the form is placed by its own rule; a Mirror copies the FORM played (7 after the Mounted form, 4 after the
//!     Normal).
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads two forms, each a registered troop card, the first the card's own cost;
//!   2. the measured elixir picks the measured form and debits its cost; the hand card cycles;
//!   3. exactly 6.000: the Mounted form under at_least, the Normal under greater_than;
//!   4. below 3.000 the play is the Normal form, refused for want of elixir;
//!   5. the verdict, the play and the hand's price read one resolution, which moves as the elixir crosses 6;
//!   6. the form's own placement judges the tap;
//!   7. a Mirror after the Empress copies the form she played;
//!   8. the Empress is never set down by name, and her forms are;
//!   9. two seats playing her at the rotated taps stay rotations of each other;
//!  10. a form resolves by the card's own name, never by a released unit's (synthetic file, the names differ);
//!  11. the loader takes the variant block in its one shape only (synthetic file).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test variant_card`):
//!   * `variant_first_option_always` -- the elixir is never read: (2) goes red.
//!   * `variant_debits_card_cost` -- the card's 6 whatever the form: (2) goes red.
//!   * `variant_trigger_strict` -- the compare key is never read: (3) goes red.
//!   * `variant_check_act_split` -- the verdict reads the hand card's cost: (5) goes red.
//!   * `variant_placement_of_hand_card` -- the hand card's placement judges the tap: (6) goes red.
//!   * `mirror_copies_root_variant` -- the hand card is recorded: (7) goes red.
//!   * `variant_form_via_units` -- a released unit of the form's name serves ahead of the card: (10) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardKind, CardSource, UnitRef};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError, Play, VariantTriggerCompare};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const CARD: &str = "MergeMaiden";
const MOUNTED: &str = "MergeMaiden_Mounted";
const NORMAL: &str = "MergeMaiden_Normal";
/// A clear point of Blue's own half.
const OWN: (i32, i32) = (9500, 9500);

fn deck() -> Vec<String> {
    [CARD, "Archer", "Giant", "Musketeer", "Mirror", "Valkyrie", "HogRider", "Minions"].iter().map(|n| n.to_string()).collect()
}

/// Past the lockout, the Empress in Blue's slot 0, Blue holding `milli` thousandths of an elixir.
fn battle_at(milli: i64, compare: VariantTriggerCompare) -> BattleState {
    let mut cfg = config();
    cfg.decks = [deck(), deck()];
    cfg.calib.variant_trigger_compare = compare;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, milli);
    assert_eq!(s.hand(Team::Blue)[0], CARD);
    s
}

fn idx(s: &BattleState, name: &str) -> u16 {
    s.cards().index(name).unwrap_or_else(|| panic!("{name} does not load"))
}

// ---------------------------------------------------------------------------
// 1. the loader

#[test]
fn the_loader_reads_two_forms_each_a_registered_card() {
    let s = BattleState::new(0, config());
    let db = s.cards();
    let i = idx(&s, CARD);
    let opts = db.get(i).variant().expect("the Empress is a variant card");
    let got: Vec<(i32, i32, &str)> = opts.iter().map(|o| (o.trigger_milli, o.precast_pending_ms, db.get(o.card).name.as_str())).collect();
    assert_eq!(got, [(6000, 1200, MOUNTED), (3000, 1200, NORMAL)]);
    for o in opts {
        let f = db.get(o.card);
        assert!(!f.summon_only && f.kind == CardKind::Troop && db.index(&f.name) == Some(o.card), "{} is a registered troop card", f.name);
    }
    assert_eq!(db.get(i).elixir, db.get(opts[0].card).elixir, "the card's cost is its first form's");
    let refs: Vec<(UnitRef, u16)> = db.unit_refs(i).into_iter().map(|(p, u, _)| (p, u)).collect();
    assert_eq!(refs, [(UnitRef::VariantForm(0), opts[0].card), (UnitRef::VariantForm(1), opts[1].card)]);
    assert!(db.check_levels(i, 11).is_ok());
}

// ---------------------------------------------------------------------------
// 2 to 5. the play

/// Plants: variant_first_option_always, variant_debits_card_cost.
#[test]
fn the_elixir_at_the_play_picks_the_form_and_its_cost() {
    // The measured taps, in thousandths (the capture's 30,114 is 3.0114 elixir, floored to 3.011 here).
    for (milli, form, cost) in [(3011, NORMAL, 3), (5993, NORMAL, 3), (6011, MOUNTED, 6), (10000, MOUNTED, 6)] {
        let mut s = battle_at(milli, VariantTriggerCompare::AtLeast);
        let (card, f) = (idx(&s, CARD), idx(&s, form));
        assert_eq!(s.resolve_play(Team::Blue, 0), Ok(Play { in_slot: card, card: f, level: 11, cost }), "at {milli}");
        assert_eq!(s.hand_costs(Team::Blue)[0], cost, "the hand's price at {milli}");
        let (before, unit) = s.elixir_raw(Team::Blue);
        s.deploy_slot(Team::Blue, 0, at(OWN)).unwrap_or_else(|e| panic!("the Empress at {milli}: {e:?}"));
        assert_eq!(before - s.elixir_raw(Team::Blue).0, cost as i64 * unit, "at {milli} the {form} debits {cost}");
        assert_eq!(s.queue_cards(Team::Blue).last(), Some(&card), "the Empress cycles, not her form");
        assert!(s.pending_spawns().iter().any(|p| p.0 == Team::Blue && p.1 == f), "the {form} is queued: {:?}", s.pending_spawns());
        run_until(&mut s, 5, |s| !find_live(s, Team::Blue, form).is_empty());
        assert_eq!(find_live(&s, Team::Blue, form).len(), 1, "the {form} stands at {milli}");
    }
}

/// Plant: variant_trigger_strict.
#[test]
fn exactly_six_elixir_is_the_mounted_form_under_at_least_and_the_normal_under_greater_than() {
    let s = battle_at(6000, VariantTriggerCompare::AtLeast);
    assert_eq!(s.resolve_play(Team::Blue, 0).map(|p| p.card), Ok(idx(&s, MOUNTED)));
    let s = battle_at(6000, VariantTriggerCompare::GreaterThan);
    assert_eq!(s.resolve_play(Team::Blue, 0).map(|p| p.card), Ok(idx(&s, NORMAL)));
    // Exactly 3.000 meets no trigger under greater_than: the last form, which 3.000 pays for.
    let mut s = battle_at(3000, VariantTriggerCompare::GreaterThan);
    assert_eq!(s.resolve_play(Team::Blue, 0).map(|p| (p.card, p.cost)), Ok((idx(&s, NORMAL), 3)));
    assert!(s.deploy_slot(Team::Blue, 0, at(OWN)).is_ok());
}

#[test]
fn below_the_last_trigger_the_play_is_the_last_form_refused_for_elixir() {
    let s = battle_at(2500, VariantTriggerCompare::AtLeast);
    assert_eq!(s.resolve_play(Team::Blue, 0).map(|p| (p.card, p.cost)), Ok((idx(&s, NORMAL), 3)));
    assert_eq!(s.check_deploy_slot(Team::Blue, 0, at(OWN)), Err(DeployError::NotEnoughElixir { have: 2, need: 3 }));
    assert_eq!(s.hand_costs(Team::Blue)[0], 3);
}

/// Plant: variant_check_act_split.
#[test]
fn the_verdict_the_play_and_the_price_read_one_resolution() {
    let mut s = battle_at(5000, VariantTriggerCompare::AtLeast);
    assert_eq!(s.hand_costs(Team::Blue)[0], 3);
    assert_eq!(s.check_deploy_slot(Team::Blue, 0, at(OWN)), Ok(()), "5 elixir pays for the Normal form");
    s.scenario_set_elixir_milli(Team::Blue, 5999);
    assert_eq!(s.hand_costs(Team::Blue)[0], 3);
    s.scenario_set_elixir_milli(Team::Blue, 6000);
    assert_eq!(s.hand_costs(Team::Blue)[0], 6, "the price moves as the elixir crosses the trigger");
    assert_eq!(s.check_deploy_slot(Team::Blue, 0, at(OWN)), Ok(()));
    assert!(s.deploy_slot(Team::Blue, 0, at(OWN)).is_ok());
    assert_eq!(s.elixir(Team::Blue), 0, "the Mounted form took all six");
}

// ---------------------------------------------------------------------------
// 6. placement

/// Plant: variant_placement_of_hand_card.
#[test]
fn the_form_is_placed_by_its_own_rule() {
    // Own half, the enemy half, the river, the own princess tower's footprint; at 4 elixir, the Normal form.
    for tap in [OWN, (9500, 20500), (9500, 16000), (3500, 7500)] {
        let mut s = battle_at(4000, VariantTriggerCompare::AtLeast);
        let mut twin = {
            let mut cfg = config();
            let d: Vec<String> = [NORMAL, "Archer", "Giant", "Musketeer", "Mirror", "Valkyrie", "HogRider", "Minions"].iter().map(|n| n.to_string()).collect();
            cfg.decks = [d.clone(), d];
            let mut t = BattleState::new(0, cfg);
            past_deploy_lockout(&mut t);
            t.scenario_set_elixir_milli(Team::Blue, 4000);
            t
        };
        assert_eq!(s.check_deploy_slot(Team::Blue, 0, at(tap)), twin.check_deploy_slot(Team::Blue, 0, at(tap)), "the verdict at {tap:?}");
        assert_eq!(s.deploy_slot(Team::Blue, 0, at(tap)), twin.deploy_slot(Team::Blue, 0, at(tap)), "where the form goes down for a tap at {tap:?}");
    }
    // The river refuses the form, as it refuses any troop.
    let s = battle_at(4000, VariantTriggerCompare::AtLeast);
    assert!(s.check_deploy_slot(Team::Blue, 0, at((9500, 16000))).is_err());
}

// ---------------------------------------------------------------------------
// 7. the Mirror

/// Plant: mirror_copies_root_variant.
#[test]
fn a_mirror_after_the_empress_copies_the_form_she_played() {
    for (milli, form, mirror_cost) in [(10000, MOUNTED, 7), (4000, NORMAL, 4)] {
        let mut s = battle_at(milli, VariantTriggerCompare::AtLeast);
        s.deploy_slot(Team::Blue, 0, at(OWN)).expect("play the Empress");
        let f = idx(&s, form);
        assert_eq!(s.mirror_target(Team::Blue), Some(f), "the record is the form, not the Empress");
        // The Mirror is fifth in the deck: it came up into slot 0.
        assert_eq!(s.hand(Team::Blue)[0], "Mirror");
        s.scenario_set_elixir_milli(Team::Blue, 10000);
        let play = s.resolve_play(Team::Blue, 0).expect("the Mirror resolves");
        assert_eq!((play.card, play.level, play.cost), (f, 12, mirror_cost), "a Mirror of the {form}");
        s.deploy_slot(Team::Blue, 0, at((5500, 10500))).expect("the Mirror of the form");
        run_until(&mut s, 5, |s| find_live(s, Team::Blue, form).len() == 2);
        assert_eq!(find_live(&s, Team::Blue, form).len(), 2, "the form and its copy");
    }
}

// ---------------------------------------------------------------------------
// 8. never by name

#[test]
fn the_empress_is_never_set_down_by_name_and_her_forms_are() {
    let mut s = battle_at(10000, VariantTriggerCompare::AtLeast);
    match s.spawn_unit(Team::Blue, CARD, at(OWN), None) {
        Err(DeployError::UnsupportedCard(n, why)) => {
            assert_eq!(n, CARD);
            assert!(why.contains(MOUNTED) && why.contains(NORMAL), "the refusal names the forms to place: {why}");
        }
        other => panic!("the Empress set down by name: {other:?}"),
    }
    assert!(matches!(s.formation_preview(Team::Blue, CARD, at(OWN)), Err(DeployError::UnsupportedCard(..))));
    assert!(matches!(s.scenario_spawn_now(Team::Blue, CARD, at(OWN), None), Err(DeployError::UnsupportedCard(..))));
    for form in [MOUNTED, NORMAL] {
        assert!(s.spawn_unit(Team::Blue, form, at(OWN), None).is_ok(), "{form} by name");
    }
    // The Mirror likewise: it has no card of its own to set down.
    assert!(matches!(s.spawn_unit(Team::Blue, "Mirror", at(OWN), None), Err(DeployError::UnsupportedCard(..))));
}

// ---------------------------------------------------------------------------
// 9. the seats

#[test]
fn two_seats_playing_the_empress_at_rotated_taps_stay_rotations() {
    // Both forms, each in a battle of its own, on the left lane and the right.
    for (milli, tap) in [(10000, (5500, 9500)), (4000, (12500, 10500))] {
        let mut cfg = symmetric_config();
        cfg.decks = [deck(), deck()];
        let mut s = BattleState::new(0, cfg);
        past_deploy_lockout(&mut s);
        s.scenario_set_elixir_milli(Team::Blue, milli);
        s.scenario_set_elixir_milli(Team::Red, milli);
        let p = at(tap);
        let q = mirror(&s, p);
        s.deploy_slot(Team::Blue, 0, p).expect("Blue plays the Empress");
        s.deploy_slot(Team::Red, 0, q).expect("Red plays the Empress");
        assert_eq!(s.mirror_target(Team::Blue), s.mirror_target(Team::Red), "both seats played the same form at {milli}");
        for _ in 0..80 {
            s.tick();
            check_mirror(&s).unwrap_or_else(|e| panic!("at {milli}: {e}"));
        }
    }
}

// ---------------------------------------------------------------------------
// 10 and 11. the loader, synthetic

/// A variant card Choice with forms Big (6) and Small (3). Small's row is a DIFFERENT character (SmallRow), and the
/// card Nest releases a `units` row named Small: so a summon-only record whose row name is the form's name exists
/// beside the card of that name, and only the card is the form.
fn choice() -> serde_json::Value {
    let troop = |name: &str, elixir: i32| {
        serde_json::json!({ "name": name, "kind": "troop", "elixir": elixir, "rarity": "Legendary", "hitpoints": 300,
            "hit_speed_ms": 1000, "range_milli": 1000, "collision_radius_milli": 500 })
    };
    let mut small = troop("Small", 3);
    small["summon_character"] = "SmallRow".into();
    let mut nest = troop("Nest", 4);
    nest["spawner"] = serde_json::json!({"character": "Small", "number": 1, "pause_time_ms": 5000});
    serde_json::json!({ "version": "test",
        "cards": [
            troop("Big", 6), small, nest,
            { "name": "Choice", "kind": "spell", "elixir": 6, "rarity": "Legendary",
              "spell": { "variant": { "options": [
                  {"trigger_milli": 6000, "precast_pending_ms": 1200, "card": "Big"},
                  {"trigger_milli": 3000, "precast_pending_ms": 1200, "card": "Small"}
              ], "use_projected_time_summon": true, "mirror_uses_root_spell": false } } }
        ],
        "units": { "Small": { "name": "Small", "rarity": "Common", "hitpoints": 80, "hit_speed_ms": 1000, "range_milli": 500,
                              "collision_radius_milli": 300 } } })
}

fn load(v: &serde_json::Value) -> CardDb {
    CardDb::from_json_str(&v.to_string(), CardSource::DerivedJson).expect("the synthetic file parses")
}

/// Plant: variant_form_via_units.
#[test]
fn a_form_resolves_by_the_cards_own_name_where_a_released_unit_shares_it() {
    let db = load(&choice());
    // The precondition: a summon-only record whose row is named Small stands beside the card Small.
    let small = db.index("Small").expect("the card Small loads");
    assert_eq!(db.get(small).unit_name, "SmallRow", "the card's row is another character");
    assert!(db.cards.iter().any(|c| c.summon_only && c.unit_name == "Small"), "Nest's unit loaded as its own record");
    let i = db.index("Choice").unwrap_or_else(|| panic!("Choice refused: {:?}", db.rejected));
    let forms: Vec<u16> = db.get(i).variant().expect("a variant card").iter().map(|o| o.card).collect();
    assert_eq!(forms, [db.index("Big").unwrap(), small], "each form is the card of its name");
}

#[test]
fn the_loader_takes_the_variant_block_in_its_one_shape() {
    let edit = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v = choice();
        f(&mut v["cards"][3]["spell"]["variant"]);
        v
    };
    for (what, v, want) in [
        ("the form chosen later", edit(&|b| b["use_projected_time_summon"] = false.into()), "UseProjectedTimeSummon"),
        ("a Mirror of the root card", edit(&|b| b["mirror_uses_root_spell"] = true.into()), "MirrorUsesRootSpell"),
        ("no options", edit(&|b| b["options"] = serde_json::json!([])), "0 options"),
        ("triggers ascending", edit(&|b| b["options"].as_array_mut().unwrap().reverse()), "strictly descending"),
        ("an option without a form", edit(&|b| b["options"][1]["card"] = serde_json::Value::Null), "SpellData"),
        ("a form that is not a card", edit(&|b| b["options"][1]["card"] = "Nobody".into()), "not a loadable card"),
        ("a form that is a spell", edit(&|b| b["options"][1]["card"] = "Choice".into()), "not a troop or building card"),
        ("a trigger that is not the form's cost", edit(&|b| b["options"][1]["trigger_milli"] = 2000.into()), "not its form"),
    ] {
        let db = load(&v);
        assert!(db.index("Choice").is_none(), "{what}: Choice loads");
        let why = db.rejected.iter().find(|(n, _)| n == "Choice").map(|(_, w)| w.clone()).unwrap_or_default();
        assert!(why.contains(want), "{what}: refused for {why:?}");
    }
    // The card's own cost must be its first form's.
    let mut v = choice();
    v["cards"][3]["elixir"] = 5.into();
    let db = load(&v);
    assert!(db.rejected.iter().any(|(n, w)| n == "Choice" && w.contains("first form costs")), "{:?}", db.rejected);
}
