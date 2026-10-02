//! THE REPLAY-PARITY HARNESS on its committed sample: the harness invariants, and a
//! floor the engine already meets.
//!
//! THE SAMPLE is `fixtures/replay/sample.json`: live capture 20260920-003751 (seat B,
//! a scripted battle of single-card deploys), trimmed to its first 1440 ticks by
//! `tools/make_replay_fixture.py ... --until-tick 1440`. The trim is that wide because
//! the walks the sample exists for are a PRINCE (tick 172: a 1.9-tile walk, the charge
//! onset at 2.5 tiles, the tower hits) and a GIANT (tick 1329, the stomp walker), and
//! no capture of that series deploys a Giant inside its first 600 ticks; the trim
//! stops before the Ram Rider at 1442, which the loader refused when the sample was
//! cut (its attached rider loads since; the sample is kept as cut). Also on the board: a Dark Prince, three cycled Skeletons behind the
//! king tower, a Battle Ram with its two death-spawned Barbarians, a Musketeer.
//!
//! WHAT IS GATED
//!   invariants   every deploy is issued on the tick before its recorded tick and the
//!                engine's units exist ON that tick (plant: replay_deploys_one_tick_
//!                late); the matching is one-to-one on both sides; the score counters
//!                are consistent (nested tolerances, parts summing to the total, the
//!                per-card rows summing to the battle); the group pairing keeps a
//!                spawner's extra wave from shifting its death spawn (synthetic); a
//!                scenario troop row is played at its RAW tap under the tile-centre snap
//!                (plant: replay_plays_the_snapped_tap); a
//!                Clone's copy is rooted as "Clone", the card the recording names every
//!                copy by (plant: replay_roots_a_copy_as_its_unit), and a Skeleton King's soul as
//!                its King (plant: replay_roots_a_soul_as_clone), and the Evo Goblin Barrel's decoy
//!                dummies as the Goblin Barrel (plant: replay_decoy_unrooted); a truth entity with
//!                no hitpoints (a visual dummy) takes no pair (plant: replay_pairs_a_dummy).
//!   the floor    the ISOLATED-WALK unit-ticks within WALK_TIGHT_NATIVE (20 native:
//!                a unit walking at a tower with full hp, bit-exact) -- measured at
//!                100 % for the Prince (92 frames, charge included),
//!                the Giant (79), the Dark Prince (119) and the Battle Ram (98, once
//!                the maker's first-step rule put its spawn at 936, not 937) -- the
//!                first three pinned at 95 %, the Battle Ram at 98 bit-exact FRAMES
//!                since combat.KAMIKAZE_DEATH put its two death-spawned Barbarians
//!                into the same root-card row (118 walk
//!                frames now; their own walk off the death point is a different
//!                mechanic and would otherwise dilute a percentage floor).
//!                WHAT THE FLOOR SEES: one tick of lag in the deploy
//!                timing (the plant: 37-120 native on every walking frame, red on all
//!                four rows), a step-law or charge-onset error of one native unit
//!                per tick after ~20 frames, a spawn point off by a subtile. WHAT IT
//!                CANNOT SEE: anything after the first contact (the rows stop at the
//!                tower), the formation layout -- the three cycled Skeletons walk 0 %
//!                within 20 and 24 % within 250 because the engine lays a 3-unit
//!                formation out differently and the maker places the group at the
//!                tap tile (the formation gap); that row is deliberately NOT
//!                pinned, so the gate never discriminates on a formation artefact.
//!                The 0 % and 24 % were read under placement.TROOP_TOWER_TAPS =
//!                closed_block, and the tap, not the layout, was most of the gap: the
//!                group is tapped on the own king's tile (own (8500, 1500)), which the
//!                shipped arm moves to own (8500, 500), the point the recording's three
//!                stand around. Since that arm ships, the sample's own Skeletons walk
//!                765 of 765 isolated ticks within 20 and within 250 (printed by
//!                the_engine_meets_the_isolated_walk_floor_on_the_sample), and the
//!                sample has no first divergence at the shipped values.
//!
//! NOT gated: the parity numbers themselves. Those are the corpus report's
//! (`cargo run --example replay_parity -- --all`), and they are expected to move.
#![allow(unexpected_cfgs)]

#[path = "../examples/replay_parity/harness.rs"]
mod harness;
mod common;

use harness::*;
use royalesim::fixed::Vec2;
use std::collections::BTreeSet;

const SAMPLE: &str = include_str!("fixtures/replay/sample.json");

fn sample() -> Fixture {
    Fixture::from_str(SAMPLE).expect("sample fixture parses")
}

/// The register is generated and gitignored (tools/mechanic_register.py); without it
/// the families are empty and nothing here reads them.
fn register() -> std::collections::BTreeMap<String, Vec<String>> {
    load_register_or_empty(&register_path()).0
}

fn play(f: &Fixture) -> Report {
    replay(f, &common::cards(), &register(), &Options::default()).expect("the sample replays")
}

#[test]
fn the_sample_is_the_documented_battle_and_playable() {
    let f = sample();
    assert_eq!(f.capture, "20260920-003751-B");
    assert_eq!(f.frame.blue_native_side, 0);
    assert_eq!(f.frame.transform, "identity");
    assert!(f.playable, "{:?}", f.unplayable_reasons);
    assert_eq!(f.last_tick(), 1440);
    let cards: BTreeSet<&str> = f.deploys.iter().filter_map(|d| d.card.as_deref()).collect();
    for want in ["Prince", "Giant", "DarkPrince", "Skeletons", "BattleRam", "Musketeer"] {
        assert!(cards.contains(want), "the sample has no {want} deploy: {cards:?}");
    }
    assert!(playability(&f, &common::cards()).is_ok());
    let r = play(&f);
    assert!(r.playable);
    assert!(r.prefix_until.is_none());
    assert_eq!(r.deploys.len(), f.deploys.len());
    assert!(r.deploys.iter().all(|d| d.result.is_ok()), "{:?}", r.deploys);
}

#[test]
fn every_deploy_is_issued_the_tick_before_and_its_units_exist_on_the_recorded_tick() {
    let f = sample();
    let r = play(&f);
    for d in &f.deploys {
        let issued = r.deploys.iter().find(|i| i.tick == d.tick && i.side == d.side && Some(&i.card) == d.card.as_ref()).unwrap_or_else(|| panic!("deploy {d:?} not issued"));
        assert_eq!(issued.issued_at + 1, d.tick, "{} at {}: spawn_unit must be called on the tick before (module doc)", issued.card, d.tick);
        if d.kind == "spell" {
            continue;
        }
        // the truth entities of this group, and their sim partners, all first seen on d.tick
        let pairs: Vec<&Pair> = r.pairs.iter().filter(|p| d.keys.contains(&p.truth_key)).collect();
        assert_eq!(pairs.len(), d.count as usize, "{} at {}: {} of {} units matched", issued.card, d.tick, pairs.len(), d.count);
        for p in pairs {
            // the recording's first FRAME of the group is at or after the spawn tick
            // (the captures miss frames; the maker recovers the spawn tick from the
            // deploy-end transition)
            assert_eq!(Some(p.truth_first_tick), d.first_seen, "truth key {} first seen at {}, deploy says {:?}", p.truth_key, p.truth_first_tick, d.first_seen);
            // The frame gap bounds the spawn only for a group the capture showed on time. A group
            // it showed LATE (tools/make_replay_fixture.py `shown_late_spawn`, whose tick_evidence
            // says "deploy-end transition; the capture shows") spawned more ticks before its
            // first frame than the gap: 3, 8 and 1 ticks with gaps of 1, 2 and 1 on the corpus.
            let shown_late = d.tick_evidence.as_deref().is_some_and(|e| e.contains("deploy-end transition; the capture shows"));
            assert!(p.truth_first_tick >= d.tick && (shown_late || p.truth_first_tick < d.tick + d.first_seen_gap.max(1)), "{d:?} vs first seen {}", p.truth_first_tick);
            assert_eq!(p.sim_first_tick, d.tick, "{} (truth key {}): the engine's unit exists from tick {}, the recording's from {}", p.root, p.truth_key, p.sim_first_tick, d.tick);
        }
    }
}

