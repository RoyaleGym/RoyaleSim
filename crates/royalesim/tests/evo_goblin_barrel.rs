//! THE EVO GOBLIN BARREL (tools/extract_cards.py `mirror_block`; card.rs `EvoDef::mirror`; state.rs `phase_spawn`), at
//! level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-GoblinBarrel-evo-s0): played at (14500, 13500), its three Goblins and, about
//! (3500, 13500), three GoblinDummies of 81 hitpoints stood on one tick, laid out alike.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_goblin_barrel`):
//!   - barrel_decoy_never -> `its_decoy_barrel_lands_on_the_mirrored_point_with_three_goblin_dummies` red;
//!   - release_ground_point_shifts_decoy -> the same red (spells.RELEASE_GROUND_POINT's one-unit shift taken by the decoy,
//!     on the left half: its dummies one lower in x than the Goblins' points mirrored).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

#[test]
fn its_decoy_barrel_lands_on_the_mirrored_point_with_three_goblin_dummies() {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["GoblinBarrel".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    // The card the next Goblin Barrel play resolves to (state.rs `resolve_play`).
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "GoblinBarrel").expect("the Goblin Barrel in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    // The basic plays far off, until the next is the form.
    let mut plays = 0;
    while next(&s) != "GoblinBarrel_EV1" {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "GoblinBarrel", n(3000, 28000)).expect("a basic play");
        for _ in 0..40 {
            s.tick();
        }
        plays += 1;
        assert!(plays < 6, "no evolved play after {plays}");
    }
    let mut seen: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "GoblinBarrel", n(14500, 13500)).expect("the evolved play");
    let mut first: Vec<(usize, String, i32, (i32, i32))> = Vec::new();
    for k in 0..80usize {
        s.tick();
        for e in s.entities().filter(|e| e.team == Team::Blue && !seen.contains(&e.id)) {
            first.push((k, e.card.to_string(), e.max_hp, (e.pos.x / K, e.pos.y / K)));
        }
        seen = s.entities().map(|e| e.id).collect();
    }
    // Its Goblins about its point (a basic play's may land late, far off).
    let goblins: Vec<_> = first.iter().filter(|f| f.1 == "Goblin" && (f.3 .1 - 13500).abs() < 2000).collect();
    let dummies: Vec<_> = first.iter().filter(|f| f.1 == "GoblinDummy").collect();
    assert_eq!((goblins.len(), dummies.len()), (3, 3), "its units: {first:?}");
    assert!(goblins.iter().chain(&dummies).all(|f| f.0 == goblins[0].0), "one landing tick: {first:?}");
    assert!(dummies.iter().all(|f| f.2 == 81), "the dummies' 81: {dummies:?}");
    let mut want: Vec<(i32, i32)> = goblins.iter().map(|f| (18000 - f.3 .0, f.3 .1)).collect();
    let mut got: Vec<(i32, i32)> = dummies.iter().map(|f| f.3).collect();
    want.sort_unstable();
    got.sort_unstable();
    assert_eq!(got, want, "the dummies on the Goblins' points mirrored: {first:?}");
}

/// The real Goblins' points and the decoys' points (native).
type DecoyPoints = (Vec<(i32, i32)>, Vec<(i32, i32)>);

/// The evolved Goblin Barrel played at `tap` under spells.EVO_BARREL_DECOY_POINTS = `arm`: (its Goblins' first points mirrored
/// (18000 - x, y), its GoblinDummies' first points), each sorted, native.
fn decoy_points(arm: royalesim::state::EvoBarrelDecoyPoints, tap: (i32, i32)) -> DecoyPoints {
    let mut cfg: BattleConfig = config();
    cfg.calib.evo_barrel_decoy_points = arm;
    cfg.decks = [vec!["GoblinBarrel".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "GoblinBarrel").expect("the Goblin Barrel in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    let mut plays = 0;
    while next(&s) != "GoblinBarrel_EV1" {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy(Team::Blue, "GoblinBarrel", n(3000, 28000)).expect("a basic play");
        for _ in 0..40 {
            s.tick();
        }
        plays += 1;
        assert!(plays < 6, "no evolved play after {plays}");
    }
    let mut seen: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "GoblinBarrel", n(tap.0, tap.1)).expect("the evolved play");
    let (mut goblins, mut dummies) = (Vec::new(), Vec::new());
    for _ in 0..80 {
        s.tick();
        for e in s.entities().filter(|e| e.team == Team::Blue && !seen.contains(&e.id)) {
            let p = (e.pos.x / K, e.pos.y / K);
            if e.card == "GoblinDummy" {
                dummies.push(p);
            } else if e.card == "Goblin" && (p.1 - tap.1).abs() < 2000 {
                goblins.push((18000 - p.0, p.1));
            }
        }
        seen = s.entities().map(|e| e.id).collect();
    }
    goblins.sort_unstable();
    dummies.sort_unstable();
    (goblins, dummies)
}

/// spells.EVO_BARREL_DECOY_POINTS = client16402_mirrored_members (client 16.402, parity's r74 census: 66 of 74 dummies on the
/// exact mirror of their real Goblin; the engine's own ring 1 to 2 off in 63 of 71): over taps on both lanes and halves the
/// dummies stand exactly on the Goblins' points mirrored; under mirrored_cast_own_formation (the vacuity check) at least one
/// tap lays them off it. Plant: decoy_own_formation.
#[test]
fn under_client16402_mirrored_members_the_decoys_stand_on_the_goblins_points_mirrored() {
    use royalesim::state::EvoBarrelDecoyPoints as D;
    let taps = [(3499, 21000), (4600, 22500), (14500, 13500), (13300, 20100), (6500, 23500)];
    let mut off = 0;
    for tap in taps {
        let (g, d) = decoy_points(D::MirroredCastOwnFormation, tap);
        assert_eq!((g.len(), d.len()), (3, 3), "tap {tap:?}: the units");
        off += usize::from(g != d);
        let (g, d) = decoy_points(D::Client16402MirroredMembers, tap);
        assert_eq!(d, g, "client16402_mirrored_members, tap {tap:?}: the dummies off the Goblins' points mirrored");
    }
    assert!(off > 0, "mirrored_cast_own_formation (the vacuity check): every tap's decoy already on the mirror");
}
