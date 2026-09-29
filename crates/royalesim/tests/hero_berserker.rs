//! THE HERO BERSERKER'S RAGE (card.rs `AbilityEffect::ActionGroup`; state.rs `fire_ability`, `wears_finish_override`;
//! combat.rs `own_damage`, `unkillable_floor`; status.rs `BuffDef::damage_pct`, `unkillable`).
//!
//! THE MEASUREMENT, client 15.535.29, sp-form-Berserker-hero-s0 (hero Berserker at level 11, the press issued on
//! t187, so P = 187 as tests/hero_forms.rs counts it; every frame below is P + k). BerserkerHeroAbility: CastTime 1450,
//! TriggerDelay 100, ManaCost 3; its group hangs BerserkerHero_buff on the hero for 4000 ms at the trigger
//! (SpeedMultiplier 150, HitSpeedMultiplier 300, DamageMultiplier 164, UNKILLABLE) and puts the hero in its bear form
//! (OverrideAttackFinishTime) from the trigger to 4000 ms after it.
//!   - The cast: the hero stands in state 10 on P + 1 .. P + 28 (t188-t215).
//!   - The walk: 134 a tick against its 89 on the same path (the Speed 90, x 150 / 100) up to P + 82 (t269), 89 from
//!     P + 83: 80 frames from the trigger on P + 3.
//!   - The hits: 167 on a Knight every 4 frames (t227 .. t263, 10 of 10), the level-11 102 x 164 / 100.
//!   - Unkillable: a Musketeer's 217 on its 43 leaves it at 1 (t248), and it lives on at 1 to t286, after the rage.
//!   - The bear form: its target killed (a Skeleton lost on t218, the Knight on t267), it takes the next on the loss
//!     + 1, twice, with its attack progress above 0: no post-kill wait (combat.POST_KILL_RETARGET_WAIT, clause (a)).
//!
//! THE SCENES here put a Blue Hero Berserker down past every tower's reach, deploy it and press on its first free
//! frame (k = 0 is the first frame after the press, P + 1): a free walk up the left lane; a red Knight beside it (four
//! of its blows, 808, cannot kill the hero's 896 inside the 80 frames, so that check stands without the rage's floor); a
//! red P.E.K.K.A beside it with the hero's hitpoints set to 100 at the press; red Skeletons in front of it. Each check
//! has a control without the press, so it cannot pass by the scene alone.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_berserker`):
//!   - own_damage_multiplier_ignored -> `the_rage_hits_for_167_every_4_frames` red (102 a hit);
//!   - unkillable_not_read -> `the_raged_hero_keeps_1_hp_and_dies_after_the_rage` red (it dies to the first blow);
//!   - form_finish_override_ignored -> `the_bear_takes_its_next_target_on_the_loss_plus_1` red (it waits to L + 6).
#![allow(unexpected_cfgs)]

mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeployError};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Berserker", "Knight", "Archer", "Giant", "Valkyrie", "HogRider", "Fireball", "Zap"];
/// The rage's frames from k = 0 (P + 1): the trigger on k = 2 (P + 3), the last raged frame k = 81 (P + 82).
const TRIGGER: u32 = 2;
const LAST_RAGED: u32 = 81;
/// The last frame the hero stands for its cast: the scene reads state 10 on k = 0 .. 27 (P + 1 .. P + 28) and no step
/// on k = 28; the engine's first step after it is k = 29.
const CAST_FRAMES: u32 = 28;

fn native(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A battle of DECK against DECK, Blue's Berserker entry its hero form, at the end of the opening lockout, both sides
/// at 10 elixir, Blue's princess towers down (so no tower reaches the scenes, which stand at y 11500 and above).
fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    s
}

