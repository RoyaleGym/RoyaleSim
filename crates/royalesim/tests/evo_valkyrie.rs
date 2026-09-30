//! THE EVO VALKYRIE (tools/extract_cards.py `attack_area_block`; card.rs `AttackAreaDef`; state.rs `attack_areas`, the
//! attract pre-pass and the source binding; spell.rs `SpellMotion::Attached`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Valkyrie-evo-s0, three hits):
//!   - her hit on H; every enemy within 5000 of her pulled toward her from H+2 through H+11 (her Valkyrie_MiniTornado_EV1:
//!     LifeDuration 500, AttractPercentage 300), while she walked;
//!   - each of them lost 42 on H+9 (the buff's 400 ms HitFrequency from its first application on H+1) and nothing more
//!     from that area.
//!
//! Read off the table, not measured: her own buff after each hit (Valkyrie_NotPushed_BUF, NO_PUSHED_BY_ENEMY, 500 ms).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_valkyrie`):
//!   - attack_area_never -> both tests red;
//!   - riding_area_never_pulls -> `each_hit_pulls_the_enemies_around_her_from_the_second_tick_for_ten` red;
//!   - riding_area_unbound -> `the_tornado_pulses_42_once_on_the_ninth_tick` red;
//!   - enemy_push_kept -> `no_enemy_pushes_her_after_her_hit` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Valkyrie".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// `card` (the Valkyrie or her form) held at (9000, 13500) hitting a red Knight held 900 above her; a red Giant held at
/// (6000, 14500), 3162 from her and out of every crown tower's reach. Both red units topped up every tick. Per frame:
/// what the Knight lost, and the Giant's move off its point and its loss.
fn scene(card: &str) -> (Vec<i32>, Vec<(Vec2, i32)>) {
    let mut s = battle();
    let (at, kn_at, gi_at) = (n(9000, 13500), n(9000, 14400), n(6000, 14500));
    s.spawn_unit(Team::Blue, card, at, None).expect("the Valkyrie");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", gi_at, None).expect("a red Giant");
    s.tick();
    let valk = find_live(&s, Team::Blue, card).first().expect("the Valkyrie").id;
    let (mut knight_lost, mut giant_seen) = (Vec::new(), Vec::new());
    for _ in 0..160 {
        assert!(s.debug_set_pos(valk, at));
        assert!(s.debug_set_pos(knight, kn_at));
        assert!(s.debug_set_pos(giant, gi_at));
        let (kt, gt) = (s.entity(knight).expect("the Knight").max_hp, s.entity(giant).expect("the Giant").max_hp);
        assert!(s.debug_set_hp(knight, kt));
        assert!(s.debug_set_hp(giant, gt));
        s.tick();
        knight_lost.push(kt - s.entity(knight).expect("the Knight").hp);
        let g = s.entity(giant).expect("the Giant");
        giant_seen.push((g.pos.sub(gi_at), gt - g.hp));
    }
    (knight_lost, giant_seen)
}

/// The first frame the Knight lost hp: her first hit.
fn first_hit(lost: &[i32]) -> usize {
    lost.iter().position(|l| *l > 0).expect("she hits the Knight")
}

#[test]
fn each_hit_pulls_the_enemies_around_her_from_the_second_tick_for_ten() {
    let (evo_kn, evo_gi) = scene("Valkyrie_EV1");
    let (base_kn, base_gi) = scene("Valkyrie");
    let h = first_hit(&evo_kn);
    assert_eq!(first_hit(&base_kn), h, "the form hits when the Valkyrie does");
    // The pull: the Giant's move beyond the plain run's (its walk from the same point), toward her.
    let pull: Vec<Vec2> = evo_gi.iter().zip(&base_gi).map(|(e, b)| e.0.sub(b.0)).collect();
    let toward = n(9000, 13500).sub(n(6000, 14500));
    for (k, p) in pull.iter().enumerate().take(h + 26).skip(h.saturating_sub(5)) {
        let pulled = *p != Vec2::default();
        assert_eq!(pulled, (h + 2..=h + 11).contains(&k), "frame H{:+}: pull {p:?}", k as i64 - h as i64);
        if pulled {
            let dot = i64::from(p.x) * i64::from(toward.x) + i64::from(p.y) * i64::from(toward.y);
            let cross = (i64::from(p.x) * i64::from(toward.y) - i64::from(p.y) * i64::from(toward.x)).abs();
            assert!(dot > 0 && cross * 10 <= dot, "frame H{:+}: toward her: {p:?}", k as i64 - h as i64);
        }
    }
}

#[test]
fn the_tornado_pulses_42_once_on_the_ninth_tick() {
    let (evo_kn, evo_gi) = scene("Valkyrie_EV1");
    let (_, base_gi) = scene("Valkyrie");
    let h = first_hit(&evo_kn);
    let extra: Vec<i32> = evo_gi.iter().zip(&base_gi).map(|(e, b)| e.1 - b.1).collect();
    assert_eq!(extra[h + 9], 42, "the buff's pulse on H+9: {:?}", &extra[h..h + 12]);
    for k in (h..h + 26).filter(|k| *k != h + 9) {
        assert_eq!(extra[k], 0, "no other pulse from that area: H{:+}", k as i64 - h as i64);
    }
}

#[test]
fn no_enemy_pushes_her_after_her_hit() {
    // A red Knight held 700 above her (their circles overlap by 300), topped up, pushes a plain Valkyrie down every tick;
    // the form, after her hit, is not pushed for her buff's 500 ms. She stands and hits it (in range).
    let run = |card: &str| -> (usize, Vec<Vec2>) {
        let mut s = battle();
        s.spawn_unit(Team::Blue, card, n(9000, 13000), None).expect("the Valkyrie");
        s.tick();
        let valk = find_live(&s, Team::Blue, card).first().expect("the Valkyrie").id;
        let knight = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 13700), None).expect("a red Knight");
        let (mut moved, mut lost) = (Vec::new(), Vec::new());
        for _ in 0..120 {
            let v = s.entity(valk).expect("the Valkyrie").pos;
            assert!(s.debug_set_pos(knight, Vec2::new(v.x, v.y + 700 * K)));
            let top = s.entity(knight).expect("the Knight").max_hp;
            assert!(s.debug_set_hp(knight, top));
            s.tick();
            moved.push(s.entity(valk).expect("the Valkyrie").pos.sub(v));
            lost.push(top - s.entity(knight).expect("the Knight").hp);
        }
        (first_hit(&lost), moved)
    };
    let (h, evo) = run("Valkyrie_EV1");
    let (hb, base) = run("Valkyrie");
    assert_eq!(hb, h, "the same hit tick");
    assert!(base[h + 1..=h + 9].iter().any(|m| *m != Vec2::default()), "the premise: the Knight pushes the plain Valkyrie: {:?}", &base[h + 1..=h + 9]);
    assert!(evo[h + 1..=h + 9].iter().all(|m| *m == Vec2::default()), "no push on her after her hit: {:?}", &evo[h + 1..=h + 9]);
}
