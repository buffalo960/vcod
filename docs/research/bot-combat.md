# Bot combat

How vcod's bots fight once an enemy is in sight, beyond the aim and
footwork in `crates/server/src/bots.rs` (`Bot::engage`, `Skill`). Retail
has no bots, so nothing here is measured against retail. Every number is
vcod's own, from the seeded in-process run each claim names. Navigation
is in `bot-navigation.md`, objectives in `bot-objectives.md`.

## 1. Cover and lean

The brain is `crates/server/src/bots/cover.rs`; the server fills
`BotView::cover` in `Server::bot_cover` (`crates/server/src/server.rs`).

### 1.1 What a cover spot is

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

### 1.2 The search

`cover::find` walks level edges (no jump, no risky edge, no step past
`STEPSIZE`) out from the node nearest the body, five rings and at most
128 units across, and tests the body's own spot first, then the nodes
nearest first. The first `Peek::Stand` in reach wins; a lean spot is
taken only where there is none (1.5). The server re-checks the spot it
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

### 1.3 What the bot does with it

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

### 1.4 Aim from a leaned eye

A lean moves the eye, and the muzzle with it, up to 20 units sideways
(`G_AddLean`, combat doc 2.1). `BotView::lean_offset` carries it and
`aim_angles` aims from there; from the upright eye a leaned round
passes up to 20 units beside a chest it was aimed at, twice
`HIT_RADIUS`. INFERRED: needed for any leaned shot to land. VERIFIED
(measured, 1.5's second and third rows): the fix alone did not lift the
first cut's score, which its walk dominated.

### 1.5 Measured

`cargo run --release -p vcod-server --example cover_ab [seed...]`: `tdm`,
8 fighting bots, 6000 ticks (5 minutes) a seed, one team with cover and
one without, the sides swapped every other seed. Deaths of each side;
fewer is better. `OFF=1` turns cover off on both sides. Over 20 seeds the
per-seed difference has a standard deviation of 12 to 14 deaths, so a
total difference under about 60 is noise.

The botskill harness was not on master when this was written; these runs
are `cover_ab`'s own.

| run (mp_carentan unless named, seeds 1-20) | cover side deaths | other side deaths |
|---|---|---|
| cover off on both sides | 1141 | 1152 |
| first cut: 192-unit reach, full-speed walk, aim from the upright eye | 1079 | 957 |
| the aim fix of 1.4 alone | 1114 | 903 |
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
