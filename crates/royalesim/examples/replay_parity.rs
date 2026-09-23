//! replay_parity -- play recorded real battles through the engine and score the two
//! per-tick states against each other.
//!
//!     cargo run --example replay_parity -- --census                   # what the loader accepts -> data/derived/replay/card_census.json
//!     cargo run --example replay_parity -- <fixture.replay.json> ...  # one or more fixtures
//!     cargo run --example replay_parity -- --all [--stride N]         # every fixture in data/derived/replay/, plus the aggregate
//!     ... [--out <dir>]                                               # default data/derived/replay/results/
//!     ... [--trace]                                                   # per-pair per-tick rows in the JSON (one battle at a time)
//!     ... [--prefix]                                                  # play an unplayable fixture up to its first unloadable deploy
//!     ... [--attacking-movement frozen|separation_only]               # judge a CANDIDATE without editing the ledger
//!
//! Fixtures come from tools/make_replay_fixture.py. Per fixture this writes
//! `<fixture>.parity.json` and `<fixture>.parity.md`; `--all` also writes
//! `aggregate.json` / `aggregate.md` (over every fixture per mechanic family and per
//! card, plus the first-divergence list) -- the corpus report's tables. The library
//! half is examples/replay_parity/harness.rs (shared with
//! tests/replay_parity.rs, which pins the harness invariants on the committed sample).
//!
//! THE COLUMNS. unit-ticks: matched pairs (or unmatched entities) x truth frame ticks
//! where at least one side has the unit alive. <=250 / <=500 / <=1000: both-alive
//! unit-ticks whose position error (native units, subtiles / 18) is within that.
//! <=250 moving: the same with the deploy-phase frames (the truth's `state` column in
//! `TRUTH_DEPLOY_STATES`, or the sim still deploying: both stationary at the spawn
//! point) left out of numerator and
//! denominator. hp exact / target / path n: both-alive unit-ticks where hp, the
//! target (by matched key) and the path-node count agree. alive/missing/extra:
//! unit-ticks where exactly one side has the unit (a matched pair dead on one side; a
//! truth entity with no sim counterpart; a sim entity with no truth counterpart).
//! walk <=250 / walk <=20: the isolated-walk subset (truth walking at a tower or at
//! nothing with full hp) within 250 and within 20 native (the bit-exact walks).
//! Fractions are over the row's unit-ticks (the walk columns over its walk ticks).
//! first divergence: the first unit-tick beyond 1000 native or an alive mismatch,
//! with the ROOT card (the card that was deployed), its register families and a
//! CAUSE read at the onset (the first tick the error passed 250): spawn (deploy timing
//! / a missing or extra unit) / knockback / status / attack-timing (attack state or hp
//! already disagree) / contact (a neighbour within reach) / walking (a free walk that
//! differs: path or step law). Every fixture report ends with the unit-ticks scored
//! after the engine's own end of battle (a frozen state) and a NOTE when the fixture
//! was classified against a different cards.json than the engine loaded.
//!
//! Needs data/derived/mechanic_register.json (python tools/mechanic_register.py) for
//! the family tables; the test on the sample runs without it.
#![allow(unexpected_cfgs)]

#[path = "replay_parity/harness.rs"]
mod harness;

