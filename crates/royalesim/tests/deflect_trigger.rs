//! status.DEFLECT_TRIGGER_TICK: the tick the Monk's Deflect lands (card.rs `AbilityEffect::Deflect`; state.rs
//! `press_ability_button`) and the tick his deflect window opens (state.rs `fire_ability`, `deflect_open`).
//!
//! THE LAW, measured on client 16.635 (parity r99, the oracle's il-mg-16635 set: his stand state began on P + 13 in 13 of
//! 13 presses; 2 of 2 hits on P + 14 cut to 35 %, 2 of 2 on P + 13 whole): the Deflect lands on the tick whose 50 ms step
//! completes its TriggerDelay, counted from the cast's first frame P: on P + ceil(TriggerDelay / 50) - 1. On 16.635's
//! table (700) that is P + 13, where the engine's arm, whole_ticks, lands it on P + floor(TriggerDelay / 50), P + 14. For
//! 933 (clients 15.535.29 and 16.402) both give P + 18.
//!
//! THE WINDOW (re-specced by parity r100, the oracle's il-mg-16635 set: his neighbours met him as an obstacle from P + 14,
//! 2 of 2; the shots on P + 14 were caught and returned on P + 14, the one landing on P + 13 was not returned): only the
//! buff and his active state come on that tick. His deflect window (his neighbours' static obstacle, the catch at his
//! area's edge, the returns) keeps the whole-tick count, P + floor(TriggerDelay / 50), for its active ticks: P + 14 ..
//! P + 93 for 700, a tick after the buff; for 933 the two counts agree. The stay keeps its end.
//!
//! THE SCENE: blue's level-11 Monk alone, held on AT until his press, status.DEFLECT_BUFF_LANDING =
//! client_after_hits_tick_short (the trigger tick's hits in full, the next ones at 35 %); P is the first tick after the
//! press, the cast's first frame; a red Zap cast so it lands on P + k, against its full damage on an unpressed twin.
//! The window's scene adds a red Musketeer held 5,000 up his lane: X is the first tick whose step brings one of her shots
//! inside his deflect area (read off an unpressed twin), and the press puts P on X less 13 (less 18 on 933's table).
//!
//! WHAT IS PINNED: on 16.635's table under client16635_last_partial_tick a Zap on P + 13 lands in full and one on P + 14
//! at 35 %; under whole_ticks (the vacuity check) the one on P + 14 lands in full. On client 15.535.29's table (933),
//! under both arms, a Zap on P + 18 lands in full and one on P + 19 at 35 %.
//! The window: on 16.635's table under client16635_last_partial_tick the shot inside his area on P + 13 is not caught
//! there (he loses nothing, it flies on, nothing comes back), and on P + 14 it is caught at 35 % and sent back; under
//! whole_ticks it is caught on P + 14 in full. On 933's table it is caught on P + 18 under both arms. The stay's end: in
//! tests/deflect_stay_time.rs's scene on 16.635's table his first move after the press comes on P + 93 under both arms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! deflect_trigger`):
//!   * `deflect_trigger_whole_ticks` -- the new arm still counts whole ticks:
//!     `under_client16635_last_partial_tick_a_700_ms_deflect_lands_on_p_plus_13` red (the Zap on P + 14 lands in full);
//!   * `deflect_window_at_trigger` -- the new arm opens the deflect window on the trigger tick:
//!     `under_client16635_last_partial_tick_the_deflect_window_opens_a_tick_after_its_buff` red (the shot inside his area
//!     on P + 13 is caught there and lands in full).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{AbilityEffect, CardDb};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, CardTable, DeflectBuffLanding, DeflectContact, DeflectStayTime, DeflectTriggerTick};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Monk", "Knight", "Archer", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
/// The Monk's point, held until his press.
const AT: (i32, i32) = (3500, 12500);
/// The ticks from the Monk's spawn to his press: room to cast the Zap before it.
const LEAD: u32 = 30;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// A level-11 battle on `table` under status.DEFLECT_TRIGGER_TICK = `arm` (status.DEFLECT_BUFF_LANDING =
/// client_after_hits_tick_short), past the opening lockout, blue's Monk put down on AT: the battle and his id.
fn monk_battle(table: CardTable, arm: DeflectTriggerTick) -> (BattleState, EntityId) {
    let mut cfg = BattleConfig::with_cards(CardDb::load_table(table).expect("the table loads"));
    cfg.calib.card_table = table;
    cfg.calib.deflect_trigger_tick = arm;
    cfg.calib.deflect_buff_landing = DeflectBuffLanding::ClientAfterHitsTickShort;
    let deck: Vec<String> = DECK.iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n(AT), None).expect("the Monk");
    (s, monk)
}

