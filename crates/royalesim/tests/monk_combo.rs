//! THE MONK'S COMBO AND ITS REACH: combat.ATTACK_COMBO (card.rs `ComboDef`, combat.rs `stage_damage`, entity.rs
//! `combo_ix`), knockback.COMBO_PUSHBACK (combat.rs `fire`) and targeting.VARIABLE_DAMAGE_WALK_REACH's
//! client15535_no_own_radius_walking_every_row (target.rs `walking_own_radius`).
//!
//! The law, measured on client 15.535.29 (the champion scenes):
//!   - a level-11 Monk's hits run 140, 140, 422 (Damage 55, VariableDamage2 55 and VariableDamage3 165 at 256 per
//!     cent, AttackSequence [0, 1, 2]), and the count runs across targets: his first hit killed a Skeleton, and his
//!     second and third fell on a Knight as 140 and 422;
//!   - the third hit (MeleePushback3 1800) pushes its target straight away from him, its first step on the hit's own
//!     tick (249 measured; 1,813 in all, where the engine's ladder for 1,800 gives 1,625: an open question);
//!   - a walking Monk's attack starts within Range + the target's radius (1,575 to 1,696 from a Knight, against 1,700),
//!     a walking Mighty Miner's too (2,050, against 2,100), where a Knight's starts at Range + both radii.
//!
//! The scenes: Blue's level-11 Monk (or Mighty Miner, or Knight) at (3500, 9000), and Red's level-11 units, all put
//! down deployed, Blue's princess towers down so no tower reaches Red. Pinned:
//!   1. the hits run 140, 140, 422, 140 across a kill under the new arm, and 140 every hit under not_read;
//!   2. the third hit pushes the Knight straight away from the Monk, its first step (about 250) on the hit's own tick
//!      and at least 1,500 over the ladder, where the first two push nothing; the count runs under COMBO_PUSHBACK alone; under
//!      not_read the Knight stands;
//!   3. the every-row arm: a walking Monk's attack starts within Range + the Knight's radius and a walking Mighty
//!      Miner's too, and neither does under the shipped arm; a Knight's starts where it did;
//!   4. the shipped values are the old arms of the two new keys, and the shipped walk reach is unchanged.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test monk_combo`):
//!   combo_unread                       every hit deals Damage under the new arm: (1) goes red.
//!   combo_push_next_tick               the push's first step is the next tick's: (2) goes red.
//!   walk_reach_every_row_flyers_only   the every-row arm reads flyers alone: (3) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttackCombo, BattleConfig, BattleState, Calib, ComboPushback, VariableDamageWalkReach};
use royalesim::{EntityId, Team};

const ME_AT: (i32, i32) = (3500, 9000);
const SHIPPED_REACH: VariableDamageWalkReach = VariableDamageWalkReach::Client16402NoOwnRadiusWalking;
const EVERY_ROW: VariableDamageWalkReach = VariableDamageWalkReach::Client15535NoOwnRadiusWalkingEveryRow;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The centre distance, native.
fn dist(a: Vec2, b: Vec2) -> i64 {
    isqrt(a.dist2(b)) / K as i64
}

fn cfg_with(combo: AttackCombo, push: ComboPushback, reach: VariableDamageWalkReach) -> BattleConfig {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.attack_combo = combo;
    cfg.calib.combo_pushback = push;
    cfg.calib.variable_damage_walk_reach = reach;
    cfg
}

/// The scene: Blue's `me` at ME_AT and Red's `reds`, all deployed, Blue's princess towers down. Returns the battle,
/// Blue's id and Red's.
fn scene(cfg: BattleConfig, me: &str, reds: &[(&str, (i32, i32))]) -> (BattleState, EntityId, Vec<EntityId>) {
    let mut s = BattleState::try_new(0, cfg).expect("the battle");
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    let id = s.scenario_spawn_now(Team::Blue, me, n(ME_AT), None).unwrap_or_else(|e| panic!("spawn {me}: {e:?}"));
    let reds = reds.iter().map(|(card, at)| s.scenario_spawn_now(Team::Red, card, n(*at), None).unwrap_or_else(|e| panic!("spawn {card}: {e:?}"))).collect();
    (s, id, reds)
}

