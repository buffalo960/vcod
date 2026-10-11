# CoD 1.1 MP: the scriptent mover verbs

What `moveto`, `movex`/`movey`/`movez`, `movegravity`, `rotateto`,
`rotatepitch`/`rotateyaw`/`rotateroll` and `rotatevelocity` do, in what units,
and what they put on the wire.

The evidence is one paired capture against the retail 1.1d Linux dedicated
server, mp_pavlov under `dm`, 2026-09-09:

- `crates/server/tests/fixtures/movers/mp_pavlov-dm-movers.txt`, the server
  half: `getorigin()` and `.angles` once per server frame through each verb,
  logged by `crates/gsc/tests/fixtures/semantics/client-probes/probe_mover.gsc`
  under `tools/run_probe.sh`;
- `crates/server/tests/fixtures/movers/mp_pavlov-dm-movers-wire.txt`, the
  client half: every change to an entity's `pos`/`apos` trajectory group, from
  a `--net-probe` attached to that same server.

The verb-to-handler mapping is the `scriptent` method table at
`game.mp.i386.so` `0x78d40`, walked by `ScriptEnt_GetMethod` (`0x60f60`), and
is in `docs/research/cod11-gsc-object-model.md` section 9. The trajectory
enumeration is `docs/protocol-1.1.md`, divergence 8.

## 1. Which entities accept a mover verb

Only three classnames. VERIFIED: a `movez` on a `mpweapon_panzerfaust` is the
fatal

```
entity 258 is not a script_brushmodel, script_model, or script_origin
```

so the mover verbs are gated on the entity's spawn class and the gate is a
script runtime error, not a silent no-op. That cost the probe one run.

## 2. The time argument is seconds

VERIFIED. `moveto((600,0,100), 1)` from `(100,0,100)` took 1000 ms of level
clock and `moveto((600,0,100), 2)` took 2000, measured between the frame the
origin first moved and the frame `movedone` fired. A frame or millisecond
reading of the same argument would have finished inside one frame.

## 3. The axis verbs are deltas; the target verbs are absolute

VERIFIED. `movex(500, 2)` on an entity at `x = 100` ended at `x = 600`, and
`moveto((600,0,100), 2)` from the same start ended at the same place, so one
argument is a displacement and the other a destination.

VERIFIED for the angular half too, and it needed a second call to see: a
single `rotateyaw(90, 2)` on an entity at yaw 0 is ambiguous, so the probe
calls it twice. The second call ran the yaw from 90 to 180, so
`rotatepitch`/`rotateyaw`/`rotateroll` are deltas and `rotateto` takes an
absolute angle set.

## 4. The motion law: a trapezoidal velocity profile

`moveto`/`movex`/`movey`/`movez` and `rotateto`/`rotatepitch`/`rotateyaw`/
`rotateroll` take an optional `accel` and `decel` after the duration, both in
**seconds of ramp**, and both default to 0.

VERIFIED from the per-frame origins. For a move of distance `D` over `T`
seconds with ramps `ta` and `td`, the cruise speed is

```
v = D / (T - ta/2 - td/2)
```

the first `ta` seconds are constant acceleration from 0 to `v`, the last `td`
seconds constant deceleration from `v` to 0, and the middle is `v`. Three
measured cases agree to the sampled digit:

| verb | D | T | ta | td | v predicted | v measured |
|---|---|---|---|---|---|---|
| `moveto` | 500 | 1 | 0 | 0 | 500 u/s | 500 u/s (25.00 per 50 ms frame) |
| `moveto` | 500 | 2 | 0 | 0 | 250 u/s | 250 u/s (12.50 per frame) |
| `moveto` | 500 | 2 | 0.5 | 0.5 | 333.33 u/s | 333.33 u/s (16.67 per frame) |
| `moveto` | 500 | 2 | 0.5 | 0 | 285.71 u/s | 285.71 u/s (14.29 per frame) |
| `rotateto` | 90 deg | 2 | 0.5 | 0.5 | 60 deg/s | 60 deg/s (3.00 per frame) |

The accel-only row is what says the two ends are independent rather than one
symmetric ease. The ramp itself is exact constant acceleration: with
`ta = 0.5` and `v = 333.33` the displacement at 50 ms is 0.83 units, which is
`½ (v/ta) t²` to the sampled digit, not a smoothstep.

With no ramps the motion is linear, and the entity is at its destination on
the frame `movedone` fires.

## 5. `movegravity`

VERIFIED. `movegravity((0,0,300), 3)` from `z = 100` traced
`z(t) = 100 + 300t - 400t²` exactly, peaking at 156.25 at t = 0.375 and
reading -2600.00 at t = 3. So the vector is an initial velocity in units per
second and gravity is 800 u/s², the `DEFAULT_GRAVITY` the trajectory code
already carries (`crates/common/src/net/trajectory.rs`).

The time argument is when `movedone` fires, not when the motion stops: the
entity was still accelerating downward on the frame the notify came. Whether
it stops there or keeps falling is UNVERIFIED; the probe's sampler ran out on
the same frame.

## 6. `rotatevelocity`

VERIFIED. `rotatevelocity((0,180,0), 2, 0, 0)` turned the yaw at 9 degrees per
50 ms frame, 180 deg/s, for exactly 2 seconds, ending back at 0 after a full
turn, and raised `rotatedone`. So the vector is an angular velocity in degrees
per second and the second argument is a duration; the accel and decel
arguments are the same pair section 4 measures.

## 7. The completion notifies

VERIFIED. The four linear families and `movegravity` raise `"movedone"`; the
four angular families and `rotatevelocity` raise `"rotatedone"`. Each fires on
the frame the duration elapses, on the entity itself.

## 8. Script-visible state while a mover runs

VERIFIED. `getorigin()` returns the interpolated position on every frame of a
move, and `.angles` reads the interpolated angles on every frame of a rotate
— the field is written back, not left at the value it had when the verb was
called. `.angles` is normalized: after `rotatevelocity` turned a full 360 it
read `(0, 0, 0)` while the wire carried 360 (section 9).

Whether `.origin` is written back the same way is UNVERIFIED; the probe read
`getorigin()`.

**Script sees the trajectory one server frame late.** VERIFIED, from both
halves of the capture at once. `moveto((600,0,100), 1)` called at level time
1050 put `trTime` 1050 on the wire, and yet `getorigin()` still read the start
origin at 1100 and the first moved value, 125, at 1150; `movedone` came at
2100 rather than at 1050 + 1000. Every phase agrees: `rotateyaw(90, 2)` called
at 13450 raised `rotatedone` at 15500. So the script-visible origin and angles
at level time `T` are the trajectory evaluated at `T - 50`, one frame at the
default `sv_fps` 20, and the completion notify comes on the first frame whose
lagged clock has passed the end. Whether the lag is one frame or a fixed 50 ms
is UNVERIFIED; the capture ran at one frame rate. INFERRED, off the entity
pass's place after the threads (combat doc 14.7, "The pass runs after the
threads"): `G_RunMover` evaluates at `level.time` on the frame's pass, script
reads that on the next frame, and the notify it raises on the frame the
trajectory ends wakes its waiter on the next frame's threads. vcod runs the
mover on its turn in the pass (`mover::run_one`) and queues the notify for
the next frame's script (`GameHost::engine_notifies`).

## 9. What goes on the wire

VERIFIED, and this is the load-bearing one: retail sends **a trajectory the
client extrapolates**, not per-frame origins. A two-second move is two
snapshot updates, one at each end, and nothing in between, whatever the frame
rate.

Per verb, with the `trType_t` enumeration from
`crates/common/src/net/trajectory.rs`:

| verb | group | at the call | at the end |
|---|---|---|---|
| `moveto`/`movex`/`movey`/`movez`, no ramps | `pos` | `trType` 3 (`TR_LINEAR_STOP`), `trTime` = the level time of the call, `trDuration` = the duration in ms, `trBase` = the start origin, `trDelta` = the velocity in units/s | `trType` 0, `trBase` = the end origin, `trTime` = the completion time. `trDelta` and `trDuration` are left stale |
| the same with ramps | `pos` | three consecutive trajectories, one per segment: `trType` 9 (`TR_ACCELERATE`) for `trDuration` = `ta` ms, then `trType` 3 for the cruise, then `trType` 10 (`TR_DECCELERATE`) for `td` ms. Every one carries `trDelta` = the cruise velocity `v` and a `trBase` at that segment's own start | `trType` 0 at the end origin |
| `movegravity` | `pos` | `trType` 5 (`TR_GRAVITY`), `trDuration` = the duration in ms, `trDelta` = the initial velocity | not observed; the probe deleted the entity first |
| `rotateto`/`rotatepitch`/`rotateyaw`/`rotateroll`/`rotatevelocity` | `apos` | `trType` 3, `trDelta` = the angular velocity in deg/s | `trType` 0 with `trBase` = the angle reached, unnormalized: a full turn read `(0, 360, 0)` |

The measured example: `movez(96, 2, 0.5, 0.5)` from `z = 592` sent
`trType 9, trDuration 500, base z 592, delta z 64`, then
`trType 3, trDuration 1000, base z 608, delta z 64`, then
`trType 10, trDuration 500, base z 672, delta z 64`, then
`trType 0, base z 688`. The three segments cover 16, 64 and 16 units, and 64
is the cruise speed section 4's formula gives for `96 / (2 - 0.25 - 0.25)`.

A mover is an `eType` 8 (`ET_SCRIPTMOVER`) entity throughout; nothing about
the entity kind changes when it starts or stops moving.

## 10. What is not measured here

- What `movegravity` leaves the entity doing after its `movedone`.
- Whether `.origin` is written back per frame.
- A rotation of more than one axis at once, and a brush model with an
  origin brush, so a pivot other than the world origin.

A second verb on an entity already moving was measured in section 12: it
replaces the trajectory.

## 11. A brush model mover's clip follows it

The evidence for sections 11 to 13 is a second paired capture against the
retail 1.1d Linux server, mp_carentan under a `dm`-shaped probe, 2026-10-05:
`crates/server/tests/fixtures/movers/mp_carentan-dm-ride.txt` (the server
half, one `PROBE f` line per server frame of a phase, from
`crates/gsc/tests/fixtures/semantics/client-probes/probe_ride.gsc`) and
`mp_carentan-dm-ride-wire.txt` (the client half, the `--probe-ride`
playerstate per snapshot and every trajectory change). The probe renames the
two bombzone brush models' `script_gameobjectname` to `dm` so `_gameobjects`
keeps them, stands the player on brush 4281 of model `*5` (entity 177, a
plank slab whose top is z -22) and runs one verb per phase on the entity.

VERIFIED, off the capture: a script_brushmodel with no `origin`, `angles` or
`model` key reads `getorigin()` and `.angles` `(0, 0, 0)` and `.model` `""`
(`PROBE bm 177`). Its brushes are in model space, so the entity's origin is
the pivot of a rotate, here the world origin.

VERIFIED, `Scr_GetGenericField` (game.mp `0x6248c`): its type-8 arm, the
`model` field's, reads the model byte at `ent+0x175` through `G_ModelName`
(`0x66f8c`), which returns configstring `0x10c` plus that index, the model
block's slot 0 for a byte of 0. INFERRED, off the `.model` `""` above and
`G_ParseField` sending a `*N` value to `ent+0x8c` instead
(`docs/research/cod11-gsc-object-model.md`): any entity whose model byte was
never set reads `""`, a brush model included. vcod keeps a BSP `*N` on the
entity apart from the field, and an unset `model` reads `""`.

VERIFIED, off the capture: the brushes move with the entity. A player
standing on the slab rides `movez(48, 2)` up and back down, `movex(48, 2)`
across and back, and is pushed out of its way when it moves sideways into
him (section 12). INFERRED: the server's clip of the inline model is placed
at the entity's `r.currentOrigin` and `r.currentAngles` every trace, which is
what `G_MoverPush` writes (section 12) and the only state the push and the
traces share.

VERIFIED, `G_RunFrame` (game.mp `0x50478`): the entity loop at
`0x50912`-`0x50975` calls the per-entity runner `0x502bc` for an in-use
entity (byte `ent+0x160`), and before it, at `0x50949`, for the entity the
first dword of the link record `ent+0x2e4` points at. The runner calls
`G_RunMissile` for `eType` 4 (`0x50375`), `G_RunItem` when byte `ent+0x161`
is set (`0x503f5`), `G_RunMover` for `eType` 5 or 8 (`0x5040e`) and
`G_RunClient` for an entity with a client (`0x50422`). INFERRED, off those
branches: a linked entity's parent runs ahead of it in the same frame, and
`ent+0x194` against the level time (`0x502cb`) keeps either from running
twice.