/// The hero put down at `at` and deployed, with red `enemies` (card, offset from the hero) put down and deployed
/// beside it: the battle and the hero's id.
fn scene(at: (i32, i32), enemies: &[(&str, (i32, i32))]) -> (BattleState, EntityId) {
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Berserker_hero", native(at.0, at.1), None).expect("the hero");
    for (card, (dx, dy)) in enemies {
        s.spawn_unit(Team::Red, card, native(at.0 + dx, at.1 + dy), None).unwrap_or_else(|e| panic!("{card}: {e:?}"));
    }
    s.tick();
    let hid = find_live(&s, Team::Blue, "Berserker_hero")[0].id;
    run_until(&mut s, 40, |s| s.entities().all(|e| !e.deploying));
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    (s, hid)
}

/// One frame of the hero and of one watched enemy.
#[derive(Clone, Copy, Debug)]
struct Frame {
    pos: Option<Vec2>,
    hp: Option<i32>,
    /// Its target, None when there is none or it is dead (the engine keeps a dead target's id until its next Target
    /// phase; the client's frame reads none on the loss).
    target: Option<EntityId>,
    attack_ms: i32,
    enemy_hp: Option<i32>,
}

/// `frames` frames from k = 0 (the first after the press, or after the moment it would have come), `edit` run on the
/// battle before the first tick.
fn run(s: &mut BattleState, hid: EntityId, enemy: Option<EntityId>, press: bool, frames: u32, edit: impl FnOnce(&mut BattleState)) -> Vec<Frame> {
    if press {
        let before = s.elixir_raw(Team::Blue);
        s.press_ability_button(Team::Blue, 0).expect("the press");
        assert!(s.elixir_raw(Team::Blue).0 < before.0, "the press is paid");
    }
    edit(s);
    let mut out = Vec::new();
    for _ in 0..frames {
        s.tick();
        let h = s.entity(hid);
        out.push(Frame {
            pos: h.as_ref().map(|e| e.pos),
            hp: h.as_ref().map(|e| e.hp),
            target: h.as_ref().and_then(|e| e.target).filter(|t| s.entity(*t).is_some()),
            attack_ms: h.as_ref().map_or(0, |e| e.attack_ms),
            enemy_hp: enemy.and_then(|id| s.entity(id)).map(|e| e.hp),
        });
    }
    out
}

/// The step on frame k (from k - 1; the first frame's from the hero's point before the press), native, rounded down.
fn steps(start: Vec2, fr: &[Frame]) -> Vec<i32> {
    let mut prev = start;
    fr.iter()
        .map(|f| {
            let p = f.pos.expect("the hero lives through the walk");
            let d = p.sub(prev);
            prev = p;
            let (dx, dy) = ((d.x / K) as i64, (d.y / K) as i64);
            (((dx * dx + dy * dy) as f64).sqrt()) as i32
        })
        .collect()
}

