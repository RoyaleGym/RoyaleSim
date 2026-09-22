//! The bits of the public API the Python layer's `Engine` protocol demands and
//! nothing else exercises: `check_deploy` as a PURE query, deploy validation
//! order, and the accessors an observation builder reads.
//!
//! WHY check_deploy EXISTS: protocol.py's contract says "check_deploy(command)
//! -> int: PURE query: the DeployStatus `step` would give this command if it were
//! the only command this step. Must not mutate state." Before this, the only way
//! to ask was to call `deploy` and see it succeed -- which mutates.
mod common;

use royalesim::fixed::Vec2;
use royalesim::state::{BattleState, DeployError};
use royalesim::Team;
use common::*;

#[test]
fn check_deploy_never_mutates_and_agrees_with_deploy() {
    // Plant: check_deploy_ignores_buildings.
    let mut s = BattleState::new(4, scripted_config());
    // Put a building down so a footprint rejection is reachable.
    s.spawn_unit(Team::Blue, "Cannon", t(900, 1000), None).unwrap();
    for _ in 0..30 {
        s.tick();
    }
    let cannon = find_live(&s, Team::Blue, "Cannon")[0].pos;
    let hand: Vec<String> = s.hand(Team::Blue).iter().map(|x| x.to_string()).collect();
    assert!(hand.len() >= 4);
    let spots = [
        t(350, 1000),             // own half, fine
        t(900, 2000),             // enemy half, refused
        cannon,                   // on a building footprint
        t(900, 1600),             // the river
        Vec2::new(0, 0),          // the corner, out of the deploy zone
        t(900, 300),              // the king's no-deploy block
    ];
    let mut verdicts = Vec::new();
    for card in &hand {
        for p in spots {
            let before = s.state_hash();
            let q = s.check_deploy(Team::Blue, card, p);
            assert_eq!(before, s.state_hash(), "check_deploy mutated the state");
            // Compare against the real thing on a clone, so the probe cannot
            // change the state under test.
            let mut probe = s.clone();
            let got = probe.deploy(Team::Blue, card, p);
            assert_eq!(q.is_ok(), got.is_ok(), "check_deploy {q:?} but deploy {got:?} for {card} at {p:?}");
            if let (Err(a), Err(b)) = (&q, &got) {
                assert_eq!(a, b, "different reasons for {card} at {p:?}");
            }
            verdicts.push(q);
        }
    }
    // Vacuity: the sample must contain both verdicts and a footprint rejection.
    assert!(verdicts.iter().any(|v| v.is_ok()), "no placement was accepted");
    assert!(verdicts.iter().filter(|v| v.is_err()).count() >= spots.len(), "almost nothing was rejected");
    let refused_on_building = s.check_deploy(Team::Blue, &hand[0], cannon);
    assert_eq!(refused_on_building, Err(DeployError::Occupied), "a deploy on top of a Cannon must be refused, as Occupied");
}