VERIFIED, `G_RunMover` (`0x5760c`): it calls `G_GeneralLink` when the entity
has a link record (`0x57623`), `G_MoverTeam` when `pos.trType` (`ent+0xc`) or
`apos.trType` (`ent+0x30`) is non-zero (`0x57676`), and `G_RunThink`
(`0x57682`). VERIFIED, `G_MoverTeam` (`0x55684`): it evaluates `pos` and
`apos` at `level.time` (`0x556c1`, `0x556d7`) and hands the push function
`0x550f0` the differences from `r.currentOrigin` (`ent+0x134`) and
`r.currentAngles` (`ent+0x140`) as `move` and `amove`. VERIFIED, off the
`0x50653` and `0x506d0` calls of `Scr_RunCurrentThreads` and the entity loop
at `0x50912`: the script threads run before the entity pass inside
`G_RunFrame`. INFERRED: the clip is on the trajectory at the level time, one
frame ahead of the `getorigin()` section 8 measured, and that is the clock
the wire's playerstate carries: `movez(48, 2)` called at 11250 put the rider
at z -20.675 on the 11300 snapshot (one frame of 24 u/s) while script read
the slab's z 0 at 11300.

vcod: `CollisionWorld::set_model_pose` places a submodel's brushes by an
origin and angles; traces skip a posed model in the BVH and clip its moved
planes after it. `crate::game::mover` poses a mover's model at the level
time each frame.

## 12. `G_MoverPush`: riders, pushes and a blocked mover

VERIFIED, the push function `0x550f0` (`G_MoverPush` by its shape; the module
has no symbol for it): it bounds the move by `r.absmin`/`r.absmax` plus
`move` when the pusher's angles and `amove` are all zero, by
`RadiusFromBounds` (`0x5518c`) about the origin otherwise, unlinks the pusher,
calls `trap_EntitiesInBox` with mask `0x2000180` (`0x5533c`), adds `move` and
`amove` to `r.currentOrigin` and `r.currentAngles` and relinks the pusher
(`0x553ae`). For each listed entity it tests `eType` 1, 3 or 4 and byte
`ent+0x161` (`0x553e0`-`0x553fa`), compares `groundEntityNum` (`ent+0x7c`)
against the pusher's number (`0x55405`), compares the entity's
`absmin`/`absmax` against the bounds (`0x5540e`-`0x5548f`) and calls
`trap_Trace` (`0x554ee`) with the entity's own `mins`/`maxs` at its own
`r.currentOrigin`, the entity as the skip (its `r.ownerNum` for `eType` 4)
and its `clipmask` (`ent+0x190`), or 0x11 when that is 0; it compares the
trace's entity against the pusher (`0x55515`). The trace is skipped when the
clipmask is set and `r.contents` is `0x4000000` (`0x554a5`). INFERRED, off
those branches: a player is pushed when it stands on the pusher, or when its
box overlaps the pusher where the pusher now is; a corpse never is, its
contents being outside the box mask.

VERIFIED, `cod_lnxded`: the area-node walk at `0x08059590` (the function
that prints `CM_AreaEntities: MAXCOUNT`) lists an entity only when its
`r.contents` (`+0x118`) shares a bit with the query's mask, and descends a
node only when the node's own contents word does. INFERRED: that is
`trap_EntitiesInBox`'s fifth argument, so `0x2000180` lists by contents
before any `eType` test runs. VERIFIED, the contents writers in game.mp: a
playing client `0x2000000`; `player_die` `0x4000000` on the dying player
(`0x49c28`); `G_SpawnItem` (`0x4e778`) and `LaunchItem` (`0x4dc97`)
`0x407c0108`; `RespawnItem` `0x407c0008` (`0x4ece9`); `Touch_Item` 0
(`0x4d90c`, `0x4d9ce`). `fire_grenade` and `fire_rocket` write a clipmask
(`0x544a0`, `0x546ef`) and no contents, and a body-queue clone's contents is
0 (`docs/research/cod11-combat.md` 5.2). INFERRED: the list holds live
players and items lying in the world, and never a grenade, a rocket, a
corpse, a dead player, an item taken, or one `RespawnItem` brought back
(`0x407c0008` has no `0x100`); the `eType` 4 arm and the `0x4000000` test
are never reached through this query.

VERIFIED, mp_carentan's BSP: all ten brushes of model `*5` are
`textures/common/clip_metal`, contents word `0x280306c0`: `0x80` and `0x10000`
set, `0x1` and `0x10` clear. A player's clipmask (`0x2810011`) sees them
through `0x10000`; an item's push trace, `0x11` for a clipmask of 0, sees
neither.

