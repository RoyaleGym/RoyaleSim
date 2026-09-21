"""The mechanic register: every card of the decoded 15.535.29 build, and every mechanic field
each one reaches through its object graph.

WHY IT EXISTS. Full coverage means every base card, evolution, hero form and champion with
every interaction exact. Nobody can list that by heart, and a hand-written mechanic gap table
(charge, spawner, death spawn, hide, evo) is only a sample of it. This tool
derives the full table from the data: a card row (spells_*.csv + TOML overlays) names a character, which names
projectiles, area-effect objects, buffs, spawned characters, abilities and scripted actions, and
so on. Every non-cosmetic field on every reachable object is collected and classified into a
mechanic FAMILY by field name. Fields no family claims are reported under "other", never
dropped, so the register is complete by construction and a new column in a future build shows up
as a diff.

WHAT IT IS NOT. A statement of what the fields MEAN. Semantics are settled by recordings of
the real game, never by this table. The register says WHICH cards carry WHICH mechanic
fields, so a capture protocol can pick representative cards per family and the engine can gate
"this card loads but its mechanic is unread" mechanically.

Usage (repo root):
    python tools/mechanic_register.py            # writes data/derived/mechanic_register.json
    python tools/mechanic_register.py --summary  # families -> counts, plus the "other" fields
    python tools/mechanic_register.py --card Prince --card Tesla_EV1
    python tools/mechanic_register.py --markdown > docs/....md
"""

# ruff: noqa: E501  -- the field-name tables are long regexes on purpose; wrapping them hides typos
from __future__ import annotations

import argparse
import csv
import json
import re
import sys
import tomllib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / "data" / "raw" / "cr-15.535.29" / "csv_logic"
if not RAW.is_dir():
    sys.exit(f"needs a decoded 15.535.29 bundle at {RAW}; see tools/decode_sc_assets.py")
OUT = ROOT / "data" / "derived" / "mechanic_register.json"

# Which files feed which object table. Per-character overlays (characters/*.toml) carry
# [KIND.Name] sections and are routed by KIND below.
TABLES: dict[str, dict[str, list[str]]] = {
    "CHARACTER": {"csv": ["characters.csv", "buildings.csv"], "toml": ["characters.toml", "characters_evo.toml", "buildings.toml", "buildings_evo.toml"]},
    "PROJECTILE": {"csv": ["projectiles.csv"], "toml": ["projectiles.toml", "projectiles_evo.toml"]},
    "AEO": {"csv": ["area_effect_objects.csv"], "toml": ["area_effect_objects.toml", "area_effect_objects_evo.toml"]},
    "BUFF": {"csv": ["character_buffs.csv"], "toml": ["character_buffs.toml", "character_buffs_evo.toml"]},
    "ACTION": {"csv": [], "toml": ["actions.toml"]},
    "ABILITY": {"csv": ["character_abilities.csv"], "toml": ["character_abilities.toml"]},
    "SPELL": {
        "csv": ["spells_characters.csv", "spells_buildings.csv", "spells_other.csv", "spells_evolved.csv", "spells_hero_form.csv"],
        "toml": ["spells_characters.toml", "spells_buildings.toml", "spells_other.toml", "spells_evolved.toml", "spells_heroes.toml"],
    },
}
SECTION_KIND = {"CHARACTER": "CHARACTER", "BUILDING": "CHARACTER", "PROJECTILE": "PROJECTILE", "AEO": "AEO", "BUFF": "BUFF",
                "ACTION": "ACTION", "ABILITY": "ABILITY", "SPELL_OTHER": "SPELL", "SPELL_CHARACTER": "SPELL", "SPELL_EVOLVED": "SPELL",
                "SPELL_BUILDING": "SPELL"}
# Sections that are data for the client only (stat display, extensions, shapes, damage types).
SKIP_SECTIONS = {"STATS", "EXT", "VARIABLE", "DAMAGE_TYPE", "SHAPE", "TARGET_RESOLVER"}

