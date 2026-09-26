//! THE LOADABLE SET, ROW BY ROW (card.rs `CardDb::from_json_str`).
//!
//! Whether a row of a card table loads is decided at many refusal sites, and the tests
//! of a site name the rows they were written for. A change aimed at one row reaches
//! every row that shares its refusal: lifting a refusal loads every row whose only
//! refusal it was, whether anyone listed that row or not. Only a check over every row
//! sees it. So this file builds the CardDb from each shipped table and holds it
//! against two lists written out below:
//!   (a) the LOADABLE rows (registered, not a summon-only unit) in CardDb order. That
//!       is the default catalogue's order (py.rs `Battle(card_names=None)`), so a
//!       row's place is its catalogue id, and the crown towers follow it;
//!   (b) the REJECTED rows, sorted, each with the FIRST CLAUSE of its refusal
//!       (`first_clause`: up to the first `:` or `;` outside parentheses), so a row
//!       that is still refused, but by another site, shows up too. The clause names
//!       the site; a different reason inside the same block keeps it.
//! The lists move only by an edit in the change that moves them. A failure names every
//! row that moved and prints both lists in full, ready to paste over the old ones, so
//! the diff of that edit is the list of rows the change reached.
//!
//! THE TABLES. data/derived/cards.json, the 15.535.29 table the engine loads (the
//! README's stage 3 copies the committed cards-15.535.json there), and
//! data/derived/cards-2018.json (tools/extract_cards.py --vintage 2018). Each list
//! belongs to one table `version`. This file does not go through `common::cards()`,
//! which panics without cards.json: a table that is absent, or that holds another
//! version, is SKIPPED LOUDLY instead. The skip is one line on stderr, written past
//! the harness's output capture so that it shows in a passing run too, naming the
//! file. Under CI, which writes both tables, a skip is a failure.
//!
//! THE CHECKS:
//!   1. `the_15535_table_loads_exactly_the_pinned_rows`: cards.json gives 103
//!      loadable rows (the 101 catalogue cards, then PrincessTower and KingTower) and
//!      43 rejected ones, and every row of the file is one or the other, once;
//!   2. `the_2018_table_loads_exactly_the_pinned_rows`: the same over cards-2018.json:
//!      69 loadable rows (the catalogue, then the towers) and 11 rejected ones, the
//!      lists the loader gave BEFORE the change that landed this file;
//!   3. `the_census_of_a_small_file_is_exact`: the census itself on a synthetic file
//!      (a summon-only unit left out, a row refused before its push and one after,
//!      the fallback towers last);
//!   4. `the_comparison_names_every_row_that_moved`: a row admitted, a row refused at
//!      another site and a pure reorder are each reported, and an unmoved row is not;
//!   5. `a_first_clause_ends_at_a_colon_or_semicolon_outside_parentheses`.
//!
//! 3, 4 and 5 need no table.
//!
//! PLANT: `RUSTFLAGS='--cfg clash_plant="census_admits_one"' CARGO_TARGET_DIR=target/plant
//! cargo test --test loadable_census`: `from_json_str` keeps a card it rejects after
//! its push registered, its blocks dropped, running as the plain unit -> 1 red with
//! ElixirGolem newly loadable and no longer rejected; 2 red with MovingCannon the same,
//! read from the loader before this file landed; 3, 4 and
//! 5 are synthetic and stay green. The two rows are ones no other test pins in that
//! table, which is the point: a row another test names is covered already. When the
//! 15.535.29 ElixirGolem loads for real, the plant lands on the 2018 table alone and
//! needs another 15.535.29 row.

use royalesim::card::{CardDb, CardSource, KING_TOWER, PRINCESS_TOWER};

/// data/derived beside the crate, where `CardDb::load_repo_file` reads.
const DERIVED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived");

/// One shipped table.
struct Table {
    /// The test that reads it, for the skip line.
    test: &'static str,
    /// Its file under data/derived.
    file: &'static str,
    /// The cards.json `version` its lists belong to.
    version: &'static str,
    /// How a checkout gets the file.
    make: &'static str,
    /// The suffix of its list constants (`LOADABLE_<tag>`, `REJECTED_<tag>`).
    tag: &'static str,
}