VERIFIED, off a paired capture against the retail 1.1d Linux server,
mp_carentan under a `dm`-shaped probe, 2026-10-06:
`crates/server/tests/fixtures/movers/mp_carentan-dm-push.txt` (the server's
`PROBE` lines from `client-probes/probe_push.gsc` and the `--probe-items`
client's `ITEM` lines). The probe drops carbine `a` onto the slab, health
pack `b` onto the ground south of it and hangs carbine `c` there
(`spawnflags 1`), then runs the slab through probe_ride's verbs.

- **Carried.** `a` rests on the slab with ground 177 and follows it to the
  hundredth through `movez`, `movex` and both `rotateyaw`s, ground 177 on
  every snapshot and the slab's position in `pos.trBase`, `trType` 0, each
  frame. From `push_y` on it creeps +x 0.021 a frame, the residual yaw of
  this section.
- **Not shoved.** `b` and `c` never move: the slab sweeps through both in
  `ride_yaw` and `push_y`, `b`'s ground stays 1022 and `c`'s 0, and the
  slab lowered onto `b` (`crush`) runs its whole move without stalling.

INFERRED, off the listing, the masks and the capture: an item resting on a
mover is carried because the ground test keeps it before any trace; an item
in a mover's way is shoved only when its own push mask sees the mover's
brushes, which a script-spawned or placed item's `0x11` does not for a clip
brush; a dropped item's `0x81` would, through `0x80` (not captured). An item
is 2 units wide, so `maxs.x * 0.5` never passes the jitter's 4, and an item
that fits nowhere is relinked where it was without stalling the mover.
`G_TryPushingEntity` writes `0x3ff` to the ground of anything it moves off
another ground or leaves in place, which `G_RunItem` drops from.

VERIFIED, the list box (`0x550f0`..`0x5530a`): the pusher's `r.absmin` and
`r.absmax` when its `r.currentAngles` and `amove` are all zero, else a cube of
`RadiusFromBounds(r.mins, r.maxs)` about `r.currentOrigin`, both before the
move, then stretched along `move` on each axis (added to the max side when
positive, to the min side otherwise; the sign test at `0x552e8`).

VERIFIED, then, in this order: the filter loop (`0x553d4`..`0x5553c`) copies
every kept entity number into a second array; a loop unlinks every kept
entity (`0x55550`..`0x55570`, the call at `0x55561`); a loop hands each, in
list order, to `G_TryPushingEntity` (`0x555c8`) after storing it and its
`r.currentOrigin` at `pushed_p` (`0x55598`..`0x555bb`) and relinks it when
that returns 1 (`0x555e1`). On a 0, an `eType` 3 entity is relinked and
skipped (`0x555d4`); otherwise a pusher whose `pos.trType` or `apos.trType`
is 4, `TR_SINE`, calls `G_Damage(check, pusher, pusher, 0, 0, 99999, 0,
0x13, 0)` (`0x55617`) and goes on with the entity still unlinked, and any
other stores the entity through its fourth argument and returns 0 (`0x55621`)
straight to the epilogue. Only when the list runs out does a last loop link
every kept entity again in list order (`0x55650`..`0x55670`, the call at
`0x55661`) and return 1. VERIFIED: nothing in `0x550f0`..`0x553ae` stores
through `pushed`, so the pusher itself is not on the undo list. INFERRED: a
push turns the kept entities round in their node's list, each relink
prepending it (combat doc 14.7); the last loop finds each in the node it
already sits in and keeps its place; and a blocked push leaves the blocker and
every kept entity listed after it out of the tree until something links them
(a player's next cmd).

VERIFIED, off a paired capture against the retail 1.1d Linux server,
mp_carentan under a `dm`-shaped probe with two clients, 2026-10-09:
`crates/server/tests/fixtures/movers/mp_carentan-dm-pushorder.txt` (the
server's `PROBE` lines from `client-probes/probe_pushorder.gsc`). Two players
set down in the slab's path, slot 0 then slot 1, are walked 1 then 0 by a
flat `radiusDamage`; after each `movey` that pushes both the walk turns round
(0 then 1, then 1 then 0, then 0 then 1), and a `movey` away from them leaves
it as it was.

VERIFIED, `G_TryPushingEntity` (`0x54930`): it returns 0 when the pusher's
`eFlags` carry 0x04000000 (byte `+0xb` bit 4, `0x54942`) and the entity's
ground is not the pusher; it adds `move` to `r.currentOrigin`
(`0x54956`-`0x54982`), calls `AngleVectors(amove)` and `VectorInverse` on the
right vector (`0x5498c`, `0x54995`), and turns the result about the pusher's
moved `r.currentOrigin` by that matrix (`0x549d5`-`0x54aae`). It calls
`trap_Trace` there (`0x54b3b`) with the entity's box. On a clear trace it
writes `0x3ff` to `groundEntityNum` when that was not the pusher
(`0x54b77`), the spot into `r.currentOrigin` and `s.pos.trBase`, and for a
client adds `(int)(amove[1] * 182.044) & 0xffff` to `ps.delta_angles[1]`
(`0x54bb5`-`0x54bf7`, the 182.044 at rodata `0x75bf4`) and writes `ps.origin`
(`0x54c00`-`0x54c1e`), steps `pushed_p` (`0x54f41`) and returns 1. When
`maxs.x * 0.5` exceeds 4 (`0x54c23`, rodata `0x75bf8` and `0x75c00`) it tries
offsets in steps of 4 (rodata `0x75c08`) with the same trace and the same
success path (`0x54db0`, `0x54e21`, `0x54e8e`); last it traces the entity's
unmoved `r.currentOrigin` (`0x55084`, `0x550a5`) and, clear, writes `0x3ff`
to `groundEntityNum` and returns 1 without stepping `pushed_p`
(`0x550cd`-`0x550d7`); otherwise it returns 0. INFERRED: the offsets are
RTCW's jitter, `z` 0 then ±4, `x` ±4, `y` ±4, in that nesting; a player is
15 wide, so it reaches 4 and no further.

VERIFIED, `G_MoverTeam` again: on a 0 from the push it walks the `pushed`
records back, newest first (`0x55744`-`0x557ef`), restoring `r.currentOrigin`,
`s.pos.trBase`, a client's `ps.origin` and subtracting
`ANGLE2SHORT(record yaw)` from `ps.delta_angles[1]` (`0x557b3`), and links
each (`0x557de`); then for each team part adds `level.time -
level.previousTime` to `pos.trTime` and `apos.trTime` (`0x5580f`, `0x55821`),
re-evaluates both (`0x55838`, `0x55851`) and links it (`0x5585d`); then calls
`ent+0x208` if set (`0x55881`). `InitScriptMover`
(`0x60214`) writes `Reached_ScriptMover` to `ent+0x204` (`0x60364`) and
nothing to `ent+0x208`. INFERRED: a blocked script mover holds where it was,
one frame per blocked frame, with no callback, and its `movedone` comes that
much later.

VERIFIED, off the capture:

- **Riding.** The rider's origin follows the slab to the hundredth through
  every ride phase, z -21.875 to 26.125 and back on `movez`, x -215 to -167
  and back on `movex`; `groundEntityNum` reads 177 throughout.
- **Rotation.** `rotateyaw(2, 1)` about the world origin carried the rider
  from `(-215, 2463)` to `(-300.83, 2453.99)`, the point turned 2 degrees
  about the origin, and `delta_angles[1]` rose 18 a frame (0.1 degrees,
  `ANGLE2SHORT` truncated).
- **Pushing.** The player standing south of the slab at `(-215, 2430)` was
  first moved on the 28550 snapshot, the frame the slab's trajectory went
  from 6 to 7.2 units, and from there by the slab's 1.2 units a frame. The
  slab moving back left him where it pushed him.
- **Blocked.** The slab lowered from z 80 onto the standing player stopped
  at z 72 and held for the four seconds he stood there, its wire `trTime`
  advancing 50 every frame, and he was not moved. Once he was teleported
  away it went on down.
- **A verb on a moving entity.** A `moveto((0, 0, 0), 1)` called at 42250
  while the stalled descent was still running replaced it: the wire's new
  `trBase` z 30 is the old trajectory at the call's level time, and its
  `trDelta` z -32 is the 32 that `getorigin()` read a frame earlier. Section
  14 has the second run that tells the two apart.
- **The residual yaw.** After `rotateyaw(2, 1)` and `rotateyaw(-2, 1)` the
  slab's `apos.trBase` reads `-0.0` and the pushed player drifts +x by 0.021
  a frame while it is pushed, the slab's whole push turning about the world
  origin by about 0.0005 degrees a frame. INFERRED: `amove` is never quite 0
  for an entity whose `r.currentAngles` came back off a rotate.

vcod: `mover::run_one` unlinks a moving mover, lists `G_MoverPush`'s box
with `AreaTree::entities_in_box` into `Step::listed` and links it again.
`crate::push::push` runs on the mover's turn in the entity pass (combat doc
14.7), after the frame's script: it keeps the listed players and items in
list order, unlinks them all, pushes and relinks each on its turn
(`G_TryPushingEntity`'s jitter and keep-in-place fallback for a player, a
player linked unsnapped at the spot), links them all again, and on a player
that fits nowhere walks its records back, relinks those, leaves the rest
unlinked and stalls the mover. A player listed after the one being pushed is
out of the tree and blocks nothing, as in retail. A player is moved from
`ps.origin`: VERIFIED, vcod measurement against the ride fixture, a push from
the truncated link origin lost the fraction retail's pushed player keeps
(`push_y` frame 8, y 2427.6 against our 2428.8). Its candidate test is
a box, as retail's; its position test is our capsule plus a sweep from where
the body stood that passes through the pusher, because a zero-length capsule
is never `startsolid` under terrain and without the sweep the lowered slab
pushed the player through the ground. VERIFIED, vcod measurement against
the same fixture: a capsule candidate test met the plank 4.8 units of slab
travel later than retail's box did. The position test also asks
`CollisionWorld::box_position_test`, retail's box against every brush and
the capsule position test against terrain and patches (section 20). The residual yaw is not
modelled. The `TR_SINE` crush is (section 21).
`crates/server/tests/ride_ab.rs` is the gate for players,
`crates/server/tests/push_ab.rs` for items and
`crates/server/tests/push_order_ab.rs` for the tree order.

### The box test

VERIFIED, `0x550f0`..`0x5530a`: besides the list box the function keeps a
second box, the pusher's box where the move takes it. Unrotated, it is
`r.absmin + move` and `r.absmax + move` (`0x55260`..`0x5527c`, stored at
`ebp-0xc` and `ebp-0x18`); otherwise `r.currentOrigin + move` less and plus
the `RadiusFromBounds` radius (`0x551d0`..`0x551f0`, same slots). Both are
taken before the pusher moves and relinks.

VERIFIED, `0x5540e`..`0x5548f`, after the ground test and before the trace:
a listed entity is dropped when, on any axis, its `r.absmin` (`+0x11c`) is
not below the moved box's max (`fcomp`, `and ah,0x45`, `dec ah`,
`cmp ah,0x40`, `jb` to the loop's `continue` at `0x55532`) or its `r.absmax`
(`+0x128`) is not above the moved box's min (`and ah,0x5`, `je` to the same
place). INFERRED: Q3's `G_MoverPush` test, boxes that only touch missing.
INFERRED: both are link boxes grown a unit a side (`SV_LinkEntity`,
`Link::boxed` in `crates/server/src/area.rs`), so the test never drops an
entity the pusher's brushes overlap; it only saves traces.

vcod: `mover::push_boxes` keeps the moved box on `Step::moved_box`, and
`crate::push` runs the test off each listed entity's last link box between
the ground test and the trace, for players and items alike.
`push::tests::the_box_test_drops_a_box_that_only_touches` pins the
comparisons; with the test forced to drop everything,
`push_order_ab` fails, so it is on the path the gates run.

### `TR_SINE` and the crush arm

VERIFIED, every immediate store `mov DWORD PTR [reg+0xc],0x4` or
`[reg+0x30],0x4` in game.mp's `.text` (an `objdump` scan): two, `SP_func_pendulum` writing
`apos.trType` at `0x57110` and `SP_func_bobbing` writing `pos.trType` at
`0x584c4`. VERIFIED, a scan of every `.bsp` and `.gsc` in the 1.1 install's
`pak*.pk3`, single-player maps included: none contains `func_bobbing` or
`func_pendulum`. VERIFIED, script reaches both anyway: `spawn(classname,
origin)` (`0x5d268`) calls `G_CallSpawnEntity` (`0x5d2f1`), which walks the
`spawns` table (`0x61861`), and both classnames are in it
(`docs/research/cod11-gsc-object-model.md` 8).

VERIFIED, off two retail runs of `client-probes/probe_sine.gsc` (mp_carentan,
no client, 2026-10-10): `spawn("func_bobbing", (0, 0, 100))` returns a live
entity with `.model` `""`, and its `origin` swings as `32 sin(2 pi t / 4000)`
about z 0 (31.90 at 1100, -31.90 at 3100), the spawn origin unused; the
`func_pendulum` beside it swings its roll (18.85 at 1100). Spawned at 1050
in one run and 2050 in the other, the server went down both times on the
frame after the 4000 line with `ERROR: Reached_BinaryMover: bad moverState`.

VERIFIED: `InitMover` stores `Reached_BinaryMover` at `ent+0x204`
(`0x565da`), and `G_MoverTeam`, for each team part, calls `ent+0x204` when
`pos.trType` is not 0 and `level.time >= pos.trTime + pos.trDuration`
(`0x558a0`..`0x558c2`), the same for `apos` (`0x558c7`..`0x558e9`).
INFERRED: a bobbing's `pos.trTime` is its phase times its period, 0 by
default, so `Reached_BinaryMover` sees a mover state no `SP_func_bobbing`
set and drops the map once the level clock passes one period; a bobbing
spawned later than that drops it on its first entity pass.

VERIFIED, `cod_lnxded` syscall 0x21 (`trap_SetBrushModel`, game.mp
`0x63724`), the case at `0x8088360` off the switch table at `0x80d4eac`:
it reads the inline model number at `ent+0x8c`, writes `r.mins`/`r.maxs`
from its bounds, `r.bmodel` 1 (`+0xfc`) and `r.contents` -1 (`+0x118`),
then links. INFERRED: a script-spawned bobbing has no `*N` key and carries
inline model 0, so before the drop it is a moving copy of the world's
brushes; whether it pushes or crushes anything in that window is not
measured.

INFERRED, then: the crush arm (`0x555f0`..`0x55617`) is reachable only
through an entity that drops the server within one period of its
`trTime`, from a classname no stock map or script uses. Section 18 has
the two spawn functions, the wire and the drop; section 21 measures the
crush inside that window.

## 13. A player linked to a mover

VERIFIED, off the capture: a player linked to a `script_origin` 32 units to
its east follows the parent's `movez(48, 2)` and `movex(48, 2)` exactly, and
on the parent's `rotateyaw(90, 2)` swings round it to `(-480, 2656)`, the
offset turned 90 degrees, with `delta_angles[1]` unchanged. VERIFIED: the
snapshot after the call carries the parent's first frame of motion, z
-22.675 at 44300 for a move called at 44250, a frame ahead of the player's
own script `.origin`.

INFERRED, off section 11's parent-first loop and `G_SetFixedLink`'s mode-2 arm
(`docs/research/cod11-gsc-object-model.md` 23.2): the re-anchor reads the
parent where it is this frame and turns the link offset by the parent's
axis; the offset is held in the parent's frame. The capture's parent had zero
angles at the link, so it cannot tell a parent-frame offset from a world one.

vcod: the re-anchor runs on the client's turn in the entity pass, after
its parent's (`ScriptRuntime::run_entity_pass`, `PassStop::Client`), reads
`ScriptRuntime::link_anchor`, the mover's plan at the level time, and
relinks the client, as `G_RunClient` (0x40660) does through
`G_SetFixedLink` (0x406d9) and `trap_LinkEntity` (0x40713); `linkTo` stores
the offset in the parent's frame.

## 14. The brush model on the wire, and the client's half

The evidence for the wire half is a third capture from the same probe with
five phases added at its end (`hide`, `show`, `notsolid`, `solid`, `delete`
on the slab), 2026-10-06:
`crates/server/tests/fixtures/movers/mp_carentan-dm-ride-ents.txt`, the
server's phase starts and the `--probe-ride` client's `RIDE_ENT` (the slab's
`solid`, `index` and `eFlags` on change), `RIDE_GONE` and trajectory lines.

VERIFIED, off that capture: both bombzone brush models arrive on the first
snapshot as `eType` 8, `solid` `0xffffff`, `index` 5 and 6 (their inline
model numbers) and `eFlags` 0. Model configstring 5 on that load is
`xmodel/barrel_black1`, so the `index` of a `0xffffff` entity names an inline
model, not a configstring slot. VERIFIED: `SV_LinkEntity` (cod_lnxded
`0x80908b0`) stores `0xffffff` into `s.solid` when `r.bmodel` is set
(`0x80908da`) before it reads `r.contents`.

VERIFIED, off the capture: `hide()` at 53450 put `eFlags` `0x100` on the
53450 snapshot and `show()` at 54450 took it off on the 54450 one; `notsolid()`
and `solid()` changed nothing on the wire, the entity staying in every
snapshot with `solid` `0xffffff`; `delete()` at 57450 took it out of the 57450
snapshot. VERIFIED, game.mp: the per-entity runner `0x602bc` sets byte
`ent+9` bit 1 (`s.eFlags` `0x100`) when byte `ent+0x17d` bit `0x10` (the
`flags` `0x1000` `ScrCmd_Hide` writes) is set and clears it otherwise, for an
entity with no client (`ent+0x158` zero). INFERRED: a `notSolid()`ed brush
model is still sent as one, and the entity leaves the snapshot on the frame
of the `delete()`, a tenth of a second before the free.

VERIFIED, off the capture's trajectory lines: a move reads stationary on the
snapshot of the frame its last segment ends (`ride_up`, called at 10450 for
2 s, reads `trType` 0 `trTime` 12450 at 12450), a frame before script's
`movedone` (section 8). Every frame the lowered slab is blocked, `trTime` of
`pos` and of the stationary `apos` both advance 50 (section 12).

VERIFIED, off the same: the `moveto((0, 0, 0), 1)` at 41450 on the
descending slab sent `trBase` z 28 and `trDelta` z -30, where script read z
30 at the call; the move ended at 42450 on `trBase` `(0, 0, 0)`. INFERRED:
`trBase` is the old trajectory at the call's level time, the velocity is
taken from `r.currentOrigin`, a frame behind it, and the stationary end is
the destination rather than where the segment ran out (z -2). The first run's
moveto (section 12) is the same rule with the slab 2 units higher.

vcod: `crate::game::wire` sends a `script_brushmodel` as `eType` 8, `solid`
`0xffffff`, `index` its `*N`, `eFlags` `0x100` while hidden, and drops it on
`delete()`; `Movers::wire` hands the plans over advanced to the level time;
`crate::world` culls a brush model by its inline model's bounds (a cube of
their radius once its angles are not zero, `SV_LinkEntity`'s `r.bmodel`
arm). `ride_ab.rs` diffs the slab's entity per snapshot against the capture.

