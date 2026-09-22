//! SAVE / LOAD -- the Python Engine protocol's save_state() -> bytes and
//! load_state(bytes).
//!
//! WHY IT EXISTS: the env layer's state setter and replay both resume battles
//! from snapshots. A snapshot that drops one field (the Rng, a projectile's
//! sub-subtile carry, a unit's route) resumes a DIFFERENT battle that looks
//! identical for a few ticks -- exactly the divergence nothing downstream can
//! attribute.
//!
//! THE CHECK: run N ticks, save, run M more recording state_hash every tick;
//! load, run the same M ticks, and the hash sequences must be identical. Done
//! mid-battle (projectiles in flight, units mid-windup, deploys pending) and
//! with a pending spawn queue.
mod common;

use royalesim::state::BattleState;
use royalesim::Team;
use common::*;

/// Drive the scripted battle for `n` ticks and return the state with its script.
///
/// THE BATTLE MUST STILL BE LIVE AT `n`, and that is asserted, not hoped for. `tick()`
/// is a no-op once the battle is decided, so `tick_count() < n` alone would spin for
/// ever -- but exiting on `is_done()` instead silently HOLLOWS the test: the caller
/// then saves a finished battle, and `hashes_after` compares 600 no-op ticks against
/// 600 no-op ticks, which any snapshot passes. Measured: with
/// time.SPEED_TO_SUBTILES_PER_TICK at 18 this scripted battle is decided at tick
/// 3600, so any `n` at or past that is vacuous.
fn scripted_until(n: u32) -> (BattleState, Script) {
    let mut s = BattleState::new(0x5AFE, scripted_config());
    let mut script = Script::new(40);
    while s.tick_count() < n && !s.is_done() {
        script.step(&mut s);
        s.tick();
    }
    assert!(
        !s.is_done() && s.tick_count() == n,
        "scripted_until({n}) wanted a LIVE battle and got one decided at tick {}; \
         move the case below the decision tick or drive a longer scenario",
        s.tick_count()
    );
    (s, script)
}

fn hashes_after(s: &mut BattleState, script: &mut Script, m: u32) -> Vec<u64> {
    let mut out = Vec::with_capacity(m as usize);
    for _ in 0..m {
        script.step(s);
        s.tick();
        out.push(s.state_hash());
    }
    out
}

fn clone_script(sc: &Script) -> Script {
    Script { period: sc.period, plays: sc.plays, rejected: sc.rejected, spell_casts: sc.spell_casts }
}

#[test]
fn save_then_load_resumes_the_identical_battle() {
    // Plant: save_drops_rng (the snapshot's Rng is replaced).
    // 3400, not 3601: the scripted battle is decided at tick 3600 (see
    // `scripted_until`), so the longest case has to sit below that to round-trip a
    // late-battle snapshot with 600 REAL ticks after it. 3400 + 600 = 4000 still runs
    // through the decision, so the compared hashes cover it.
    for n in [1u32, 97, 1203, 3400] {
        let (mut s, script) = scripted_until(n);
        let blob = s.save();
        let saved_hash = s.state_hash();
        let mut script_a = clone_script(&script);
        let a = hashes_after(&mut s, &mut script_a, 600);

        let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("load failed at tick {n}: {e}"));
        assert_eq!(l.state_hash(), saved_hash, "loaded state hashes differently from the saved one (tick {n})");
        let mut script_b = clone_script(&script);
        let b = hashes_after(&mut l, &mut script_b, 600);
        if let Some(k) = a.iter().zip(b.iter()).position(|(x, y)| x != y) {
            panic!("resumed battle diverged {k} ticks after loading a tick-{n} snapshot");
        }
        assert_eq!(s.tower_hp(Team::Blue), l.tower_hp(Team::Blue));
        assert_eq!(s.tower_hp(Team::Red), l.tower_hp(Team::Red));
    }
}

#[test]
fn snapshot_mid_battle_actually_contains_the_hard_parts() {
    // Vacuity guard: find a tick with projectiles in flight AND a unit mid-windup
    // AND a deploying unit, and round-trip exactly there.
    let mut s = BattleState::new(0x5AFE, scripted_config());
    let mut script = Script::new(40);
    let mut found = false;
    for _ in 0..4000 {
        script.step(&mut s);
        s.tick();
        let windup = s.entities().any(|e| e.attack_phase == royalesim::entity::AttackPhase::Windup);
        let deploying = s.entities().any(|e| e.deploying);
        if !s.projectiles().is_empty() && windup && deploying {
            found = true;
            break;
        }
    }
    assert!(found, "never reached a tick with projectiles, a windup and a deploy at once");
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap();
    let mut sa = clone_script(&script);
    let mut sb = clone_script(&script);
    assert_eq!(hashes_after(&mut s, &mut sa, 300), hashes_after(&mut l, &mut sb, 300));
}

