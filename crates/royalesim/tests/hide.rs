//! HIDE (Tesla): buildings whose card `hides_when_not_attacking`.
//!
//! WHAT IS PINNED, all from the loaded CardDb and `Calib::shipped()`:
//!   1. a hidden Tesla is never targeted by a building-targeter that can see it but
//!      that it cannot see, and stays under;
//!   2. an enemy walking INTO its sight starts the rise on the next tick, it is up
//!      exactly ceil(UpTimeMs / TICK_MS) ticks later, and its first shot lands on
//!      the first tick the attack pipeline allows after that;
//!   3. its target dying sends it under exactly ceil(HideTimeMs / TICK_MS) ticks
//!      after the first Target phase with no live target;
//!   4. a Fireball on a hidden Tesla changes nothing; the same Fireball on an up
//!      Tesla deals the data's damage;
//!   5. a hidden Tesla still dies at its lifetime expiry;
//!   6. a Knight mid-swing on a deploying Tesla drops it the tick it goes under and
//!      re-acquires it when it is up again;
//!   7. mirror symmetry of a two-seat hide scene, every tick;
//!   8. a snapshot taken mid-rise resumes hash-for-hash;
//!   9. every calibration hide.* candidate moves a measurable behaviour.
//!
//! TICK ALIGNMENT (state.rs `hide_pass`): timers are decremented in the Target
//! phase, before any entity decides its target. "Post-tick k" below means the
//! state observed after the k-th `tick()` call of the scene, i.e. `tick_count() == k`.
//!
//! PLANTS: tesla_always_up (the earlier engine; (1) and (4) go red),
//! hidden_targetable, hidden_takes_damage, expiry_respects_hide.

mod common;

use common::*;
use royalesim::arena::Lane;
use royalesim::card::{CardDb, CardSource};
use royalesim::entity::{AttackPhase, EntityKind, HideState};
use royalesim::fixed::{Vec2, SUBTILE};
use royalesim::state::{BattleConfig, BattleState, Calib, DeployError, HideDelayMeaning, RiseTrigger};
use royalesim::{EntityId, Team};

// ---------------------------------------------------------------------------
// data, read not typed

fn calib() -> Calib {
    Calib::shipped()
}

fn dt() -> i32 {
    calib().tick_ms
}

/// ceil(ms / TICK_MS): the number of Target-phase decrements that run a ms timer out.
fn ticks_of(ms: i32) -> u32 {
    ((ms + dt() - 1) / dt()) as u32
}

fn tesla_hide(s: &BattleState) -> royalesim::card::HideDef {
    card_stat(s, "Tesla").hide.expect("cards.json Tesla hides_when_not_attacking with both timers")
}

/// The engine's own edge-to-edge sight test, written out here from the data so the
/// test does not share code with the thing it measures: `from` sees `to` when
/// centre distance <= from.sight + to.radius (targeting.ADD_CHARACTER_RANGE_TO_RADIUS).
fn sees(s: &BattleState, from: EntityId, to: EntityId) -> bool {
    let (f, t) = (s.entity(from).unwrap(), s.entity(to).unwrap());
    let sight = card_stat(s, f.card).sight_range + if t.kind == EntityKind::Building { calib().extra_sight_range_to_building } else { 0 };
    let r = (sight + t.radius) as i64;
    f.pos.dist2(t.pos) <= r * r
}

fn hide_of(s: &BattleState, id: EntityId) -> (HideState, i32) {
    let e = s.entity(id).unwrap_or_else(|| panic!("{id:?} is dead"));
    (e.hide_state, e.hide_ms)
}

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(7, cfg)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = config();
    f(&mut cfg.calib);
    cfg
}

// ---------------------------------------------------------------------------
// scenes

/// THE APPROACH SCENE: a Blue Tesla beside the engine-left lane and a Red Knight on
/// the left bridge's approach, just OUTSIDE the Tesla's sight, walking down the
/// bridge toward Blue's engine-left princess tower and into the Tesla's sight.
/// Far from every crown tower's reach until well after the first shot.
fn approach(cfg: BattleConfig) -> (BattleState, EntityId, EntityId) {
    let mut s = bare(cfg);
    let bridge_x = s.arena().princess_tower_pos(Team::Blue, Lane::Left).x;
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", Vec2::new(bridge_x - 3 * SUBTILE / 2, 25 * SUBTILE / 2), None).unwrap();
    let knight = s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(bridge_x, 189 * SUBTILE / 10), None).unwrap();
    assert!(!sees(&s, tesla, knight), "scene: the Knight must start outside the Tesla's sight");
    assert!(!sees(&s, knight, tesla), "scene: the Knight must start outside its own sight of the Tesla");
    (s, tesla, knight)
}

