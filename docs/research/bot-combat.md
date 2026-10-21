# Bot combat and the match harness

How well vcod's bots fight, and how a change to them is measured. Retail
Call of Duty has no bots, so nothing here is checked against a retail
binary: the yardstick is vcod's own seeded matches. The brain is
`crates/server/src/bots.rs`; how bots move is in `bot-navigation.md`, how
they play objectives in `bot-objectives.md`.

## 1. The harness

`cargo run --release -p vcod-server --example bot_match [seed...]` runs one
in-process match per map and seed (default 1..=8) with every bot's fire
on, on the stock gametype scripts, until the script calls `exitLevel` or
`TICKS` (12000, ten minutes of 50 ms ticks) run out. `JOBS` (4) matches
run at once; the nav graph is built once and shared. It is the one bot
measuring tool: the per-feature examples that came before it
(`team_tally`, `cover_ab`, `bot_fights`) are folded in, and a new
measure goes in it too. A match is deterministic in its
seed: the same seed on the same build prints the same lines. The example
passes `2 * seed + 1` to `Server::test_seed_rng`, which sets the low bit
itself, so neighbouring seeds don't replay one match.

Knobs: `MAPS=` a comma list (`MAP=` too; mp_carentan), `GAMETYPE=`
(tdm), `BOTS=` (6), `TICKS=`, `JOBS=`, `SET=name=value,...` (any cvar,
e.g. `scr_tdm_timelimit=5`), `UO=1` (a United Offensive server:
`COD_DIR` at a UO install), `VERBOSE=1` (every kill line).
`AXIS_SKILL=` / `ALLIES_SKILL=` add the `BOTS` through `addbot <side> <n>
skill <level>`, half a side; `SKILLS=a,b,...` adds one bot per entry by
balance (for `dm`). Without them the bots join as `--bots` does, at
`vc_bot_skill`.

Feature arms: `COVER=` (section 4, `Server::test_bot_cover`), `RETREAT=`
(section 5, `test_bot_retreat`) and `TEAMPLAY=` (`bot-objectives.md` 9,
`test_bot_team_play`: off, a bot ranks by slot, every attacker keeps by
an `re` carrier, nobody covers a planter, plans pay no crowd cost, a
carrier fights as any bot and every noise draws the bot, the brain
before team play; section 7) each take `all`
(the default), `none`, `axis`, `allies` or `alt` (axis on odd seeds,
allies on even, so the map's own asymmetry cancels over an even seed
count). A default arm never calls its hook, so an all-default run plays
the stock game bit for bit (checked: seeds 1 and 2 print the lines of
the harness before the arms). `dm` has no sides: only `all` and `none`
mean anything there. With an arm one-sided the totals add a row per
arm, named by the one-sided features on for it (`control` for none),
and the wins and actions by arm.

Everything it counts comes off the script's `logPrint` lines, the same
`games_mp.log` lines a retail server writes:

| Column | From | Meaning |
|---|---|---|
| kills, deaths | `K;` | A kill by another player. A suicide (attacker is the victim or the world, `-1`; on the victim's row) and a team kill (same team, not `dm`) are counted apart and are not kills. World damage (a fall) is left out. |
| damage | `D;`, `K;` | Damage dealt to others. A non-lethal hit logs `D;`, the lethal one `K;` alone. |
| ttk | `D;`, `K;` | Time to kill: from the attacker's first hit on that victim in the victim's life to the kill, in ms. A one-hit kill is 0. Mean and median; the mean carries the long tails of a fight broken off and resumed. |
| wins | `W;` | The winning team per line: one per round in `sd` and `re`, one per match in the others (`dm` names the player). |
| score | `getTeamScore` | Axis and allies at the end (before `exitLevel` zeroes them). Rounds won in `sd` and `re`. |
| actions | `A;` | Objective actions by team: `bomb_plant`, `bomb_defuse`, `re_pickup`, `re_capture`, UO's `ctf_take` and kin. |
| stalled | bodies | Bot-ticks whose last 12 s lie within 150 units of the first, as `ship_stalls` counts them. With fire on this also counts a bot holding a spot on purpose (an `sd` defender at its site), so read it against a baseline, not as zero. |
| ended | `exitLevel` | Matches that reached their end inside `TICKS`. |
| life | bodies | Mean life: bot-ticks playing over the lives they made up (a life still running at the end is left out). |
| wounds, dead in 5 s | bodies | A life's first tick under 50 health while `BotBody::engaged`, and the share of those whose bot died within 5 s. |
| retreats hurt, dry | bodies | `BotBody::retreating` going from 0 to 1 (hurt) or 2 (dry): retreats started (section 5). |
| cover hold, lean | `bot_cover_state` | Bot-ticks playing on a held cover spot, and with `ps.lean` off zero (section 4). |
| mate, packed | bodies | How close a team keeps. Each bot-tick playing with a teammate playing adds its distance to the nearest one (3D, units): `mate` is the mean. `packed` is the share of those bot-ticks with two or more teammates within 400 units, the bot and two others in one room. Bots only; a corpse or spectator is left out. `dm` has no teammates and prints 0. |

Each match prints one line, then the totals by team, by skill level and,
with an arm one-sided, by arm; with several maps a block per map comes
first. Each row prints twice: the log's columns, then the bodies'.

A change to bot behaviour reports its before and after on the same seeds,
map and gametype, in this doc's section for it.

## 2. Baseline

Master at the harness's commit, `bot_match` defaults (mp_carentan `tdm`,
six bots, seeds 1..=8, 12000 ticks, none ended):

| Team | Kills | Deaths | Damage | TTK mean / median ms |
|---|---|---|---|---|
| allies | 644 | 568 | 87952 | 1603 / 250 |
| axis | 568 | 644 | 82955 | 1889 / 0 |

1212 kills over 80 minutes of play, 88 stalled bot-ticks. No suicides and
no team kills.

## 3. Skill levels

`vc_bot_skill` (0-3, default 1) and `addbot ... skill N` pick one of four
presets of `bots::Skill` (`bots/skill.rs`). Level 1 is `Skill::default()`,
field for field (`the_default_level_is_the_default_skill`), and its hearing
scale of 1.0 multiplies a noise's radius exactly, so a default server plays
bit for bit as before: section 2's eight matches print the same lines on
the skill branch, before and after merging master.

| Field | 0 easy | 1 normal | 2 hard | 3 expert |
|---|---|---|---|---|
| reaction, ticks (min + rand(spread)) | 8 + 6 | 5 + 6 | 3 + 4 | 2 + 2 |
| turn per tick, degrees | 10 | 15 | 22 | 30 |
| aim error start, floor (units) | 64, 5 | 48, 4 | 32, 2.5 | 24, 1.5 |
| error decay per tick | 0.91 | 0.88 | 0.84 | 0.80 |
| error doubles at range | 900 | 1000 | 1300 | 1600 |
| error while moving | x1.9 | x1.75 | x1.5 | x1.3 |
| forget a target, ticks | 16 | 20 | 30 | 40 |
| remember a spot, a noise, ticks | 70, 140 | 100, 200 | 140, 260 | 180, 300 |
| hearing scale | 0.75 | 1 | 1.25 | 1.5 |