#[test]
fn the_matching_is_one_to_one_on_both_sides() {
    let r = play(&sample());
    let truth_keys: BTreeSet<i64> = r.pairs.iter().map(|p| p.truth_key).collect();
    assert_eq!(truth_keys.len(), r.pairs.len(), "a truth entity is paired twice");
    let sim_ids: BTreeSet<(u32, u32)> = r.pairs.iter().map(|p| (p.sim_index, p.sim_generation)).collect();
    assert_eq!(sim_ids.len(), r.pairs.len(), "a sim entity is paired twice");
    assert_eq!(r.pairs.len() + r.unmatched_truth.len(), r.truth_entities);
    assert_eq!(r.pairs.len() + r.unmatched_sim.len(), r.sim_entities);
    for (key, _) in &r.unmatched_truth {
        assert!(!truth_keys.contains(key), "truth key {key} both paired and unmatched");
    }
    // every pair is the same side and root card on both ends
    for p in &r.pairs {
        assert!(!p.root.is_empty());
        assert!(p.root_how != "unrooted", "{p:?}");
    }
    // the sample: the six towers, the six deploys' units and the two Barbarians
    assert_eq!(r.truth_entities, 19 - 3, "19 entities in the capture, the Ram Rider's two and its rider's Skeletons trimmed");
    assert_eq!(r.unmatched_truth.len(), 0, "{:?}", r.unmatched_truth);
    assert_eq!(r.unmatched_sim.len(), 0, "{:?}", r.unmatched_sim);
    let barbarians: Vec<&Pair> = r.pairs.iter().filter(|p| p.sim_card == "Barbarian").collect();
    assert_eq!(barbarians.len(), 2);
    assert!(barbarians.iter().all(|p| p.root == "BattleRam" && p.root_how == "death-spawn"), "{barbarians:?}");
    // the Barbarians pair inside PAIR_WINDOW_TICKS. This used to assert they appeared LATER in
    // the engine than in the recording, because its death spawns materialised in the next
    // tick's Spawn phase; under spawner.RELEASE_TIMING = end_of_event_phase (2026-09-24) they
    // appear on the recording's own tick, 1153 on both. The exact tick is emergent -- it moves
    // with anything that moves the ram's death -- so it is not pinned here; the window is.
    for p in &barbarians {
        assert!(p.sim_first_tick.abs_diff(p.truth_first_tick) <= PAIR_WINDOW_TICKS, "{p:?}");
    }
}

#[test]
fn group_pairing_prefers_the_same_size_inside_the_window_and_leaves_an_extra_wave_unmatched() {
    // one (side, root): a Tombstone's truth waves of 2 at 100 and 170 and its death
    // spawn of 4 at 200; the engine's waves 2 ticks late, an EXTRA wave at 205 (its
    // Tombstone lived longer) and its death spawn of 4 at 240
    let p = |x: i32, y: i32| Vec2::new(x * 18, y * 18);
    let truth: Vec<(u32, Vec<Option<Vec2>>)> = vec![
        (100, vec![Some(p(1000, 1000)), Some(p(1400, 1000))]),
        (170, vec![Some(p(1000, 1000)), Some(p(1400, 1000))]),
        (200, vec![Some(p(800, 800)), Some(p(1200, 800)), Some(p(800, 1200)), Some(p(1200, 1200))]),
    ];
    let sim: Vec<(u32, Vec<Vec2>)> = vec![
        (102, vec![p(1400, 1000), p(1000, 1000)]),
        (172, vec![p(1400, 1000), p(1000, 1000)]),
        (205, vec![p(1000, 1000), p(1400, 1000)]),
        (240, vec![p(1200, 1200), p(800, 800), p(1200, 800), p(800, 1200)]),
    ];
    let pairs = pair_groups(&truth, &sim, PAIR_WINDOW_TICKS);
    assert_eq!(pairs.len(), 8, "{pairs:?}");
    // waves with waves, nearest in position inside each
    assert!(pairs.contains(&(0, 0, 0, 1)) && pairs.contains(&(0, 1, 0, 0)), "{pairs:?}");
    assert!(pairs.contains(&(1, 0, 1, 1)) && pairs.contains(&(1, 1, 1, 0)), "{pairs:?}");
    // the death spawn of 4 with the sim's 4 at 240 (same size), not with the extra
    // wave of 2 at 205 (nearer in time) -- order-of-appearance pairing took the wave
    for (gi, _, pi, _) in &pairs {
        if *gi == 2 {
            assert_eq!(*pi, 3, "{pairs:?}");
        }
    }
    assert!(pairs.contains(&(2, 0, 3, 1)) && pairs.contains(&(2, 3, 3, 0)), "{pairs:?}");
    assert!(!pairs.iter().any(|(_, _, pi, _)| *pi == 2), "the extra wave stays unmatched: {pairs:?}");
    // outside the window nothing pairs
    let far: Vec<(u32, Vec<Vec2>)> = vec![(100 + PAIR_WINDOW_TICKS + 1, vec![p(1000, 1000), p(1400, 1000)])];
    assert!(pair_groups(&truth[..1], &far, PAIR_WINDOW_TICKS).is_empty());
    // a sim pool of another size still fills a group when it is all there is
    let short: Vec<(u32, Vec<Vec2>)> = vec![(203, vec![p(800, 800), p(1200, 1200)])];
    let got = pair_groups(&truth[2..], &short, PAIR_WINDOW_TICKS);
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got.contains(&(0, 0, 0, 0)) && got.contains(&(0, 3, 0, 1)), "{got:?}");
}

#[test]
fn the_cards_json_hash_is_fnv1a64_and_the_fixture_carries_the_engine_s() {
    // known answers (FNV-1a 64): "" and "a"
    assert_eq!(fnv1a64(b""), "cbf29ce484222325");
    assert_eq!(fnv1a64(b"a"), "af63dc4c8601ec8c");
    let f = sample();
    let mine = cards_json_hash().expect("data/derived/cards.json on disk");
    assert_eq!(mine.len(), 16);
    // the sample was made from this tree's cards.json; a mismatch is a NOTE, not a refusal
    let r = play(&f);
    assert_eq!(r.cards_json_engine.as_deref(), Some(mine.as_str()));
    assert!(f.cards_json_fnv1a64.is_some(), "the maker writes cards_json_fnv1a64");
    if f.cards_json_fnv1a64.as_deref() == Some(mine.as_str()) {
        assert!(r.notes.is_empty(), "{:?}", r.notes);
    } else {
        assert!(r.notes.iter().any(|n| n.contains("cards.json")), "{:?}", r.notes);
        assert!(r.playable);
    }
}

#[test]
fn the_score_counters_are_consistent() {
    let r = play(&sample());
    r.score.consistent().expect("battle score");
    let mut sum = Score::default();
    for (card, sc) in &r.per_card {
        sc.consistent().unwrap_or_else(|e| panic!("{card}: {e}"));
        sum.add(sc);
    }
    assert_eq!(sum, r.score, "the per-card rows must sum to the battle");
    assert!(r.score.unit_ticks > 0);
    // a matched pair alive on both sides on every scored frame is one unit-tick per
    // frame: the towers that never fell are on every frame
    let frames = sample().truth.unwrap().ticks.len() as u64;
    let king = &r.per_card["KingTower"];
    assert_eq!(king.unit_ticks, 2 * frames, "two kings on every frame");
    assert_eq!(king.both_alive, king.unit_ticks);
    // THE FIRST DIVERGENCE is the earliest unit-tick beyond the tolerance or an alive mismatch, so nothing before it
    // may be one. At the shipped values the engine follows the client through the whole sample, since
    // placement.TROOP_TOWER_TAPS = client16402_half_open_relocate lays the own-king Skeletons where the client does:
    // there is none. Under the key's old arm, closed_block, those Skeletons spawn off, and the divergence's own
    // invariants are checked on that run.
    assert!(r.first_divergence.is_none(), "at the shipped values the sample diverges: {:?}", r.first_divergence);
    let mut opts = Options::default();
    opts.calibration_overrides.insert("placement.TROOP_TOWER_TAPS".to_string(), "\"closed_block\"".to_string());
    let old = replay(&sample(), &common::cards(), &register(), &opts).expect("the sample replays under closed_block");
    let d = old.first_divergence.as_ref().expect("under closed_block the own-king Skeletons spawn where the client's do not");
    assert!(d.tick > 0 && d.tick <= old.last_tick);
    assert!(["walking", "contact", "spawn", "death", "attack-timing", "knockback", "status"].contains(&d.cause.as_str()), "{}", d.cause);
    assert!(d.onset_tick <= d.tick);
}

