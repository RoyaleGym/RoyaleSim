//! PERIODIC SPAWNERS AND DEATH SPAWN: the Spawn* and DeathSpawn*
//! columns (card.rs `SpawnerDef` / `DeathSpawnDef`; state.rs `spawner_pass`,
//! `phase_reap`; calibration `spawner.*`).
//!
//! WHAT IS PINNED, every number read from the loaded CardDb and `Calib::shipped()`:
//!   1. a Tombstone alone spawns exactly one Skeleton per SpawnPauseTime, on the
//!      predicted ticks, alive, on its team and level, in front of it, on dry ground,
//!      and the unit is queued one tick before it exists (the one-tick latency);
//!   2. a Goblin Hut does the same with a SpearGoblin;
//!   3. a Witch spawns a wave of SpawnNumber Skeletons SpawnInterval apart, the first
//!      SpawnStartTime after activation, the next wave one pause after the last;
//!   4. a Golem killed by debug_set_hp leaves two Golemites next tick ON a ring of
//!      DeathSpawnRadius around the death point, deploying, AND its death damage lands;
//!   5. a Lava Hound leaves six flying Pups, on its own ring of six;
//!   6. (spawner.DEATH_SPAWN_LAYOUT = facing_ring) the ring's AXIS: a
//!      Battle Ram that died on its own hit leaves its two Barbarians on the line
//!      from the death point to the tower it hit, one ahead and one behind -- the
//!      corpus's two ram deaths, the measurement the key carries; the numbering
//!      below is one out from here on (the list is a map, not a contract);
//!   6. a Tombstone expiring by lifetime leaves its four Skeletons;
//!   7. a Zap on a Tombstone delays its next wave by exactly the stun's ticks;
//!   8. SpawnLimit caps a spawner's live units (a constructed CardDb: no 2018 row sets it);
//!   9. a symmetric two-seat Tombstone + Golem scene is its own mirror every tick;
//!  10. a snapshot taken mid-countdown, mid-wave, resumes hash-for-hash;
//!  11. a full scripted battle with Tombstone and Witch in both decks keeps every
//!      tests/common invariant;
//!  12. every calibration spawner.* candidate moves a measurable behaviour;
//!  13. the loader reads the blocks from cards.json, shares one unit table with the
//!      Goblin Barrel, and rejects the cards whose chain it cannot load, with why;
//!  14. a DarkWitch's simultaneous two-Bat wave lands both Bats
//!      on the same post-tick, flying, tagged, one Bat diameter apart around the
//!      point SpawnRadius ahead of her; a killed BattleRam's Barbarians carry the
//!      block's DeathSpawnDeployTime, not their own;
//!  15. a hut never targets: a Goblin Barrel's goblins inside a
//!      Tombstone's footprint leave it target-less and Idle;
//!  16. no card, rejected ones included, keeps a unit block
//!      pointing at the unresolved u16::MAX; a constructed death spawn whose grid
//!      reaches past its radius is pulled back onto it (the engine_grid_within_radius
//!      arm, the foil rather than the shipped layout).
//!
//! TICK ALIGNMENT (state.rs `spawner_pass`; spawner.EMISSION_TIMING =
//! move_phase_immediate, measured): the timer is decremented in the MOVE phase, right
//! after the deploy countdown, and the unit is CREATED there, in that same tick -- a
//! live Tombstone's first Skeleton is on the board on the deploy-end tick. "Post-tick
//! k" means the state after the k-th `tick()` call (`tick_count() == k`). A scenario
//! spawn activates before tick 1 runs, so with a blank SpawnStartTime its first unit is
//! live at post-tick 1 and with a start time S at post-tick max(ceil(S / TICK_MS), 1);
//! a spawner that served a deploy timer activates INSIDE the Move phase of post-tick
//! A = ceil(DeployTime / TICK_MS), where the pass decrements once already, so its first
//! unit lands at A + max(ceil(S / TICK_MS), 1) - 1. The n-th wave follows
//! (n - 1) * ceil(P / TICK_MS) post-ticks later. ~~The old arm (spawn_phase_next_tick,
//! kept runnable): the timer ran at the end of the SPAWN phase and a unit queued in
//! tick index E was PENDING at post-tick E + 1 and LIVE from post-tick E + 2.~~
//!
//! PLANTS: spawner_never_fires -- (1), (2), (3), (7), (8), (9),
//! (10), (12) go red (8 of 13; (11) stays green because a dying Tombstone's DEATH
//! spawn still puts Skeletons on the board); death_spawn_dropped -- (4), (5), (6),
//! (9), (12) go red (5 of 13). Run 2026-09-21: spawner_first_wave_late (the
//! earlier emission, whatever the ledger says) -- 6 of the file's 17 tests go
//! red, every one of them on a TICK: the Tombstone's and the hut's wave ticks, the
//! Witch's [21, 21, 21, 21, 161, ...] against [20, 20, 20, 20, 160, ...], the Dark
//! Witch's wave one post-tick late, the SpawnLimit refill at 62 instead of 61, and
//! the candidate sweep's first_wave_after_one_pause row at 70 instead of 71.

mod common;

use common::*;
use royalesim::card::{CardDb, CardKind, CardSource};
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{isqrt, milli, Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{
    BattleConfig, BattleState, BuffExpiry, Calib, DeathSpawnDeploy, DeathSpawnLayout, DeathSpawnRadius, FirstWave, PauseAnchor, SpawnPoint,
    SpawnedDeploy, SpawnerEmission, StartTimeOrigin,
};
use royalesim::{EntityId, Team};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// data, read not typed

fn calib() -> Calib {
    Calib::shipped()
}

fn dt() -> i32 {
    calib().tick_ms
}

/// ceil(ms / TICK_MS): the Spawn-phase decrements that run a ms timer out.
fn ticks_of(ms: i32) -> u32 {
    ((ms + dt() - 1) / dt()) as u32
}

fn spawner(s: &BattleState, card: &str) -> royalesim::card::SpawnerDef {
    card_stat(s, card).spawner.unwrap_or_else(|| panic!("cards.json {card} has no spawner block"))
}

fn death_spawn(s: &BattleState, card: &str) -> royalesim::card::DeathSpawnDef {
    card_stat(s, card).death_spawn.unwrap_or_else(|| panic!("cards.json {card} has no death_spawn block"))
}

fn unit_name(s: &BattleState, idx: u16) -> String {
    s.cards().get(idx).name.clone()
}

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(7, cfg)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = config();
    f(&mut cfg.calib);
    cfg
}

/// A spot on Blue's half, clear of every tower footprint and the river, for a
/// building or a troop under test; its rotation is Red's.
fn blue_spot(s: &BattleState) -> Vec2 {
    let p = t(900, 900);
    assert!(s.arena().is_passable_ground(p), "scene: the spot is on dry ground");
    p
}

/// The POST-TICK at which the first unit of a spawner activated at tick index 0 is
/// live (calibration spawner.FIRST_WAVE, shipped arm; module doc), then one pause
/// per wave.
fn first_wave_tick(sp: &royalesim::card::SpawnerDef) -> u32 {
    assert_eq!(calib().spawner_first_wave, FirstWave::AfterStartTimeOrImmediately, "the shipped arm this test pins");
    let start = sp.start_time_ms.map_or(0, ticks_of).max(1);
    match calib().spawner_emission_timing {
        // the unit is created in the Move phase of the tick the timer runs out
        SpawnerEmission::MovePhaseImmediate => start,
        // one tick to queue it, one more for the queue to drain
        SpawnerEmission::SpawnPhaseNextTick => start + 1,
    }
}

/// The appearance post-ticks of `waves` waves of `sp` whose first unit lands at `m1`:
/// the units of a wave `SpawnInterval` apart (a blank interval: all on one tick), the
/// next wave one pause after the LAST unit (the shipped PauseAnchor::AfterLastUnit).
fn wave_ticks(sp: &royalesim::card::SpawnerDef, m1: u32, waves: u32) -> Vec<u32> {
    let step = ticks_of(sp.interval_ms);
    let mut out = Vec::new();
    let mut base = m1;
    for _ in 0..waves {
        for j in 0..sp.number as u32 {
            out.push(base + j * step);
        }
        base = *out.last().unwrap() + ticks_of(sp.pause_time_ms);
    }
    out
}

/// The first simulable card of `prefer` whose spawner block satisfies `ok` -- the
/// data decides which row exercises a shape (2018 and 15.535 differ: the reworked
/// Goblin Hut and the Furnace spawn from action graphs the loader refuses, the
/// 15.535 Witch's wave is instantaneous where the 2018 one was 300 ms apart, the
/// 15.535 Tombstone's is two Skeletons 500 ms apart where the 2018 one was one).
fn spawner_card(s: &BattleState, prefer: &[&'static str], ok: impl Fn(&royalesim::card::SpawnerDef) -> bool) -> &'static str {
    let db = s.cards();
    prefer
        .iter()
        .copied()
        .find(|n| db.index(n).is_some_and(|i| db.get(i).spawner.is_some_and(|sp| ok(&sp))))
        .unwrap_or_else(|| panic!("no simulable card among {prefer:?} has the spawner shape this test needs"))
}

/// A hut with a column spawner (2018: the Goblin Hut; 15.535: the Barbarian Hut).
fn plain_hut(s: &BattleState) -> &'static str {
    spawner_card(s, &["GoblinHut", "BarbarianHut"], |_| true)
}

/// A card whose wave is TIMED: several units, SpawnInterval apart (2018: the Witch;
/// 15.535: the Tombstone).
fn timed_wave_card(s: &BattleState) -> &'static str {
    spawner_card(s, &["Witch", "Tombstone", "BarbarianHut"], |sp| sp.number >= 2 && sp.interval_ms > 0)
}