const TABLE_15535: Table = Table {
    test: "the_15535_table_loads_exactly_the_pinned_rows",
    file: "cards.json",
    version: "cards-15535.1",
    make: "the README's stage 3 copies data/derived/cards-15.535.json to data/derived/cards.json",
    tag: "15535",
};

const TABLE_2018: Table = Table {
    test: "the_2018_table_loads_exactly_the_pinned_rows",
    file: "cards-2018.json",
    version: "cards-2018.1",
    make: "tools/extract_cards.py --vintage 2018 writes it",
    tag: "2018",
};

/// A table's two lists.
struct Pin {
    /// Every loadable row, in CardDb order: the default catalogue, then the towers.
    loadable: &'static [&'static str],
    /// Every rejected row with the first clause of its refusal, sorted.
    rejected: &'static [(&'static str, &'static str)],
}

/// cards.json at `version` cards-15535.1: the committed cards-15.535.json, FNV-1a 64
/// 5a1dac3d2fb1b4a9. The lists are what the loader gives for that file.
/// data/derived/replay/card_census.json (`cargo run --example replay_parity --
/// --census`), written from the same file, holds the same 103 loadable rows in the
/// same order and the same 43 refusals, word for word.
const PIN_15535: Pin = Pin { loadable: LOADABLE_15535, rejected: REJECTED_15535 };

