//! THE EVO ICE SPIRITS (tools/extract_cards.py `impact_area_block`; card.rs `EvoDef::impact_area`, EVO_IMPACT_AREA;
//! spell.rs `AreaRelease::target`; state.rs `phase_projectile`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-IceSpirits-evo-s0): the shot landed on t755, 110 off a Knight and a
//! Musketeer beside it (43 at level 1) and the Freeze; its area hit both again for 110 on t815, 3000 ms on.
//! Read off the table, not measured: the area riding on the shot's target (FollowTarget), staying where a dead one was.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_ice_spirits`):
//!   - impact_area_stays -> `its_area_rides_its_target_and_hits_once_3000_ms_on` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

#[test]
fn its_area_rides_its_target_and_hits_once_3000_ms_on() {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["IceSpirits".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    // Blue's princess towers down (its king wakes; its reach stops short of y 12000): no crown tower reaches the scene.
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    // The spirit 2500 short of a red Knight, a red Musketeer 2040 from the Knight (in the shot's splash), both held.
    let (kn_at, mu_at) = (n(9000, 13000), n(7000, 13400));
    s.spawn_unit(Team::Blue, "IceSpirits_EV1", n(9000, 10500), None).expect("the spirit");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", mu_at, None).expect("a red Musketeer");
    let (ktop, mtop) = (s.entity(knight).expect("the Knight").max_hp, s.entity(musk).expect("the Musketeer").max_hp);
    let (mut landed, mut kh, mut mh) = (None, Vec::new(), Vec::new());
    for k in 0..400 {
        // Held until the shot lands; from the next tick the Knight stands 4500 off the landing point.
        let far = landed.is_some();
        assert!(s.debug_set_pos(knight, if far { n(13500, 13000) } else { kn_at }));
        assert!(s.debug_set_pos(musk, mu_at));
        s.tick();
        kh.push(s.entity(knight).expect("the Knight").hp);
        mh.push(s.entity(musk).expect("the Musketeer").hp);
        if landed.is_none() && kh[k] < ktop {
            landed = Some(k);
        }
        if landed.is_some_and(|h| k >= h + 70) {
            break;
        }
    }
    let h = landed.expect("the shot landed on the Knight");
    assert_eq!((kh[h], mh[h]), (ktop - 110, mtop - 110), "the shot's 110 on both");
    // The area rides on the Knight: one hit of 110 on the landing + 60, on the Knight 4500 off, none on the Musketeer.
    for k in h..=h + 70 {
        let want = if k < h + 60 { ktop - 110 } else { ktop - 220 };
        assert_eq!(kh[k], want, "the Knight on frame {k} (landed on {h}): {:?}", &kh[h..]);
        assert_eq!(mh[k], mtop - 110, "the Musketeer on frame {k} (landed on {h}): {:?}", &mh[h..]);
    }
}