#[test]
fn the_engine_meets_the_isolated_walk_floor_on_the_sample() {
    let r = play(&sample());
    for (card, sc) in &r.per_card {
        eprintln!("{card}: isolated walk {} ticks, within 250 on {}, within {WALK_TIGHT_NATIVE} on {}", sc.walk_ticks, sc.walk_within[0], sc.walk_tight);
    }
    // at least `min_tight` of the row's isolated-walk frames are bit-exact: the form
    // to use when the row carries more than one entity (below)
    let floor_tight_count = |card: &str, min_tight: u64, min_ticks: u64| {
        let sc = r.per_card.get(card).unwrap_or_else(|| panic!("no {card} row"));
        assert!(sc.walk_ticks >= min_ticks, "{card}: only {} isolated-walk ticks (want >= {min_ticks})", sc.walk_ticks);
        assert!(sc.walk_tight >= min_tight, "{card}: only {} of {} isolated-walk ticks within {WALK_TIGHT_NATIVE} native, floor {min_tight}", sc.walk_tight, sc.walk_ticks);
    };
    let floor = |card: &str, min_permille: u64, min_ticks: u64| {
        let sc = r.per_card.get(card).unwrap_or_else(|| panic!("no {card} row"));
        assert!(sc.walk_ticks >= min_ticks, "{card}: only {} isolated-walk ticks (want >= {min_ticks})", sc.walk_ticks);
        let pm = Score::permille(sc.walk_tight, sc.walk_ticks);
        assert!(pm >= min_permille, "{card}: isolated walk within {WALK_TIGHT_NATIVE} native on {pm} permille of {} ticks, floor {min_permille}", sc.walk_ticks);
    };
    // measured (bit-exact, error 0 on every frame): Prince 92 / 92, Giant
    // 79 / 79, Dark Prince 119 / 119 (module doc). The floor is 980 permille -- at
    // 1000 achieved, 980 leaves exactly one frame of slack on the shortest row
    // (Giant, 79 ticks: 78 of 79 is 987 permille and passes, 77 is 974 and does not).
    floor("Prince", 980, 80);
    floor("Giant", 980, 60);
    floor("DarkPrince", 980, 100);
    // THE BATTLE RAM ROW IS NO LONGER THE RAM ALONE (combat.KAMIKAZE_DEATH):
    // the engine now breaks the Ram on its hit, so the row -- keyed by the ROOT card --
    // carries its two death-spawned Barbarians' walk as well (118 isolated-walk frames,
    // was 98). The Ram's own 98 are still bit-exact and that is what this floor pins;
    // the Barbarians' own walk off the death point is the death-spawn / contact work,
    // not this row's, and is left to the corpus numbers (module doc). The floor is
    // 99 of the 100 the sample achieves: the ram's own 98 stay pinned and one of
    // its two Barbarians' frames is pinned with them, which
    // leaves the other free for the death-spawn timing work that is still open.
    floor_tight_count("BattleRam", 99, 98);
    // the formation row is measured, printed and NOT pinned (module doc)
    let sk = &r.per_card["Skeletons"];
    assert!(sk.walk_ticks >= 500, "the three cycled Skeletons walk {} isolated ticks", sk.walk_ticks);
    // the deploy-phase frames are counted apart and are stationary matches on the
    // single-unit deploys (DeployTime 1000 ms = 19 frames after the spawn frame,
    // fewer where the capture missed some)
    for card in ["Prince", "Giant", "DarkPrince", "Musketeer"] {
        let sc = &r.per_card[card];
        assert!(sc.deploy_ticks >= 9 && sc.deploy_ticks <= 20, "{card}: {} deploy-phase frames", sc.deploy_ticks);
        assert_eq!(sc.deploy_within[0], sc.deploy_ticks, "{card}: a deploying unit stands on its spawn point in both");
    }
}

#[test]
fn a_prefix_play_stops_before_the_first_unloadable_deploy() {
    // the sample with a deploy the engine refuses appended after tick 1000: a card name no
    // table carries, so no card becoming loadable can take this test's subject away
    let mut f = sample();
    let mut bogus = f.deploys[0].clone();
    bogus.tick = 1000;
    bogus.card = Some("NoSuchCard".into());
    bogus.keys = Vec::new();
    bogus.count = 1;
    f.deploys.push(bogus);
    let db = common::cards();
    let why = playability(&f, &db).expect_err("NoSuchCard is not loadable");
    assert!(why.iter().any(|u| u.why.starts_with("NoSuchCard: ") && u.cut == Some(1000)), "{why:?}");
    assert_eq!(prefix_cut(&f, &why), Some(1000));
    let register = register();
    let full = replay(&f, &db, &register, &Options::default()).unwrap();
    assert!(!full.playable);
    let pre = replay(&f, &db, &register, &Options { prefix: true, ..Options::default() }).unwrap();
    assert!(pre.playable);
    assert_eq!(pre.prefix_until, Some(1000));
    assert!(pre.last_tick < 1000);
    assert!(pre.deploys.iter().all(|d| d.tick < 1000), "{:?}", pre.deploys);
    // and a structural reason is never prefixed
    let mut g = sample();
    g.unplayable_reasons.push("capture starts mid-battle: first frame is tick 5 with 1 non-tower entities on the board".into());
    let why = playability(&g, &db).expect_err("structural");
    assert_eq!(prefix_cut(&g, &why), None);
}

#[test]
fn the_replay_is_deterministic() {
    let f = sample();
    let a = play(&f);
    let b = play(&f);
    assert_eq!(a.score, b.score);
    assert_eq!(serde_json::to_string(&a.first_divergence).unwrap(), serde_json::to_string(&b.first_divergence).unwrap());
    assert_eq!(a.pairs.len(), b.pairs.len());
}

#[test]
fn a_calibration_override_reaches_the_battle_config_names_itself_and_refuses_a_key_the_ledger_lacks() {
    // replay_parity --calibration-override section.KEY=JSON: the corpus judges a candidate
    // like for like without a ledger edit. The same hook as the binding's
    // calibration_overrides (Calib::shipped_with_overrides), so the two cannot part.
    let f = sample();
    let shipped = royalesim::state::Calib::shipped().king_activate_time_ms;
    let want = shipped + 250;
    let mut ov = std::collections::BTreeMap::new();
    ov.insert("match.KING_ACTIVATE_TIME_MS".to_string(), want.to_string());
    let (cfg, notes) = config_for_with(&f, common::cards(), None, &ov).expect("an override of a real key loads");
    assert_eq!(cfg.calib.king_activate_time_ms, want, "the override did not reach the config");
    assert!(notes.iter().any(|n| n == &format!("calibration override match.KING_ACTIVATE_TIME_MS = {want}")), "the run does not name its override: {notes:?}");
    let (plain, plain_notes) = config_for_with(&f, common::cards(), None, &std::collections::BTreeMap::new()).unwrap();
    assert_eq!(plain.calib.king_activate_time_ms, shipped, "no override, the shipped value");
    assert!(!plain_notes.iter().any(|n| n.starts_with("calibration override")));
    let mut bad = std::collections::BTreeMap::new();
    bad.insert("match.NOT_A_KEY".to_string(), "1".to_string());
    let err = config_for_with(&f, common::cards(), None, &bad).expect_err("an override cannot add a key");
    assert!(err.contains("not a key in the ledger"), "{err}");
}

#[test]
fn a_runs_overrides_are_in_its_notes_and_its_own_field_not_among_the_level_deviations() {
    let f = sample();
    let mut opts = Options::default();
    let want = royalesim::state::Calib::shipped().king_activate_time_ms + 250;
    opts.calibration_overrides.insert("match.KING_ACTIVATE_TIME_MS".to_string(), want.to_string());
    let r = replay(&f, &common::cards(), &register(), &opts).expect("the sample replays under an override");
    assert_eq!(r.calibration_overrides.get("match.KING_ACTIVATE_TIME_MS"), Some(&want.to_string()), "the report does not carry the run's override");
    assert!(r.notes.iter().any(|n| n.starts_with("calibration override match.KING_ACTIVATE_TIME_MS")), "the notes do not name the override: {:?}", r.notes);
    assert!(!r.level_deviations.iter().any(|n| n.contains("calibration override")), "an override is filed as a level deviation");
    assert!(play(&f).calibration_overrides.is_empty(), "the shipped run names no override");
}

