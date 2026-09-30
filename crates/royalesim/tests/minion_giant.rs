//! THE MINION GIANT: a live-client card the 15.535.29 tables lack, loaded from its datamined 16.402 rows
//! (data/client_additions/minion_giant.toml, tools/extract_cards.py `load_tables`). One behaviour, the datamined one:
//! a 4-elixir Rare flying unit (FlyingHeight 3000) that targets buildings only, ignores pushback, and fires a homing
//! ground-only shot of 74 (level 1) every 1500 ms from range 4000.
//!
//! Pinned: the card loads and is last in the card table (so no other card's place moved); its unit flies and ignores
//! pushback; it walks past an enemy troop to an enemy building and hits it from range, never the troop.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

#[test]
fn the_minion_giant_loads_last_and_plays_its_datamined_card() {
    let cfg = config();
    let db = cfg.cards.clone();
    let idx = db.index("MinionGiant").expect("the Minion Giant loads");
    let card = db.get(idx);
    assert_eq!(card.elixir, 4);
    assert_eq!(card.flying_height, 3000, "it flies at 3000");
    assert!(card.target_only_buildings, "it targets buildings only");
    assert!(card.ignore_pushback, "it ignores pushback");
    let mut s = BattleState::try_new(0, cfg).expect("a battle");
    past_deploy_lockout(&mut s);
    let mg = s.scenario_spawn_now(Team::Blue, "MinionGiant", n((3500, 20000)), None).expect("the Minion Giant");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n((3500, 21500)), None).expect("a Knight in its way");
    let tower_hp = |s: &BattleState| s.entities().filter(|e| e.team == Team::Red && e.card.contains("Tower")).map(|e| e.hp).sum::<i32>();
    let start = tower_hp(&s);
    let mut knight_targeted = false;
    for _ in 0..400 {
        s.tick();
        if s.entity(mg).is_some_and(|e| e.target == Some(knight)) {
            knight_targeted = true;
        }
    }
    assert!(!knight_targeted, "it never takes the Knight: buildings only");
    assert!(tower_hp(&s) < start, "it hits an enemy crown tower");
}