/// Per tick: each Red unit's hp (0 once gone) and point, and Blue's attacker's point.
struct Row {
    hp: Vec<i32>,
    at: Vec<Option<Vec2>>,
    me: Option<Vec2>,
}

fn run(s: &mut BattleState, me: EntityId, reds: &[EntityId], ticks: u32) -> Vec<Row> {
    (0..ticks)
        .map(|_| {
            s.tick();
            Row {
                hp: reds.iter().map(|r| s.entity(*r).map_or(0, |e| e.hp)).collect(),
                at: reds.iter().map(|r| s.entity(*r).map(|e| e.pos)).collect(),
                me: s.entity(me).map(|e| e.pos),
            }
        })
        .collect()
}

/// Red unit `k`'s hp drops, in order, with the row each lands on.
fn drops(start: i32, rows: &[Row], k: usize) -> Drops {
    let mut prev = start;
    let mut out = Vec::new();
    for (t, r) in rows.iter().enumerate() {
        if r.hp[k] < prev {
            out.push((t, prev - r.hp[k]));
        }
        prev = r.hp[k];
    }
    out
}

/// A unit's hp drops, in order, with the row each lands on (`drops`).
type Drops = Vec<(usize, i32)>;

/// The Skeleton-and-Knight scene: the Skeleton's hp drops and the Knight's.
fn skeleton_then_knight(combo: AttackCombo) -> (Drops, Drops) {
    let (mut s, monk, reds) = scene(cfg_with(combo, ComboPushback::NotRead, SHIPPED_REACH), "Monk", &[("Skeletons", (3500, 10300)), ("Knight", (3500, 12500))]);
    let hp: Vec<i32> = reds.iter().map(|r| s.entity(*r).expect("on the board").hp).collect();
    let rows = run(&mut s, monk, &reds, 200);
    (drops(hp[0], &rows, 0), drops(hp[1], &rows, 1))
}

#[test]
fn the_hits_run_140_140_422_across_a_kill() {
    let (skeleton, knight) = skeleton_then_knight(AttackCombo::SequenceAcrossTargets);
    assert_eq!(skeleton.len(), 1, "the scene drifted: the Skeleton did not die of one blow: {skeleton:?}");
    assert!(knight.len() >= 3, "the scene drifted: fewer than three blows on the Knight: {knight:?}");
    assert!(skeleton[0].0 < knight[0].0, "the scene drifted: the Knight was hit before the Skeleton");
    let blows: Vec<i32> = knight.iter().take(3).map(|d| d.1).collect();
    assert_eq!(blows, [140, 422, 140], "the Knight takes the combo's second and third entries, then the first: {knight:?}");
    let (_, knight) = skeleton_then_knight(AttackCombo::NotRead);
    let blows: Vec<i32> = knight.iter().take(3).map(|d| d.1).collect();
    assert_eq!(blows, [140, 140, 140], "not_read: every hit deals Damage: {knight:?}");
}

/// The Knight scene under `combo` and `push`: the Knight's hp drops, and its point and the Monk's per row.
fn knight_pushed(combo: AttackCombo, push: ComboPushback) -> (Drops, Vec<Row>) {
    let (mut s, monk, reds) = scene(cfg_with(combo, push, SHIPPED_REACH), "Monk", &[("Knight", (3500, 11000))]);
    let hp = s.entity(reds[0]).expect("the Knight").hp;
    let rows = run(&mut s, monk, &reds, 120);
    (drops(hp, &rows, 0), rows)
}

