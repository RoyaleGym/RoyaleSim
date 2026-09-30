//! EVOLVED FORMS: Evo Skeletons, Evo Cannon and Evo Musketeer.
//!
//! A deck marks a card evolved (`BattleConfig::forms`, 1 on its entry). Counting only that card's own plays from the
//! battle's start, plays 1 and 2 put the base card down and play 3 its evolved form, and the count starts again
//! (state.rs `EVO_BASIC_PLAYS`, measured on client 15.535.29). A form loads as a card of its own (card.rs `EvoDef`),
//! after every card of the table, so a battle without forms runs and hashes as before (tests/hash_continuity.rs).
//!
//! One scene per form, each at level 11:
//!   - Evo Cannon: its nine bombs land where and when client 15.535.29 put them, 281 each, never twice on one unit;
//!   - Evo Skeletons: every hit of the play's group makes one more evolved skeleton, up to 8 alive;
//!   - Evo Musketeer: her first three shots at a target far ahead in her lane are snipes, then plain shots.
//!
//! And the invariant that keeps every other battle: the forms take slots after every existing card and buff.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test evolution`):
//!   forms_load_first        the forms load before the spawned units: forms_take_slots_after_every_existing_card
//!                           goes red (and so does tests/hash_continuity.rs).
//!   evo_hashed_when_absent  the evolved units' state is hashed in every battle: tests/hash_continuity.rs goes red.
//!   barrage_lands_after_the_move  an Evo Cannon bomb lands after the move pass and only its survivors step:
//!                           a_barrage_bomb_lands_before_the_move_pass goes red.
//!   evo_copy_stands         an Evo Skeletons copy is made ahead of its hitter's point at its hit and takes no first
//!                           update: evo_skeletons_copy_on_every_group_hit and
//!                           an_evo_copy_is_made_exactly_ahead_of_its_hitter_after_the_move go red.
//!   evo_copy_ahead_only     spawner.EVO_COPY_POINT = client15535_ahead_outward_behind still makes every copy ahead:
//!                           an_evo_copy_on_the_river_bank_goes_outward_then_behind_under_client15535_ahead_outward_behind
//!                           goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{barrage_spells, BattleConfig, BattleState, EvoCopyPoint, SpawnedFirstStep, TapSnap, EVO_BASIC_PLAYS};
use royalesim::Team;

/// A native point, in subtiles.
fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Level 11 for both sides, cards and towers.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// A battle at level 11 with the clock past the opening lockout.
fn battle(cfg: BattleConfig) -> BattleState {
    let mut s = BattleState::new(7, level11(cfg));
    past_deploy_lockout(&mut s);
    s
}

fn idx(s: &BattleState, name: &str) -> u16 {
    s.cards().index(name).unwrap_or_else(|| panic!("{name} does not load"))
}

