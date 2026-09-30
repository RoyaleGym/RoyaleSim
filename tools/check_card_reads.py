#!/usr/bin/env python3
"""The coverage gate: does the engine READ every mechanic the cards it LOADS carry?

WHY THIS EXISTS
    `tools/check_data.py` asks whether cards.json is a faithful copy of the card
    table.  It never asks the other question: of the columns that survive into
    cards.json, which ones does `crates/royalesim/src/card.rs` actually look at?
    Nothing crashes when it does not.  `RawCard` has no `deny_unknown_fields`, so a
    column the loader has no field for is dropped in silence, and the card plays as
    a plainer card with the same name -- an Inferno Dragon whose damage never ramps,
    a Mega Knight that never leaps, a Mortar that shoots at its own feet.  A deck
    drawn from the whole catalogue is much more likely than not to hold one.

    This gate builds the set of cards.json fields card.rs consumes, compares it BOTH
    WAYS against what the data carries, and says per card which mechanics are
    carried but unread.  It FAILS on the thin slice (the cards the engine is checked
    on) and REPORTS on the rest of the catalogue.

HOW THE READ SET IS DERIVED -- three mechanical links, no hand-written map
    1. card.rs -> the cards.json keys the loader deserialises.  The `Raw*` serde
       structs are parsed out of the source; a field is CONSUMED when it is declared
       in one of them AND accessed (`.field`) somewhere outside the struct
       declarations.  Declared-but-never-accessed is reported separately: serde fills
       it in and nothing reads it, which is the same silence as not declaring it.
       The loader also reads a few card-table columns straight out of a unit row's
       `raw` block (MultipleProjectiles, MultipleTargets, AllTargetsHit,
       CustomFirstProjectile: `ctx.units` ... `.get("raw")`).  Those are found the
       same way, off the source: a capitalised string literal passed alone to a call
       (`r.get("AllTargetsHit")`, `raw_count("MultipleProjectiles")`) that names a
       column some row's `raw` carries.
    2. tools/extract_cards.py -> which card-table column becomes which cards.json
       field.  `norm_unit`'s dict literal is walked with `ast`, so `c["ChargeRange"]`
       under the `charge` block gives ChargeRange -> charge.charge_range_raw.  Nine
       columns are read outside the literal (five in the prologue, and
       DeathSpawnPushback and the dash's JumpSpeed, DashConstantTime and
       DashLandingTime after it, on the 15.535 rows only); those are listed in
       PROLOGUE below and the gate refuses to run if the function reads a column
       outside the literal that the list does not name.
    3. cards.json `units[*].raw` -> which columns each character row actually ships.
       A column is UNREAD when link 2 gives it no cards.json field, or gives it one
       that link 1 says nothing reads.

    So the chain runs card table -> cards.json -> card.rs, each link read off the
    artefact itself.  Nobody has to remember to update a list when a column is
    added, and a field card.rs stops reading turns its columns red on the next run.

    What the chain cannot see is a calibration ARM.  A mechanic whose code sits
    behind a data/calibration.json key is loaded under every value of that key and
    run under one, so link 1 calls its columns read while the shipped value switches
    it off.  LOADED_NOT_RUN names those columns and their key by hand, because the
    arm is not in any artefact the chain reads, and the gate reads the key's shipped
    value: a column that value does not run is reported as unread.

WHAT COULD MAKE THIS WRONG (read this before trusting a green run)
    - `units[*].raw` is the character row MINUS tools/extract_cards.py's COSMETIC
      filter.  A mechanic column whose NAME matches that filter is invisible here --
      ProjectileStartRadius is one such column already (it is read; the filter drops
      it from `raw` because the name begins "ProjectileStart").  The filter cannot
      be re-run here to size the hole, because what it removed is gone; the gate
      instead counts the columns the extractor demonstrably reads that `raw` does
      not carry, and says out loud that the count is a lower bound.
    - A row's mechanic can live in a projectile, an area effect, a buff or a
      scripted action rather than in its own columns.  `raw` carries the character
      row alone, so those are out of this instrument's reach: the Electro Dragon's
      chained hit is a projectile column and the gate cannot see it.  That is what
      the register pass (`data/derived/mechanic_register.json`, when present) is
      for, and why that pass REPORTS rather than passes.
    - Two `Raw*` structs that share a field name make an unread field look read:
      `.name` is accessed for one of them and the search cannot tell which.  The
      report names every such collision.
    - A field accessed only under `#[cfg(test)]` counts as consumed.
    - A raw-block read is found by its literal (link 1): a capitalised string passed
      alone to a call that happens to spell a `raw` column's name would count as a
      read of that column.  Section A lists every one it found, so a reader can check.
    - LOADED_NOT_RUN is per column (or per cards.json key), with ONE per-row rule,
      RANGE_ROW_ONLY: a troop projectile's range keys are read for a RANGE projectile
      only (ProjectileRange and ProjectileRadius set), so under the shipped
      combat.RANGE_PROJECTILE = straight_to_range (since the 2026-09-27 flip) they are
      called run on a range row and still unread on every other row (the Wizard's
      projectile).  The per-card splash check below (AoeToAir / AoeToGround against the
      attacker's own flags) does not depend on it.
    - "Loaded" is every card the engine loads when the extension module imports:
      its default catalogue (royalesim.Battle(None, ...)) and every other card row
      a catalogue that names it alone builds with (royalesim.Battle([name], ...)).
      The default holds the Mirror and the cards that travel under ground too; any
      card row it did not hold would be offered by name.  Without the module the gate falls back to every card
      in the file and says so.  The thin slice is loaded under either reading.

VINTAGE AND A THIN CHECKOUT
    Runs on both card tables: `--cards data/derived/cards-2018.json` scores the 2018
    vintage.  It needs cards.json, card.rs and data/calibration.json and nothing
    else -- no card-table
    bundle, no `tools/mechanic_register.py` run, no built extension module.  The two
    optional passes (the engine catalogue, the mechanic register) each SKIP LOUDLY,
    naming what is missing.  A skip is not a pass.

USAGE (repo root)
    python tools/check_card_reads.py                  # the gate; exit 1 if red
    python tools/check_card_reads.py --quiet          # failures and the summary only
    python tools/check_card_reads.py --card MegaKnight --card Mortar
    python tools/check_card_reads.py --cards data/derived/cards-2018.json
    python tools/check_card_reads.py --plant slice_mechanic    # prove it can fail
    python tools/check_card_reads.py --all-plants

PLANTS
    A plant is only evidence when the tree is green WITHOUT it.  Each one corrupts
    an in-memory copy of the data or of the source and asserts the gate goes red:
      slice_mechanic  a thin-slice character row grows an unread column
      unread_field    a thin-slice card row carries a cards.json key card.rs has no
                      field for
      stale_gap       a KNOWN_SLICE_GAPS entry no card in the slice carries any more
      loaded_not_run  a thin-slice character row grows a column card.rs loads and
                      runs only under a calibration arm that does not ship
      blind_ledger    card.rs stops reading `charge`, so Prince's charge columns
                      must turn red without anyone editing this file
      null_block      Prince's `charge` block is null while its columns stay in
                      `raw`: a field the loader reads elsewhere, absent here
      range_row_everywhere  every non-spell projectile grows a ProjectileRange and a
                      ProjectileRadius under straight_to_range, so RANGE_ROW_ONLY calls
                      the slice's projectile gaps run and they must go stale

SHOULD A LOADED CARD CARRYING AN UNREAD MECHANIC BE REFUSED, THE WAY RAGE AND HEAL ARE?
    Recommendation, 2026-09-22: NOT as a blanket rule, and YES for a graded list.
    Nothing here changes the loader; this is the measurement the decision needs.

    Against the blanket rule, measured on the 15.535 table with this tool:
    82 of the 95 cards the engine registers carry at least one column or key the
    loader never reads.  Refusing all of them leaves 13 playable cards -- fewer
    than the 18-card thin slice, because 13 of the 18 are among them.  Archer,
    Musketeer, Wizard, Cannon, Tesla, Valkyrie and The Log all go, and with them
    every scripted-battle test that names one.  A rule that empties the catalogue
    and reddens the suite is not a fidelity improvement.

    The reason the blanket rule fails is that "unread" is not one thing.  Most of
    the 82 are unread columns with no behavioural difference for the engine to get
    wrong: a projectile's Homing on a projectile that always reaches its target,
    OnlyEnemies on a hit that only ever lands on the other team, AoeToAir /
    AoeToGround where they agree with the attacker's own two flags, IsBuilding on a
    row the loader already classes by its table.  Take those away and 39 of the 95
    are left -- cards whose unread column changes what a player would see: the
    Inferno family's damage ramp, the champions' Ability (the Golden Knight's is read
    since 2026-09-28), the Mega Knight's
    and Assassin's dash and landing hit, Mortar's MinimumRange, the Electro Giant's
    reflect, the Fisherman's pull, the Bowler's projectile knockback.

    So the graded rule: refuse a card when the mechanic it carries makes the loaded
    card a DIFFERENT card, and keep loading it when the column has nothing to
    change.  That is already the loader's own principle -- it refuses a spawn
    pathfind, a death area effect, a scripted mechanic graph and a buff with a
    column it does not run (DamageMultiplier today), each with a named
    consequence -- and the 39 are the population it has not yet been applied to.
    The evidence that running the wrong row is worse than refusing it is already in
    the tree: the Goblin Drill scored 0.0 % against a recording, with 99.6 % of its
    unit-ticks an alive mismatch, because the engine ran a row the game never put on
    the board (`card.rs`, refuse_spawn_pathfind).

    What to weigh before doing it: refusing the 39 takes the catalogue to 56, which
    is the number that matters for anything that draws decks from it, and it will
    move any measurement taken against the wider catalogue.  The grade itself has to
    live somewhere a reviewer can argue with, per column, and this file is the
    natural home -- at which point the gate can check the policy as well as report
    it: every column graded "different card" must belong to a card the engine
    REFUSES, and a card refused for a column not so graded is over-refusal.  That
    check is a follow-up, not something this pass does today.
"""