#[test]
fn check_deploy_reports_unknown_and_unsupported_cards_apart() {
    let s = BattleState::new(1, scripted_config());
    match s.check_deploy(Team::Blue, "NotACard", t(900, 1000)) {
        Err(DeployError::UnknownCard(n)) => assert_eq!(n, "NotACard"),
        other => panic!("expected UnknownCard, got {other:?}"),
    }
    // A card that is in cards.json but whose mechanic the engine does not simulate
    // must read as UNSUPPORTED with a reason, never as "unknown".
    // It must be a card the loader genuinely refuses: Fireball no longer is, now
    // that spells load, and Rage no longer carries an area effect in the 15.535
    // data at all (its buff is an action graph), so it is refused for "no mechanic
    // in the data". Poison is a pulsing area effect (HitSpeed set) in both vintages,
    // refused by card.rs `convert_spell`, with its reason read back from the loader.
    match s.check_deploy(Team::Blue, "Poison", t(900, 1000)) {
        Err(DeployError::UnsupportedCard(n, why)) => {
            assert_eq!(n, "Poison");
            assert!(why.contains("pulsing"), "Poison refused for an unexpected reason: {why}");
        }
        other => panic!("expected UnsupportedCard, got {other:?}"),
    }
    match s.check_deploy(Team::Blue, "Rage", t(900, 1000)) {
        Err(DeployError::UnsupportedCard(n, _)) => assert_eq!(n, "Rage"),
        other => panic!("expected UnsupportedCard for Rage, got {other:?}"),
    }
    // ...and the expired half becomes a REGRESSION gate: every thin-slice spell is
    // now simulable, so asking about one that is not in hand says NotInHand -- never
    // Unsupported. Plant: spells_rejected (card.rs, the pre-spell loader).
    for spell in ["Fireball", "Arrows", "Zap", "Log", "GoblinBarrel"] {
        assert!(s.cards().index(spell).is_some(), "{spell} is not simulable: {:?}", s.cards().rejected.iter().find(|(n, _)| n == spell));
        assert_eq!(s.check_deploy(Team::Blue, spell, t(900, 1000)), Err(DeployError::NotInHand), "{spell}");
    }
}

#[test]
fn not_in_hand_and_not_enough_elixir_are_distinguishable() {
    let mut s = BattleState::new(1, scripted_config());
    let hand: Vec<String> = s.hand(Team::Blue).iter().map(|x| x.to_string()).collect();
    let next = s.next_card(Team::Blue).expect("deck has a 5th card").to_string();
    assert!(!hand.contains(&next));
    assert_eq!(s.check_deploy(Team::Blue, &next, t(350, 1000)), Err(DeployError::NotInHand));
    // Spend down to nothing: the most expensive card in hand must eventually be
    // refused for elixir, with the numbers attached.
    let mut refusal = None;
    for _ in 0..200 {
        let hand: Vec<String> = s.hand(Team::Blue).iter().map(|x| x.to_string()).collect();
        let pick = hand
            .iter()
            .max_by_key(|c| card_stat(&s, c).elixir)
            .expect("hand is not empty")
            .clone();
        match s.check_deploy(Team::Blue, &pick, t(350, 1000)) {
            Err(e @ DeployError::NotEnoughElixir { .. }) => {
                refusal = Some(e);
                break;
            }
            Ok(()) => s.deploy(Team::Blue, &pick, t(350, 1000)).unwrap(),
            Err(e) => panic!("unexpected {e:?}"),
        }
        s.tick();
    }
    match refusal {
        Some(DeployError::NotEnoughElixir { have, need }) => assert!(have < need, "have {have}, need {need}"),
        other => panic!("never ran out of elixir: {other:?}"),
    }
}

#[test]
fn accessors_agree_with_each_other() {
    // An observation builder reads these; they must not contradict.
    let mut s = BattleState::new(9, scripted_config());
    let mut script = Script::new(40);
    for _ in 0..1500 {
        script.step(&mut s);
        s.tick();
        assert_eq!(s.live_count(), s.entities().count());
        for team in [Team::Blue, Team::Red] {
            let hp = s.tower_hp(team);
            for (k, id) in s.tower_ids(team).iter().enumerate() {
                let live = id.and_then(|i| s.entity(i));
                match live {
                    Some(v) => assert_eq!(hp[k], v.hp.max(0), "tower {k} hp disagrees"),
                    None => assert_eq!(hp[k], 0, "tower {k} is dead but hp is {}", hp[k]),
                }
            }
            let (raw, unit) = s.elixir_raw(team);
            assert_eq!(s.elixir(team), (raw / unit) as i32);
            assert!(s.elixir(team) <= s.config().calib.max_mana);
            assert_eq!(s.hand(team).len(), 4);
        }
        assert_eq!(s.time_ms(), s.tick_count() as i64 * s.config().calib.tick_ms as i64);
    }
}