#[test]
fn a_deck_card_marked_evolved_plays_its_form_every_third_play() {
    let mut cfg = config();
    cfg.decks = [vec!["Cannon".into(), "Knight".into(), "Mirror".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0, 0], Vec::new()];
    let mut s = battle(cfg);
    let (cannon, form) = (idx(&s, "Cannon"), idx(&s, "Cannon_EV1"));
    // Each play at 10 elixir, then one tick: the card that went down is the newest Blue unit. Each building on a spot
    // of its own, the Knight away from them all.
    let mut spot = 0;
    let mut play = |s: &mut BattleState, card: &str| -> String {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        let before: Vec<_> = s.entities().map(|e| e.id).collect();
        let at = if card == "Knight" {
            n(2000, 11500)
        } else {
            spot += 1;
            n(3000 + 3000 * (spot % 4 + 1), 10000 + 3000 * (spot / 4))
        };
        s.deploy(Team::Blue, card, at).unwrap_or_else(|e| panic!("{card}: {e:?}"));
        s.tick();
        let new: Vec<String> = s.entities().filter(|e| e.team == Team::Blue && !before.contains(&e.id)).map(|e| e.card.to_string()).collect();
        assert_eq!(new.len(), 1, "{card} put down {new:?}");
        new[0].clone()
    };
    let counter = |s: &BattleState| s.evo_counters(Team::Blue)[0];
    let cost = |s: &BattleState| s.hand_costs(Team::Blue)[s.hand(Team::Blue).iter().position(|c| *c == "Cannon").unwrap()];
    assert_eq!(s.evo_counters(Team::Blue).len(), 1);
    assert_eq!((counter(&s).card, counter(&s).form), (cannon, form));
    assert_eq!(EVO_BASIC_PLAYS, 2);
    // Play 1 (from the opening hand) and play 2 are basic; the Knight between them does not count.
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert_eq!(play(&mut s, "Knight"), "Knight");
    assert_eq!(counter(&s).plays, 1);
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert!(counter(&s).next_evolved(), "two basic plays make the next one evolved");
    assert_eq!(cost(&s), 3, "the evolved play costs the base card's elixir");
    // Play 3 is the form, and the count starts again.
    assert_eq!(play(&mut s, "Cannon"), "Cannon_EV1");
    assert_eq!(counter(&s).plays, 0);
    // A Mirror after the evolved play copies the base card, and does not count as a Cannon play.
    assert_eq!(s.mirror_target(Team::Blue), Some(cannon));
    assert_eq!(play(&mut s, "Mirror"), "Cannon");
    assert_eq!(counter(&s).plays, 0);
    assert!(s.hand(Team::Blue).contains(&"Mirror"));
    // Plays 4 and 5 basic, play 6 evolved.
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert_eq!(play(&mut s, "Cannon"), "Cannon");
    assert_eq!(play(&mut s, "Cannon"), "Cannon_EV1");
    // The evolved unit reports status bit 3; the plain ones do not.
    for e in s.entities().filter(|e| e.team == Team::Blue && e.card.starts_with("Cannon")) {
        assert_eq!(e.status_flags & 8 != 0, e.card == "Cannon_EV1", "{} status {}", e.card, e.status_flags);
    }
    // The counter survives a save and a load.
    let back = BattleState::load(&s.save()).expect("the battle loads");
    assert_eq!(back.evo_counters(Team::Blue), s.evo_counters(Team::Blue));
    assert_eq!(back.state_hash(), s.state_hash());
}