from __future__ import annotations

import argparse
import ast
import functools
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARD_RS = ROOT / "crates" / "royalesim" / "src" / "card.rs"
EXTRACT = ROOT / "tools" / "extract_cards.py"
CARDS = ROOT / "data" / "derived" / "cards.json"
REGISTER = ROOT / "data" / "derived" / "mechanic_register.json"
CALIBRATION = ROOT / "data" / "calibration.json"

# --- the two hand-written tables, and what keeps each of them honest -------------

# `norm_unit` reads these outside its dict literal: the first five in its prologue
# (damage and the area radius fall back to the projectile row, so they are computed
# before the literal is built), DeathSpawnPushback and the dash's motion after it (written
# on the 15.535 rows only, so the 2018 file does not grow the keys).  The gate refuses to run if the
# function reads a column outside the literal that this list does not name: an
# unlisted one would silently look unread.
PROLOGUE = {
    "Damage": "damage",
    "CrownTowerDamagePercent": "crown_tower_damage_percent",
    "AreaDamageRadius": "area_damage_radius_milli",
    "HitSpeed": "hit_speed_ms",
    "Projectile": "projectile",
    "DeathSpawnPushback": "death_spawn_pushback",
    # The dash's motion, written into the dash block after the literal on the 15.535 rows only
    # (JumpSpeed is also read inside the literal, into the jump block).
    "JumpSpeed": "dash.speed",
    "DashConstantTime": "dash.constant_time_ms",
    "DashLandingTime": "dash.landing_time_ms",
    # The idle invisibility (the Royal Ghost), written after the literal on the 15.535 rows whose
    # idle buff's own row sets Invisible; that buff column is read off the buff row.
    "BuffWhenNotAttacking": "idle_invisibility.buff",
    "BuffWhenNotAttackingTime": "idle_invisibility.time_ms",
    "BuffWhenNotAttackingUseAttackRange": "idle_invisibility.use_attack_range",
    "AllowAreaDmgWhenInvisible": "idle_invisibility.area_damage_when_invisible",
    # Written only where the row sets it (the Evo Royal Ghost's pair: false).
    "StartWithBuffWhenNotAttacking": "idle_invisibility.starts_hidden",
    "Invisible": "idle_invisibility",
    # The attached rider (the Ram Rider's rider, the Goblin Giant's Spear Goblins), written after
    # the literal on the 15.535 rows that set each column: the spawner block's SpawnAttach, and the
    # rider row's targeting columns, its SpawnMaxAngle (the arc its mount's riders spread over,
    # calibration rider.OFFSET_LAW) and the facing clamp SpawnAttachMaxRotation (carried and read by
    # nothing, so it stays unread here).
    "SpawnAttach": "spawner.attach",
    "TargetOnlyTroops": "target_only_troops",
    "IgnoreTargetsWithBuff": "ignore_targets_with_buff",
    "DeprioritizeTargetsWithBuff": "deprioritize_targets_with_buff",
    "SpawnAttachMaxRotation": "attach_max_rotation_deg",
    "SpawnMaxAngle": "spawn_max_angle_deg",
    # The attack selector (the Three Musketeers), written after the literal on the 15.535 rows by `attack_select`,
    # which reads these three columns of the row (and the actions and AttackSequenceList they name).
    "OnStartingAttackAction": "attack_select",
    "AttackSequenceMode": "attack_select",
    "AttackSequence": "attack_select",
    # A second periodic unit (the Super Witch's Bat), written into the spawner block after the literal on
    # the 15.535 rows only; the loader refuses a row that sets it.
    "SpawnCharacter2": "spawner.character2",
    # The Clone never copies this unit (the Goblin Drill's dig, the chess Recruits), written after the literal on
    # the 15.535 rows that set it.
    "IgnoreClone": "ignore_clone",
}

# Not a card-table column: `base_ops` is the extractor's own record of how a row was
# assembled, written into the unit record beside `raw` rather than read out of it.
NOT_A_COLUMN = {"base_ops"}

# Columns that say WHICH ROW THIS IS rather than what it does.  Nothing else belongs
# here: a column is exempted because it is not behaviour, never because the engine
# happens not to run it.
NOT_BEHAVIOUR = {
    "Name": "the row's own key; the loader looks units up by it",
    "Rarity": "read as `rarity`, and used only to pick the level ladder",
}

# What a THIN-SLICE card carries that the engine does not read: a card-table column
# name, or a dotted cards.json path.  Each is an OPEN ITEM, not an exemption.  The
# gate fails on anything not listed here, and fails again when an entry goes stale --
# no card of the slice carries it any more -- so the list cannot outlive the gap.
# The first element names the card tables that show it.
KNOWN_SLICE_GAPS = {
    "DeployDelay": (
        "both",
        "Archer, Musketeer, Minion, Skeleton, Goblin, Hog Rider. Not carried into "
        "cards.json at all. The engine staggers a summon's members by the CARD's "
        "SummonDeployDelay (`formation.DEPLOY_STAGGER`, measured) and gives every "
        "member the row's own DeployTime; what this second per-row delay does on "
        "top of that is not settled",
    ),
    "AttackFinishTime": (
        "15.535",
        "Valkyrie. The engine's attack is windup then cooldown (`combat.rs`); it has "
        "no third phase, so a hit lands and the unit is free at the same tick the "
        "cooldown says",
    ),
    "WalkingSpeedTweakPercentage": (
        "both",
        "Wizard. The engine walks every unit at its Speed column through "
        "`movement.SPEED_TO_SUBTILES_PER_TICK`; no per-row trim is applied",
    ),
    "IsBuilding": (
        "15.535",
        "Cannon, Tesla. The loader takes a row's kind from the table it came from "
        "(`source_table`), not from this column. The gate checks the two agree on "
        "every row it reaches, so this gap is inert until they disagree",
    ),
    "SightClip": (
        "2018",
        "Giant, Hog Rider. The engine's sight is one radius (`target.rs` sight "
        "range); it has no clipped sight shape",
    ),
    "SightClipSide": ("2018", "Giant, Hog Rider. The same shape the engine does not have"),
    "projectile.homing": (
        "both",
        "Archer, Musketeer, Minions, Wizard, Baby Dragon, Cannon (true) and the "
        "Arrows carrier. A troop's projectile in the engine always arrives on the "
        "entity it was fired at and a spell's always at the tap, so neither reading "
        "of the column changes anything the engine does. It is a gap all the same: "
        "84 of the 166 projectile rows in the 15.535 table ship it false",
    ),
    # The next three are LOADED since 2026-09-26: card.rs `RawProjectileObj` reads them for a
    # range projectile's hit (a row with ProjectileRange and ProjectileRadius, run only under
    # combat.RANGE_PROJECTILE = straight_to_range). No slice card's projectile is such a row, so
    # for the slice nothing runs them under any value. LOADED_NOT_RUN kept them in view while
    # to_target shipped; since the 2026-09-27 flip to straight_to_range RANGE_ROW_ONLY does, so
    # these three entries were re-read at the flip and stay.
    "projectile.only_enemies": (
        "both",
        "true on every slice projectile. The engine's projectile damages the team "
        "opposite its owner and no other, so a false here would have nothing to act "
        "on; 6 of 166 rows ship false. Loaded for range projectiles only (RANGE_ROW_ONLY)",
    ),
    "projectile.aoe_to_air": (
        "both",
        "Wizard, Baby Dragon, Arrows. The engine filters a splash by the ATTACKER's "
        "AttacksAir / AttacksGround (`combat.rs` fire -> splash), never by these two "
        "columns. The gate checks the two agree on every row it scores, so the gap "
        "is inert where they do and named per card where they do not. Loaded for range "
        "projectiles only (RANGE_ROW_ONLY)",
    ),
    "projectile.aoe_to_ground": (
        "both",
        "Wizard, Baby Dragon, Arrows, the Goblin Barrel carrier. The same splash "
        "filter and the same per-row check as aoe_to_air",
    ),
    "projectile.radius_y_milli": (
        "both",
        "The Log. The airborne row's elliptical radius. The engine's Log does no "
        "damage in the air at all: the hit is the rolling projectile the airborne "
        "one releases, and that rectangle is read from its own "
        "projectile_radius_milli / projectile_radius_y_milli",
    ),
}

