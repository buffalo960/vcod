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
(section 5, `test_bot_retreat`), `TEAMPLAY=` (`bot-objectives.md` 9,
`test_bot_team_play`: off, a bot ranks by slot, every attacker keeps by
an `re` carrier, nobody covers a planter, plans pay no crowd cost, a
carrier fights as any bot and every noise draws the bot, the brain
before team play; section 7) and `AWARE=` (section 8, `test_bot_aware`:
off, a bot sees all round, fights the nearest enemy in sight, pays the
full reaction and error on every switch and ignores hits from out of
sight) and `FOOTWORK=` (section 9, `test_bot_footwork`: off, a bot
stands still on its sights, never backs off and never lies down) each
take `all`
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
| answered | `D;`, bodies | Of the hits a bot takes from an enemy (one per attacker until it is answered or 1 s passes), the share it answers within 1 s: a tick after the hit's with `BotBody::firing` and that attacker as `BotBody::target`. Fire at someone else does not count; a bot killed inside the second has not answered. Printed with the hit count. |
| blindsided | `K;`, bodies | Of a bot's deaths to an enemy, the share whose killer was never its `BotBody::target` that life, reaction delay included. Printed with the death count. |
| mate, packed | bodies | How close a team keeps. Each bot-tick playing with a teammate playing adds its distance to the nearest one (3D, units): `mate` is the mean. `packed` is the share of those bot-ticks with two or more teammates within 400 units, the bot and two others in one room. Bots only; a corpse or spectator is left out. `dm` has no teammates and prints 0. |
| grenades thrown | bodies | Frags let go (`BotBody::throws`, counted at the release; a throw put back is not one). |
| grenade hits, kills | `D;`, `K;` | Lines with means of death `MOD_GRENADE_SPLASH` on another player, by the thrower; kills are the `K;` ones. A frag that kills its own thrower is a suicide. |
| engagements, clip at start | bodies | `BotBody::engaged` going from off to on with a bullet weapon in hand, and the mean of its clip over its `clipSize` on that tick. |
| shots | bodies | Rounds fired: each tick's drop in the held clip while the weapon stays the same bullet gun (`weaponType bullet`, `BotBody::gun`). Grenades, rockets and a mounted MG's rounds are not counted. |
| hits | `D;`, `K;` | Lines on another player whose means of death is a bullet (`MOD_RIFLE_BULLET`, `MOD_PISTOL_BULLET`, `MOD_HEAD_SHOT`). A mounted MG's hits land here too; bots seldom use one. |
| accuracy | | Hits over shots. |
| sight to shot | bodies | From a tick whose `BotBody::target` comes up with none the tick before (the reaction delay included) to the first tick that fires a round while that target is still the one tracked, in ms; mean and median. A switch from one target straight to another starts no count. "Fired on" is the share of those acquisitions that got a round before the target went (died, hid past `forget_ticks`, or the bot died). |
| walking ticks, turn mean, snaps | bodies | The walking view (section 11). A bot-tick counts when `BotBody::walking` holds on it and the tick before: playing, on the ground, off the rungs, steering at its waypoint with no unstick spell, and no target tracked (`Bot::engaged` false). `turn mean` is the mean of the view's yaw change over those ticks in degrees (wrapped to ±180); `snaps` counts the ticks it turned more than 30 degrees, and their share of the walking ticks. |

Each match prints one line, then the totals by team, by skill level and,
with an arm one-sided, by arm; with several maps a block per map comes
first. Each row prints five lines: the log's columns, the bodies', the
grenades and the clip, the aim, then the walking view.

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
| reaction, ticks (min + rand(spread)) | 8 + 6 | 4 + 4 | 3 + 4 | 2 + 2 |
| turn per tick, degrees | 10 | 15 | 22 | 30 |
| aim error start, floor (units) | 64, 5 | 36, 4 | 32, 2.5 | 24, 1.5 |
| error decay per tick | 0.91 | 0.85 | 0.84 | 0.80 |
| error doubles at range | 900 | 1000 | 1300 | 1600 |
| error while moving at run speed | x1.9 | x1.75 | x1.5 | x1.3 |
| aim trails a crossing target by (s) | 0.09 | 0.06 | 0.045 | 0.03 |
| footwork chance a spell on the sights | 0.25 | 0.45 | 0.6 | 0.75 |
| forget a target, ticks | 16 | 20 | 30 | 40 |
| remember a spot, a noise, ticks | 70, 140 | 100, 200 | 140, 260 | 180, 300 |
| hearing scale | 0.75 | 1 | 1.25 | 1.5 |
| sight cone for a new target, degrees across | 100 | 120 | 140 | 160 |
| walking view's turn per tick, degrees (section 11) | 18 | 24 | 30 | 36 |

Normal's reaction (5 + 6), error start (48) and decay (0.88) changed with
section 9; the measurements below are from before it.