const LOADABLE_15535: &[&str] = &[
    "Knight",
    "Archer",
    "Goblins",
    "Giant",
    "Pekka",
    "Minions",
    "Balloon",
    "Witch",
    "Barbarians",
    "Golem",
    "Skeletons",
    "Valkyrie",
    "SkeletonArmy",
    "Bomber",
    "Musketeer",
    "BabyDragon",
    "Prince",
    "Wizard",
    "MiniPekka",
    "SpearGoblins",
    "GiantSkeleton",
    "HogRider",
    "MinionHorde",
    "IceWizard",
    "RoyalGiant",
    "SkeletonWarriors",
    "Princess",
    "DarkPrince",
    "LavaHound",
    "IceSpirits",
    "FireSpirits",
    "ZapMachine",
    "Bowler",
    "BattleRam",
    "InfernoDragon",
    "IceGolemite",
    "MegaMinion",
    "BlowdartGoblin",
    "GoblinGang",
    "ElectroWizard",
    "AngryBarbarians",
    "Hunter",
    "AxeMan",
    "Assassin",
    "RoyalRecruits",
    "DarkWitch",
    "Bats",
    "MiniSparkys",
    "Rascals",
    "MegaKnight",
    "DartBarrell",
    "Wallbreakers",
    "RoyalHogs",
    "Fisherman",
    "EliteArcher",
    "ElectroDragon",
    "Firecracker",
    "MightyMiner",
    "BattleHealer",
    "SkeletonKing",
    "ArcherQueen",
    "GoldenKnight",
    "SuperIceGolemite",
    "Monk",
    "SuperArcher",
    "RoyalRecruits_Chess",
    "SkeletonDragons",
    "SuperHogRiderTerry",
    "ElectroSpirit",
    "ElectroGiant",
    "PrinceBuff",
    "Phoenix",
    "TriWizards",
    "GoblinMachine",
    "SuperKnight",
    "SkeletonWarriors_SpookyChess",
    "Berserker",
    "MergeMaiden_Normal",
    "MergeMaiden_Mounted",
    "Cannon",
    "Mortar",
    "InfernoTower",
    "BombTower",
    "BarbarianHut",
    "Tesla",
    "Xbow",
    "Tombstone",
    "BarbarianLauncher",
    "GoblinCage",
    "GoblinPartyHut",
    "Fireball",
    "Arrows",
    "Rocket",
    "GoblinBarrel",
    "Freeze",
    "Zap",
    "Poison",
    "Log",
    "Tornado",
    "Earthquake",
    "Snowball",
    "PrincessTower",
    "KingTower",
];
const REJECTED_15535: &[(&str, &str)] = &[
    ("BarbLog", "rolling BarbLogProjectileRolling with targets / spawns / buffs is not simulated"),
    ("BossBandit", "the unit runs an action graph this loader does not read (ActionGroup, ActionPlayEffect, ActionRunIfGameObjectExists, ActionRunIfInstigatorMatches)"),
    ("Clone", "area effect Clone runs an action graph this loader does not read (ActionClone, ActionSpawn; spawns BuffType:Clone)"),
    ("DarkMagic", "area effect DarkMagicAOE hits neither ground nor air"),
    ("Elixir Collector", "missing hit_speed_ms"),
    ("ElixirGolem", "units.ElixirGolem2 itself spawns units (ElixirGolem4)"),
    ("FirespiritHut", "the unit runs an action graph this loader does not read (ActionInterval, ActionPlayEffect, ActionSpawnToLocation; spawns CharacterType:FireSpirits)"),
    ("Ghost", "hide_time_ms Some(400) / up_time_ms None on a card that does not hide"),
    ("GiantBuffer", "the unit runs an action graph this loader does not read (ActionGiantBufferBuff, ActionGiantBufferBuffVisual, ActionGiantBufferCollectFriends, ActionPlayEffect)"),
    ("GlobalClone", "area effect GlobalClone runs an action graph this loader does not read (ActionClone, ActionSpawn; spawns BuffType:Clone)"),
    ("GlobalLightning", "area effect Event_Global_Lightning_Charge1 runs an action graph this loader does not read (ActionSpawn; spawns AreaEffectType:Event_Global_Lightning_Charge2)"),
    ("GoblinCurse", "area effect GoblinCurse runs an action graph this loader does not read (ActionGroup, ActionPlayEffect, ActionSpawn; spawns AreaEffectType:GoblinCurseBase)"),
    ("GoblinDemolisher", "the unit runs an action graph this loader does not read (ActionChangeGameObjectData, ActionGroup, ActionRunActionAtHealth, ActionSpawn; spawns AreaEffectType:CancelTauntAEO)"),
    ("GoblinDrill", "the unit is spawned at its own king tower and travels underground to the tap (SpawnPathfindSpeed 300, morphing into GoblinDrill on arrival)"),
    ("GoblinGiant", "spawner SpearGoblinGiant"),
    ("GoblinHut", "the unit runs an action graph this loader does not read (ActionGoblinHutLifeState, ActionGroup, ActionPlayEffect)"),
    ("GoblinPartyRocket", "projectile GoblinMorphProjectile with a target cap or an area effect is not simulated"),
    ("GoblinRocketSilo", "the unit runs an action graph this loader does not read (ActionChangeGameObjectData, ActionGroup, ActionPlayEffect)"),
    ("Goblinstein", "the unit runs an action graph this loader does not read (ActionActivateOnCardDeploy, ActionEnabbleHPBarConditionForDuration, ActionGroup, ActionWithDuration)"),
    ("Graveyard", "area effect Graveyard_rework runs an action graph this loader does not read (ActionGroup, ActionSpawnToLocation; spawns CharacterType:Graveyard_rework_Skeleton, CharacterType:Graveyard_rework_Skeleton, CharacterType:Graveyard_rework_Skeleton, CharacterType:Graveyard_rework_Skeleton, CharacterType:Graveyard_rework_Skeleton, CharacterType:Graveyard_rework_Skeleton, CharacterType:Graveyard_rework_Skeleton, CharacterType:Graveyard_rework_Skeleton)"),
    ("Heal", "spell with no projectile and no area effect"),
    ("Lightning", "pulsing area effect Lightning pulses no buff"),
    ("LittlePrince", "the unit runs an action graph this loader does not read (ActionFilter, ActionGroup, ActionInterval, ActionSetAttackSequenceIndex, ActionSetVariable)"),
    ("MergeMaiden", "spell with no projectile and no area effect"),
    ("Miner", "the unit is spawned at its own king tower and travels underground to the tap (SpawnPathfindSpeed 650)"),
    ("Mirror", "spell with no projectile and no area effect"),
    ("MovingCannon", "the unit runs an action graph this loader does not read (ActionChangeGameObjectData, ActionPlayEffect, ActionRunActionAtHealth)"),
    ("Rage", "spell with no projectile and no area effect"),
    ("RageBarbarian", "death area effect RageBarbarianDummyForSpawn"),
    ("RamRider", "spawner RamRider"),
    ("Ronin", "the unit runs an action graph this loader does not read (ActionCounter, ActionDealDamage, ActionGroup, ActionPlayEffect, ActionRunForcedAnimationOnce, ActionSpawn, ActionWithDuration; spawns BuffType:ronin_reflect_stun_buff)"),
    ("RoyalDelivery", "pulsing area effect RoyalDeliveryArea pulses no buff"),
    ("SkeletonBalloon", "the unit runs an action graph this loader does not read (ActionSkeletonBarrelPopBalloon)"),
    ("SuperEliteArcher", "the unit's projectile"),
    ("SuperHogRider", "units.SantaPresent"),
    ("SuperLavaHound", "units.SuperLavaHound2 itself spawns units (LavaPups)"),
    ("SuperMiniPekka", "units.SuperMiniPekkaPancakes"),
    ("SuperWitch", "the unit's projectile"),
    ("SuspiciousBush", "death area effect SuspiciousBush_DummyAEO"),
    ("ThreeMusketeers", "the unit runs an action graph this loader does not read (ActionFilter, ActionSetAttackSequenceIndex)"),
    ("Vines", "area effect Vines_AeO runs an action graph this loader does not read (ActionAirToGround, ActionGroup, ActionRunActionListOnObjectsInShapeWithPrio, ActionSelect, ActionSpawn; spawns BuffType:Vines_Trap_Snare_XXLarge, BuffType:Vines_Trap_Snare_XLarge, BuffType:Vines_Trap_Snare_Large, BuffType:Vines_Trap_Snare_Medium, BuffType:Vines_Trap_Snare_Small)"),
    ("WarmSpell", "own-troop area effect WarmAOE is not simulated"),
    ("WitchMother", "the unit's projectile"),
];