#[test]
fn the_raged_hero_stands_its_cast_then_walks_half_again_as_fast_for_4000_ms() {
    // The left lane, below its bridge: the hero walks up at the red princess tower, a short diagonal leg (89 a tick
    // unraged) and then straight (90, its Speed).
    let (mut s, hid) = scene((3500, 11500), &[]);
    let start = s.entity(hid).unwrap().pos;
    let db = s.cards().clone();
    let rage = db.buff_names.iter().position(|n| n.split('|').any(|p| p == "BerserkerHero_buff")).expect("the rage is a loaded buff") as u16 + 1;
    let mut raged_on: Vec<bool> = Vec::new();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut fr = Vec::new();
    for _ in 0..100 {
        s.tick();
        let e = s.entity(hid).expect("the hero lives");
        raged_on.push(e.buffs.iter().any(|b| b.id == rage));
        fr.push(Frame { pos: Some(e.pos), hp: Some(e.hp), target: e.target, attack_ms: e.attack_ms, enemy_hp: None });
    }
    let st = steps(start, &fr);
    let (mut c, hc) = scene((3500, 11500), &[]);
    let cstart = c.entity(hc).unwrap().pos;
    let base = steps(cstart, &run(&mut c, hc, None, false, 100, |_| {}));
    assert!(base.iter().all(|v| (89..=90).contains(v)) && base.contains(&90), "the control walks 89 then 90 a tick: {base:?}");
    // The rage lands on the trigger (k = 2, P + 3) for 4000 ms: its slot reads on at the end of k = 2 .. 80 and runs
    // out in k = 81's Status phase, after that tick's walk and attack (status.BUFF_EXPIRY_TICK_ALIGNMENT), so k = 81
    // (P + 82) is the last raged frame, as the scene's walk shows.
    let window: Vec<u32> = (0..100u32).filter(|&k| raged_on[k as usize]).collect();
    assert_eq!((window.first().copied(), window.last().copied(), window.len()), (Some(TRIGGER), Some(LAST_RAGED - 1), 79), "the rage's slot");
    // It stands through the cast (the scene reads state 10 on k = 0 .. 27 and no step on k = 28 either), then walks
    // at 150 % of its walk (90 x 150 / 100 = 135 straight, 133 or 134 on the diagonal) to k = 81, and at 89 or 90
    // after.
    assert!(st[..=CAST_FRAMES as usize].iter().all(|v| *v == 0), "the hero stands through its cast: {st:?}");
    let raged = &st[CAST_FRAMES as usize + 1..=LAST_RAGED as usize];
    assert!(raged.iter().all(|v| (133..=135).contains(v)) && raged.contains(&135), "raged steps from k = {}: {st:?}", CAST_FRAMES + 1);
    assert!(st[LAST_RAGED as usize + 1..].iter().all(|v| (89..=90).contains(v)), "unraged steps after the rage: {st:?}");
}

#[test]
fn the_rage_hits_for_167_every_4_frames() {
    let (mut s, hid) = scene((9000, 12500), &[("Knight", (0, 1500))]);
    let pk = find_live(&s, Team::Red, "Knight")[0].id;
    let fr = run(&mut s, hid, Some(pk), true, 80, |_| {});
    let drops = drops_of(&fr);
    let (mut c, hc) = scene((9000, 12500), &[("Knight", (0, 1500))]);
    let cpk = find_live(&c, Team::Red, "Knight")[0].id;
    let control = drops_of(&run(&mut c, hc, Some(cpk), false, 80, |_| {}));
    let base = c.cards().scaled(c.cards().index("Berserker_hero").unwrap(), 11, 40).unwrap();
    assert_eq!(base, 102, "the hero's damage at level 11");
    assert!(control.len() >= 3 && control.iter().all(|d| d.1 == base), "the control's hits: {control:?}");
    assert!(control.windows(2).all(|w| w[1].0 - w[0].0 == 12), "the control hits every 12 frames: {control:?}");
    let raged: Vec<(u32, i32)> = drops.iter().copied().filter(|d| d.0 <= LAST_RAGED).collect();
    assert!(raged.len() >= 8, "the raged hero lands at least 8 hits: {drops:?}");
    assert!(raged.iter().all(|d| d.1 == 167), "every raged hit is 102 x 164 / 100 = 167: {drops:?}");
    assert!(raged.windows(2).all(|w| w[1].0 - w[0].0 == 4), "a raged hit every 4 frames: {drops:?}");
    // The first lands on the frame after its first attacking frame, whose progress reads LoadTime + 150 (the raged
    // tick's advance): the scene's first raged hit, a Skeleton entered on P + 30 at progress 550 and struck on P + 31.
    let entry = (CAST_FRAMES as usize + 1..fr.len()).find(|&k| fr[k].attack_ms > 0).expect("the hero attacks after its cast");
    assert_eq!(fr[entry].attack_ms, 550, "its first attacking frame reads 400 + 150");
    assert_eq!(raged[0].0, entry as u32 + 1, "the first raged hit lands the frame after it: {drops:?}");
}

/// The frames on which the watched enemy lost hp, and how much.
fn drops_of(fr: &[Frame]) -> Vec<(u32, i32)> {
    let mut out = Vec::new();
    for k in 1..fr.len() {
        if let (Some(a), Some(b)) = (fr[k - 1].enemy_hp, fr[k].enemy_hp) {
            if b < a {
                out.push((k as u32, a - b));
            }
        }
    }
    out
}