/// Under `table` and `arm`: the hp the Monk loses to a red Zap landing on P + `k` (P the first tick after his press, LEAD
/// ticks after his spawn), and the Zap's full damage on him unpressed. The Zap's flight is read off an unpressed twin:
/// cast before tick c, it lands on c + d.
fn zap_on(table: CardTable, arm: DeflectTriggerTick, k: u32) -> (i32, i32) {
    let (mut s, monk) = monk_battle(table, arm);
    let p = s.tick_count() + LEAD;
    let mut twin = s.clone();
    let c = twin.tick_count();
    let hp0 = twin.entity(monk).expect("the Monk").hp;
    twin.spawn_unit(Team::Red, "Zap", n(AT), None).expect("the Zap");
    let mut d = None;
    for _ in 0..20 {
        let before = twin.entity(monk).expect("the Monk").hp;
        twin.tick();
        if twin.entity(monk).expect("the Monk").hp < before {
            d = Some(twin.tick_count() - 1 - c);
            break;
        }
    }
    let d = d.expect("the Zap never landed on the unpressed Monk");
    let full = hp0 - twin.entity(monk).expect("the Monk").hp;
    let land = p + k;
    assert!(land >= c + d, "the Zap's flight ({d} ticks) is longer than the scene's lead");
    let mut lost = None;
    while s.tick_count() <= land {
        let now = s.tick_count();
        if now < p {
            assert!(s.debug_set_pos(monk, n(AT)), "the Monk lives");
        }
        if now == p {
            s.scenario_set_elixir_milli(Team::Blue, 10_000);
            s.press_ability_button(Team::Blue, 0).expect("the press");
        }
        if now == land - d {
            s.spawn_unit(Team::Red, "Zap", n(AT), None).expect("the Zap");
        }
        let before = s.entity(monk).expect("the Monk").hp;
        s.tick();
        if now == land {
            lost = Some(before - s.entity(monk).expect("the Monk").hp);
        }
    }
    (lost.expect("the row's tick"), full)
}

/// status.DEFLECT_TRIGGER_TICK = client16635_last_partial_tick (client 16.635, parity r99, the oracle's il-mg-16635 set:
/// 249c9115 t535 and 0b5f1c68 t2785, hits on P + 14 cut to 35 %): on 16.635's table (TriggerDelay 700) the Deflect lands on
/// P + 13, a Zap there in full and one on P + 14 at 35 %; under whole_ticks (the vacuity check) it lands on P + 14, the Zap
/// there in full. Plant: deflect_trigger_whole_ticks.
#[test]
fn under_client16635_last_partial_tick_a_700_ms_deflect_lands_on_p_plus_13() {
    let table = CardTable::Client160635007Nov03;
    let (s, _) = monk_battle(table, DeflectTriggerTick::WholeTicks);
    assert_eq!(card_stat(&s, "Monk").ability.as_ref().expect("the Monk's button").trigger_ms, 700, "16.635's TriggerDelay");
    let (old_14, full) = zap_on(table, DeflectTriggerTick::WholeTicks, 14);
    assert!(full > 0, "the scene drifted: the Zap took nothing off the unpressed Monk");
    // NOT VACUOUS: whole_ticks lands the Deflect on P + 14, that tick's hit in full.
    assert_eq!(old_14, full, "whole_ticks: the Zap on P + 14 (its trigger tick) lands in full");
    let (on_13, _) = zap_on(table, DeflectTriggerTick::Client16635LastPartialTick, 13);
    assert_eq!(on_13, full, "client16635_last_partial_tick: the Zap on P + 13 (its trigger tick) lands in full");
    let (on_14, _) = zap_on(table, DeflectTriggerTick::Client16635LastPartialTick, 14);
    assert!(on_14 > 0 && on_14 < full, "client16635_last_partial_tick: the Zap on P + 14 lands at 35 % ({on_14} of {full})");
}