/// Run `approach` until the Tesla is UP (post-tick U + 1) and has fired once.
/// Returns (state, tesla, knight, N) where N is the post-tick index at which the
/// Knight was first inside the Tesla's sight.
fn approach_until_first_shot(cfg: BattleConfig) -> (BattleState, EntityId, EntityId, u32) {
    let (mut s, tesla, knight) = approach(cfg);
    let full = s.entity(knight).unwrap().hp;
    let n = run_until(&mut s, 400, |s| sees(s, tesla, knight));
    assert!(n < 400, "the Knight never walked into sight");
    let fired = run_until(&mut s, 200, |s| s.entity(knight).unwrap().hp < full);
    assert!(fired < 200, "the Tesla never fired");
    assert_eq!(hide_of(&s, tesla).0, HideState::Up);
    (s, tesla, knight, n)
}

// ---------------------------------------------------------------------------
// (1) hidden means untargetable

#[test]
fn a_hidden_tesla_is_not_targeted_by_a_giant_that_sees_it_but_that_it_cannot_see() {
    // A Blue Tesla 7 tiles left of a Red Giant: inside the Giant's sight of a
    // building (7.5 edge-to-edge), outside the Tesla's 5.5. Blue's engine-right
    // princess tower is the Giant's next-nearest building and it walks there,
    // moving AWAY from the Tesla, which therefore stays under for the whole run.
    // Plant tesla_always_up: the Giant locks on the nearer Tesla at tick 0.
    let mut s = bare(config());
    let giant_at = t(1000, 1350);
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", Vec2::new(giant_at.x - 7 * SUBTILE, giant_at.y), None).unwrap();
    let giant = s.scenario_spawn_now(Team::Red, "Giant", giant_at, None).unwrap();
    assert!(card_stat(&s, "Giant").target_only_buildings, "the Giant must be a building-targeter for this scene to bite");
    assert!(sees(&s, giant, tesla), "scene: the Giant must see the Tesla");
    assert!(!sees(&s, tesla, giant), "scene: the Tesla must not see the Giant");
    let right = s.tower_ids(Team::Blue)[2].unwrap();
    let mut targeted_tower = false;
    for k in 0..300 {
        s.tick();
        let g = s.entity(giant).unwrap_or_else(|| panic!("the Giant died at tick {k}"));
        // The behavioural claim first, so the plant is caught by it and not by the
        // state read below.
        assert_ne!(g.target, Some(tesla), "tick {k}: the Giant targeted a hidden Tesla");
        assert_eq!(hide_of(&s, tesla), (HideState::Hidden, 0), "tick {k}: the Tesla left the Hidden state with nothing in its sight");
        targeted_tower |= g.target == Some(right);
    }
    assert!(targeted_tower, "the Giant never went for the princess tower: the scene did not exercise targeting");
    let g = s.entity(giant).unwrap();
    assert!(g.pos.dist2(s.entity(tesla).unwrap().pos) > giant_at.dist2(s.entity(tesla).unwrap().pos), "the Giant walked toward the Tesla");
}

// ---------------------------------------------------------------------------
// (2) rise timing and the first shot

