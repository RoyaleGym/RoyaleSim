//! combat.REFLECT_ATTACK: the Electro Giant answers each melee hit on him with damage and a stun on
//! the attacker (state.rs `reflect_melee_hit`, card.rs `ReflectDef`).
//!
//! THE LAW, measured on client 15.535.29 (its Electro Giant scenario): each hit a Knight lands on the
//! Electro Giant is answered in the same tick by 192 on the Knight (ReflectedAttackDamage 75 at level
//! 11) and by ZapFreeze for ReflectedAttackBuffDuration 500 ms, which holds the Knight's attack progress
//! for 9 ticks and then lets it run on from where it stood: the Knight's hit period goes from 24 ticks
//! to 33. The princess towers that shot him, from beyond the reach, got nothing. Under the old arm,
//! not_read, the engine is today's: the Knight hits every 24 ticks and takes nothing.
//!
//! THE DUEL is the one tests/test_reflect_attack.py runs through the Python binding: a Blue Electro
//! Giant (or a plain Giant) at (9000, 13500) and a Red Knight at (9000, 14700), from tick 200, out of
//! every tower's reach for the 110 ticks watched. An hp drop is read between ticks, so a hit and its
//! answer resolved in one tick fall on the same index.
//!
//! WHAT IS PINNED
//!   1. each answer falls on the tick of the hit it answers, and the stunned period is 33 -- which is
//!      the Knight's HitSpeed / TICK_MS plus the buff's ticks less the hit's own, read from the data;
//!   2. each answer is 192, and 192 is ReflectedAttackDamage scaled at the battle's level;
//!   3. a princess tower's shot at him from beyond the reach is not answered (the measured case);
//!   4. a Musketeer's shot launched from INSIDE the reach is not answered either: the engine answers
//!      a hit that lands in the attacker's own pass, never a shot. The measurement has no shot from
//!      inside the reach, so this pins the engine's reading of the ledger key's open item;
//!   5. an Inferno Dragon's hit, which is not a shot, is not answered from BEYOND the reach, with
//!      the Giant's own radius to spare, so it holds whichever radius the reach is read with;
//!   6. a plain Giant, whose card carries no reflect, answers nothing (the control);
//!   7. under not_read the Knight hits on ticks 10, 34, 58 and 82 and takes nothing: today's engine;
//!   8. the card table gives the Electro Giant a reflect with a full-stop buff, and no other card
//!      one. RED while data/derived/cards.json predates the extractor's `reflected_attack` block,
//!      and so is every test above that runs the new arm.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test
//! reflect_attack`), each aimed at one gate:
//!   * `reflect_stun_buffered` -- the stun goes through the effect buffer and lands in Resolve: the
//!     period is 34 and (1) goes red.
//!   * `reflect_damage_unscaled` -- the level-1 column, 75: (2) goes red.
//!   * `reflect_every_attack` -- every attack on him is answered, a shot from any distance included:
//!     (3) goes red, and (4) and (5) with it.
//!   * `reflect_answers_shots` -- a shot is answered at its launch, the reach still read: (4) goes
//!     red and (3) stays green, its tower being beyond the reach.
//!   * `reflect_ignores_reach` -- a hit is answered from any distance, a shot still not: (5) goes
//!     red and (3) and (4) stay green.
//!   * `reflect_ignores_victim_card` -- every melee hit is answered as the Electro Giant's would be:
//!     (6) goes red.
//!   * `reflect_ignores_key` -- the reflect runs whatever combat.REFLECT_ATTACK says: (7) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, ReflectAttack};
use royalesim::status::{compose, Sel};
use royalesim::Team;

/// Measured on client 15.535.29: the answer on a level-11 Knight, and its hit period with and
/// without the stun.
const REFLECT: i32 = 192;
const PERIOD_STUNNED: u32 = 33;
const PERIOD_FREE: u32 = 24;

const REGENERATE: &str = "cards.json carries no reflected_attack on the Electro Giant: regenerate it with tools/extract_cards.py";

