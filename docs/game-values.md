# Game values

The numbers the engine plays by. This page is generated from the engine (`tools/game_values.py`), so it
matches the engine it was made with. Do not edit it by hand.

## Time

| What | Value |
|---|---|
| One tick | 50 ms (20 ticks a second) |
| Regular time | 180 s (3600 ticks) |
| Overtime | 120 s |
| No plays at the start | the first 90 ticks (4.5 s) |

## Elixir

| What | Value |
|---|---|
| At the start | 6 |
| Most you can hold | 10 |
| Normal rate | 1 elixir every 2.8 s |
| Double elixir (last 60 s of regular time, and overtime) | 1 elixir every 1.4 s |
| Triple elixir (from 60 s into overtime) | 1 elixir every 0.93 s |

## Arena and towers

| What | Value |
|---|---|
| Arena | 18 tiles wide, 32 tiles long; the river is rows 15 and 16 |
| Position units | 18,000 to one tile |
| King tower hitpoints (level 11) | 4,824 |
| Princess tower hitpoints (level 11) | 3,052 |

## Cards (level 11)

Names are the ones the engine takes. Hitpoints are for one unit of the card, and Units says how many it
puts down. Evolution says whether the engine plays the card's evolved form. Hero is what the hero form's
ability button costs, for a card with a hero form.