/// cards-2018.json at `version` cards-2018.1 (tools/extract_cards.py --vintage 2018), 457423 bytes, FNV-1a
/// 64 2c4978693f313a1a. The lists are what the loader BEFORE the change that landed this file gave for that
/// file: this file compiled unchanged on its parent, run there, `the_2018_table_loads_exactly_the_pinned_rows`
/// failing with these two constants as its paste. Lists printed by the change itself would only restate it.
const PIN_2018: Option<Pin> = Some(Pin { loadable: LOADABLE_2018, rejected: REJECTED_2018 });

const LOADABLE_2018: &[&str] = &[
    "Knight",
    "Archer",
    "Goblins",
    "Giant",
    "Pekka",
    "Minions",
    "Balloon",
    "Witch",
    "Barbarians",
    "Golem",
    "Skeletons",
    "Valkyrie",
    "SkeletonArmy",
    "Bomber",
    "Musketeer",
    "BabyDragon",
    "Prince",
    "Wizard",
    "MiniPekka",
    "SpearGoblins",
    "GiantSkeleton",
    "HogRider",
    "MinionHorde",
    "IceWizard",
    "RoyalGiant",
    "SkeletonWarriors",
    "Princess",
    "DarkPrince",
    "ThreeMusketeers",
    "LavaHound",
    "IceSpirits",
    "FireSpirits",
    "ZapMachine",
    "Bowler",
    "BattleRam",
    "InfernoDragon",
    "IceGolemite",
    "MegaMinion",
    "BlowdartGoblin",
    "GoblinGang",
    "ElectroWizard",
    "AngryBarbarians",
    "AxeMan",
    "Assassin",
    "DarkWitch",
    "Bats",
    "MegaKnight",
    "DartBarrell",
    "Cannon",
    "GoblinHut",
    "Mortar",
    "InfernoTower",
    "BombTower",
    "BarbarianHut",
    "Tesla",
    "Xbow",
    "Tombstone",
    "FirespiritHut",
    "Fireball",
    "Arrows",
    "Rocket",
    "GoblinBarrel",
    "Freeze",
    "Zap",
    "Poison",
    "Log",
    "Tornado",
    "PrincessTower",
    "KingTower",
];
const REJECTED_2018: &[(&str, &str)] = &[
    ("Clone", "own-troop area effect Clone is not simulated"),
    ("Elixir Collector", "missing hit_speed_ms"),
    ("Graveyard", "own-troop area effect Graveyard is not simulated"),
    ("Heal", "own-troop area effect Heal is not simulated"),
    ("Lightning", "pulsing area effect Lightning pulses no buff"),
    ("Miner", "the unit is spawned at its own king tower and travels underground to the tap (SpawnPathfindSpeed 650)"),
    ("Mirror", "spell with no projectile and no area effect"),
    ("MovingCannon", "units.BrokenCannon is a troop with a LifeTime"),
    ("Rage", "own-troop area effect Rage is not simulated"),
    ("RageBarbarian", "units.RageBarbarianBottle"),
    ("SkeletonBalloon", "units.SkeletonContainer"),
];