#[test]
fn evo_cannon_drops_its_barrage() {
    // The Golems and the Knights stand at measured distances from the bombs, exact points: placement.TAP_SNAP's old
    // arm, none (the shipped tile-centre snap would move each to its tile's centre).
    let mut cfg = config();
    cfg.calib.placement_tap_snap = TapSnap::None;
    let mut s = battle(cfg);
    let form = idx(&s, "Cannon_EV1");
    // Five Golems (radius 750) about the far row, deploying (3000 ms) through the whole barrage, far from every other
    // attack:
    //   (7000, 18000) 2000 from the 9000 bomb (lands I + 26) and from the 5000 bomb (I + 28): hit once, on I + 26;
    //   (13000, 18000) under the 13000 bomb (I + 28);
    //   (17000, 18000) under the 17000 bomb (I + 30);
    //   (13000, 20400) 2400 behind the 13000 bomb: hit, on I + 28;
    //   (9000, 20600) 2600 behind the 9000 bomb: missed, the reach being 2500 centre to centre, though its edge is 1850
    //   from the bomb.
    let golems = [(n(7000, 18000), Some(26)), (n(13000, 18000), Some(28)), (n(17000, 18000), Some(30)), (n(13000, 20400), Some(28)), (n(9000, 20600), None)];
    let play = s.tick_count();
    s.spawn_unit(Team::Blue, "Cannon_EV1", n(9000, 9500), None).unwrap();
    for (at, _) in golems {
        s.spawn_unit(Team::Red, "Golem", at, None).unwrap();
    }
    s.tick();
    // The nine bombs: x absolute, y the cannon's plus 1500 and 8500 toward the enemy.
    let mut bombs: Vec<(i32, i32)> = s.spells().iter().filter(|sp| sp.card == form).map(|sp| match sp.motion {
        royalesim::spell::SpellMotion::Flight { pos, .. } => (pos.x / K, pos.y / K),
        ref m => panic!("a bomb in motion {m:?}"),
    }).collect();
    bombs.sort_unstable();
    let mut want: Vec<(i32, i32)> = [1500, 6500, 11500, 16500].iter().map(|x| (*x, 11000)).chain([1000, 5000, 9000, 13000, 17000].iter().map(|x| (*x, 18000))).collect();
    want.sort_unstable();
    assert_eq!(bombs, want);
    // Side 1's barrage: the same x, the y toward side 0.
    let def = s.cards().get(form).evo.as_ref().unwrap().barrage.as_ref().unwrap();
    let red: Vec<i32> = barrage_spells(s.cards(), &s.config().calib, def, Team::Red, form, 11, n(9000, 22500)).iter().map(|sp| match sp.motion {
        royalesim::spell::SpellMotion::Flight { pos, .. } => pos.y / K,
        _ => unreachable!(),
    }).collect();
    assert!(red.iter().all(|y| *y == 21000 || *y == 14000), "{red:?}");
    // Each Golem in reach loses 281 once, on the tick measured for its bomb; the one past 2500 loses nothing.
    let ids: Vec<_> = golems.iter().map(|(at, _)| s.entities().filter(|e| e.card == "Golem").min_by_key(|e| e.pos.dist2(*at)).expect("a Golem").id).collect();
    let full: Vec<i32> = ids.iter().map(|id| s.entity(*id).unwrap().hp).collect();
    let mut lost: Vec<Vec<(u32, i32)>> = vec![Vec::new(); golems.len()];
    let mut last = full.clone();
    // Two Knights (no IgnorePushback, unlike a Golem) played on I + 10, so they stand in deploy state (1000 ms)
    // through every bomb: one 2400 behind the 5000 bomb (lands I + 28), one 2600 behind the 1000 bomb (I + 30), each
    // clear of every Golem.
    let knights = [n(5000, 20400), n(1000, 20600)];
    let mut kid = Vec::new();
    let mut at: Vec<Vec2> = Vec::new();
    // Each Knight's moves, (tick after the play, native step).
    let mut moves: [Vec<(u32, (i32, i32))>; 2] = [Vec::new(), Vec::new()];
    while s.tick_count() < play + 40 {
        s.tick();
        if s.tick_count() == play + 10 {
            for p in knights {
                s.spawn_unit(Team::Red, "Knight", p, None).unwrap();
            }
        }
        if s.tick_count() == play + 11 {
            for p in knights {
                let e = s.entities().filter(|e| e.card == "Knight").min_by_key(|e| e.pos.dist2(p)).expect("the Knight");
                at.push(e.pos);
                kid.push(e.id);
            }
        }
        for (k, id) in ids.iter().enumerate() {
            let e = s.entity(*id).expect("the Golem lives");
            if k == 0 && s.tick_count() == play + 26 {
                assert!(e.deploying, "the scene needs the Golems standing");
            }
            if e.hp != last[k] {
                lost[k].push((s.tick_count() - play, last[k] - e.hp));
                last[k] = e.hp;
            }
        }
        for (k, id) in kid.iter().enumerate() {
            let e = s.entity(*id).expect("the Knight lives");
            if s.tick_count() == play + 28 {
                assert!(e.deploying, "the scene needs the Knights standing");
            }
            if e.pos != at[k] && s.tick_count() <= play + 36 {
                moves[k].push((s.tick_count() - play, ((e.pos.x - at[k].x) / K, (e.pos.y - at[k].y) / K)));
            }
            at[k] = e.pos;
        }
    }
    for (k, (_, tick)) in golems.iter().enumerate() {
        assert_eq!(lost[k], tick.map(|t| vec![(t, 281)]).unwrap_or_default(), "Golem {k}");
    }
    // THE PUSH: the Knight hit 2400 behind its bomb is pushed straight away from it by the knockback ladder of the
    // bomb's Pushback 1000, 200 - 25k a tick for 8 ticks, though it stands in deploy state (measured on client
    // 15.535.29: 199, 174 ... 24 on diagonals, 900 in all, for Barbarians, an Ice Golem and standing and walking
    // units alike, the first step on the hit's own tick; the Giant, whose row sets IgnorePushback, not at all). The
    // missed one stays put.
    let steps: Vec<(i32, i32)> = moves[0].iter().map(|(_, d)| *d).collect();
    assert_eq!(steps, (0..8).map(|k| (0, 200 - 25 * k)).collect::<Vec<_>>(), "the pushed Knight's moves {:?}", moves[0]);
    // The first step on the hit's own tick (I + 28), where every other push steps from the next tick (measured).
    assert_eq!(moves[0][0].0, 28, "the push's first step: {:?}", moves[0]);
    // The missed one is not pushed: it stands through its deploy and then walks toward side 0 (-y), never away.
    assert!(moves[1].iter().all(|(t, (_, dy))| *t > 30 && *dy <= 0), "the missed Knight was pushed: {:?}", moves[1]);
}