# Columns card.rs LOADS that the shipped engine still does not RUN, because the code that
# runs them sits behind a calibration.json key whose shipped value is not the arm that does.
# Link 1 sees the load and would call each one read, so a card whose mechanic is loaded and
# switched off would leave the report while it still plays as the plainer card.  Each entry
# is (key, the values under which the engine runs the column); an empty tuple is a column no
# value runs yet.  The gate reads each key's value from calibration.json and refuses to run
# when one is missing: an arm it cannot read is not an arm that is off.
# targeting.DEPRIORITIZED_TARGET_BUFF's arms that read the rider row's deprioritize columns.
DEPRIORITIZE_RUNS = ("rescan_on_landing_keep_progress", "rank_last_only")

LOADED_NOT_RUN = {
    "ReflectedAttackDamage": ("combat.REFLECT_ATTACK", ("client_reflect_stun",)),
    "ReflectedAttackRadius": ("combat.REFLECT_ATTACK", ("client_reflect_stun",)),
    "ReflectedAttackBuff": ("combat.REFLECT_ATTACK", ("client_reflect_stun",)),
    "ReflectedAttackBuffDuration": ("combat.REFLECT_ATTACK", ("client_reflect_stun",)),
    # Loaded as ReflectDef::crown_tower_damage and read by nothing under either value: the
    # engine answers a hit that lands in the attacker's own pass, and a crown tower's is a shot.
    "ReflectAttackCrownTowerDamage": ("combat.REFLECT_ATTACK", ()),
    # The dash block (card.rs `DashDef`), run only under combat.DASH_ATTACK = client_dash. JumpSpeed is
    # not listed: it is the river leap's speed too, and this table is per column. DashPushBack and
    # DashLandingTime are loaded and read by nothing under either value.
    "DashDamage": ("combat.DASH_ATTACK", ("client_dash",)),
    "DashMinRange": ("combat.DASH_ATTACK", ("client_dash",)),
    "DashMaxRange": ("combat.DASH_ATTACK", ("client_dash",)),
    "DashCooldown": ("combat.DASH_ATTACK", ("client_dash",)),
    "DashRadius": ("combat.DASH_ATTACK", ("client_dash",)),
    "DashImmuneToDamageTime": ("combat.DASH_ATTACK", ("client_dash",)),
    "DashConstantTime": ("combat.DASH_ATTACK", ("client_dash",)),
    "DashPushBack": ("combat.DASH_ATTACK", ()),
    "DashLandingTime": ("combat.DASH_ATTACK", ()),
    # The damage ramp (card.rs `VariableDamageDef`, the `variable_damage` block): the Inferno Tower,
    # the Inferno Dragon and the Mighty Miner. The Monk's and the Mega Monk's two damages come with
    # no times, so their block is not written and the columns stay unread under every value.
    "VariableDamage2": ("combat.VARIABLE_DAMAGE", ("client16402_attack_progress_stages",)),
    "VariableDamage3": ("combat.VARIABLE_DAMAGE", ("client16402_attack_progress_stages",)),
    "VariableDamageTime1": ("combat.VARIABLE_DAMAGE", ("client16402_attack_progress_stages",)),
    "VariableDamageTime2": ("combat.VARIABLE_DAMAGE", ("client16402_attack_progress_stages",)),
    # The Sparky's first hit timed from its deploy end, and the recoil of its launches.
    "LoadFirstHit": ("combat.LOAD_FIRST_HIT", ("load_time_from_deploy_end",)),
    "AttackPushBack": ("knockback.ATTACK_PUSHBACK", ("ladder_away_from_target",)),
    # The Skeleton Barrel's delayed death (card.rs `CardDef::kamikaze_time_ms`), run by either drain arm.
    "KamikazeTime": ("combat.KAMIKAZE_TIME", ("flat_drain_to_zero", "flat_drain_then_expire")),
    # A death bomb's push (card.rs `CardDef::death_pushback`). Per column, so conservative: under the shipped
    # containers_ladder the gate calls it unread on every row, which is true for the Golem, the Golemite and the Giant
    # Skeleton's bomb, and false only for the Skeleton Barrel's container, which is a unit row and not a card row.
    "DeathPushBack": ("knockback.DEATH_PUSHBACK", ("every_death_bomb_ladder",)),
    # The Fisherman's hook (card.rs `SpecialDef`, the `special` block).
    "SpecialRange": ("combat.SPECIAL_HOOK", ("client_hook_drag",)),
    "SpecialMinRange": ("combat.SPECIAL_HOOK", ("client_hook_drag",)),
    "SpecialLoadTime": ("combat.SPECIAL_HOOK", ("client_hook_drag",)),
    "ProjectileSpecial": ("combat.SPECIAL_HOOK", ("client_hook_drag",)),
    # The Mortar's minimum range (card.rs `CardDef::minimum_range`).
    "MinimumRange": ("targeting.MINIMUM_RANGE", ("client16402_edge_distance",)),
    # The Battle Healer's river crossing (card.rs `CardDef::hovering`).
    "Hovering": ("pathfinding.HOVERING_WATER_RULE", ("priced_water_no_hop",)),
    # The Ram Rider's rider ranks snared troops last (card.rs `CardDef::deprioritize_buff`); not_read reads neither.
    "IgnoreTargetsWithBuff": ("targeting.DEPRIORITIZED_TARGET_BUFF", DEPRIORITIZE_RUNS),
    "DeprioritizeTargetsWithBuff": ("targeting.DEPRIORITIZED_TARGET_BUFF", DEPRIORITIZE_RUNS),
    # The Phoenix's fireball and egg, the Goblin Demolisher's blast (the card row's and its
    # kamikaze form's), and the Battle Healer's heal where she appears.
    "DeathSpawnProjectile": ("spawner.DEATH_SPAWN_PROJECTILE", ("client_projectile",)),
    "SpawnAreaObject": ("spawner.SPAWN_AREA_OBJECT_SCOPE", ("every_row",)),
    # Read straight out of the unit row's `raw` block (link 1): the Hunter's fan, the Electro
    # Wizard's second bolt and the Princess's first arrow.
    "MultipleProjectiles": ("combat.MULTIPLE_PROJECTILES", ("client_fan",)),
    "MultipleTargets": ("combat.MULTIPLE_TARGETS", ("client_bolts_per_target",)),
    "AllTargetsHit": ("combat.MULTIPLE_TARGETS", ("client_bolts_per_target",)),
    "CustomFirstProjectile": ("combat.CUSTOM_FIRST_PROJECTILE", ("client_first_of_volley",)),
    # CARDS.JSON KEYS, not card-table columns (a lower-case name is a cards.json path): what the
    # loader reads behind an arm and no character column of the row carries, so pass C's column
    # check cannot see the arm. `derived_gaps` reads these. A troop's or a building's `projectile`
    # block is the RawProjectileObj; its range columns and the pingpong time are read for a range
    # projectile (ProjectileRange and ProjectileRadius both set) and run only under
    # combat.RANGE_PROJECTILE = straight_to_range (the Hunter's pellets also under
    # combat.MULTIPLE_PROJECTILES = client_fan, which this one-key table does not name: a column
    # it calls unread may run there). A SPELL's `projectile` is its damage carrier, read as the
    # RawSpellProjectile and run under every value, so these never apply to a spell row.
    "projectile.aoe_to_air": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    "projectile.aoe_to_ground": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    "projectile.only_enemies": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    "projectile.projectile_range_milli": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    "projectile.projectile_radius_milli": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    "projectile.pushback_milli": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    "projectile.pushback_all": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    "projectile.pingpong_visual_time_ms": ("combat.RANGE_PROJECTILE", ("straight_to_range",)),
    # The card row's own deploy blow (the Mega Knight's, spells_characters Projectile) and deploy
    # area (the Electro Wizard's zap, the Ice Wizard's chill: the spells row's AreaEffectObject).
    "deploy_projectile": ("combat.DEPLOY_PROJECTILE", ("client_on_landing", "client_on_landing_action_at_2")),
    "deploy_area_effect": ("spells.DEPLOY_AREA_EFFECT", ("client_area_effect",)),
}