// ---------------------------------------------------------------------------
// the census and the comparison

/// What a table loads to: `Pin`'s two lists as the loader gives them.
#[derive(Clone, Debug, PartialEq)]
struct Census {
    loadable: Vec<String>,
    rejected: Vec<(String, String)>,
}

/// The census of `db`. LOADABLE: every record a name resolves to that is not a
/// summon-only unit, in CardDb order (a card rejected after its push stays in `cards`,
/// but no name resolves to it); the same rule examples/replay_parity/harness.rs
/// `census` writes to card_census.json. REJECTED: `CardDb::rejected` with each reason
/// cut to its first clause, sorted; a row refused twice is listed twice.
fn census(db: &CardDb) -> Census {
    let loadable = db
        .cards
        .iter()
        .enumerate()
        .filter(|(i, c)| !c.summon_only && db.index(&c.name) == Some(*i as u16))
        .map(|(_, c)| c.name.clone())
        .collect();
    let mut rejected: Vec<(String, String)> = db.rejected.iter().map(|(n, why)| (n.clone(), first_clause(why).to_string())).collect();
    rejected.sort();
    Census { loadable, rejected }
}

/// A refusal up to its first `:` or `;` outside parentheses. The loader writes a
/// refusal from inside a block as `<the block>: <why>`, and a consequence after `;`.
/// A parenthesised list (an action graph's classes, a unit's name) belongs to the
/// clause it sits in, separators and all.
fn first_clause(why: &str) -> &str {
    let mut depth = 0;
    for (i, ch) in why.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ':' | ';' if depth == 0 => return why[..i].trim_end(),
            _ => {}
        }
    }
    why.trim_end()
}

/// Ok when `got` is `pin`; else Err with one line per row that moved (or, when no row
/// did, the order that moved).
fn compare(got: &Census, pin: &Pin) -> Result<(), String> {
    let loadable: Vec<&str> = got.loadable.iter().map(String::as_str).collect();
    let rejected: Vec<(&str, &str)> = got.rejected.iter().map(|(n, w)| (n.as_str(), w.as_str())).collect();
    if loadable == pin.loadable && rejected == pin.rejected {
        return Ok(());
    }
    let mut moved: Vec<String> = Vec::new();
    for n in loadable.iter().filter(|n| !pin.loadable.contains(*n)) {
        moved.push(format!("  NOW LOADABLE, not in the list: {n}"));
    }
    for n in pin.loadable.iter().filter(|n| !loadable.contains(*n)) {
        moved.push(format!("  NO LONGER LOADABLE: {n}"));
    }
    for (n, why) in &rejected {
        match pin.rejected.iter().find(|(p, _)| p == n) {
            None => moved.push(format!("  NOW REJECTED, not in the list: {n} -- {why:?}")),
            Some((_, was)) if was != why => moved.push(format!("  REFUSED AT ANOTHER SITE: {n} -- listed {was:?}, now {why:?}")),
            Some(_) => {}
        }
    }
    for (n, was) in pin.rejected.iter().filter(|(p, _)| !rejected.iter().any(|(r, _)| r == p)) {
        moved.push(format!("  NO LONGER REJECTED: {n} -- was {was:?}"));
    }
    if moved.is_empty() {
        // The same rows, each on its side: what moved is an order, or a row refused twice.
        let first = loadable.iter().zip(pin.loadable).position(|(a, b)| a != b);
        moved.push(match first {
            Some(k) => format!("  THE CATALOGUE ORDER MOVED at place {k}: listed {}, now {} (every later catalogue id moves with it)", pin.loadable[k], loadable[k]),
            None if loadable.len() != pin.loadable.len() => format!("  THE LOADABLE LIST repeats a row ({} entries, {} listed)", loadable.len(), pin.loadable.len()),
            None => format!("  THE REJECTED LIST repeats a row or is out of order ({} entries, {} listed)", rejected.len(), pin.rejected.len()),
        });
    }
    Err(moved.join("\n"))
}