# A field whose NAME matches this is cosmetic: animation, sound, sprite, UI. It never reaches the
# tick. Whitelisted mechanic fields that would otherwise match are listed right after.
COSMETIC = re.compile(
    r"(Effect|Export|FileName|Filename|File$|Clip|Anim|Frame|Scale|Shadow|Icon|^TID|Sound|Audio|Color|Prestige|Label|Sprite|"
    r"Layer|HealthBar|HealthNumber|Indicator|Skin|Popover|Visual|SWF|Placeholder|Highres|Pivot|Alpha|Wobble|Tribe|^Rarity$|"
    r"UnlockArena|UnlockLevel|^Name$|^Base$|ClassType|StatsTags|ContainerName|^Stats$|^stats$|^Icon|Shader|^Scale|"
    r"^UseAnimator|^use360Frames|HasRotationOnTimeline|AttachToSprite|PositionToSprite|MustBeOnTopOf|^Filter$|^FilterFile|"
    r"^FilterExportName|CardEffect|Crowd|Deploy(BaseAnim|edClip|AnimationOverride|TimeChangesDeployAnim)|"
    r"SpawnDeployBaseAnim|HideHealthbar|NotVisible|^Visible$|^Options$|MatchName|PartType|RelativeX|RelativeY|OffsetY$|OffsetX$|"
    r"OffsetYBlue|OffsetYRed|OffsetXBlue|OffsetXRed|ExtraSpellTarget|TargetEffect|TargetingEffect|TargetedEffect|Crosshair|"
    r"Snipe(Side|Target)Clip|LoadAttack|Flame|Tetherv|^Kind$|IngamePathfind(Visible|Effect|Start|Stop)|RequestShowBadge|"
    r"^Delays$|^ShowCardEffect|Underlay|Overlay|PingpongVisual|DontStopMoveAnim|AlwaysResetAnimation|TryToFinishAttackAnimation|"
    r"CustomAnimationPostfix|AnimationsKeepLastFrame|IdleStartFrame|IdleEndFrame|TargetStartFrame|TargetEndFrame|"
    r"DeathEffectOverride|Effects$|ToggleEffectTag|Sprite|^Height$|ConstantHeight|ProjectileStartZ|ProjectileStartHeight|"
    r"StartPositionZOffset|BombZOffset|AttachedCharacterHeight|DoEffectSourcePositionHeightCorrection|^ShadowCustom|^Priority$|"
    r"HitSoundWhenParentAlive|SpecialKamikaze(Start|End)|SpecialDeploy(Start|End)|BalloonFly|BalloonPop|OnPopBalloonEffectList|"
    r"CustomDummyObjectLabel|^SegHpPercentage$|HasIntroAnim|HasDamageVisualPivot|EffectAbsolutePosition|SecondaryEffect|"
    r"LoopSecondaryEffect|Wobble|VisualWaitTime|UseLerpForSouls|FlipPivot|DamageLevelTransition|ProjectileYOffset|"
    r"ProjectileOffsetToCharacterLookDirection|ShakesTargets|ShakesShooter|AttackShakeTime|RotateAngleSpeed|TurretMovement|"
    r"UseCustomMovement|HideWhenDelayed|CustomStateNumber|Hovering|StretchingClip|StretcingClip|PullEnd(Clip|Idle|Grab)|"
    r"PullStartEffect|PullGrabEffect|Capture(Animation)|IdleAnimation|GrabPointOffset|PullCenterOffset|PullFileName|"
    r"AbilityAnimationController|ReplacementFileName|ReplacementExportName|PlaceholderInstanceName|ContextMode|PlaybackDuration|"
    r"^IconFile$|^HighresImageFilename$|^HighresPlaceholderAsset$|^IconSWF$|CharacterSkin|PveDefenseType|^PrestigeCount$|"
    r"^Mirror$|^NotInUse$|^AliveTimeList$|^Boost$|BoostItems|AutoChessForm|EvoInfoSpeed|InBattleStats|StatsUnderInfo|PrefabAsset|"
    r"^Tags$|TypeOfSpell|OmitFromStartingHand|^CardGroup$|AvoidCountingForBuffAmountStats|LoopingFilter|SpecialAttackRangeForStats|"
    r"TargetStartIndicationAction|EvolvedSpells|StartIndication)"
)
KEEP_EVEN_IF_COSMETIC = re.compile(r"^(SummonCharactersOffsets|BombHorizontalOffsets|BombAbsoluteHorizontalOffsets|BombVerticalOffsets|"
                                   r"OffsetXList|OffsetYList|SpawnOffset|SpawnCharacterEffect|DeathSpawnDeployBaseAnim)")