/// The hp drops, (tick, amount), of the Blue `card` and of the Red Knight over `ticks` ticks of the
/// duel, under `arm`.
#[allow(clippy::type_complexity)]
fn duel(card: &str, arm: ReflectAttack, ticks: u32) -> (Vec<(u32, i32)>, Vec<(u32, i32)>) {
    let mut cfg = config();
    cfg.calib.reflect_attack = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(200);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, card, Vec2::new(9000 * K, 13500 * K), None), (Team::Red, "Knight", Vec2::new(9000 * K, 14700 * K), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let mut prev = [None::<i32>; 2];
    let mut drops: [Vec<(u32, i32)>; 2] = [Vec::new(), Vec::new()];
    for t in 0..ticks {
        for ((id, last), out) in ids.iter().zip(prev.iter_mut()).zip(drops.iter_mut()) {
            let Some(e) = s.entity(*id) else { continue };
            if let Some(p) = last.filter(|p| e.hp < *p) {
                out.push((t, p - e.hp));
            }
            *last = Some(e.hp);
        }
        s.tick();
    }
    let [on_card, on_knight] = drops;
    (on_card, on_knight)
}

fn ticks_of(drops: &[(u32, i32)], n: usize) -> Vec<u32> {
    drops.iter().take(n).map(|d| d.0).collect()
}