#[test]
fn the_raged_hero_keeps_1_hp_and_dies_after_the_rage() {
    // A P.E.K.K.A's blow is far above 100: the hero at 100 hp dies to the first one it takes, but for the rage.
    let low = |s: &mut BattleState, hid: EntityId| {
        assert!(s.debug_set_hp(hid, 100));
    };
    let (mut s, hid) = scene((9000, 12500), &[("Pekka", (0, 1500))]);
    let fr = run(&mut s, hid, None, true, 140, |s| low(s, hid));
    let hp: Vec<Option<i32>> = fr.iter().map(|f| f.hp).collect();
    let (mut c, hc) = scene((9000, 12500), &[("Pekka", (0, 1500))]);
    let chp: Vec<Option<i32>> = run(&mut c, hc, None, false, 140, |s| low(s, hc)).iter().map(|f| f.hp).collect();
    let blow = chp.iter().position(|h| h.is_none()).expect("the control hero dies to the P.E.K.K.A") as u32;
    assert!(blow <= LAST_RAGED, "the control dies inside the rage's window (k = {blow}), so the scene tests the rage");
    let floored = hp.iter().position(|h| *h == Some(1)).expect("the raged hero is struck to 1") as u32;
    assert!(hp[floored as usize..=LAST_RAGED as usize].iter().all(|h| *h == Some(1)), "at 1 hp to the rage's end: {hp:?}");
    let dies = hp.iter().position(|h| h.is_none()).expect("the hero dies after the rage") as u32;
    assert!(dies > LAST_RAGED, "it dies after the rage (k = {dies})");
}

#[test]
fn the_bear_takes_its_next_target_on_the_loss_plus_1() {
    // Red Skeletons in front of the hero: one hit kills a Skeleton (the raged 167 and the plain 102 alike). The loss L
    // is the first frame whose target reads none (or dead); the gap is the frames from L to the next target. The plain
    // hero waits (combat.POST_KILL_RETARGET_WAIT: L + 6, the unpressed scene sp-form-Berserker-hero-nopress-s0 too:
    // a Skeleton lost on t203, the Knight its target on t209).
    let gap = |press: bool| -> Vec<(u32, u32)> {
        let (mut s, hid) = scene((9000, 11500), &[("Skeletons", (0, 2000))]);
        let fr = run(&mut s, hid, None, press, 80, |_| {});
        let mut gaps = Vec::new();
        let mut k = 1;
        while k < fr.len() {
            if fr[k].target.is_none() && fr[k - 1].target.is_some() && fr[k - 1].attack_ms != 0 {
                let back = (k + 1..fr.len()).find(|&j| fr[j].target.is_some());
                if let Some(j) = back {
                    gaps.push((k as u32, (j - k) as u32));
                    k = j;
                    continue;
                }
            }
            k += 1;
        }
        gaps
    };
    let control = gap(false);
    let raged: Vec<(u32, u32)> = gap(true).into_iter().filter(|g| g.0 <= LAST_RAGED).collect();
    assert!(!raged.is_empty(), "the raged hero kills a Skeleton inside the rage");
    assert!(raged.iter().all(|g| g.1 == 1), "the bear takes its next target on L + 1: {raged:?}");
    assert!(!control.is_empty() && control.iter().all(|g| g.1 == 6), "the plain hero waits to L + 6: {control:?}");
}

#[test]
fn the_press_is_one_charge_for_3_elixir() {
    let (mut s, _) = scene((3500, 11500), &[]);
    let (m, u) = s.elixir_raw(Team::Blue);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (m2, _) = s.elixir_raw(Team::Blue);
    assert_eq!((m - m2) * 1000 / u, 3000, "ManaCost 3");
    s.tick();
    assert_eq!(s.press_ability_button(Team::Blue, 0), Err(DeployError::AbilitySpent));
}