# Field name -> mechanic family. First match wins; order matters (specific before generic).
FAMILIES: list[tuple[str, str]] = [
    ("charge", r"^Charge|KeepChargingAfterAttack|OnStartChargingAction|^DamageSpecial$|^DamageEffectSpecial$|SpecialProjectile|ProjectileSpecial|SpecialLoadTime|SpecialRange|SpecialMinRange|ResurrectGainCharge|ResurrectChargeFilter|StrongDamage|StrongHit|FirstStrongHitPushback"),
    ("dash", r"^Dash|BackDashRadius|AttackDashTime|DoFollowUpJump"),
    ("jump", r"^Jump"),
    ("hide_invisible", r"^Hide|OnAppearAction|OnDisappearAction|UpTimeMs|Invisible|AllowAreaDmgWhenInvisible|AffectsHidden|UntargetableWhenSpawned|AppearBehindAtDistance|OnHideEffect|OnReappearEffect|FirstAppearAction"),
    ("spawner", r"^SpawnData$|^SpawnType$|SpawnerAliveRequired|CustomSpawnFilter|^Spawn(Character|Number|PauseTime|Interval|StartTime|Limit|Radius|Time|InitialDelay|Count|Chain|Clones|MinRadius|MaxRadius|RandomizeSequence|AxisY|Offset|MaxAngle|AngleShift|ConstPriority|Pushback|CharacterDeployTime|CharacterWithDeploy|Attach|Object|AreaObject|AreaEffectObject|Projectile|Character)|^IsSpawnConstPriority|SpawnPathfind|ActivationSpawn|SpawnCharaterRadius|SpawnCharacterCount|SpawnCharacterLevelIndex|SpawnAreaObjectLevelIndex|SpawnEffectOnce|OnSpawnAction|OnStartSpawningAction|OnStartWaitingAction|ActionToRunOnSpawned|MatchOnlyOwnSpawnedTroops|AffectedBySpawnSpeed|StartCookingDelay"),
    ("death_spawn", r"^DeathSpawn|IsDeathSpawn|DeathInherit|OnDeathAction|OnAboutToDieAction|OnKilledAction|OnKilledDoneAction|OnKillAction|^Kamikaze|ManaOnDeath|OtherBuffDeathSpawnAllowed"),
    ("death_damage", r"^DeathDamage|DeathAreaEffect|DeathPushBack"),
    ("shield", r"^Shield"),
    ("resurrect", r"Resurrect|TempResurrect"),
    ("tether", r"^Tether|OnTetherActivation|ConnectedCharacter|GameTagsToSetDutingTether"),
    ("capture_pull", r"^Capture|^Pull|NumberOfUnitsToCapture|TimePausedWhenGrabbing|OnFirstCaptureAction|ActionOnCapturedObject|BuffDuringCapture"),
    ("attract_drag", r"Attract|^Drag"),
    ("warp", r"^Warp|LockDelay|ReleaseLockDelay|AllowWarpWhen"),
    ("deflect", r"^Deflect|ActionOnDeflector|CustomDeflectAction"),
    ("reflect", r"^Reflect"),
    ("chain_hit", r"^Chain|MaxChainLength|RepeatTargets|DeprioritizeRepeatTargets|MaximumTargetsToRemember"),
    ("variable_damage", r"^VariableDamage|DamageScalingMode|^DamageList$|AddedDamage|OnAddedDamageAction|DamageMultiplierPerUnit|CrownDamageDamageMultiplier|DamageScalar|DefenseScalar|BuffAfterHits"),
    ("multi_attack", r"MultipleTargets|MultipleProjectiles|^Projectiles$|^Projectile[23]$|AmmoCount|^Scatter$|HitBiggestTargets|AllTargetsHit|GroupProjectiles|AttackSequence|AttackIndex|AttackStateCount|AttackAmount|OncePerTarget|OneHitPerTarget|CustomFirstProjectile|AttackCooldown|AttackDelay|VisualHitSpeed|NumMatchesNeeded"),
    ("pushback", r"PushFilter|PushRadiusDirectionalOffset|PushToSide|Pushback|PushBack|PushMassFactor|PushSpeedFactor|LateralPushPercentage|DistanceProportinalPush|MeleePushback|IgnorePushBack"),
    ("ability_champion", r"^Ability|IsChampion|UseAbility|MaxCharges|^Cooldown$|InitialCooldown|ActivationTime|^CastTime$|TriggerDelay|OnActivat|GameTagsWhileAbilityActive|KeepIconEvenWhenOutOfCharges|HideChargesTextField|ChampionCharacterData|GameTagsToSetOnReadyToActivate|Elixir(Gain|Full|Cost)|DeployActivate|DeployElixir|IgnoreDeployEffectCards|ActionOnCooldownReady|PendingBuff"),
    ("summoner_hero", r"IsSummoner|^Summon|UseDeployForSummons|InstantHitForSummons|LeftSummon|RightSummon|ManaCostFromSummonerMana|UseProjectedTimeSummon"),
    ("balloon", r"Balloon|^Bomb(Projectile|HorizontalOffsets|AbsoluteHorizontalOffsets|VerticalOffsets|AreaEffectObjects|SpellTarget)|TotalBalloons|DropBalloonAtHpList|OverrideKamikazeDoubleContainer|ContainerAeoList|OffsetXList|OffsetYList"),
    ("clone_mirror", r"^Clone|ClonedVersion|NotCloned|IgnoreClone|CustomCloneFilter|OnClonedAction|MirrorUsesRootSpell|CloneTriggersLandingActions"),
    ("morph", r"Morph|NewCharacterData|NewProjectileData"),
    ("elixir", r"^Mana(CollectAmount|GenerateTimeMs)$"),
    ("stomp", r"StopMovementAfterMS|WaitMS"),
    ("buff_status", r"^Buff|TargetBuff|StartingBuff|SpeedMultiplier|HitSpeedMultiplier|SpawnSpeedMultiplier|DamagePerSecond|HealPerSecond|DamageReduction|EnableStacking|MaxStacks|StackAmountChecks|DamageMultiplier|HitpointMultiplier|IgnoreBuff|CapBuffTimeToAreaEffectTime|AddAsIndividualBuff|ApplyBuff|AllowedOverHealPerc|RemoveOn(Attack|Hit|Heal)|ChainedBuff|DeprioritizeBuffed|IgnoreTargetsWithBuff|DeprioritizeTargetsWithBuff|ValidTargetBuff|ControlsBuff|ControlledByParent|PlayerSpecificBuff|BuffTimeIncreasePerLevel|ActionWhenUnitBuffed|OnBuffAction|DistanceToBuff|DistanceToUnbuff|MaxFriendlyTroops|DistanceToGetTargets|OverrideChargeRange|Immune|ImmuneToDamage|HitTickFromSource|OnRemoveAction|LevelIncrease|SwitchTeam|Rally|Portal"),
    ("lifetime_lane", r"^LifeTime$|^LifeDuration$|SwitchLanes|DestroyAtLimit|PortalTimer|StayAfterParentDies|StayAliveAfterParentDies|AbortIfInstigatorDies|FinishIfInstigatorDies|OnLifeTimeEndAction|ValidDuration"),
    ("sniper_ammo", r"^Snipe|LoopingEffectWhileHasBullets"),
    ("contribution_meter", r"Contribution|Pancake|HoldFullBarTime|WaitPancakeThrowAfterAttackTime"),
    ("deploy_placement", r"CanPlaceOnWater|FullLaneDeploy|TouchdownLimitedDeploy|DeployWTileMargin|GroupMaxSize|^DeployTime$|^DeployDelay$|CustomDeployTime|^Radius$|^SummonRadius|SummonWidth|SummonDeployDelay|SummonNumber|CanPlaceOnBuildings|CanDeployOnEnemySide|SpellAsDeploy|ProjectileWave|ProjectileInterval|ProjectileAngle|NoDeploySize|TileSizeOverride|IsBuilding|UseDeploy$|DeployActive|SingleDeployOffsetAngle|UseDistanceBasedPositioning|IsAGroup|EvaluateDeployedCard|CheckCollisions|ManaCost|IsEnemyAction|IsSameTeamAction"),
    ("targeting_range", r"DetectionRadius|^MaxRange$|LowestHPPercent|^Targets$|MaxUnitPerActionList|MaxUnits_|MinUnits_|^Range$|SightRange|MinimumRange|^MinRange$|^MinDistance$|ProjectileRange|TargetOnly|OnlyEnemies|OnlyOwnTroops|IgnoreBuildings|TargetFilter|TroopFilter|ObjectFilter|GameObjectFilter|HitFilter|SnipeTargetFilter|LockTarget|KeepCurrentTarget|ResetTarget|AllowResetTarget|KeepTargetWithPendingDamage|IgnorePendingDamageTargets|TargetSelectionMode|TargetResolver|SightClip|WaitForTarget|TargetRadius|TargetAoE|DeprioritizeTargets|IgnoreTargets|ExcludeName|ActionToRunIfNoMatch|ActionIfNoMatch|ConsiderZDistance|DistanceX|DistanceY|CaptureRadius|CapturePriority|ResetHitTimerWhenNoTarget|ResetRealHitStarted|OnTargetReachedAction|ActionOnTargetReached|ActionOnTargets|HitAction|OnHitAction|OnHitTargetAction|OnAttackAction|OnStartingAttackAction|OnProjectileShootAction|ActionOnShot|ActionOnGround|OnDamageTakenAction|RelativeLevelAdjustment|CardDataForStats|EnableLevelScaling|BaseDamageType|DamageType|BaseDamageAmount|DamagePerHit"),
    ("area_projectile", r"^Projectile$|^MaxRadius$|^Homing|Gravity|AoeToGround|AoeToAir|HitsGround|HitsAir|SelfAsAoeCenter|^Shape$|^Width$|^RadiusY$|ProjectileRadius|ProjectileStartRadius|ProjectileStartExtraRadius|AreaDamageRadius|AreaEffectObject|AreaEffectOnHit|^Aeo$|AeoList|DamageAEO|StopAeoIfParentHasCombatDisabled|ScaledEffectFollowAeO|^Interval$|HitFrequency|^Duration$|TotalDuration|OverrideDuration|ForcedDuration|CrownTowerDamagePercent|CrownTowerDamagePerHit|BuildingDamagePercent|NoEffectToCrownTowers|CrownTowerDuration|ConstantFlightDuration|FlyDirectPaths|FlyingHeight|CrownTower|^Count$|TargetExprX|TargetExprY|XPositionExpression|YPositionExpression|MirroredX|MirroredY|IgnoreEffects|RandomDelay|UseFixedEffectSourcePosition|ProjectileStartRadius|ContinuousEffect|PreContinuousEffect|LoopContinuousEffect|OneShotEffect|LoopingEffect|SetEvenIfCombatDisabled|InheritPrestigeFromParent|DeathSpawnSameLocation|IgnoreResurrect"),
    ("core_stats", r"ActionCount|TotalHitCount|FirstHitDelay|^MinDamage$|^MaxDamage$|PrecastPendingTime|AvailableManaTrigger|SpellData|^Level$|CharactersOffsetsXMirrored|IngamePathfindSpeed|PingpongMovingShooter|OnExecuteAction|ActionOnSelfWhenTriggered|ApplyImmediate|^Speed$|^HitSpeed$|^LoadTime$|^Damage$|^Hitpoints$|^CollisionRadius$|^Mass$|AttacksGround|AttacksAir|LoadFirstHit|WalkingSpeedTweakPercentage|OverrideAttackFinishTime|AttackFinishTime|HitSpeedOffset|AffectedByHitSpeed|AllowIsGroundTagOnIdle|SightClipSide|StartCounterAt|ProjectileStartRadius|ParentGOAsSource|InstigatorDepth|AttachedCharacter|AttachedInheritAs|SpawnAttachMaxRotation|FollowBehaviour|DeflectBehaviour|DeflectRadius|StartWithBuffWhenNotAttacking|PointToInstigator|SelfAction|InstigatorAction|OnStartAction|OnStartingAction|OnFinishedAction|OnTrueAction|OnFalseAction|NextAction|NextActionWait|SubActions|SubActionsDelay|ActionToExecute|ActionToRun|ActionToTakeDataFrom|ActionToGetDataFrom|ActionDuration|ActionDelay|PassOptionalActionDelay|AllowRepeatAction|ExecuteIfTrue|ForceStopIfTrue|PauseIfTrue|AliveIfTrue|^Condition$|^Conditions$|PerActionConditions|^Variable$|^Value$|DefaultValue|^Parameter$|GameTagsToSet|PauseTag|PauseTags|StateToSet|Persistent|StrategyList|AIStateName|^Delay$|SetEvenIfCombatDisabled|AddToSourceGroup|Singleton|^Unit$|^Action$|^Actions$|Threshold|HealthPercentages|MinCurrentHp|MinMaxHp|OnStartingAction|TransitionDuration|TransitionTime|WarpY|SpawnMaxRadius|ExcludeName"),
]
FAMILY_RE = [(name, re.compile(rx)) for name, rx in FAMILIES]