/// A scenario deploy seen only as its members' centroid, or a spell request no unit was seen at, is played at the
/// tapped tile's centre. A deploy seen where it stands, and a corpus deploy (no pos_source; its `tap` an object), is
/// played at `pos`. Plant: replay_plays_the_centroid.
#[test]
fn a_scenario_deploy_seen_as_a_centroid_or_never_seen_is_played_at_its_tapped_tile() {
    let deploy = |extra: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": 218, "side": 0, "card": "Rascals", "card_id": 26000053, "kind": "troop", "level": 11, "count": 3,
                "pos": [9500, 11262], "source": "tap_tile"{extra}}}"#
        ))
        .expect("a deploy parses")
    };
    let centroid = deploy(r#", "tap": [9500, 11500], "pos_source": "observed_spawn_centroid""#);
    assert_eq!(play_point(&centroid), [9500, 11500], "a centroid is played at the tapped tile");
    let request = deploy(r#", "tap": [14188, 13556], "pos_source": "tap_request_no_unit_observed""#);
    assert_eq!(play_point(&request), [14500, 13500], "a raw request is played at its tile centre");
    let seen = deploy(r#", "tap": [9500, 11500], "pos_source": "observed_spawn""#);
    assert_eq!(play_point(&seen), [9500, 11262], "a deploy seen where it stands is played there");
    let corpus = deploy(r#", "tap": {"x": 9500, "y": 11500}"#);
    assert_eq!(play_point(&corpus), [9500, 11262], "a corpus deploy is played at pos");
}

/// A deploy of a card that travels underground (a corpus Miner: `pos` is its tunnel's first frame, next to its own
/// King) is played at its `destination`, where it came up; the engine walks it there from the King itself. The same
/// deploy without a destination is played at `pos`, as every corpus deploy is. Plant: replay_plays_the_tunnel_start.
#[test]
fn a_tunnelling_deploy_is_played_at_its_destination() {
    let deploy = |extra: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": 1203, "side": 0, "card": "Miner", "card_id": 26000032, "kind": "troop", "level": 11, "count": 1,
                "pos": [9235, 1777], "source": "centroid", "tap": {{"x": 3500, "y": 1500}}{extra}}}"#
        ))
        .expect("a deploy parses")
    };
    assert_eq!(play_point(&deploy(r#", "destination": [3500, 1500]"#)), [3500, 1500], "a tunnel is played where it came up");
    assert_eq!(play_point(&deploy("")), [9235, 1777], "a corpus deploy with no destination is played at pos");
}

/// A SCENARIO BUILDING ROW IS LAID FROM ITS RAW TAP, resolved as a play's (`scenario_building_tap`, `issue_row`,
/// `BattleState::spawn_unit_tapped`): client 15.535.29's Cannon tapped at (9000, 14500), its box over the river, stood on
/// (8500, 13500), where the row's snapped `pos` (9500, 14500) through `spawn_unit` stands it as put. A corpus building
/// row, a scenario troop row and a tunnel row with a `destination` are not scenario building taps. Plant:
/// replay_stacks_scenario_buildings.
#[test]
fn a_scenario_building_row_is_laid_from_its_raw_tap() {
    use royalesim::state::BattleState;
    use royalesim::Team;
    let row = |card: &str, kind: &str, extra: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": 961, "side": 0, "card": "{card}", "card_id": 27000000, "kind": "{kind}", "level": 11, "count": 1,
                "pos": [9500, 14500], "source": "tap_tile"{extra}}}"#
        ))
        .expect("a deploy parses")
    };
    let db = common::cards();
    let scenario = row("Cannon", "building", r#", "tap": [9000, 14500], "pos_source": "snapped_tap""#);
    assert_eq!(scenario_building_tap(&scenario, &db), Some([9000, 14500]), "the raw tap, not the snapped pos");
    assert_eq!(scenario_building_tap(&row("Cannon", "building", r#", "tap": {"tick": 940, "native": [9000, 14500]}"#), &db), None, "a corpus building row");
    assert_eq!(scenario_building_tap(&row("Knight", "troop", r#", "tap": [9000, 14500]"#), &db), None, "a scenario troop row");
    assert_eq!(scenario_building_tap(&row("Cannon", "building", r#", "tap": [9000, 14500], "destination": [9500, 14500]"#), &db), None, "a tunnel row");
    let k = royalesim::fixed::SUBTILE_PER_MILLITILE;
    let laid = |issue: &dyn Fn(&mut BattleState) -> Result<(), royalesim::state::DeployError>| -> Vec<Vec2> {
        let mut s = BattleState::new(1, common::config());
        issue(&mut s).expect("the Cannon is laid");
        s.pending_spawns().iter().map(|&(_, _, p)| p).collect()
    };
    let pos = from_native(play_point(&scenario)[0], play_point(&scenario)[1]);
    let issued = laid(&|s| issue_row(s, &db, &scenario, Team::Blue, "Cannon", pos));
    let tapped = laid(&|s| s.spawn_unit_tapped(Team::Blue, "Cannon", Vec2::new(9000 * k, 14500 * k), None));
    assert_eq!(issued, tapped, "the row is not laid as spawn_unit_tapped lays its raw tap");
    // Not vacuous: through spawn_unit at its play point the row stands where it was put.
    let put = laid(&|s| s.spawn_unit(Team::Blue, "Cannon", pos, None));
    assert_eq!(put, vec![Vec2::new(9500 * k, 14500 * k)]);
    assert_ne!(issued, put, "vacuous: the row is laid where spawn_unit puts it");
}

/// A MIRROR play, as tools/make_replay_fixture.py `mirror_plays` publishes it (card Mirror, kind "mirror", the copy
/// under `mirrored`), is played as the copied card at the row's level; every other row as its own card; a mirror row
/// that names no copy as the Mirror itself, which the engine then refuses by name.
#[test]
fn a_mirror_row_plays_the_card_it_copied() {
    let deploy = |extra: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": 805, "side": 1, "card_id": 26000000, "level": 12, "count": 1, "pos": [9500, 20500], "source": "tap_tile"{extra}}}"#
        ))
        .expect("a deploy parses")
    };
    let copy = deploy(
        r#", "card": "Mirror", "kind": "mirror", "mirrored": {"card": "ElixirGolem", "card_id": 26000067},
            "mirror_evidence": "ElixirGolem at level 12, one above the side's ElixirGolem level 11""#,
    );
    assert_eq!(mirror_play(&copy), "ElixirGolem");
    assert_eq!(copy.level, Some(12), "the copy's own level is kept");
    let plain = deploy(r#", "card": "Knight", "kind": "troop""#);
    assert_eq!(mirror_play(&plain), "Knight");
    let unnamed = deploy(r#", "card": "Mirror", "kind": "mirror""#);
    assert_eq!(mirror_play(&unnamed), "Mirror");
    let mut s = royalesim::state::BattleState::new(0, common::config());
    assert!(
        matches!(s.spawn_unit(royalesim::Team::Red, &mirror_play(&unnamed), Vec2::new(9500 * royalesim::fixed::SUBTILE_PER_MILLITILE, 20500 * royalesim::fixed::SUBTILE_PER_MILLITILE), Some(12)), Err(royalesim::state::DeployError::UnsupportedCard(..))),
        "a mirror row with no copy is refused, never guessed"
    );
}

/// A CLONE'S COPY IS ROOTED AS "Clone": the recording names every copy by the Clone card's id (28000013), whatever
/// unit it copies, so the maker labels it "Clone", and a copy rooted as the unit it copies has no counterpart and is
/// never scored. The sample gains a Knight on 700 and a Clone cast on it 30 ticks later; the recording has neither, so
/// both are unmatched on the sim side, where the copy is listed under "Clone" and the Knight under its own name. Plant:
/// replay_roots_a_copy_as_its_unit (a copy roots as its unit: no "Clone" row).
#[test]
fn a_clones_copy_is_rooted_as_clone() {
    let row = |tick: u32, card: &str, card_id: i64, kind: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": {tick}, "side": 0, "card": "{card}", "card_id": {card_id}, "kind": "{kind}", "level": 11, "count": 1,
                "pos": [8500, 5500], "source": "tap_tile"}}"#
        ))
        .expect("a deploy parses")
    };
    let mut f = sample();
    f.deploys.push(row(700, "Knight", 26000000, "troop"));
    f.deploys.push(row(730, "Clone", 28000013, "spell"));
    f.deploys.sort_by_key(|d| d.tick);
    let r = play(&f);
    let roots: Vec<&str> = r.unmatched_sim.iter().map(|(_, _, root)| root.as_str()).collect();
    assert!(roots.contains(&"Knight"), "vacuous: the Knight never stood on the board: {roots:?}");
    assert!(roots.contains(&"Clone"), "no copy is rooted as Clone: {roots:?}");
}

/// A SCENARIO TROOP ROW IS PLAYED AT ITS RAW TAP under placement.TAP_SNAP = client16402_tile_centre
/// (`scenario_troop_tap`): the engine snaps it to the tile the maker's `pos` names, and the relocation off an own crown
/// tower reads the raw tap (placement.TOWER_TAP_PUSH). Oracle's line tap (4500, 7000) on side 0's princess box, whose
/// maker `pos` is the floor-snapped (4500, 7500), lands on (5500, 6500) as the client's Knight does
/// (tests/tower_tap_push.rs's line taps); played at `pos`, the push reads (4500, 7500), a tie, and goes the other way.
/// Under placement.TAP_SNAP = none the row plays its `pos` as before. Plant: replay_plays_the_snapped_tap.
#[test]
fn a_scenario_troop_row_is_played_at_its_raw_tap_under_the_snap() {
    let db = common::cards();
    let row: Deploy = serde_json::from_str(
        r#"{"tick": 200, "side": 0, "card": "Knight", "card_id": 26000000, "kind": "troop", "level": 11, "count": 1,
            "pos": [4500, 7500], "tap": [4500, 7000], "pos_source": "snapped_tap", "source": "tap_tile"}"#,
    )
    .expect("a deploy parses");
    let native = |x: i32, y: i32| Vec2::new(x * royalesim::fixed::SUBTILE_PER_MILLITILE, y * royalesim::fixed::SUBTILE_PER_MILLITILE);
    let s = royalesim::state::BattleState::new(0, common::config());
    assert_eq!(scenario_troop_tap(&s, &db, &row), Some([4500, 7000]), "the shipped snap plays the raw tap");
    assert_eq!(resolve_on_board(&s, &db, &row), Some(native(5500, 6500)), "the line tap does not land on the client's tile");
    // Not vacuous: the maker's pos, played as the tap, lands elsewhere.
    let knight = db.index("Knight").expect("Knight loads");
    assert_ne!(s.resolve_point(royalesim::Team::Blue, knight, native(4500, 7500)), native(5500, 6500), "vacuous: pos and tap land alike");
    // Under TAP_SNAP none the row plays its pos, as before.
    let mut cfg = common::config();
    cfg.calib.placement_tap_snap = royalesim::state::TapSnap::None;
    let none = royalesim::state::BattleState::new(0, cfg);
    assert_eq!(scenario_troop_tap(&none, &db, &row), None);
}

/// A corpus troop row is resolved on the board of its TAP tick when that tick is before its issue tick: its
/// placements-log tap is an object with a `tick` (20260918-134739's Goblins, tapped on 204 on the tile of a Rage
/// cast on 202, are laid one tile over in the client though they appear on 228). A tap on the issue tick itself, a
/// scenario row (its tap an [x, y] pair), a row with no tap, a spell and a building are resolved when issued.
/// Plant: replay_resolves_at_issue.
#[test]
fn a_corpus_troop_is_resolved_on_the_board_of_its_tap_tick() {
    let row = |kind: &str, extra: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": 228, "side": 0, "card": "Goblins", "card_id": 26000002, "kind": "{kind}", "level": 11, "count": 4,
                "pos": [3500, 1500], "source": "tap_tile"{extra}}}"#
        ))
        .expect("a deploy parses")
    };
    let tapped = r#", "tap": {"tick": 204, "native": [3500, 1500], "cycled": true}"#;
    assert_eq!(resolve_tick(&row("troop", tapped)), Some(204), "tapped on 204, issued on 227: resolved on 204");
    assert_eq!(resolve_tick(&row("troop", r#", "tap": {"tick": 227, "native": [3500, 1500]}"#)), None, "a tap on the issue tick");
    assert_eq!(resolve_tick(&row("troop", r#", "tap": [3500, 1500]"#)), None, "a scenario row");
    assert_eq!(resolve_tick(&row("troop", "")), None, "a row with no tap");
    assert_eq!(resolve_tick(&row("spell", tapped)), None, "a spell");
    assert_eq!(resolve_tick(&row("building", tapped)), None, "a building");
}

/// Every row of the committed sample is a troop that carries its placements-log tap, 22 to 36 ticks before its units
/// appear, and the report says it was resolved on that tap's tick. The play is the one the gates above pin: under the
/// shipped ledger the resolution does not read the board, so the tick changes nothing here. Plant:
/// replay_resolves_at_issue.
#[test]
fn the_sample_rows_are_resolved_on_their_tap_ticks() {
    let f = sample();
    let r = play(&f);
    for d in &f.deploys {
        let tap = d.tap.as_ref().and_then(|t| t.get("tick")).and_then(|t| t.as_u64()).unwrap_or_else(|| panic!("{d:?} carries no tap tick")) as u32;
        let issued = r.deploys.iter().find(|i| i.tick == d.tick && i.side == d.side && Some(&i.card) == d.card.as_ref()).unwrap_or_else(|| panic!("deploy {d:?} not issued"));
        assert_eq!(issued.resolved_on, Some(tap), "{} at {}: resolved on {:?}, tapped on {tap}", issued.card, d.tick, issued.resolved_on);
        assert_eq!(issued.issued_at + 1, d.tick, "{} at {}: still issued on the tick before", issued.card, d.tick);
    }
    assert_eq!(r.deploys.iter().filter(|i| i.resolved_on.is_some()).count(), 6, "{:?}", r.deploys);
}

/// Why the tick matters: the engine's placement resolution reads the board. Under placement.TROOP_TOWER_TAPS the
/// sample's Skeletons tap on the own king tile goes to the tile behind the king on an empty board, and elsewhere once
/// a building stands on that tile, so one row resolved on its tap tick's board and on its issue tick's can land
/// apart.
#[test]
fn the_same_row_resolves_elsewhere_once_a_building_stands_on_its_landing_tile() {
    use royalesim::state::{BattleState, TapSnap, TroopTowerTaps};
    let k = royalesim::fixed::SUBTILE_PER_MILLITILE;
    let mut cfg = common::config();
    cfg.calib.placement_troop_tower_taps = TroopTowerTaps::HalfOpenRelocate;
    cfg.calib.placement_tap_snap = TapSnap::TileCentre;
    let db = common::cards();
    let mut s = BattleState::new(1, cfg);
    let row: Deploy = serde_json::from_str(
        r#"{"tick": 603, "side": 0, "card": "Skeletons", "card_id": 26000010, "kind": "troop", "level": 11, "count": 3,
            "pos": [8500, 1500], "source": "tap_tile", "tap": {"tick": 577, "native": [8500, 1500], "cycled": true}}"#,
    )
    .expect("a deploy parses");
    let empty = resolve_on_board(&s, &db, &row).expect("Skeletons load");
    assert_eq!(empty, Vec2::new(8500 * k, 500 * k), "on an empty board the king tap goes to the tile behind the king");
    s.spawn_unit(royalesim::Team::Blue, "Cannon", Vec2::new(8500 * k, 500 * k), None).expect("a Cannon is placed as given");
    s.tick();
    let built = resolve_on_board(&s, &db, &row).expect("Skeletons load");
    assert_ne!(built, empty, "a building on the landing tile must move the landing");
}

/// A SPAWNER'S EMISSION IS ROOTED THROUGH ITS SPAWNER, whatever record its unit is. The Furnace (FirespiritHut) puts
/// down the card FireSpirits' own unit on an interval, and the recording labels each spirit with the Furnace's card
/// id (client 16.402, 20260920-071744 and 071056: its spirits every 100 ticks from its deploy + 38). The harness rooted
/// the engine's spirits as a FireSpirits deploy, so the recording's had no counterpart. A synthetic fixture: one
/// Furnace, its first spirit in the truth on the tick the engine emits it; the spirit must pair, through the spawner,
/// and nothing of the engine's is left over. Plant: replay_roots_an_emitted_card_as_deployed.
#[test]
fn a_spawners_emission_whose_unit_is_a_card_is_rooted_through_its_spawner() {
    let n = 91usize;
    let ticks: Vec<u32> = (0..n as u32).collect();
    let col = |v: i64, frames: usize| format!("[{v}, {frames}]");
    let entity = |key: i64, role: &str, unit: &str, max_hp: i64, t0: usize, x: i64, y: i64| -> String {
        let frames = n - t0;
        format!(
            r#"{{"key": {key}, "side": 0, "card_id": 27000010, "card": "FirespiritHut", "role": "{role}", "unit": "{unit}",
                "level": 11, "max_hp": {max_hp}, "t0": {t0}, "n": {frames}, "x": {}, "y": {}, "hp": {}, "target": {},
                "path_n": {}, "state": {}}}"#,
            col(x, frames),
            col(y, frames),
            col(max_hp, frames),
            col(-1, frames),
            col(0, frames),
            col(4, frames)
        )
    };
    let furnace = entity(7, "summon", "Furnace_rework", 727, 10, 8500, 500);
    let spirit = entity(9, "spawned", "FireSpirits", 215, 48, 8500, 1500);
    let text = format!(
        r#"{{"format": "{FORMAT}", "deploy_tick_convention": "{DEPLOY_TICK_CONVENTION}", "capture": "synthetic-furnace",
            "frame": {{"blue_native_side": 0, "transform": "identity"}}, "truth_stride": 1, "playable": true,
            "ticks": {{"first": 0, "last": {last}, "frames": {n}}}, "tower_level": {{"0": 11, "1": 11}},
            "decks": {{"0": {{"deploy_order": ["FirespiritHut"]}}, "1": {{"deploy_order": ["Knight"]}}}},
            "deploys": [{{"tick": 10, "side": 0, "card": "FirespiritHut", "card_id": 27000010, "kind": "building",
                "level": 11, "count": 1, "keys": [7], "pos": [8500, 500], "source": "centroid"}}],
            "truth": {{"ticks": {ticks:?}, "entities": [{furnace}, {spirit}]}}}}"#,
        last = n - 1
    );
    let f = Fixture::from_str(&text).expect("the synthetic fixture parses");
    let r = replay(&f, &common::cards(), &register(), &Options::default()).expect("the synthetic fixture replays");
    let hut = r.pairs.iter().find(|p| p.truth_key == 7).unwrap_or_else(|| panic!("the Furnace did not pair: {:?}", r.pairs));
    assert_eq!(hut.root_how, "deployed");
    let p = r.pairs.iter().find(|p| p.truth_key == 9).unwrap_or_else(|| {
        panic!("the Furnace's spirit did not pair: pairs {:?}, the engine's unmatched {:?}", r.pairs, r.unmatched_sim)
    });
    assert_eq!((p.root.as_str(), p.sim_card.as_str(), p.root_how.as_str()), ("FirespiritHut", "FireSpirits", "spawner"), "{p:?}");
    assert!(p.sim_first_tick.abs_diff(p.truth_first_tick) <= 2, "the spirit pairs with the engine's first emission: {p:?}");
    assert!(r.unmatched_sim.iter().all(|(_, _, root)| root != "FireSpirits"), "an engine spirit rooted as a deploy: {:?}", r.unmatched_sim);
}