/// Both lists as the Rust to paste over `LOADABLE_<tag>` and `REJECTED_<tag>`.
fn paste_form(tag: &str, got: &Census) -> String {
    let loadable: Vec<String> = got.loadable.iter().map(|n| format!("    {n:?},")).collect();
    let rejected: Vec<String> = got.rejected.iter().map(|(n, why)| format!("    ({n:?}, {why:?}),")).collect();
    format!(
        "const LOADABLE_{tag}: &[&str] = &[\n{}\n];\nconst REJECTED_{tag}: &[(&str, &str)] = &[\n{}\n];",
        loadable.join("\n"),
        rejected.join("\n")
    )
}

/// FNV-1a 64: the hash data/derived/replay/card_census.json records for the cards.json
/// it was written from. A report prints it to say which bytes it is about.
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// SKIP LOUDLY. The line goes straight to the process's stderr, which the harness's
/// capture (it takes `print!` and `eprint!` output) does not intercept, so it shows in
/// a passing run too; the caller then returns. Under CI it is a failure instead: the
/// workflow writes both tables (the README's stage 3), so a missing one there is a
/// broken run, not a checkout without data.
fn skip(test: &str, why: &str) {
    if std::env::var_os("CI").is_some() {
        panic!("{why} -- and CI is set, where a skip would read as a pass");
    }
    let line = format!("\nSKIP loadable_census::{test}: {why}. The loadable set was NOT checked: this is not a pass.\n");
    let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
}

/// The census of `table`, with a line naming the bytes it came from. Err(why to skip)
/// when the file is absent or holds another version. A table that is there and does
/// not load is a failure, not a skip.
fn load(table: &Table) -> Result<(Census, String), String> {
    let Table { file, version, make, .. } = *table;
    let path = format!("{DERIVED}/{file}");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(format!("{path} is absent ({make})")),
        Err(e) => panic!("{path}: {e}"),
    };
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    let found = doc["version"].as_str().unwrap_or("(none)");
    let what = format!("{path} (version {found}, {} bytes, FNV-1a 64 {:016x})", text.len(), fnv1a64(text.as_bytes()));
    if found != version {
        return Err(format!("{what} is not the {version} table its lists belong to ({make})"));
    }
    let db = CardDb::from_json_str(&text, CardSource::DerivedJson).unwrap_or_else(|e| panic!("{what}: the loader refused the whole file: {e}"));
    assert!(!db.towers_from_fallback, "{what}: the file has no crown towers, and the fallback pair is not its rows");
    let got = census(&db);
    // Every row of the file is loaded or refused, and none is both.
    let mut rows: Vec<&str> = ["cards", "towers"]
        .iter()
        .flat_map(|k| doc[*k].as_array().into_iter().flatten())
        .map(|row| row["name"].as_str().unwrap_or_else(|| panic!("{what}: a row without a name: {row}")))
        .collect();
    rows.sort_unstable();
    rows.dedup();
    let mut seen: Vec<&str> = got.loadable.iter().chain(got.rejected.iter().map(|(n, _)| n)).map(String::as_str).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, rows, "{what}: the loaded and the refused rows are not the file's rows");
    for n in &got.loadable {
        assert!(!got.rejected.iter().any(|(r, _)| r == n), "{what}: {n} is both loaded and refused");
    }
    Ok((got, what))
}