#[test]
fn a_barrage_bomb_lands_before_the_move_pass() {
    // sp-m3-radius-s0 in miniature (client 15.535.29, t986). Deploying skeletons inside the reach of the 9000 bomb (lands
    // I + 26) are killed by it. A Knight created after them, just clear of the farthest one and outside the reach, is
    // pushed straight out of that skeleton's stepped position on the landing tick: the skeleton took its first push
    // step in the tick's move pass, dying or not. The plant barrage_lands_after_the_move (the bomb lands after the move
    // pass, and only its survivors step) leaves the Knight where it stood.
    let mut s = battle(config());
    let play = s.tick_count();
    s.spawn_unit(Team::Blue, "Cannon_EV1", n(9000, 9500), None).unwrap();
    let bomb = n(9000, 18000);
    while s.tick_count() < play + 10 {
        s.tick();
    }
    s.spawn_unit(Team::Red, "Skeletons", n(9000, 19900), None).unwrap();
    // The formation settles in its first ticks; the Knight goes down after that, beside the settled skeletons.
    while s.tick_count() < play + 20 {
        s.tick();
    }
    let skeletons: Vec<_> = s.entities().filter(|e| e.team == Team::Red && e.card == "Skeletons").map(|e| (e.id, e.pos)).collect();
    assert_eq!(skeletons.len(), 3, "the scene drifted: {skeletons:?}");
    let (far, at) = *skeletons.iter().max_by_key(|(_, p)| p.dist2(bomb)).unwrap();
    let (dx, dy) = ((at.x - bomb.x) / K, (at.y - bomb.y) / K);
    // Integers only (tests/test_no_floats.py): the length in thousandths, and each offset rounded to the nearest unit.
    let len_milli = royalesim::fixed::isqrt((dx as i64 * dx as i64 + dy as i64 * dy as i64) * 1_000_000);
    assert!(len_milli > 1_500_000 && len_milli < 2_400_000, "the scene drifted: the far skeleton stands {} from its bomb", len_milli / 1000);
    let along = |d: i32| -> i32 {
        let num = d as i64 * 1010 * 1000 * 2;
        (if num >= 0 { (num + len_milli) / (2 * len_milli) } else { -((-num + len_milli) / (2 * len_milli)) }) as i32
    };
    // 1010 beyond it along the bomb's line: clear of it (radii 500 + 500), and more than 2500 from the bomb.
    let spot = Vec2::new(at.x + along(dx) * K, at.y + along(dy) * K);
    s.spawn_unit_resolved(Team::Red, "Knight", spot, None).unwrap();
    s.tick();
    let knight = s.entities().filter(|e| e.team == Team::Red && e.card == "Knight").map(|e| e.id).next().expect("the Knight");
    let mut before = None;
    while s.tick_count() < play + 26 {
        if s.tick_count() == play + 25 {
            before = Some(s.entity(knight).unwrap().pos);
            assert!(s.entity(far).is_some_and(|e| e.pos == at && e.deploying), "the scene drifted: the far skeleton moved or deployed");
        }
        s.tick();
    }
    let before = before.expect("the tick before the landing");
    let k = s.entity(knight).expect("the Knight lives");
    assert!(k.deploying && k.hp == k.max_hp, "the scene drifted: the Knight walked or the bomb reached it");
    assert!(s.entity(far).is_none(), "the scene drifted: the bomb did not kill the far skeleton");
    let (mx, my) = ((k.pos.x - before.x) / K, (k.pos.y - before.y) / K);
    assert!(mx * dx + my * dy > 0, "the Knight is pushed out of the dying skeleton's stepped position on the landing tick, away from the bomb: moved ({mx}, {my})");
    assert!(mx * mx + my * my <= 150 * 150 + 2, "a contact push, at most the 150 cap: moved ({mx}, {my})");
}