#[test]
fn a_knight_entering_sight_starts_the_rise_next_tick_up_after_up_time_and_the_first_shot_follows() {
    let (mut s, tesla, knight) = approach(config());
    let h = tesla_hide(&s);
    let (load, base) = (card_stat(&s, "Tesla").load_time_ms, card_stat(&s, "Tesla").damage);
    let full = s.entity(knight).unwrap().hp;
    let level = s.config().card_level[Team::Blue as usize];
    let db = s.cards();
    let shot = db.scaled(db.index("Tesla").unwrap(), level, base).unwrap();
    // N: the first post-tick observation with the Knight inside the Tesla's sight.
    // The hide pass of tick N-1 ran on the Knight's position BEFORE that tick's
    // move, so at post-tick N the Tesla is still under.
    let n = run_until(&mut s, 400, |s| sees(s, tesla, knight));
    assert!((1..400).contains(&n), "the Knight never walked into sight (n = {n})");
    assert_eq!(hide_of(&s, tesla), (HideState::Hidden, 0), "post-tick {n}: the rise started before the Knight was seen");
    assert_ne!(s.entity(knight).unwrap().target, Some(tesla));
    // Tick N's hide pass sees the Knight: Rising, timer = UpTimeMs, no decrement yet.
    s.tick();
    assert_eq!(hide_of(&s, tesla), (HideState::Rising, h.up_time_ms), "post-tick {}: the rise did not start", n + 1);
    // Rising for ceil(UpTimeMs / TICK_MS) ticks; up with a full hide countdown; never
    // targeted by the Knight and never firing meanwhile (hide.TARGETABLE_WHILE_RISING).
    let rise = ticks_of(h.up_time_ms);
    for k in 1..rise {
        s.tick();
        assert_eq!(hide_of(&s, tesla), (HideState::Rising, h.up_time_ms - (k as i32) * dt()), "post-tick {}", n + 1 + k);
        assert_ne!(s.entity(knight).unwrap().target, Some(tesla), "post-tick {}: a rising Tesla was targeted", n + 1 + k);
        assert_eq!(s.entity(knight).unwrap().hp, full, "post-tick {}: a rising Tesla fired", n + 1 + k);
    }
    s.tick();
    let up = n + 1 + rise; // post-tick index at which it is Up; the up tick index is up - 1
    assert_eq!(hide_of(&s, tesla), (HideState::Up, h.hide_time_ms), "post-tick {up}: not up exactly UpTimeMs after the rise began");
    // The same Target phase let it target, and let the Knight target it back.
    assert_eq!(s.entity(tesla).unwrap().target, Some(knight), "post-tick {up}: the up Tesla did not target the Knight");
    assert_eq!(s.entity(knight).unwrap().target, Some(tesla), "post-tick {up}: the Knight did not re-acquire the up Tesla");
    assert_eq!(s.entity(knight).unwrap().hp, full, "post-tick {up}: a shot landed on the up tick, before any windup");
    // First shot: the windup started in the up tick's Attack phase with TICK_MS
    // already on the clock and lands when the clock reaches LoadTime (combat.rs
    // attack_step): tick index (up - 1) + ceil(LoadTime / TICK_MS) - 1, observed one
    // post-tick later.
    let windup = ticks_of(load).max(1);
    let fire_post = up + windup - 1;
    while s.tick_count() < fire_post - 1 {
        s.tick();
        assert_eq!(s.entity(knight).unwrap().hp, full, "post-tick {}: fired early", s.tick_count());
    }
    s.tick();
    assert_eq!(s.tick_count(), fire_post);
    assert_eq!(s.entity(knight).unwrap().hp, full - shot, "post-tick {fire_post}: the first shot did not land with the data's damage");
}

// ---------------------------------------------------------------------------
// (3) hiding again

#[test]
fn losing_its_target_sends_the_tesla_under_exactly_hide_time_later() {
    // Kill the Knight by hp (dies in that tick's Resolve). The hide pass of the very
    // tick the target is no longer live counts, so the Tesla is under after exactly
    // ceil(HideTimeMs / TICK_MS) tick calls from the kill. Idle ticks before the kill
    // do not count: a live target resets the countdown every Target phase.
    let (mut s, tesla, knight, _) = approach_until_first_shot(config());
    let h = tesla_hide(&s);
    for _ in 0..5 {
        s.tick();
        assert_eq!(hide_of(&s, tesla), (HideState::Up, h.hide_time_ms), "a live target must hold the countdown at HideTimeMs");
    }
    assert!(s.debug_set_hp(knight, 0));
    let n = ticks_of(h.hide_time_ms);
    for k in 1..n {
        s.tick();
        assert!(s.entity(knight).is_none(), "the Knight did not die");
        assert_eq!(hide_of(&s, tesla), (HideState::Up, h.hide_time_ms - (k as i32) * dt()), "tick call {k} after the kill");
        assert_eq!(s.entity(tesla).unwrap().target, None);
    }
    s.tick();
    assert_eq!(hide_of(&s, tesla), (HideState::Hidden, 0), "not under after {n} tick calls");
    // And it stays under: nothing else is in its sight.
    for _ in 0..20 {
        s.tick();
        assert_eq!(hide_of(&s, tesla), (HideState::Hidden, 0));
    }
}

// ---------------------------------------------------------------------------
// (4) immunity

/// Fireball a Tesla at `pos` and step until the spell has landed; returns (battle
/// with the cast, control without it), both stopped at the landing tick.
fn fireball_on(mut s: BattleState, pos: Vec2) -> (BattleState, BattleState, i32) {
    let mut control = s.clone();
    s.spawn_unit(Team::Red, "Fireball", pos, None).unwrap();
    s.tick();
    assert_eq!(s.spells().len(), 1, "the Fireball did not launch");
    let damage = s.spells()[0].damage;
    control.tick();
    let n = run_until(&mut s, 400, |s| s.spells().is_empty());
    assert!(n < 400, "the Fireball never landed");
    for _ in 0..n {
        control.tick();
    }
    (s, control, damage)
}