/// Load `table` and hold it against `pin`; None is the placeholder, which fails.
fn check_table(table: &Table, pin: Option<&Pin>) {
    let Table { test, file, tag, .. } = *table;
    let loaded = load(table);
    let Some(pin) = pin else {
        match loaded {
            Ok((got, what)) => panic!(
                "{what}: THE LISTS FOR {file} ARE A PLACEHOLDER, so this test fails until they are filled. The file loads \
                 {} rows and refuses {}. If this build is the loader BEFORE the change that lands this file (never the \
                 change itself; see PIN_{tag}), paste these two constants into tests/loadable_census.rs and set PIN_{tag} \
                 to Some(Pin {{ loadable: LOADABLE_{tag}, rejected: REJECTED_{tag} }}):\n\n{}\n",
                got.loadable.len(),
                got.rejected.len(),
                paste_form(tag, &got)
            ),
            Err(why) => panic!("THE LISTS FOR {file} ARE A PLACEHOLDER, so this test fails until they are filled, and they cannot be filled here: {why}"),
        }
    };
    let (got, what) = match loaded {
        Ok(loaded) => loaded,
        Err(why) => {
            skip(test, &why);
            return;
        }
    };
    if let Err(moved) = compare(&got, pin) {
        panic!(
            "{what}: THE LOADABLE SET MOVED. It loads {} rows and refuses {}; the lists hold {} and {}.\n{moved}\n\n\
             If the change meant to move these rows, paste the lists below over LOADABLE_{tag} and REJECTED_{tag} in \
             tests/loadable_census.rs, so that its diff names them. If it did not, the change reached rows it did not \
             plan for.\n\n{}\n",
            got.loadable.len(),
            got.rejected.len(),
            pin.loadable.len(),
            pin.rejected.len(),
            paste_form(tag, &got)
        );
    }
}

// ---------------------------------------------------------------------------
// (1), (2)

/// Plant: census_admits_one.
#[test]
fn the_15535_table_loads_exactly_the_pinned_rows() {
    check_table(&TABLE_15535, Some(&PIN_15535));
}

#[test]
fn the_2018_table_loads_exactly_the_pinned_rows() {
    check_table(&TABLE_2018, PIN_2018.as_ref());
}

// ---------------------------------------------------------------------------
// (3)

/// One row of each kind the census must tell apart:
///   Alpha    loads, with a spawner whose Imp is a summon-only unit (not a row, not
///            listed);
///   Beta     refused before its push (no HitSpeed);
///   Gamma    refused AFTER its push: its death spawn Nester death-spawns Imp itself,
///            a chain, and the refusal goes on after a `;`;
///   Delta    refused before its push from inside its spawner block (a `:`);
///   Epsilon  loads.
/// No towers: the fallback pair follows every card.
const SMALL: &str = r#"{ "version": "test", "cards": [
 { "name":"Alpha", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "spawner":{"character":"Imp", "number":1, "pause_time_ms":5000} },
 { "name":"Beta", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300,
   "range_milli":1000, "collision_radius_milli":500 },
 { "name":"Gamma", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Nester", "count":1} },
 { "name":"Delta", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "spawner":{"character":"Imp", "number":1} },
 { "name":"Epsilon", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500 }
 ],
 "units": {
  "Imp":    { "name":"Imp", "rarity":"Common", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Nester": { "name":"Nester", "rarity":"Common", "hitpoints":120, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300,
              "death_spawn":{"character":"Imp", "count":1} }
 }
}"#;

