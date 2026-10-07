//! THE EVO ICE SPIRITS (tools/extract_cards.py `impact_area_block`; card.rs `EvoDef::impact_area`, EVO_IMPACT_AREA;
//! spell.rs `AreaRelease::target`; state.rs `phase_projectile`), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-IceSpirits-evo-s0): the shot landed on t755, 110 off a Knight and a
//! Musketeer beside it (43 at level 1) and the Freeze; its area hit both again for 110 on t815, 3000 ms on.
//! combat.EVO_IMPACT_AREA_ANCHOR (item 289): the shipped landing_point keeps the area where the shot landed; under
//! client15535_follows_target it rides the unit the shot landed on (sp-f2-ice-s0 t752: the Hog Rider it froze took the
//! area's 110 and a second freeze 4,200 off the landing point).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_ice_spirits`):
//!   - impact_area_cast -> `its_area_stands_where_the_shot_landed_and_hits_once_3000_ms_on` red.
//!   - impact_area_stands -> `its_area_rides_the_unit_it_landed_on_under_client15535_follows_target` red.
//!   - one_hit_area_damage_refused -> the same test red: the form is refused, its area's Damage read as a repeating
//!     pulse's (card.rs `convert_one_hit_area`).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, EvoImpactAreaAnchor};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The scene under `arm`: (the frame the shot landed, the Knight's hitpoints, the Giant's, the Knight's top, the Giant's).
fn impact_run(arm: EvoImpactAreaAnchor) -> (usize, Vec<i32>, Vec<i32>, i32, i32) {
    let mut cfg: BattleConfig = config();
    cfg.calib.evo_impact_area_anchor = arm;
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
    // The spirit 2500 short of a red Knight, a red Giant 2040 from the Knight (in the shot's splash; it targets buildings
    // only, so it cannot shoot the spirit while it deploys, as a Musketeer there did), both held.
    let (kn_at, mu_at) = (n(9000, 13000), n(7000, 13400));
    s.spawn_unit(Team::Blue, "IceSpirits_EV1", n(9000, 10500), None).expect("the spirit");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kn_at, None).expect("a red Knight");
    let musk = s.scenario_spawn_now(Team::Red, "Giant", mu_at, None).expect("a red Giant");
    let (ktop, mtop) = (s.entity(knight).expect("the Knight").max_hp, s.entity(musk).expect("the Giant").max_hp);
    let (mut landed, mut kh, mut mh) = (None, Vec::new(), Vec::new());
    // The spirit's first 60 frames (point, attack phase, target, shots in flight), for the message if the shot never lands.
    let mut trail = Vec::new();
    for k in 0..400 {
        // Held until the shot lands; from the next tick the Knight stands 4500 off the landing point.
        let far = landed.is_some();
        assert!(s.debug_set_pos(knight, if far { n(13500, 13000) } else { kn_at }));
        assert!(s.debug_set_pos(musk, mu_at));
        s.tick();
        if k < 60 {
            let sp = find_live(&s, Team::Blue, "IceSpirits_EV1").first().map(|e| (e.pos.x / K, e.pos.y / K, format!("{:?}", e.attack_phase), e.target));
            trail.push((k, sp, s.projectiles().len()));
        }
        kh.push(s.entity(knight).expect("the Knight").hp);
        mh.push(s.entity(musk).expect("the Giant").hp);
        if landed.is_none() && kh[k] < ktop {
            landed = Some(k);
        }
        if landed.is_some_and(|h| k >= h + 70) {
            break;
        }
    }
    let h = landed.unwrap_or_else(|| panic!("the shot never landed on the Knight: {trail:?}"));
    assert_eq!((kh[h], mh[h]), (ktop - 110, mtop - 110), "the shot's 110 on both");
    (h, kh, mh, ktop, mtop)
}

#[test]
fn its_area_stands_where_the_shot_landed_and_hits_once_3000_ms_on() {
    let (h, kh, mh, ktop, mtop) = impact_run(EvoImpactAreaAnchor::LandingPoint);
    // The area stands where the shot landed: one hit of 110 on the landing + 60 on the Giant beside that point, none
    // on the Knight 4500 off it.
    for k in h..=h + 70 {
        let want = if k < h + 60 { mtop - 110 } else { mtop - 220 };
        assert_eq!(mh[k], want, "the Giant on frame {k} (landed on {h}): {:?}", &mh[h..]);
        assert_eq!(kh[k], ktop - 110, "the Knight on frame {k} (landed on {h}): {:?}", &kh[h..]);
    }
}

/// combat.EVO_IMPACT_AREA_ANCHOR (item 289): under client15535_follows_target the area's one hit, 60 frames on, lands on
/// the Knight it rode 4,500 off the landing point, and not on the Giant beside that point. Plant: impact_area_stands.
#[test]
fn its_area_rides_the_unit_it_landed_on_under_client15535_follows_target() {
    let (h, kh, mh, ktop, mtop) = impact_run(EvoImpactAreaAnchor::Client15535FollowsTarget);
    for k in h..=h + 70 {
        let want = if k < h + 60 { ktop - 110 } else { ktop - 220 };
        assert_eq!(kh[k], want, "the Knight on frame {k} (landed on {h}): {:?}", &kh[h..]);
        assert_eq!(mh[k], mtop - 110, "the Giant on frame {k} (landed on {h}): {:?}", &mh[h..]);
    }
}