/// On client 15.535.29's table (TriggerDelay 933) both arms land the Deflect on P + 18: a Zap there in full, one on P + 19
/// at 35 %.
#[test]
fn a_933_ms_deflect_lands_on_p_plus_18_under_both_arms() {
    let table = CardTable::Client15535;
    let (s, _) = monk_battle(table, DeflectTriggerTick::WholeTicks);
    assert_eq!(card_stat(&s, "Monk").ability.as_ref().expect("the Monk's button").trigger_ms, 933, "15.535.29's TriggerDelay");
    for arm in [DeflectTriggerTick::WholeTicks, DeflectTriggerTick::Client16635LastPartialTick] {
        let (on_18, full) = zap_on(table, arm, 18);
        assert!(full > 0, "the scene drifted: the Zap took nothing off the unpressed Monk");
        assert_eq!(on_18, full, "{arm:?}: the Zap on P + 18 (its trigger tick) lands in full");
        let (on_19, _) = zap_on(table, arm, 19);
        assert!(on_19 > 0 && on_19 < full, "{arm:?}: the Zap on P + 19 lands at 35 % ({on_19} of {full})");
    }
}

#[test]
fn the_shipped_arm_is_whole_ticks() {
    assert_eq!(Calib::shipped().deflect_trigger_tick, DeflectTriggerTick::WholeTicks);
}

/// A red Musketeer's point, 5,000 up the lane from AT (inside her range), held there before every tick.
const MUSK: (i32, i32) = (AT.0, AT.1 + 5000);

/// The Monk's deflect area's radius (card.rs `AbilityEffect::Deflect::radius`), in subtiles.
fn deflect_radius(s: &BattleState) -> i64 {
    match card_stat(s, "Monk").ability.as_ref().map(|a| &a.effect) {
        Some(AbilityEffect::Deflect { radius, .. }) => i64::from(*radius),
        other => panic!("the Monk's button is no Deflect: {other:?}"),
    }
}

/// The shots at `monk` that no deflect sent back, in flight after the tick: (any, any within `r` subtiles of AT).
fn shots_at(s: &BattleState, monk: EntityId, r: i64) -> (bool, bool) {
    let at = n(AT);
    let mut flies = false;
    let mut inside = false;
    for p in s.projectiles().iter().filter(|p| p.target == monk && !p.deflected) {
        flies = true;
        inside |= p.pos.dist2(at) <= r * r;
    }
    (flies, inside)
}

/// Does a shot a deflect sent back fly after the tick?
fn sent_back(s: &BattleState) -> bool {
    s.projectiles().iter().any(|p| p.deflected)
}

/// THE CROSSING, read off an unpressed twin of `s` (the Monk put back on AT and the Musketeer on MUSK before every tick):
/// the first tick X, at least LEAD + 14 ticks on, whose step brings one of her shots at him within his deflect area's
/// radius of AT (none was on X less 1), and F, the hp that shot takes off him where it lands a few ticks later. In a
/// pressed run he stands on AT from his cast (and is put back there before every tick), and the twin's Monk walks along
/// the shot's line, so the shot flies as it does here.
fn crossing(s: &BattleState, monk: EntityId, musk: EntityId) -> (u32, i32) {
    let mut twin = s.clone();
    let r = deflect_radius(&twin);
    let start = twin.tick_count();
    let mut was_inside = false;
    let mut x: Option<u32> = None;
    for _ in 0..200 {
        assert!(twin.debug_set_pos(monk, n(AT)) && twin.debug_set_pos(musk, n(MUSK)), "the scene drifted: the Monk or the Musketeer is gone");
        let now = twin.tick_count();
        let hp = twin.entity(monk).expect("the Monk").hp;
        twin.tick();
        let lost = hp - twin.entity(monk).expect("the Monk").hp;
        let (flies, inside) = shots_at(&twin, monk, r);
        match x {
            Some(at) => {
                if lost > 0 {
                    return (at, lost);
                }
                assert!(now < at + 10, "the scene drifted: the shot inside his area on {at} never landed");
            }
            None if inside && !was_inside && now >= start + LEAD + 14 => {
                assert_eq!(lost, 0, "the scene drifted: the unpressed Monk was hit on the crossing tick {now}");
                assert!(flies, "the scene drifted: the shot crossing his area's edge on {now} landed there");
                x = Some(now);
            }
            None => {}
        }
        was_inside = inside;
    }
    panic!("the scene drifted: no Musketeer shot crossed his deflect area's edge in 200 ticks");
}