fn gaps(ticks: &[u32]) -> Vec<u32> {
    ticks.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn each_hit_on_the_electro_giant_is_answered_in_its_tick_and_the_stun_stretches_the_period_to_33() {
    // The period's operands, from the data: the cycle is HitSpeed / TICK_MS ticks, and the stun
    // holds the buff's ticks less one, because the hit's own tick is the first of them and its
    // attack pass has already run.
    let s = BattleState::new(0, config());
    let tick = s.config().calib.tick_ms;
    let cycle = card_stat(&s, "Knight").hit_speed_ms / tick;
    let stun = card_stat(&s, "ElectroGiant").reflect.expect(REGENERATE).buff.expect("the reflect carries its stun").time_ms;
    assert_eq!(cycle, PERIOD_FREE as i32, "the Knight's cycle this file was measured on");
    assert_eq!(cycle + stun / tick - 1, PERIOD_STUNNED as i32, "33 = the cycle + the held ticks");

    let (on_giant, on_knight) = duel("ElectroGiant", ReflectAttack::ClientReflectStun, 110);
    // THE FIRST TWO HITS ONLY: both land in reach. The third lands with the Giant 3117 away (it walks on
    // during the stun), beyond the Knight's reach and the reflect radius; the engine still lands that hit,
    // where the client cancels a hit whose target has left its reach, so its answer is not pinned here.
    assert!(on_giant.len() >= 2, "the Knight landed fewer than two hits: {on_giant:?}");
    let hits = ticks_of(&on_giant, 2);
    assert_eq!(ticks_of(&on_knight, 2), hits, "each answer falls on the tick of the hit it answers: {on_knight:?}");
    assert_eq!(gaps(&hits), vec![PERIOD_STUNNED; 1], "the stunned Knight hits 33 ticks later: {on_giant:?}");
}

#[test]
fn each_answer_is_192_the_reflected_attack_damage_at_the_battles_level() {
    let (_, on_knight) = duel("ElectroGiant", ReflectAttack::ClientReflectStun, 110);
    // the two in-reach hits (the first test says why not the third)
    let amounts: Vec<i32> = on_knight.iter().take(2).map(|d| d.1).collect();
    assert_eq!(amounts, vec![REFLECT; 2], "{on_knight:?}");
    let s = BattleState::new(0, config());
    let db = s.cards();
    let eg = db.index("ElectroGiant").expect("the Electro Giant loads");
    let r = db.get(eg).reflect.expect(REGENERATE);
    let level = s.config().card_level[Team::Blue as usize];
    assert_eq!(db.scaled(eg, level, r.damage), Ok(REFLECT), "192 is the column at the battle's level, not a number typed twice");
}

#[test]
fn a_princess_towers_shot_from_beyond_the_reach_is_not_answered() {
    let mut cfg = config();
    cfg.calib.reflect_attack = ReflectAttack::ClientReflectStun;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(200);
    let tower = s.tower_ids(Team::Red)[1].expect("Red's engine-left princess tower");
    let (at, full, radius) = {
        let w = s.entity(tower).expect("the tower stands");
        (w.pos, w.hp, w.radius)
    };
    // The centre distance inside which the tower's edge is within the reach.
    let reach = card_stat(&s, "ElectroGiant").reflect.expect(REGENERATE).radius + radius;
    // Straight toward the river, 3000 millitiles beyond that: inside the tower's range, outside
    // the reach, as the measured towers were. He walks in to attack it; the window closes there.
    let off = reach + 3000 * K;
    let y = if at.y * 2 > s.arena().height { at.y - off } else { at.y + off };
    let eg = s
        .scenario_spawn_batch(&[(Team::Blue, "ElectroGiant", Vec2::new(at.x, y), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"))[0];
    let mut prev = None::<i32>;
    let mut shots = 0;
    for _ in 0..200 {
        let (hp, pos) = {
            let g = s.entity(eg).expect("the Electro Giant lives");
            (g.hp, g.pos)
        };
        let w = s.entity(tower).expect("the tower stands");
        if pos.dist2(w.pos) <= (reach as i64) * (reach as i64) {
            break;
        }
        assert_eq!(w.hp, full, "the tower took an answer to a shot from beyond the reach");
        assert_eq!(w.stun_ms, 0, "the tower was stunned by an answer to a shot from beyond the reach");
        assert!(w.buffs.iter().all(|b| b.is_empty()), "the tower carries a buff: {:?}", w.buffs);
        if prev.is_some_and(|p| hp < p) {
            shots += 1;
        }
        prev = Some(hp);
        s.tick();
    }
    assert!(shots >= 2, "only {shots} tower shots landed on him from beyond the reach, so this looked at nothing");
}

/// A Red `attacker` at `at` against the Blue Electro Giant at the duel's spot, under the new arm,
/// for `ticks` ticks from tick 200. Returns (the squared centre distance at each of the attacker's
/// hits or launches, the attacker's radius, his radius). The distance is read from the positions
/// the attack pass used, the ones before the tick, because Attack runs before Move; a tick after
/// which the attacker's phase is Cooldown is one it hit or launched in (combat.ATTACK_CYCLE =
/// progress_credit). Panics on the tick anything answers the attacker: it takes no damage, no
/// stun and no buff for the whole run. Nothing else can touch it, because he attacks buildings
/// only and both spots are out of every tower's reach for the run.
fn attacks_on_the_electro_giant(attacker: &str, at: Vec2, ticks: u32) -> (Vec<i64>, i32, i32) {
    let mut cfg = config();
    cfg.calib.reflect_attack = ReflectAttack::ClientReflectStun;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(200);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "ElectroGiant", Vec2::new(9000 * K, 13500 * K), None), (Team::Red, attacker, at, None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (giant, foe) = (ids[0], ids[1]);
    let (full, foe_radius, giant_radius) = {
        let (g, w) = (s.entity(giant).expect("he stands"), s.entity(foe).expect("the attacker stands"));
        (w.hp, w.radius, g.radius)
    };
    let mut attacks = Vec::new();
    for t in 0..ticks {
        let d2 = {
            let (g, w) = (s.entity(giant).expect("he lives"), s.entity(foe).expect("the attacker lives"));
            g.pos.dist2(w.pos)
        };
        s.tick();
        let w = s.entity(foe).expect("the attacker lives");
        assert_eq!(w.hp, full, "the {attacker} took an answer on tick {t}");
        assert_eq!(w.stun_ms, 0, "the {attacker} was stunned on tick {t}");
        assert!(w.buffs.iter().all(|b| b.is_empty()), "the {attacker} carries a buff on tick {t}: {:?}", w.buffs);
        if w.attack_phase == AttackPhase::Cooldown {
            attacks.push(d2);
        }
    }
    (attacks, foe_radius, giant_radius)
}

#[test]
fn a_shot_launched_from_inside_the_reach_is_not_answered() {
    // The engine's reading of the ledger key's open item: a shot is never answered, launched from
    // inside the reach or not. A Musketeer at the Knight's spot launches on ticks 13, 33 and 53
    // from inside the reach while he walks off toward the bridge (seen through the Python binding
    // on the shared build of 2026-09-25 (b7ef7e4), where nothing answers it either).
    let s = BattleState::new(0, config());
    assert!(card_stat(&s, "Musketeer").projectile.is_some(), "the Musketeer shoots");
    let radius = card_stat(&s, "ElectroGiant").reflect.expect(REGENERATE).radius;
    let (launches, foe, _) = attacks_on_the_electro_giant("Musketeer", Vec2::new(9000 * K, 14700 * K), 110);
    // The reach as the engine reads it, the attacker's edge within the radius of his centre.
    let reach = (radius + foe) as i64;
    let inside = launches.iter().filter(|d2| **d2 <= reach * reach).count();
    assert!(inside >= 2, "only {inside} of {} launches left from inside the reach, so this looked at nothing", launches.len());
}

#[test]
fn a_hit_from_beyond_the_reach_is_not_answered() {
    // The Inferno Dragon's hit is not a shot: it lands in its own attack pass like the Knight's,
    // so the reach alone stands between it and an answer. Flying at (9000, 18000) it hits him
    // every 8 ticks from 3,780 to 4,734 millitiles centre to centre while he walks off (seen the
    // same way as the Musketeer's launches above).
    let s = BattleState::new(0, config());
    assert!(card_stat(&s, "InfernoDragon").projectile.is_none(), "the Inferno Dragon's hit is not a shot");
    let radius = card_stat(&s, "ElectroGiant").reflect.expect(REGENERATE).radius;
    let (hits, foe, giant) = attacks_on_the_electro_giant("InfernoDragon", Vec2::new(9000 * K, 18000 * K), 90);
    // Beyond the reach even with his own radius added to it: the measured reflect at 2,008 centre
    // to centre says a radius is added to ReflectedAttackRadius, and not whose, so every hit
    // counted here is beyond the reach under either reading.
    let far = (radius + foe + giant) as i64;
    assert!(hits.len() >= 3, "the Inferno Dragon hit him only {} times, so this looked at nothing", hits.len());
    assert!(hits.iter().all(|d2| *d2 > far * far), "a hit landed from within {far} subtiles of his centre: {hits:?}");
}

#[test]
fn a_plain_giant_answers_nothing() {
    let (on_giant, on_knight) = duel("Giant", ReflectAttack::ClientReflectStun, 110);
    assert!(on_giant.len() >= 3, "the Knight landed fewer than three hits on the Giant: {on_giant:?}");
    assert!(on_knight.is_empty(), "a Giant's card carries no reflect, and the Knight took {on_knight:?}");
    assert_eq!(gaps(&ticks_of(&on_giant, 3)), vec![PERIOD_FREE; 2], "an unstunned Knight: {on_giant:?}");
}

#[test]
fn under_not_read_the_knight_hits_every_24_ticks_and_takes_nothing() {
    // Today's engine, measured on the shared build of 2026-09-25 (b7ef7e4) through the Python
    // binding with the key absent: hits on 10, 34, 58 and 82, nothing back.
    let (on_giant, on_knight) = duel("ElectroGiant", ReflectAttack::NotRead, 110);
    assert!(on_knight.is_empty(), "the old arm answers nothing, and the Knight took {on_knight:?}");
    assert_eq!(ticks_of(&on_giant, 4), vec![10, 34, 58, 82], "{on_giant:?}");
    assert_eq!(gaps(&ticks_of(&on_giant, 3)), vec![PERIOD_FREE; 2]);
}

#[test]
fn the_card_table_gives_the_electro_giant_a_full_stop_reflect_and_no_other_card_one() {
    let s = BattleState::new(0, config());
    let r = card_stat(&s, "ElectroGiant").reflect.expect(REGENERATE);
    let b = r.buff.expect("the Electro Giant's reflect carries its stun");
    let def = s.cards().buffs[b.buff as usize];
    assert_eq!(compose([def].iter(), Sel::Speed, 100), 0, "the stun is a full stop, so it drives the hold timer");
    assert_eq!(compose([def].iter(), Sel::HitSpeed, s.config().calib.tick_ms), 0, "and it holds the attack progress");
    assert!(r.radius > 0 && r.damage > 0, "{r:?}");
    let carriers: Vec<&str> = s.cards().cards.iter().filter(|c| c.reflect.is_some()).map(|c| c.name.as_str()).collect();
    assert_eq!(carriers, vec!["ElectroGiant"], "the 15.535 table sets the ReflectedAttack columns on one row");
}