VERIFIED, off a fourth capture, 2026-10-06:
`crates/server/tests/fixtures/movers/mp_carentan-dm-cull.txt`, from
`client-probes/probe_cull.gsc` and a `--probe-ride` client standing on the
attackers' spawn. `movez(20000, 8)` on the slab at 12350 took entity 177 out
of the 13300 snapshot, the first frame its trajectory stood 2375 units up
(2250 on the frame before), and `movez(-20000, 8)` at 21350 brought it back
on the 28450 snapshot, the first frame the trajectory was down to 2250. A
cull at `trBase` would have kept it for the whole lift (`trBase` is the
segment's start) and dropped it for the whole descent. INFERRED, off that and
`G_MoverPush` relinking the pusher at its moved `r.currentOrigin` and
`r.currentAngles` every frame it moves (section 12, `0x553ae`): the snapshot
cull reads the clusters of the box the mover was last linked with, at its
trajectory at the level time.

vcod evaluates an `eType` 8 entity's `pos` and `apos` at the frame's level
time for the cull (`crate::world::entity_visible`). VERIFIED, vcod
measurement with the same probe and client against ours: the slab left the
snapshot 950 ms into the lift and came back 7100 ms into the descent, as on
retail.

### The client

VERIFIED, `cgame_mp_x86.dll`: case 8 of the entity-type switch at
`0x3001d5f0` is `0x3001b710`, which returns without drawing when `eFlags`
carries `0x100` (`test ah,0x1` at `0x3001b732`), and when `solid` is
`0xffffff` (`0x3001b76d`, `0x3001b7de`) adds a ref entity of type 0 whose
model is the table at `0x301d31c0` indexed by `es.index`, at the entity's
interpolated origin (`cent+0x1f8`..`+0x200`); any other `eType` 8 entity is a
type 1 ref entity from the xmodel table at `0x301d24fc`. INFERRED: a brush
model draws only through its snapshot entity, at its interpolated pose, and
a hidden one draws nothing.

VERIFIED, a census of the 16 BSPs in the mounted paks: 30
`script_brushmodel`s, none with an `origin` key, so every one's brushes and
surfaces are in world space; the only submodels with draw surfaces belong to
`script_brushmodel`s (18 of them); carentan's bombzones `*5` and `*6` carry
none. INFERRED: the ride capture's slab is clip only, drawn by nothing on
retail either.

INFERRED, off the ref entity at `0x3001b76d` naming the inline model rather
than an xmodel: the renderer draws a moved brush model's own BSP surfaces,
lightmap indices and all, so it keeps the lighting baked where it was
compiled. Not read in `CoDMP.exe`.

vcod: `entities::resolve_visual` takes a `0xffffff` entity's `index` as its
inline model and skips one with `eFlags` `0x100`. A brush model whose entity
stands at the zero pose draws with the world; one that has moved draws its
own soups through the world's pipelines and lightmaps under a camera whose
`model` matrix is its interpolated pose; one with no entity in the snapshot
draws nowhere (`Renderer::set_submodels`). With no server, every one draws
at spawn.

VERIFIED, `cgame_mp_x86.dll`: `CG_ClipMoveToEntities` (`0x30028df0`) takes
an entity whose `solid` is `0xffffff` down a separate arm that calls syscall
`0x21` with `s.modelindex` and `BG_EvaluateTrajectory` (`0x30005470`) twice
before the trace syscall. INFERRED: that is Q3's `trap_CM_InlineModel` and the
`pos` and `apos` evaluated at the prediction time, so prediction clips a
moving brush model where its trajectory has it. `CG_BuildSolidList`
(`0x30028d50`) leaves out a `0xffffff` entity only on a flag bit 2 of a byte
at `centity+0xf8`.

VERIFIED: the function at `0x3001baa0` reads an entity number between 1 and
`0x3fd`, tests its `eType` against 5 and 8, calls `BG_EvaluateTrajectory`
four times and writes `in + (a - b)` and an angle delta;
`CG_PredictPlayerState` (`0x300294f0`) calls it at two sites. INFERRED: it is
Q3's `CG_AdjustPositionForMover`, carrying the predicted origin with the
ground entity's motion between the snapshot time and the render time.

VERIFIED, cgame: `CG_BuildSolidList` (`0x30028d50`) walks the snapshot's
entities and skips one whose next state has `solid` `0xffffff`
(`0x30028d93`) and `eFlags` bit `0x2` (`test byte [eax+0xf8],0x2` at
`0x30028d9b`); `SP_trigger_multiple`, `SP_trigger_damage` and
`SP_trigger_once` (game.mp `0x74c50`, `0x75278`, `0x75c0c`) are the writers
of that bit read so far. VERIFIED: the brush model arm of
`CG_ClipMoveToEntities` evaluates `apos` and `pos` (`0x30028e67`,
`0x30028e78`) at the time held in `0x30207150`, which `CG_PredictPlayerState`
loads from the snapshot's `serverTime` (`snap+8`). VERIFIED: the carry after
the cmd loop (`0x30029a2e`) passes the dword at `0x302071b0` as the entity
number, that time, the dword at `0x30207148` as the target time and the
predicted origin at `0x30207170` as both in and out; the angle delta it
writes to `[esp+0x34]` is not read again before the function returns. The
second call site is `0x3002972d`, inside the cmd loop. INFERRED: the entity
is the predicted `groundEntityNum` and the target time `cg.time`; retail
clips a brush model where it stood at the snapshot, not per cmd, carries the
predicted origin by the ground mover's translation only, so a rider of a
turning mover is predicted standing still between snapshots; and the second
site is Q3's miss test, which carries the new replay's origin at the old
prediction's `commandTime` before comparing, so a ride is no correction.
INFERRED, off the `eFlags` test and section 14's wire: a `notSolid()`ed
brush model still in the snapshot is clipped by retail's prediction, as the
four exploder brush models `_load.gsc` hides and `notSolid()`s are (mp_depot
`*1`, mp_powcamp `*3` and `*9`, mp_rocket `*3`; cod11-mantle.md).

VERIFIED, cgame `0x3001d210` (`CG_CalcEntityLerpPositions` by its shape):
it reads `currentState.pos.trType` (`cent+0xc`, `0x3001d216`) and calls the
snapshot lerp `0x3001d090` when it is 1, `TR_INTERPOLATE` (`0x3001d219`), or
when it is 3, `TR_LINEAR_STOP`, and the entity number is below `0x40`
(`0x3001d22d`, `0x3001d232`). Otherwise it calls `BG_EvaluateTrajectory`
twice at `cg.time` (`0x30207148`, `0x3001d246`, `0x3001d25b`) into
`cent+0x1f8` and `cent+0x204`, and, unless `cent` is `0x3020922c`, calls
`0x3001baa0` at `0x3001d2ee` with `currentState.groundEntityNum`
(`cent+0x7c`), `cg.snap->serverTime` (`[0x301e2160]+8`), `cg.time` and a
null angle out (`ecx` 0 from `0x3001d263`, pushed at `0x3001d2dd`), the
origin `cent+0x1f8` as both in and out. VERIFIED: `0x3001baa0` copies in to
out unchanged unless the number is above 0 and below `0x3fe` (`0x3001bab5`,
`0x3001babd`) and that entity's `eType` is 5 or 8 (`0x3001bad6`,
`0x3001badf`). INFERRED: `0x3020922c` is `cg.predictedPlayerEntity`; any
entity whose position is not lerped is drawn off the older snapshot's
trajectories at the drawn time and carried by its ground mover's translation
since that snapshot, so an item resting on a moving brush model (section 12:
`trType` 0, ground the mover) rides it smoothly between snapshots, and one
with no mover under it holds its older `trBase` until the next snapshot.

VERIFIED, game.mp `G_GeneralLink` (`0x68530`), which `G_RunMover` calls for
a linked entity (section 11): `G_SetFixedLink(ent, 0)`, `G_SetOrigin` and
`G_SetAngle` at the re-anchored `r.currentOrigin` and `r.currentAngles`,
then 1 to `pos.trType` and `apos.trType` (`0x68568`, `0x6856f`) and
`trap_LinkEntity`. INFERRED: a script model linked to a mover goes out
`TR_INTERPOLATE` at its re-anchored pose every frame, and the client lerps
it between snapshots; the ground carry above never reaches it.

vcod: `entities::lerp_pos_angles` makes the same choice on the older
snapshot's state and carries through `SnapshotMovers::carry`. Section 15
measures the link itself.

vcod: `vcod_common::pmove::movers::SnapshotMovers` is that solid list and
that carry. The client unlinks every submodel at map load, and each
prediction links and poses the snapshot's brush models at its `serverTime`,
naming each by its entity so a ground trace reads it; the replay's newest
origin and the drawn origin are carried by the ground mover, and the
correction compares both sides carried to the old `commandTime`.
`predict_ab.rs`'s `predictor_rides_retail_movers` predicts the ride capture's
rider from each snapshot to the next against retail's next playerstate:
exact to 0.05 through every carried phase, 4.3 units out through the two yaw
phases, as retail's own prediction is.

## 15. `linkTo` on script entities

The evidence is one paired capture against the retail 1.1d Linux server,
mp_carentan under a `dm`-shaped probe, 2026-10-08:
`crates/server/tests/fixtures/movers/mp_carentan-dm-linkto.txt` (the server
half, one `PROBE` line per server frame of a phase, from
`crates/gsc/tests/fixtures/semantics/client-probes/probe_linkto.gsc`) and
`mp_carentan-dm-linkto-wire.txt` (the client half, every snapshot's state of
the linked script_model, entity 171). The probe keeps model `*5` (entity 177,
origin and angles zero) as `probe_ride` does, links a script_origin `a`
(170, yaw 30) and a script_model `b` (171) to it and moves it, runs two
three-entity chains, deletes a moving parent, links four script_origins to
the player, and ends on a link cycle.

### Which receivers

VERIFIED: `InitScriptMover` (`0x60214`) writes `eType` 8 (`0x60378`) and
sets the `ent+0x17d` bit 0x20 `linkTo` tests (`0x6038d`); object-model doc
23.2 lists the other writers (`G_SpawnItem`, `G_SpawnTurret`, `ClientSpawn`,
`enableLinkTo`). INFERRED, off the mover verbs' classname gate (section 1)
naming the same three classes: every `script_model`, `script_origin` and
`script_brushmodel` takes a link, and so do items and turrets.

### The record

VERIFIED, the helper both link builtins call (`0x662e0`, unnamed, between
`G_EntAttach` and `G_CalcTagAxis`): it calls `G_EntUnlink` on the child
(`0x662f3`), `trap_DObjExists` on the parent (`0x66307`) and
`trap_DObjGetBoneIndex` (`0x6631b`), and loads -1 as the bone at `0x66330`;
it compares the parent with the child (`0x66335`) and reads each record's
first dword along the parent's chain (`0x66344`..`0x66353`); it allocates
`0x70` bytes (`0x6635c`) and stores the parent at `+0x0`, the lowercased tag
string at `+0x8` (`SL_GetLowercaseString` `0x66376`), the parent's old
first child at `+0x4` and the bone at `+0xc`, zeroes `+0x10` and `+0x40`,
0x30 bytes each, and writes `parent+0x2e8 = child`, `child+0x2e4 = record`
(`0x66386`..`0x663bf`). INFERRED, off the branches around those sites and to
`0x66380`, which returns 0: the two DObj calls run for a non-empty tag and
the -1 is an empty tag's; a missing DObj or bone, the parent being the
child and a parent chain that reaches the child all refuse the link, and
the unlink at the top has already run by then.

VERIFIED, `G_EntLinkTo` (`0x68034`): it calls the helper (`0x6804a`) and
`G_CalcTagAxis(child, 0)` (`0x6805c`). VERIFIED, `G_CalcTagAxis`
(`0x663d4`): it reads the parent's `AnglesToAxis(r.currentAngles)` and
`r.currentOrigin`, calls `G_DObjCalcBone` and `DObjSkelMatrixMultiply43`
(`0x6643c`..`0x66463`), the child's own `AnglesToAxis` (`0x664b4`),
`MatrixInverseOrthogonal43` (`0x66502`) and `MatrixMultiply43` into
`record+0x10` (`0x6652e`). INFERRED, off the bone test at `0x663f8` and the
mode test at `0x664c9`: the bone matrix enters only for a bone that is not
negative, and on mode 0 the record ends up holding the child's pose in the
parent's (or the tag's) frame, so the link moves nothing on the frame it is
made.
VERIFIED, off the capture's `PROBE linked` and `PROBE linked_pl` lines: every
child reads the pose it had before the call.