#[test]
fn pending_spawns_and_the_rng_round_trip() {
    // A deploy queued but not yet materialised, and a shuffled-deck battle
    // (the Rng has been consumed), must both survive.
    let mut cfg = scripted_config();
    cfg.shuffle_decks = true;
    let mut s = BattleState::new(31337, cfg);
    let card = s.hand(Team::Blue)[0].to_string();
    s.tick();
    let pos = Script::blue_pos(&s, &card, false);
    s.deploy(Team::Blue, &card, pos).unwrap();
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap();
    assert_eq!(s.state_hash(), l.state_hash());
    let before = l.live_count();
    for k in 0..200 {
        s.tick();
        l.tick();
        assert_eq!(s.state_hash(), l.state_hash(), "diverged at tick {}", s.tick_count());
        if k == 0 {
            assert!(!find_live(&l, Team::Blue, &card).is_empty() && l.live_count() > before, "the pending {card} deploy did not materialise after load");
        }
    }
}

#[test]
fn restore_into_an_existing_state_matches_load() {
    let (s, _) = scripted_until(500);
    let blob = s.save();
    let (mut other, _) = scripted_until(10);
    other.restore(&blob).unwrap();
    assert_eq!(other.state_hash(), s.state_hash());
}

#[test]
fn load_rejects_corruption_and_foreign_card_data() {
    let (s, _) = scripted_until(300);
    let blob = s.save();
    // Truncated.
    assert!(BattleState::load(&blob[..blob.len() / 2]).is_err());
    // A snapshot whose recorded hash does not match its contents: change the
    // tick inside the JSON and the hash self-check must refuse it.
    // Plant: load_skips_selfcheck.
    let text = String::from_utf8(blob.clone()).expect("snapshot is utf-8 json");
    let tampered = text.replacen("\"tick\":300", "\"tick\":301", 1);
    assert_ne!(tampered, text, "tamper did not apply -- a plant must verify its own edit");
    assert!(BattleState::load(tampered.as_bytes()).is_err(), "a snapshot whose contents disagree with its hash loaded");
    // Built from different card data: load() (which uses data/derived/cards.json)
    // must refuse rather than resume against the wrong cards.
    let foreign = BattleState::new(1, royalesim::state::BattleConfig::with_cards(royalesim::card::CardDb::fallback()));
    let fb = foreign.save();
    assert!(BattleState::load(&fb).is_err(), "a fallback-card snapshot loaded against cards.json");
}

#[test]
fn save_is_reasonably_small_and_fast() {
    let (s, _) = scripted_until(2000);
    let t0 = std::time::Instant::now();
    let mut blob = Vec::new();
    for _ in 0..50 {
        blob = s.save();
    }
    let save_us = t0.elapsed().as_micros() / 50;
    let t1 = std::time::Instant::now();
    for _ in 0..50 {
        let _ = BattleState::load(&blob).unwrap();
    }
    let load_us = t1.elapsed().as_micros() / 50;
    println!("snapshot: {} bytes, {} live entities, save {save_us} us, load {load_us} us (this build profile)", blob.len(), s.live_count());
    assert!(blob.len() < 1_000_000, "snapshot is {} bytes", blob.len());
}