# THE ONE PER-ROW RULE. The LOADED_NOT_RUN keys of a troop's or a building's `projectile` block that
# combat.RANGE_PROJECTILE gates are read only for a RANGE projectile: card.rs `range_shot_of` takes a
# row whose ProjectileRange and ProjectileRadius are both above 0 (the Bowler's boulder, the Hunter's
# pellets, the Elite Archer's arrow, the Executioner's axe). On any other row (the Wizard's, the Wall
# Breakers' range 1 with no radius) nothing reads them under any value. straight_to_range ships since the
# 2026-09-27 flip, and a per-key reading would call them run on every row.
RANGE_ROW_ONLY = {
    k for k, (key, _) in LOADED_NOT_RUN.items() if key == "combat.RANGE_PROJECTILE" and k.startswith("projectile.")
}
RANGE_ROW_WHY = (
    "run only for a range projectile (ProjectileRange and ProjectileRadius both above 0, card.rs "
    "range_shot_of), and this row's is not one"
)


def is_range_row(projectile) -> bool:
    """card.rs `range_shot_of`'s test: ProjectileRange and ProjectileRadius both above 0."""
    if not isinstance(projectile, dict):
        return False
    return all(
        isinstance(projectile.get(k), int) and not isinstance(projectile.get(k), bool) and projectile[k] > 0
        for k in ("projectile_range_milli", "projectile_radius_milli")
    )


def is_cards_json_path(name: str) -> bool:
    """A LOADED_NOT_RUN name that is a cards.json key (`projectile.aoe_to_air`,
    `deploy_projectile`) rather than a card-table column (`MinimumRange`)."""
    return name == name.lower()

# Mechanic families the register names whose fields never reach a CHARACTER row, so
# the derived read set cannot see them and pass D would report a family the engine
# does run.  An entry here says the engine reads the family's main columns, and
# names the ones it still does not: a family is a coarser thing than a field, and
# pass D cannot split one.
REGISTER_FAMILIES_READ = {
    "summoner_hero": (
        "SummonCharacter, SummonNumber, SummonRadius, SummonWidth, SummonDeployDelay, "
        "SummonDeployDelaySecond, SummonCharacterSecond and SummonCharacterSecondCount are CARD-row "
        "columns; the extractor resolves them into `summon_character`, `count`, the summon_* fields "
        "and `second_summon`, and card.rs reads all of those (formation.rs lays the summon out); "
        "SummonCharactersOffsetsX / OffsetsY are read as `summon_members`. "
        "Still unread inside this family: SummonNumberListOnly, SummonCharacterLevelIndex"
    ),
    "variant_select": (
        "The Spirit Empress's Options: AvailableManaTrigger, SpellData and UseProjectedTimeSummon are SPELL-row "
        "fields the extractor carries as `spell.variant`, and card.rs `convert_variant` reads all of them. Still "
        "unread inside this family, a KNOWN GAP: PrecastPendingTime, carried as `precast_pending_ms` and read by no "
        "arm of calibration match.VARIANT_ELIXIR_MOMENT (the shipped `command` chooses at the play, measured)"
    ),
}

# cards.json keys that are PROVENANCE, not mechanics: they say where a number came
# from, or they repeat a number the loader already reads through another key.  Each
# says which.  An entry no row carries is stale and fails the gate, so the list
# cannot outlive the schema.  Everything NOT listed here and not consumed is
# reported as an unread mechanic.
PROVENANCE = {
    "raw": "the row's own card-table columns, kept for exactly this gate",
    "overlays": "which overlay files contributed to the row",
    "list_columns": "the row's continued (list) columns, held apart from the stat fields",
    "base_ops": "how the row was assembled from its base row",
    "defaults_applied": "which of this row's values the extractor supplied because the column was blank",
    "damage_source": "names the table row `damage` was taken from",
    "summon_character": "names the character row whose stats are already flattened into this card row",
    "tower": "marks the two crown-tower rows; the loader finds them by name",
    "no_deploy_size_raw": "the W and H columns behind `no_deploy_size_tiles`, which the loader does read",
    "no_deploy_size_provenance": "where those two columns came from",
    "not_visible": "roster visibility, not behaviour: the row is not offered in the normal card list",
    "level_scaling.rarity": "the ladder the extractor chose, written out for a reader",
    "level_scaling.ladder_rarity": "the same choice written from the other side: which rarity's ladder was used",
    "level_scaling.level_count": "how many levels that ladder has; the loader takes it from cards.json `rarities`",
    "level_scaling.relative_level": "the same, from `rarities`",
    "level_scaling.applies_to": "which stats the ladder scales; the loader scales hitpoints and damage",
    "level_scaling.rounding": "prose: how the multiplication rounds",
    "action_graph.roots": "the action names the row reaches; the loader reads `mechanic`, `class_types` and `spawns`",
    "projectile.name": "names the projectile row",
    "projectile.crown_tower_damage_percent": "the extractor lifts the effective percent into the row's own "
    "`crown_tower_damage_percent`, which the loader reads",
    "projectile.crown_tower_damage_percent_raw": "the column behind that percent",
    "spell.spawn": "the resolved spawn block: a copy of the carrier projectile's spawn_character / "
    "spawn_character_count / spawn_character_deploy_time_ms, which the loader reads off the carrier",
}

# --- link 1: the cards.json keys card.rs consumes -------------------------------

