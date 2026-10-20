# Bot combat and the match harness

How well vcod's bots fight, and how a change to them is measured. Retail
Call of Duty has no bots, so nothing here is checked against a retail
binary: the yardstick is vcod's own seeded matches. The brain is
`crates/server/src/bots.rs`; how bots move is in `bot-navigation.md`, how
they play objectives in `bot-objectives.md`.

## 1. The harness

`cargo run --release -p vcod-server --example bot_match [seed...]` runs one
in-process match per seed (default 1..=8) with every bot's fire on, on the
stock gametype scripts, until the script calls `exitLevel` or `TICKS`
(12000, ten minutes of 50 ms ticks) run out. `JOBS` (4) seeds run at once;
the nav graph is built once and shared. A match is deterministic in its
seed: the same seed on the same build prints the same lines. The example
passes `2 * seed + 1` to `Server::test_seed_rng`, which sets the low bit
itself, so neighbouring seeds don't replay one match.

Knobs: `MAP=` (mp_carentan), `GAMETYPE=` (tdm), `BOTS=` (6), `TICKS=`,
`JOBS=`, `SET=name=value,...` (any cvar, e.g. `scr_tdm_timelimit=5`),
`UO=1` (a United Offensive server: `COD_DIR` at a UO install),
`VERBOSE=1` (every kill line). `AXIS_SKILL=` / `ALLIES_SKILL=` add the
`BOTS` through `addbot <side> <n> skill <level>`, half a side; `SKILLS=a,b,...`
adds one bot per entry by balance (for `dm`). Without them the bots join
as `--bots` does, at `vc_bot_skill`.

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

Each seed prints one line, then the totals by team and by skill level.

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

## 4. Retreat

A bot in a fight breaks contact when it is hurt or its clip runs dry, then
reloads, waits and plays on (`crates/server/src/bots/retreat.rs`). Retail
has no bots, so none of this is measured against retail; the numbers are
from `--example bot_fights` (below), whose per-side `RETREAT=` switch and
wound count `bot_match` lacks. The baseline and skill tables above predate
the retreat (on by default since this section).

- Hurt: health under 50% of the most (`HURT_PCT`) with an enemy in
  sight, once per wound (`Bot::hurt_at`): a bot that held its cover fights
  on at that health until it is hit again, or healed past it. 1.1, 1.5 and
  UO have no health regeneration; a health pack is the only way back up.
- Dry: the clip at 0 with rounds in the reserve, an enemy in sight and no
  switch under way at the end of the think, so a pistol draw
  (`Bot::sidearm`) comes first. It ends once the clip holds a round and the
  weapon is idle, or the reserve is gone.
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