#[test]
fn evo_skeletons_copy_on_every_group_hit() {
    let mut s = battle(config());
    let form = idx(&s, "Skeletons_EV1");
    s.spawn_unit(Team::Red, "Giant", n(9000, 13500), None).unwrap();
    s.spawn_unit(Team::Blue, "Skeletons_EV1", n(9000, 12000), None).unwrap();
    let skeletons = |s: &BattleState| s.entities().filter(|e| e.team == Team::Blue && e.card_idx == form).count() as u32;
    let mut hits = 0u32;
    let mut first_copy: Option<(Vec2, Vec<Vec2>, Vec<Vec2>)> = None;
    let mut capped_hits = 0;
    for _ in 0..400 {
        let before: Vec<_> = s.entities().filter(|e| e.card_idx == form).map(|e| (e.id, e.pos)).collect();
        s.tick();
        let hitters: Vec<_> = s.entities().filter(|e| e.card_idx == form && e.attack_phase == AttackPhase::Cooldown).map(|e| e.id).collect();
        hits += hitters.len() as u32;
        let alive = skeletons(&s);
        assert_eq!(alive, (3 + hits).min(8), "after {hits} group hits");
        for e in s.entities().filter(|e| e.card_idx == form) {
            assert_eq!(e.status_flags & 8, 8, "an evolved skeleton reports status bit 3");
            if !before.iter().any(|(id, _)| *id == e.id) && s.tick_count() > 1 && hits >= 1 {
                assert_eq!(e.hp, e.max_hp, "a copy starts at full hp");
                assert!(e.target.is_some(), "a copy has a target on its first frame");
                if first_copy.is_none() {
                    let at_hit: Vec<Vec2> = before.iter().filter(|(id, _)| hitters.contains(id)).map(|(_, p)| *p).collect();
                    let moved: Vec<Vec2> = hitters.iter().filter_map(|id| s.entity(*id)).map(|h| h.pos).collect();
                    first_copy = Some((e.pos, at_hit, moved));
                }
            }
        }
        if alive == 8 {
            capped_hits += hitters.len();
        }
        if capped_hits >= 4 {
            break;
        }
    }
    assert!(capped_hits >= 4, "the group never held 8 through four more hits ({hits} hits)");
    // The first copy is made 1000 ahead (toward side 1) of the skeleton whose hit made it, where that skeleton stands
    // after the tick's move pass, and takes its first update on its creation tick: attacking the Giant, it is pushed by
    // the contact law, at most the 150 cap (sp-scene-b-s0 t796: 150 out of a Giant 524 away). So it stands within one
    // push of that point and off the point 1000 ahead of the hitter at its hit. The plant evo_copy_stands (the earlier
    // reading: at the hitter's point at its hit, and no step) turns this red.
    let (at, at_hit, moved) = first_copy.expect("a copy was made");
    let made = moved.iter().map(|p| Vec2::new(p.x, p.y + 1000 * K)).min_by_key(|m| m.dist2(at)).expect("a hitter");
    let (dx, dy) = ((at.x - made.x) / K, (at.y - made.y) / K);
    assert!(dx * dx + dy * dy <= 150 * 150 + 2, "copy at {at:?}, made at {made:?}: its first update moved it ({dx}, {dy})");
    assert!(at_hit.iter().all(|p| *p != Vec2::new(at.x, at.y - 1000 * K)), "the copy stands where the hitter was at its hit plus 1000: {at:?}, {at_hit:?}");
}