#[test]
fn a_fireball_on_a_hidden_tesla_changes_nothing_and_on_an_up_tesla_deals_the_data_damage() {
    // HIDDEN: a Tesla alone. Nothing in its sight, so it is under when the spell
    // lands; hp equal to the control's, still under, and the spell is gone.
    // Plants tesla_always_up / hidden_takes_damage: the hit lands.
    let mut s = bare(config());
    let at = t(900, 1200);
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", at, None).unwrap();
    let (s, control, damage) = fireball_on(s, at);
    assert!(damage > 0);
    assert_eq!(s.entity(tesla).unwrap().hp, control.entity(tesla).unwrap().hp, "a hidden Tesla took Fireball damage");
    assert_eq!(s.entity(tesla).unwrap().hp, s.entity(tesla).unwrap().max_hp);
    assert_eq!(hide_of(&s, tesla).0, HideState::Hidden);
    // UP: a Red Knight in its sight but far from melee keeps it up for the flight;
    // the Fireball then takes exactly the level-scaled damage the spell carries.
    let mut s = bare(config());
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", at, None).unwrap();
    let knight = s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(at.x - 54 * SUBTILE / 10, at.y), None).unwrap();
    assert!(sees(&s, tesla, knight));
    let n = run_until(&mut s, 100, |s| hide_of(s, tesla).0 == HideState::Up);
    assert!(n < 100);
    let (s, control, damage) = fireball_on(s, at);
    assert_eq!(hide_of(&s, tesla).0, HideState::Up, "the Knight did not hold the Tesla up through the flight");
    assert_eq!(control.entity(tesla).unwrap().hp - s.entity(tesla).unwrap().hp, damage, "an up Tesla did not take the Fireball's damage");
}

#[test]
fn a_hidden_tesla_ignores_zap_stun() {
    // A Zap on a hidden Tesla: no stun timer, no damage. Then the Knight walks in,
    // the Tesla rises and is up on the same schedule as without the Zap -- measured
    // against an unzapped copy of the same scene, to the first shot.
    let (mut s, tesla, knight) = approach(config());
    let mut control = s.clone();
    let at = s.entity(tesla).unwrap().pos;
    s.spawn_unit(Team::Red, "Zap", at, None).unwrap();
    let n = run_until(&mut s, 200, |s| s.spells().is_empty() && s.tick_count() > 0);
    assert!(n < 200);
    for _ in 0..n {
        control.tick();
    }
    let e = s.entity(tesla).unwrap();
    assert_eq!((e.stun_ms, e.hp, e.hide_state), (0, e.max_hp, HideState::Hidden), "a hidden Tesla was stunned or damaged by Zap");
    let full = s.entity(knight).unwrap().hp;
    let first_shot = |s: &mut BattleState| -> (u32, Vec<(u32, HideState, i32)>) {
        let mut states = Vec::new();
        let fired = run_until(s, 400, |s| {
            let (st, ms) = hide_of(s, tesla);
            states.push((s.tick_count(), st, ms));
            s.entity(knight).unwrap().hp < full
        });
        assert!(fired < 400, "the Tesla never fired");
        (s.tick_count(), states)
    };
    let (zapped_at, zapped) = first_shot(&mut s);
    let (control_at, unzapped) = first_shot(&mut control);
    assert_eq!(zapped_at, control_at, "the Zap changed the first-shot tick");
    assert_eq!(zapped, unzapped, "the Zap changed the rise schedule");
    assert!(zapped.iter().any(|(_, st, _)| *st == HideState::Rising), "vacuous: no rise observed");
}

// ---------------------------------------------------------------------------
// (5) lifetime

#[test]
fn a_hidden_tesla_still_dies_at_lifetime_expiry() {
    // Plant expiry_respects_hide: it lives for ever.
    let mut s = bare(config());
    let life = card_stat(&s, "Tesla").lifetime_ms.expect("Tesla has a lifetime");
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", t(900, 1200), None).unwrap();
    let n = ticks_of(life);
    for k in 1..n {
        s.tick();
        assert_eq!(hide_of(&s, tesla), (HideState::Hidden, 0), "tick call {k}: the Tesla surfaced with nothing near it");
    }
    assert!(s.entity(tesla).is_some(), "died before its lifetime");
    s.tick();
    assert!(s.entity(tesla).is_none(), "a hidden Tesla outlived its LifeTime ({life} ms = {n} tick calls)");
}