#[test]
fn the_census_of_a_small_file_is_exact() {
    let db = CardDb::from_json_str(SMALL, CardSource::DerivedJson).unwrap();
    let got = census(&db);
    assert_eq!(got.loadable, ["Alpha", "Epsilon", KING_TOWER, PRINCESS_TOWER], "{:?}", db.rejected);
    let rejected: Vec<(&str, &str)> = got.rejected.iter().map(|(n, w)| (n.as_str(), w.as_str())).collect();
    assert_eq!(rejected, [("Beta", "missing hit_speed_ms"), ("Delta", "spawner Imp"), ("Gamma", "units.Nester itself spawns units (Imp)")]);
    // What the census leaves out is there: the unit loaded, and the chain's refusal
    // is longer than its first clause.
    assert!(db.cards.iter().any(|c| c.summon_only && c.name == "Imp"), "the spawner's unit did not load");
    let (_, gamma) = db.rejected.iter().find(|(n, _)| n == "Gamma").expect("Gamma is refused");
    assert!(gamma.len() > rejected[2].1.len(), "{gamma}");
}

// ---------------------------------------------------------------------------
// (4)

/// Lists for the comparison's own test, with no table behind them.
const TINY: Pin = Pin {
    loadable: &["Alpha", "Beta", "Gamma", KING_TOWER],
    rejected: &[("Delta", "missing hitpoints"), ("Omega", "units.Nester itself spawns units (Imp)")],
};

#[test]
fn the_comparison_names_every_row_that_moved() {
    let exact = Census {
        loadable: TINY.loadable.iter().map(|n| n.to_string()).collect(),
        rejected: TINY.rejected.iter().map(|(n, w)| (n.to_string(), w.to_string())).collect(),
    };
    assert_eq!(compare(&exact, &TINY), Ok(()));
    // A row admitted: Delta leaves the refused list for the catalogue.
    let mut admitted = exact.clone();
    admitted.rejected.retain(|(n, _)| n != "Delta");
    admitted.loadable.insert(1, "Delta".into());
    let report = compare(&admitted, &TINY).unwrap_err();
    assert!(report.contains("NOW LOADABLE, not in the list: Delta") && report.contains("NO LONGER REJECTED: Delta"), "{report}");
    assert!(!report.contains("Alpha") && !report.contains("Omega"), "an unmoved row is reported: {report}");
    // A row still refused, but at another site.
    let mut resited = exact.clone();
    resited.rejected[1].1 = "units.Nester".into();
    let report = compare(&resited, &TINY).unwrap_err();
    assert!(report.contains("REFUSED AT ANOTHER SITE: Omega") && !report.contains("Delta"), "{report}");
    // A row gone from both lists.
    let mut lost = exact.clone();
    lost.loadable.retain(|n| n != "Gamma");
    let report = compare(&lost, &TINY).unwrap_err();
    assert!(report.contains("NO LONGER LOADABLE: Gamma"), "{report}");
    // Only the order: no row moved, the catalogue ids did.
    let mut reordered = exact;
    reordered.loadable.swap(0, 1);
    let report = compare(&reordered, &TINY).unwrap_err();
    assert!(report.contains("THE CATALOGUE ORDER MOVED at place 0: listed Alpha, now Beta"), "{report}");
}

// ---------------------------------------------------------------------------
// (5)

#[test]
fn a_first_clause_ends_at_a_colon_or_semicolon_outside_parentheses() {
    // The loader's refusal forms.
    for (why, clause) in [
        ("missing hit_speed_ms", "missing hit_speed_ms"),
        ("units.Nester itself spawns units (Imp); a spawn chain is not simulated", "units.Nester itself spawns units (Imp)"),
        ("units.SantaPresent: missing hitpoints", "units.SantaPresent"),
        (
            "death area effect D: area effect D runs an action graph this loader does not read (ActionSpawn; spawns CharacterType:B)",
            "death area effect D",
        ),
        (
            "area effect C runs an action graph this loader does not read (ActionClone, ActionSpawn; spawns BuffType:Clone)",
            "area effect C runs an action graph this loader does not read (ActionClone, ActionSpawn; spawns BuffType:Clone)",
        ),
        ("hide_time_ms Some(400) / up_time_ms None on a card that does not hide", "hide_time_ms Some(400) / up_time_ms None on a card that does not hide"),
    ] {
        assert_eq!(first_clause(why), clause, "{why}");
    }
}