# Which table a reference field points at. Anything else is tried against every table.
REF_HINTS: list[tuple[re.Pattern, str]] = [
    (re.compile(r"Projectile"), "PROJECTILE"),
    (re.compile(r"Buff(?!Time|Number|Delay|Override|After|OnDamageTime|WhenNotAttackingTime|WhenNotAttackingUseAttackRange|TimeIncrease)"), "BUFF"),
    (re.compile(r"Ability"), "ABILITY"),
    (re.compile(r"Action|Strateg"), "ACTION"),
    (re.compile(r"AreaEffect|AreaObject|^Aeo|AEO|AeoList|SpawnObject|DeathAreaEffect"), "AEO"),
    (re.compile(r"Character|Summon|Spawn|Unit|Clone|Morph|Attached|Champion|NewCharacterData|Resurrect"), "CHARACTER"),
]


def read_csv(path: Path) -> dict[str, dict[str, list[str]]]:
    """Supercell csv_logic: row 0 names, row 1 types, then rows; a row with an empty Name
    continues the previous row's list columns. Values are kept as lists of non-empty strings."""
    rows: dict[str, dict[str, list[str]]] = {}
    if not path.exists():
        return rows
    with path.open(encoding="utf-8", newline="") as fh:
        rd = csv.reader(fh)
        header = next(rd)
        next(rd, None)  # the types row
        current: dict[str, list[str]] | None = None
        for row in rd:
            if not row or all(c == "" for c in row):
                continue
            name = row[0]
            if name:
                current = defaultdict(list)
                rows[name] = current
            if current is None:
                continue
            for col, val in zip(header, row, strict=False):
                if val != "":
                    current[col].append(val)
    return rows