// ---------------------------------------------------------------------------
// (6) a locked attacker drops the building the tick it goes under

#[test]
fn a_knight_mid_swing_on_a_deploying_tesla_drops_it_when_it_goes_under_and_reacquires_it_when_up() {
    // The Tesla is deployed WITH its deploy time (spawn_unit), and is Up -- targetable
    // -- while deploying, like any deploying building. The adjacent Knight locks on and
    // starts swinging. On the tick the deploy timer ends the Tesla goes under
    // (hide.STARTS_HIDDEN); the Knight's windup is cancelled and it rescans in the same
    // Target phase. The Knight is in the Tesla's sight, so the rise starts on that same
    // tick and the Knight re-acquires it when it is up.
    //
    // TIMING: the Knight is placed LATE enough that its windup
    // (LoadTime) spans the deploy end -- placed at tick 0 it fires at tick 13 and is
    // in COOLDOWN at deploy end, and the lock-drop path (target.rs decide's
    // `target_locked` cancel; phase_target's cancel of a hidden target) is never
    // exercised. ~~`if in_windup { .. }`~~: the windup is asserted, not tested for.
    let mut s = bare(config());
    let tesla_at = t(900, 1400);
    let knight_at = Vec2::new(tesla_at.x, tesla_at.y - 3 * SUBTILE / 2);
    s.spawn_unit(Team::Blue, "Tesla", tesla_at, None).unwrap();
    s.tick(); // Spawn phase: the Tesla materialises, deploying and Up.
    let tesla = find_live(&s, Team::Blue, "Tesla")[0].id;
    let deploy = card_stat(&s, "Tesla").deploy_time_ms;
    let load = card_stat(&s, "Knight").load_time_ms;
    let h = tesla_hide(&s);
    assert!(deploy > 0 && s.entity(tesla).unwrap().deploying);
    assert_eq!(hide_of(&s, tesla).0, HideState::Up, "a deploying Tesla is Up (targetable) until its deploy time ends");
    // Deploy ends in the Upkeep of tick index ceil(DeployTime / TICK_MS) (the spawn
    // tick's Upkeep ran before the Spawn phase). A windup started in tick index W
    // fires in tick index W + ceil(LoadTime / TICK_MS) - 1, so a Knight placed at
    // tick index `end - ticks_of(load) + 1` (>= 1: the data guard) is still winding up
    // when the deploy ends.
    let end = ticks_of(deploy);
    let windup_ticks = ticks_of(load);
    assert!(load > 0 && windup_ticks >= 2 && end > windup_ticks, "data: LoadTime {load} / DeployTime {deploy} cannot stage a windup across the deploy end");
    let place = end - windup_ticks + 1;
    while s.tick_count() < place {
        s.tick();
    }
    let knight = s.scenario_spawn_now(Team::Red, "Knight", knight_at, None).unwrap();
    for _ in place..end {
        s.tick();
        let k = s.entity(knight).unwrap();
        assert_eq!(k.target, Some(tesla), "the Knight did not lock on the deploying Tesla");
        assert_eq!(k.attack_phase, AttackPhase::Windup, "post-tick {}: the Knight is not winding up on the deploying Tesla", s.tick_count());
    }
    let hp_before = s.entity(tesla).unwrap().hp;
    let k = s.entity(knight).unwrap();
    assert!(k.target_locked && k.attack_phase == AttackPhase::Windup, "vacuous: no locked windup at deploy end ({:?}, locked {})", k.attack_phase, k.target_locked);
    s.tick();
    let e = s.entity(tesla).unwrap();
    assert!(!e.deploying, "post-tick {}: the deploy timer should have ended", end + 1);
    assert_eq!(e.hide_state, HideState::Rising, "under at deploy end and, with the Knight in sight, rising in the same Target phase");
    let k = s.entity(knight).unwrap();
    assert_ne!(k.target, Some(tesla), "the Knight kept a building that went under");
    assert_eq!(k.attack_phase, AttackPhase::Idle, "the cancelled windup should be Idle");
    assert!(!k.target_locked, "the lock survived the building going under");
    assert_eq!(e.hp, hp_before, "the cancelled swing landed");
    // Rising: not targetable; up after UpTimeMs; then the Knight has it again and
    // lands hits on it -- the building takes damage when it is up.
    let rise = ticks_of(h.up_time_ms);
    for _ in 1..rise {
        s.tick();
        assert_ne!(s.entity(knight).unwrap().target, Some(tesla));
    }
    s.tick();
    assert_eq!(hide_of(&s, tesla).0, HideState::Up);
    assert_eq!(s.entity(knight).unwrap().target, Some(tesla), "the Knight did not re-acquire the Tesla when it came up");
    let hit = run_until(&mut s, 60, |s| s.entity(tesla).unwrap().hp < hp_before);
    assert!(hit < 60, "the Knight never landed a hit on the up Tesla");
}