/// A battle with every kind of spell state live at once, for the round-trip tests:
/// Blue and Red Fireballs, Logs, Goblin Barrels and Zaps cast at staggered ticks onto
/// each other's Knights and Giants.
///
/// KNOCKBACK SLIDES: the shipped knockback.DISPLACEMENT_LAW is the 16.402 ladder
/// (its mid-ladder round trip is tests/knockback16402.rs), and the fixed_distance
/// arm's shipped DURATION_MS is 0 (instant), under which no slide state ever exists
/// between ticks. This battle runs the fixed_distance arm with the registry's other
/// duration candidate (500 ms, crforge's unsourced value) so `knock_rem` /
/// `knock_ms` are live and can be caught being dropped. A snapshot carries its
/// Calib, so the loaded battle runs the same candidates.
fn spell_battle() -> BattleState {
    let mut cfg = config();
    cfg.calib.knock_law = royalesim::state::KnockLaw::FixedDistance;
    cfg.calib.knock_stacking = royalesim::state::KnockStacking::VectorSum;
    cfg.calib.knock_zero_vector = royalesim::state::KnockZeroVector::CasterForward;
    cfg.calib.knock_duration_ms = 500;
    let mut s = BattleState::new(372_241, cfg);
    // Cannons on x = 9 so both Logs' rolls hit something (the troops walk off the axis).
    for (team, card, x, y) in [(Team::Red, "Knight", 900, 1900), (Team::Red, "Giant", 1050, 2000), (Team::Blue, "Knight", 900, 1300), (Team::Blue, "Giant", 750, 1200), (Team::Red, "Cannon", 900, 1750), (Team::Blue, "Cannon", 900, 1450)] {
        // The Cannons shoot each other across the river; they must outlive the rolls.
        let hp = (card == "Cannon").then_some(1_000_000);
        s.scenario_spawn_now(team, card, t(x, y), hp).unwrap();
    }
    s
}

fn spell_script(s: &mut BattleState) {
    let k = s.tick_count();
    let casts: &[(u32, Team, &str, (i32, i32))] = &[
        (0, Team::Blue, "Fireball", (900, 1850)),
        (0, Team::Red, "Fireball", (900, 1350)),
        (4, Team::Blue, "Log", (900, 1250)),
        (4, Team::Red, "Log", (900, 1950)),
        (10, Team::Blue, "GoblinBarrel", (900, 2300)),
        (12, Team::Red, "Zap", (900, 1300)),
        (30, Team::Blue, "Zap", (950, 1850)),
    ];
    for &(at, team, card, (x, y)) in casts {
        if at == k {
            s.spawn_unit(team, card, t(x, y), None).unwrap();
        }
    }
}

#[test]
fn spells_in_flight_stuns_and_knockback_slides_round_trip() {
    // Save on a tick where a spell is in flight, a Log is mid-roll with a non-empty hit
    // set, a unit is stunned and a unit is mid-knockback-slide; the loaded battle must
    // reproduce state_hash on every one of the next 200 ticks.
    // Plants: save_drops_spells, save_drops_knockback.
    let mut s = spell_battle();
    let mut found = None;
    for _ in 0..200 {
        spell_script(&mut s);
        s.tick();
        let rolling_with_hits = s.spells().iter().any(|sp| matches!(&sp.motion, royalesim::spell::SpellMotion::Rolling { hit, .. } if !hit.is_empty()));
        let flight = s.spells().iter().any(|sp| matches!(&sp.motion, royalesim::spell::SpellMotion::Flight { .. }));
        let stunned = s.entities().any(|e| e.stun_ms > 0);
        let sliding = s.entities().any(|e| e.knock_ms > 0 && e.knock_rem != royalesim::fixed::Vec2::default());
        if rolling_with_hits && stunned && sliding && flight {
            found = Some(s.tick_count());
            break;
        }
    }
    let at = found.expect("vacuous: never reached a tick with a rolling Log that has hit, a flight, a stun and a slide at once");
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("load at tick {at}: {e}"));
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 0..200 {
        spell_script(&mut s);
        spell_script(&mut l);
        s.tick();
        l.tick();
        assert_eq!(s.state_hash(), l.state_hash(), "resumed spell battle diverged {k} ticks after a tick-{at} snapshot");
    }
    println!("spell snapshot at tick {at}: {} bytes, {} live, {} spells", blob.len(), s.live_count(), s.spells().len());
}

#[test]
fn state_hash_sees_spells_in_flight() {
    // Two battles identical except that one Fireball is aimed one subtile apart: before
    // anything lands, only the spell list differs, and the hash must see it.
    // Plant: hash_skips_spells.
    let mut a = spell_battle();
    let mut b = spell_battle();
    a.spawn_unit(Team::Blue, "Fireball", t(900, 2400), None).unwrap();
    let p = t(900, 2400);
    b.spawn_unit(Team::Blue, "Fireball", royalesim::fixed::Vec2::new(p.x, p.y + 1), None).unwrap();
    a.tick();
    b.tick();
    assert!(!a.spells().is_empty() && a.spells() != b.spells(), "vacuous: the spell lists do not differ");
    assert_ne!(a.state_hash(), b.state_hash(), "state_hash does not see a spell in flight");
}