/// A FORM ROW (tools/make_replay_fixture.py `deploy_form`) spawns its form's own card: an evolved play (form "ev1") the
/// evolution, a hero play (form "hero") the hero form. The truth names both by their base card, and so does the harness
/// (`base_of_form`): the evolved Cannon pairs as "Cannon", the hero as "Musketeer". The deck entry is marked with the form
/// its rows play (`deck_form`), which gives the hero her button, and a row of kind "ability" presses it: the turret it
/// puts down roots to "Musketeer" too. A row whose form does not load plays its base card. A VARIANT row (a "base"
/// row of a variant card whose form_row is one of its forms: the Merge Maiden) plays its form_row; a variant row naming
/// no form of it, a Mirror row whose form_row names a card, and a plain card's row do not (plant:
/// replay_refuses_a_variant_row).
#[test]
fn a_form_row_spawns_its_form_and_its_units_score_as_the_base_card() {
    let db = common::cards();
    let row = |card: &str, form: &str, form_row: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": 20, "side": 0, "card": "{card}", "card_id": 0, "kind": "troop", "level": 11, "count": 1,
                "pos": [3500, 5500], "source": "tap_tile", "form": "{form}", "form_row": "{form_row}"}}"#
        ))
        .expect("a deploy parses")
    };
    assert_eq!(deploy_play(&row("Cannon", "ev1", "Cannon_EV1"), &db), "Cannon_EV1");
    assert_eq!(deploy_play(&row("Musketeer", "hero", "Musketeer_hero"), &db), "Musketeer_hero");
    assert_eq!(deploy_play(&row("Musketeer", "base", "Musketeer"), &db), "Musketeer");
    // A form the engine does not load plays its base card: the first of these evolved rows the table has no card for
    // (the Giant's is NotInUse in the table; the Lumberjack's, RageBarbarian_EV1, is not simulated yet).
    let unloaded = ["Giant", "RageBarbarian"]
        .into_iter()
        .find(|c| db.index(&format!("{c}_EV1")).is_none())
        .expect("vacuous: every listed form loads; name one that does not");
    assert_eq!(deploy_play(&row(unloaded, "ev1", &format!("{unloaded}_EV1")), &db), unloaded, "a form the engine does not load plays its base card");
    let mm = db.index("MergeMaiden").expect("the Merge Maiden loads");
    assert!(db.get(mm).variant().is_some(), "vacuous: the Merge Maiden is not a variant card");
    for form in ["MergeMaiden_Mounted", "MergeMaiden_Normal"] {
        assert_eq!(deploy_play(&row("MergeMaiden", "base", form), &db), form, "a variant row plays its form");
    }
    assert_eq!(deploy_play(&row("MergeMaiden", "base", "Knight"), &db), "MergeMaiden", "a form_row that is no form of the card");
    assert_eq!(deploy_play(&row("Mirror", "base", "Cannon"), &db), "Mirror", "a Mirror's copy is never read off form_row");
    assert_eq!(deploy_play(&row("Knight", "base", "Musketeer"), &db), "Knight", "a loadable card's row plays its own card");
    for (form, base) in [("Cannon_EV1", "Cannon"), ("Skeletons_EV1", "Skeletons"), ("Musketeer_hero", "Musketeer"), ("Knight", "Knight")] {
        assert_eq!(base_of_form(&db, form), base);
    }

    let n = 160usize;
    let ticks: Vec<u32> = (0..n as u32).collect();
    let col = |v: i64, frames: usize| format!("[{v}, {frames}]");
    let entity = |key: i64, card_id: i64, card: &str, max_hp: i64, t0: usize, x: i64, y: i64| -> String {
        let frames = n - t0;
        format!(
            r#"{{"key": {key}, "side": 0, "card_id": {card_id}, "card": "{card}", "role": "troop", "unit": null,
                "level": 11, "max_hp": {max_hp}, "t0": {t0}, "n": {frames}, "x": {}, "y": {}, "hp": {}, "target": {},
                "path_n": {}, "state": {}}}"#,
            col(x, frames),
            col(y, frames),
            col(max_hp, frames),
            col(-1, frames),
            col(0, frames),
            col(4, frames)
        )
    };
    let hero = entity(7, 203000014, "Musketeer", 721, 20, 3500, 5500);
    let cannon = entity(8, 13000096, "Cannon", 824, 30, 14500, 5500);
    let text = format!(
        r#"{{"format": "{FORMAT}", "deploy_tick_convention": "{DEPLOY_TICK_CONVENTION}", "capture": "synthetic-forms",
            "frame": {{"blue_native_side": 0, "transform": "identity"}}, "truth_stride": 1, "playable": true,
            "ticks": {{"first": 0, "last": {last}, "frames": {n}}}, "tower_level": {{"0": 11, "1": 11}},
            "decks": {{"0": {{"deploy_order": ["Musketeer", "Cannon"]}}, "1": {{"deploy_order": ["Knight"]}}}},
            "deploys": [
              {{"tick": 20, "side": 0, "card": "Musketeer", "card_id": 26000014, "kind": "troop", "level": 11, "count": 1,
                "keys": [7], "pos": [3500, 5500], "source": "tap_tile", "form": "hero", "form_row": "Musketeer_hero"}},
              {{"tick": 30, "side": 0, "card": "Cannon", "card_id": 27000000, "kind": "building", "level": 11, "count": 1,
                "keys": [8], "pos": [14500, 5500], "source": "tap_tile", "form": "ev1", "form_row": "Cannon_EV1"}},
              {{"tick": 121, "side": 0, "card": "Musketeer", "card_id": 26000014, "kind": "ability", "level": 11, "count": 0,
                "pos": [0, 0], "source": "ability_press", "form": "hero", "form_row": "Musketeer_hero"}}],
            "truth": {{"ticks": {ticks:?}, "entities": [{hero}, {cannon}]}}}}"#,
        last = n - 1
    );
    let f = Fixture::from_str(&text).expect("the synthetic fixture parses");
    assert_eq!(deck_form(&f, 0, "Musketeer", &db), royalesim::card::FORM_HERO);
    assert_eq!(deck_form(&f, 0, "Cannon", &db), 1, "an evolution is form 1");
    assert_eq!(deck_form(&f, 1, "Knight", &db), 0);
    let r = replay(&f, &db, &register(), &Options::default()).expect("the synthetic fixture replays");
    for (key, root, sim_card) in [(7, "Musketeer", "Musketeer_hero"), (8, "Cannon", "Cannon_EV1")] {
        let p = r.pairs.iter().find(|p| p.truth_key == key).unwrap_or_else(|| panic!("key {key} did not pair: {:?} {:?}", r.pairs, r.unmatched_sim));
        assert_eq!((p.root.as_str(), p.sim_card.as_str(), p.root_how.as_str()), (root, sim_card, "deployed"), "{p:?}");
    }
    let press = r.deploys.iter().find(|d| d.card == "Musketeer ability").expect("the press is issued");
    assert!(press.result.is_ok(), "the press is taken: {press:?}");
    assert!(r.unmatched_sim.iter().any(|(_, _, root)| root == "Musketeer"), "the turret roots to its hero: {:?}", r.unmatched_sim);
}


