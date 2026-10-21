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

The `bot_match` harness (section 1) was not on master when this was written; these runs
are `cover_ab`'s own.

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