// ---------------------------------------------------------------------------
// (7) mirror

#[test]
fn a_two_seat_hide_scene_is_its_own_mirror_every_tick() {
    // Each seat: a Tesla beside its own-left lane and an enemy Knight coming down
    // that lane's bridge, exactly the approach scene rotated 180 degrees for Red.
    // Under `symmetric_config` (the trace-fitted search): the shipped search is the
    // game's absolute-grid one and is not seat-symmetric (tests/common
    // `symmetric_config`; mirror.rs `the_shipped_search_is_absolute_grid_not_seat_symmetric`),
    // so every seat-symmetry gate measures the OTHER systems under this config.
    let mut s = bare(symmetric_config());
    let bridge_x = s.arena().princess_tower_pos(Team::Blue, Lane::Left).x;
    let tesla_b = Vec2::new(bridge_x + 3 * SUBTILE / 2, 12 * SUBTILE);
    let knight_r = Vec2::new(bridge_x, 189 * SUBTILE / 10);
    let (tesla_r, knight_b) = (mirror(&s, tesla_b), mirror(&s, knight_r));
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Tesla", tesla_b, None), (Team::Red, "Knight", knight_r, None), (Team::Red, "Tesla", tesla_r, None), (Team::Blue, "Knight", knight_b, None)])
        .unwrap();
    check_mirror(&s).unwrap();
    let mut seen = [false; 3];
    let mut damaged = false;
    for _ in 0..400 {
        s.tick();
        check_mirror(&s).unwrap_or_else(|e| panic!("{e}\ncensus: {:?}", census(&s)));
        if let Some(e) = s.entity(ids[0]) {
            seen[e.hide_state as usize] = true;
            damaged |= e.hp < e.max_hp;
        }
    }
    assert_eq!(seen, [true, true, true], "the scene must pass through Up, Hidden and Rising (saw [up, hidden, rising] = {seen:?})");
    assert!(damaged, "the Knights never hit the Teslas: the scene is too quiet to certify");
}

// ---------------------------------------------------------------------------
// (8) snapshot mid-rise

#[test]
fn a_snapshot_taken_mid_rise_resumes_hash_for_hash() {
    let (mut s, tesla, knight) = approach(config());
    let h = tesla_hide(&s);
    let full = s.entity(knight).unwrap().hp;
    let n = run_until(&mut s, 400, |s| hide_of(s, tesla).0 == HideState::Rising);
    assert!(n < 400);
    for _ in 0..3 {
        s.tick();
    }
    let (st, ms) = hide_of(&s, tesla);
    assert_eq!(st, HideState::Rising);
    assert!(ms > 0 && ms < h.up_time_ms, "the snapshot must be taken with the rise timer mid-way ({ms})");
    let blob = s.save();
    // The rise timer is in the snapshot (a plant that drops it cannot self-check).
    assert!(String::from_utf8_lossy(&blob).contains("\"hide_ms\""));
    let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("load: {e}"));
    assert_eq!(l.state_hash(), s.state_hash());
    assert_eq!(hide_of(&l, tesla), (st, ms));
    for k in 0..150 {
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
    }
    // Vacuity: the resumed battle went on to fire at the Knight.
    assert!(s.entity(knight).map_or(true, |k| k.hp < full), "the Knight was never hit");
}

// ---------------------------------------------------------------------------
// (9) every candidate moves something

#[test]
fn starts_hidden_false_stands_the_tesla_up_at_deploy_end_and_it_goes_under_after_hide_time() {
    let cfg = with_calib(|c| c.hide_starts_hidden = false);
    let mut s = bare(cfg);
    let h = tesla_hide(&s);
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", t(900, 1200), None).unwrap();
    assert_eq!(hide_of(&s, tesla), (HideState::Up, h.hide_time_ms), "false: a setup-spawned Tesla stands up with a full countdown");
    let n = ticks_of(h.hide_time_ms);
    for k in 1..n {
        s.tick();
        assert_eq!(hide_of(&s, tesla), (HideState::Up, h.hide_time_ms - (k as i32) * dt()));
    }
    s.tick();
    assert_eq!(hide_of(&s, tesla), (HideState::Hidden, 0), "false: under after ceil(HideTimeMs / TICK_MS) idle ticks");
    // true (shipped): under from the start.
    let mut s = bare(config());
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", t(900, 1200), None).unwrap();
    assert_eq!(hide_of(&s, tesla), (HideState::Hidden, 0));
}