/// THE BATTLE'S END (`towers_down_agree`): the score stops at the engine's end only when the client's battle is over on
/// the same tick, which its crown towers decide. On the sample's first frame every tower stands in both, so they agree;
/// a truth tower shown at 0 hp on that frame (as a capture shows a fallen King Tower after its battle) is down in the
/// client and up in the engine, so they do not.
#[test]
fn the_score_stops_at_the_end_only_when_both_battles_are_over() {
    let f = sample();
    let mut truth = TruthTable::decode(f.truth.as_ref().expect("the sample has truth")).expect("the truth decodes");
    let (cfg, _) = config_for(&f, common::cards()).expect("the sample configures");
    let s = royalesim::state::BattleState::new(0, cfg);
    let first = truth.ticks[0];
    assert!(towers_down_agree(&f, &truth, &s, first), "every crown tower stands in both on the first frame");
    let t = &f.towers[0];
    let key = f.truth.as_ref().unwrap().entities.iter().find(|e| e.card_id == -1 && e.side == t.side).map(|e| e.key);
    let k = truth.entities.iter().position(|e| e.card_id == -1 && e.side == t.side && Some(e.key) == key).expect("a tower");
    let (t0, rows) = &mut truth.rows[k];
    let i = 0usize.saturating_sub(*t0);
    let mut row = rows[i].expect("the tower has a first row");
    row.hp = 0;
    rows[i] = Some(row);
    let _ = t;
    assert!(!towers_down_agree(&f, &truth, &s, first), "a tower down in the client and up in the engine: the ends differ");
    let r = play(&f);
    assert_eq!(r.engine_end_tick, None, "the sample's battle does not end");
    assert_eq!(r.score_until, None, "nothing is cut when the engine's battle does not end");
}