The miss is `(error + across * track_lag) * (1 + range / error_range) *
moving`, `across` the target's speed across the line and `moving` `1 +
(error_moving - 1) * (speed / 190)^2` (section 9). Until section 9 the
trigger only went down while the gun's line passed within `HIT_RADIUS`
(10 units) of the target's chest (`Bot::trigger`), so the error floor set
the range a bot could fire at at all: `floor * (1 + range / error_range) *
moving < 10`. It still sets where the trigger goes down every tick; past
it the trigger goes down by chance. A first easy preset with a floor of 10 never met it: in
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
| `AWARE=alt` (section 8) | aware | 2882 | 2330 | 1.24 | 27.7 | 1719 | 72% | 1882 | +8.7 +- 1.6 |
| | control | 2326 | 2890 | 0.80 | 22.6 | 2188 | 75% | 2035 | |
| stock, noise answering (section 7) | both | 4667 | 4671 | 1.00 | 28.2 | 3446 | 72% | 1918 | -0.2 +- 1.1 |
| `TEAMPLAY=alt`, answering one side | teamplay | 2313 | 2607 | 0.89 | 25.2 | 1860 | 76% | 2065 | -4.5 +- 1.2 |
| | control | 2603 | 2314 | 1.12 | 27.9 | 1717 | 72% | 1399 | |

The last three rows came later, on branch `feat/bot-spread` (section 7),
where the stock run before the change gave 5265 kills and lives of
24.9 s (the dry-start fix below). In 1.1 `tdm` team play is only the
noise answering, so `TEAMPLAY=alt` pits it against the old rule.

The skill row is two runs: `AXIS_SKILL=2` on seeds 1..=8 and
`ALLIES_SKILL=2` on 9..=16 (the other side at 1), summed. The `AWARE`
row is a later tip (branch `feat/bot-awareness`), whose `AWARE=none` run
prints 5265 kills, the stock run's figure after the dry-retreat fix below.

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

### 6.5 Grenades and the tactical reload (2026-10-21)

`bot_match` 1.1 (`COD_DIR` at the 1.1 install), seeds 1..=4, 12000 ticks:
`tdm` on mp_carentan, mp_harbor, mp_depot and mp_brecourt with 12 bots
(16 matches, every one ended on the score), `dm` on mp_carentan,
mp_harbor and mp_depot with 10 (12 matches, none ended). Before is the
harness commit, after the branch tip.

What changed (`Bot::throw_spot`, `Bot::think_cook`, `bots::notices_frag`,
`Server::lob_probe`):

- A throw no longer goes at an enemy the gun can hit. It goes at one in
  sight no round has gone at for 1.5 s (`COVER_THROW_TICKS`: behind cover,
  or never lined up), or at the spot a seen enemy was lost at 0.2 to 3 s
  ago, if he was slower than 100 units/s then (`LOST_STILL`), 250 to 600
  units off, from a body under 60 units/s with its eye settled. The 1-in-4
  roll is gone and the cooldown is 10 s (was 20).
- `lob_probe` finds no lob while a live teammate stands within the frag's
  `explosionRadius` of the target.
- A throw under way follows the enemy it started at, and a lost spot's
  throw follows nobody: one held for one enemy went at another who came
  into sight in the hold, released on the old view, and killed its
  thrower.
- A 1.1 or 1.5 frag (no `cookOffHold`) still without a clear lob, or with
  the view off it, once the 16-tick wait is out is put back: the cmd that
  lets the trigger go asks for the rifle, and the switch cancels the
  pullback with the clip unspent (`cod11-combat.md` 1.14). A UO frag,
  its fuse running, is still let go.
- Another's frag is run from only once noticed: in a clear line and
  within 60 degrees of the view's yaw, or within 150 units. A bot's own
  is always run from.
- A clip under half reloads once no enemy has been in sight for 1 s
  (`QUIET_RELOAD_TICKS`), retreat or not (before, only a dry clip did
  outside a retreat); not the pistol, which goes back by then.

VERIFIED (measured, ours, `bot_match`, 2026-10-21):

| Run | Kills | Suicides | Life s | TTK mean / median ms | Wounds | Dead in 5 s | Retreats dry | Stalled | Thrown | Hits | Grenade kills | Clip at start |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `tdm` before | 2908 | 4 | 22.0 | 1270 / 0 | 2160 | 76% | 102 | 65863 | 1941 | 39 | 5 | 94% |
| `tdm` after | 3001 | 0 | 20.2 | 1266 / 0 | 2246 | 76% | 61 | 81750 | 720 | 53 | 7 | 96% |
| `dm` before | 3116 | 0 | 20.1 | 1253 / 300 | 2333 | 74% | 132 | 45053 | 1729 | 9 | 1 | 90% |
| `dm` after | 3163 | 0 | 19.7 | 1335 / 300 | 2376 | 75% | 75 | 45452 | 1052 | 9 | 0 | 93% |

- VERIFIED (measured, the table): a `tdm` frag hits someone 7.4% of the
  time against 2.0%, with a third of the throws; grenade kills stay
  rare (7 of 3001). Dry retreats fall by 40% in both gametypes, and the
  clip a fight starts on rises 2 to 3 points.
- VERIFIED (measured, ours, the same `dm` seeds on mp_carentan and
  mp_harbor, each throw and probe logged): with only the new targets, 7
  of 10 grenade kills were the thrower's own. All seven released with no
  clear lob after the wait: one held for an enemy who left while another
  came into sight; the rest started on the run or crouched, slid or
  stood up through the switch to a spot where a wall stopped every lob,
  and the hold let go into it. With the rules above, none.
- VERIFIED (measured, ours, `tdm` before the noticing rule): run from
  wherever it lay, no frag killed anyone: 718 thrown, 0 grenade kills.
- `dm` frags still hit almost nobody (9 of 1052). INFERRED: with no
  teams every enemy in sight is a candidate, so a throw at one is in
  front of him and he notices it.
- `tdm`'s `stalled` count rises 24% while `dm`'s holds. INFERRED: bots
  stand through a throw's switch and hold, and through a reload they
  did not do before; read it against this row, not the older tables.

A probed lob is flown on to its burst (2026-10-21). `Server::lob_probe`
takes a lob only when the frag, flown by the missile code from the release
to the end of its fuse (`Missiles::predict_flight`: `Missile::step` against
the world, no bodies or models), stays farther than its `explosionRadius`
from the thrower's box at every frame the fuse can end on: from
`COOK_MOST_MS` (2250 ms) short of the full fuse for a UO cook, at the full
fuse elsewhere. The arc check alone ends at the first contact.

- VERIFIED (measured, ours, release, stock UO paks, `ship_stalls` with a
  per-tick dump, seed 19 `tdm` on 2180b31d): a stick grenade lobbed from
  (2583, -925, 0) at an enemy 550 off met a wall about 310 out, past the
  arc check's 60 per cent, came back and rolled to (2558, -1126, 2); the
  wounded thrower ran from it to (2518, -791, 4), 337 off, and died of its
  9 points.
- VERIFIED (`weapons/mp/stielhandgranate_mp` in `pakuo04.pk3`):
  `explosionRadius 350`, `explosionInnerDamage 120`,
  `explosionOuterDamage 5`, `fuseTime 4`.
- VERIFIED (measured, ours, release, stock UO paks, burst test off and
  on): `ship_stalls` with `UO=1 MAP=mp_cassino GAMETYPE=tdm SHOOT=1
  TICKS=1800` over seeds 1 to 63 odd, own frags 1 to 0, frag kills 4 to
  3, kills 502 to 507; `bot_match` with `UO=1
  MAPS=mp_cassino,mp_kursk BOTS=10` over seeds 1 to 8, thrown 176 to 151,
  hits 74 to 46, grenade kills 7 to 5, kills 1613 to 1586. Its `suicides`
  (43 and 47) counts every self-kill and is not split by cause.

### 6.6 Aim and footwork: `tdm` and `dm`, 12 and 10 bots (2026-10-21)

Section 9's change, before (master d82876e1) and after on the same seeds.
`tdm`: mp_carentan, mp_harbor, mp_depot, mp_brecourt, `BOTS=12`, seeds
1..=8 (32 matches a row, every one ended at the score limit). `dm`:
mp_carentan, mp_harbor, mp_depot, `BOTS=10`, seeds 1..=4 (12 matches, none
ended). "Sight" is sight to shot, mean / median ms; "fired" the share of
acquisitions fired on (section 1). Stalled is per 1000 match ticks.

VERIFIED (measured, ours, `bot_match`, 2026-10-21):

| Run | Kills | Allies K/D | Life s | Wounds | Dead in 5 s | TTK mean ms | Shots | Accuracy | Sight ms | Fired | Stalled |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `tdm` before | 5903 | 1.01 | 21.4 | 4354 | 75% | 1361 | 24084 | 69.6% | 1701 / 1450 | 19% | 459 |
| `tdm` after | 5870 | 1.04 | 21.6 | 4312 | 73% | 1483 | 45943 | 37.4% | 963 / 750 | 32% | 555 |
| `dm` before | 3116 | | 20.1 | 2333 | 74% | 1253 | 15562 | 52.1% | 2228 / 1900 | 50% | 313 |
| `dm` after | 3237 | | 19.2 | 2440 | 72% | 1487 | 27289 | 32.1% | 1185 / 850 | 66% | 367 |
| `dm` after, `FOOTWORK=none` | 3284 | | 19.0 | 2478 | 73% | 1542 | 27366 | 32.6% | 1192 / 850 | 68% | 318 |

`FOOTWORK=alt`, `tdm` as above (32 matches). Margin as in 6.1, the
footwork side's score less the other's per match:

| Run | Side | Kills | Deaths | K/D | Life s | Dead in 5 s | Accuracy | Sight ms | Margin |
|---|---|---|---|---|---|---|---|---|---|
| after | footwork | 2858 | 2963 | 0.96 | 20.4 | 73% | 39.3% | 1022 / 800 | -3.1 +- 3.7 |
| | control | 2958 | 2858 | 1.03 | 21.2 | 74% | 38.1% | 976 / 800 | |
| strafe only (no back-off, no prone) | footwork | 2962 | 3024 | 0.98 | 21.8 | 73% | 38.3% | 976 / 750 | -1.8 +- 2.8 |
| | control | 3019 | 2967 | 1.02 | 22.2 | 72% | 36.9% | 977 / 750 | |
| moving error linear in speed | footwork | 2816 | 2986 | 0.94 | 21.4 | 75% | 38.6% | 1032 / 800 | -5.2 +- 4.1 |
| | control | 2981 | 2817 | 1.06 | 22.7 | 72% | 37.9% | 1024 / 800 | |

- VERIFIED (measured, the tables): sight to shot falls from a median
  1450 ms to 750 ms in `tdm` and 1900 to 850 in `dm`, and a fresh target
  is fired on two thirds more often (19% to 32% in `tdm`). Accuracy falls
  from 70% to 37%: rounds go out before the aim settles, and an
  automatic fires twice the rounds for the same kills.
- VERIFIED (measured, the tables): kills, lives, TTK and the share of
  wounded bots dead within 5 s move a few percent; the score stays even
  between the teams (allies less axis +3.2 +- 3.6 a match after, +1.2 +-
  3.4 before). With every weapon class given one reach of 3 the allies
  led by +6.8 +- 4.1, so the class reaches are not what tilts it.
- VERIFIED (measured, the tables): the footwork side gives up 3 kills a
  match, inside one standard error; standing still is no longer clearly
  better. With the moving error linear in speed it gave up 5. Neither
  back-off nor prone alone accounts for the gap (-6.2 +- 4.2 without the
  back-off, -4.8 +- 3.7 without prone, both on the linear build).
- VERIFIED (measured, the merged tip with team spread, grenades and
  awareness, `tdm` seeds 1..=4, 16 matches): 2899 kills, allies k/d
  1.09, life 26.5 s, 74% of wounded dead in 5 s, TTK mean 1803 ms,
  accuracy 32.6%, sight to shot 960 / 750 ms with 56% of acquisitions
  fired on, 95% of hits turned on and 45% answered, 10% of deaths
  blindsided, stalled 514 per 1000 ticks.
- VERIFIED (measured): `stalled` rises a fifth in `tdm`; cover holds rise
  by a quarter (921k to 1150k bot-ticks) and dry retreats from 193 to 507.
  INFERRED: twice the rounds empty twice the clips, and a dry clip holds
  a cover spot to reload (section 5).

## 7. A side spread over the map (2026-10-21)

A playtest report: with many bots in `tdm` nearly a whole team moved
together. `bot_match`'s `mate` and `packed` columns (section 1) count
it.

### 7.1 Why

- VERIFIED (measured, ours, `bot_match`, 12 bots, `tdm`, mp_carentan,
  mp_harbor, mp_brecourt, mp_depot, seeds 1..=6, 6000 ticks): 72% of
  playing bot-ticks had two or more teammates within 400 units, the
  nearest one 210 units off on average. The packing did not loosen
  over a life: 68% in a life's first 5 s, 73-74% from 5 s on (that
  split measured with the crowd cost below on, which moved nothing).
  INFERRED: `tdm.gsc`'s `getSpawnpoint_NearTeam` puts a respawn near
  the living teammates, and the brain kept the team there.
- VERIFIED (same runs, the brain's goal tallied each tick with a
  temporary counter): a seen enemy was the goal 43% of bot-ticks (83%
  of those packed), a remembered sighting 14% (79%), a heard noise 16%
  (57%), the roam 11% (33%).
- VERIFIED (same runs): with no noise heard at all the share packed fell
  to 29% and the nearest teammate stood 493 units off, and the seen
  enemy's ticks fell to 42% packed. INFERRED: the noise was what
  gathered a side. `bots::loudest` gives every bot the noise loudest to
  it out of every shot within `HEAR_GUNFIRE` (2000) units, teammates'
  shots included, so a whole side within 2000 units of a fight ran to
  the same spot, met the enemy there as one, and its shots drew the
  rest (`Server::bot_think`'s view pass, `view.noise`).
- VERIFIED (same runs): the route spreading of `bot-objectives.md` 9
  (`team::crowd`) switched on for `tdm` moved nothing (72.4% packed,
  mate 206): the team's paths shared a goal, not a corridor.

### 7.2 The rule

A bot on a side with team play on hears a noise only when fewer than
`team::ANSWERS` (2) of its playing teammates stand nearer it, the
shooter who made it not counted (`team::answers`, applied in the view
pass before `bots::loudest`). The two nearest answer a shot; the rest
keep to their roam or their own noise. Off with `TEAMPLAY=none` (a bot
then hears every noise, as before). `dm` has no sides and is unchanged.

Variants measured on 7.1's runs (kills over the 24 matches, share
packed, mean nearest teammate), each VERIFIED (measured, ours,
`bot_match`). Rows marked * ran with the crowd cost on in `tdm` as
well, which alone moved nothing (with it, the shipped rule read 2519,
35.9%, 417):

| Variant | Kills | Packed | Mate |
|---|---|---|---|
| before | 3404 | 72.1% | 210 |
| no noise at all * | 2241 | 29.3% | 493 |
| enemy shots only * | 3296 | 62.2% | 272 |
| the 3 nearest answer | 2823 | 51.4% | 335 |
| the 2 nearest answer (shipped) | 2580 | 39.5% | 398 |
| the 2 nearest answer, the others go 500 units to one side of it * | 3127 | 58.8% | 262 |
| the 2 nearest answer, the others 900 units to one side * | 2577 | 44.3% | 326 |

- VERIFIED (the table): every variant traded kills for spread along one
  curve; sending the others to a flank point bought nothing over
  dropping the noise for them. The plain rule stayed.

### 7.3 Before and after

`tdm`, mp_carentan, mp_harbor, mp_brecourt, mp_depot, 6000 ticks a
match, team play on for both sides. VERIFIED (measured, ours,
`bot_match`, before on master d82876e1, after on `feat/bot-spread`):

| Bots, seeds | Run | Kills | Life s | Mate | Packed | Stalled |
|---|---|---|---|---|---|---|
| 6, 1..=8 | before | 1712 | 28.9 | 572 | 19.1% | 28361 |
| | after | 1780 | 27.6 | 570 | 19.4% | 28073 |
| 8, 1..=16 | before | 5265 | 24.9 | 355 | 42.7% | 120746 |
| | after | 4667 | 28.2 | 472 | 29.9% | 85739 |
| 12, 1..=6 | before | 3404 | 21.7 | 210 | 72.1% | 71300 |
| | after | 2580 | 28.6 | 398 | 39.5% | 34176 |
| 12, 7..=12 | before | 3560 | 20.3 | 199 | 75.2% | 66364 |
| | after | 2529 | 29.3 | 397 | 40.8% | 38703 |
| 16, 1..=6 | before | 4419 | 20.2 | 139 | 85.8% | 87379 |
| | after | 3336 | 29.4 | 354 | 47.2% | 47855 |

Per map, 12 bots, seeds 1..=6 (before, after):

| Map | Kills | Mate | Packed |
|---|---|---|---|
| mp_carentan | 871, 699 | 167, 292 | 77.1%, 51.0% |
| mp_harbor | 920, 701 | 177, 335 | 76.8%, 46.1% |
| mp_brecourt | 781, 537 | 309, 612 | 61.5%, 22.7% |
| mp_depot | 832, 643 | 185, 352 | 73.0%, 38.7% |

Objective levels, 8 bots, mp_carentan, mp_harbor, mp_brecourt,
mp_depot, mp_dawnville, seeds 1..=6 (30 matches a row):

| Run | Kills | Wins axis, allies | Actions | Mate | Packed |
|---|---|---|---|---|---|
| `sd` before | 856 | 64, 86 | 120 plants, 31 defuses | 183 | 48.6% |
| `sd` after | 842 | 63, 82 | 123 plants, 31 defuses | 174 | 48.6% |
| `re` before | 988 | 96, 77 | 129 pickups, 26 captures | 210 | 63.2% |
| `re` after | 1020 | 105, 71 | 131 pickups, 23 captures | 204 | 63.7% |

- VERIFIED (the tables): from 8 bots up a side spreads: at 12 the
  share packed halves (72-75% to 40-41%) and the nearest teammate
  stands twice as far off; at 16, 86% to 47%. Three a side never had
  two others near and does not change.
- VERIFIED (the tables): the cost is fights. With 12 bots kills fall
  24-29% and lives run 7-9 s longer; with 8, 11%. K/D, the share of
  wounded bots dead in 5 s (71-75% against 74-75%) and TTK barely move.
  `stalled` falls 29-52%, INFERRED: fewer bots holding cover in one big
  fight.
- VERIFIED (the tables): `sd` and `re` keep their plants, defuses,
  pickups, captures and wins within a few of before; their bots gather
  at the objectives, so the spread does not move there.
- VERIFIED (measured, `TEAMPLAY=alt`): a spread side loses to one that
  converges: 8 bots, seeds 1..=16, -4.5 +- 1.2 a match (K/D 0.89 against
  1.12, section 6.1); 12 bots, seeds 1..=12, -11.5 +- 2.0 (0.82 against
  1.21); the 3-nearest rule at 12 bots, -9.7 +- 2.4. INFERRED: a whole
  side arriving at once outnumbers whoever it meets. Both sides play by
  the same rule in a real match, and a human now meets two or three bots
  at a fight instead of a side.
## 8. Target choice and sight (2026-10-21)

Before this, `Server::bot_enemy` handed a bot the nearest enemy within
`SHOOT_RANGE` whose chest or eye its eye line reached, all round, picked
afresh every `ENEMY_REFRESH_MS` (100); `Bot::track` gave every change of
slot the full reaction delay and `error_start`, and a hit told the brain
nothing. A bot between two enemies at like range swapped targets, paying
the delay each time, and one shot from out of sight ignored it.

Now (`crates/server/src/bots.rs`, `server.rs`, `spectate.rs`):

- `ClientSim::last_hit`: each `SimOp::Damaged` with another client as
  attacker and damage above 0 records his slot, the frame's `sv_time` and
  the damage direction; a spawn clears it. `Server::bot_hurt` hands it to
  the brain as `BotView::hurt` for `HURT_MS` (2 s) while the attacker
  plays.
- `bots::pick_enemy`: the attacker of that hit when the eye line reaches
  him, else a new enemy nearer than `RETARGET_RATIO` (0.6) of the tracked
  target's range, else the tracked target while in sight, else the
  nearest.
- A sight cone, `Skill::fov_deg` across (100, 120, 140, 160 by level,
  section 3), about the view (`bots::in_sight_cone`, on the chest), for
  every other enemy; the tracked target and the attacker pass it on any
  bearing. A bot in a vehicle seat sees all round: its view is the
  seat's, and with the cone the tank driver of
  `a_bot_fires_rockets_at_an_enemy_tank` (`tests/bot_vehicles.rs`) no
  longer held still for the rocket man behind his hull, and the tank
  survived the rocket.
- `Bot::track`: a target let go of within `forget_ticks` (`Bot::prior`)
  is taken again on a third of the drawn reaction and its old error times
  1.25, between `error_floor` and `error_start`; the attacker of the hit
  on half the reaction and 0.75 of `error_start`; anyone else on the full
  ones, as before.
- `Bot::face_hurt`: with nothing in sight, a hit in the last
  `HURT_LOOK_MS` (1.5 s) turns the view back along the damage direction
  at `turn_deg`, the path's keys rotated to keep the path; the cone
  sweeps over the shooter.
- Hearing has no cone (`bots::loudest`, bot-navigation.md section 4): a
  shot behind the bot pulls it as one ahead does.
- `Bot::aware` off (`AWARE=`, `Server::test_bot_aware`) restores all of
  the before.

### 8.1 Before and after

`AWARE=none` replays master line for line: both sets below print master's
match lines (kills, deaths, damage, ticks, stalls) with it, so it is the
before and supplies the `turned` column master's harness lacks.

VERIFIED (measured, ours, `bot_match`, 2026-10-21): `tdm`, 12 bots,
mp_carentan, mp_harbor, mp_depot and mp_brecourt, seeds 1..=4 (16
matches, all ended); `dm`, 10 bots, mp_carentan, mp_harbor and mp_depot,
seeds 1..=4 (12 matches, none ended in 12000 ticks):

| Run | Kills | K/D | Life s | TTK mean / median ms | Wounds | Dead in 5 s | Turned | Answered | Blindsided | Cover hold | Stalled |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `tdm` before | 2908 | 1.00 | 22.0 | 1270 / 0 | 2160 | 76% | 63% | 17% of 4324 | 12% of 2908 | 483913 | 65863 |
| `tdm` after | 2935 | 1.00 | 22.0 | 1357 / 0 | 2179 | 74% | 92% | 19% of 4237 | 13% of 2935 | 642973 | 127392 |
| `dm` before | 3116 | 1.00 | 20.1 | 1253 / 300 | 2333 | 74% | 92% | 37% of 3726 | 2% of 3116 | 635254 | 45053 |
| `dm` after | 2895 | 1.00 | 21.7 | 1136 / 350 | 2275 | 79% | 96% | 28% of 3406 | 14% of 2895 | 574948 | 30322 |

`Turned` is the share of hits whose attacker the bot targeted within 1 s;
`answered` (section 1) adds the trigger down.

- VERIFIED (measured, the table): in `tdm` the attacker becomes the
  target within 1 s after 92% of hits, from 63%.
- VERIFIED (measured, the table): `answered` barely moves (17% to 19% in
  `tdm`). INFERRED (`Bot::trigger`): the trigger waits until the aim's
  miss falls inside the 10-unit cone, and 0.75 of `error_start` (36
  units) decays under it in about 13 ticks at 360 units standing, longer
  moving; the turn and that wait, not the choice, hold the answer past a
  second.
- VERIFIED (measured, the table): in `dm` the share blindsided goes from
  2% to 14% and `answered` falls from 37% to 28%. INFERRED: before, a
  `dm` bot saw every player all round and was on the nearest; with the
  cone a shot from behind lands before a 180 degree turn (12 ticks at 15
  degrees a tick).
- VERIFIED (measured, the table): `tdm` stalls nearly double, `dm` stalls
  fall a third. On the branch with the parts switched off one at a time
  (mp_carentan, mp_depot, mp_brecourt, `tdm`, 12 bots, seeds 1..=4),
  stalled bot-ticks with no target fell from 6793 to 2903 and those
  holding cover against a target rose from 41k to 80k, all on versus all
  off; on seeds 1..=8 the reacquire and hit-by discounts each raised the
  total alone (124k off, 141k and 143k each alone, 178k both). INFERRED:
  the extra stalls are bots fighting longer from cover, not lost bots.
- VERIFIED (measured, release, stock UO paks): `uo_bots_fight_and_sprint_in_tdm`
  seeds 1 and 19 stalled a bot on mp_cassino's roofs (bot-objectives.md
  10.6) on this branch alone; merged with section 7's noise answering and
  6.5's grenade rules seed 1 passes, and the replay list plays 13 for 19,
  whose game there ends on a bot's own frag.

### 8.2 One-sided

VERIFIED (measured, ours, `bot_match`, 2026-10-21): `AWARE=alt`, `tdm`,
12 bots, the four maps above, seeds 1..=16 (64 matches, all ended):

| Arm | Kills | Deaths | K/D | Life s | Dead in 5 s | Turned | Answered | Blindsided | Wins |
|---|---|---|---|---|---|---|---|---|---|
| aware | 6386 | 4351 | 1.47 | 26.7 | 71% | 91% | 21% | 13% | 62 |
| control | 4344 | 6399 | 0.68 | 18.0 | 75% | 59% | 13% | 14% | 2 |

- VERIFIED (measured, the table): the aware side scores 31.9 +- 1.9 more
  a match and wins 62 of 64, as large as a skill level (6.1's `vc_bot_skill
  2` row, +31.9 +- 1.6); in 6.1's shorter eight-bot matches it is +8.7 +-
  1.6.
- VERIFIED (measured, the table): a blindsided death is no likelier on
  the aware side (13% against 14%) though it sees a cone. INFERRED: it
  turns on its attacker and keeps its target where the control flips to
  the nearest.

## 9. Aim and footwork

`Bot::engage`, `Bot::trigger` and `Bot::footwork` in
`crates/server/src/bots.rs`; measured in 6.6.

Before this, the aim error only shrank with time on the target, and the
trigger waited until the gun's miss fell inside the 10-unit cone: a bot
paused about a second on a fresh target, then hit nearly every round.
A target's movement added nothing, so strafing did not help against a
settled bot, and a bot on its sights never moved: the x1.75 error while
moving cost more than anything moving bought.

### 9.1 The miss

The aimed point is off the target's chest by

`(error + across * track_lag) * (1 + range / error_range) * moving`

- `error` starts at `error_start` on a fresh target and shrinks by
  `error_decay` a tick to `error_floor`, as before.
- `across` is the target's speed across the line from the bot (units/s,
  `EnemyView::velocity` less its part along the line), and `track_lag`
  (seconds, by skill: 0.09, 0.06, 0.045, 0.03) how far the aim trails it.
  At normal skill a target running across at 190 adds 11.4 units, past
  the cone on its own.
- `moving` is `1 + (error_moving - 1) * (speed / 190)^2`. VERIFIED
  (combat doc 2.1, `game.mp.i386.so` 0x385e8): retail's
  `PM_AdjustAimSpreadScale` adds nothing to a settled sight
  (`pmove::weapon::adjust_aim_spread_scale`), so this penalty is the
  brain's own. INFERRED (the formula, a half-key strafe on the sights at
  about 45 units/s): such a strafe costs 4% at normal skill, where linear
  in speed it cost 18%; the footwork side lost more on the linear build
  (6.6).

### 9.2 The trigger

Inside the cone the trigger goes down as before. Past it, it goes down
with chance `(reach - r) / (reach - 1)` a tick, `r` the miss in cones:
`REACH_AUTO` 4.5 for an automatic, `REACH_SEMI` 3 for a semi-automatic,
`REACH_BOLT` 2 for a bolt rifle (`fireTime` 500 ms or more), whose miss
costs a rechamber. An automatic's burst keeps the trigger down while the
miss stays inside its reach. An automatic inside `AUTO_HIP_RANGE` (700)
also fires while its sights come up; the round takes the hip cone, as
retail's spread is the hip one until `fWeaponPosFrac` reaches 1
(`game::combat::spread_deg`). Everything else still waits for the
sights and a settled eye.

Normal skill's reaction went from 5 + rand(6) ticks to 4 + rand(4), its
error start from 48 to 36 and its decay from 0.88 to 0.85, between easy
and hard as before. VERIFIED (measured, ours, a temporary per-tick
trace of the trigger's gates on mp_harbor `dm`, seed 1, 4000 ticks,
before the reaction change): a fresh target's time before the first
round was about 8.7 ticks of reaction, 5 waiting on the sights and 4 on
the aim. INFERRED: the sights' wait is the weapon's `adsTransInTime`,
so the reaction was what was left to cut.

### 9.3 Footwork

Each strafe spell (8 to 23 ticks) rolls once against `Skill::footwork`
(0.25, 0.45, 0.6, 0.75). A spell that wins, while the bot is on its
sights:

- strafes at half key (`right` 63) instead of standing still; a scope
  still holds still;
- with a bolt rifle inside `BACK_OFF_RANGE` (300) walks backwards;
- past `PRONE_RANGE` (900), on a quarter of the chance, lies down for 2
  to 4 s instead of crouching; inside three quarters of that range the
  body gets up. A held cover spot, a sidestep off a teammate's line and
  a flee from a grenade release the stance.

Off the sights the strafe stays full and the crouch roll stays as it
was. `Bot::footwork` false (`FOOTWORK=` in `bot_match`,
`Server::test_bot_footwork`) skips the roll's effects: the sighted body
stands still, never backs off and never lies down.

### 9.4 What the UO bot gates found

VERIFIED (measured, ours, `tests/bots.rs` ignored tests at a stock UO
install, 2026-10-21). Before master's `fix/uo-bot-pins`, this change
made `four_shooting_uo_bots_end_a_bas_round` (mp_foy, seed 7) die of its
own panzerschreck off `Bot::think_armor`, and
`uo_tdm_games_that_stalled_or_never_sprinted` stall seeds 25 and 33 at
mp_cassino's axis roof pin, (4939, -1410, 280). That fix's muzzle check
clears the first. Merged with it, team spread and awareness, two pinned
games still stall, both passing on master alone:

- Seed 25: the same roof pin. The bot, standing and not engaged, keys
  forward at 115 toward the waypoint (4896, -1384, 280) and bounces
  between (4939, -1410) and (4957, -1405) for the whole window, never
  jumping and never at rest long enough for `Follower::rest_jump_fails`.
- Seed 17: on the steps at (1440, -700..-725) the waypoint flips each
  time the body crosses a step, between (1440, -725, 13) and (1440, -669,
  56) or (1441, -761, 25), and the bot walks back and forth over it.

Both are the follower's; the new fights only route bots into them. Both
are fixed on `fix/uo-bot-pins2` (bot-objectives.md 10.6, bot-navigation.md
"Jumps", "A goal that flickers"): seed 25 by a replan on landing under a
level edge, seed 17 by a plan parked for a goal that comes back. Seed 17's
cause is the goal, not the node pick: an enemy seen for a tick or two from
the step flipped `Bot::goal` and back, and each replan began at the node
behind the body.

## 11. The walking view and string-pulling (2026-10-21)

`Bot::walk_turn` in `crates/server/src/bots.rs` and `Follower::pull` in
`crates/server/src/nav.rs`; the follower's side is in
`bot-navigation.md` section 3, "String-pulling".

Before this a walking bot's waypoint was the plan's next node on the
32-48 unit grid, and its view took the yaw to it straight from `atan2`
every tick. Across a diagonal the grid's eight neighbours make a stair
of 45 degree kinks, and at every node the view jumped to the next one.

### 11.1 The rules

- The view turns at most `Skill::walk_turn_deg` a tick toward the
  path's heading (18, 24, 30, 36 by level: 360 to 720 deg/s). The keys
  are the path's keys turned into the view's frame (`rekey`'s turn) and
  scaled so the larger is 127, so the body runs along the path at the
  run's speed while the view comes round.
- Only on a plain run: on the ground, off the rungs, level view, full
  forward key and nothing else held, no leap's or creep's foot, no jump,
  run off, push off or unpin, no creep. Those keep the view the walk
  that proved them had (AGENTS.md, "Bots").
- A fight, a launcher on a vehicle, a bipod's linger, a hurt look, a
  cover peek, a recalled spot and a retreat's look set the view after
  the walk, by their own rules, from the tick they take it. The tick
  after one of them lets go the view takes the path's heading at once
  (`Bot::view_owned`: the last think's view was not the walk's), and the
  limit runs again from the tick after that (11.4).
- The waypoint is pulled one node further along the plan a tick when a
  straight walk from the body reaches it (bot-navigation.md section 3).

### 11.2 Before and after

VERIFIED (measured, release `bot_match`, `COD_DIR` at 1.1, 12 bots
`tdm` on mp_carentan, mp_harbor, mp_depot and mp_brecourt, 10 bots `dm`
on the first three, normal skill; master d3b03cb0 with the section 1
columns, then this change with 11.4's hand-back). Turn is the walking
view's mean yaw change a tick, snaps the walking ticks it turned more
than 30 degrees.

| run | kills | life s | accuracy | walking ticks | turn deg | snaps | stalled |
|---|---|---|---|---|---|---|---|
| tdm seeds 1-4 | 3016 / 2955 | 28.5 / 26.7 | 33.9 / 33.4% | 975 332 / 863 241 | 3.87 / 2.93 | 19 926 (2.04%) / 2 196 (0.25%) | 63 151 / 62 899 |
| tdm seeds 5-8 | 2908 / 2949 | 27.9 / 26.9 | 34.1 / 33.7% | 910 296 / 854 828 | 3.83 / 2.97 | 19 050 (2.09%) / 2 245 (0.26%) | 70 669 / 69 137 |
| dm seeds 1-4 | 3129 / 3124 | 19.9 / 20.0 | 33.9 / 33.3% | 521 357 / 517 329 | 4.53 / 3.75 | 10 780 (2.07%) / 3 328 (0.64%) | 33 834 / 31 890 |

Snaps fall eight times in `tdm` and three in `dm`. Most of those left
are hand-backs: a recalled spot's look or a hurt look ends on a tick the
bot walks on both sides of, and the view takes the path at once. Without
the hand-back (the build before 11.4) they were 818 (0.10%), 889
(0.10%) and 412 (0.08%), at the cost 11.4 measures.

Per map, `tdm` seeds 1-4 then 5-8, stalled: mp_carentan 31 133 / 22 209
and 33 031 / 25 534, mp_harbor 9 858 / 10 730 and 8 894 / 14 347,
mp_depot 14 592 / 21 736 and 18 333 / 20 589, mp_brecourt 7 568 / 8 224
and 10 411 / 8 667. mp_depot's accuracy 31.9 / 30.8% and 30.6 / 28.7%.
Kills move inside what the two seed sets differ by. `tdm` life is a
second shorter in both sets (28.5 / 26.7, 27.9 / 26.9 s), `dm`'s level;
not looked into.

Taken apart before 11.4 on mp_depot and mp_harbor `tdm` seeds 1-4
(VERIFIED, measured, temporary switches since removed): stalled 14 592 /
9 858 before, 24 480 / 9 230 with the turn limit alone, 17 763 / 11 475
with the pull alone, 28 947 / 13 428 with both.

### 11.3 The rocket test

`tests/bot_vehicles.rs`'s `a_bot_fires_rockets_at_an_enemy_tank` failed
with this change at a stock UO install: the rocket went out on the same
tick (21) and struck the same face of the tank's hull as on master, and
the tank lived. VERIFIED (measured, temporary logs in `think_armor`,
`Missile::impact` and `vehicle_damage`): master's struck at (4782.927,
-1938.947, 21.149), this build's at (4782.925, -1938.546, 21.183), the
hull at (4673.791, -1979.002) facing -179.83. `vehicle::damage_scale`
takes the behind scale only within 20 degrees of the hull's back axis;
the point lay about 19.99 degrees off it on master (scale 1, 1000
damage, the hull burns and blows up 214 ticks on) and about 20.17 here
(the side's 0.5, 500 damage of 2900, and the tank's gun killed the
shooter). The shooter was set down 600 units dead behind the hull and
walked about 110 units aside while the launcher came up (`think_armor`
keeps the path's keys while the muzzle is not clear), and where that
walk ended decided the zone.

The test now holds him on the back axis with a script `setOrigin` each
tick until the rocket goes (`Server::test_script_set_origin`;
`place_client` respawns the body and the launcher never comes up). He
fires at tick 91 from (5275.4, -1979.0, -7.75) and the tank blows up 214
ticks on (VERIFIED, measured), the kill assertion kept. Held 500 units
back it fires at 21 and 400 back at 52; 700 back, and 900 to 1450 back
unheld, it never does (no ground, or the line blocked).

### 11.4 The hand-back

Before it, the turn limit doubled mp_depot's `tdm` stalls (above). The
stall column counts a bot holding cover as stalled, so I traced where
they sat. VERIFIED (measured, temporary per-tick dumps in `bot_match`,
mp_depot `tdm` 12 bots, seeds 3 and 5, master then the build without
11.4): the stalled bot-ticks were 6 342 and 21 294; engaged and on a
held cover spot 4 459 and 16 567, with the view within 10 degrees of
the target on 69% and 81% of the targeted ones; walking 459 and 617. So
no bot walked into a fight facing its path. Engaged bot-ticks rose
84 685 to 115 317, mostly at 600 to 1000 units (26 759 to 42 535), both
sides on cover 50.6 to 56.2% of them.

The fights themselves (a bot on one target, gaps under a second), seed
5 then seed 3: 744 to 1 171 and 963 to 1 007 fights; those ending with
the target down 40 to 28% and 39 to 35%; those over 10 s 18 to 33 and 14
to 20. The same pair fighting again within 10 s of the last, 189 to 416
and 226 to 299, more often with the view already within 30 degrees of
the target at the first tick. INFERRED (read off that and the code): a
peek, a recall look or a fight let go, and the view came back round to
the path from the threat at the walking rate, so the bot leaving its
cover saw the same enemy again, dug back in, and the two traded rounds
from cover at range. Where the snap had turned it away, it walked on.

With the hand-back, the same dumps (VERIFIED, measured): stalled 3 240
on seed 3 and 5 386 on seed 5 (master 2 895 and 3 447, without the
hand-back 9 345 and 11 949); seed 5's fights over 10 s 21 (master 18)
and those ending with the target down 33% (master 40%, without 28%). The
same pair again within 10 s is still 385 and 405 against master's 226
and 189; its cause is not traced. Without the hand-back
`bots::tests::the_walking_view_takes_the_path_at_once_after_a_fight`
fails.