#[test]
fn rise_trigger_separates_sight_from_attack_range_on_a_card_whose_sight_exceeds_its_range() {
    // The shipped Tesla has SightRange == Range, so the two arms agree on it; this
    // probe widens the Tesla's sight IN THE DATA (the loader, not a typed number,
    // turns it into the engine's value) and stands a Knight between the two radii.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let tesla = doc["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Tesla").unwrap();
    let range = tesla["range_milli"].as_i64().unwrap();
    tesla["sight_range_milli"] = serde_json::Value::from(range + 2000);
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).unwrap();
    for (trigger, wakes) in [(RiseTrigger::EnemyInSightRange, true), (RiseTrigger::EnemyInAttackRange, false)] {
        let mut cfg = BattleConfig::with_cards(db.clone());
        cfg.calib.hide_rise_trigger = trigger;
        let mut s = bare(cfg);
        let (sight, reach) = (card_stat(&s, "Tesla").sight_range, card_stat(&s, "Tesla").range);
        assert!(sight > reach);
        let at = t(900, 1200);
        let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", at, None).unwrap();
        // Centre distance = range + radius + one tile: outside attack range, inside sight.
        let kr = card_stat(&s, "Knight").collision_radius;
        let knight = s.scenario_spawn_now(Team::Red, "Knight", Vec2::new(at.x - (reach + kr + SUBTILE), at.y), None).unwrap();
        assert!(sees(&s, tesla, knight));
        assert!(s.entity(tesla).unwrap().pos.dist2(s.entity(knight).unwrap().pos) > ((reach + kr) as i64) * ((reach + kr) as i64));
        s.tick();
        let woke = hide_of(&s, tesla).0 == HideState::Rising;
        assert_eq!(woke, wakes, "{trigger:?}: woke = {woke}");
    }
}

#[test]
fn targetable_while_rising_lets_the_knight_lock_on_the_tick_the_rise_begins() {
    for (flag, on_rise_tick) in [(false, false), (true, true)] {
        let cfg = with_calib(|c| c.hide_targetable_while_rising = flag);
        let (mut s, tesla, knight) = approach(cfg);
        let n = run_until(&mut s, 400, |s| hide_of(s, tesla).0 == HideState::Rising);
        assert!(n < 400);
        assert_eq!(s.entity(knight).unwrap().target == Some(tesla), on_rise_tick, "TARGETABLE_WHILE_RISING = {flag}");
    }
}

#[test]
fn hidden_immune_false_lets_a_fireball_hit_a_hidden_tesla() {
    let cfg = with_calib(|c| c.hide_hidden_immune = false);
    let mut s = bare(cfg);
    let at = t(900, 1200);
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", at, None).unwrap();
    let (s, control, damage) = fireball_on(s, at);
    assert_eq!(hide_of(&s, tesla).0, HideState::Hidden, "the false arm changes damage, not targeting");
    assert_eq!(control.entity(tesla).unwrap().hp - s.entity(tesla).unwrap().hp, damage, "HIDDEN_IMMUNE_TO_DAMAGE = false: the hit must land");
}

#[test]
fn hide_delay_time_since_last_shot_counts_from_the_shot_not_from_the_kill() {
    // Same scene, same kill 5 ticks after the first shot lands. idle_time_without_target:
    // under ceil(HideTimeMs / TICK_MS) tick calls after the kill. time_since_last_shot:
    // under on the first Target phase MORE than HideTimeMs after the shot, which is
    // earlier by the 5 ticks the Knight outlived the shot (and, while the Knight lives,
    // the Tesla keeps firing every HitSpeed and never goes under).
    let mut got = Vec::new();
    for meaning in [HideDelayMeaning::IdleTimeWithoutTarget, HideDelayMeaning::TimeSinceLastShot] {
        let cfg = with_calib(|c| c.hide_delay_meaning = meaning);
        let (mut s, tesla, knight, _) = approach_until_first_shot(cfg);
        let hit_speed = card_stat(&s, "Tesla").hit_speed_ms;
        // The Tesla keeps firing: a second shot lands one HitSpeed after the first.
        let hp1 = s.entity(knight).unwrap().hp;
        let second = run_until(&mut s, 100, |s| s.entity(knight).unwrap().hp < hp1);
        assert_eq!(second, ticks_of(hit_speed), "{meaning:?}: the second shot is one HitSpeed after the first");
        assert_eq!(hide_of(&s, tesla).0, HideState::Up);
        for _ in 0..5 {
            s.tick();
        }
        assert!(s.debug_set_hp(knight, 0));
        let k = run_until(&mut s, 100, |s| hide_of(s, tesla).0 == HideState::Hidden);
        got.push((meaning, k));
    }
    let n = ticks_of(tesla_hide(&bare(config())).hide_time_ms) as i64;
    assert_eq!(got[0].1 as i64, n, "idle_time_without_target: {n} tick calls after the kill");
    // time_since_last_shot: HideTimeMs after the shot is n ticks; the strict "more
    // than" adds one; the 5 ticks the Knight outlived the shot come off.
    assert_eq!(got[1].1 as i64, n + 1 - 5, "time_since_last_shot: {:?}", got);
    assert_ne!(got[0].1, got[1].1);
}