/// MEMBERS ON ONE POINT (a Ram and its Rider, created together on one spot) are told apart by their first hp: the tie
/// cost decides only between assignments of equal total distance. 20260920-003751's Ram (1766) and Rider (593) were paired
/// crosswise by order, and neither's hp matched on any tick.
#[test]
fn members_on_one_point_pair_by_their_first_hp() {
    let at = |x: i32, y: i32| Vec2::new(x * royalesim::fixed::SUBTILE_PER_MILLITILE, y * royalesim::fixed::SUBTILE_PER_MILLITILE);
    let p = at(14500, 8500);
    let truth = vec![(1443u32, vec![Some(p), Some(p)])];
    let sim = vec![(1442u32, vec![p, p])];
    let pairs = |got: Vec<(usize, usize, usize, usize)>| -> Vec<(usize, usize)> {
        let mut m: Vec<(usize, usize)> = got.iter().map(|&(_, mi, _, si)| (mi, si)).collect();
        m.sort();
        m
    };
    let got = pairs(pair_groups_hp(&truth, &sim, &[vec![Some(593), Some(1766)]], &[vec![1766, 593]], PAIR_WINDOW_TICKS));
    assert_eq!(got, vec![(0, 1), (1, 0)], "the 593 with the 593 and the 1766 with the 1766");
    assert_eq!(pairs(pair_groups(&truth, &sim, PAIR_WINDOW_TICKS)), vec![(0, 0), (1, 1)], "without hp, by order");
    // distance still decides first: the member 1000 away goes to the far truth point whatever its hp
    let truth2 = vec![(1443u32, vec![Some(p), Some(at(15500, 8500))])];
    let got = pairs(pair_groups_hp(&truth2, &sim_far(p, at(15500, 8500)), &[vec![Some(593), Some(1766)]], &[vec![593, 1766]], PAIR_WINDOW_TICKS));
    assert_eq!(got, vec![(0, 1), (1, 0)], "positions decide before hp");
}

fn sim_far(near: Vec2, far: Vec2) -> Vec<(u32, Vec<Vec2>)> {
    vec![(1442u32, vec![far, near])]
}


/// A capture runs the card values of the client that recorded it (`own_client_card_values`): a fixture naming client
/// 15.535.29 runs cards.CLIENT16402_VALUES = none and its report says so; one naming no client (the corpus maker's, the
/// sample) or 16.402 runs the ledger's arm; a run that overrides the key runs its override. Plant:
/// replay_card_values_by_ledger.
#[test]
fn a_capture_runs_the_card_values_of_the_client_that_recorded_it() {
    use royalesim::state::{Calib, CardValuesArm};
    let shipped = Calib::shipped().card_values;
    assert_eq!(shipped, CardValuesArm::Client16402, "the ledger ships the 16.402 values; this test reads a capture that differs");
    let none = std::collections::BTreeMap::new();
    let plain = sample();
    assert!(plain.card_table.is_none(), "the sample is a corpus capture and names no client");
    assert_eq!(config_for_with(&plain, common::cards(), None, &none).unwrap().0.calib.card_values, shipped);
    let mut old = sample();
    old.card_table = Some(CardTable { game_version: Some("15.535.29".to_string()) });
    assert_eq!(
        config_for_with(&old, common::cards(), None, &none).unwrap().0.calib.card_values,
        CardValuesArm::None,
        "a 15.535.29 capture ran the tables' values"
    );
    let r = replay(&old, &common::cards(), &register(), &Options::default()).expect("the sample replays as a 15.535.29 capture");
    assert_eq!(r.card_values_client.as_deref(), Some("15.535.29"), "the report does not name the client whose values it ran");
    assert!(r.notes.iter().any(|n| n.contains("arm none")), "the notes do not say the key ran at arm none: {:?}", r.notes);
    assert!(!r.level_deviations.iter().any(|n| n.contains("card values")), "the client is filed as a level deviation");
    assert_eq!(play(&plain).card_values_client, None, "a corpus capture names no client");
    let mut new = sample();
    new.card_table = Some(CardTable { game_version: Some("16.402.7".to_string()) });
    assert_eq!(config_for_with(&new, common::cards(), None, &none).unwrap().0.calib.card_values, shipped, "a 16.402 capture ran the 16.402 values");
    let ledger: serde_json::Value = serde_json::from_str(include_str!("../../../data/calibration.json")).unwrap();
    let value = ledger.pointer("/cards/CLIENT16402_VALUES/value").expect("the ledger carries the key's value");
    let mut ov = std::collections::BTreeMap::new();
    ov.insert(CARD_VALUES_KEY.to_string(), value.to_string());
    assert_eq!(
        config_for_with(&old, common::cards(), None, &ov).unwrap().0.calib.card_values,
        shipped,
        "a run that overrides the key runs its override, whatever the fixture's client"
    );
}

/// A TRUTH ENTITY WITH NO HITPOINTS AT ALL (max_hp below 0: the Hero Tombstone's visual dummy, NO_DAMAGE, UNTARGETABLE,
/// NO_CHECKCOLLISIONS) is paired with nothing, as an unknown object is. A copy of a troop's row made such a dummy and
/// put a frame ahead of it, first in its group, takes no pair and leaves every pair as the sample's own.
#[test]
fn a_truth_entity_with_no_hitpoints_takes_no_pair() {
    let pairs = |r: &Report| -> Vec<(i64, u32, u32)> {
        r.pairs.iter().map(|p| (p.truth_key, p.sim_index, p.sim_generation)).collect()
    };
    let base = play(&sample());
    let mut f = sample();
    let truth = f.truth.as_mut().expect("the sample carries its truth");
    let k = truth
        .entities
        .iter()
        .position(|e| e.role != "tower" && e.max_hp > 0 && e.t0 > 0)
        .expect("a unit row after the first frame");
    let mut dummy = truth.entities[k].clone();
    dummy.key = truth.entities.iter().map(|e| e.key).max().unwrap_or(0) + 1;
    dummy.max_hp = -1;
    dummy.t0 -= 1;
    let key = dummy.key;
    truth.entities.push(dummy);
    let r = play(&f);
    assert!(!r.pairs.iter().any(|p| p.truth_key == key), "the dummy took a pair");
    assert_eq!(pairs(&r), pairs(&base), "the dummy moved a pair");
}

/// A SKELETON KING'S SOUL IS ROOTED TO ITS KING (`register_new`): the units his button puts down are copies (state.rs
/// `soul_pass`, 1 hitpoint), and the recording names them by the King's card, as it names any unit a button puts down;
/// a Clone spell's copy stays "Clone" (`a_clones_copy_is_rooted_as_clone`). Plant: replay_roots_a_soul_as_clone.
#[test]
fn a_skeleton_kings_soul_is_rooted_to_its_king() {
    let row = |tick: u32, kind: &str, count: u32, source: &str| -> Deploy {
        serde_json::from_str(&format!(
            r#"{{"tick": {tick}, "side": 0, "card": "SkeletonKing", "card_id": 26000069, "kind": "{kind}", "level": 11,
                "count": {count}, "pos": [8500, 5500], "source": "{source}", "form": "base", "form_row": "SkeletonKing"}}"#
        ))
        .expect("a deploy parses")
    };
    let mut f = sample();
    // A champion's button comes from the deck: the sample's side 0 has no Skeleton King, so he goes first in it.
    f.decks.entry("0".to_string()).or_default().deploy_order.insert(0, "SkeletonKing".to_string());
    f.deploys.push(row(700, "troop", 1, "tap_tile"));
    f.deploys.push(row(760, "ability", 0, "ability_press"));
    f.deploys.sort_by_key(|d| d.tick);
    let r = play(&f);
    let press = r.deploys.iter().find(|d| d.card == "SkeletonKing ability").expect("the press is issued");
    assert!(press.result.is_ok(), "the press is taken: {press:?}");
    let kings = r.unmatched_sim.iter().filter(|(_, _, root)| root == "SkeletonKing").count();
    let clones = r.unmatched_sim.iter().filter(|(_, _, root)| root == "Clone").count();
    assert_eq!(clones, 0, "a soul rooted as Clone: {:?}", r.unmatched_sim);
    assert!(kings >= 2, "vacuous: no soul stood on the board (rooted to the King: {kings}, the King included)");
}

