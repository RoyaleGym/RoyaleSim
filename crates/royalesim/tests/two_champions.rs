//! A champion's card cycles as any card, and a second copy of the champion may be played while the first lives
//! (state.rs `deploy_slot`: the played card goes to the back of the queue; nothing refuses a champion on the board).
//!
//! THE EVIDENCE:
//!   * the cycle: in ranked replays' play order no card is played again within 4 plays of itself, the champions
//!     included (405,644 champion gaps, the shortest 4), and the plain 4-card cycle predicts every play of 94,111 sides
//!     holding a champion; right after a champion's own ability press, which shows it alive, the side's next play is the
//!     champion's card about as often as another card of the hand (20% within half a second of the press, 27% for a
//!     control card);
//!   * two at once, measured on client 16.402 (RoyaleLive recordings): 452 champion plays put a second copy down while
//!     the first stood (432 Little Prince, 20 Golden Knight), on both sides. Battle 20261007-004819: a Little Prince
//!     from t3014 to t3669, a second one deployed at t3364.
//! The tables' CHAMPION_RETURN_TO_CYCLE_POSITION = 2 governs neither: nothing above shows a card held out or put back.
//!
//! THE SCENE: Blue puts a Little Prince down in its back corner, far from every enemy, and plays four other cards; the
//! fourth draws his card back into the hand while he walks up his lane.
//!
//! WHAT IS PINNED: his card is back in the hand while he lives, a second Little Prince is taken then, and two stand on
//! the board. A rule that withholds the card or refuses the play turns it red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Blue's back corner, far from both enemy princess towers' reach.
fn corner() -> Vec2 {
    n(2500, 2500)
}

/// Plays `card` for Blue with a full bar, then ticks until the hand has refilled.
fn play(s: &mut BattleState, card: &str, at: Vec2) {
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, card, at).unwrap_or_else(|e| panic!("tick {}: {card} refused {e:?}", s.tick_count()));
    tick_until_refilled(s, Team::Blue);
}

#[test]
fn a_second_little_prince_is_taken_while_the_first_lives() {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = ["LittlePrince", "Skeletons", "Knight", "Zap", "Archers", "Musketeer", "Fireball", "Arrows"].iter().map(|s| s.to_string()).collect();
    let red: Vec<String> = ["Pekka", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck, red];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(15, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    play(&mut s, "LittlePrince", corner());
    assert!(!s.hand(Team::Blue).contains(&"LittlePrince"), "the scene: his card left the hand");
    assert_eq!(s.queue_cards(Team::Blue).last().copied(), s.cards().index("LittlePrince"), "his card went to the back of the queue");
    for c in ["Skeletons", "Knight", "Zap", "Archers"] {
        let at = if c == "Zap" { n(9000, 29000) } else { n(9000, 6000) };
        play(&mut s, c, at);
    }
    assert!(s.hand(Team::Blue).contains(&"LittlePrince"), "his card is back after four other plays: {:?}", s.hand(Team::Blue));
    assert_eq!(find_live(&s, Team::Blue, "LittlePrince").len(), 1, "the scene: he lives");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    assert_eq!(s.check_deploy(Team::Blue, "LittlePrince", corner()), Ok(()), "a second Little Prince while the first lives");
    s.deploy(Team::Blue, "LittlePrince", corner()).expect("taken");
    s.tick();
    s.tick();
    assert_eq!(find_live(&s, Team::Blue, "LittlePrince").len(), 2, "two Little Princes on the board");
}
