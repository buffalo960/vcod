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
`VERBOSE=1` (every kill line).

Everything it counts comes off the script's `logPrint` lines, the same
`games_mp.log` lines a retail server writes:

| Column | From | Meaning |
|---|---|---|
| kills, deaths | `K;` | A kill by another player. A suicide (attacker is the victim or the world, `-1`) and a team kill (same team, not `dm`) are counted apart and are not kills. |
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