/// One tick of a pressed run: P + k's k, the hp the Monk lost on it, and after it whether a shot at him that no deflect
/// sent back flies, whether one is inside his deflect area, and whether a shot a deflect sent back flies.
#[derive(Debug)]
struct ShotRow {
    k: u32,
    lost: i32,
    flies: bool,
    inside: bool,
    back: bool,
}

/// The row of P + `k` in `rows`.
fn row(rows: &[ShotRow], k: u32) -> &ShotRow {
    rows.iter().find(|r| r.k == k).unwrap_or_else(|| panic!("no row for P + {k}: {rows:?}"))
}

/// THE PRESSED RUN on `table` under `arm`: the crossing X off the unpressed twin (`crossing`), the press made so that the
/// cast's first frame is P = X less `lead` (`zap_on`'s count), the Monk put back on AT and the Musketeer on MUSK before
/// every tick: the rows of P + `lead` less 1 through P + `lead` + 1, and F.
fn pressed_crossing(table: CardTable, arm: DeflectTriggerTick, lead: u32) -> (Vec<ShotRow>, i32) {
    let (mut s, monk) = monk_battle(table, arm);
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", n(MUSK), None).expect("the Musketeer");
    let (x, full) = crossing(&s, monk, musk);
    let p = x - lead;
    let r = deflect_radius(&s);
    let mut rows = Vec::new();
    while s.tick_count() <= x + 1 {
        let now = s.tick_count();
        assert!(s.debug_set_pos(monk, n(AT)) && s.debug_set_pos(musk, n(MUSK)), "the scene drifted: the Monk or the Musketeer is gone");
        if now == p {
            s.scenario_set_elixir_milli(Team::Blue, 10_000);
            s.press_ability_button(Team::Blue, 0).expect("the press");
        }
        let hp = s.entity(monk).expect("the Monk").hp;
        s.tick();
        if now >= x - 1 {
            let lost = hp - s.entity(monk).expect("the Monk").hp;
            let (flies, inside) = shots_at(&s, monk, r);
            rows.push(ShotRow { k: now - p, lost, flies, inside, back: sent_back(&s) });
        }
    }
    (rows, full)
}

/// status.DEFLECT_TRIGGER_TICK = client16635_last_partial_tick, re-specced by parity r100 (client 16.635, the oracle's
/// il-mg-16635 set: the shots on P + 14 caught and returned on P + 14, the one landing on P + 13 not returned; his
/// neighbours met him as an obstacle from P + 14): on 16.635's table his buff comes on P + 13 and his deflect window opens
/// on P + 14. A Musketeer shot whose step brings it inside his area on P + 13 is not caught there (he loses nothing, it
/// flies on, nothing comes back); on P + 14 it is caught, lands at 35 % and is sent back. Under whole_ticks (the vacuity
/// check for the buff) it is caught on P + 14 in full. Plant: deflect_window_at_trigger.
#[test]
fn under_client16635_last_partial_tick_the_deflect_window_opens_a_tick_after_its_buff() {
    let table = CardTable::Client160635007Nov03;
    let (old, full) = pressed_crossing(table, DeflectTriggerTick::WholeTicks, 13);
    assert!(full > 0, "the scene drifted: the shot took nothing off the unpressed Monk");
    assert_eq!(row(&old, 13).lost, 0, "whole_ticks: nothing lands on P + 13: {old:?}");
    assert_eq!(row(&old, 14).lost, full, "whole_ticks: the shot is caught on P + 14 (its trigger tick) in full: {old:?}");
    let (new, _) = pressed_crossing(table, DeflectTriggerTick::Client16635LastPartialTick, 13);
    let (on_13, on_14) = (row(&new, 13), row(&new, 14));
    assert_eq!(on_13.lost, 0, "client16635_last_partial_tick: the shot inside his area on P + 13 is not caught there: {new:?}");
    assert!(on_13.flies && !on_13.back, "client16635_last_partial_tick: it flies on after P + 13, and nothing comes back: {new:?}");
    assert!(on_14.lost > 0 && on_14.lost < full, "client16635_last_partial_tick: it is caught on P + 14 at 35 % ({} of {full})", on_14.lost);
    assert!(on_14.back, "client16635_last_partial_tick: it is sent back on P + 14: {new:?}");
    for (arm, rows) in [("whole_ticks", &old), ("client16635_last_partial_tick", &new)] {
        assert!(!row(rows, 12).inside && row(rows, 13).inside, "{arm}: the scene drifted: the pressed run's shot does not cross his area's edge on P + 13: {rows:?}");
    }
}