/// Run `n` ticks and record, per unit of `card` on `team` NOT already alive, the
/// post-tick index it first existed at (in order of appearance).
fn first_seen(s: &mut BattleState, n: u32, team: Team, card: &str) -> Vec<(u32, EntityId)> {
    let mut seen: BTreeSet<(u32, u32)> = find_live(s, team, card).iter().map(|e| (e.id.index, e.id.generation)).collect();
    let mut out = Vec::new();
    for _ in 0..n {
        s.tick();
        for e in find_live(s, team, card) {
            if seen.insert((e.id.index, e.id.generation)) {
                out.push((s.tick_count(), e.id));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// (1), (2): the huts

fn hut_spawns_on_the_predicted_ticks(hut: &str) {
    let mut s = bare(config());
    let sp = spawner(&s, hut);
    let unit = unit_name(&s, sp.unit);
    assert!(sp.start_time_ms.is_none(), "data: {hut} has a blank SpawnStartTime");
    let pos = blue_spot(&s);
    let hut_id = s.scenario_spawn_now(Team::Blue, hut, pos, None).unwrap();
    let hut_r = s.entity(hut_id).unwrap().radius;
    let unit_r = s.cards().get(sp.unit).collision_radius;
    let m1 = first_wave_tick(&sp);
    let period = ticks_of(sp.pause_time_ms);
    // Up to three waves within the hut's LifeTime (the 15.535 Barbarian Hut's 14 s
    // pause fits two in its 30 s): one unit per wave in the 2018 rows, the 15.535
    // Tombstone's two Skeletons 500 ms apart, the Barbarian Hut's three -- the data's
    // own shape.
    let life = card_stat(&s, hut).lifetime_ms.map_or(u32::MAX, ticks_of);
    let waves = (1..=3).rev().find(|&w| wave_ticks(&sp, m1, w).last().copied().unwrap() + 2 < life).unwrap_or(1);
    assert!(waves >= 2, "scene: {hut} fits only {waves} wave(s) in its lifetime");
    let want = wave_ticks(&sp, m1, waves);
    // The unit is CREATED in the pass, never queued (spawner.EMISSION_TIMING =
    // move_phase_immediate); under the old arm it was PENDING one post-tick
    // before it existed (the whole wave when its interval is blank, its first unit
    // otherwise), which is what the branch below still pins.
    let first_batch = if sp.interval_ms > 0 { 1 } else { sp.number as usize };
    let pending = |s: &BattleState| s.pending_spawns().iter().filter(|(t, c, _)| *t == Team::Blue && *c == sp.unit).count();
    for _ in 0..m1 - 1 {
        s.tick();
    }
    assert!(find_live(&s, Team::Blue, &unit).is_empty(), "no {unit} before post-tick {m1}");
    let queued_arm = calib().spawner_emission_timing == SpawnerEmission::SpawnPhaseNextTick;
    assert_eq!(pending(&s), if queued_arm { first_batch } else { 0 }, "post-tick {}: what the pass left behind", m1 - 1);
    s.tick();
    assert_eq!(pending(&s), 0, "post-tick {m1}: nothing of this wave is still pending");
    let mut got = vec![(s.tick_count(), find_live(&s, Team::Blue, &unit).first().expect("the first unit exists at post-tick m1").id)];
    // The first unit, as it materialised: Blue, the hut's level, in front (at the
    // spawn point plus the first capped separation push), on dry ground, tagged
    // with its spawner.
    let (_, id) = got[0];
    let u = s.entity(id).unwrap();
    assert_eq!(u.team, Team::Blue);
    assert_eq!(u.kind, EntityKind::Troop);
    assert_eq!(s.cards().get(u.card_idx).name, unit);
    assert_eq!(u.spawned_by, Some(hut_id));
    assert!(s.arena().is_passable_ground(u.pos), "{unit} at {:?} is on water", u.pos);
    let lvl = s.config().card_level[Team::Blue as usize];
    let hp_at_level = s.cards().scaled(sp.unit, lvl, s.cards().get(sp.unit).hitpoints).unwrap();
    assert_eq!(u.max_hp, hp_at_level, "the {unit} is at the hut's unified level {lvl}");
    // "In front": the owner's forward axis is +y for Blue (spell.rs forward_dy).
    // The unit is deploying, so nothing but the separation push has moved it: it
    // materialises AT the spawn point (centre + hut radius, inside the hut's
    // circle) and the 16.402 contact law (move16402.rs separation_scan /
    // move_towards: the mean push capped at 150 native per tick, deploying units
    // included) slides it out along the same axis over the next ticks until the
    // two circles no longer touch. (The engine's own earlier static pass put it
    // edge to edge in the materialisation tick; that pass is retired.)
    assert_eq!(calib().spawner_spawn_point, SpawnPoint::InFrontAtOwnRadius);
    let cap = 150 * SUBTILE_PER_MILLITILE;
    let spawn_point = Vec2::new(pos.x, pos.y + hut_r);
    assert_eq!(u.pos.x, spawn_point.x, "the {unit} left the {hut}'s forward axis");
    // Under the shipped emission the unit is created in the MOVE phase, after that
    // tick's contact pass, so it materialises exactly AT the spawn point and the
    // separation starts sliding it out next tick; under the queued arm it materialised
    // in the Spawn phase and had taken one capped push by the time it was first seen.
    let slack = if queued_arm { cap } else { 0 };
    assert!(u.pos.y >= spawn_point.y && u.pos.y - spawn_point.y <= slack, "the {unit} stands just in front of the {hut}: the spawn point plus at most one capped push ({:?} vs {spawn_point:?})", u.pos);
    // spawner.SPAWNED_DEPLOY_TIME: the game's emitted unit is born WALKING, with no
    // deploy timer at all (measured on 1230 live emissions); the other arm gave it the
    // unit's own DeployTime, less the countdown that had already run on its spawn tick
    // under the queued emission (match.TICK_ORDER).
    match calib().spawner_spawned_deploy_time {
        SpawnedDeploy::Zero => assert!(!u.deploying && u.deploy_ms == 0, "zero: the {unit} is born walking, not deploying ({} ms left)", u.deploy_ms),
        SpawnedDeploy::UnitOwnDeployTime => {
            assert!(u.deploying, "a spawned unit takes its own DeployTime");
            let ran = if queued_arm { spawn_tick_countdown(&calib()) } else { 0 };
            assert_eq!(u.deploy_ms, s.cards().get(sp.unit).deploy_time_ms - ran);
        }
    }
    {
        // On a clone (the cadence below counts from here): clear of the footprint,
        // edge to edge or beyond, within ceil(unit_r / cap) + 1 further ticks -- the
        // slide never waits for the deploy timer, and it runs whether or not there is
        // one (the contact law includes deploying units).
        let mut probe = s.clone();
        let more = (unit_r + cap - 1) / cap + 1;
        for _ in 0..more {
            probe.tick();
        }
        let v = probe.entity(id).unwrap();
        assert!(v.pos.y >= pos.y + hut_r + unit_r, "post-tick {}: the {unit} is still inside the {hut}'s circle ({:?})", probe.tick_count(), v.pos);
    }
    // The rest of the first wave and the next two waves, on the tick.
    got.extend(first_seen(&mut s, want.last().unwrap() - m1 + 2, Team::Blue, &unit));
    let ticks: Vec<u32> = got.iter().map(|(k, _)| *k).collect();
    assert_eq!(ticks, want, "{hut}: {unit} first-appearance ticks ({} per wave {} ms apart, pause {} ms = {period} ticks)", sp.number, sp.interval_ms, sp.pause_time_ms);
}

#[test]
fn tombstone_spawns_its_wave_of_skeletons_per_pause_on_the_predicted_ticks() {
    // Plant spawner_never_fires: no Skeleton ever exists.
    hut_spawns_on_the_predicted_ticks("Tombstone");
}

#[test]
fn a_hut_spawns_its_wave_per_pause_on_the_predicted_ticks() {
    // The 2018 Goblin Hut (one Spear Goblin per pause); in 15.535 the reworked hut
    // spawns from an action graph the loader refuses, and the Barbarian Hut (three
    // Barbarians 500 ms apart per pause) stands in.
    let hut = plain_hut(&bare(config()));
    hut_spawns_on_the_predicted_ticks(hut);
}

// ---------------------------------------------------------------------------
// (3): a timed wave

#[test]
fn witch_spawns_a_wave_of_skeletons_interval_apart_starting_start_time_after_activation() {
    // Plant spawner_never_fires: no Skeleton.
    let mut s = bare(config());
    let sp = spawner(&s, "Witch");
    let unit = unit_name(&s, sp.unit);
    // 2018: 3 Skeletons 300 ms apart; 15.535: 4 on one tick (a blank SpawnInterval).
    assert!(sp.number >= 2, "data: the Witch's wave is several units ({} units {} ms apart)", sp.number, sp.interval_ms);
    let start = sp.start_time_ms.expect("data: the Witch has a SpawnStartTime");
    assert_eq!(calib().spawner_pause_anchor, PauseAnchor::AfterLastUnit, "the shipped arm this test pins");
    let pos = blue_spot(&s);
    let witch = s.scenario_spawn_now(Team::Blue, "Witch", pos, None).unwrap();
    assert_eq!(first_wave_tick(&sp), ticks_of(start).max(1) + if calib().spawner_emission_timing == SpawnerEmission::SpawnPhaseNextTick { 1 } else { 0 }, "the start time decides");
    let m1 = first_wave_tick(&sp);
    // Two waves: the second one pause after the LAST unit of the first.
    let want = wave_ticks(&sp, m1, 2);
    let seen = first_seen(&mut s, *want.last().unwrap() + 2, Team::Blue, &unit);
    let ticks: Vec<u32> = seen.iter().map(|(k, _)| *k).collect();
    assert_eq!(ticks, want, "Witch: {unit} first-appearance ticks");
    for (_, id) in &seen {
        if let Some(u) = s.entity(*id) {
            assert_eq!(u.spawned_by, Some(witch));
        }
    }
    // Vacuity: the Witch is alive and the wave state is readable through the view.
    let w = s.entity(witch).expect("the Witch survives the scene");
    assert!(w.spawn_ms > 0 && w.spawn_ms <= sp.pause_time_ms, "spawn_ms {} is a countdown toward the next wave", w.spawn_ms);
}

// ---------------------------------------------------------------------------
// (4), (5), (6): death spawn

/// Kill `victim` with debug_set_hp before tick k; it dies in tick k (Resolve, then
/// Reap queues its units); its units exist from post-tick k + 1.
/// A death spawn's members sit ON a ring of `radius` around the death point
/// (calibration spawner.DEATH_SPAWN_LAYOUT = facing_ring, measured on the corpus's
/// two Battle Ram deaths: the two Barbarians at +-600 = DeathSpawnRadius on
/// the axis from the death point to the tower the ram was hitting).
///
/// The members are POSITIONS AS THE ENGINE LAID THEM: the caller reads them off the
/// spawn queue, not off the entities a tick later, because a death spawn with no
/// deploy timer (spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT = zero) walks in the very
/// tick it materialises in.
///
/// The ring's centre is read back as the members' CENTROID -- exact for a ring of any
/// count -- because the caller's `death_pos` is the victim's position before the tick
/// it died in, and a victim that was walking (and being pushed off its neighbours)
/// moved once more inside it; that gap is checked loosely and the ring itself
/// exactly. The per-member tolerance is the sine table's rounding, about a
/// thousandth of the radius.
///
/// With `axis`, the ring's base direction is pinned too: a ring of TWO must lie along
/// it, one member ahead and one behind, which is the law's own claim.
fn assert_on_the_ring(members: &[Vec2], death_pos: Vec2, radius: i32, axis: Option<Vec2>) {
    assert!(members.len() >= 2, "vacuous: a ring of {}", members.len());
    let n = members.len() as i32;
    let centre = Vec2::new(members.iter().map(|m| m.x).sum::<i32>() / n, members.iter().map(|m| m.y).sum::<i32>() / n);
    let drift = royalesim::fixed::isqrt(centre.sub(death_pos).len2()) as i32;
    assert!(drift <= radius / 4, "the ring's centre {centre:?} is {drift} subtiles off the death point {death_pos:?} (more than the victim's last step)");
    let tol = (radius / 100).max(4);
    let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
    for m in members {
        let d = royalesim::fixed::isqrt(m.sub(centre).len2()) as i32;
        assert!((d - radius).abs() <= tol, "a member sits {d} subtiles from the ring's centre, radius {radius} (tolerance {tol})");
        assert!(seen.insert((m.x, m.y)), "two members on one point");
    }
    // equally spaced: no two members closer than the chord of 360 / n degrees, of
    // which the smallest (n = 6) is the radius itself
    for a in members {
        for b in members {
            if (a.x, a.y) < (b.x, b.y) {
                let d = royalesim::fixed::isqrt(a.sub(*b).len2()) as i32;
                assert!(d >= radius - tol, "two members are {d} subtiles apart on a ring of {radius} with {n} on it");
            }
        }
    }
    if let Some(v) = axis {
        assert_eq!(members.len(), 2, "the axis check is written for a ring of two");
        let o = members[0].sub(centre);
        let cross = (o.x as i64) * (v.y as i64) - (o.y as i64) * (v.x as i64);
        let vlen = royalesim::fixed::isqrt(v.len2()).max(1);
        // |cross| / |v| is the member's distance from the axis line
        let off_axis = (cross.abs() / vlen) as i32;
        assert!(off_axis <= tol, "the pair's axis is {off_axis} subtiles off the direction to the target; the ring is not laid on the facing");
        let dot = (o.x as i64) * (v.x as i64) + (o.y as i64) * (v.y as i64);
        let other = members[1].sub(centre);
        let dot2 = (other.x as i64) * (v.x as i64) + (other.y as i64) * (v.y as i64);
        assert!(dot.signum() * dot2.signum() < 0, "both members are on the same side of the death point");
    }
}

/// The positions the spawn queue holds for `card`'s units on Blue -- where a death
/// spawn was LAID, before the tick that materialises it lets it walk.
fn queued_points(s: &BattleState, unit: u16) -> Vec<Vec2> {
    s.pending_spawns().iter().filter(|(t, c, _)| *t == Team::Blue && *c == unit).map(|(_, _, p)| *p).collect()
}

fn kill_and_settle(s: &mut BattleState, victim: EntityId) -> (u32, Vec2) {
    let pos = s.entity(victim).unwrap().pos;
    assert!(s.debug_set_hp(victim, 0));
    s.tick();
    let k = s.tick_count();
    assert!(s.entity(victim).is_none(), "the victim died in tick {k}");
    (k, pos)
}

#[test]
fn golem_killed_leaves_two_golemites_on_the_radius_next_tick_and_its_death_damage_lands() {
    // Plant death_spawn_dropped: no Golemite.
    let mut s = bare(config());
    let ds = death_spawn(&s, "Golem");
    let unit = unit_name(&s, ds.unit);
    let radius = ds.radius.expect("data: the Golem has a DeathSpawnRadius");
    let golem_card = card_stat(&s, "Golem").clone();
    assert!(golem_card.death_damage > 0 && golem_card.death_damage_radius > 0, "data: the Golem has death damage");
    assert!(ds.deploy_time_ms.is_none(), "data: the Golem's death spawn takes the unit's own deploy time");
    assert_eq!(calib().death_spawn_deploy_default, DeathSpawnDeploy::Zero, "the shipped arm this test pins");
    let pos = blue_spot(&s);
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", pos, None).unwrap();
    // A Red Knight inside the death damage radius, deploying so it stands still.
    let knight_at = Vec2::new(pos.x + golem_card.death_damage_radius / 2, pos.y);
    s.spawn_unit(Team::Red, "Knight", knight_at, None).unwrap();
    s.tick();
    let knight = find_live(&s, Team::Red, "Knight")[0].id;
    let full = s.entity(knight).unwrap().hp;
    let (k, death_pos) = kill_and_settle(&mut s, golem);
    assert!(find_live(&s, Team::Blue, &unit).is_empty(), "post-tick {k}: the {unit}s are queued, not yet in the world");
    let laid = queued_points(&s, ds.unit);
    assert_eq!(laid.len(), ds.count as usize);
    s.tick();
    let pups = find_live(&s, Team::Blue, &unit);
    assert_eq!(pups.len(), ds.count as usize, "post-tick {}: {} {unit}s", k + 1, ds.count);
    let lvl = s.config().card_level[Team::Blue as usize];
    // the ring only: this scene pushes the Golem off its own facing (a Red Knight
    // stands inside its radius for the death damage), and the AXIS is pinned on the
    // clean scene below, the one the corpus measured
    assert_on_the_ring(&laid, death_pos, radius, None);
    for p in &pups {
        assert!(s.arena().is_passable_ground(p.pos));
        // spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT = zero (measured: every live death
        // spawn whose row leaves the column blank is born moving).
        assert_eq!((p.deploy_ms, p.deploying), (0, false), "a blank DeathSpawnDeployTime is no deploy timer");
        assert_eq!(p.max_hp, s.cards().scaled(ds.unit, lvl, s.cards().get(ds.unit).hitpoints).unwrap(), "the Golem's level");
        assert_eq!(p.spawned_by, None, "a death spawn owes nothing to a periodic spawner");
    }
    assert_ne!(laid[0], laid[1], "two units, two points");
    // BOTH effects: the death damage landed on the Knight (buffered in Reap of tick
    // k, applied in Resolve of tick k + 1).
    let golem_idx = s.cards().index("Golem").unwrap();
    let dd = s.cards().scaled(golem_idx, lvl, golem_card.death_damage).unwrap();
    assert_eq!(s.entity(knight).unwrap().hp, full - dd, "the Golem's death damage {dd} landed");
}

#[test]
fn lava_hound_killed_leaves_six_flying_pups() {
    let mut s = bare(config());
    let ds = death_spawn(&s, "LavaHound");
    let unit = unit_name(&s, ds.unit);
    let radius = ds.radius.expect("data: the Lava Hound has a DeathSpawnRadius");
    assert!(s.cards().get(ds.unit).is_flying(), "data: the pups fly");
    let pos = blue_spot(&s);
    let hound = s.scenario_spawn_now(Team::Blue, "LavaHound", pos, None).unwrap();
    let (_, death_pos) = kill_and_settle(&mut s, hound);
    let laid = queued_points(&s, ds.unit);
    s.tick();
    let pups = find_live(&s, Team::Blue, &unit);
    assert_eq!(pups.len(), ds.count as usize);
    assert_on_the_ring(&laid, death_pos, radius, None);
    for p in &pups {
        assert!(p.flying, "a {unit} flies");
    }
    assert_eq!(laid.iter().map(|p| (p.x, p.y)).collect::<BTreeSet<_>>().len(), laid.len(), "six distinct points");
}

#[test]
fn a_battle_rams_barbarians_land_on_the_axis_to_the_tower_it_hit() {
    // THE MEASURED CASE (calibration spawner.DEATH_SPAWN_LAYOUT = facing_ring): the
    // corpus's two Battle Ram deaths put the two Barbarians at +-DeathSpawnRadius on
    // the axis from the death point to the tower the ram was hitting -- capture
    // 20260920-003751 t1153: the ram at (6635, 27823) with the king tower at 26.5
    // degrees, its Barbarians at (+539, +263) and (-539, -263), i.e. +-597 along
    // that axis and 54 across; capture 20260920-002736 t5033 the same at 83.7
    // degrees. The old grid arm laid them across the facing instead, 300-500 native
    // off each.
    //
    // The ram is the clean scene: it dies on its own hit (combat.KAMIKAZE_DEATH), so
    // it is standing still in the tick it dies in and the death point is exact.
    let mut s = bare(config());
    let ds = death_spawn(&s, "BattleRam");
    let unit = unit_name(&s, ds.unit);
    let radius = ds.radius.expect("data: the Battle Ram has a DeathSpawnRadius");
    assert!(card_stat(&s, "BattleRam").kamikaze, "data: the Battle Ram is a Kamikaze");
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", blue_spot(&s), None).unwrap();
    let mut aim = None;
    let mut death_pos = None;
    for _ in 0..600 {
        let seen = s.entity(ram).map(|e| (e.pos, e.target));
        s.tick();
        if s.entity(ram).is_none() {
            let (pos, target) = seen.expect("the ram existed the tick before it died");
            death_pos = Some(pos);
            aim = target.map(|t| s.entity(t).unwrap().pos.sub(pos));
            break;
        }
    }
    let (death_pos, aim) = (death_pos.expect("the ram never died"), aim.expect("the ram died without a target"));
    let laid = queued_points(&s, ds.unit);
    s.tick();
    let barbs = find_live(&s, Team::Blue, &unit);
    assert_eq!(barbs.len(), ds.count as usize, "the ram's death spawn");
    assert_on_the_ring(&laid, death_pos, radius, Some(aim));
}

#[test]
fn tombstone_expiring_by_lifetime_leaves_its_death_spawn_skeletons() {
    // Plant death_spawn_dropped: no burst after the expiry.
    let mut s = bare(config());
    let ds = death_spawn(&s, "Tombstone");
    let sp = spawner(&s, "Tombstone");
    let unit = unit_name(&s, ds.unit);
    assert_eq!(sp.unit, ds.unit, "data: the Tombstone's two blocks name one unit");
    let life = card_stat(&s, "Tombstone").lifetime_ms.expect("data: the Tombstone has a LifeTime");
    assert!(ds.radius.is_none(), "data: the Tombstone's death radius is blank");
    assert_eq!(calib().death_spawn_radius_default, DeathSpawnRadius::OwnCollisionRadius, "the shipped arm this test pins");
    let pos = blue_spot(&s);
    let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
    let tomb_r = s.entity(tomb).unwrap().radius;
    // lifetime.HP_DECAY = linear_drain: the LifeTime is bled out of the hp pool, so the
    // Tombstone dies when the pool empties -- ceil(max_hp x 100 / drain) ticks, one or
    // two past ticks_of(LifeTime), never before it.
    let drain = s.lifetime_drain(tomb);
    let want = if drain > 0 { (s.entity(tomb).unwrap().max_hp * 100 + drain - 1) / drain } else { ticks_of(life) as i32 } as u32;
    assert!(want >= ticks_of(life), "the drain cannot kill the Tombstone before its own LifeTime");
    let died = run_until(&mut s, want + 5, |s| s.entity(tomb).is_none());
    assert_eq!(died, want, "the Tombstone expired at post-tick {died}, want {want}");
    let before: BTreeSet<(u32, u32)> = find_live(&s, Team::Blue, &unit).iter().map(|e| (e.id.index, e.id.generation)).collect();
    // As LAID (the burst has no deploy timer and walks in the tick it materialises in).
    let laid = queued_points(&s, ds.unit);
    s.tick();
    let burst: Vec<_> = find_live(&s, Team::Blue, &unit).into_iter().filter(|e| !before.contains(&(e.id.index, e.id.generation))).collect();
    assert_eq!(burst.len(), ds.count as usize, "the expiry left {} {unit}s at once", ds.count);
    assert_eq!(laid.len(), ds.count as usize, "the burst was queued as one batch");
    for b in &laid {
        assert!(b.dist2(pos) <= (tomb_r as i64) * (tomb_r as i64), "{unit} at {b:?} outside the Tombstone's own radius");
        assert!(s.arena().is_passable_ground(*b));
    }
    // And the periodic cadence ended with the building: no further unit for a pause.
    let more = first_seen(&mut s, ticks_of(sp.pause_time_ms) + 2, Team::Blue, &unit);
    assert!(more.is_empty(), "a dead Tombstone kept spawning: {more:?}");
}

// ---------------------------------------------------------------------------
// (7): stun

#[test]
fn a_zap_on_a_tombstone_delays_its_next_wave_by_exactly_the_stun() {
    // Plant spawner_never_fires: neither battle spawns.
    assert!(calib().spawner_stun_pauses, "the shipped arm this test pins");
    assert_eq!(calib().buff_expiry, BuffExpiry::CeilFromNextTick, "the tick alignment the delay is derived under");
    let zap = card_stat(&bare(config()), "Zap").spell.clone().unwrap();
    let stun_ms = match zap.shape {
        royalesim::card::SpellShape::AreaEffect { hit } => hit.buff.map(|b| b.time_ms).unwrap_or(0),
        other => panic!("Zap is not an area effect: {other:?}"),
    };
    assert!(stun_ms > 0, "data: Zap stuns");
    let held = ticks_of(stun_ms);
    let run = |cfg: BattleConfig, zap_at: Option<u32>| -> (Vec<u32>, i32) {
        let mut s = bare(cfg);
        let sp = spawner(&s, "Tombstone");
        let unit = unit_name(&s, sp.unit);
        let pos = blue_spot(&s);
        let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
        let mut seen = Vec::new();
        let mut known: BTreeSet<(u32, u32)> = BTreeSet::new();
        let horizon = first_wave_tick(&sp) + 2 * ticks_of(sp.pause_time_ms) + held + 2;
        for k in 0..horizon {
            if zap_at == Some(k) {
                s.spawn_unit(Team::Red, "Zap", pos, None).unwrap();
            }
            s.tick();
            for e in find_live(&s, Team::Blue, &unit) {
                if known.insert((e.id.index, e.id.generation)) {
                    seen.push(s.tick_count());
                }
            }
        }
        (seen, s.entity(tomb).expect("the Tombstone survives a Zap").hp)
    };
    let (control, full) = run(config(), None);
    assert!(control.len() >= 3, "vacuous: control waves {control:?}");
    let zap_tick = control[0] + 5;
    let (stunned, hp) = run(config(), Some(zap_tick));
    assert!(hp < full, "the Zap hit the Tombstone");
    assert_eq!(stunned[0], control[0], "the wave before the Zap is untouched");
    assert_eq!(&stunned[1..], control[1..].iter().map(|k| k + held).collect::<Vec<_>>().as_slice(), "every later wave is late by exactly the stun's {held} ticks");
    // The other arm: the timer runs through the stun.
    let (through, _) = run(with_calib(|c| c.spawner_stun_pauses = false), Some(zap_tick));
    assert_eq!(through, control, "spawner.STUN_PAUSES_SPAWNER = false: the cadence ignores the stun");
}

// ---------------------------------------------------------------------------
// (8): SpawnLimit (constructed: no 2018 row sets it)

const LIMITED_CARDS: &str = r#"{ "version": "test", "cards": [
 { "name":"Nest", "kind":"building", "elixir":3, "rarity":"Common",
   "hitpoints":1000, "hit_speed_ms":10000, "collision_radius_milli":1000, "deploy_time_ms":1000, "lifetime_ms":60000,
   "spawner": { "character":"Statue", "number":1, "interval_ms":null, "start_time_ms":null, "pause_time_ms":500, "limit":2, "radius_milli":null } }
], "units": {
 "Statue": { "name":"Statue", "source_table":"characters", "rarity":"Common", "hitpoints":100, "damage":1,
   "hit_speed_ms":1000, "load_time_ms":500, "speed":0, "range_milli":500, "sight_range_milli":5500,
   "collision_radius_milli":500, "mass":1, "deploy_time_ms":1000 }
} }"#;

#[test]
fn spawn_limit_caps_a_spawners_live_units_and_a_death_frees_a_place_at_the_next_wave() {
    // Plant spawner_never_fires: no Statue.
    let db = CardDb::from_json_str(LIMITED_CARDS, CardSource::DerivedJson).unwrap();
    let nest = db.index("Nest").unwrap_or_else(|| panic!("Nest rejected: {:?}", db.rejected));
    let sp = db.get(nest).spawner.unwrap();
    assert_eq!(sp.limit, Some(2));
    let statue = sp.unit;
    assert!(db.get(statue).summon_only);
    let mut s = bare(BattleConfig::with_cards(db));
    let pos = blue_spot(&s);
    let nest_id = s.scenario_spawn_now(Team::Blue, "Nest", pos, None).unwrap();
    let period = ticks_of(sp.pause_time_ms);
    let m1 = first_wave_tick(&sp);
    // Five waves' worth of ticks: never more than the limit alive or queued.
    for k in 0..m1 + 5 * period {
        s.tick();
        let live = find_live(&s, Team::Blue, "Statue").len();
        let queued = s.pending_spawns().iter().filter(|(_, c, _)| *c == statue).count();
        assert!(live + queued <= 2, "post-tick {}: {live} live + {queued} queued Statues over the limit", k + 1);
    }
    let live = find_live(&s, Team::Blue, "Statue");
    assert_eq!(live.len(), 2, "the limit is reached, not undershot");
    assert!(live.iter().all(|e| e.spawned_by == Some(nest_id)));
    // A death frees a place; the cadence was kept (spawner.LIMIT_RULE), so the refill
    // comes at the next scheduled wave, not at once. The wave ticks are m1 + k *
    // period FROM THE DATA (not read off the Nest's own timer): the
    // loop ended on the k = 5 wave (skipped at the limit), so the refill is k = 6.
    let victim = live[0].id;
    let next_wave = m1 + 6 * period;
    assert!(s.debug_set_hp(victim, 0));
    s.tick();
    assert_eq!(find_live(&s, Team::Blue, "Statue").len(), 1);
    let refilled = run_until(&mut s, period + 2, |s| find_live(s, Team::Blue, "Statue").len() == 2);
    assert!(refilled < period + 2, "never refilled");
    assert_eq!(s.tick_count(), next_wave, "the refill came with the next scheduled wave (m1 {m1} + 6 x {period})");
}

// ---------------------------------------------------------------------------
// (9): mirror symmetry

#[test]
fn a_symmetric_tombstone_and_golem_scene_stays_its_own_mirror_every_tick() {
    // Under `symmetric_config` (the trace-fitted search): the shipped search is the
    // game's absolute-grid one and is not seat-symmetric (tests/common
    // `symmetric_config`; mirror.rs `the_shipped_search_is_absolute_grid_not_seat_symmetric`),
    // so every seat-symmetry gate measures the OTHER systems under this config.
    let mut s = bare(symmetric_config());
    let tomb_at = blue_spot(&s);
    let golem_at = Vec2::new(tomb_at.x - 4 * royalesim::fixed::SUBTILE, tomb_at.y + 2 * royalesim::fixed::SUBTILE);
    let m_tomb = mirror(&s, tomb_at);
    let m_golem = mirror(&s, golem_at);
    // Red's spawns listed first: the batch orders canonically, not by list order.
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Golem", m_golem, None), (Team::Red, "Tombstone", m_tomb, None), (Team::Blue, "Tombstone", tomb_at, None), (Team::Blue, "Golem", golem_at, None)])
        .unwrap();
    check_mirror(&s).unwrap();
    let mut skeletons_seen = false;
    let mut golemites_seen = false;
    for k in 0..240 {
        if k == 90 {
            // Both Golems die on the same tick: the death spawns must mirror too.
            assert!(s.debug_set_hp(ids[0], 0) && s.debug_set_hp(ids[3], 0));
        }
        s.tick();
        check_mirror(&s).unwrap_or_else(|e| panic!("{e}\ncensus: {:?}", census(&s)));
        skeletons_seen |= !find_live(&s, Team::Blue, "Skeleton").is_empty();
        golemites_seen |= !find_live(&s, Team::Blue, "Golemite").is_empty();
    }
    assert!(skeletons_seen && golemites_seen, "vacuous: skeletons {skeletons_seen}, golemites {golemites_seen}");
}

// ---------------------------------------------------------------------------
// (10): save / load

#[test]
fn a_snapshot_mid_countdown_and_mid_wave_resumes_hash_for_hash() {
    // Plant spawner_never_fires: the resumed battle never spawns (the test then
    // fails on vacuity, which is the point).
    let mut s = bare(config());
    // The timed-wave card (2018: the Witch; 15.535: the Tombstone) beside a companion
    // spawner whose units are its own.
    let timed = timed_wave_card(&s);
    let companion = if timed == "Witch" { "Tombstone" } else { "Witch" };
    let sp = spawner(&s, timed);
    let unit = unit_name(&s, sp.unit);
    let pos = blue_spot(&s);
    s.scenario_spawn_now(Team::Blue, companion, pos, None).unwrap();
    let witch = s.scenario_spawn_now(Team::Blue, timed, Vec2::new(pos.x + 3 * royalesim::fixed::SUBTILE, pos.y), None).unwrap();
    // Into its first wave, the rest of it still to come. Under the shipped emission
    // (move_phase_immediate) the unit it has already emitted EXISTS and carries
    // its owner; under spawn_phase_next_tick one unit is PENDING instead, and the blob
    // has to carry that PendingSpawn.owner. Both are pinned.
    let hers = |s: &BattleState| find_live(s, Team::Blue, &unit).into_iter().filter(|e| e.spawned_by == Some(witch)).count();
    let queued_arm = calib().spawner_emission_timing == SpawnerEmission::SpawnPhaseNextTick;
    let pending = |s: &BattleState| s.pending_spawns().iter().filter(|(t, c, _)| *t == Team::Blue && *c == sp.unit).count();
    let mid = run_until(&mut s, 200, |s| {
        let w = s.entity(witch).unwrap();
        w.spawn_wave_left > 0 && w.spawn_wave_left < sp.number && if queued_arm { pending(s) > 0 } else { hers(s) > 0 }
    });
    assert!(mid < 200, "vacuous: {timed} never reached a mid-wave state with a unit of its own");
    let w = s.entity(witch).unwrap();
    assert!(w.spawn_wave_left > 0 && w.spawn_wave_left < sp.number, "vacuous: not mid-wave ({} left)", w.spawn_wave_left);
    if queued_arm {
        assert_eq!(pending(&s), 1, "vacuous: no owned {unit} pending at the save");
        assert_eq!(hers(&s), 0, "vacuous: {timed}'s first {unit} already exists (the companion's are its own)");
    } else {
        assert_eq!(hers(&s), 1, "vacuous: {timed} has not emitted its first {unit} yet (the companion's are its own)");
    }
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap();
    assert_eq!(l.state_hash(), s.state_hash());
    let lw = l.entity(witch).unwrap();
    assert_eq!((lw.spawn_ms, lw.spawn_wave_left), (w.spawn_ms, w.spawn_wave_left));
    let before = find_live(&s, Team::Blue, &unit).len();
    for k in 0..200 {
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
        if k == 0 {
            // Under the queued arm the pending unit materialised in the loaded battle
            // WITH its owner; under the shipped one the emitted unit was already hers
            // and stayed hers across the save.
            assert_eq!(hers(&l), 1, "the loaded battle lost {timed}'s own {unit} (the spawner tag did not survive the save)");
        }
    }
    assert!(find_live(&s, Team::Blue, &unit).len() > before || s.entities().filter(|e| e.card == unit).count() > 0, "vacuous: nothing spawned after the load");
}

// ---------------------------------------------------------------------------
// (11): the scripted battle

#[test]
fn scripted_battle_with_tombstone_and_witch_keeps_every_invariant() {
    // Not a plant target: a dying Tombstone's death spawn puts Skeletons on the board
    // even with the periodic spawner planted out.
    let mut cfg = config();
    cfg.decks = [
        ["Tombstone", "Witch", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Valkyrie"].iter().map(|s| s.to_string()).collect(),
        ["Knight", "Cannon", "Tombstone", "Witch", "Prince", "Wizard", "Giant", "Minions"].iter().map(|s| s.to_string()).collect(),
    ];
    let cap = tick_cap(&cfg);
    let mut s = BattleState::new(0xC1A5, cfg);
    let mut script = Script::new(40);
    let mut inv = Invariants::new(DEFAULT_TOLERANCE);
    let mut skeletons = [false; 2];
    let mut huts = [false; 2];
    while !s.is_done() && s.tick_count() < cap {
        script.step(&mut s);
        s.set_phase_trace(true);
        s.tick();
        if let Err(e) = inv.check(&s) {
            panic!("{e}\ncensus: {:?}", census(&s));
        }
        for team in [Team::Blue, Team::Red] {
            skeletons[team as usize] |= !find_live(&s, team, "Skeleton").is_empty();
            huts[team as usize] |= !find_live(&s, team, "Tombstone").is_empty();
        }
    }
    assert!(script.plays[0] > 0 && script.plays[1] > 0);
    assert_eq!(huts, [true, true], "vacuous: a Tombstone was never on the board");
    assert_eq!(skeletons, [true, true], "vacuous: no Skeleton was ever seen (plant spawner_never_fires)");
    println!("spawner battle: {} ticks, outcome {:?}, worst overlap {}%", s.tick_count(), s.outcome(), inv.worst_pct);
}

// ---------------------------------------------------------------------------
// (12): every candidate moves something

#[test]
fn every_spawner_candidate_moves_a_measurable_behaviour() {
    // FIRST_WAVE: a blank start time waits one pause under the other arm.
    {
        let mut s = bare(with_calib(|c| c.spawner_first_wave = FirstWave::AfterOnePause));
        let sp = spawner(&s, "Tombstone");
        let pos = blue_spot(&s);
        s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
        let seen = first_seen(&mut s, ticks_of(sp.pause_time_ms) + 2, Team::Blue, "Skeleton");
        assert_eq!(seen.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![ticks_of(sp.pause_time_ms)], "first_wave_after_one_pause");
    }
    // START_TIME_ORIGIN: from_placement loads SpawnStartTime less the DeployTime, so a
    // Witch deployed WITH her deploy time (spawn_unit) fires her first wave
    // ceil(DeployTime / TICK_MS) ticks earlier than under from_activation.
    {
        let first_skeleton = |cfg: BattleConfig| -> u32 {
            let mut s = bare(cfg);
            let pos = blue_spot(&s);
            s.spawn_unit(Team::Blue, "Witch", pos, None).unwrap();
            let seen = first_seen(&mut s, 80, Team::Blue, "Skeleton");
            seen.first().expect("the Witch never spawned").0
        };
        let s0 = bare(config());
        let (sp, deploy) = (spawner(&s0, "Witch"), card_stat(&s0, "Witch").deploy_time_ms);
        let start = sp.start_time_ms.expect("data: the Witch ships a SpawnStartTime");
        assert!(deploy > 0 && start >= deploy, "data: Witch DeployTime {deploy} / SpawnStartTime {start} cannot part the arms");
        let activation = first_skeleton(with_calib(|c| c.spawner_start_time_origin = StartTimeOrigin::FromActivation));
        let placement = first_skeleton(with_calib(|c| c.spawner_start_time_origin = StartTimeOrigin::FromPlacement));
        // spawn_unit: materialised in tick 1, deploy over in the Move phase of
        // post-tick ceil(D / TICK_MS), where the spawner pass then decrements once
        // already -- so the first unit lands (start ticks - 1) later, on start
        // (from_activation) or start - D (from_placement).
        let deploy_end = ticks_of(deploy);
        assert_eq!(activation, deploy_end + ticks_of(start).max(1) - 1, "from_activation: {activation}");
        assert_eq!(placement, deploy_end + ticks_of(start - deploy).max(1) - 1, "from_placement: {placement}");
        assert!(placement < activation, "the arms agree on the Witch: {placement} vs {activation}");
    }
    // SPAWN_POINT: at_centre puts the unit ON the spawner's own point, where the
    // shipped arm puts it one spawner radius ahead. Read on the tick it materialises
    // in -- the unit has no deploy timer and walks off on its next one, and the
    // coincident push (move16402.rs separation_scan, `d2 == 0`) starts then too.
    // ~~pushed the whole radius sum in one tick~~ (the retired static pass).
    {
        let first_point = |cfg: BattleConfig| -> (Vec2, Vec2) {
            let mut s = bare(cfg);
            let pos = blue_spot(&s);
            s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
            let m1 = first_wave_tick(&spawner(&s, "Tombstone"));
            let seen = first_seen(&mut s, m1, Team::Blue, "Skeleton");
            (pos, s.entity(seen[0].1).unwrap().pos)
        };
        let (pos, centred) = first_point(with_calib(|c| c.spawner_spawn_point = SpawnPoint::AtCentre));
        assert_eq!(centred, pos, "at_centre: the unit materialises on the spawner's own point");
        let (_, ahead) = first_point(config());
        let tomb_r = {
            let mut s = bare(config());
            let id = s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
            s.entity(id).unwrap().radius
        };
        assert_eq!(ahead, Vec2::new(pos.x, pos.y + tomb_r), "in_front_toward_enemy_at_own_radius: one spawner radius along Blue's forward axis");
    }
    // PAUSE_ANCHOR: the timed-wave card's second wave comes one pause after the
    // FIRST unit (the 2018 Witch; the 15.535 Tombstone -- the 15.535 Witch's wave
    // is instantaneous and cannot part the arms).
    {
        let mut s = bare(with_calib(|c| c.spawner_pause_anchor = PauseAnchor::AfterFirstUnit));
        let timed = timed_wave_card(&s);
        let sp = spawner(&s, timed);
        let unit = unit_name(&s, sp.unit);
        let pos = blue_spot(&s);
        s.scenario_spawn_now(Team::Blue, timed, pos, None).unwrap();
        let m1 = first_wave_tick(&sp);
        let want = m1 + ticks_of(sp.pause_time_ms - (sp.number - 1) * sp.interval_ms) + (sp.number as u32 - 1) * ticks_of(sp.interval_ms);
        let seen = first_seen(&mut s, want + 2, Team::Blue, &unit);
        assert_eq!(seen[sp.number as usize].0, want, "after_first_unit_of_wave: waves {:?}", seen.iter().map(|(k, _)| *k).collect::<Vec<_>>());
        assert!(want < m1 + ticks_of(sp.pause_time_ms) + (sp.number as u32 - 1) * ticks_of(sp.interval_ms), "the arms differ on {timed}");
    }
    // DEATH_SPAWN_RADIUS_DEFAULT = zero: every Tombstone Skeleton is queued ON the point.
    {
        let mut s = bare(with_calib(|c| c.death_spawn_radius_default = DeathSpawnRadius::Zero));
        let pos = blue_spot(&s);
        let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
        s.tick(); // the first periodic Skeleton is queued and out of the way
        let (_, death_pos) = kill_and_settle(&mut s, tomb);
        let queued: Vec<Vec2> = s.pending_spawns().iter().map(|(_, _, p)| *p).collect();
        assert_eq!(queued.len(), death_spawn(&s, "Tombstone").count as usize);
        assert!(queued.iter().all(|p| *p == death_pos), "zero: all on the death point, {queued:?}");
        let mut c = bare(config());
        let tomb = c.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
        c.tick();
        kill_and_settle(&mut c, tomb);
        assert!(c.pending_spawns().iter().any(|(_, _, p)| *p != death_pos), "own_collision_radius: spread around it");
    }
    // DEATH_SPAWN_DEPLOY_TIME_DEFAULT = unit_own_deploy_time (the earlier arm):
    // Golemites serve their own DeployTime instead of walking at once.
    {
        let mut s = bare(with_calib(|c| c.death_spawn_deploy_default = DeathSpawnDeploy::UnitOwnDeployTime));
        let pos = blue_spot(&s);
        let golem = s.scenario_spawn_now(Team::Blue, "Golem", pos, None).unwrap();
        kill_and_settle(&mut s, golem);
        s.tick();
        let g = find_live(&s, Team::Blue, "Golemite");
        assert_eq!(g.len(), 2);
        assert!(g.iter().all(|e| e.deploy_ms > 0 && e.deploying), "unit_own_deploy_time: the Golemites deploy");
    }
    // EMISSION_TIMING = spawn_phase_next_tick (the earlier arm): the first
    // Skeleton is queued, not created, and lands two post-ticks later.
    {
        let sp = spawner(&bare(config()), "Tombstone");
        let first = |cfg: BattleConfig| -> u32 {
            let mut s = bare(cfg);
            let pos = blue_spot(&s);
            s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
            first_seen(&mut s, 10, Team::Blue, "Skeleton").first().expect("no Skeleton").0
        };
        assert!(sp.start_time_ms.is_none(), "data: the Tombstone's SpawnStartTime is blank");
        let now = first(config());
        let queued = first(with_calib(|c| c.spawner_emission_timing = SpawnerEmission::SpawnPhaseNextTick));
        assert_eq!((now, queued), (1, 2), "move_phase_immediate {now} vs spawn_phase_next_tick {queued}");
    }
    // SPAWNED_DEPLOY_TIME = unit_own_deploy_time (the earlier arm): the
    // Tombstone's Skeleton stands still for its own DeployTime instead of walking.
    {
        let mut s = bare(with_calib(|c| c.spawner_spawned_deploy_time = SpawnedDeploy::UnitOwnDeployTime));
        let pos = blue_spot(&s);
        s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
        let seen = first_seen(&mut s, 4, Team::Blue, "Skeleton");
        let u = s.entity(seen[0].1).unwrap();
        assert!(u.deploying && u.deploy_ms > 0, "unit_own_deploy_time: the Skeleton deploys ({} ms)", u.deploy_ms);
    }
    // The single-arm keys are refused for their other candidate.
    let json = |key: &str, from: &str, to: &str| {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/calibration.json")).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["spawner"][key]["value"], from);
        v["spawner"][key]["value"] = serde_json::Value::from(to);
        serde_json::to_string(&v).unwrap()
    };
    // DEATH_SPAWN_LAYOUT = engine_grid_within_radius (the earlier arm): the
    // two Golemites sit on the engine's square grid across the owner's frame, not on
    // the facing axis, so the pair's axis turns by the angle between them.
    {
        let ring = {
            let mut s = bare(config());
            let pos = blue_spot(&s);
            let golem = s.scenario_spawn_now(Team::Blue, "Golem", pos, None).unwrap();
            let (_, death) = kill_and_settle(&mut s, golem);
            s.tick();
            let g = find_live(&s, Team::Blue, "Golemite");
            (g[0].pos.sub(death), g.len())
        };
        let mut s = bare(with_calib(|c| c.death_spawn_layout = DeathSpawnLayout::EngineGridWithinRadius));
        let pos = blue_spot(&s);
        let golem = s.scenario_spawn_now(Team::Blue, "Golem", pos, None).unwrap();
        let (_, death) = kill_and_settle(&mut s, golem);
        s.tick();
        let g = find_live(&s, Team::Blue, "Golemite");
        assert_eq!((g.len(), ring.1), (2, 2), "vacuous: the two arms did not both spawn the pair");
        assert_ne!(g[0].pos.sub(death), ring.0, "engine_grid_within_radius: the same point as the facing ring");
    }
    for (key, from, to) in [("LIMIT_RULE", "skip_unit_keep_cadence", "hold_until_room"), ("DEATH_SPAWN_LAYOUT", "facing_ring", "ring_at_radius")] {
        let err = Calib::from_json(&json(key, from, to)).err().unwrap_or_else(|| panic!("{key} = {to} loaded"));
        assert!(err.contains(key) && err.contains("no engine implementation"), "{key}: {err}");
    }
}

// ---------------------------------------------------------------------------
// (13): the loader

#[test]
fn loader_reads_both_blocks_from_cards_json_shares_the_unit_table_and_rejects_broken_chains() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let db = cards();
    let raw = |n: &str| doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == n).unwrap().clone();
    let int = |v: &serde_json::Value| v.as_i64().map(|x| x as i32);
    // Every card of the file with a periodic-spawner block the loader took (the
    // 2018 Goblin Hut and Furnace; in 15.535 both are refused for their action
    // graphs and their blocks are cleared rows).
    let with_block: Vec<String> = doc["cards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["spawner"]["character"].is_string() && c["spawner"]["number"].is_number() && db.index(c["name"].as_str().unwrap()).is_some())
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert!(with_block.len() >= 4 && ["Tombstone", "Witch", "DarkWitch", "BarbarianHut"].iter().all(|n| with_block.iter().any(|w| w == n)), "vacuous: spawner rows {with_block:?}");
    for hut in with_block.iter().map(String::as_str) {
        let idx = db.index(hut).unwrap_or_else(|| panic!("{hut} rejected: {:?}", db.rejected.iter().find(|(n, _)| n == hut)));
        let sp = db.get(idx).spawner.unwrap_or_else(|| panic!("{hut}: no spawner read"));
        let j = &raw(hut)["spawner"];
        assert_eq!(db.get(sp.unit).name, j["character"].as_str().unwrap());
        assert_eq!(sp.number, int(&j["number"]).unwrap());
        assert_eq!(sp.interval_ms, int(&j["interval_ms"]).unwrap_or(0));
        assert_eq!(sp.start_time_ms, int(&j["start_time_ms"]));
        assert_eq!(sp.pause_time_ms, int(&j["pause_time_ms"]).unwrap());
        assert_eq!(sp.limit, int(&j["limit"]));
        assert_eq!(sp.radius, int(&j["radius_milli"]).map(milli));
    }
    for card in ["Tombstone", "Golem", "LavaHound", "BattleRam", "DarkWitch"] {
        let ds = db.get(db.index(card).unwrap()).death_spawn.unwrap_or_else(|| panic!("{card}: no death_spawn read"));
        let j = &raw(card)["death_spawn"];
        assert_eq!(db.get(ds.unit).name, j["character"].as_str().unwrap());
        assert_eq!(ds.count, int(&j["count"]).unwrap());
        assert_eq!(ds.radius, int(&j["radius_milli"]).map(milli));
        assert_eq!(ds.deploy_time_ms, int(&j["deploy_time_ms"]));
    }
    // ONE UNIT TABLE: the Tombstone, the Witch and Skeletons' own record agree, and the
    // Goblin Barrel's Goblin sits in the same summon-only list.
    let skel = db.get(db.index("Tombstone").unwrap()).spawner.unwrap().unit;
    assert_eq!(db.get(db.index("Witch").unwrap()).spawner.unwrap().unit, skel);
    assert_eq!(db.get(db.index("Tombstone").unwrap()).death_spawn.unwrap().unit, skel);
    assert!(db.get(skel).summon_only && db.get(skel).kind == CardKind::Troop);
    assert_eq!(db.cards.iter().filter(|c| c.summon_only && c.name == "Skeleton").count(), 1, "the Skeleton record is loaded once");
    // A unit that IS a card resolves to that card, not a duplicate: the 2018
    // Furnace's FireSpirits (in 15.535 the reworked Furnace is refused for its
    // action graph and no loaded spawner names a card -- the Goblins card summons
    // Goblin_Stab -- so the rule is checked over every loaded block and holds
    // vacuously there).
    let mut shared = 0;
    for c in db.cards.iter().filter(|c| !c.summon_only) {
        for unit in c.spawner.map(|sp| sp.unit).into_iter().chain(c.death_spawn.map(|ds| ds.unit)) {
            let u = db.get(unit);
            // A playable card of the unit's name (a summon-only record is registered
            // under its name too, so the playable one is the non-summon-only card).
            if let Some(as_card) = db.cards.iter().position(|x| !x.summon_only && x.spell.is_none() && x.name == u.name) {
                assert_eq!(as_card as u16, unit, "{}: its unit {} is a playable card but loaded twice", c.name, u.name);
                assert!(!u.summon_only, "{}: {} is a card and must not be summon-only", c.name, u.name);
                shared += 1;
            }
        }
    }
    if let Some(hut) = db.index("FirespiritHut") {
        assert_eq!(db.get(hut).spawner.unwrap().unit, db.index("FireSpirits").unwrap());
        assert!(shared >= 1);
    }
    // The huts are non-attacking buildings: range 0, no damage, never a target decision.
    let tomb = db.get(db.index("Tombstone").unwrap());
    assert_eq!((tomb.range, tomb.sight_range, tomb.damage), (0, 0, 0));
    // Units the loader cannot run are REJECTED, naming the unit AND the reason: the
    // bottles/containers are hitpoint-less objects, the 2018 BrokenCannon a
    // troop with a LifeTime, the 15.535 Lumberjack's rage a death area effect, the
    // 15.535 Cannon Cart and Skeleton Barrel action graphs. ~~SkeletonBalloon: a
    // chain~~ -- its SkeletonContainer is refused on `hitpoints` first; its
    // `SpawnCharacter = Skeleton` with blank SpawnNumber / SpawnPauseTime is read as
    // NO periodic spawner (the game's own blank), not as a partial block. Each card
    // lists the reasons either vintage's row earns.
    // ~~Balloon, GiantSkeleton~~ -- a DEATH BOMB is no longer refused: a hitpoint-less
    // row with DeployTime + DeathDamage + DeathDamageRadius is a timed impact, not a
    // unit (card.rs `convert_death_bomb`; tests/death_bomb.rs).
    for (card, reasons) in [
        ("RageBarbarian", &["RageBarbarianBottle: missing hitpoints", "death area effect RageBarbarianDummyForSpawn"][..]),
        ("MovingCannon", &["BrokenCannon is a troop with a LifeTime", "action graph"]),
        ("SkeletonBalloon", &["SkeletonContainer: missing hitpoints", "action graph"]),
    ] {
        assert!(db.index(card).is_none(), "{card} must not be simulable");
        let (_, why) = db.rejected.iter().find(|(n, _)| n == card).unwrap_or_else(|| panic!("{card} not listed as rejected"));
        assert!(reasons.iter().any(|r| why.contains(r)), "{card}: {why} (expected one of {reasons:?})");
        assert!(!why.contains("SpawnNumber"), "{card}: the pair-blank spawner block was read as a partial block: {why}");
    }
    // (16) Nothing points at the unresolved unit: a rejected card's blocks are dropped
    // (a format-3 board entity of a rejected card must never index cards[65535]).
    for c in &db.cards {
        assert!(c.spawner.is_none_or(|sp| (sp.unit as usize) < db.cards.len()), "{}: spawner unit unresolved", c.name);
        assert!(c.death_spawn.is_none_or(|ds| (ds.unit as usize) < db.cards.len()), "{}: death spawn unit unresolved", c.name);
        if let Some(royalesim::card::SpellDef { shape: royalesim::card::SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = &c.spell {
            assert!((sp.unit as usize) < db.cards.len(), "{}: spell unit unresolved", c.name);
        }
    }
    for card in ["RageBarbarian", "SkeletonBalloon"] {
        // Rejected after the push (an unloadable unit, a death area effect): in the
        // list, unregistered, its blocks dropped. Rejected in `convert` (an action
        // graph): never pushed at all.
        if let Some(c) = db.cards.iter().find(|c| c.name == card) {
            assert!(c.death_spawn.is_none() && c.spawner.is_none(), "{card}: a rejected card kept a unit block");
        }
    }
    // A partial block is refused with its column named.
    let broken = LIMITED_CARDS.replacen("\"pause_time_ms\":500", "\"pause_time_ms\":null", 1);
    let db2 = CardDb::from_json_str(&broken, CardSource::DerivedJson).unwrap();
    assert!(db2.index("Nest").is_none());
    assert!(db2.rejected[0].1.contains("SpawnPauseTime"), "{:?}", db2.rejected);
    // A deck with a spawner validates the unit's level with the card's.
    let mut cfg = BattleConfig::with_cards(cards());
    cfg.decks[0] = vec!["Tombstone".into()];
    assert!(BattleState::try_new(1, cfg).is_ok());
}

// ---------------------------------------------------------------------------
// (14): the simultaneous wave and the death-spawn deploy override

#[test]
fn a_dark_witch_lands_both_bats_at_once_around_her_spawn_radius_and_a_rams_barbarians_take_the_blocks_deploy_time() {
    // Plant spawner_never_fires: no Bat. The DarkWitch is the one 2018 row with
    // SpawnNumber > 1 and a blank SpawnInterval (both units in ONE pass, gridded
    // around the spawn point) and the one row with a SpawnRadius.
    let mut s = bare(config());
    let sp = spawner(&s, "DarkWitch");
    let unit = unit_name(&s, sp.unit);
    assert!(sp.number > 1 && sp.interval_ms == 0, "data: the DarkWitch wave is not simultaneous ({sp:?})");
    let radius = sp.radius.expect("data: the DarkWitch ships a SpawnRadius");
    let unit_r = s.cards().get(sp.unit).collision_radius;
    assert!(s.cards().get(sp.unit).is_flying(), "data: {unit} flies");
    let pos = blue_spot(&s);
    let witch = s.scenario_spawn_now(Team::Blue, "DarkWitch", pos, None).unwrap();
    let m1 = first_wave_tick(&sp);
    // She walks (a scenario spawn has no deploy time), so the spawn point moves with
    // her: the anchor is her position at the moment the pass ran. Under the shipped
    // emission that is the Move phase of post-tick m1, after her own step; under
    // spawn_phase_next_tick the pass ran in the Spawn phase of post-tick m1 - 2.
    let lag = match calib().spawner_emission_timing {
        SpawnerEmission::MovePhaseImmediate => 0,
        SpawnerEmission::SpawnPhaseNextTick => 2,
    };
    let mut anchor = s.entity(witch).unwrap().pos;
    let mut known: BTreeSet<(u32, u32)> = find_live(&s, Team::Blue, &unit).iter().map(|e| (e.id.index, e.id.generation)).collect();
    // (post-tick, flying, tagged, deploying, position AS IT LANDED): the Bats fly off
    // on their next tick, so nothing here may be read a tick later.
    let mut seen: Vec<(u32, bool, bool, bool, Vec2)> = Vec::new();
    for _ in 0..m1 + 2 {
        s.tick();
        if s.tick_count() + lag == m1 {
            anchor = s.entity(witch).unwrap().pos;
        }
        for e in find_live(&s, Team::Blue, &unit) {
            if known.insert((e.id.index, e.id.generation)) {
                seen.push((s.tick_count(), e.flying, e.spawned_by == Some(witch), e.deploying, e.pos));
            }
        }
    }
    assert_eq!(seen.len(), sp.number as usize, "{unit}s seen: {seen:?}");
    assert!(seen.iter().all(|r| r.0 == m1), "the whole wave lands on post-tick {m1}: {seen:?}");
    assert!(seen.iter().all(|r| r.1 && r.2), "flying and tagged");
    let deploying = calib().spawner_spawned_deploy_time == SpawnedDeploy::UnitOwnDeployTime;
    assert!(seen.iter().all(|r| r.3 == deploying), "spawner.SPAWNED_DEPLOY_TIME: deploying should be {deploying}");
    let bats: Vec<Vec2> = seen.iter().map(|r| r.4).collect();
    assert_ne!(bats[0], bats[1], "the two Bats are on one point");
    // The engine formation grid for two units: one unit diameter apart (spacing
    // 2 x unit_r, offsets -+ unit_r on the owner's lateral axis), centred on the
    // point SpawnRadius ahead. The game's separation may nudge a touching pair by
    // its minimum push (1 native = 18 subtiles) in the materialisation tick.
    let point = Vec2::new(anchor.x, anchor.y + radius);
    let mut xs: Vec<i32> = bats.iter().map(|b| b.x - point.x).collect();
    xs.sort();
    let nudge = royalesim::fixed::SUBTILE_PER_MILLITILE;
    assert!((xs[0] + unit_r).abs() <= nudge && (xs[1] - unit_r).abs() <= nudge, "the Bats are not one diameter apart around the spawn point: {xs:?} vs -+{unit_r}");
    assert!(bats.iter().all(|b| (b.y - point.y).abs() <= nudge), "the Bats are not SpawnRadius ahead: {bats:?} vs {point:?}");
    assert_eq!(s.entity(witch).unwrap().spawn_wave_left, 0, "a simultaneous wave leaves nothing pending");

    // BattleRam: DeathSpawnDeployTime overrides the Barbarian's own DeployTime (the
    // 2018 row: 800 against 1000, which parts the override from the unit's own; the
    // 15.535 row ships 1000 = the Barbarian's, so on the shipped data only the VALUE
    // is pinned and the two readings coincide -- no other 15.535 death spawn ships a
    // DeathSpawnDeployTime at all). So the MECHANISM is pinned on a synthetic row
    // too: the shipped file with the block's deploy time moved
    // 200 ms off the Barbarian's own, where the Barbarians must take the block's.
    let ram_barbarians = |cfg: BattleConfig| -> (i32, i32, Vec<i32>) {
        let mut s = bare(cfg);
        let ds = death_spawn(&s, "BattleRam");
        let own = s.cards().get(ds.unit).deploy_time_ms;
        let block = ds.deploy_time_ms.expect("data: the BattleRam ships a DeathSpawnDeployTime");
        let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", pos, None).unwrap();
        s.tick();
        assert!(s.debug_set_hp(ram, 0));
        let barb = unit_name(&s, ds.unit);
        let barbs = first_seen(&mut s, 3, Team::Blue, &barb);
        assert_eq!(barbs.len(), ds.count as usize, "{barbs:?}");
        let took = barbs
            .iter()
            .map(|(seen_at, id)| {
                let b = s.entity(*id).unwrap();
                // One TICK_MS counted down per tick since it was first seen, plus the
                // spawn tick's own countdown (match.TICK_ORDER).
                b.deploy_ms + dt() * (s.tick_count() - seen_at) as i32 + spawn_tick_countdown(&calib())
            })
            .collect();
        (own, block, took)
    };
    let (own, block, took) = ram_barbarians(config());
    assert!(took.iter().all(|t| *t == block), "the Barbarians took {took:?} instead of the block's {block} (the shipped data: the unit's own is {own})");
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let ram = doc["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "BattleRam").unwrap();
    let moved = own + 200;
    ram["death_spawn"]["deploy_time_ms"] = serde_json::Value::from(moved);
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).unwrap();
    let (own2, block2, took) = ram_barbarians(BattleConfig::with_cards(db));
    assert_eq!((own2, block2), (own, moved), "the synthetic row");
    assert!(took.iter().all(|t| *t == moved), "synthetic DeathSpawnDeployTime {moved}: the Barbarians took {took:?} (their own is {own2}) -- the column is not read");
}

// ---------------------------------------------------------------------------
// (15): a hut never targets

#[test]
fn a_hut_with_goblins_inside_its_footprint_never_takes_a_target() {
    // Range 0 / sight 0 alone was not enough: the edge-to-edge range test adds the
    // target's radius, so a goblin released onto the Tombstone was in range; the
    // loader now forces attacks_ground / attacks_air off on a building with no
    // damage source.
    let mut s = bare(config());
    let tomb = card_stat(&s, "Tombstone").clone();
    assert!(!tomb.attacks_ground && !tomb.attacks_air && tomb.damage == 0 && tomb.projectile.is_none(), "data: {tomb:?}");
    let pos = blue_spot(&s);
    let hut = s.scenario_spawn_now(Team::Blue, "Tombstone", pos, None).unwrap();
    s.spawn_unit(Team::Red, "GoblinBarrel", pos, None).unwrap();
    let landed = run_until(&mut s, 200, |s| !find_live(s, Team::Red, "Goblin").is_empty());
    assert!(landed < 200, "the barrel never released its goblins");
    let inside = find_live(&s, Team::Red, "Goblin").iter().any(|g| g.pos.dist2(pos) <= (tomb.collision_radius as i64 * tomb.collision_radius as i64));
    assert!(inside, "vacuous: no goblin materialised inside the Tombstone's circle");
    for _ in 0..40 {
        let h = s.entity(hut).unwrap();
        assert_eq!(h.target, None, "post-tick {}: the Tombstone took a target", s.tick_count());
        assert_eq!(h.attack_phase, AttackPhase::Idle, "post-tick {}: the Tombstone swung", s.tick_count());
        s.tick();
    }
}

// ---------------------------------------------------------------------------
// (16b): the death-spawn pull-back with a radius smaller than the grid

const TIGHT_DEATH_CARDS: &str = r#"{ "version": "test", "cards": [
 { "name":"Crate", "kind":"building", "elixir":3, "rarity":"Common",
   "hitpoints":1000, "hit_speed_ms":10000, "collision_radius_milli":1000, "deploy_time_ms":0, "lifetime_ms":60000,
   "death_spawn": { "character":"Statue", "count":4, "radius_milli":100, "deploy_time_ms":null } }
], "units": {
 "Statue": { "name":"Statue", "source_table":"characters", "rarity":"Common", "hitpoints":100, "damage":1,
   "hit_speed_ms":1000, "load_time_ms":500, "speed":0, "range_milli":500, "sight_range_milli":5500,
   "collision_radius_milli":500, "mass":1, "deploy_time_ms":1000 }
} }"#;

#[test]
fn a_death_spawn_whose_grid_reaches_past_its_radius_is_pulled_back_onto_it() {
    // Plant death_spawn_dropped: no Statue. Four units of radius 500 grid one
    // diameter apart (offsets -+500 on both axes), but DeathSpawnRadius is 100: every
    // point is pulled back radially onto the radius (state.rs death_spawn_points),
    // so each Statue is queued within 100 native of the death point.
    let db = CardDb::from_json_str(TIGHT_DEATH_CARDS, CardSource::DerivedJson).unwrap();
    let crate_idx = db.index("Crate").unwrap_or_else(|| panic!("Crate rejected: {:?}", db.rejected));
    let ds = db.get(crate_idx).death_spawn.unwrap();
    let unit_r = db.get(ds.unit).collision_radius;
    let r = ds.radius.unwrap();
    assert!(r < unit_r, "vacuous: the radius does not undercut the grid");
    let mut s = bare(BattleConfig::with_cards(db));
    let pos = blue_spot(&s);
    let id = s.scenario_spawn_now(Team::Blue, "Crate", pos, None).unwrap();
    s.tick();
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    // The QUEUED points (the contact law slides the overlapping Statues apart once
    // they materialise, which is not what this test measures).
    let mut at: Vec<Vec2> = s.pending_spawns().iter().filter(|(t, c, _)| *t == Team::Blue && *c == ds.unit).map(|(_, _, p)| *p).collect();
    assert_eq!(at.len(), ds.count as usize, "{at:?}");
    at.sort_by_key(|p| (p.x, p.y));
    at.dedup();
    assert!(at.len() >= 2, "the pull-back collapsed the grid onto one point: {at:?}");
    for p in &at {
        let d2 = p.dist2(pos);
        assert!(d2 <= (r as i64) * (r as i64), "a Statue at {p:?} is {} from the death point, radius {r}", isqrt(d2));
        assert!(d2 > 0, "a Statue sits ON the death point: the pull-back did not run");
    }
}