STRUCT_RE = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)\s*\{", re.M)
FIELD_RE = re.compile(r"^\s{4}(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:\s*(.+?),\s*$", re.M)
JSON_KEY_RE = re.compile(r'\.get\("([a-z_0-9]+)"\)')
# A card-table column read straight out of a unit row's `raw` block: a capitalised string
# literal passed alone to a call (`r.get("AllTargetsHit")`, `raw_count("MultipleProjectiles")`).
# Only a literal that names a column some row's `raw` carries counts (`Consumed.raw_reads`).
RAW_COLUMN_RE = re.compile(r'\(\s*"([A-Z][A-Za-z0-9_]*)"\s*\)')

# Which cards.json block each `Raw*` struct deserialises.  A block path is the one
# the report and KNOWN_SLICE_GAPS use: "" is a card / tower / unit row itself.
BLOCKS = {
    "": "RawCard",
    "charge": "RawCharge",
    "jump": "RawJump",
    "dash": "RawDash",
    "spawner": "RawSpawner",
    "death_spawn": "RawDeathSpawn",
    "second_summon": "RawSecondSummon",
    "spawn_pathfind": "RawSpawnPathfind",
    "buff_on_damage": "RawBuffOnDamage",
    "reflected_attack": "RawReflectedAttack",
    "attack_select": "RawAttackSelect",
    "variable_damage": "RawVariableDamage",
    "special": "RawSpecial",
    "action_graph": "RawActionGraph",
    "transform_at_hp": "RawTransform",
    "parry": "RawParry",
    "level_scaling": "RawLevelScaling",
    "projectile": "RawProjectileObj",
    "spell": "RawSpell",
    "spell.first_projectile": "RawSpellProjectile",
    "spell.area_effect_object": "RawAreaEffect",
}
# Blocks the loader reads through a different struct depending on where they hang.
# A troop's `projectile` is a RawProjectileObj (speed, damage, radius, a buff); the
# same block under a SPELL is the richer RawSpellProjectile.  The gate scores a
# troop's projectile against the narrow struct, which is the one that runs.


class Consumed:
    """The cards.json keys `card.rs` deserialises, and which of them it then reads."""

    def __init__(self, text: str):
        self.structs: dict[str, list[str]] = {}
        spans = []
        for m in STRUCT_RE.finditer(text):
            i = text.index("{", m.start())
            depth = 0
            for j in range(i, len(text)):
                if text[j] == "{":
                    depth += 1
                elif text[j] == "}":
                    depth -= 1
                    if depth == 0:
                        break
            spans.append((m.start(), j + 1))
            if m.group(1).startswith("Raw"):
                self.structs[m.group(1)] = [f.group(1) for f in FIELD_RE.finditer(text[i + 1 : j])]
        # The source with every struct declaration cut out: what is left is code.
        rest = text
        for a, b in sorted(spans, reverse=True):
            rest = rest[:a] + rest[b:]
        self.rest = rest
        self.literal_keys = set(JSON_KEY_RE.findall(rest))
        # Capitalised single-argument literals: candidate `raw` columns, narrowed to the ones a
        # row's `raw` actually carries by `raw_reads`. Only meaningful when the loader reads the
        # `raw` block at all.
        self.raw_literals = set(RAW_COLUMN_RE.findall(rest)) if "raw" in self.literal_keys else set()
        # A field name declared by more than one struct cannot be attributed: an
        # access to `.name` could be either one's.  Named in the report.
        owners: dict[str, list[str]] = defaultdict(list)
        for s, fs in self.structs.items():
            for f in fs:
                owners[f].append(s)
        self.ambiguous = {f: ss for f, ss in owners.items() if len(ss) > 1}
        self.accessed = {f for f in owners if re.search(r"\." + re.escape(f) + r"\b", rest)}
        self.declared_unread = sorted(f for f in owners if f not in self.accessed)
        # Card-table column -> why the shipped engine does not run it, for the columns this
        # source loads behind a calibration arm (LOADED_NOT_RUN, read by `switched_off`).
        # `load` fills it from calibration.json; empty until then.
        self.off: dict[str, str] = {}

    def raw_reads(self, units: dict) -> set[str]:
        """The card-table columns the loader reads straight out of a unit row's `raw` block:
        the capitalised single-argument literals of the source that name a column some row's
        `raw` carries. NOT_BEHAVIOUR columns (Name) are left out: they say which row it is."""
        carried: set[str] = set()
        for rec in units.values():
            carried |= set(rec.get("raw") or {})
        return {c for c in self.raw_literals if c in carried and c not in NOT_BEHAVIOUR}

    def keys(self, block: str, kind: str = "troop") -> set[str]:
        """Every cards.json key the loader consumes in `block` (a BLOCKS path).

        `kind` is the row's cards.json `kind`, because one block is read through two
        different structs: a troop's `projectile` is a RawProjectileObj (speed,
        damage, radius and a buff -- four numbers), while the same block under a
        SPELL card is its damage carrier and is read as the much wider
        RawSpellProjectile. Scoring a troop against the wide struct would call its
        unread projectile columns read."""
        st = BLOCKS.get(block)
        if block == "projectile" and kind == "spell":
            st = "RawSpellProjectile"
        if st is None or st not in self.structs:
            # A block whose struct card.rs does not declare (yet) reads nothing.
            return set()
        out = {f for f in self.structs[st] if f in self.accessed}
        if block == "":
            out |= self.literal_keys
        return out

    def reads_path(self, path: str, kind: str = "troop") -> bool:
        """Is the cards.json field at dotted `path` (e.g. charge.damage_special) read?

        Every block on the way down has to be read too: a `charge` block the loader
        has no field for takes `charge.damage_special` with it, however faithfully
        RawCharge still declares the leaf."""
        parts = path.split(".")
        for i in range(len(parts)):
            block = ".".join(parts[:i])
            if block and block not in BLOCKS:
                return False
            if parts[i] not in self.keys(block, kind):
                return False
        return True


def switched_off(calibration: dict, forced: dict[str, str] | None = None) -> dict[str, str]:
    """LOADED_NOT_RUN read against calibration.json: column -> why the shipped engine does
    not run it.  A column whose key's shipped value runs it is left out.  `forced` gives a
    key a value in place of the file's (the loaded_not_run plant)."""
    out: dict[str, str] = {}
    for col, (key, runs) in LOADED_NOT_RUN.items():
        value = (forced or {}).get(key)
        if value is None:
            node = calibration
            for part in key.split("."):
                node = node.get(part) if isinstance(node, dict) else None
            value = node.get("value") if isinstance(node, dict) else None
        if not isinstance(value, str):
            raise SystemExit(
                f"LOADED_NOT_RUN says {col} runs only under {key}, and data/calibration.json has "
                "no value for that key. A missing key is not an arm that is off: add the key "
                "(the engine refuses to load without it too), or retire the entry"
            )
        if value not in runs:
            where = f"only under {key} = {' / '.join(runs)}" if runs else f"under no value of {key}"
            out[col] = f"run {where}; calibration.json ships {value}"
    return out


# --- link 2: which card-table column becomes which cards.json field --------------


def column_map(text: str) -> tuple[dict[str, set[str]], list[str]]:
    """Column -> the cards.json field paths `norm_unit` writes it into.

    Returns the map and the list of columns read in the prologue, so the caller can
    check PROLOGUE still covers it."""
    fn = next(n for n in ast.parse(text).body if isinstance(n, ast.FunctionDef) and n.name == "norm_unit")

    def cols(node) -> set[str]:
        out = set()
        for n in ast.walk(node):
            if isinstance(n, ast.Subscript) and isinstance(n.value, ast.Name) and n.value.id == "c":
                if isinstance(n.slice, ast.Constant) and isinstance(n.slice.value, str):
                    out.add(n.slice.value)
            elif isinstance(n, ast.Call):
                f = n.func
                on_c = isinstance(f, ast.Attribute) and isinstance(f.value, ast.Name) and f.value.id == "c"
                getter = on_c and f.attr == "get"
                flagger = isinstance(f, ast.Name) and f.id == "flag" and len(n.args) == 2
                if getter and n.args and isinstance(n.args[0], ast.Constant):
                    out.add(n.args[0].value)
                elif flagger and isinstance(n.args[1], ast.Constant):
                    out.add(n.args[1].value)
        return out

    out: dict[str, set[str]] = defaultdict(set)

    def literal(d: ast.Dict, prefix: str) -> None:
        for k, v in zip(d.keys, d.values, strict=False):
            if not isinstance(k, ast.Constant) or not isinstance(k.value, str):
                continue
            path = prefix + k.value
            subs = [n for n in ast.walk(v) if isinstance(n, ast.Dict)]
            for sd in subs:
                literal(sd, path + ".")
            # A block written as `None if <col is None> else {...}` reads its guard
            # column for the block itself, which is where the loader sees it.
            for c in cols(v):
                out[c].add(path)

    def is_the_record(n) -> bool:
        return isinstance(n, ast.Assign) and isinstance(n.targets[0], ast.Name) and n.targets[0].id == "u"

    assign = next(n for n in ast.walk(fn) if is_the_record(n))
    literal(assign.value, "")
    prologue = sorted({c for st in fn.body if st is not assign for c in cols(st)} - set(out) - NOT_A_COLUMN)
    for c, path in PROLOGUE.items():
        out[c].add(path)
    return dict(out), prologue


def filtered_out(colmap: dict[str, set[str]], units: dict) -> list[str]:
    """Columns the extractor READS that appear in no `raw` block at all.

    `raw` is the character row minus extract_cards.py's COSMETIC name filter, so a
    mechanic column whose NAME matches that filter never reaches this gate. The
    filter cannot be run here to size the hole -- it has already run, and what it
    removed is gone. What CAN be counted is the columns the extractor demonstrably
    reads and `raw` does not carry: each one is a column the filter removed while
    the extractor went on using it. That count is a LOWER BOUND on the blind spot,
    not its size."""
    seen = set()
    for rec in units.values():
        seen |= set(rec.get("raw") or {})
    return sorted(c for c in colmap if c not in seen and c not in NOT_A_COLUMN)


@functools.lru_cache(maxsize=1)
def _register_module():
    """tools/mechanic_register.py, which imports without any card-table bundle (its
    own check for one lives in `main`). None when it will not load at all."""
    try:
        sys.path.insert(0, str(ROOT / "tools"))
        import mechanic_register as mr

        return mr
    except Exception:
        return None


@functools.cache
def family_of(column: str) -> str:
    """The mechanic family a column's NAME belongs to (mechanic_register.py's own
    table). "?" when that module will not load."""
    mr = _register_module()
    return mr.family_of(column) if mr is not None else "?"


# --- the card graph inside cards.json -------------------------------------------


def reached_units(card: dict, units: dict) -> list[str]:
    """Every `units` row the engine loads for this card: its own character row and
    the rows its spawner, death spawn, second summon and spell release, transitively
    (a death spawn that itself spawns is refused by the loader, but the gate walks it
    anyway so a future chain is not invisible)."""
    out: list[str] = []
    seen: set[str] = set()

    def add(name) -> None:
        if not isinstance(name, str) or name not in units or name in seen:
            return
        seen.add(name)
        out.append(name)
        walk(units[name])

    def walk(rec: dict) -> None:
        add(rec.get("summon_character"))
        for blk in ("spawner", "death_spawn", "second_summon"):
            b = rec.get(blk)
            if isinstance(b, dict):
                add(b.get("character"))
        # a deploy's members at explicit offsets (the Three Musketeers), each its own row
        for m in rec.get("summon_members") or []:
            if isinstance(m, dict):
                add(m.get("character"))
        sp = rec.get("spell")
        if isinstance(sp, dict):
            for key in ("first_projectile", "spawn"):
                p = sp.get(key)
                if isinstance(p, dict):
                    add(p.get("spawn_character"))
                    add(p.get("character"))
                    q = p.get("spawn_projectile")
                    if isinstance(q, dict):
                        add(q.get("spawn_character"))

    walk(card)
    return out


def carries(value) -> bool:
    """Does this cards.json value CARRY a mechanic, as opposed to saying it is absent?
    A null, a false flag, a zero and an empty block all say absent."""
    return value not in (None, False, 0, "", [], {})


# --- the gate --------------------------------------------------------------------


class Result:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.notes: list[str] = []
        self.lines: list[str] = []
        self.per_card: dict[str, dict] = {}
        self.slice_gaps_seen: set[str] = set()

    def fail(self, msg: str) -> None:
        self.failures.append(msg)

    def say(self, msg: str = "") -> None:
        self.lines.append(msg)


def loaded_catalogue(names: list[str]) -> tuple[set[str] | None, str]:
    """The cards of `names` (the file's card rows) the engine actually loads, from the built extension module.

    The default catalogue (`Battle(None, ...)`) holds every card row that loads, the Mirror and the cards that travel
    under ground included; so that a card left out of it one day is still counted, every card row the default does
    not hold is offered to the engine by name, alone, and kept when a catalogue that names it builds. The engine
    refuses a card it does not load with a ValueError that names the reason; any other error is not an answer about
    the card, and skips the whole pass loudly."""
    try:
        import royalesim

        slots = [[0, 1, 2], [0, 1, 2]]
        rows = json.loads(royalesim.Battle(None, slots).catalogue_json())
        loaded = {r[0] for r in rows}
        named = set()
        for n in names:
            if n in loaded:
                continue
            try:
                royalesim.Battle([n], slots)
            except ValueError:
                continue
            named.add(n)
        how = f"royalesim.Battle catalogue ({len(rows)} cards by default"
        how += f", and {len(named)} more by name: {', '.join(sorted(named))})" if named else ", none more by name)"
        return loaded | named, how
    except Exception as e:
        why = f"{type(e).__name__}: {e}"
        return None, f"SKIPPED the engine's own catalogue ({why}); every card in the file is scored instead"


def check(
    doc: dict, consumed: Consumed, colmap: dict[str, set[str]], *, only: list[str], register: dict | None
) -> Result:
    r = Result()
    units = doc["units"]
    slice_names = set(doc.get("thin_slice") or [])
    cards = {c["name"]: c for c in doc["cards"]}
    loaded, how = loaded_catalogue(list(cards))
    if loaded is None:
        r.notes.append(how)
        loaded = set(cards)
    scored = sorted(n for n in loaded if n in cards and (not only or n in only))

    r.say(
        f"cards: {doc.get('version')}   {len(doc['cards'])} card rows, "
        f"{len(units)} unit rows, thin slice {len(slice_names)}"
    )
    r.say(f"loaded set: {how}")
    r.say()

    # --- A. the consumed set ---
    r.say("A. FIELDS card.rs CONSUMES (from crates/royalesim/src/card.rs)")
    for block in sorted(BLOCKS):
        ks = consumed.keys(block)
        r.say(f"   {'card row' if block == '' else block:28s} {len(ks):3d}  {' '.join(sorted(ks))}")
    if consumed.declared_unread:
        r.fail(
            "card.rs declares serde fields nothing reads (serde fills them in and they go nowhere): "
            + ", ".join(consumed.declared_unread)
        )
    if consumed.ambiguous:
        shared = " ".join(sorted(consumed.ambiguous))
        r.say(f"   field names shared by more than one Raw* struct (an access cannot be attributed): {shared}")
    raw_read = consumed.raw_reads(units)
    r.say(f"   {'unit row raw block':28s} {len(raw_read):3d}  {' '.join(sorted(raw_read))}")
    r.say()

    # --- B. schema drift, both ways ---
    r.say("B. SCHEMA DRIFT")
    rows = doc["cards"] + doc["towers"] + list(units.values())
    present: dict[str, set[str]] = defaultdict(set)
    for rec in rows:
        for k, v in rec.items():
            present[""].add(k)
            if isinstance(v, dict) and k in BLOCKS:
                present[k] |= set(v)
            if k == "spell" and isinstance(v, dict):
                for sub in ("first_projectile", "area_effect_object"):
                    if isinstance(v.get(sub), dict):
                        present[f"spell.{sub}"] |= set(v[sub])
    for block in sorted(BLOCKS):
        if block not in present:
            continue
        extra = sorted(present[block] - consumed.keys(block))
        absent = sorted(consumed.keys(block) - present[block])
        label = "card row" if block == "" else block
        if extra:
            r.say(f"   carried, not consumed  {label:28s} {' '.join(extra)}")
        if absent:
            r.say(f"   consumed, absent       {label:28s} {' '.join(absent)}")
    # A key the loader needs and the file does not have: `name` and `kind` have no
    # serde default, so their absence is a hard failure rather than a note.
    for req in ("name", "kind"):
        if req not in present[""]:
            r.fail(f"cards.json card rows carry no `{req}`; card.rs cannot deserialise them")
    r.say()

    # --- C. unread mechanics per card ---
    r.say("C. MECHANICS CARRIED BUT UNREAD, per card")
    vintage = "2018" if "2018" in str(doc.get("version", "")) else "15.535"
    provenance_seen: set[str] = set()

    # A cards.json key is not a card-table column, so `family_of` (which matches
    # COLUMN names) would answer "other" -- itself a real family name, and so a
    # claim rather than a shrug.  Name the family only when the key can be traced
    # back to a column through the extractor's own map.
    by_path: dict[str, set[str]] = defaultdict(set)
    for col, paths in colmap.items():
        for path in paths:
            by_path[path].add(col)

    def family_of_path(path: str) -> str:
        fams = {family_of(c) for c in by_path.get(path, ())}
        return "/".join(sorted(fams)) if fams else "-"

    def derived_gaps(rec: dict) -> list[tuple[str, str, str]]:
        """(key, what, family) for every cards.json key on `rec` that carries a value
        and that no `Raw*` field reads. PROVENANCE keys are skipped and counted."""
        out = []
        kind = rec.get("kind", "troop")

        def off(path: str) -> str | None:
            # LOADED_NOT_RUN's cards.json keys; a spell's projectile block is its damage carrier,
            # read through another struct and run under every value (the table's note).
            if kind == "spell" and path.startswith("projectile."):
                return None
            why = consumed.off.get(path)
            if why is None and path in RANGE_ROW_ONLY and not is_range_row(rec.get("projectile")):
                return RANGE_ROW_WHY
            return why

        for k, v in rec.items():
            if k in PROVENANCE:
                provenance_seen.add(k)
            elif carries(v) and k not in consumed.keys("", kind):
                out.append((k, f"cards.json `{k}`", family_of_path(k)))
            elif carries(v) and off(k):
                out.append((k, f"cards.json `{k}` (loaded, not run: {off(k)})", family_of_path(k)))
            if isinstance(v, dict) and k in BLOCKS and k in consumed.keys("", kind):
                for k2, v2 in v.items():
                    if f"{k}.{k2}" in PROVENANCE:
                        provenance_seen.add(f"{k}.{k2}")
                    elif carries(v2) and k2 not in consumed.keys(k, kind):
                        out.append((f"{k}.{k2}", f"cards.json `{k}.{k2}`", family_of_path(f"{k}.{k2}")))
                    elif carries(v2) and off(f"{k}.{k2}"):
                        path = f"{k}.{k2}"
                        out.append((path, f"cards.json `{path}` (loaded, not run: {off(path)})", family_of_path(path)))
        return out

    def on_row(rec: dict, path: str) -> bool:
        """Whether the dotted cards.json `path` holds a value on `rec`: every block on the
        way a dict, the last key present and not null (a block written as null is where a
        guard column's value did not go)."""
        cur = rec
        for part in path.split("."):
            if not isinstance(cur, dict) or part not in cur:
                return False
            cur = cur[part]
        return cur is not None

    def column_gaps(rec: dict) -> list[tuple[str, str, str]]:
        """(key, what, family) for every card-table column on `rec` the loader never
        reads."""
        out = []
        kind = rec.get("kind", "troop")
        for col in rec.get("raw") or {}:
            if col in NOT_BEHAVIOUR:
                continue
            if col in raw_read:
                # Read out of `raw` itself (link 1), so there is no cards.json field to find on
                # the row: loaded, and unread only while its calibration arm is off.
                if col in consumed.off:
                    out.append((col, f"{col} (read from `raw`, not run: {consumed.off[col]})", family_of(col)))
                continue
            paths = colmap.get(col)
            # Read only where the field is ON THIS ROW: a column the extractor writes into a block
            # that is null here (the Golden Knight's DashDamage, carried as `triggered_dash` while
            # its `dash` is null) reaches no loader field, however well the loader reads that path
            # on other rows.
            here = [p for p in paths or () if on_row(rec, p)]
            if not paths:
                why = "not carried into cards.json"
            elif not here:
                why = "its field " + "/".join(sorted(paths)) + " is not on this row"
            elif not any(consumed.reads_path(p, kind) for p in here):
                why = "carried as " + "/".join(sorted(here)) + ", unread"
            elif col in consumed.off:
                # Loaded, and still unread while its calibration arm is off (LOADED_NOT_RUN).
                why = "loaded as " + "/".join(sorted(here)) + ", not run: " + consumed.off[col]
            else:
                continue
            out.append((col, f"{col} ({why})", family_of(col)))
        return out

    for name in scored:
        card = cards[name]
        own = card.get("summon_character")
        reached = reached_units(card, units)
        found: list[tuple[str, str, str, str]] = []  # (where, key, what, family)
        # The card row IS its summon character's row plus the card-level columns, so
        # the character row is scored for its `raw` columns (which the card row does
        # not carry) and not a second time for its derived keys.
        for key, what, fam in derived_gaps(card) + (column_gaps(units[own]) if own in units else []):
            found.append((name, key, what, fam))
        for u in reached:
            if u == own:
                continue
            for key, what, fam in derived_gaps(units[u]) + column_gaps(units[u]):
                found.append((f"{name}/{u}", key, what, fam))
        # Two checks that turn an "unread column" into a measurable consequence.
        # `own` is skipped: the card row already IS that row's stats, so scoring both
        # says the same thing twice.
        for where, rec in [(name, card)] + [(f"{name}/{u}", units[u]) for u in reached if u != own]:
            # The kind a row is loaded as must agree with its own IsBuilding column.
            if (rec.get("raw") or {}).get("IsBuilding") and rec.get("source_table") not in (None, "buildings"):
                tbl = rec.get("source_table")
                r.fail(f"{where}: the row says IsBuilding and the loader takes its kind from source_table={tbl!r}")
            # The engine filters a splash by the ATTACKER's AttacksAir / AttacksGround,
            # so a projectile whose own AoeToAir / AoeToGround disagree splashes the
            # wrong layers. Reported per card; only a thin-slice disagreement fails.
            p = rec.get("projectile")
            splashes = rec.get("kind") != "spell" and isinstance(p, dict) and carries(p.get("radius_milli"))
            if splashes and (p["aoe_to_air"], p["aoe_to_ground"]) != (rec["attacks_air"], rec["attacks_ground"]):
                msg = (
                    f"{where}: the splash of {p['name']} is AoeToAir={p['aoe_to_air']} "
                    f"AoeToGround={p['aoe_to_ground']} and the engine filters it by the row's "
                    f"AttacksAir={rec['attacks_air']} AttacksGround={rec['attacks_ground']}, "
                    "so it touches the wrong layers"
                )
                if name in slice_names:
                    r.fail(msg)
                else:
                    r.say("   " + msg)
        if not found:
            continue
        fams = sorted({fam for _, _, _, fam in found})
        r.per_card[name] = {
            "keys": sorted({key for _, key, _, _ in found}),
            "families": fams,
            "slice": name in slice_names,
        }
        tag = "SLICE" if name in slice_names else "     "
        r.say(f"   {tag} {name:26s} {', '.join(fams)}")
        for where, _key, what, fam in found:
            r.say(f"            {where:34s} {what}   [{fam}]")
        if name in slice_names:
            for _, key, what, _ in found:
                if key in KNOWN_SLICE_GAPS:
                    r.slice_gaps_seen.add(key)
                else:
                    r.fail(f"{name} is a THIN-SLICE card and carries {what}: the engine loads it and never reads it")
    if not only:
        for col, (vin, _why) in sorted(KNOWN_SLICE_GAPS.items()):
            listed = vin in (vintage, "both")
            if listed and col not in r.slice_gaps_seen:
                r.fail(
                    f"KNOWN_SLICE_GAPS names {col} for this table, and no thin-slice card "
                    "carries it any more: retire the entry"
                )
            if not listed and col in r.slice_gaps_seen:
                r.fail(
                    f"KNOWN_SLICE_GAPS says {col} belongs to the {vin} table and the "
                    f"{vintage} table's slice carries it too: widen the entry"
                )
    # PROVENANCE is scored against the WHOLE file, not just the cards scored, so a
    # key that only ever appears on a tower or on a unit nothing loads still counts.
    in_file: set[str] = set()
    for rec in rows:
        in_file |= set(rec)
        for k, v in rec.items():
            if isinstance(v, dict):
                in_file |= {f"{k}.{k2}" for k2 in v}
    unseen = sorted(set(PROVENANCE) - in_file)
    if unseen:
        r.say(f"   PROVENANCE names keys this table does not have: {' '.join(unseen)} (another vintage may)")
    gone = filtered_out(colmap, units)
    r.notes.append(
        f"{len(gone)} card-table columns the extractor reads appear in no `raw` block "
        f"({', '.join(gone) or 'none'}): extract_cards.py's COSMETIC name filter removed them from the "
        "passthrough. A MECHANIC column whose name that filter matches is invisible to pass C the same "
        "way, so this is a lower bound on the blind spot, not its size"
    )
    r.say()

    # --- D. the mechanic register, when the file is there ---
    r.say("D. MECHANIC REGISTER cross-check (per card, every object it reaches)")
    if register is None:
        r.notes.append(
            f"SKIPPED the register pass: {REGISTER} is not there "
            "(it is generated and gitignored; `python tools/mechanic_register.py` rebuilds it "
            "from the card-table bundle). A skip is not a pass: pass C sees a card's own columns "
            "only, never its projectiles, area effects, buffs or scripted actions"
        )
        r.say("   SKIPPED -- see the notes")
    else:
        read_families = set()
        for col, paths in colmap.items():
            # A column loaded behind an arm that is off does not make its family read.
            if col not in consumed.off and any(consumed.reads_path(p) for p in paths):
                read_families.add(family_of(col))
        read_families |= {family_of(c) for c in raw_read if c not in consumed.off}
        read_families |= set(REGISTER_FAMILIES_READ)
        r.say(f"   families the read columns fall into: {' '.join(sorted(read_families))}")
        for fam, note in sorted(REGISTER_FAMILIES_READ.items()):
            r.say(f"   {fam}: {note}")
        for name in scored:
            entry = register["cards"].get(name)
            if entry is None:
                continue
            gaps = {f: fl for f, fl in entry["families"].items() if f not in read_families and f != "other"}
            if not gaps:
                continue
            r.per_card.setdefault(name, {"keys": [], "families": [], "slice": name in slice_names})
            r.per_card[name]["register_families"] = sorted(gaps)
            tag = "SLICE" if name in slice_names else "     "
            r.say(f"   {tag} {name:26s} {', '.join(sorted(gaps))}")
        r.notes.append(
            "the register pass REPORTS and never fails: a family whose name matches a column the "
            "engine reads is called read, and a family may hold both read and unread fields"
        )
    return r


# --- plants ----------------------------------------------------------------------


def plant_slice_mechanic(doc, consumed, colmap):
    # Any column the engine does not read will do. It was ReflectedAttackDamage until
    # card.rs began loading the reflect (combat.REFLECT_ATTACK, 2026-09-25), then the Inferno
    # ramp's VariableDamage2 until the extractor carried it into `variable_damage` (2026-09-26).
    # A champion's Ability is carried into cards.json for the Golden Knight alone (`champion_dash_chain`, as the
    # card's `ability`); the Knight's row has none, so an Ability planted on it is read by nothing.
    doc["units"]["Knight"]["raw"]["Ability"] = "KnightAbility"
    return doc, consumed, colmap


def plant_unread_field(doc, consumed, colmap):
    # A cards.json key no Raw* struct declares. It was minimum_range_milli until card.rs began
    # reading it (targeting.MINIMUM_RANGE, 2026-09-26); `attached_character` is written on
    # every unit row and read by nothing.
    for c in doc["cards"]:
        if c["name"] == "Giant":
            c["attached_character"] = "Knight"
    return doc, consumed, colmap


def plant_stale_gap(doc, consumed, colmap):
    for rec in doc["units"].values():
        (rec.get("raw") or {}).pop("WalkingSpeedTweakPercentage", None)
    return doc, consumed, colmap


def plant_blind_ledger(doc, consumed, colmap):
    consumed.structs["RawCard"] = [f for f in consumed.structs["RawCard"] if f != "charge"]
    consumed.accessed.discard("charge")
    return doc, consumed, colmap


def plant_null_block(doc, consumed, colmap):
    doc["units"]["Prince"]["charge"] = None
    for c in doc["cards"]:
        if c["name"] == "Prince":
            c["charge"] = None
    return doc, consumed, colmap


def plant_loaded_not_run(doc, consumed, colmap):
    # A column card.rs LOADS and runs only under an arm that does not ship: link 1 alone
    # calls it read, and LOADED_NOT_RUN must not. The arm is forced to its old value here,
    # so the plant lands whatever calibration.json ships.
    # The Knight carries the block as well, so the field is on its row (the null_block check passes)
    # and only the arm can call the column unread.
    doc["units"]["Knight"]["raw"]["ReflectedAttackDamage"] = 120
    doc["units"]["Knight"]["reflected_attack"] = dict(doc["units"]["ElectroGiant"]["reflected_attack"])
    calibration = json.loads(CALIBRATION.read_text(encoding="utf-8"))
    consumed.off = switched_off(calibration, forced={"combat.REFLECT_ATTACK": "not_read"})
    return doc, consumed, colmap


def plant_range_row_everywhere(doc, consumed, colmap):
    # RANGE_ROW_ONLY must be what keeps the slice's projectile gaps in view under straight_to_range: every
    # non-spell projectile grows a ProjectileRange and a ProjectileRadius, so every row is a range row, with
    # the arm forced on so the plant lands whatever calibration.json ships. The three projectile gaps of
    # KNOWN_SLICE_GAPS must then go stale.
    for rec in [*doc["cards"], *doc["units"].values()]:
        p = rec.get("projectile")
        if rec.get("kind") != "spell" and isinstance(p, dict):
            p["projectile_range_milli"] = 5000
            p["projectile_radius_milli"] = 500
    calibration = json.loads(CALIBRATION.read_text(encoding="utf-8"))
    consumed.off = switched_off(calibration, forced={"combat.RANGE_PROJECTILE": "straight_to_range"})
    return doc, consumed, colmap


PLANTS = {
    "slice_mechanic": plant_slice_mechanic,
    "unread_field": plant_unread_field,
    "stale_gap": plant_stale_gap,
    "blind_ledger": plant_blind_ledger,
    "null_block": plant_null_block,
    "loaded_not_run": plant_loaded_not_run,
    "range_row_everywhere": plant_range_row_everywhere,
}


def load(cards_path: Path | str) -> tuple[dict, Consumed, dict[str, set[str]], dict | None]:
    doc = json.loads(Path(cards_path).read_text(encoding="utf-8"))
    consumed = Consumed(CARD_RS.read_text(encoding="utf-8"))
    consumed.off = switched_off(json.loads(CALIBRATION.read_text(encoding="utf-8")))
    colmap, prologue = column_map(EXTRACT.read_text(encoding="utf-8"))
    missed = [c for c in prologue if c not in PROLOGUE]
    if missed:
        raise SystemExit(
            f"tools/extract_cards.py `norm_unit` reads {missed} outside its dict literal and "
            "check_card_reads.py's PROLOGUE does not name them: add each one with the cards.json "
            "field it becomes, or this gate would call those columns unread"
        )
    register = json.loads(REGISTER.read_text(encoding="utf-8")) if REGISTER.exists() else None
    return doc, consumed, colmap, register


def run(cards_path: Path | str, *, only=(), plant: str | None = None) -> Result:
    doc, consumed, colmap, register = load(cards_path)
    if plant:
        doc, consumed, colmap = PLANTS[plant](doc, consumed, colmap)
    return check(doc, consumed, colmap, only=list(only), register=register)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cards", type=Path, default=CARDS)
    ap.add_argument("--card", action="append", default=[], help="score these cards only")
    ap.add_argument("--quiet", action="store_true", help="failures, notes and the summary only")
    ap.add_argument("--plant", choices=sorted(PLANTS))
    ap.add_argument("--all-plants", action="store_true")
    a = ap.parse_args()
    if not a.cards.exists():
        print(f"missing {a.cards}: generate it first (tools/extract_cards.py)", file=sys.stderr)
        return 2

    if a.all_plants or a.plant:
        base = run(a.cards, only=a.card)
        if base.failures:
            print("INCONCLUSIVE: the gate is already red without any plant:", file=sys.stderr)
            for f in base.failures:
                print(f"  {f}", file=sys.stderr)
            return 1
        names = sorted(PLANTS) if a.all_plants else [a.plant]
        ok = True
        for n in names:
            got = run(a.cards, only=a.card, plant=n)
            landed = bool(got.failures)
            ok &= landed
            print(f"PLANT {n:16s} {'LANDED (gate red)' if landed else 'DID NOT LAND -- the gate cannot see it'}")
            for f in got.failures[:3]:
                print(f"    {f}")
        return 0 if ok else 1

    r = run(a.cards, only=a.card)
    if not a.quiet:
        for line in r.lines:
            print(line)
    for n in r.notes:
        print(f"NOTE: {n}")
    proven = {n for n, v in r.per_card.items() if v["keys"]}
    n_slice = sum(1 for n in proven if r.per_card[n]["slice"])
    print(
        f"\n{len(proven)} scored cards carry a column or key the chain shows card.rs never reads "
        f"({n_slice} of them in the thin slice, every one a KNOWN_SLICE_GAPS entry while the gate is green); "
        f"{len(r.per_card) - len(proven)} more are named by the coarser register pass alone; "
        f"{len(KNOWN_SLICE_GAPS)} known slice gaps, {len(r.slice_gaps_seen)} of them seen in this table"
    )
    for f in r.failures:
        print(f"FAIL: {f}")
    print("RED" if r.failures else "GREEN")
    return 1 if r.failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