/// THE EVO GOBLIN BARREL'S DECOY DUMMIES ARE ROOTED TO THE GOBLIN BARREL (the harness's spell casts): the engine casts the
/// decoy barrel with the form (card.rs `EvoDef::mirror`), and the recording names its GoblinDummies by the Goblin Barrel.
/// Plant: replay_decoy_unrooted.
#[test]
fn an_evo_goblin_barrels_decoy_dummies_are_rooted_to_the_goblin_barrel() {
    let row: Deploy = serde_json::from_str(
        r#"{"tick": 700, "side": 0, "card": "GoblinBarrel", "card_id": 28000004, "kind": "spell", "level": 11, "count": 1,
            "pos": [14500, 22500], "source": "tap_tile", "form": "ev1", "form_row": "GoblinBarrel_EV1"}"#,
    )
    .expect("a deploy parses");
    let mut f = sample();
    f.deploys.push(row);
    f.deploys.sort_by_key(|d| d.tick);
    let r = play(&f);
    let cast = r.deploys.iter().find(|d| d.tick == 700 && d.card.starts_with("GoblinBarrel")).expect("the cast is issued");
    assert!(cast.result.is_ok(), "the cast is taken: {cast:?}");
    let dummies = r.unmatched_sim.iter().filter(|(_, _, root)| root == "GoblinDummy").count();
    let barrels = r.unmatched_sim.iter().filter(|(_, _, root)| root == "GoblinBarrel").count();
    assert_eq!(dummies, 0, "a decoy dummy rooted as itself: {:?}", r.unmatched_sim);
    assert!(barrels >= 6, "vacuous: the barrel's Goblins and the decoy's dummies did not all stand ({barrels} rooted to it)");
}

/// A CAPTURE RUNS ITS CLIENT'S OWN MECHANICS (`CLIENT15535_ARMS`): a fixture naming client 15.535.29 runs
/// movement.DYING_UNIT_VISIBILITY = client_doomed_static; one naming no client (the sample, a 16.402 corpus capture) or
/// 16.402 runs the ledger's whole_tick; a run that overrides the key runs its override. Plant: replay_client_arms_unread.
#[test]
fn a_capture_runs_its_clients_own_mechanics() {
    use royalesim::state::{Calib, ChaseHoldPastLimit, DeathDamageTick, DoomedOwnUpdate, DyingUnitVisibility, HeldFacing, HeldUnitAvoidance, KamikazeDeathContact, LadderEndRoute, LaunchPastTarget, RelocationTieOrder, TickOrder};
    let none = std::collections::BTreeMap::new();
    assert_eq!(Calib::shipped().dying_unit_visibility, DyingUnitVisibility::WholeTick, "the ledger ships whole_tick; this test reads a client that differs");
    assert_eq!(Calib::shipped().tick_order, TickOrder::Client16402, "the ledger ships client16402; this test reads a client that differs");
    assert_eq!(Calib::shipped().death_damage_tick, DeathDamageTick::NextTick, "the ledger ships next_tick; this test reads a client that differs");
    let plain = sample();
    let cfg = config_for_with(&plain, common::cards(), None, &none).unwrap().0;
    assert_eq!((cfg.calib.dying_unit_visibility, cfg.calib.tick_order), (DyingUnitVisibility::WholeTick, TickOrder::Client16402), "a capture with no version");
    assert_eq!(cfg.calib.death_damage_tick, DeathDamageTick::NextTick, "a capture with no version");
    let mut old = sample();
    old.card_table = Some(CardTable { game_version: Some("15.535.29".to_string()) });
    let (cfg, notes) = config_for_with(&old, common::cards(), None, &none).unwrap();
    assert_eq!(cfg.calib.dying_unit_visibility, DyingUnitVisibility::ClientDoomedStatic, "a 15.535.29 capture runs its client's arm");
    assert_eq!(cfg.calib.tick_order, TickOrder::ClientSequentialStrike, "a 15.535.29 capture runs its client's tick order");
    assert_eq!(cfg.calib.death_damage_tick, DeathDamageTick::Client15535DeathTick, "a 15.535.29 capture lands its death blows on the death tick");
    assert_eq!(cfg.calib.ladder_end_route, LadderEndRoute::Client15535Kept, "a 15.535.29 capture keeps a route through a knockback ladder");
    assert_eq!(cfg.calib.held_facing, HeldFacing::Client15535TowardWaypoint, "a 15.535.29 capture turns a held unit toward its waypoint");
    assert_eq!(cfg.calib.held_unit_avoidance, HeldUnitAvoidance::Scanned, "a 15.535.29 capture scans a held unit");
    assert_eq!(cfg.calib.doomed_own_update, DoomedOwnUpdate::Client15535KamikazeStays, "a 15.535.29 capture leaves a doomed kamikaze where it stood");
    assert_eq!(cfg.calib.kamikaze_death_contact, KamikazeDeathContact::Client15535AvoidedNotPushed, "a 15.535.29 capture's dying kamikaze pushes nobody");
    assert_eq!(cfg.calib.chase_hold_past_limit, ChaseHoldPastLimit::Client15535TroopsKept, "a 15.535.29 capture's troop holds past the limit");
    assert_eq!(cfg.calib.launch_past_target, LaunchPastTarget::Client15535HomingUnclamped, "a 15.535.29 capture's homing shot starts past a near target");
    assert_eq!(cfg.calib.relocation_tie_order, RelocationTieOrder::Client15535ArenaClockwise, "a 15.535.29 capture's building ties in the arena order");
    assert!(notes.iter().any(|n| n.contains("DYING_UNIT_VISIBILITY")), "the notes do not name the client's arm: {notes:?}");
    assert!(notes.iter().any(|n| n.contains("TICK_ORDER")), "the notes do not name the client's tick order: {notes:?}");
    let mut new = sample();
    new.card_table = Some(CardTable { game_version: Some("16.402.7".to_string()) });
    let cfg = config_for_with(&new, common::cards(), None, &none).unwrap().0;
    assert_eq!((cfg.calib.dying_unit_visibility, cfg.calib.tick_order), (DyingUnitVisibility::WholeTick, TickOrder::Client16402), "a 16.402 capture");
    assert_eq!(cfg.calib.death_damage_tick, DeathDamageTick::NextTick, "a 16.402 capture");
    assert_eq!(cfg.calib.ladder_end_route, LadderEndRoute::Client16402Dropped, "a 16.402 capture");
    assert_eq!(cfg.calib.held_facing, HeldFacing::Unchanged, "a 16.402 capture");
    assert_eq!(cfg.calib.held_unit_avoidance, HeldUnitAvoidance::Masked, "a 16.402 capture");
    assert_eq!(cfg.calib.doomed_own_update, DoomedOwnUpdate::Walks, "a 16.402 capture");
    assert_eq!(cfg.calib.kamikaze_death_contact, KamikazeDeathContact::AsDoomed, "a 16.402 capture");
    assert_eq!(cfg.calib.chase_hold_past_limit, ChaseHoldPastLimit::InsideOnly, "a 16.402 capture");
    assert_eq!(cfg.calib.launch_past_target, LaunchPastTarget::Clamped, "a 16.402 capture");
    assert_eq!(cfg.calib.relocation_tie_order, RelocationTieOrder::PlacerFrameFirstFound, "a 16.402 capture");
    let mut ov = std::collections::BTreeMap::new();
    ov.insert("match.TICK_ORDER".to_string(), "\"client16402\"".to_string());
    let cfg = config_for_with(&old, common::cards(), None, &ov).unwrap().0;
    assert_eq!(
        (cfg.calib.dying_unit_visibility, cfg.calib.tick_order),
        (DyingUnitVisibility::ClientDoomedStatic, TickOrder::Client16402),
        "an override of one key runs its override and leaves the client's other arms"
    );
    let mut ov = std::collections::BTreeMap::new();
    ov.insert("movement.DYING_UNIT_VISIBILITY".to_string(), "\"whole_tick\"".to_string());
    assert_eq!(
        config_for_with(&old, common::cards(), None, &ov).unwrap().0.calib.dying_unit_visibility,
        DyingUnitVisibility::WholeTick,
        "a run that overrides the key runs its override, whatever the fixture's client"
    );
}