use harness::*;
use royalesim::card::{CardDb, CardSource};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("replay_parity: {e}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let root = repo_root();
    let replay_dir = format!("{root}/data/derived/replay");
    let mut out_dir = format!("{replay_dir}/results");
    let mut fixtures: Vec<String> = Vec::new();
    let mut all = false;
    let mut census_only = false;
    let mut opts = Options::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--census" => census_only = true,
            "--all" => all = true,
            "--out" => {
                i += 1;
                out_dir = args.get(i).cloned().ok_or("--out needs a directory")?;
            }
            "--stride" => {
                i += 1;
                opts.stride = args.get(i).and_then(|s| s.parse().ok()).ok_or("--stride needs an integer")?;
            }
            "--trace" => opts.trace = true,
            "--prefix" => opts.prefix = true,
            // A CANDIDATE, not a change: run the corpus under the other arm of
            // movement.ATTACKING_UNIT_MOVEMENT without editing the ledger every other
            // session's engine reads. Two ledger edits and two rebuilds to answer one
            // question is how a shared file becomes an outage.
            "--attacking-movement" => {
                i += 1;
                let name = args.get(i).ok_or("--attacking-movement needs a candidate name")?;
                opts.attacking_movement = Some(
                    royalesim::state::AttackingUnitMovement::from_calibration_name(name)
                        .ok_or_else(|| format!("{name}: not a candidate of movement.ATTACKING_UNIT_MOVEMENT"))?,
                );
            }
            "--seed" => {
                i += 1;
                opts.seed = args.get(i).and_then(|s| s.parse().ok()).ok_or("--seed needs an integer")?;
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => fixtures.push(other.to_string()),
        }
        i += 1;
    }
    let db = CardDb::load_repo()?;
    if db.source != CardSource::DerivedJson {
        return Err("cards.json did not load from data/derived (fallback cards are not the game's)".into());
    }
    if census_only {
        std::fs::create_dir_all(&replay_dir).map_err(|e| e.to_string())?;
        let c = census(&db);
        let path = format!("{replay_dir}/card_census.json");
        std::fs::write(&path, serde_json::to_string_pretty(&c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        println!("{path}: {} loadable, {} rejected, {} summon-only (cards.json {})", c.loadable.len(), c.rejected.len(), c.summon_only.len(), c.cards_json_fnv1a64.as_deref().unwrap_or("unhashed"));
        return Ok(());
    }
    if all {
        let mut names: Vec<String> = std::fs::read_dir(&replay_dir)
            .map_err(|e| format!("{replay_dir}: {e} (run tools/make_replay_fixture.py --all first)"))?
            .filter_map(|e| e.ok())
            .map(|e| e.path().to_string_lossy().to_string())
            .filter(|p| p.ends_with(".replay.json"))
            .collect();
        names.sort();
        fixtures.extend(names);
    }
    if fixtures.is_empty() {
        return Err("no fixtures given (a path, or --all)".into());
    }
    let register = load_register(&register_path()).map_err(|e| format!("{e}: the report's per-family table needs the mechanic register (python tools/mechanic_register.py writes it)"))?;
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let mut reports = Vec::new();
    for path in &fixtures {
        let t0 = std::time::Instant::now();
        let f = Fixture::load(path)?;
        let r = replay(&f, &db, &register, &opts)?;
        let stem = Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let stem = stem.trim_end_matches(".replay.json").to_string();
        std::fs::write(format!("{out_dir}/{stem}.parity.json"), serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::write(format!("{out_dir}/{stem}.parity.md"), render_fixture_markdown(&r)).map_err(|e| e.to_string())?;
        let sc = &r.score;
        if r.playable {
            println!(
                "{stem}{}: {} unit-ticks, <=250 {}% (moving {}%), <=1000 {}%, hp exact {}%, first divergence {} ({:.1}s)",
                r.prefix_until.map(|c| format!(" [prefix < {c}]")).unwrap_or_default(),
                sc.unit_ticks,
                Score::permille(sc.within[0], sc.unit_ticks) / 10,
                Score::permille(sc.moving_within(0), sc.moving_ticks()) / 10,
                Score::permille(sc.within[2], sc.unit_ticks) / 10,
                Score::permille(sc.hp_exact, sc.unit_ticks) / 10,
                r.first_divergence.as_ref().map(|d| format!("tick {} {} {}", d.tick, d.card, d.cause)).unwrap_or_else(|| "none".into()),
                t0.elapsed().as_secs_f64()
            );
        } else {
            println!("{stem}: UNPLAYABLE -- {}", r.unplayable_reasons.join("; "));
        }
        reports.push(r);
    }
    if all || reports.len() > 1 {
        let a = aggregate(&reports);
        std::fs::write(format!("{out_dir}/aggregate.json"), serde_json::to_string_pretty(&a).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::write(format!("{out_dir}/aggregate.md"), render_aggregate_markdown(&a, &reports)).map_err(|e| e.to_string())?;
        println!(
            "aggregate: {} played ({} prefixes), {} unplayable, {} unit-ticks, <=250 {}%, <=1000 {}% -> {out_dir}/aggregate.md",
            a.fixtures_played,
            a.fixtures_prefix,
            a.fixtures_unplayable,
            a.score.unit_ticks,
            Score::permille(a.score.within[0], a.score.unit_ticks) / 10,
            Score::permille(a.score.within[2], a.score.unit_ticks) / 10
        );
    }
    Ok(())
}