VERIFIED, `G_EntLinkToWithOffset` (`0x68074`): it calls the helper and
writes `AnglesToAxis` of the fourth argument into `record+0x10` and the
third into `record+0x34`. VERIFIED, off the capture: `k4 linkto(player, "", (16, 0, 8),
(0, 90, 0))` reads the player's origin plus `(16, 0, 8)` and angles
`(0, 90, 0)`.

VERIFIED, the builtin's failure arm (`0x59df5`..`0x59ea8`): it calls
`trap_DObjExists` on the parent (`0x59e01`), reads the model byte
`ent+0x175` (`0x59e0d`), `trap_DObjGetBoneIndex` (`0x59e5d`) and carries the
errors `"failed to link entity since parent has no model"` (`0x76bc0`),
`"failed to link entity since parent model '%s' is invalid"` (`0x76c00`),
`"failed to link entity since tag '%s' does not exist in parent model
'%s'"` (`0x76c40`) and `"failed to link entity due to link cycle"`
(`0x76ca0`). INFERRED, off its branches: a parent with no DObj takes the
first error when its model byte is 0 and the second otherwise, a missing bone
takes the third, and only a parent with a DObj and a good tag reaches the
cycle's. VERIFIED, off the capture: `y linkto(x)` with
`x` already linked to `y`, both script_origins, is the fatal `failed to link
entity since parent has no model`.

### The per-frame re-anchor

VERIFIED, the per-entity runner `0x502bc` (section 11) compares `eType`
with 3 (`0x50380`), tests the link record (`0x50385`) and calls
`G_GeneralLink` (`0x50392`) and the think pointer (`0x503e1`) on that path;
`G_RunMover` tests the record (`0x57616`), calls `G_GeneralLink`
(`0x57623`) and jumps to `G_RunThink` (`0x57628`, `0x5767e`). INFERRED, off
those branches: a linked item runs the link and its think instead of
`G_RunItem`, and a linked mover never reaches `G_MoverTeam`. VERIFIED,
`G_GeneralLink` (`0x68530`): `G_SetFixedLink(ent, 0)` (`0x68540`),
`G_SetOrigin`, `G_SetAngle`, 1 into both `trType`s (`0x68568`, `0x6856f`),
`trap_LinkEntity`. VERIFIED, `G_SetFixedLink` (`0x66540`): it builds the
parent's frame from the same reads as `G_CalcTagAxis` and its mode-0 arm
(`0x66630`) calls `MatrixMultiply43` on `record+0x10` and `AxisToAngles`
into the child's `r.currentAngles`.

VERIFIED, off the capture: `a` and `b` move on the same script frame as the
brush model's own `getorigin()`: at 12600 the model reads z 1.20, `a` 41.20
and `b` -20.80. INFERRED, off section 11's clocks: the pass runs after the
threads and reads the parent where it is on the level time, and script reads
the result a frame later, as it reads the parent's.

VERIFIED, off the wire half: entity 171 goes out `trType` 1 with `trTime` 0
and `trDuration` 0 on both groups on every snapshot it is linked, and its
`trBase` z at `serverTime` 12550 is -20.8, the value script reads at 12600.

VERIFIED, off the capture: a turning parent swings the child round its own
origin and adds its yaw to the child's: at 15600 the model reads yaw 0.50,
`a` yaw 30.50 and `b` yaw 0.50. `b.angles` reads `(0, 360.00, 0)` once the
model's yaw is back to `-0.00`: `AxisToAngles` wraps a yaw a hair below 0
into `[0, 360)`.

VERIFIED, off the capture's `verb_linked` phase: `a movez(100, 1)` on the
linked `a` moves nothing and no `movedone` comes in the next 2 s, nor after
the unlink. INFERRED, off `G_RunMover` skipping `G_MoverTeam` and
`G_SetOrigin` rewriting the trajectory on every link frame: a verb on a
linked entity is lost outright.

VERIFIED, off the `unlink` phase: `a unlink()` at 25500 leaves `a` at z
65.20, the value script read that frame, while `b` rides on. VERIFIED,
`G_EntUnlink` (object-model doc 23.2): `G_SetOrigin` and `G_SetAngle` at the
entity's own current pose.

### Order: a parent runs first, a grandparent does not

VERIFIED, `G_RunFrame` (`0x50912`..`0x50975`, section 11): the loop counts
the entity number up, calls the runner on the first dword of the link
record (`0x50949`) and on the entity (`0x50955`), and nowhere reads the
parent's own record. INFERRED, off that order and the runner's level-time
stamp (`0x502cb`): a parent runs ahead of its child and never twice, and
nothing runs the parent's own parent first.

VERIFIED, off the capture: the forward chain `c0` 172 (moved), `c1` 173,
`c2` 174 reads in step, all three z 102.40 at 30600. The reversed chain `r2`
175, `r1` 176, `r0` 179 (moved) does not: at 33800 `r0` reads 102.40 and
`r1` and `r2` 100.00, and through the yaw `r1` reads `r0`'s previous frame
plus 45 (35350: 9.00 and 49.50). INFERRED: at 175 the loop runs 176 first,
which reads 179 before 179 has run; 175 then reads 176 current.

### A deleted parent

VERIFIED, `G_FreeEntity` (`0x66948`): `0x66954`..`0x669ee` is
`G_EntUnlink`'s body inlined on the entity itself, and `0x669f1`..`0x66a9e`
the same body looped over the list at `ent+0x2e8`, `G_SetOrigin` and
`G_SetAngle` at each child's own current pose. INFERRED, off the loop's
exit test at `0x66a9c`: every child is unlinked where it stands before the
entity's slot is cleared.

VERIFIED, off the `del_parent` phase: the parent deleted at 37700 mid-move
carries its child for two more frames (124.00 at 37750, 125.20 at 37800) and
the child then holds 125.20. INFERRED: `delete()` frees a tenth of a second
later off the think (object-model doc 14), and the deleted mover runs its
trajectory until then.

### A player parent

VERIFIED, `ClientThink_real` (`0x405bb`..`0x405f6`, just past its
`G_TouchTriggers` call): it copies `ps.origin` into `r.currentOrigin` and
writes 0 to all three of `r.currentAngles`. VERIFIED, off the capture: the player's `.angles` read
`(0, 0, 0)` on every frame but the one `setPlayerAngles((0, 180, 0))` ran
on, and `k1 linkto(player)` with no tag keeps its `(32, 0, 40)` offset and
angles 0, swinging to `(-32, 0, 40)` and yaw 180 for that one frame
(39850). INFERRED: a link to a player's entity ignores where the player
looks.