#[test]
fn hidden_occludes_path_false_is_refused_at_load() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/calibration.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["hide"]["HIDDEN_OCCLUDES_PATH"]["value"], serde_json::Value::Bool(true));
    v["hide"]["HIDDEN_OCCLUDES_PATH"]["value"] = serde_json::Value::Bool(false);
    let err = Calib::from_json(&v.to_string()).expect_err("the unimplemented arm loaded");
    assert!(err.contains("HIDDEN_OCCLUDES_PATH"), "{err}");
    // And an unknown name for an enum key is refused, never mapped to another arm.
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["hide"]["RISE_TRIGGER"]["value"] = serde_json::Value::from("enemy_anywhere");
    assert!(Calib::from_json(&v.to_string()).is_err());
}

#[test]
fn a_hidden_footprint_still_refuses_a_deploy_on_it() {
    // hide.HIDDEN_OCCLUDES_PATH = true: the footprint of a hidden Tesla is a
    // building footprint like any other for deploys (the path grid shares the rule).
    let mut s = bare(scripted_config()); // decks dealt, so Blue has a hand
    let at = t(900, 1200);
    let tesla = s.scenario_spawn_now(Team::Blue, "Tesla", at, None).unwrap();
    assert_eq!(hide_of(&s, tesla).0, HideState::Hidden);
    let hand: Vec<String> = s.hand(Team::Blue).iter().map(|c| c.to_string()).collect();
    let troop = hand.iter().find(|c| card_stat(&s, c).kind == royalesim::card::CardKind::Troop).expect("a troop in Blue's opening hand");
    assert!(matches!(s.check_deploy(Team::Blue, troop, at), Err(DeployError::Occupied)), "{troop}: {:?}", s.check_deploy(Team::Blue, troop, at));
    let beside = Vec2::new(at.x - 3 * SUBTILE, at.y);
    assert!(s.check_deploy(Team::Blue, troop, beside).is_ok(), "{troop} beside the Tesla: {:?}", s.check_deploy(Team::Blue, troop, beside));
}

// ---------------------------------------------------------------------------
// data gate

#[test]
fn the_loader_refuses_a_partial_hide_block_and_reads_teslas_whole() {
    let s = bare(config());
    let h = tesla_hide(&s);
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let raw = doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "Tesla").unwrap();
    assert_eq!(raw["hides_when_not_attacking"], serde_json::Value::Bool(true));
    assert_eq!(h.hide_time_ms as i64, raw["hide_time_ms"].as_i64().unwrap());
    assert_eq!(h.up_time_ms as i64, raw["up_time_ms"].as_i64().unwrap());
    assert!(s.cards().cards.iter().filter(|c| c.hide.is_some()).all(|c| c.name == "Tesla"), "only Tesla hides in the 2018 data");
    // A broken block REJECTS the card with a reason (CardDb::rejected, the loader's
    // per-card refusal path); it never loads as a plain building.
    for (field, value) in [("hide_time_ms", serde_json::Value::Null), ("up_time_ms", serde_json::Value::Null), ("hides_when_not_attacking", serde_json::Value::Bool(false))] {
        let mut d = doc.clone();
        d["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Tesla").unwrap()[field] = value;
        let db = CardDb::from_json_str(&d.to_string(), CardSource::DerivedJson).unwrap();
        assert!(db.index("Tesla").is_none(), "{field}: Tesla with a partial hide block loaded as a card");
        let (_, why) = db.rejected.iter().find(|(n, _)| n == "Tesla").unwrap_or_else(|| panic!("{field}: Tesla neither loaded nor rejected"));
        assert!(why.contains("hide"), "{field}: rejected for the wrong reason: {why}");
    }
}