def merge_toml(table: dict[str, dict[str, list[str]]], obj: dict) -> None:
    for name, fields in obj.items():
        if not isinstance(fields, dict):
            continue
        row = table.setdefault(name, defaultdict(list))
        for k, v in fields.items():
            vals = v if isinstance(v, list) else [v]
            # An inline table ([KIND.Name.Field]) is an anonymous sub-object (usually an
            # action); it is kept as a dict and walked in place.
            row[k] = [x if isinstance(x, dict) else str(x) for x in vals]


def load_tables() -> dict[str, dict[str, dict[str, list[str]]]]:
    tables: dict[str, dict[str, dict[str, list[str]]]] = {k: {} for k in TABLES}
    for kind, spec in TABLES.items():
        for f in spec["csv"]:
            for name, row in read_csv(RAW / f).items():
                tables[kind].setdefault(name, defaultdict(list)).update(row)
                if kind == "SPELL":
                    tables[kind][name]["_source"] = [f]
        for f in spec["toml"]:
            p = RAW / f
            if p.exists():
                merge_toml(tables[kind], tomllib.load(p.open("rb")))
    for p in sorted((RAW / "characters").glob("*.toml")):
        doc = tomllib.load(p.open("rb"))
        for section, body in doc.items():
            kind = SECTION_KIND.get(section)
            if kind is None:
                if section not in SKIP_SECTIONS:
                    print(f"note: unknown section [{section}] in {p.name}", file=sys.stderr)
                continue
            merge_toml(tables[kind], body)
    return tables