#[test]
fn an_evo_copy_is_made_exactly_ahead_of_its_hitter_after_the_move() {
    // The point the copy is MADE at, exactly: evo_skeletons_copy_on_every_group_hit reads it through the copy's first
    // update, so within one push. Under spawner.SPAWNED_FIRST_STEP = none no first update runs (a target, no step,
    // no push), and the first copy stands exactly 1000 ahead (toward side 1) of where a hitter stands after the
    // tick's move pass; the scene is the same Giant and group, and the hitter's point at its hit is elsewhere.
    let mut cfg = config();
    cfg.calib.spawned_first_step = SpawnedFirstStep::None;
    let mut s = battle(cfg);
    let form = idx(&s, "Skeletons_EV1");
    s.spawn_unit(Team::Red, "Giant", n(9000, 13500), None).unwrap();
    s.spawn_unit(Team::Blue, "Skeletons_EV1", n(9000, 12000), None).unwrap();
    let (mut copies, mut moved) = (0, 0);
    for _ in 0..400 {
        let before: Vec<_> = s.entities().filter(|e| e.card_idx == form).map(|e| (e.id, e.pos)).collect();
        s.tick();
        if before.is_empty() {
            continue;
        }
        let hitters: Vec<_> = s.entities().filter(|e| e.card_idx == form && e.attack_phase == AttackPhase::Cooldown).map(|e| e.id).collect();
        let made: Vec<Vec2> = hitters.iter().filter_map(|id| s.entity(*id)).map(|h| Vec2::new(h.pos.x, h.pos.y + 1000 * K)).collect();
        let at_hit: Vec<Vec2> = before.iter().filter(|(id, _)| hitters.contains(id)).map(|(_, p)| Vec2::new(p.x, p.y + 1000 * K)).collect();
        for copy in s.entities().filter(|e| e.card_idx == form && !before.iter().any(|(id, _)| *id == e.id)) {
            assert!(made.contains(&copy.pos), "a copy stands at {:?}, not 1000 ahead of a hitter after the move: {made:?}", copy.pos);
            copies += 1;
            moved += usize::from(!at_hit.contains(&copy.pos));
        }
    }
    // Not vacuous: some copy's hitter was moved on its hit's tick, so the point at its hit would miss.
    assert!(copies >= 5 && moved >= 1, "{copies} copies, {moved} of them where the hitter moved on its hit's tick");
}

/// Evo Skeletons swarming a Red Cannon held on Blue's river bank between the bridges, under spawner.EVO_COPY_POINT
/// `arm` and no first update (so a copy stands exactly where it was made): (copies, copies off every hitter's ahead
/// point, copies standing where a ground unit cannot).
fn copies_on_the_bank(arm: EvoCopyPoint) -> (usize, usize, usize) {
    let mut cfg = config();
    cfg.calib.spawned_first_step = SpawnedFirstStep::None;
    cfg.calib.evo_copy_point = arm;
    let mut s = battle(cfg);
    let form = idx(&s, "Skeletons_EV1");
    s.scenario_spawn_now(Team::Red, "Cannon", n(6000, 14400), None).unwrap();
    s.spawn_unit(Team::Blue, "Skeletons_EV1", n(6000, 12600), None).unwrap();
    let (mut copies, mut off_ahead, mut wet) = (0, 0, 0);
    for _ in 0..400 {
        let before: Vec<_> = s.entities().filter(|e| e.card_idx == form).map(|e| e.id).collect();
        s.tick();
        if before.is_empty() {
            continue;
        }
        let hitters: Vec<Vec2> = s.entities().filter(|e| before.contains(&e.id) && e.attack_phase == AttackPhase::Cooldown).map(|e| e.pos).collect();
        let made: Vec<Vec2> = hitters.iter().map(|p| s.evo_copy_point(*p, Team::Blue)).collect();
        let ahead: Vec<Vec2> = hitters.iter().map(|p| Vec2::new(p.x, p.y + 1000 * K)).collect();
        for copy in s.entities().filter(|e| e.card_idx == form && !before.contains(&e.id)) {
            assert!(made.contains(&copy.pos), "{arm:?}: a copy stands at {:?}, not at a hitter's copy point: {made:?}", copy.pos);
            copies += 1;
            off_ahead += usize::from(!ahead.contains(&copy.pos));
            wet += usize::from(!s.arena().is_passable_ground(copy.pos));
        }
    }
    (copies, off_ahead, wet)
}

#[test]
fn an_evo_copy_on_the_river_bank_goes_outward_then_behind_under_client15535_ahead_outward_behind() {
    // spawner.EVO_COPY_POINT (parity's item 61). A hitter beside the Cannon has its ahead point in the river. Under the
    // new arm each copy is made at the first of ahead, outward (-x here, left of the centre line) and behind where a
    // ground unit can stand; under ahead_only at the ahead point, river or not. Measured on client 15.535.29: 239 of
    // 239 copies, the real-match replay 208a8647's t1219 among them. Plant: evo_copy_ahead_only.
    let (copies, off_ahead, wet) = copies_on_the_bank(EvoCopyPoint::ClientAheadOutwardBehind);
    assert!(copies >= 3 && off_ahead >= 1, "vacuous: {copies} copies, {off_ahead} off the ahead point");
    assert_eq!(wet, 0, "no copy is made in the river");
    let (old, old_off, old_wet) = copies_on_the_bank(EvoCopyPoint::AheadOnly);
    assert_eq!(old_off, 0, "ahead_only makes every copy ahead");
    assert!(old_wet >= 1, "vacuous: ahead_only made no copy in the river ({old} copies)");
}

