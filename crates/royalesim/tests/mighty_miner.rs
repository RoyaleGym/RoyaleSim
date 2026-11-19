//! THE MIGHTY MINER'S BUTTON (tools/extract_cards.py `champion_lane_switch`; card.rs `LaneSwitchDef`, `UnitUse::LaneSwitchBomb`,
//! `CardDef::dropped_by_ability`; spell.rs `death_bomb_push`; state.rs `LaneRun`, `lane_pass`, `tunnel_step`, `surface`),
//! at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-champ-MightyMiner-s0; P the press's issue tick, t196): the cast holds
//! him P + 1 .. P + 9, he stands on the trigger P + 10 and drops his bomb there; under ground from P + 11 (the Knight that
//! held him took a tower that tick), (11314, 13178) to (10667, 13224), (10017, 13237), (9367, 13242), (8717, 13243),
//! (8067, 13244), (7417, 13244) on P + 11 .. P + 16 and his point (6686, 13178) on P + 17, the other lane's mirror; there
//! he deploys to P + 35 and attacks from P + 37; the bomb's 332 (130 at level 11) on a Knight 3486 from it on P + 30, and
//! the knockback ladder of 1800 from P + 31 (its first steps 245 and 247 on the diagonal: 250 capped).
//!
//! THE SCENE: the Mighty Miner held on the measured point 40 ticks, a red Knight held 2000 west of him (out of every crown
//! tower's reach) until the bomb's tick; then the press.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! mighty_miner`):
//!   - lane_switch_never -> `his_press_drops_a_bomb_and_takes_him_under_ground_to_the_other_lane` red;
//!   - lane_under_at_trigger -> the same red (under ground on P + 10);
//!   - lane_bomb_unpushed -> `his_bomb_strikes_and_pushes_twenty_ticks_after_the_trigger` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::state::AbilityBombFuse;
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["MightyMiner", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (11314, 13178);
const KNIGHT: (i32, i32) = (9314, 13178);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// One row after each tick, indexed from P (the press's issue tick): the Miner's (x, y), whether he is under ground and
/// deploying; the Knight's (x, y), hitpoints and target.
#[derive(Clone, Copy, Debug)]
struct Row {
    miner: (i32, i32),
    under: bool,
    deploying: bool,
    knight: (i32, i32),
    knight_hp: i32,
    knight_target: Option<EntityId>,
}

/// The scene, `ticks` rows from P + 1; the Knight held on its point through P + `hold_knight`.
fn scene(ticks: u32, hold_knight: u32) -> (Vec<Row>, EntityId) {
    scene_at(ticks, hold_knight, AbilityBombFuse::Client15535TickAfterFuse)
}

/// `scene` under spawner.ABILITY_BOMB_FUSE = `fuse`.
fn scene_at(ticks: u32, hold_knight: u32, fuse: AbilityBombFuse) -> (Vec<Row>, EntityId) {
    let mut cfg = config();
    cfg.calib.ability_bomb_fuse = fuse;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let mm = s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the Mighty Miner");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT), None).expect("a red Knight");
    for _ in 0..40 {
        assert!(s.debug_set_pos(mm, n(AT)));
        assert!(s.debug_set_pos(kn, n(KNIGHT)));
        s.tick();
    }
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = vec![];
    for k in 1..=ticks {
        if k <= hold_knight {
            assert!(s.debug_set_pos(kn, n(KNIGHT)));
        }
        s.tick();
        assert_eq!(s.tick_count() - 1 - p, k, "the row's tick");
        let m = s.entity(mm).expect("the Mighty Miner lives");
        let kv = s.entity(kn).expect("the Knight lives");
        rows.push(Row {
            miner: (m.pos.x / K, m.pos.y / K),
            under: m.tunnel_dest.is_some(),
            deploying: m.deploying,
            knight: (kv.pos.x / K, kv.pos.y / K),
            knight_hp: kv.hp,
            knight_target: kv.target,
        });
    }
    (rows, mm)
}

#[test]
fn his_press_drops_a_bomb_and_takes_him_under_ground_to_the_other_lane() {
    let (rows, mm) = scene(40, 40);
    let row = |k: usize| rows[k - 1];
    for k in 1..=10 {
        assert_eq!((row(k).miner, row(k).under), (AT, false), "P + {k}: he stands, above ground: {:?}", row(k));
    }
    assert!(row(10).knight_target == Some(mm), "the scene drifted: the Knight is not on him on the trigger: {:?}", row(10));
    let path = [(10667, 13224), (10017, 13237), (9367, 13242), (8717, 13243), (8067, 13244), (7417, 13244)];
    for (j, want) in path.iter().enumerate() {
        let k = 11 + j;
        assert!(row(k).under, "P + {k}: under ground: {:?}", row(k));
        assert_eq!(row(k).miner, *want, "P + {k}: his step under ground (measured)");
    }
    assert_ne!(row(11).knight_target, Some(mm), "the Knight drops him on P + 11");
    assert_eq!((row(17).miner, row(17).under), ((18000 - AT.0, AT.1), false), "P + 17: up on the other lane's mirror point");
    for k in 17..=35 {
        assert!(row(k).deploying, "P + {k}: deploying after he came up");
    }
    assert!(!row(36).deploying && row(36).miner == row(35).miner, "P + 36: deployed, not moved");
}

#[test]
fn his_bomb_strikes_and_pushes_twenty_ticks_after_the_trigger() {
    let (rows, _) = scene(34, 30);
    let row = |k: usize| rows[k - 1];
    // The Knight stands in a princess tower's reach: its arrows (109) land too.
    let lost: Vec<(usize, i32)> = (12..=34).map(|k| (k, row(k - 1).knight_hp - row(k).knight_hp)).filter(|(_, d)| *d != 0).collect();
    let at30 = lost.iter().find(|r| r.0 == 30).map(|r| r.1);
    assert!(matches!(at30, Some(332) | Some(441)), "the bomb's 332 (130 at level 11) on P + 30: {lost:?}");
    assert!(lost.iter().all(|&(k, d)| k >= 30 || d == 109), "nothing but the towers' arrows before P + 30: {lost:?}");
    let step = (row(31).knight.0 - row(30).knight.0, row(31).knight.1 - row(30).knight.1);
    assert_eq!(step, (-250, 0), "P + 31: the ladder of 1800's first step, 250 capped, away from the bomb");
}

/// His button is a champion's (state.rs `champion_button`): reported so, and charged, before the press.
#[test]
fn his_button_is_a_champions() {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the champion");
    for _ in 0..30 {
        s.tick();
    }
    let b = s.ability_buttons(Team::Blue);
    assert_eq!(b.len(), 1, "one button: his");
    assert!(b[0].champion && b[0].available, "a champion's button, charged: {:?}", b[0]);
}

/// spawner.ABILITY_BOMB_FUSE = client16402_two_ticks_after_fuse (client 16.402, parity's r63 census: the blow on the drop +
/// 21, 2 of 2, where 15.535.29's came on the drop + 20): the bomb's 332 on P + 31, its ladder's first step on P + 32.
/// Plant: ability_bomb_fuse_unread.
#[test]
fn under_client16402_two_ticks_after_fuse_his_bomb_strikes_a_tick_later() {
    let (rows, _) = scene_at(35, 31, AbilityBombFuse::Client16402TwoTicksAfterFuse);
    let row = |k: usize| rows[k - 1];
    let lost: Vec<(usize, i32)> = (12..=35).map(|k| (k, row(k - 1).knight_hp - row(k).knight_hp)).filter(|(_, d)| *d != 0).collect();
    let at31 = lost.iter().find(|r| r.0 == 31).map(|r| r.1);
    assert!(matches!(at31, Some(332) | Some(441)), "the bomb's 332 on P + 31: {lost:?}");
    assert!(lost.iter().all(|&(k, d)| k >= 31 || d == 109), "nothing but the towers' arrows before P + 31: {lost:?}");
    let step = (row(32).knight.0 - row(31).knight.0, row(32).knight.1 - row(31).knight.1);
    assert_eq!(step, (-250, 0), "P + 32: the ladder of 1800's first step, 250 capped, away from the bomb");
}

/// The Mighty Miner at AT pressed (no Knight in his way), under movement.LANE_SWITCH_FIRST_STEP_CONTACT = `arm`; when `red_at` is
/// given, a red Knight held there until the tick before his first underground step: (the tick, from P, of his first step under
/// ground; his point after it; the Knight's point after each of that tick and the next).
fn first_step_push(arm: royalesim::state::LaneSwitchFirstStepContact, red_at: Option<Vec2>) -> (u32, Vec2, Option<(Vec2, Vec2)>) {
    let mut cfg = config();
    cfg.calib.lane_switch_first_step_contact = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let mm = s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the Mighty Miner");
    for _ in 0..40 {
        assert!(s.debug_set_pos(mm, n(AT)));
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    // the first underground step's tick, from a bare run of the same scene (the press's ticks do not depend on the Knight)
    let kn = red_at.map(|p| s.scenario_spawn_now(Team::Red, "Knight", p, None).expect("a red Knight"));
    let mut under_at = None;
    let mut knight = None;
    for k in 1..=40u32 {
        let first = under_at.is_none();
        if let (Some(kn), Some(p), true) = (kn, red_at, first) {
            if s.entity(kn).is_some() {
                assert!(s.debug_set_pos(kn, p));
            }
        }
        s.tick();
        let m = s.entity(mm).expect("him");
        if first && m.status_flags & 1 == 1 {
            under_at = Some((k, m.pos));
            let a = kn.and_then(|kn| s.entity(kn)).map(|e| e.pos);
            s.tick();
            let b = kn.and_then(|kn| s.entity(kn)).map(|e| e.pos);
            knight = a.zip(b);
            break;
        }
    }
    let (k, p) = under_at.expect("the scene drifted: he never went under ground");
    (k, p, knight)
}

/// movement.LANE_SWITCH_FIRST_STEP_CONTACT = client16635_first_step_pushes (client 16.635, parity r94, live15n 20261103-191213-A
/// t538: a Hog Rider his first underground step landed on was pushed the 150 cap straight away from his new point): a red Knight
/// put 600 past his first underground point is pushed on that tick, away from it and by at most 150, against its own step under
/// tunnel (the vacuity check: met by nothing); on the next tick no second push (his later steps push nothing). Plant: lane_first_step_unpushed.
#[test]
fn under_client16635_first_step_pushes_his_first_underground_step_pushes_a_knight_he_lands_on() {
    use royalesim::state::LaneSwitchFirstStepContact as L;
    let (k, p, _) = first_step_push(L::Tunnel, None);
    let (k2, p2, _) = first_step_push(L::Client16635FirstStepPushes, None);
    assert_eq!((k, p), (k2, p2), "the scene: the two arms put his first step on one tick and point");
    // 600 further along his step (he heads for the other lane's mirror point, the arena's width less his x)
    let dir_x = if p.x < n(AT).x { -1 } else { 1 };
    let red = Vec2::new(p.x + dir_x * 600 * K, p.y);
    let old = first_step_push(L::Tunnel, Some(red)).2.expect("the Knight stands");
    let new = first_step_push(L::Client16635FirstStepPushes, Some(red)).2.expect("the Knight stands");
    let push = ((new.0.x - old.0.x) / K, (new.0.y - old.0.y) / K);
    let away = ((red.x - p.x) / K, (red.y - p.y) / K);
    assert!(push != (0, 0), "client16635_first_step_pushes: the Knight he landed on was not pushed (tunnel {old:?}, new {new:?})");
    assert!(push.0 * away.0 + push.1 * away.1 > 0, "client16635_first_step_pushes: the push {push:?} is not away from his point (Knight - Miner {away:?})");
    assert!(push.0 * push.0 + push.1 * push.1 <= 151 * 151, "client16635_first_step_pushes: the push {push:?} is past the 150 cap");
    // the next tick: no second push (the gap between the two arms' Knights grows by less than a push; their steps may part a
    // little, as the client's Hog's did after its push)
    let gap1 = ((new.0.x - old.0.x) / K, (new.0.y - old.0.y) / K);
    let gap2 = ((new.1.x - old.1.x) / K, (new.1.y - old.1.y) / K);
    let grew = (gap2.0 - gap1.0, gap2.1 - gap1.1);
    assert!(grew.0 * grew.0 + grew.1 * grew.1 < 60 * 60, "the next tick: a second push ({gap1:?} -> {gap2:?})");
}

/// Since parity r94 (tunnel before it; client 15.535.29 keeps tunnel).
#[test]
fn the_shipped_arm_is_client16635_first_step_pushes() {
    use royalesim::state::{Calib, LaneSwitchFirstStepContact};
    assert_eq!(Calib::shipped().lane_switch_first_step_contact, LaneSwitchFirstStepContact::Client16635FirstStepPushes);
    assert_eq!(config().calib.lane_switch_first_step_contact, LaneSwitchFirstStepContact::Client16635FirstStepPushes);
}

/// The Mighty Miner at AT held 40 ticks with a red Archer held 4500 north of him shooting at him (his hitpoints kept), then
/// the press `delay` ticks later, under hide.TUNNEL_SHOT = `arm`: on the tick he goes under ground, whether an Archer's arrow
/// is in flight at him; then the Archer is killed, and the hitpoints he loses in the next 40 ticks (None: never under, or
/// dead).
fn lane_shot(arm: royalesim::state::TunnelShot, delay: u32) -> Option<(bool, i32)> {
    let mut cfg = config();
    cfg.calib.tunnel_shot = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let mm = s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the Mighty Miner");
    let aat = n((AT.0, AT.1 + 4500));
    let archer = s.scenario_spawn_now(Team::Red, "Archer", aat, None).expect("the Archer");
    let full = s.entity(mm).expect("him").max_hp;
    for _ in 0..40 + delay {
        assert!(s.debug_set_pos(mm, n(AT)) && s.debug_set_hp(mm, full));
        if s.entity(archer).is_some() {
            assert!(s.debug_set_pos(archer, aat));
        }
        s.tick();
    }
    assert!(s.debug_set_hp(mm, full));
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..40u32 {
        if s.entity(archer).is_some() {
            assert!(s.debug_set_pos(archer, aat));
        }
        s.tick();
        let m = s.entity(mm)?;
        if m.status_flags & 1 == 1 {
            let flying = s.projectiles().iter().any(|p| p.team == Team::Red && p.target == mm);
            let hp = m.hp;
            if s.entity(archer).is_some() {
                assert!(s.debug_set_hp(archer, 0));
            }
            for _ in 0..40 {
                s.tick();
            }
            return Some((flying, hp - s.entity(mm)?.hp));
        }
    }
    None
}

/// hide.TUNNEL_SHOT = client16635_dropped (client 16.635, parity r95, the oracle's il-mg-16635 set: 7 of 7 shots in flight at a
/// switching Miner ended on his dig point with no damage): an Archer's arrow in flight at him as he goes under ground takes
/// nothing from him; under lands (the vacuity check) it lands on him. Plant: tunnel_shot_lands.
#[test]
fn under_client16635_dropped_an_arrow_in_flight_as_he_goes_under_takes_nothing() {
    use royalesim::state::TunnelShot as T;
    let delay = (0..30u32)
        .find(|&d| lane_shot(T::Lands, d).is_some_and(|(flying, _)| flying))
        .expect("the scene drifted: no press delay leaves an arrow in flight at him as he goes under");
    let (_, old) = lane_shot(T::Lands, delay).expect("lands: the scene");
    assert!(old > 0, "lands (the vacuity check): the arrow in flight took nothing (delay {delay})");
    let (flying, new) = lane_shot(T::Client16635Dropped, delay).expect("client16635_dropped: the scene");
    assert!(flying, "the scene: no arrow in flight under the new arm");
    assert_eq!(new, 0, "client16635_dropped: the arrow in flight took {new} from him (delay {delay})");
}

/// Since parity r96 for clients 16.402 and 16.635 (lands before it; client 15.535.29 keeps lands).
#[test]
fn the_shipped_arm_is_client16635_dropped() {
    assert_eq!(royalesim::state::Calib::shipped().tunnel_shot, royalesim::state::TunnelShot::Client16635Dropped);
    assert_eq!(config().calib.tunnel_shot, royalesim::state::TunnelShot::Client16635Dropped);
}


/// The trigger-tick scene under hide.LANE_SWITCH_UNDER_TICK = `arm`: as `scene`, with no Knight and a red Musketeer held at
/// (14500, 22500), out of everyone's reach, as a shot's firer; right after frame P + `put_after` a shot of the Musketeer's is
/// put on the Miner, landing in the next tick's Projectile phase. (his hitpoints on P + put_after and P + put_after + 1, and his
/// (point, under ground) on P + 10 and P + 11.)
#[allow(clippy::type_complexity)]
fn trigger_hit(arm: royalesim::state::LaneSwitchUnderTick, put_after: u32) -> (i32, i32, ((i32, i32), bool), ((i32, i32), bool)) {
    let mut cfg = config();
    cfg.calib.ability_bomb_fuse = AbilityBombFuse::Client15535TickAfterFuse;
    cfg.calib.lane_switch_under_tick = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let mm = s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the Mighty Miner");
    let mu_at = n((14500, 22500));
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", mu_at, None).expect("a red Musketeer");
    for _ in 0..40 {
        assert!(s.debug_set_pos(mm, n(AT)) && s.debug_set_pos(musk, mu_at));
        s.tick();
    }
    // nothing in his reach: he walked a step on the last tick; back on his point before the press (the cast holds him from P + 1)
    assert!(s.debug_set_pos(mm, n(AT)));
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let (mut hp, mut rows) = (Vec::new(), Vec::new());
    for k in 1..=12u32 {
        assert!(s.debug_set_pos(musk, mu_at));
        if k == put_after + 1 {
            let at = s.entity(mm).expect("the Mighty Miner").pos;
            s.scenario_projectile_in_flight(musk, Some(mm), at, at).expect("the shot");
        }
        s.tick();
        let m = s.entity(mm).expect("the Mighty Miner lives");
        hp.push(m.hp);
        rows.push(((m.pos.x / K, m.pos.y / K), m.tunnel_dest.is_some()));
    }
    let p = put_after as usize;
    (hp[p - 1], hp[p], rows[9], rows[10])
}

/// hide.LANE_SWITCH_UNDER_TICK = client16635_trigger_tick (client 16.635, parity r97, the oracle's il-mg-16635 set: 7 switches hit
/// on the trigger tick in the engine, none in the client): a shot landing on the trigger tick P + 10 takes nothing; one landing
/// on P + 9 still lands (both arms); under first_step (the vacuity check) the P + 10 shot lands. He is under ground from P + 10's
/// frame, on his point, and takes his measured first step on P + 11. Plant: lane_trigger_tick_touchable.
#[test]
fn under_client16635_trigger_tick_a_shot_on_the_trigger_tick_does_not_land() {
    use royalesim::state::LaneSwitchUnderTick as U;
    for arm in [U::FirstStep, U::Client16635TriggerTick] {
        let (a, b, _, _) = trigger_hit(arm, 8);
        assert!(a > b, "{arm:?}: the shot on P + 9 did not land ({a} -> {b})");
    }
    let (a, b, _, _) = trigger_hit(U::FirstStep, 9);
    assert!(a > b, "first_step (the vacuity check): the shot on the trigger tick did not land ({a} -> {b})");
    let (a, b, r10, r11) = trigger_hit(U::Client16635TriggerTick, 9);
    assert_eq!(a, b, "client16635_trigger_tick: the shot on the trigger tick landed ({a} -> {b})");
    assert_eq!(r10, (AT, true), "client16635_trigger_tick: P + 10, on his point and under ground");
    assert_eq!(r11, ((10667, 13224), true), "client16635_trigger_tick: P + 11, his measured first step");
}

/// The tunnel scene under targeting.LANE_SWITCH_TARGET = `arm`: as `scene`, the red Knight held on KNIGHT throughout and a
/// second red Knight held at (7417, 14500), 1256 from his P + 16 point (nearer than the first's 1897). (target, attack phase,
/// attack progress, under ground) on P + 1 ..= P + 19, and the first Knight.
#[allow(clippy::type_complexity)]
fn tunnel_rows(arm: royalesim::state::LaneSwitchTarget) -> (Vec<(Option<EntityId>, royalesim::entity::AttackPhase, i32, bool)>, EntityId) {
    let mut cfg = config();
    cfg.calib.ability_bomb_fuse = AbilityBombFuse::Client15535TickAfterFuse;
    cfg.calib.lane_switch_target = arm;
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let mm = s.scenario_spawn_now(Team::Blue, "MightyMiner", n(AT), None).expect("the Mighty Miner");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT), None).expect("a red Knight");
    let k2_at = n((7417, 14500));
    let k2 = s.scenario_spawn_now(Team::Red, "Knight", k2_at, None).expect("a second red Knight");
    let hold = |s: &mut BattleState| {
        for (id, p) in [(kn, n(KNIGHT)), (k2, k2_at)] {
            if let Some(e) = s.entity(id) {
                let m = e.max_hp;
                assert!(s.debug_set_pos(id, p) && s.debug_set_hp(id, m));
            }
        }
    };
    for _ in 0..40 {
        assert!(s.debug_set_pos(mm, n(AT)));
        hold(&mut s);
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = Vec::new();
    for _ in 1..=19 {
        hold(&mut s);
        s.tick();
        let m = s.entity(mm).expect("the Mighty Miner lives");
        rows.push((m.target, m.attack_phase, m.attack_ms, m.tunnel_dest.is_some()));
    }
    (rows, kn)
}

/// targeting.LANE_SWITCH_TARGET = client16635_held_through_tunnel (client 16.635, parity r97, the oracle's il-mg-16635 set: 30 of
/// 30): under ground he keeps the Knight he held, never winds up, and holds nothing on his surfacing frame; under rescans (the
/// vacuity check) he re-picks or winds up under ground. Plant: lane_tunnel_rescans.
#[test]
fn under_client16635_held_through_tunnel_he_keeps_his_target_and_never_winds_up() {
    use royalesim::entity::AttackPhase;
    use royalesim::state::LaneSwitchTarget as A;
    let (old, kn) = tunnel_rows(A::Rescans);
    let row = |r: &Vec<(Option<EntityId>, AttackPhase, i32, bool)>, k: usize| r[k - 1];
    assert!(
        (11..=16).any(|k| row(&old, k).0 != Some(kn) || row(&old, k).1 != AttackPhase::Idle) || row(&old, 17).0.is_some(),
        "rescans (the vacuity check): he kept the Knight, idle, through the tunnel and up: {:?}",
        &old[9..17]
    );
    let (new, kn) = tunnel_rows(A::Client16635HeldThroughTunnel);
    for k in 10..=16 {
        assert_eq!(row(&new, k).0, Some(kn), "client16635_held_through_tunnel: P + {k}'s target {:?}", row(&new, k));
    }
    for k in 11..=16 {
        assert!(row(&new, k).3, "the scene drifted: P + {k} not under ground");
        assert_eq!((row(&new, k).1, row(&new, k).2), (AttackPhase::Idle, 0), "client16635_held_through_tunnel: P + {k} wound up");
    }
    assert!(!row(&new, 17).3, "the scene drifted: P + 17 still under ground");
    assert_eq!(row(&new, 17).0, None, "client16635_held_through_tunnel: a target on the surfacing frame");
}