| Card | Elixir | Type | Units | Flying | Hitpoints | Evolution | Hero |
|---|---|---|---|---|---|---|---|
| Knight | 3 | troop | 1 |  | 1,766 | yes | 2 elixir |
| Archer | 3 | troop | 2 |  | 304 | yes |  |
| Goblins | 2 | troop | 4 |  | 202 |  | 1 elixir |
| Giant | 5 | troop | 1 |  | 3,968 |  | 2 elixir |
| Pekka | 7 | troop | 1 |  | 3,760 | yes |  |
| Minions | 3 | troop | 3 | yes | 230 |  |  |
| Balloon | 5 | troop | 1 | yes | 1,676 |  | 2 elixir |
| Witch | 5 | troop | 1 |  | 839 | yes |  |
| Barbarians | 5 | troop | 5 |  | 716 | yes |  |
| Golem | 8 | troop | 1 |  | 5,120 |  |  |
| Skeletons | 1 | troop | 3 |  | 81 | yes |  |
| Valkyrie | 4 | troop | 1 |  | 1,907 | yes | 3 elixir |
| SkeletonArmy | 3 | troop | 15 |  | 81 | yes |  |
| Bomber | 2 | troop | 1 |  | 304 | yes |  |
| Musketeer | 4 | troop | 1 |  | 721 | yes | 3 elixir |
| BabyDragon | 4 | troop | 1 | yes | 1,152 | yes |  |
| Prince | 5 | troop | 1 |  | 1,920 |  |  |
| Wizard | 5 | troop | 1 |  | 755 | yes | 1 elixir |
| MiniPekka | 4 | troop | 1 |  | 1,390 |  | 1 elixir |
| SpearGoblins | 2 | troop | 3 |  | 133 |  |  |
| GiantSkeleton | 6 | troop | 1 |  | 3,361 |  |  |
| HogRider | 4 | troop | 1 |  | 1,697 |  |  |
| MinionHorde | 5 | troop | 6 | yes | 230 | yes |  |
| IceWizard | 3 | troop | 1 |  | 688 |  |  |
| RoyalGiant | 6 | troop | 1 |  | 3,164 | yes |  |
| SkeletonWarriors | 3 | troop | 3 |  | 81 |  |  |
| Princess | 3 | troop | 1 |  | 261 | yes |  |
| DarkPrince | 4 | troop | 1 |  | 1,200 |  | 3 elixir |
| ThreeMusketeers | 9 | troop | 3 |  | 883 |  |  |
| LavaHound | 7 | troop | 1 | yes | 3,581 |  |  |
| IceSpirits | 1 | troop | 1 |  | 215 | yes |  |
| FireSpirits | 1 | troop | 1 |  | 215 |  |  |
| Miner | 3 | troop | 1 |  | 1,210 |  |  |
| ZapMachine | 6 | troop | 1 |  | 1,451 |  |  |
| Bowler | 5 | troop | 1 |  | 2,081 |  | 2 elixir |
| RageBarbarian | 4 | troop | 1 |  | 1,282 | yes |  |
| BattleRam | 4 | troop | 1 |  | 967 | yes |  |
| InfernoDragon | 4 | troop | 1 | yes | 1,295 | yes |  |
| IceGolemite | 2 | troop | 1 |  | 1,228 |  | 2 elixir |
| MegaMinion | 3 | troop | 1 | yes | 837 |  | 2 elixir |
| BlowdartGoblin | 3 | troop | 1 |  | 261 | yes |  |
| GoblinGang | 3 | troop | 3 |  | 202 |  |  |
| ElectroWizard | 4 | troop | 1 |  | 714 |  |  |
| AngryBarbarians | 6 | troop | 2 |  | 1,341 | yes |  |
| Hunter | 4 | troop | 1 |  | 885 | yes |  |
| AxeMan | 5 | troop | 1 |  | 1,280 | yes |  |
| Assassin | 3 | troop | 1 |  | 906 |  |  |
| RoyalRecruits | 7 | troop | 6 |  | 547 | yes |  |
| DarkWitch | 4 | troop | 1 |  | 906 |  |  |
| Bats | 2 | troop | 5 | yes | 81 | yes |  |
| Ghost | 3 | troop | 1 |  | 1,210 | yes |  |
| RamRider | 5 | troop | 1 |  | 1,766 |  |  |
| MiniSparkys | 4 | troop | 3 |  | 529 |  |  |
| Rascals | 5 | troop | 1 |  | 1,832 |  |  |
| MovingCannon | 5 | troop | 1 |  | 1,809 |  |  |
| MegaKnight | 7 | troop | 1 |  | 3,993 | yes |  |
| SkeletonBalloon | 3 | troop | 1 | yes | 532 | yes |  |
| DartBarrell | 4 | troop | 1 | yes | 614 |  |  |
| Wallbreakers | 2 | troop | 2 |  | 330 | yes |  |
| RoyalHogs | 5 | troop | 4 |  | 837 | yes |  |
| GoblinGiant | 6 | troop | 1 |  | 3,110 | yes |  |
| Fisherman | 3 | troop | 1 |  | 870 |  |  |
| EliteArcher | 4 | troop | 1 |  | 529 |  | 2 elixir |
| ElectroDragon | 5 | troop | 1 | yes | 1,049 | yes |  |
| Firecracker | 3 | troop | 1 |  | 304 | yes |  |
| MightyMiner | 4 | troop | 1 |  | 2,250 |  |  |
| ElixirGolem | 3 | troop | 1 |  | 1,569 |  |  |
| BattleHealer | 4 | troop | 1 |  | 1,920 |  |  |
| SkeletonKing | 4 | troop | 1 |  | 2,298 |  |  |
| ArcherQueen | 5 | troop | 1 |  | 1,000 |  |  |
| GoldenKnight | 4 | troop | 1 |  | 1,799 |  |  |
| SuperIceGolemite | 4 | troop | 1 |  | 3,630 |  |  |
| Monk | 5 | troop | 1 |  | 2,214 |  |  |
| SuperArcher | 3 | troop | 2 |  | 701 |  |  |
| RoyalRecruits_Chess | 7 | troop | 8 |  | 532 |  |  |
| SkeletonDragons | 4 | troop | 2 | yes | 560 |  |  |
| SuperHogRiderTerry | 4 | troop | 1 |  | 2,300 |  |  |
| WitchMother | 4 | troop | 1 |  | 529 |  |  |
| ElectroSpirit | 1 | troop | 1 |  | 217 |  |  |
| ElectroGiant | 7 | troop | 1 |  | 3,952 |  |  |
| PrinceBuff | 5 | troop | 1 |  | 2,304 |  |  |
| Phoenix | 4 | troop | 1 | yes | 1,052 |  |  |
| TriWizards | 7 | troop | 1 |  | 755 |  |  |
| GoblinDemolisher | 4 | troop | 1 |  | 1,300 |  |  |
| GoblinMachine | 5 | troop | 1 |  | 2,265 |  |  |
| SuspiciousBush | 2 | troop | 1 |  | 81 |  |  |
| SuperKnight | 4 | troop | 1 |  | 2,030 |  |  |
| SkeletonWarriors_SpookyChess | 7 | troop | 8 |  | 81 |  |  |
| GiantBuffer | 4 | troop | 1 |  | 2,816 |  |  |
| Berserker | 2 | troop | 1 |  | 896 |  | 3 elixir |
| MergeMaiden_Normal | 3 | troop | 1 |  | 1,121 |  |  |
| MergeMaiden_Mounted | 6 | troop | 1 | yes | 1,121 |  |  |
| Ronin | 5 | troop | 1 |  | 1,779 |  |  |
| Cannon | 3 | building | 1 |  | 824 | yes |  |
| GoblinHut | 4 | building | 1 |  | 1,180 |  |  |
| Mortar | 4 | building | 1 |  | 1,369 | yes |  |
| InfernoTower | 5 | building | 1 |  | 1,748 |  |  |
| BombTower | 4 | building | 1 |  | 1,356 |  |  |
| BarbarianHut | 6 | building | 1 |  | 1,164 |  |  |
| Tesla | 4 | building | 1 |  | 1,182 | yes |  |
| Elixir Collector | 6 | building | 1 |  | 1,070 |  |  |
| Xbow | 6 | building | 1 |  | 1,600 |  |  |
| Tombstone | 3 | building | 1 |  | 529 |  | 5 elixir |
| FirespiritHut | 4 | troop | 1 |  | 727 | yes |  |
| BarbarianLauncher | 5 | building | 1 |  | 1,472 |  |  |
| GoblinCage | 4 | building | 1 |  | 780 | yes |  |
| GoblinDrill | 4 | building | 1 |  | 2,560 | yes |  |
| GoblinPartyHut | 5 | building | 1 |  | 1,180 |  |  |
| Fireball | 4 | spell |  |  |  |  |  |
| Arrows | 3 | spell |  |  |  |  |  |
| Rage | 2 | spell |  |  |  |  |  |
| Rocket | 6 | spell |  |  |  |  |  |
| GoblinBarrel | 3 | spell |  |  |  | yes |  |
| Freeze | 4 | spell |  |  |  |  |  |
| Mirror | 1 | spell |  |  |  |  |  |
| Lightning | 6 | spell |  |  |  |  |  |
| Zap | 2 | spell |  |  |  | yes |  |
| Poison | 4 | spell |  |  |  |  |  |
| Graveyard | 5 | spell |  |  |  |  |  |
| Log | 2 | spell |  |  |  |  |  |
| Tornado | 3 | spell |  |  |  |  |  |
| Clone | 3 | spell |  |  |  |  |  |
| Earthquake | 3 | spell |  |  |  |  |  |
| BarbLog | 2 | spell |  |  |  |  | 1 elixir |
| Heal | 1 | spell |  |  |  |  |  |
| Snowball | 2 | spell |  |  |  | yes |  |
| RoyalDelivery | 3 | spell |  |  |  |  |  |
| WarmSpell | 1 | spell |  |  |  |  |  |
| DarkMagic | 5 | spell |  |  |  |  |  |
| GoblinCurse | 2 | spell |  |  |  |  |  |
| MergeMaiden | 6 | troop | 1 | yes | 1,121 |  |  |
| Vines | 3 | spell |  |  |  |  |  |
| MinionGiant | 4 | troop | 1 | yes | 1,817 |  |  |
| LittlePrince | 3 | troop | 1 |  | 698 |  |  |
| Goblinstein | 5 | troop | 1 |  | 2,385 |  |  |
| BossBandit | 6 | troop | 1 |  | 2,624 |  |  |
| SuperWitch | 6 | troop | 1 |  | 1,064 |  |  |
| SuperLavaHound | 8 | troop | 1 | yes | 7,168 |  |  |
| SuperEliteArcher | 5 | troop | 1 |  | 701 |  |  |
| SuperMiniPekka | 5 | troop | 1 |  | 1,573 |  |  |
| GoblinRocketSilo | 7 | building | 1 |  | 1,999 |  |  |
| GlobalClone | 3 | spell |  |  |  |  |  |
| GoblinPartyRocket | 5 | spell |  |  |  |  |  |
| SuperHogRider | 5 | troop | 1 |  | 1,694 |  |  |
| GlobalLightning | 1 | spell |  |  |  |  |  |
