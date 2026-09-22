//! THE REPLAY-PARITY HARNESS on its committed sample: the harness invariants, and a
//! floor the engine already meets.
//!
//! THE SAMPLE is `fixtures/replay/sample.json`: live capture 20260920-003751 (seat B,
//! a scripted battle of single-card deploys), trimmed to its first 1440 ticks by
//! `tools/make_replay_fixture.py ... --until-tick 1440`. The trim is that wide because
//! the walks the sample exists for are a PRINCE (tick 172: a 1.9-tile walk, the charge
//! onset at 2.5 tiles, the tower hits) and a GIANT (tick 1329, the stomp walker), and
//! no capture of that series deploys a Giant inside its first 600 ticks; the trim
//! stops before the Ram Rider at 1442, which the loader refuses
//! (`spawner RamRider: no SpawnPauseTime`) and which would make the fixture
//! unplayable. Also on the board: a Dark Prince, three cycled Skeletons behind the
//! king tower, a Battle Ram with its two death-spawned Barbarians, a Musketeer.
//!
//! WHAT IS GATED
//!   invariants   every deploy is issued on the tick before its recorded tick and the
//!                engine's units exist ON that tick (plant: replay_deploys_one_tick_
//!                late); the matching is one-to-one on both sides; the score counters
//!                are consistent (nested tolerances, parts summing to the total, the
//!                per-card rows summing to the battle); the group pairing keeps a
//!                spawner's extra wave from shifting its death spawn (synthetic).
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
            assert!(p.truth_first_tick >= d.tick && p.truth_first_tick < d.tick + d.first_seen_gap.max(1), "{d:?} vs first seen {}", p.truth_first_tick);
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
    // the Barbarians appear 63 ticks later in the engine (the ram dies later: the
    // report's item 3) and still pair, inside PAIR_WINDOW_TICKS
    for p in &barbarians {
        assert!(p.sim_first_tick > p.truth_first_tick && p.sim_first_tick - p.truth_first_tick <= PAIR_WINDOW_TICKS, "{p:?}");
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
    // the first divergence is the earliest unit-tick beyond the tolerance or an alive
    // mismatch, so nothing before it may be one
    let d = r.first_divergence.as_ref().expect("the sample diverges (the tower falls later in the engine)");
    assert!(d.tick > 0 && d.tick <= r.last_tick);
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
    // 79 / 79, Dark Prince 119 / 119, Battle Ram 98 / 98 (module doc)
    floor("Prince", 950, 80);
    floor("Giant", 950, 60);
    floor("DarkPrince", 950, 100);
    // THE BATTLE RAM ROW IS NO LONGER THE RAM ALONE (combat.KAMIKAZE_DEATH):
    // the engine now breaks the Ram on its hit, so the row -- keyed by the ROOT card --
    // carries its two death-spawned Barbarians' walk as well (118 isolated-walk frames,
    // was 98). The Ram's own 98 are still bit-exact and that is what this floor pins;
    // the Barbarians' own walk off the death point is the death-spawn / contact work,
    // not this row's, and is left to the corpus numbers (module doc).
    floor_tight_count("BattleRam", 98, 98);
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
    // the sample with a deploy the engine refuses appended after tick 1000
    let mut f = sample();
    let mut bogus = f.deploys[0].clone();
    bogus.tick = 1000;
    bogus.card = Some("RamRider".into());
    bogus.keys = Vec::new();
    bogus.count = 1;
    f.deploys.push(bogus);
    let db = common::cards();
    let why = playability(&f, &db).expect_err("RamRider is not loadable");
    assert!(why.iter().any(|u| u.why.starts_with("RamRider: ") && u.cut == Some(1000)), "{why:?}");
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