#[test]
fn the_third_hit_pushes_the_target_away_from_the_hit_tick() {
    let (d, rows) = knight_pushed(AttackCombo::SequenceAcrossTargets, ComboPushback::LadderFromAttackerHitTick);
    assert!(d.len() >= 3, "the scene drifted: fewer than three blows: {d:?}");
    let at = |t: usize| rows[t].at[0].expect("the Knight lives");
    for &(t, _) in &d[..2] {
        assert_eq!(at(t), at(t - 1), "hits 1 and 2 push nothing (tick row {t})");
    }
    let h = d[2].0;
    assert_eq!(d[2].1, 422, "the third blow");
    let steps: Vec<i64> = (h..h + 14).map(|t| dist(at(t - 1), at(t))).collect();
    assert!((240..=260).contains(&steps[0]), "the push's first step is on the hit's own tick: {steps:?}");
    let monk = rows[h - 1].me.expect("the Monk lives");
    assert!(dist(monk, at(h)) >= dist(monk, at(h - 1)) + steps[0] - 2, "the push is straight away from the Monk");
    // The push's length is the knockback ladder's for MeleePushback3 (the engine's 1,625 over 11 ticks); the two
    // measured pushes moved 1,813, which the ladder does not give: knockback.COMBO_PUSHBACK's open question. Pinned
    // here: a push, not a nudge, and still away from the Monk after the ladder.
    let total = dist(at(h - 1), at(h + 10));
    assert!(total >= 1500, "the push runs the ladder for MeleePushback3: {total} ({steps:?})");
    assert!(dist(monk, at(h + 10)) >= dist(monk, at(h - 1)) + 1500, "the push stays away from the Monk");
    // Under COMBO_PUSHBACK alone the count runs, so the third hit (140 now) still pushes.
    let (d, rows) = knight_pushed(AttackCombo::NotRead, ComboPushback::LadderFromAttackerHitTick);
    let h = d[2].0;
    assert_eq!(d[2].1, 140, "not_read's third blow");
    assert!(dist(rows[h - 1].at[0].unwrap(), rows[h].at[0].unwrap()) > 200, "the count runs under COMBO_PUSHBACK alone");
    // Under not_read nothing moves the Knight on the third hit.
    let (d, rows) = knight_pushed(AttackCombo::SequenceAcrossTargets, ComboPushback::NotRead);
    let h = d[2].0;
    assert_eq!(rows[h].at[0], rows[h - 1].at[0], "not_read: the third hit pushes nothing");
}

/// The centre distance at the tick `me`'s attack starts on (its phase leaves Idle), from the tick's start, with a
/// Red Knight standing 5,000 up the lane; and whether `me` walked on some tick before.
fn attack_start(me: &str, reach: VariableDamageWalkReach) -> (i64, bool) {
    let (mut s, id, reds) = scene(cfg_with(AttackCombo::NotRead, ComboPushback::NotRead, reach), me, &[("Knight", (3500, 14000))]);
    let mut walked = false;
    for _ in 0..400 {
        let (a0, k0) = (s.entity(id).expect("it lives").pos, s.entity(reds[0]).expect("the Knight lives").pos);
        s.tick();
        let a = s.entity(id).expect("it lives");
        if a.attack_phase != AttackPhase::Idle {
            return (dist(a0, k0), walked);
        }
        walked |= a.pos != a0;
    }
    panic!("{me}: no attack started");
}

#[test]
fn the_every_row_arm_starts_a_walking_monk_and_mighty_miner_within_range_plus_the_target_radius() {
    for (me, short) in [("Monk", 1200 + 500), ("MightyMiner", 1600 + 500)] {
        let (d, walked) = attack_start(me, EVERY_ROW);
        assert!(walked, "{me}: the scene drifted: it never walked");
        assert!(d <= short, "{me}: every-row arm: the attack started {d} from the Knight, outside Range + its radius ({short})");
        let (d, _) = attack_start(me, SHIPPED_REACH);
        assert!(d > short && d <= short + 500, "{me}: shipped arm: the attack started {d}, not within Range + both radii and outside Range + the Knight's radius");
    }
    assert_eq!(attack_start("Knight", EVERY_ROW), attack_start("Knight", SHIPPED_REACH), "a Knight's start moved with the arm");
}

#[test]
fn the_shipped_values_are_the_old_arms() {
    let c = Calib::shipped();
    assert_eq!(c.attack_combo, AttackCombo::NotRead);
    assert_eq!(c.combo_pushback, ComboPushback::NotRead);
    assert_eq!(c.variable_damage_walk_reach, SHIPPED_REACH);
}