The trigger only goes down while the gun's line passes within `HIT_RADIUS`
(10 units) of the target's chest (`Bot::trigger`), so the error floor sets
the range a bot can fire at at all: `floor * (1 + range / error_range) *
moving < 10`. A first easy preset with a floor of 10 never met it: in
`dm` its bots scored 26 kills to 518 deaths over eight matches and stood
in fights without firing (3566 stalled bot-ticks in an all-easy `tdm`
against the baseline's 88). The floor of 5 lets an easy bot fire standing
inside 900 units and almost never on the move.

Measured with `bot_match`, mp_carentan, seeds 1..=8, 12000 ticks:

Mixed `dm`, eight bots, two per level (`GAMETYPE=dm SKILLS=0,1,2,3,0,1,2,3`;
all eight matches ended at the score limit):

| Level | Kills | Deaths | K/D | Damage | TTK median ms |
|---|---|---|---|---|---|
| 0 | 166 | 518 | 0.32 | 20996 | 200 |
| 1 | 207 | 491 | 0.42 | 32017 | 150 |
| 2 | 605 | 345 | 1.75 | 74568 | 400 |
| 3 | 678 | 302 | 2.25 | 93670 | 250 |

The same on mp_depot: 0.31, 0.62, 1.50, 2.37.

`tdm` three against three, one side at a level against normal, both ways
round to cancel the side bias (allies lead axis 644 to 568 at equal skill,
section 2). Kills of the side at the level, to the normal side's:

| Level | As axis | As allies |
|---|---|---|
| 0 | 354 : 764 | 406 : 715 |
| 2 | 800 : 346 | 793 : 309 |
| 3 | 799 : 268 | 800 : 209 |

At one level for all six (`SET=vc_bot_skill=N`), total kills in the 80
minutes: 1044 at 0, 1212 at 1, 1245 at 2, 1367 at 3; stalled bot-ticks
364, 88, 95, 38 (levels 2 and 3 ran on the branch before its master
merge, which leaves level 1's lines unchanged).

Hard and expert sit close in `dm` because both react and settle faster than
a fight lasts; the gap shows head to head against normal (3 : 1 kills at
hard, 3.4 : 1 at expert). Nothing here is tuned against a human. How a
level feels to play against is a hand check (pending-manual-test.md 519).

## 4. Cover and lean

The brain is `crates/server/src/bots/cover.rs`; the server fills
`BotView::cover` in `Server::bot_cover` (`crates/server/src/server.rs`).

### 4.1 What a cover spot is

A spot is a place the bot's body hides from the threat's eye while a peek
from it still has a line to the point the bot aims at on the threat
(`cover::Sight::peek`). Two peeks:

- `Peek::Stand`: crouched (eye 40) the body is hidden, standing (eye 60)
  the eye sees the threat. A wall waist to chest high.
- `Peek::Lean(side)`: standing, the body is hidden, and leaning `side`
  (retail's lean keys, `wbuttons` 0x10 left and 0x20 right) the eye sees
  the threat. A corner, or a wall edge.

"Hidden" is four bullet segments (`CollisionWorld::shot_trace`, props
included) from the threat's eye, all blocked: to the eye, the chest (two
thirds of the eye's height) and the chest 10 units either side across the
line. The leaned eye is `aim::lean_offset` at the standing lean's limit
(1 on 1.1, 0.5 on 1.5 and UO, `pmove::lean_scale`), cut by the fraction
of `PM_UpdateLean`'s 16-unit box sweep a wall leaves, as pmove cuts it.

### 4.2 The search

`cover::find` walks level edges (no jump, no risky edge, no step past
`STEPSIZE`) out from the node nearest the body, five rings and at most
128 units across, and tests the body's own spot first, then the nodes
nearest first. The first `Peek::Stand` in reach wins; a lean spot is
taken only where there is none (4.5). The server re-checks the spot it
gave last every 250 ms against the threat's current eye and keeps it
while it still hides and peeks; it searches again only when it does not.
Within 300 units of the threat there is no search: the bot fights where
it stands.

Decision: the search runs at fight time, not at graph build. A spot
depends on where the threat stands, so a per-node table would need a
verdict per direction and still a trace at run time to confirm it. A
search costs at most a few dozen candidates of one to six segments,
most rejected on the first, four times a second per fighting bot. The
graph and its build are unchanged, so `bot-navigation.md`'s Build times
and the `nav_census` floors do not move.

### 4.3 What the bot does with it

- Walks to the spot, keys eased inside 48 units so it comes to rest on
  it, sights down beyond 64 units. It aims and fires on the way as the
  fight allows (`Bot::cover_keys`).
- On the spot it holds still. While the weapon is ready it peeks
  (stands, or holds the lean key); while it reloads or the weapon is busy
  past 300 ms (a bolt's rechamber, not a semi-automatic's tap spacing)
  it crouches behind a `Peek::Stand` wall or comes back off a lean.
- With the threat out of sight the spot is held for 3 s
  (`cover::WAIT_TICKS`), peeking where the threat was last aimed at; the
  target is not counted lost meanwhile, so a re-peek keeps its settled
  aim (`Bot::track`). Past that the hold goes and the bot hunts the
  remembered spot as before.
- No cover while a plant, defuse or carry is under way, on a capture
  zone or radio, with a UO LMG, or held by a link. A guard or a sweep
  takes cover (`Bot::may_cover`). A live frag near clears the lean and
  crouch keys with the rest (`Bot::flee`).

### 4.4 Aim from a leaned eye

A lean moves the eye, and the muzzle with it, up to 20 units sideways
(`G_AddLean`, combat doc 2.1). `BotView::lean_offset` carries it and
`aim_angles` aims from there; from the upright eye a leaned round
passes up to 20 units beside a chest it was aimed at, twice
`HIT_RADIUS`. INFERRED: needed for any leaned shot to land. VERIFIED
(measured, 4.5's second and third rows): the fix alone did not lift the
first cut's score, which its walk dominated.

### 4.5 Measured

`cargo run --release -p vcod-server --example cover_ab [seed...]`: `tdm`,
8 fighting bots, 6000 ticks (5 minutes) a seed, one team with cover and
one without, the sides swapped every other seed. Deaths of each side;
fewer is better. `OFF=1` turns cover off on both sides. Over 20 seeds the
per-seed difference has a standard deviation of 12 to 14 deaths, so a
total difference under about 60 is noise.

The `bot_match` harness (section 1) was not on master when this was
written; these runs are `cover_ab`'s own, an example since folded into
`bot_match` (`COVER=alt` is its side swap; its seeds passed straight to
`test_seed_rng`, so seeds 2k and 2k+1 played one match). Section 6
re-measures on `bot_match`.

| run (mp_carentan unless named, seeds 1-20) | cover side deaths | other side deaths |
|---|---|---|
| cover off on both sides | 1141 | 1152 |
| first cut: 192-unit reach, full-speed walk, aim from the upright eye | 1079 | 957 |
| the aim fix of 4.4 alone | 1114 | 903 |
| 128-unit reach, own spot first, eased walk | 1027 | 1055 |
| the same, lean spots only | 1027 | 1035 |
| the same, crouch-and-stand spots only | 1092 | 1220 |
| shipped: a stand spot before a lean | 987 | 1050 |
| shipped, mp_dawnville | 813 | 887 |

- VERIFIED (measured, the rows above): the shipped cover side dies 6%
  fewer times on mp_carentan and 8% on mp_dawnville, winning 12 and 13
  of 20 seeds. Pooled over both maps the mean per-seed gain is 3.4
  deaths at a standard error of about 2.2: a small gain, not a proven
  one.
- VERIFIED (measured): crouch-and-stand spots alone gained more (1092
  to 1220, 14 seeds of 20) and lean spots alone broke even.
- INFERRED: a leaned body shows its head and the threat's sight then
  aims there (`aim_point` falls back to the eye when the chest is
  hidden), so a lean trades body exposure for headshots; behind a low
  wall the bot drops out of sight on every reload and the threat's
  target lapses after a second (`Skill::forget_ticks`) while the bot's
  own does not.
- VERIFIED (measured, the walk counted per tick, seed 3, 3000 ticks):
  with a 192-unit reach 1447 of the 2176 ticks holding a spot (66%) were
  spent walking to it, at `Skill::error_moving` and mostly off the
  sights. With the 128-unit reach, the body's own spot first and the
  eased walk, 3538 of 6321 (56%).
- VERIFIED (measured, rows 4 and later): fewer deaths overall than with
  cover off (2037 against 2293): fights from cover last longer.

A run that walks to the spot only while the weapon is busy (reload
behind cover, fight in the open otherwise) scored 1045 to 1091, inside
the noise of the shipped rule, so the simpler rule stayed.

## 5. Retreat

A bot in a fight breaks contact when it is hurt or its clip runs dry, then
reloads, waits and plays on (`crates/server/src/bots/retreat.rs`). Retail
has no bots, so none of this is measured against retail; the numbers are
from `--example bot_fights` (below), since folded into `bot_match`
(`RETREAT=`, and its wound columns). The baseline, skill and cover
tables above predate the retreat (on by default since this section);
section 6 re-measures all of them on `bot_match`.

- Hurt: health under 50% of the most (`HURT_PCT`) with an enemy in
  sight, once per wound (`Bot::hurt_at`): a bot that held its cover fights
  on at that health until it is hit again, or healed past it. 1.1, 1.5 and
  UO have no health regeneration; a health pack is the only way back up.
- Dry: the clip at 0 with rounds in the reserve, an enemy in sight and no
  switch under way at the end of the think, so a pistol draw
  (`Bot::sidearm`) comes first. It ends once the clip holds a round and the
  weapon is idle, or the reserve is gone. Once per dry clip (`Bot::dry_had`,
  cleared when the clip holds a round or the reserve is gone): one that
  found no cover reloads where it stands, as 6.4 explains.
- Cover (`cover_spot`): the nearest graph node by path from the body,
  within 1200 units of path and 300 nodes tested (the graph's nodes sit a
  few tens of units apart: on mp_carentan, mp_harbor and mp_depot the
  first hidden node took a median 7 tests and at most 205), whose standing
  eye and chest the threat's eye cannot see by `shot_trace` (props
  included), nearer the bot than the threat and over 200 units from it.
  The threat's eye is the enemy's `ps.view().eye` as last seen. The cover
  found is kept while it stays hidden from the threat's eye; otherwise it
  is searched again. No cover found ends the retreat: the bot fights on.
- On the way the keys follow the path (`rekey` off the path's yaw) while
  the view and the trigger stay on the enemy, sights up. At the cover the
  body stands still (no unstick spell), crouches with nothing in sight,
  tops the clip up and turns to where the threat was. A hurt bot holds 3 s
  (`HOLD_TICKS`), less with a health pack near; no retreat runs past 10 s.
- Health packs: a bot under 70% (`HEAL_PCT`) with nothing in sight walks
  to the nearest `item_health*` lying within 1200 units across and 200 up
  or down (`ScriptRuntime::health_items`; stock `dm`, `tdm` and `sd` drop
  one on every death, `dropHealth`, cod11-items.md).
- Duties a retreat or a health detour never breaks (`duty_bound`): a plant
  or defuse under way, an `re` objective carried, a UO `ctf` flag carried,
  a defuse against the clock (S&D's planted bomb, a charge on a `bas`
  defender's base), and a body inside the `dom` flag zone or `hq` radio
  zone it is taking. Everything else retreats: an S&D attacker on its way
  to a site, a guard, an `re` defender chasing, a `bas` breacher, both
  `bel` sides. On a ladder or with an LMG's bipod down nothing starts.
- `BotBody::retreating` reads 1 (hurt), 2 (dry), 3 (at the cover) or 0;
  `Server::test_bot_retreat` turns it off per bot.

### Measured (2026-10-20)

`cargo run --release -p vcod-server --example bot_fights`: 8 bots, `tdm`,
3000 ticks (2.5 min) a match, mp_carentan, mp_harbor, mp_depot and
mp_brecourt, 16 seeds each (64 matches a row), 1.1. In a symmetric match
every kill is someone's death, so one side runs the retreat and the other
does not; `none` is master's brain (the retreat off is the old code path).
"Wounds" counts a life's first tick under 50 health while engaged, and
the share of those dead within 5 s. A changed brain moves every later
draw, so each match diverges whole: across seed sets the same side's K/D
swung by 0.15 with nothing changed but the other side's brain.

| Seeds | Retreat on | allies K/D | axis K/D | allies wounds dead in 5 s | axis wounds dead in 5 s |
|---|---|---|---|---|---|
| 101-116 | none | 1.06 | 0.94 | 80% | 87% |
| 101-116 | allies | 1.13 | 0.88 | 71% | 86% |
| 101-116 | axis | 1.07 | 0.93 | 79% | 77% |
| 117-132 | none | 1.13 | 0.88 | 78% | 85% |
| 117-132 | allies | 0.99 | 1.01 | 73% | 85% |
| 117-132 | axis | 0.96 | 1.04 | 81% | 77% |

United Offensive (`UO=1`), mp_cassino and mp_kursk, seeds 1-8, 16 matches
a row:

| Retreat on | allies K/D | axis K/D | allies wounds dead in 5 s | axis wounds dead in 5 s |
|---|---|---|---|---|
| none | 1.08 | 0.91 | 83% | 86% |
| allies | 1.00 | 0.99 | 73% | 78% |
| axis | 1.03 | 0.97 | 72% | 77% |

- A wounded bot running the retreat dies within 5 s 5 to 10 points less
  often, on all six runs. On 1.1 the untreated side's share stays within
  3 points; on UO it fell 8 to 11 too, perhaps because an enemy backing
  off finishes fewer wounded (a guess; 16 matches, about 180 wounds a
  side).
- K/D: the treated side's ratio against `none`, both sides and both seed
  sets in one geometric mean, is 1.02 on 1.1 and 0.99 on UO: no change
  within the noise.
- Variants, seeds 101-116, the treated side's K/D against `none`: the
  sights dropped while backing off, allies 1.03 and axis 0.93 (geometric
  mean 0.98; the default keeps them up); a hurt retreat only past 400
  units, 1.02 and 0.92 (0.97); the dry retreat and the health packs with no
  hurt retreat, 1.07 and 0.95 (1.01, wounds dead in 5 s unchanged at 78%
  and 86%).
- Retreats in 64 matches with one side on: 1150 to 1440 hurt and 40 to
  100 dry starts, of which 28 to 32% reach the cover; the rest die on the
  way, or the retreat ends first (a dry clip refilled, or no cover left
  from a threat that moved).

## 6. One table on `bot_match` (2026-10-21)

Sections 2 to 5 and `bot-objectives.md` 9 were measured on four
different tools with different seeds, bot counts and lengths, so their
numbers do not compare. These rows are all `bot_match` at one tip
(branch `test/bot-harness-unify` off master c8c3fd1f), 8 bots, 6000
ticks (5 min) a match, 1.1 unless named. Retail has no bots: every
number is ours.

Each arm leaves the other features at their default, on for both sides
(`COVER=alt` runs the retreat on both sides). "Margin" is the arm
side's score less the other's per match, mean and standard error: the
symmetric rows give the same figure for the `alt` split with nothing
different between the sides, the noise floor.

### 6.1 `tdm`, mp_carentan, mp_harbor, mp_depot, mp_brecourt, seeds 1..=16 (64 matches a row)

VERIFIED (measured, ours, `bot_match`, 2026-10-21):

| Run | Side | Kills | Deaths | K/D | Life s | Wounds | Dead in 5 s | TTK mean ms | Margin |
|---|---|---|---|---|---|---|---|---|---|
| stock (all on) | both | 5237 | 5244 | 1.00 | 24.8 | 3873 | 72% | 1783 | -1.1 +- 1.3 |
| `COVER=none RETREAT=none` | both | 6596 | 6600 | 1.00 | 19.5 | 4826 | 85% | 1215 | -2.2 +- 2.1 |
| `COVER=alt` | cover | 3045 | 3043 | 1.00 | 21.4 | 2277 | 75% | 1672 | +0.1 +- 2.0 |
| | control | 3040 | 3046 | 1.00 | 21.4 | 2320 | 74% | 1913 | |
| `RETREAT=alt` | retreat | 2736 | 2726 | 1.00 | 24.1 | 2010 | 72% | 1438 | +0.2 +- 1.5 |
| | control | 2724 | 2743 | 0.99 | 23.9 | 2062 | 82% | 1636 | |
| `COVER=alt RETREAT=alt` | both on | 3124 | 2904 | 1.08 | 22.3 | 2104 | 74% | 1142 | +3.5 +- 1.9 |
| | control | 2903 | 3125 | 0.93 | 20.7 | 2377 | 85% | 1833 | |
| one side `vc_bot_skill 2` | hard | 3790 | 1747 | 2.17 | 36.7 | 1228 | 66% | 1150 | +31.9 +- 1.6 |
| | normal | 1747 | 3790 | 0.46 | 17.0 | 2672 | 80% | 3080 | |

The skill row is two runs: `AXIS_SKILL=2` on seeds 1..=8 and
`ALLIES_SKILL=2` on 9..=16 (the other side at 1), summed.

- VERIFIED (measured, the table): skill is the one large effect: a hard side outscores a normal one 2.2
  to 1, as section 3's three-a-side runs found (2.3 to 2.6).
- VERIFIED (measured, the table): cover alone and the retreat alone move no score: both margins sit
  inside one standard error, as small as the stock run's null split.
  Section 4.5's 6% (one map, 20 seeds, the retreat not yet in) does not
  show here.
- VERIFIED (measured, the table): the retreat does what it was built for and nothing more: a wounded bot
  running it dies within 5 s 72% of the time against 82%, with K/D
  unchanged (section 5 found the same).
- VERIFIED (measured, the table): together on one side they gave +3.5 a match, 1.8 standard errors, and
  the lowest share of wounded deaths against the highest (74% against
  85%). 6.2 repeats the three arms on a second seed set.
- VERIFIED (measured, the table): both features on everywhere make the
  game slower: 21% fewer kills than with both off, lives 24.8 s against
  19.5 s, and the `stalled` count nearly eightfold (104718 bot-ticks
  against 13666). INFERRED: the extra stalls are bots holding a cover
  spot or a retreat's cover on purpose, so read `stalled` against a run
  with the same features on.

### 6.2 The arms again, seeds 17..=32

The same three `alt` runs as 6.1 on fresh seeds (64 matches a row), and
the two sets pooled (128 matches).

VERIFIED (measured, ours, `bot_match`, 2026-10-21):

| Run | Side | Kills | Deaths | K/D | Dead in 5 s | Margin | Pooled margin |
|---|---|---|---|---|---|---|---|
| `COVER=alt` | cover | 2984 | 2871 | 1.04 | 73% | +1.8 +- 1.8 | +0.9 +- 1.3 |
| | control | 2869 | 2986 | 0.96 | 76% | | |
| `RETREAT=alt` | retreat | 2725 | 2686 | 1.01 | 72% | +0.7 +- 1.3 | +0.4 +- 1.0 |
| | control | 2681 | 2728 | 0.98 | 80% | | |
| `COVER=alt RETREAT=alt` | both on | 3026 | 2894 | 1.05 | 74% | +2.1 +- 1.7 | +2.8 +- 1.3 |
| | control | 2891 | 3027 | 0.96 | 82% | | |

- VERIFIED (measured, the table): pooled, cover and the retreat
  together give a side about 2.8 kills a match more than the other
  (some 2 standard errors, K/D 1.05 to 1.08); each alone stays within
  one standard error of nothing. Both are on by default, so the shipped
  game has the pair.
- VERIFIED (measured, both tables): the retreat cuts the share of
  wounded bots dead within 5 s by 8 to 10 points in all four runs.

### 6.3 Team play: `sd` and `re` on the ten 1.1 maps that run both, seeds 1..=6 (60 matches a row)

`bot-objectives.md` 9's table compared two builds with `team_tally`;
`TEAMPLAY=` now switches the same rules per side inside one build, the
retreat and cover on for everyone. The attackers are, per map, the team
that plants or picks up (axis on mp_pavlov in `sd`, on mp_dawnville and
mp_railyard in `re`; allies elsewhere). Wins are rounds.

VERIFIED (measured, ours, `bot_match`, 2026-10-21):

| Run | att W | def W | att share | plants / pickups | defuses / captures | stalled |
|---|---|---|---|---|---|---|
| `sd TEAMPLAY=none` | 104 | 178 | 37% | 200 | 85 | 408827 |
| `sd TEAMPLAY=all` | 166 | 107 | 61% | 229 | 52 | 474275 |
| `re TEAMPLAY=none` | 142 | 197 | 42% | 233 | 25 | 363031 |
| `re TEAMPLAY=all` | 137 | 194 | 41% | 219 | 46 | 346276 |

With `TEAMPLAY=alt` (team play on one side, attack or defence by seed)
the team-play side took 168 `sd` rounds to the other's 116 and 179 `re`
rounds to 157.

- VERIFIED (measured, the table): `sd`: team play lifts the attackers from 37% to 61% of rounds, the
  one-site rule as `bot-objectives.md` 9 found (47% to 62% there, a
  build without cover or the retreat).
- VERIFIED (measured, the table): `re`: captures nearly double (25 to
  46), as there (9 to 18), but the attackers' round share does not move.
  INFERRED: most rounds end on the clock or in a wipe, not a capture.

### 6.4 United Offensive: `tdm`, mp_cassino and mp_kursk, seeds 1..=8 (16 matches a row)

`UO=1`, `COD_DIR` at a UO install.

VERIFIED (measured, ours, `bot_match`, 2026-10-21):

| Run | Side | Kills | Deaths | K/D | Life s | Wounds | Dead in 5 s | Margin |
|---|---|---|---|---|---|---|---|---|
| stock | both | 601 | 626 | 0.96 | 44.8 | 417 | 75% | -1.1 +- 1.8 |
| `COVER=none RETREAT=none` | both | 657 | 702 | 0.94 | 39.0 | 470 | 81% | -1.6 +- 2.1 |
| `COVER=alt` | cover | 324 | 332 | 0.98 | 43.3 | 228 | 76% | +0.8 +- 2.6 |
| | control | 321 | 344 | 0.93 | 40.7 | 242 | 78% | |
| `RETREAT=alt` | retreat | 341 | 306 | 1.11 | 44.1 | 207 | 71% | +3.0 +- 3.0 |
| | control | 281 | 354 | 0.79 | 37.7 | 237 | 84% | |

- VERIFIED (measured, the table): kills and deaths differ here by
  suicides and team kills; sixteen matches leave every margin within
  about one standard error of nothing; the retreat's share of wounded
  deaths falls 13 points, as on 1.1.
- VERIFIED (measured, the table's runs): UO's axis bots started three to
  four times the dry retreats its allies did (958 against 292 in the
  stock run; 1.1's two sides start 112 and 149), nearly all of them on
  mp_kursk (axis 942, allies 278; mp_cassino 18 and 26). Logging each
  start showed one dry clip starting a retreat every other tick through
  its whole reload (`busy_ms` 2400, 2300, ... 30): a retreat that found no
  cover ended on its next tick (`cover_spot` `None`), and the tick after
  that started it again on the same clip, re-running the cover search.
  mp_kursk is open ground, so a reload there rarely finds cover, and the
  axis carry the 5-round kar98k (423 of the 1264 logged starts), the
  gewehr43 (259) and the luger (147).
- VERIFIED (measured, same command and seeds, 2026-10-21): with one dry
  retreat per clip (section 5) the dry starts fall to axis 69, allies 48
  (mp_kursk 51 and 22; mp_cassino's 18 and 26 unchanged, and its
  matches identical line for line). Kills, deaths and lives move inside
  the noise (mp_kursk 93 kills to 99): a retreat with no cover never
  changed the keys (`retreat_keys` returns on `None`), so the flapping
  cost counts and cover searches, not play. The axis side's remaining
  1.4x is its weapons: of the 69 axis starts, 28 were on the kar98k
  (5 rounds) and 14 on the luger, against the allies' 71-round ppsh on
  mp_kursk. No weapon or ammo read was wrong: the logged `clipSize`s
  match the weapon files in the `pakuo` paks (kar98k 5, gewehr43 10).
- VERIFIED (measured, 6.1's stock run again with the fix): 1.1's dry
  starts go from 261 to 241 (allies 116, axis 125); kills 5237 to 5265,
  lives 24.8 s to 24.7 s. 1.1's four maps nearly always had cover, so
  little flapped there. The tables above predate the fix.