VERIFIED, off the capture: `k2 linkto(player, "bip01 head")` and
`k3 linkto(player, "tag_weapon_right", (0,0,0), (0,0,0))` follow the
posed bones: `k3` sits on the right hand, `(6.3, -5.2, 48.1)` from the feet,
and both drift a few degrees a frame with the idle. From the
`setPlayerAngles` frame on, `k3`'s yaw reads 179.47, 269.54, 44.62, 21.99 and
then settles back near 0. INFERRED: the bone carries the legs' yaw off the view
(`tag_origin`'s local tag, combat doc 16.4), and the legs swing after the
view and back. VERIFIED, `linkto_ab` with that swing modelled: ours reads
182.16, 272.0, 46.6 and 23.75 on the same frames, and the gate holds the yaw
to 4 degrees.

vcod: `crate::game::link` holds the records and `link::run_linked` is the
link arm of the entity pass (`ScriptRuntime::run_entity_pass`), ascending by
number with the parent run first, a mover parent read at the level time once
it has run. It writes
the child's `origin` and `angles`, relinks it, poses a brush model's clip
and forgets any mover plan; the wire sends a linked entity `TR_INTERPOLATE`.
A player's bone is the hit rig's pose (`link::client_bone`), a script
model's its bind pose. `crates/server/tests/linkto_ab.rs` replays the probe:
every non-player row matches to 0.02 but the yaw residual of section 12
(0.1 units at the children's radius), the player's entity-frame links match
to 0.02, and the bone-linked children sit within 3 units of retail's with
their angles not compared; the body's swing after `setPlayerAngles` is not
modelled. `enableLinkTo`, turrets, a tag parent's model change and an item
unlinked in mid-air are section 16.

## 16. `enableLinkTo`, turrets, model changes and unlinked items

The evidence is a second paired capture against the retail 1.1d Linux
server, mp_carentan under a `dm`-shaped probe, 2026-10-08:
`crates/server/tests/fixtures/movers/mp_carentan-dm-linkto2.txt` (the server
half, from `crates/gsc/tests/fixtures/semantics/client-probes/probe_linkto2.gsc`)
and `mp_carentan-dm-linkto2-wire.txt` (the client half). The player stands
at `(-512, 2688, -15.91)` throughout. Addresses are `game.mp.i386.so`.

### `enableLinkTo` and `Think_GeneralLink`

VERIFIED, `enableLinkTo` (0x5d5d0): object-model doc 23.2 has its tests and
stores; the think it installs is `Think_GeneralLink` (relocation at
0x5d69c). VERIFIED, `Think_GeneralLink` (0x68588): it writes
`ent+0x1fc = level.time + 50` (0x6859a), tests the link record at
`ent+0x2e4` (0x685a0), and calls `G_SetFixedLink(ent, 0)` (0x685af),
`G_SetOrigin` (0x685bf), `G_SetAngle` (0x685d2), writes 1 to `ent+0xc` and
`ent+0x30` (0x685d7, 0x685de) and calls `trap_LinkEntity` (0x685e9).
INFERRED, off the store at 0x6859a sitting ahead of the record test: the
think re-arms itself every frame whether or not the entity is linked, and on
a linked frame does what `G_GeneralLink` does (section 15).

VERIFIED: `Touch_Multi` (0x65a18) raises its `"trigger"` notify (0x65a5a)
before it compares the think with `Think_GeneralLink` (0x65aa2) and
`nextthink` with 0 (0x65ab2); `multi_trigger` (0x65897) and `Use_Multi`
(0x6595f) carry the same compare, and `Activate_trigger_damage` compares
with it at 0x650db and 0x651b8. INFERRED, off the branches those compares
feed: a `trigger_multiple` whose think is `Think_GeneralLink` skips the
`wait` arm (`multi_wait`, or the 100 ms `G_FreeEntity` for a `wait` not above
0) and keeps its think. VERIFIED: `SP_trigger_multiple` (0x64c50) reads
`wait` with the default `"0.5"` (0x79940, `G_SpawnFloat` at 0x64c6e).

VERIFIED, off the capture: the bombzone_A `trigger_multiple` (entity 176,
model `*4`, no `wait` key), moved onto the player by an origin write and not
linked, notified on every touch, two to four a frame with the client's three
cmds a frame, from 10700 to 12150. INFERRED: the notify does not wait out
`wait`; only the think arm behind it does.

VERIFIED, off the capture: after `t enableLinkTo()` and `t linkTo(p)`, `p
moveto(at, 1)` carries the trigger at the parent's pace (`-164.30` at
12750, the player's spot at 13700) and the notifies run from 13450 to 15450,
the frames its box overlaps the player.

VERIFIED, off the capture's first run: `spawn("trigger_radius", origin, 0,
32, 64)` dies with `unable to spawn "trigger_radius" entity`. A trigger to
link has to come from the map.

VERIFIED, off the capture: `enableLinkTo` on a script_origin is the fatal
`entity already has linkTo enabled`.

### A linked turret

VERIFIED: `turret_think` (0x5328c) writes `ent+0x1fc = level.time + 50`
(0x5329e), tests the record (0x532a4) and calls `G_GeneralLink` (0x532b2)
before it reads the gunner. VERIFIED, off the capture: `mg linkTo(mp)` on
the misc_mg42 297 with `mp` at its origin, then `mp movez(32, 1)` and `mp
rotateyaw(45, 1)`, carries the gun from z 175 to 207 and its yaw from 295
to 340 in step with the parent. VERIFIED, off the wire half: entity 297 goes
out `apos.trType` 3 from its spawn, both groups `trType` 1 with `trTime` 0
on every linked snapshot (16650..19150), and both 0 from the unlink on
(19650). VERIFIED, `G_EntUnlink` (0x680d4) calls `G_SetOrigin` (0x680f9) and
`G_SetAngle` (0x68109), and `G_SetAngle` writes 0 to `ent+0x30..0x38`
(0x67dcd..0x67ddb). INFERRED: an unlinked turret never goes back to the 3.

INFERRED, off `turret_think` running in `G_RunFrame`'s entity loop and
`turret_think_client` in `ClientEndFrame` (turrets doc 6): a gunner reads
the gun where this frame's link put it. The capture had no gunner.

### A tag parent's model change

VERIFIED: `G_DObjUpdate` (0x66054) returns at once on a client
(`ent+0x158`, 0x66060); otherwise it frees the DObj and, with model byte
`ent+0x175` 0, calls `G_UpdateTagInfoOfChildren(ent, 0)` (0x6608d), and
with a model builds the DObj and calls it with 1 (0x6617e). The `setModel`
method (0x5dabc) calls `G_SetModel` and then `G_DObjUpdate` (0x5db0d);
`G_EntAttach`, `G_EntDetach` and `G_EntDetachAll` call it too (0x662bc,
0x67fb0, 0x68022). VERIFIED, `G_SetModel` (0x67020): an empty name stores
model byte 0 and returns (0x6702c..0x6703b). VERIFIED,
`G_UpdateTagInfoOfChildren` (0x68294): it walks the list at `ent+0x2e8`, and
for a record with a tag (`record+0x8`, 0x682bc) calls
`trap_DObjGetBoneIndex` (0x682e1) only when its second argument is non-zero
(0x682c7) and stores the result at `record+0xc`; the unlink body at
0x682f4..0x6837b (`G_SetOrigin`, `G_SetAngle`, the list splice, the record
freed) runs past both tests; a record with no tag gets -1 (0x68385).
INFERRED, off those branches: a child on a tag is unlinked where it stands
when the parent has no model or its new model has no such bone, and keeps
its link with the bone looked up again otherwise; a child on the entity's
own frame is never touched. VERIFIED: no call and no relocation in the
module targets `G_UpdateTagInfo` (0x6819c), the same body for one entity.

VERIFIED, off the capture: k1 linked to a script_model on `bip01 head` with
zero offsets sits `(-2.18, 0, 62.68)` from the model's origin with angles
`(282.50, 0, -90)`, the body's bind pose. After `setModel` to
`playerbody_german_wehrmacht` it rides on at the same offset; after
`setModel("xmodel/weapon_thompson")` it stays at z 38.77, the pose script
read on that frame, while k0 (no tag) rides on; setting the body back does
not relink it. Linked again and followed by `setModel("")`, k1 does not
move.

### An item unlinked in mid-air

VERIFIED, `G_RunItem` (0x4eb18): with `groundEntityNum` (`ent+0x7c`) at
0x3ff and `pos.trType` not 5 (0x4eb24, 0x4eb2d) it writes 5 and the level
time (0x4eb33, 0x4eb3f); a type of 0 or 8 then runs only `G_RunThink`
(0x4eb42..0x4eb52).

VERIFIED, off the capture: an `item_health` linked on the frame it spawns
at the player's origin plus 100 (it reads 8 lower, 76.09), carried up 20 and
unlinked, stays at 96.09 for the 2 s sampled; one that had landed at
-22.91, lifted 60 and unlinked, stays at 37.09. VERIFIED, off the wire
half: from the unlink on, both go out `pos.trType` 5 with `trTime` the
snapshot's own time and the same `trBase`, which is how the landed one went
out before its link (30650..30750). The capture does not say which store
puts the type back each frame; ours produced the same rows and wire before
this round, so `crate::game::item::run_items` is unchanged.

vcod: `crate::game::link::enable` is `enableLinkTo` (the link bit is
`link::has_link_bit`), and the touch pass never spends an enabled
`trigger_multiple` (section 17). `linkTo` takes a turret; the record
reads the gun's own pose, so the barrel and gunner follow, and
`TurretRecord::angle_set` keeps `apos.trType` 0 after the unlink.
`link::model_changed`, from the `setModel` builtin on an entity that is not
a client, is `G_UpdateTagInfoOfChildren`. `crates/server/tests/linkto_ab.rs`
replays the probe: every row matches to 0.06 (the player stood 0.04 higher
on ours), the notify frames match, and the turret's and the mid-air item's
wire types and bases match per snapshot. Not done: `attach` and `detach`
re-resolve tags on retail, but ours resolves a tag only in the main model.

## 17. `Touch_Multi`'s `wait`: a free, never a gate

The evidence is a paired capture against the retail 1.1d Linux server,
2026-10-09: `crates/server/tests/fixtures/movers/mp_carentan-dm-trigwait.txt`
from `crates/gsc/tests/fixtures/semantics/client-probes/probe_trigwait.gsc`.
No stock MP map gives a trigger a `wait` key and `wait` is a gsc keyword, so a
script cannot write the field; the capture ran on mp_carentan with four
same-length entity lump edits (the fixture's header), served from a pk3 in
the server's homepath. Addresses are `game.mp.i386.so`.

VERIFIED: `SP_trigger_multiple` (0x64c50) reads `wait` into `ent+0x268` with
the default `"0.5"` (strings at 0x79944 and 0x79940, `G_SpawnFloat` at
0x64c6e) and `random` into `ent+0x26c` with the default `"0"` (0x7994b,
0x79949), and installs `Touch_Multi` and `Use_Multi` (0x64cdd, 0x64ce7).
VERIFIED: `SP_trigger_once` (0x65c0c) stores the float at `.rodata` 0x79ad8,
-1.0, into `ent+0x268` (0x65c16) and installs the same two functions.

VERIFIED, `Touch_Multi` (0x65a18): with the script system active it raises
the `"trigger"` notify, at once through `Scr_Notify` (0x65a5a) when the
pending queue at `level+0x29ec` holds 256 entries and otherwise queued there
(0x65a68..0x65a96), before any test of `wait`. It then stores the toucher at
`ent+0x25c` (0x65a9c) and returns when the think is `Think_GeneralLink`
(0x65aa2) or `nextthink` is non-zero (0x65ab2). Otherwise, with `wait` above
0 (the compare at 0x65abf..0x65acf) it installs `multi_wait` and sets
`nextthink` to `level.time + (wait + random * crandom) * 1000`
(0x65ad1..0x65b28); with `wait` not above 0 it clears the touch function
`ent+0x20c` (0x65b30), sets `nextthink = level.time + 100` (0x65b3f) and
installs `G_FreeEntity` (0x65b48). VERIFIED, `multi_wait` (0x65870) only
zeroes `nextthink`. `multi_trigger` (0x65884) and `Use_Multi` (0x6594c) carry
the same arm without the notify. VERIFIED, `G_TouchTriggers` (0x3fa22..0x3fa8b)
raises the two `"touch"` notifies on every contact and calls the touch
function only when `ent+0x20c` is non-null. INFERRED, off those branches: a
positive `wait` gates nothing a script sees, since `nextthink` gates only the
arm itself; a `wait` not above 0, which every `trigger_once` has, lets the
first touch notify and then frees the trigger two frames on; a trigger whose
think is `Think_GeneralLink` is never spent.

VERIFIED, off the capture (the client sent about three cmds a frame):

- bombzone_A, `"wait" "5"`: three notifies a frame (two to four on a few
  frames) for all 20 frames it sat on the player.
- bombzone_B, `"wait" "0"`: one notify, on the frame after the origin write
  (offset 50). `isdefined` reads 1 at offsets 0, 50 and 100 and 0 from 150,
  and the `trigger_multiple` + `trigger_once` count drops from 4 to 3 on the
  same frame.
- auto1, `"wait" "-1"` after `enableLinkTo` with no parent: notifies on every
  frame, never freed.
- auto2 as a `trigger_once`: one notify at offset 50, freed at 150 like
  bombzone_B.

INFERRED, off the notify landing at offset 50: the touch ran in the cmds
between the frames at offsets 0 and 50, at `level.time` = offset 0, so its
`nextthink` is offset 100 and `G_FreeEntity` ran in that frame's entity pass
(`G_RunEntity`, 0x502bc, called from `G_RunFrame` at 0x50955) after the
frame's timed waits had already read the trigger as defined: `G_RunFrame`
calls `Scr_SetTime` (0x5070c) ahead of that entity loop.

vcod: `crate::game::trigger::Triggers::fire` returns `Fire::Spent` for the
touch that spends a `trigger_multiple` with `wait` not above 0 or any
`trigger_once`, and the touch pass schedules the free `SPENT_FREE_MS` out.
`crate::game::spawn` stores 500 for a `trigger_multiple` with no `wait` key
and -1000 for every `trigger_once`. `random` is read by nothing.
`crates/server/tests/linkto_ab.rs` (`touch_multi_spends_a_trigger_like_retail`)
replays the capture on the same patched lump: the entity numbers, every
frame's `isdefined` and count, and the notify frames all match.

## 18. `func_bobbing` and `func_pendulum`

The two classnames section 12 found storing `TR_SINE`. Three retail runs of
`crates/gsc/tests/fixtures/semantics/client-probes/probe_bobbing.gsc` on
mp_carentan, 2026-10-11, measured them; the probe directory's README has the
recipes and `crates/server/tests/sine_ab.rs` replays all three:

- `crates/server/tests/fixtures/movers/mp_carentan-dm-sine.txt`: five
  script-spawned movers, no client, up to the drop;
- `-sine-map.txt`: a map-placed pair, the bombzone brush models `*5` and
  `*6` rewritten into a bobbing and a pendulum (the fixture header has the
  keys);
- `-sine-wire.txt`: what a `--net-probe --probe-ride` client was sent of a
  spawned pair and of the map-placed pair.

### The spawn functions

VERIFIED, `SP_func_bobbing` (game.mp `0x583d0`): `G_SpawnFloat("speed",
"4")` into `ent->speed` (`+0x1e0`), `"height"` default `"32"`,
`G_SpawnInt("dmg", "2")` into `+0x238`, `"phase"` default `"0"`, then
`trap_SetBrushModel` and `InitMover`. After those it copies
`r.currentOrigin` into `pos.trBase`, stores `pos.trDuration = speed * 1000`
and `pos.trTime = trDuration * phase`, both truncated (FPU control word
`| 0xc00`), `pos.trType` 4 (`0x584c4`), and the height into `trDelta`'s x
for spawnflags 1, y for spawnflags 2, z otherwise.

VERIFIED, `SP_func_pendulum` (`0x56fe0`): `"speed"` default `"30"` into a
local, `dmg`, `phase`, `trap_SetBrushModel`, then the period: `length =
|r.mins[2]|`, at least 8 (`0x75dc4`), `freq = 0.15915494309189535 *
sqrt(g_gravity / (length * 3))` (`0x75dd0`, `0x75dc8`), and
`1000 / freq` truncated (`0x75dd8`, `0x5709a`). It stores that period in
`pos.trDuration` ahead of `InitMover` (`0x570a3`), copies `r.currentOrigin`
to `pos.trBase` and `r.currentAngles` to `apos.trBase`, and writes the
period to `apos.trDuration` (`0x570e2`), `trDuration * phase` truncated to
`apos.trTime`, `apos.trType` 4 (`0x57110`) and the speed local to
`apos.trDelta[2]` (`0x5711a`): a roll swing.

VERIFIED, `InitMover` (`0x56454`), the part Ghidra's export cuts at the
link: `noise` and `light`/`color` keys, `Reached_BinaryMover` into
`ent+0x204` (`0x565da`), `r.currentOrigin = pos1` (`+0x1a4`, `0x565e4`),
`moverState` 0, `r.svFlags` 0x80, `s.eType` 5 (`0x56619`), the link, then
`pos.trBase = pos1`, `pos.trType` 0, a zero `speed` made 100 (`0x75cb8`,
`0x566a4`) and `pos.trDuration` the `pos1`..`pos2` distance over the speed,
at least 1 (`0x566dd`). VERIFIED: the only stores to `+0x1a4` in `.text`
are `SP_func_door`'s (`0x56d83`, `0x56eaf`), so for both classnames `pos1`
is zero and the mover swings about the world origin. The pendulum's own
period store to `pos.trDuration` is overwritten by `InitMover`'s 1; the
bobbing writes its period after.

VERIFIED, off the runs:

- **Spawn origin and `origin` key unused.** `spawn("func_bobbing", (10, 20,
  100))` reads `origin` (0, 0, 0) on the line after the call, and the
  map-placed bobbing with `"origin" "16 32 8"` swings about x 0.
- **Spawnflags.** `spawn(..., 1)` swings along x, `spawn(..., 2)` along y,
  the third argument landing in `spawnflags` (`0x5d2e7`); the pendulum
  ignores them. A script spawn reads no key (`G_SpawnFloat` sees the spawn
  vars `G_ParseSpawnVars` cleared after the last map block), so it takes
  every default: 32 units over 4 s, and 30 degrees.
- **Keys.** The map-placed bobbing (`speed` 3, `height` 40, `phase` 100,
  spawnflags 1) went on the wire with `trTime` 300000, `trDuration` 3000,
  `trDelta` (40, 0, 0); the pendulum (`speed` 20, `phase` 100) with `apos`
  `trTime` 246300, `trDuration` 2463, `trDelta` (0, 0, 20).
- **The pendulum's length.** The periods are 9711 for a script-spawned
  pendulum (model 0, the world) and 2463 for `*6`. The lump-27 floors are
  -636.12 and -40, which give 9704 and 2433; the floors one unit lower give
  both retail periods. VERIFIED, cod_lnxded: `trap_SetBrushModel`
  (0x8089544, and the syscall arm at 0x8088360) takes
  `CM_InlineModel(s.index)` (0x804bac8) and copies `CM_ModelBounds`
  (0x804bbac) into `r.mins`/`r.maxs`, and the submodel loader stores
  each lump-27 bound a unit out (`fld1`, `fsubr` / `fadd` at
  0x804a8e5..0x804a8fa). `SV_LinkEntity` then grows the abs box one more
  unit (`docs/protocol-1.1.md`, "The box an entity links with"), so a brush
  entity's `r.absmin` sits two units outside its lump bounds.
  vcod: `bsp::Model::clip_bounds` is the loader's box, behind
  `GameHost::model_bounds` and `Visibility::model_bounds` (2026-10-12;
  before, both held the lump's).
- **Script reads.** `origin` and `angles` read the trajectory at the last
  frame's level time (section 8), and the roll reads negative through half
  the swing: a sine mover's angles are not normalized.
- **The wire.** `eType` 5, `solid` 0xffffff, `eFlags` 0, `index` the inline
  model number, 0 for a script-spawned one. The bobbing's `apos` and the
  pendulum's `pos` are stationary at the world origin, the pendulum's `pos`
  with `trDuration` 1. A client standing at the world origin was sent the
  spawned pair.

### The drop

VERIFIED: `G_RunMover` reaches `G_MoverTeam` off a non-zero `trType`
(`0x57666`), and `G_MoverTeam`, once the team's push went through, calls
`ent+0x204` for a group whose `trTime + trDuration` the level time has
reached (`0x558a0`..`0x558e9`). `Reached_BinaryMover` (`0x55c9c`) has arms
for mover states 3, 4, 9 and 10 only and `G_Error`s
`"Reached_BinaryMover: bad moverState"` on `InitMover`'s 0. VERIFIED: the
spawned run's last frame is `f 4000`, one bobbing period after its
`trTime` 0, and the console prints that error; a map-placed one with
`phase` 100 ran 40 s untouched. A bobbing spawned at a level time past one
period drops the level on its first entity pass.

VERIFIED, cod_lnxded `Com_Error` (`0x806b93c`) with `ERR_DROP` on a
dedicated server: `com_errorEntered` set, the longjmp, and `0x806b7ec`
prints `ERROR: %s` and runs `SV_Shutdown` (`0x808ad8c`), whose
`SV_FinalMessage` loop is gated on `com_errorEntered` being 0, so no
client is told. VERIFIED by the wire run: the client's last snapshot was
serverTime 3950 and no server command came after. INFERRED: the master
flatline still goes out and the process idles like after `killserver`
(handshake doc, "Shutdown"). The message reaches the console with a 0x15
byte ahead of it.

vcod: `crate::game::mover::spawn_sine` is both spawn functions plus
`InitMover`, called from the map load and from the `spawn` builtin; it
links the entity with the model's bounds and contents -1 and gives it a
mover row whose plans are the two trajectories. `mover::run_one` writes
`origin` and the raw `angles` back each frame like any mover, and
`mover::reached` sets `GameHost::game_error` when a group is due, on the
mover's turn or after its push. The entity pass stops there and
`Server::drop_level` logs the error and shuts the level down without a
final message. A script-spawned mover's clip is a moving copy of the
world's (section 19) that pushes like any brush model (section 20) and
crushes what it cannot place (section 21); a map-placed one moves and pushes
like a script brush model.

VERIFIED, the same probe: a script-spawned entity's unwritten `spawnflags`
reads 0, as `Scr_GetGenericField` reads an int straight off the zeroed
`gentity_t` (`cod11-gsc-object-model.md` 4). vcod reads an unwritten
engine int, float or vector entity field as 0 for that reason.

## 19. A script-spawned brush entity clips as a copy of the world

The evidence is
`crates/gsc/tests/fixtures/semantics/client-probes/probe_spawnclip.gsc` on
the retail 1.1d Linux server, mp_carentan, no client, 2026-10-12:
`crates/server/tests/fixtures/movers/mp_carentan-dm-spawnclip.txt`.

VERIFIED, game.mp: the `spawn` builtin (0x5d268) runs `G_Spawn`, writes the
classname, `r.currentOrigin` and `spawnflags`, and calls
`G_CallSpawnEntity`; nothing writes `s.index`, so it stays 0.
`SP_script_brushmodel` (0x60fb8) calls `trap_SetBrushModel`,
`InitScriptMover`, writes `r.contents` 1 (0x60fd4) and links.
VERIFIED, cod_lnxded: the brush-model setter (0x8089544) takes
`CM_InlineModel(s.index)`, which for 0 is the world's model, copies its
bounds (section 18) and sets `r.bmodel`. The entity clip at 0x8091390 takes
`CM_InlineModel(s.index)` (0x80913a3) for an entity with `r.bmodel` and a
temporary box model of `r.mins`/`r.maxs` (0x804baf8) otherwise, and traces
it with `CM_TransformedBoxTrace` (0x80568d8) at `r.currentOrigin` (`+0x134`)
and, for a bmodel, `r.currentAngles` (`+0x140`).

VERIFIED, off the capture (`bulletTrace` straight down from 100 above the
first dm spawn point, whose floor is at z -9.46):

- With nothing spawned the trace stops on the floor, no entity.
- A `script_brushmodel` spawned at (0, 0, 64) reads `.model` `""` and moves
  the hit 64 up (z 54.54) on the same frame, naming the entity (299).
- After `movez(32, 0.5)` the hit is 96 up (86.54). `notSolid()` puts the
  floor back at once, `solid()` the copy, and `delete()` the floor on the
  same frame.
- A spawned `func_bobbing` swings down from the world origin (section 18);
  the copy sits below the floor all period and the trace reads the floor.

INFERRED: so a brush entity whose `s.index` is 0 carries a copy of the
world's whole clip (brushes, terrain, patches, not the submodels or the
static models) wherever its entity stands, moving with it. A tie with the
world keeps the world's hit (Q3's `SV_ClipMoveToEntities` replaces only on
a nearer fraction or allsolid). No stock map holds a brush entity without
a `*N` model or with `*0`.

vcod: `CollisionWorld::set_world_copy` holds one copy per entity
(origin, angles, `r.contents`), clipped after the world and the posed
submodels in every trace, the sweep turned into the copy's frame and the
box kept axial there (Q3's approximation for a rotated box), and merged by
the rule above; `entity_num` names the copy's entity, and
`point_contents` reads the copies. A capsule's sphere offset is turned into
the copy's frame for its brushes (section 21). `GameHost::link_entity_at` places the
copy of a brush entity with no `*N` (or `*0`) at every link,
`set_link_contents` follows `solid`/`notSolid`, `delete` and
`free_entity` take it out, and a map load clears them all. `bulletTrace`
returns the brush entity a trace stopped on. `spawnclip_ab.rs` replays the
capture line for line.

Section 20 measures the copy on the wire, as a pusher and in the client's
prediction.

## 20. A world copy on the wire, as a pusher, and on the client

The evidence is
`crates/gsc/tests/fixtures/semantics/client-probes/probe_worldcopy.gsc` on
the retail 1.1d Linux server, mp_carentan, with one `--probe-ride` client,
2026-10-12: `crates/server/tests/fixtures/movers/mp_carentan-dm-worldcopy.txt`
(the server's `PROBE f` line per frame: player, health pack, copy) and
`-worldcopy-wire.txt` (the client's `RIDE` line per snapshot, a `RIDE_NEW`
line per entity that entered, and the trajectory lines). The player stands
at (-512, 2688) on the street's terrain (z -23.875) with a pack dropped 40
units east of it; a `script_brushmodel` spawned at (0, 0, -64) rises 96 in
2 s, slides 32 east in 1 s, sinks 96 in 2 s and is deleted; then a
`func_static` is spawned at (0, 0, -20000) and deleted.

### On the wire

VERIFIED, off the capture:

- The `script_brushmodel` enters the snapshot of its spawn frame as
  `eType` 8, `solid` 0xffffff, `index` 0, `eFlags` 0, at (0, 0, -64), and
  leaves it on the frame of its `delete()`, as a `*N` brush model does
  (section 14). Its moves go out as for any script mover.
- The `func_static` enters as `eType` 5, `solid` 0xffffff, `index` 0,
  origin (0, 0, 0) whatever the spawn origin: `SP_func_static` (game.mp
  0x5820c) runs `trap_SetBrushModel` and `InitMover` (section 18), copies
  `r.currentOrigin` into `s.pos.trBase`, and unlinks under spawnflags 1.
  VERIFIED, a scan of every `.bsp` in the 1.1 paks: none holds a
  `func_static`.

VERIFIED, cgame: the inline-model loop at 0x300218b0 registers handles from
1 (`ebp` 1 at 0x3002104e, the count from syscall 0x20) into the table at
0x301d31c0, and nothing else writes it. INFERRED: an `index` 0 brush model
draws through handle 0, which draws no world; vcod draws nothing for one
(`entities::resolve_visual`).

vcod: `crate::game::wire` sends a `script_brushmodel` with no `*N` with
`index` 0, and a `func_static` (`mover::spawn_static`) as `eType` 5 like
the sine movers. `worldcopy_ab.rs`'s
`a_spawned_brush_entity_goes_on_the_wire_like_retail` compares both entry
lines.

### The push

VERIFIED, off the capture:

- **Lifted.** The player and the pack, both on the street (ground 1022),
  leave it the frame the copy's floor passes their feet: the copy at
  script z 0.8, its clip a frame ahead at 3.2 (section 8), the player a
  frame later at -22.58, then 2.4 a frame, the copy's speed, to 8.125, 32
  over the street. The player's ground reads the copy (171) from there.
- **Carried.** Riding the copy east, the player and the pack follow it
  1.6 a frame until something in the real world is in the way: the pack
  stops at x -467.2 and from then on is left in place each frame and falls
  back onto the copy (`trType` 5 every other snapshot), and the player is
  jittered (-4, -4) off its carried spot at x -492.8 and again at -488.8
  (section 12's jitter).
- **Lowered.** On the way down both ride the copy to the street (z -23.07
  on the frame the copy's floor sinks through the street's), then the
  player is jittered once more and both rest on the street.

INFERRED: `G_MoverPush` treats the copy like any brush model: the list
box is the copy's link box, the world's bounds about its origin, so every
player and item is listed; the candidate trace and `G_TryPushingEntity`'s
trace see the copy, and the real world blocks a carried spot.

VERIFIED, vcod measurement against the capture: two things decided
retail's frames. The candidate and spot tests are a trace that does not
move, which against terrain is the capsule position test
(`cod11-mantle.md`, "A trace that does not move"); without it the push came
a frame late and left the player 0.8 inside the copy's floor. And
`G_TryPushingEntity`'s trace is `trap_Trace`'s box against brushes: with
vcod's capsule the player met the obstacle 8 units of travel later and
jittered once where retail jittered twice.

vcod: `mover::clip_step` hands a world copy's move to the push as
`Pusher::Copy`; `CollisionWorld::pusher_box_trace` tests a body against
the copy alone, `box_trace_except` passes through it, and
`box_position_test` is the spot test, boxes against brushes and capsule
position tests against terrain and patches, which the player's and the
item's fit now both ask. A copy whose entity is `notSolid` pushes nothing,
as a non-solid submodel. `worldcopy_ab.rs`'s
`a_moving_world_copy_pushes_like_retail` replays the capture frame for
frame. VERIFIED, the capture: every landing of the pack rests it a fresh
`G_BounceItem` lift (`docs/research/cod11-items.md` 14.2), up to half a
unit, over a floor ours finds to the hundredth (-23.875 on the street,
8.125 on the copy); ours cannot replay retail's `rand()`, so the gate lands
the pack with retail's lift and holds each lift to the range. Its gap: on
the frame after the first lift the player stands 0.11 higher on ours, a
frame in the air whose length is the usercmd's (section 21).

### The client

VERIFIED, cgame: the brush model arm of `CG_ClipMoveToEntities`
(0x30028df0) calls syscall 0x21 with `es.index` and no test of it, and
tests no contents mask on that arm. INFERRED: `CM_InlineModel(0)` is the
world's model, so retail's prediction clips an `index` 0 brush model as a
copy of the whole world at its trajectory, whatever the trace's mask, as
the server does.

vcod: `pmove::movers::SnapshotMovers` keeps an `index` 0 brush model and
places it as a world copy (contents -1) at the prediction time.
`predict_ab.rs`'s `predictor_rides_a_retail_world_copy` predicts the
capture's rider from each snapshot to the next against retail's next
playerstate, exact to 0.05 on every snapshot both stand on the copy with no
jitter between (82 of them); all 82 miss without the copy.

Section 21 measures a turned copy and the `TR_SINE` crush.

## 21. A turned world copy, `func_static` damage and the sine crush

Two captures on the retail 1.1d Linux server, mp_carentan, each with one
`--probe-team allies` client, 2026-10-12:

- `crates/server/tests/fixtures/movers/mp_carentan-dm-worldcopy2.txt`, off
  `client-probes/probe_worldcopy2.gsc`: the player at (-512, 2688) on the
  street and a health pack 40 east; a `script_brushmodel` spawned at
  (-576, 2688, -75.875), so the player stands 64 out along the copy's x on
  the copy's terrain hill (the world's ground about its origin), 8 under the
  street; it rises 16, turns `rotateyaw` 45 and 45 more, back 90, `rotatepitch`
  5 and back, sinks 16 and is deleted. A `PROBE f` line per frame (player,
  pack, copy origin and angles) and a 3 x 3 grid of `bulletTrace`s round the
  player at each rest. Then three `func_static`s at the world origin,
  spawnflags 0, 2 and 4, the second moved 64 up by an `origin` write, and
  three `radiusDamage`s.
- `-worldcopy3.txt`, off `client-probes/probe_worldcopy3.gsc`: the same
  copy and verbs to the end of the unpitch, with a `PROBE g` line per frame
  that prints the player's offset from the copy's spawn origin to the
  ten-thousandth.
- `-sinecrush.txt`, off `client-probes/probe_sinecrush.gsc`: a
  `func_bobbing` spawned on the first frame and the player stood at
  (752, -40), under a sloped ceiling 89 to 96 over its floor, at 2400.

`crates/server/tests/worldcopy_ab.rs` replays all three.

### Clip

VERIFIED, the grid: every trace at yaw 0, 45, 90 and pitch 5 names the copy
(171) or the world where vcod's copy, turned through `AnglesToAxis`, does,
at the same height to the hundredth.

VERIFIED, `cod_lnxded` `CM_TransformedBoxTrace` (0x80568d8): a model is
turned when any of its angles is non-zero and the model is not the box
handle (0x1ff). The start and end go into the model's frame through
`AngleVectors` with the right vector inverted (0x8063388, 0x8065c38); the
trace's box stays axial in that frame (its half extents are not turned);
the capsule's sphere offset, `half height - radius` up, is turned as
`(forward.z, -left.z, up.z) * offset`, Quake III's
`sphere.offset[1] = -matrix[1][2] * t`. The trace work struct carries the
offset at `+0x100..+0x108` (copied in from the sphere argument by
0x8056310). VERIFIED: the brush arm (0x8054e90) reads `+0x100..+0x108` to
pick the sphere nearer each non-axial side (0x805506d..0x8055080); the
terrain sweep (0x8052a58) and position test (0x8053280) read only the
radius and half height (`+0xf8`, `+0xfc`) and shift z. INFERRED: against a
turned model's brushes the capsule stays upright in the world, give or take
the `-left.z` sign that only a roll reaches; against its terrain it tilts
with the model.

vcod: `WorldCopy::sphere` turns the offset and `clip_brush_capsule` takes it;
terrain and patches keep the upright one.

### Push

VERIFIED, the capture, against ours with the player put back at retail's
rest after each fall (below): through both 45-degree turns, the turn back
and the unpitch the player is carried to the hundredth, including the
(-4, -4) jitters the real world forces, and the pack rests and is carried
the same.

Four rows of frames differ, each a player in the air:

- The first lift: two frames in the air sliding 0.86 down the copy's slope
  on retail, 0.11 on ours, which leaves that offset at rest.
- A jitter off a blocked turn (yaw45 frame 14) and one off the lowered copy
  (down frame 13): the fall after each lands a frame apart.
- Pitch frame 15: retail's carried spot is blocked and the player jitters
  (-4, +4); ours is blocked a frame later. The cause of the block is the
  turned centre (below); the frame of difference is the resync, which puts
  ours' player at retail's printed hundredths, up to 0.005 off the real
  spot, where the margin the turned centre leaves at frame 15 is 0.002.

INFERRED, off the first and the wire capture of section 20 (`ct` 52 to 65 ms
apart, a velocity of -42 after one frame of fall): the frames in the air
are the retail client's usercmd lengths; ours sends one cmd a frame. A run
of ours at three cmds a frame missed by more.

### The turned centre

VERIFIED, `cod_lnxded` `CM_TransformedBoxTrace` (0x80568d8): before it
turns anything it takes the box's centre, `(mins + maxs) * 0.5` (the 0.5 at
`.rodata` 0x80cd440, loaded at 0x80568fc), the half extents about it, and
the start and end each plus that centre (the loop at 0x8056940..0x8056974),
less the model's origin (0x805698b on). The turn (`AngleVectors` at
0x8056a6f, `VectorInverse` at 0x8056a78) is applied to those points, and
`CM_BoxTrace` (called at 0x8056c2d) runs on the half extents about the
turned centre. INFERRED: in the copy's frame the box, and the capsule its
terrain test builds, stand about the turned centre, 35 units up a standing
player, not about the turned origin. On a pitch or roll the capsule moves
by `35 * (R^T up - up)` against a body turned rigidly about its feet: about
2 units along the copy's x and 0.06 toward its terrain at 3.5 degrees of
pitch. A yaw leaves `R^T up` as `up`, which is why the yaw turns
matched before.

VERIFIED, the `-worldcopy3` capture: from the rest before the pitch the
player is carried to the ten-thousandth for 14 frames, and the carried spot
of frame 15 (3.5 degrees) is blocked: the player is jittered (-4, +4), as in
`-worldcopy2`. VERIFIED, vcod measurement in the `-worldcopy2` replay: with
the centre turned, the spot test against the copy's terrain triangle 1603
leaves a margin falling from 0.059 at the first pitch frame to 0.011 at
frame 14 and 0.002 at frame 15, and none at 16; turning the origin, the
margin holds 0.059 on every frame and the push carries on. VERIFIED, vcod
measurement in the `-worldcopy3` replay, from retail's rest to the
ten-thousandth: ours is blocked at frame 15 and jitters (-4, +4).

vcod: `CollisionWorld::clip_copy` turns `start + mid` and `end + mid` into
the copy's frame and traces from there less `mid`, `mid` the box's centre;
every trace against a copy goes through it, pmove's and the spot test's
alike. `worldcopy_ab.rs`'s `a_pitched_copy_blocks_its_rider_like_retail`
resyncs ours' player to the `PROBE g` rest before the pitch and holds the
carry to 0.0003 and the block to its frame. Left: the jitter's fall lands
sliding 0.78 down the copy's slope on retail and 0.1 on ours, the same
shape as the first lift's slide.

VERIFIED, vcod measurement against the crush capture: `G_MoverPush`'s
candidate test against a world copy is a box against its brushes, as
against a submodel (section 12). With a capsule ours listed the player
under the sloped ceiling two frames late, where a box corner meets the
slope 5 units lower than the capsule's dome.

### `func_static`

VERIFIED, `SP_func_static` (game.mp 0x5820c): after `InitMover` it stores
`Use_Static` as `use` (`+0x210`), copies `r.currentOrigin` into
`s.pos.trBase` and unlinks under spawnflags 1; a non-zero `health` key, read
unless bit 4 of the byte at `+0x17c` is set, sets `takedamage` (`+0x171`); spawnflags 2 or 4 store
`Static_Pain` as `pain` (`+0x214`), scale `+0x274` by a constant, set
`takedamage`, write `health` 9999 (`+0x230`) and default `+0x250` to 4.
`Use_Static` (0x58070) unlinks a linked entity and links an unlinked one.

VERIFIED, `G_Damage` (0x49dac): an entity with no client and `s.eType` 5
takes its own arm: with a `use` and the byte at `+0x174` zero it calls
`Scr_AddEntity(attacker)`, `Scr_Notify(ent, scr_const+0x92, 1)` (`"trigger"`)
and the `use`, and returns, touching no `health`, `pain` or `die`. INFERRED:
`+0x174` is `moverState`, which `InitMover` zeroes; Quake III's "shootable
doors" rule.

VERIFIED, the capture:

- `health` reads 0, 9999 and 9999 for spawnflags 0, 2 and 4.
- An `origin` write moves the second static's copy at once: a trace 64 west
  of the player lands on it at z 40.12, naming it.
- A `radiusDamage` in the sky over the world's middle, inside every copy's
  link box: neither health moves; the two statics with `takedamage` are
  notified `trigger` with the world (1022), after the caller moved on, the
  second spawned first; the moved one's copy is gone from the next trace.
- A second blast there and one on the street reach neither, unlinked.

vcod: `spawn_static` gives spawnflags 2 or 4 `health` 9999, and either or a
`health` key `takedamage`, marked on the mover row. `GameHost::blast_entities`
lists a linked one with its brush model's box, `Blast::entity_damage`
measures it by box distance (`r.bmodel`, combat doc 14.1) with its own clip
passed through by `CanDamage`'s traces, and `GameHost::damage_nonclient`
takes the `use` arm: `"trigger"` and `mover::use_static`. `Static_Pain` is
never reached.

### The sine crush

VERIFIED, the crush capture: the player stood at 2400 (the `setOrigin` reads
z one up, -6.75) is crushed on that frame's entity pass, the copy at -18.81
pressing a box corner of its sloped ceiling into the head: the damage
callback reads inflictor and attacker the bobbing (299), damage 99999,
`dflags` 0, `MOD_CRUSH`, weapon and hit location `none`, health 100, then
the killed callback with the same. The bobbing goes on and drops the level
at 4000 as in section 18. The corpse settles at -7.16 and then -7.99, 0.12
under a live player's rest.

INFERRED: the 0.12 is the flat-terrain sink of `cod11-player-clip.md` 8.12.
A corpse falls on every cmd and lands again; on a flat partition a fall
whose end stays over the face is not stopped at the 0.125 pad, so the first
cmd after the landing sinks it `0.5 * 800 * dt^2`, 0.116 for a 17 ms cmd
(-7.875 to -7.991), and every later fall would cross the face and is stopped
where it starts. A 50 ms cmd falls a unit, never ends over the face, and
leaves the body at the pad. VERIFIED, vcod measurement 2026-10-12: with one
cmd a frame ours rested at -7.875; sending the probe client's cmds (16, 17
and 17 ms a frame) ours rests at -7.99 from 2550 on, as retail. The first
two rows after the death, 0.41 then a landing on retail, are the length of
the cmds retail's client sent across the death, which ours cannot replay.

vcod: `crate::push::push` returns the players a sine mover could not place
instead of stalling it, `ScriptRuntime::push_mover` hands each `G_Damage`
through `deliver_world_hit` before the end-of-period test, and the mover
moves on. `a_sine_mover_crushes_what_it_cannot_push_like_retail` sends three
cmds a frame and holds every line exact past the corpse's first two rows.