#[test]
fn evo_musketeer_spends_three_snipes() {
    let mut s = battle(config());
    let form = idx(&s, "Musketeer_EV1");
    let mult = s.config().calib.projectile_speed_to_subtiles_per_tick;
    // A Golem 12000 ahead of her in her lane, deploying 3000 ms: far past her Range of 6000.
    s.spawn_unit(Team::Blue, "Musketeer_EV1", n(3500, 9000), None).unwrap();
    s.spawn_unit(Team::Red, "Golem", n(3500, 21000), None).unwrap();
    s.tick();
    let me = s.entities().find(|e| e.card_idx == form).unwrap().id;
    let golem = s.entities().find(|e| e.card == "Golem").unwrap().id;
    let mut shots: Vec<(i32, i32, bool)> = Vec::new();
    for _ in 0..800 {
        s.tick();
        let m = s.entity(me).expect("she lives");
        if m.attack_phase == AttackPhase::Cooldown {
            let p = s.projectiles().iter().rev().find(|p| p.firer_card == Some(form)).expect("her shot");
            shots.push((p.speed / mult, p.damage, p.target == golem));
        }
        if shots.len() >= 4 {
            break;
        }
    }
    assert!(shots.len() >= 4, "{shots:?}");
    // Three snipes at the Golem: Speed 2650, 153 x 256 % = 391 at level 11.
    assert_eq!(&shots[..3], &[(2650, 391, true); 3]);
    // Then her plain shot: Speed 1000, 85 x 256 % = 217.
    assert_eq!((shots[3].0, shots[3].1), (1000, 217));
}

/// The table with the top-level lists `lists` removed.
fn without(lists: &[&str]) -> CardDb {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json");
    let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for list in lists {
        doc.as_object_mut().unwrap().remove(*list);
    }
    CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).unwrap()
}