/// On client 15.535.29's table (TriggerDelay 933) the deflect window opens with the buff under both arms: a shot whose
/// step brings it inside his area on P + 18 is caught there in full (the buff lands after the tick's hits) and sent back.
#[test]
fn a_933_ms_deflect_window_opens_on_p_plus_18_under_both_arms() {
    let table = CardTable::Client15535;
    for arm in [DeflectTriggerTick::WholeTicks, DeflectTriggerTick::Client16635LastPartialTick] {
        let (rows, full) = pressed_crossing(table, arm, 18);
        assert!(full > 0, "{arm:?}: the scene drifted: the shot took nothing off the unpressed Monk");
        assert!(!row(&rows, 17).inside, "{arm:?}: the scene drifted: a shot inside his area on P + 17: {rows:?}");
        let on_18 = row(&rows, 18);
        assert_eq!(on_18.lost, full, "{arm:?}: the shot inside his area on P + 18 is caught there in full: {rows:?}");
        assert!(on_18.back, "{arm:?}: and sent back on P + 18: {rows:?}");
    }
}

/// tests/deflect_stay_time.rs's scene on 16.635's table under status.DEFLECT_TRIGGER_TICK = `arm` (movement.DEFLECT_CONTACT
/// = client15535_pushes, movement.DEFLECT_STAY_TIME = client15535_tick_short): blue's Monk held on AT until a red Giant
/// walking down his lane comes within 2,200, pressed; from P + 85 the Giant is put 1,000 above him before every tick. The
/// tick P + k his own point first changes after P + 84 (P the cast's first frame, `zap_on`'s count).
fn first_pushed(arm: DeflectTriggerTick) -> u32 {
    let table = CardTable::Client160635007Nov03;
    let mut cfg = BattleConfig::with_cards(CardDb::load_table(table).expect("the table loads"));
    cfg.calib.card_table = table;
    cfg.calib.deflect_trigger_tick = arm;
    cfg.calib.deflect_buff_landing = DeflectBuffLanding::ClientAfterHitsTickShort;
    cfg.calib.deflect_contact = DeflectContact::Client15535Pushes;
    cfg.calib.deflect_stay_time = DeflectStayTime::Client15535TickShort;
    let deck: Vec<String> = DECK.iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n(AT), None).expect("the Monk");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n((AT.0, AT.1 + 5000)), None).expect("the Giant");
    let gap = |s: &BattleState| {
        let (a, b) = (s.entity(monk).expect("the Monk").pos, s.entity(giant).expect("the Giant").pos);
        isqrt(a.dist2(b)) / i64::from(K)
    };
    let mut waited = 0;
    while gap(&s) > 2200 {
        assert!(s.debug_set_pos(monk, n(AT)));
        s.tick();
        waited += 1;
        assert!(waited < 400, "{arm:?}: the scene drifted: the Giant never came near ({})", gap(&s));
    }
    assert!(s.debug_set_pos(monk, n(AT)));
    let p = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press");
    while s.tick_count() < p + 85 {
        s.tick();
    }
    let stood = s.entity(monk).expect("the Monk").pos;
    while s.tick_count() < p + 110 {
        let m = s.entity(monk).expect("the Monk").pos;
        assert!(s.debug_set_pos(giant, Vec2::new(m.x, m.y + 1000 * K)));
        let now = s.tick_count();
        s.tick();
        if s.entity(monk).expect("the Monk").pos != stood {
            return now - p;
        }
    }
    panic!("{arm:?}: the scene drifted: the Monk never moved by P + 110");
}

/// THE STAY'S END (movement.DEFLECT_STAY_TIME) is unchanged under client16635_last_partial_tick: the stay lands with the
/// buff on P + 13 and runs a tick longer, so on 16.635's table his first move after the press comes on P + 93 under both
/// arms (the client: e6031b38 key 66 first pushed on P + 93).
#[test]
fn under_both_arms_the_stay_ends_after_p_plus_92() {
    let old = first_pushed(DeflectTriggerTick::WholeTicks);
    assert_eq!(old, 93, "whole_ticks: his first move after the press");
    assert_eq!(first_pushed(DeflectTriggerTick::Client16635LastPartialTick), old, "client16635_last_partial_tick: the stay keeps its end");
}