def is_cosmetic(field: str) -> bool:
    return bool(COSMETIC.search(field)) and not KEEP_EVEN_IF_COSMETIC.search(field)


def family_of(field: str) -> str:
    for name, rx in FAMILY_RE:
        if rx.search(field):
            return name
    return "other"


# Reference-shaped fields that are NOT part of the card's own mechanic graph: evolution links,
# name filters/lists that merely mention other cards, stat-display pointers.
NO_FOLLOW = re.compile(r"Filter|Names$|IgnoreList|EvolvedSpells|AutoChessForm|CardGroup|ExcludeName|^Tags$|MatchName|"
                       r"StatsUnderInfo|InBattleStats|CardDataForStats|ForStats|BoostItems|^Boost$|PrefabAsset|"
                       r"ChampionCharacterData$")


def resolve_ref(tables, field: str, value: str) -> tuple[str, str] | None:
    if NO_FOLLOW.search(field):
        return None
    order = [kind for rx, kind in REF_HINTS if rx.search(field)]
    order += [k for k in ("CHARACTER", "PROJECTILE", "AEO", "BUFF", "ACTION", "ABILITY") if k not in order]
    for kind in order:
        if value in tables[kind]:
            return kind, value
    return None


def walk(tables, kind: str, name: str, seen: set[tuple[str, str]], out: list[dict]) -> None:
    if (kind, name) in seen:
        return
    seen.add((kind, name))
    row = tables[kind].get(name)
    if row is None:
        return
    walk_fields(tables, f"{kind}:{name}", (kind, name), row, seen, out)