#[test]
fn forms_take_slots_after_every_existing_card() {
    // The table with its evolved forms against the table without them, both without the hero forms: every card of
    // the parent build, and nothing else. The hero pass loads after the evolved forms (tests/hero_forms.rs holds it),
    // and may add its row's name to a buff the table already has, which is why it is left out here.
    let (db, db0) = (without(&["hero_forms"]), without(&["evolutions", "hero_forms"]));
    assert_eq!(db.rejected_evolutions, Vec::<(String, String)>::new());
    assert_eq!(db0.forms.len(), 0);
    let n0 = db0.cards.len();
    let names = |d: &CardDb, n: usize| d.cards[..n].iter().map(|c| format!("{c:?}")).collect::<Vec<_>>();
    assert_eq!(names(&db, n0), names(&db0, n0), "a card slot of the table without forms moved");
    assert_eq!(db.buffs[..db0.buffs.len()], db0.buffs[..], "a buff slot moved");
    assert_eq!(db.buff_names[..db0.buff_names.len()], db0.buff_names[..], "a buff's names moved");
    let forms: Vec<(String, String)> = db.forms.iter().map(|(b, _, f)| (db.get(*b).name.clone(), db.get(*f).name.clone())).collect();
    assert_eq!(
        forms,
        [
            ("Skeletons", "Skeletons_EV1"),
            ("Cannon", "Cannon_EV1"),
            ("Musketeer", "Musketeer_EV1"),
            ("AngryBarbarians", "AngryBarbarians_EV1"),
            ("Zap", "Zap_EV1"),
            ("BattleRam", "BattleRam_EV1"),
            ("InfernoDragon", "InfernoDragon_EV1"),
            ("BabyDragon", "BabyDragon_EV1"),
            ("Ghost", "Ghost_EV1"),
            ("SkeletonArmy", "SkeletonArmy_EV1"),
            ("Snowball", "Snowball_EV1"),
            ("SkeletonBalloon", "SkeletonBalloon_EV1"),
            ("Mortar", "Mortar_EV1")
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
    );
    assert!(db.forms.iter().all(|(_, _, f)| *f as usize >= n0));
    // Thirteen forms, the Elite Barbarians' second member (AngryBarbarian_EV1_2) right after its form, the Evo Battle Ram's
    // death spawn (Barbarian_EV1, with its rage) right after the ram, the Evo Royal Ghost's pair right after it, the Evo
    // Skeleton Army's General and Spectral right after it, the Evo Giant Snowball (a spell: no unit of its own), and the
    // Evo Skeleton Barrel with its two drops, and the Evo Mortar (its Goblin a loaded unit) last.
    assert_eq!(db.cards.len(), n0 + 21);
    assert_eq!(db.cards[n0 + 20].name, "Mortar_EV1");
    assert!(db.cards[n0 + 20].evo.as_ref().and_then(|e| e.shot_spawn).is_some_and(|s| db.get(s.unit.unit).unit_name == "Goblin" && s.deploy_ms == 500));
    let barrel: Vec<&str> = db.cards[n0 + 17..n0 + 20].iter().map(|c| c.name.as_str()).collect();
    assert_eq!(barrel, ["SkeletonBalloon_EV1", "SkeletonBalloonEvoDummyAeO_EXTRA", "SkeletonBalloonEvoDummyAeO_DEATH"]);
    assert!(db.cards[n0 + 18].summon_only && db.cards[n0 + 19].summon_only && db.cards[n0 + 18].death_bomb_fuse_ms() == Some(600));
    assert_eq!(db.cards[n0 + 17].death_spawn.map(|d| d.unit), Some((n0 + 19) as u16), "the death drop is its death spawn");
    assert_eq!(db.cards[n0 + 16].name, "Snowball_EV1");
    assert!(matches!(db.cards[n0 + 16].spell.as_ref().map(|d| &d.shape), Some(royalesim::card::SpellShape::CaptureRoll(_))));
    let army: Vec<&str> = db.cards[n0 + 13..n0 + 16].iter().map(|c| c.name.as_str()).collect();
    assert_eq!(army, ["SkeletonArmy_EV1", "SkeletonArmy_EV1_General", "SkeletonArmy_EV1_Spectral"]);
    assert!(db.cards[n0 + 14].summon_only && db.cards[n0 + 15].summon_only && db.cards[n0 + 15].no_damage && !db.cards[n0 + 14].no_damage);
    let pair: Vec<&str> = db.cards[n0 + 10..n0 + 13].iter().map(|c| c.name.as_str()).collect();
    assert_eq!(pair, ["Ghost_EV1", "Ghost_EV1_Summon_Left", "Ghost_EV1_Summon_Right"]);
    assert!(db.cards[n0 + 11].summon_only && db.cards[n0 + 12].summon_only && db.cards[n0 + 11].starts_visible);
    assert_eq!((db.cards[n0 + 8].name.as_str(), db.cards[n0 + 9].name.as_str()), ("InfernoDragon_EV1", "BabyDragon_EV1"));
    assert_eq!(db.cards[n0 + 4].name, "AngryBarbarian_EV1_2");
    assert!(db.cards[n0 + 4].summon_only);
    assert_eq!((db.cards[n0 + 6].name.as_str(), db.cards[n0 + 7].name.as_str()), ("BattleRam_EV1", "Barbarian_EV1"));
    assert!(db.cards[n0 + 7].summon_only && db.cards[n0 + 7].evo.as_ref().is_some_and(|e| e.hit_rage.is_some()));
    assert_eq!(db.cards[n0 + 6].death_spawn.map(|d| d.unit), Some((n0 + 7) as u16));
    // The whole table: the hero pass leaves every one of those slots where it was, the forms included, and loads only
    // after them.
    let full = cards();
    let n = db.cards.len();
    assert_eq!(names(&full, n), names(&db, n), "the hero pass moved a slot");
    assert_eq!(full.forms, db.forms);
    assert!((n..full.cards.len()).all(|k| full.is_hero_record(k as u16)), "a slot after the evolved forms that the hero pass did not load");
}