def walk_fields(tables, label: str, me: tuple[str, str], row, seen, out) -> None:
    for field, vals in row.items():
        if field.startswith("_"):
            continue
        for v in vals:
            if isinstance(v, dict):
                # Anonymous inline object: its fields belong to this card too.
                walk_fields(tables, f"{label}.{field}", me, {k: (x if isinstance(x, list) else [x]) for k, x in v.items()}, seen, out)
                continue
            v = str(v)
            ref = resolve_ref(tables, field, v) if not v.replace("-", "").replace(".", "").isdigit() else None
            if ref is not None and ref != me:
                walk(tables, ref[0], ref[1], seen, out)
        if is_cosmetic(field):
            continue
        plain = [str(v) for v in vals if not isinstance(v, dict)]
        if not plain:
            continue
        out.append({"object": label, "field": field, "value": plain if len(plain) > 1 else plain[0], "family": family_of(field)})


def card_kind(row) -> str:
    src = (row.get("_source") or [""])[0]
    if "evolved" in src:
        return "evolution"
    if "hero" in src:
        return "hero"
    return "base"


def build() -> dict:
    tables = load_tables()
    cards = {}
    for name, row in tables["SPELL"].items():
        if row.get("NotInUse") and row["NotInUse"][0].upper() == "TRUE":
            continue
        fields: list[dict] = []
        seen: set[tuple[str, str]] = set()
        walk(tables, "SPELL", name, seen, fields)
        fams = defaultdict(set)
        for f in fields:
            fams[f["family"]].add(f["field"])
        cards[name] = {
            "kind": card_kind(row),
            "source": (row.get("_source") or [""])[0],
            "elixir": (row.get("ManaCost") or [None])[0],
            "rarity": (row.get("Rarity") or [None])[0],
            "objects": sorted(f"{k}:{n}" for k, n in seen if k != "SPELL"),
            "families": {k: sorted(v) for k, v in sorted(fams.items())},
            "fields": fields,
        }
    families = defaultdict(lambda: {"cards": [], "fields": set()})
    for name, c in cards.items():
        for fam, fl in c["families"].items():
            families[fam]["cards"].append(name)
            families[fam]["fields"].update(fl)
    return {
        "version": "15.535.29",
        "generated_by": "tools/mechanic_register.py",
        "note": "Fields by name only; semantics are settled by recordings of the real game.",
        "families": {k: {"cards": sorted(v["cards"]), "fields": sorted(v["fields"])} for k, v in sorted(families.items())},
        "cards": dict(sorted(cards.items())),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--markdown", action="store_true")
    ap.add_argument("--card", action="append", default=[])
    ap.add_argument("--out", type=Path, default=OUT)
    a = ap.parse_args()
    if not RAW.exists():
        print(f"missing {RAW}: decode the 15.535.29 assets first (tools/decode_sc_assets.py)", file=sys.stderr)
        return 2
    reg = build()
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(reg, indent=1), encoding="utf-8")
    kinds = defaultdict(int)
    for c in reg["cards"].values():
        kinds[c["kind"]] += 1
    if a.summary or a.markdown:
        p = print
        if a.markdown:
            p("| Family | Cards | Fields |")
            p("|---|---|---|")
        for fam, v in reg["families"].items():
            if a.markdown:
                p(f"| {fam} | {len(v['cards'])} | {', '.join(v['fields'])} |")
            else:
                p(f"{fam:20s} {len(v['cards']):4d} cards  fields: {', '.join(v['fields'])}")
        p(f"\ncards: {dict(kinds)}; other-family fields (unclassified): {reg['families'].get('other', {}).get('fields', [])}")
    for name in a.card:
        c = reg["cards"].get(name)
        if c is None:
            print(f"{name}: not a card", file=sys.stderr)
            continue
        print(f"\n{name} ({c['kind']}, {c['elixir']} elixir): objects {c['objects']}")
        for fam, fl in c["families"].items():
            print(f"  {fam}: {', '.join(fl)}")
    print(f"wrote {a.out} ({len(reg['cards'])} cards: {dict(kinds)})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
