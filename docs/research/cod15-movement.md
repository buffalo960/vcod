# CoD 1.5 movement against 1.1

What patch 1.5 changed in the movement a client predicts, measured on the
1.5b server and read out of its game module. `cod15-protocol-diff.md` covers
the wire; this covers `bg_pmove` and the weapon machine. The short answer:
1.5 adds a jump penalty, drops the prone dive, steps the eye one stance at a
time with the body's stance following the eye, halves a standing lean, and
touches a handful of step, ladder and weapon details. Pmove's tunables did
not move.

## Sources and conventions

The binaries are `cod15-protocol-diff.md`'s: 1.1d and 1.5b
`game.mp.i386.so`, 1.1 and 1.5 `cgame_mp_x86.dll`. Addresses are `nm`
addresses of the game module unless a cgame VA (0x30000000 base) is named;
Ghidra's import of either module sits 0x10000 higher. Method: the Ghidra
exports of both modules, paired function by function through `PmoveSingle`'s
call order (1.1 0x33dfc, 1.5 0x27eb7), with `tools/re/annotate_func.py`
resolving each call and constant, and the two captures below.

1.5's module is an unoptimised build with every function a frame of its own,
so 1.1's inlined helpers are separate functions there; a different shape is
not a different behaviour. Every claim carries its own label. VERIFIED
means read out of a binary or a capture. INFERRED means read off control
flow.

## The captures

`crates/server/tests/fixtures/playerstate/mp_carentan-dm-slope-8ms-moves-1.1.txt`
and `...-moves-1.5.txt`: one script (`moves_script` in
`crates/client/src/probe.rs`, `--probe-moves`) run on the street placement
of `client-probes/probe_prone` against `tools/run_probe.sh` on each server,
cmds every 8 ms, every snapshot's whole decoded playerstate logged. Dated
2026-10-13. What they show, all VERIFIED:

- 1.5 sets `pm_flags` 0x2000 at a jump's takeoff and keeps it for about
  1.8 s after the landing (`pm_time` 1765 on the first snapshot after the
  landing frame).
- A jump from rest peaks 39.3 to 40.8 up on both (125 fps cmds). Pressed
  about 900 ms after the previous takeoff, 1.5's peaks 16.6 to 18.1 up, four
  times over, standing and running; 1.1's peak 40.0 to 41.1.
- `fJumpPeak` on 1.5 is the takeoff height and stays after the landing; on
  1.1 it is the takeoff plus 39 and reads 0 on the ground.
- The prone press while running forward or backward lifts 1.1's body
  (`pm_flags` 0x4, vz 200) and not 1.5's.
- From standing, 1.5's `viewHeightTarget` steps 60, 40, 11 with `eFlags`
  reading crouch until the second step, about 200 ms in; 1.1's goes
  straight to 11 with `eFlags` prone at once.
- Prone entry on 1.5 sets 0x2000 with `pm_time` 1800 (less the cmd).

`...-turns-1.1.txt` and `...-turns-1.5.txt` (2026-10-13): `turns_script`
(`--probe-turns`) from the same placement facing world yaw 90, a crawl back
until the legs near the kerb, quarter turns each way there, a crawl with the
view swinging, a turn past the yaw cap, then a run, lean runs, a lean
strafe, a sighted walk and a leaning crouch walk. VERIFIED from them: 1.5
reads `pm_flags` 0x8000 on 29 snapshots of the quarter turns, where its
`delta_angles[1]` moves with no cap involved (the view pull of section 4).

`crates/server/tests/moves_ab.rs` replays all four through the client's
predictor, each under its own rules, and the 1.5 captures under 1.1's to
show the gate tells them apart.

## 1. Function map

| Function | 1.1 | 1.5 |
|---|---|---|
| `PmoveSingle` | 0x33dfc | 0x27eb7 |
| ground trace | 0x30474 | 0x21ccd |
| `PM_SetWaterLevel` | 0x30778 | 0x2219f |
| `PM_CrashLand` | 0x2fd68 | 0x2115e |
| `PM_CheckDuck` | 0x316f4 | 0x232c4 |
| `PM_ViewHeightAdjust` | 0x309d8 | 0x226a6 |
| `PM_GetViewHeightLerpTime` | 0x345b8 | 0x223a6 |
| `PM_GetEffectiveStance` | 0x34554 | 0x1e9a5 |
| `PM_DropTimers` | 0x32a44 | 0x25aef |
| `PM_CheckLadderMove` | 0x336e8 | 0x27330 |
| `PM_LadderMove` | 0x33944 | 0x27846 |
| `PM_WalkMove` | 0x2f258 | 0x2003d |
| `PM_AirMove` | 0x2f03c | 0x1fe8d |
| `PM_CheckJump` | 0x2eb98 | 0x1f985 (takeoff 0x1f882) |
| `PM_Friction` | 0x2e460 | 0x1eab3 |
| `PM_CmdScale`, walk / air and ladder | 0x2e690 / 0x2e5bc | 0x1f08b / 0x1eec9 |
| `PM_SlideMove` / `PM_StepSlideMove` | 0x347c0 / 0x34fbc | 0x28d7d / 0x29942 |
| `PM_Footsteps` | 0x322c8 | 0x24ae8 |
| `PM_UpdateViewAngles` / `PM_UpdateLean` | 0x32d7c / 0x32ac8 | 0x26118 / 0x25c2e |
| `BG_CheckProneValid` | 0x2d428 | 0x1cf58 |

Symbols and call sites VERIFIED. The pairing is INFERRED from call order.

## 2. The jump penalty

1.5's 0x2000 rides `pm_time`, the timer 1.1's knockback (0x100) and damage
knockback (0x200) use. In 1.1 the bit is set only by `PM_CrashLand`
(0x2ffab-0x2ffe0, `fJumpOriginZ` non-zero, `pm_time` 200), after the ground
trace has zeroed `fJumpOriginZ` (0x305c8), so it never fires (INFERRED);
1.5 removed that block (VERIFIED by absence).

- The takeoff (0x1f882), called by `PM_CheckJump` with 39.0 (immediate at
  0x1fa4e): `v2 = (float)(gravity * (h + h))`; with 0x2000 set and
  `pm_time <= 1800` (cmp 0x708 at 0x1f906) `v2 /= curve(pm_time)` (0x1f917);
  `vz = sqrt(v2)` (0x1f92c); `pm_flags |= 0x2008` (0x1f94b); `pm_time = 0`
  (0x1f95d); `fJumpOriginZ = origin.z` (0x1f97b), the takeoff and not
  1.1's takeoff plus 39. VERIFIED.
- The curve (0x1ea52 for the jump, 0x1e9f1 for friction, one formula with
  its own rodata each): `t * 1.5 * 0.00058823527 + 1.0` up to t = 1699
  (cmp 0x6a3), 2.5 above (rodata 0x7566c..0x75678, copies at
  0x7567c..0x75688). 1 at 0, 2.5 from 1700 on. VERIFIED.
- The landing, at the top of `PM_WalkMove` before `PM_CheckJump`: with
  0x2000 set, `pm_time > 1800` (a landing stun) clears 0x2000 and
  `fJumpOriginZ` and scales the velocity by 0.65 (0x20076, 0x2009a, rodata
  0x756e8); `pm_time == 0` (the first walking frame after a jump) sets
  `pm_time` 1800 and scales by 0.65 when `origin.z < fJumpOriginZ + 18`
  (0x756ec, 0x20169), else `pm_time` 1200 and scales by 0.5 (0x201e2,
  0x756f0). All three velocity components. The constants VERIFIED. The
  order INFERRED.
- `PM_Friction`'s ground control: 0x100 still multiplies by 0.3; otherwise
  0x2000 with `pm_time <= 1800` multiplies by the curve (0x1ec14..0x1ec30).
  VERIFIED.
- `PM_DropTimers` is 1.1's: it clears 0x100, 0x200 and 0x2000 when the
  timer runs out (mask 0xffffdcff at 0x25b3e) and leaves `fJumpOriginZ`.
  VERIFIED.
- Also cleared, with `fJumpOriginZ`: by the ground trace's steep branch
  (0x220f5, 0x22107), by a failed `PM_CorrectAllSolid` (0x219ec), and by
  `PM_StepSlideMove` on a ladder and for an airborne body whose penalty
  timer runs (0x2997f-0x299b2, 0x299dd). The stores VERIFIED. The
  conditions INFERRED.
- Prone entry arms it: `PM_CheckDuck` sets 0x2000 and `pm_time` 1800 when
  the eye target steps to the prone height (0x24037, 0x24049). VERIFIED.

Worked at gravity 800 (INFERRED from the formula, matched by the capture):
an unpenalised jump takes off at 249.8 and rises 39; at `pm_time` 1800 at
158.0 and 15.6; at 1200 at 174.1 and 18.9. Friction after a landing starts
at 2.5 times 1.1's and falls back to 1.1's over 1.7 s.

The penalty never reaches the accel, the wish speed or the cmd scale (no
read in 0x1ed62, 0x1f08b, 0x1eec9; VERIFIED by absence). Because it runs on
`pm_time`, `PM_SlideMove`'s tail (a running timer hands the primal velocity
back) now applies for 1.2-1.8 s after every jump's landing (INFERRED).

## 3. Stance and the eye

1.5's body stance is the eye's target, `viewHeightTarget` (ps+0xcc), not the
flags.

- `PM_GetEffectiveStance` (0x1e9a5): crouch when the target is the crouch
  view height, prone when it is the prone one, else standing. 1.1's mixed
  the flags with the lerp's target, time and direction. VERIFIED. The jump,
  the walk accel and scale, the ladder check, the footsteps, the lean and
  `BG_PlayerStateToEntityState` read it (INFERRED from the callers).
- `PM_CheckDuck` still sets `pm_flags` 0x1 and 0x2 from the keys with 1.1's
  traces and events, with these changes (VERIFIED unless marked):
  - every stance trace masks bodies out (`tracemask & ~0x2000000`, 0x23a18
    and six more), so another player no longer blocks a rise;
  - an airborne press on a body not already prone is dropped without a
    flag, an 0x8000 or an event (0x237d6, 0x23921);
  - a prone fit clears 0x2 (0x2390c), and so does every cmd that holds
    prone, so 0x1 and 0x2 never stand together;
  - no dive: no write of 0x4, no `sqrt`, no vz (by absence);
  - `BG_CheckProne` takes a body length, 60 here (0x237e3), the same body
    as 1.1's fixed 54 plus the 6 of the box.
- Then the target steps, only while no leg of the eye runs
  (`viewHeightLerpTime == 0`, 0x23e3c): prone asked, standing target to
  the crouch, any other non-prone target to prone (arming the penalty
  above); prone not asked, a prone target to the crouch, otherwise crouch
  or stand by 0x2. VERIFIED.
- The box and `eFlags` 0x20/0x40 follow the effective stance after the step
  (0x24155..0x241ce): 30 prone, 50 crouched, 70 standing. VERIFIED.
- `PM_ViewHeightAdjust`'s curves are 1.1's byte for byte (1.5 .data
  0x814a0, 0x81520, 0x815a0, 0x81680; VERIFIED). The lengths come from
  `PM_GetViewHeightLerpTime` (0x223a6) with the cvars gone: 400 into prone,
  200 down to the crouch (1.1: 150), 400 up to the crouch, 200 to standing
  (VERIFIED immediates). Since the target only moves between legs, 1.1's
  turn back mid-leg is dead code for a live player, and stand to prone is
  two legs with the second starting on the cmd after the first ends
  (INFERRED). The capture's `viewHeightLerpTime` agrees (VERIFIED).

Consequences: a crouch tap runs the full 200 ms down and 400 up; standing to
prone the box is 50 and `eFlags` read crouch for the first leg, and a rise
from prone keeps the 50 box for the 400 ms leg (INFERRED).

A mounted player runs the same `PM_CheckDuck` (`PmoveSingle`'s mounted
arm calls 0x332c4; UO's calls 0x37e16), its flags set to the gun's
stance, so the target and `eFlags` step to the gun's stance on the
mount (VERIFIED decompile and live, `cod11-turrets.md` 14.8).

## 4. Lean, view and prone body

- `PM_UpdateLean`: the scale is 0.25 with a prone effective stance and 0.5
  otherwise, standing included (0x25ce0..0x25cf5, immediates 0x3e800000
  and 0x3f000000; VERIFIED), so a standing lean tops out at half 1.1's.
  Ramp times 350/280 are 1.1's (0x75a60/0x75a64). The wall trace now runs
  for a linked player too (VERIFIED by absence of 1.1's 0x32c63 skip).
- `PM_CmdScale`: a lean counts as the walk, `walkSpeedScale` in place of
  `runSpeedScale * leanSpeedScale` on the ground (the same 0.4 with stock
  values) and newly in the air and on a ladder (0x1eec9). VERIFIED.
- `PM_UpdateViewAngles` (0x26118), the prone branch. The soft edge is
  always on (1.1's `bg_prone_softyawedge` default). VERIFIED. The order,
  INFERRED from the control flow, with `d = AngleDelta(proneDirection,
  view)` folded through the shorts (`AngleNormalize360` 0x36a3c multiplies
  the float by the float 182.04445 on the x87 stack and truncates):
  - the swing: past the soft edge, or moving with `d` non-zero, the
    candidate is 1.1's (55 a second, rodata 0x75a8c, or the view when one
    frame covers it). While the candidate does not fit, it walks a degree
    at a time back toward the old heading (`|delta| > 1`, double 1.0 at
    0x75a90); within a degree it jumps to the old heading, marks a refusal
    and tries once more, and a failure there skips to the cap. A fitting
    candidate is taken only when a 45-unit body (immediate 0x42340000) fits
    along both the view and the candidate, else a refusal;
  - the fit (0x25ff5): `f = |AngleDelta(yaw, view)| / 240` (rodata
    0x75a68), the body tried along `AngleNormalize360Accurate(yaw - (1 - f)
    * AngleDelta(yaw, view))` with length `60 (1 - f) + 45 f` (0x75a74,
    0x75a70), writing the prone body on a fit;
  - the pull: with the view still off the heading, while the 45-unit body
    along the heading or the fit there fails, the view turns toward the
    body a degree at a time (the full remainder within a degree), each turn
    pushed into `delta_angles[1]` and marked a refusal; the heading itself
    moves by the new `d` whenever the 45-unit test failed;
  - the cap: 1.1's 85, measured on the last `d` and placing the view off
    the new heading;
  - a refusal sets 0x8000, and when the final view is within a degree of
    the view before the cmd and on the same side as the cmd's own view,
    0.98 of that degree is added back (rodata 0x75a98).
  The constants and calls VERIFIED. VERIFIED, vcod measurement (2026-10-13,
  `moves_ab.rs`): the turns capture's `proneDirection`, view and
  `delta_angles[1]` match on all 1100 snapshots, the 29 with 0x8000
  included; under 1.1's rules 54 `proneDirection` and 140 `delta_angles[1]`
  rows miss.
- `BG_CheckProneValid` (0x1cf58): the body length is an argument (60 from
  every caller but the swing; the sweep is the length less 6 and the bent
  reach the length less 30). A sample landing on a plane with normal.z <
  0.7 refuses outright, ground or not (0x1d842, 0x1dd86, 0x1def4, rodata
  0x74f88). A blocked bent-leg sweep from the hips tries once more from the
  hit 18 units up (rodata 0x74fb0). An off-ground caller with a clear sweep
  still runs the ground samples and writes the body they give; a sample
  that misses lays it flat and accepts, as 1.1's early return did. The
  constants and trace counts VERIFIED. The order INFERRED.
- `PM_UpdatePronePitch` is 1.1's in effect (VERIFIED constants).

### The airborne prone body

VERIFIED, `game.mp.i386.so` 1.5b: `PM_UpdatePronePitch` (0x26d80) calls
`BG_CheckProne` (0x26ed6) only when `groundEntityNum` (ps `+0x54`) is
0x3ff, and hands it `fTorsoHeight`, `fTorsoPitch` and `fWaistPitch` (ps
`+0x3c4`, `+0x3c8`, `+0x3cc`) to write. INFERRED, from the order: the call
runs at the cmd's start, so it samples the ground under the origin the
previous cmd left. An off-ground caller with a clear sweep still runs the
ground samples (above), so a prone body that drops writes the chest
height and pitches it reads from mid-air, and nothing rewrites them once
it lands and lies still.

VERIFIED, live (retail 1.5b and UO 1.51b, 2026-10-16,
`client-probes/probe_pronebones`, fixtures in
`crates/server/tests/fixtures/pronebones/`): a prone body dropped from
`setorigin`'s raised unit onto mp_carentan's flat spot (1435.88, 2675.88)
lands at z -23.875 with `fTorsoHeight` -0.1341343 on both servers, where
the ground under him gives about 0. Its last airborne cmd started 50 ms
into the fall at -23.740 (the snapshot before the landing), and -17.8745
(the chest sample's hit) - (-23.740) - 6 is -0.1345. The same drop timed
16, 16, 18 ms from a cmd on the placement's frame (the first placement
after a spawn) leaves -0.3521347: the cmd that starts at -23.5225. On
mp_carentan's mound the drop is longer and the body slides on landing,
so the slide's own checks rewrite the pitches (-0.536, -13.760, -12.524
on both). On 1.1 an off-ground caller gets the flat body, so 1.1's lies
flat after any drop (cod11-mantle.md, "The fit check").

VERIFIED, same runs: the net probe's cmds run 16, 16 and 18 ms apart, the
last on each server frame's time, and that one arrives after the frame has
run (the snapshot's `commandTime` on the placement's frame is the frame's
time less 18). vcod reproduces each wire body above to the float when its
gate's cmds follow that cadence (`ProbeHold::framed`,
`pronepose_ab`), and misses by up to 0.3 units on an even 16 ms one. A
retail run with a 2 ms cmd inside the drop lands with another body; so
does one whose first lie-down took a heading off the view (`proneDirection`
-45 where the client held 270), which the fixtures exclude: that heading is
the spawn point's yaw, below.

### The spawn's lie-down

VERIFIED, `game.mp.i386.so` 1.5b `ClientSpawn` (0x3c9a8): after the
`gclient_t` memset it reads the client's last usercmd into `pers.cmd`
(`trap_GetUsercmd`, 0x3cce6), sets the view with `SetClientViewAngle`
(0x3ceac), stamps `pers.cmd.serverTime` with `level.time` (0x3cefe), sets
`commandTime` to `level.time - 100` and runs `ClientThink_real` on
`pers.cmd` (0x3cf35). UO 1.51b's (0x44df4) is the same: `trap_GetUsercmd`
at 0x45162, `ClientThink_real` at 0x45409. 1.1d's runs a zeroed cmd
(`memset` of 0x18 bytes at 0x42a2f) whose angles are the negated
`delta_angles` (cod11-combat.md, 9.2). INFERRED: on 1.5 and UO the spawn's
100 ms think runs the client's held keys and stance at the spawn's view.

VERIFIED, `game.mp.i386.so` 1.5b: `PmoveSingle` calls `PM_CheckDuck`
(0x28814) before the ground trace (0x28819), and `PM_CheckDuck` drops a
prone press while `groundEntityNum` (ps `+0x54`) is 0x3ff (0x237d6). The
memset leaves it 0. INFERRED: the think's first step takes the press even
for a spawn point above the ground.

VERIFIED, live (retail 1.1d, 1.5b, UO 1.51b, 2026-10-16,
`client-probes/probe_pronebones` with `probe_spawn_yaw 315`, two runs each,
fixtures `crates/server/tests/fixtures/pronebones/mp_brecourt-spawn315*.txt`):
a client holding prone at world yaw 270 is `spawn`ed on mp_brecourt's spot
at yaw 315 and turned to 270 on the same frame. On 1.5 and UO the spawn's
snapshot is already prone (`pm_flags` 0x40001) with `proneDirection` -45,
and the heading stays -45 for the rest of the run with the view 45 off it.
The eye's prone leg steps 166 ms after the spawn (`pm_time` 1784 on the
snapshot 200 ms in). INFERRED: the view inside the soft edge and no move,
nothing swings the heading; the crouch leg started at the end of the
think's first 66 ms step. On 1.1 the spawn's snapshot
stands (`pm_flags` 0x40000) and the next cmd lies down along 270 (-90). A
dm spawn point's yaw is what the brecourt runs above lay down along.

VERIFIED, same runs: through the crouch leg the legs play the crouch idle
(`pb_crouch_alert_unarmed`), then the prone one: 1.5's `PM_Footsteps`
calls `PM_GetEffectiveStance` (0x24c46), so the anim's stance is the
stepped eye target (section 3). 1.1's run reads `pb_crouch_alert_unarmed`
for one snapshot too: 1.1's `PM_Footsteps` picks its idles off
`viewHeightLerpTarget` alone (player-model-anim-system.md, "How retail
picks the movetype", "The stance").

VERIFIED, `game.mp.i386.so` 1.5b `ClientSpawn`: every conditional jump
in it lands at or before 0x3cc41, and none lies between there and the
`ClientThink_real` call (0x3cf35); UO's last lands at 0x450bd, before its
call at 0x45409. INFERRED: a spectator's spawn thinks on the last cmd as a
player's does.
VERIFIED, live (retail 1.1d, 1.5b and UO 1.51b, 2026-10-16,
`client-probes/probe_specspawn` beside `--probe-edge spec-fly`, fixtures
`crates/server/tests/fixtures/specspawn/mp_carentan*.txt`): a spectator
holding forward, `spawn`ed again as a spectator at (-224, 60, 200) facing
0, reads (-200.402, 60, 200) in the spawn frame's snapshot on 1.5 and UO
(100 ms of flight on his last cmd, five spawns each) and (-224, 60, 200) on
1.1. The script's own read of `self.origin` straight after the `spawn`
gives the same. INFERRED from the call order above: `ClientThink_real`
returns inside `ClientSpawn`, which the builtin (1.1d 0x455cc) calls before
it returns to the script.

vcod: the sim moves in the frame's spawn pass, after the spawning thread,
so the `spawn` builtin runs the same think on a fresh sim (the client's
last cmd mirrored into `GameHost::client_last_cmd`) and writes and links
that origin; `specspawn_ab` holds the `PROBE spec` lines to retail's.

vcod: `ClientSim::spawn_cmd` runs the client's last cmd under 1.5's and
UO's rules, a 1.5 spawn starts with the body on the ground, and
`update_anims` takes 1.5's stance off the eye target. `pronepose_ab`'s
three `a_spawn_lies_down_on_its_own_yaw_*` gates hold the heading, the
prone body and every bone to retail's; `specspawn_ab` holds the spectator's
spawn frame.

The framed cadence elsewhere. VERIFIED, vcod measurement (2026-10-16,
`ProbeHold::framed` forced on for every `ProbeHold` user): `scriptmodel_ab`,
`vehicle_ram_ab` and `vehicle_shell_ab` pass either way at both installs;
at the UO install `vehicle_hits_ab` loses one test and `vehicle_armor_ab`
two (a pistol round that hits a seat on retail misses on ours); at 1.1
`scriptmodel_ab`'s prone rest on a grade fails. INFERRED: a gate's cadence
has to be the one its capture's `CMDS` lines show, so `framed` stays a
per-gate choice: on for a capture taken with the fall probe's 16/16/18
cmds whose result rides an airborne cmd's start, off for the vehicle
probes' 16 ms holds.

## 5. Ground, step, ladder, landing

- Ground trace: 1.1 zeroes `fJumpOriginZ` on every hit (0x305c8); 1.5 only
  in the steep branch. A new `pml` int at +0x34 ("near the ground", a hit
  within a unit, or within 1/64 of a 64-unit trace on a lift) replaces
  `pml.groundPlane` in `PM_UpdateAimDownSightFlag`'s test. The stores
  VERIFIED. The use INFERRED. Not modelled.
- `PM_Friction`: under one unit of speed 1.5 zeroes x, y and z (0x1eb4f),
  1.1 only x and y; air moves call it, so a vertical jump's apex can snap
  vz to 0. VERIFIED.
- `PM_StepSlideMove` (VERIFIED unless marked): the jump step tests 0x2000
  and climbs to `fJumpOriginZ + 39` (0x75ca8); a rising climber steps
  whether blocked or not; the ground snap reaches 9 (0x75cb4) where 1.1
  reaches half the step size (5 prone); the ground-plane flag is
  `pml.groundPlane`, a steep plane's included (INFERRED); and the step
  event, its velocity scale, its footstep and the prone check run only for
  a move that started on a ground plane off a ladder (0x2a46f), where 1.1
  ran them for every move past the entry gate.
- `PM_CheckLadderMove`: skipped only while `pm_time` runs with 0x100 or
  0x200 set (0x2734f-0x27369), not under the penalty; a new grab needs a
  second hit along the face's normal (0x27652, 0x277a9); leaving the
  rungs airborne plays the jump anim (0x27838). VERIFIED calls. The climb
  itself is 1.1's: `mp_ship-dm-ladder-1.5.txt` and `-uo.txt` settle at
  1.1's rates and `ladder_ab.rs` matches both under their rules
  (`cod11-mantle.md`, "Ladders"). VERIFIED. The climb's accelerate is a
  call (0x27a7e to 0x1ed62) where 1.1 inlines it; its `ps.friction`
  divide (0x1ee4f) divides by the 1.0 `ClientSpawn` writes (0x3ce4b), the
  field's only store (`cod11-mantle.md`, "Ladders"). VERIFIED.
- `PM_CrashLand`: the land anim plays from the damage branch only (0x2144f),
  where 1.1 played it for any landing under -220. VERIFIED.
- `PM_trace` (0x1e738), new: a trace starting inside a body adds it to the
  touch list, drops bodies from the mask for the rest of the move, and
  retraces. Every mover trace but `PM_GroundTraceMissed` goes through it.
  VERIFIED. Not modelled.
- `PmoveSingle`: `PM_Footsteps` runs before `PM_Weapon` (0x28889, 0x2888e;
  1.1 after, 0x34331/0x34336); the talk button keeps the sight bit
  (`buttons &= 0x12`, 0x27f5e); the ADS-prone move zeroing tests the
  effective stance; every non-normal arm clears the ladder flag. VERIFIED.
- `PM_Footsteps` (0x24ae8): the rate formula is 1.1's
  (`cod11-sound-system.md` 9a), every intermediate stored as a float, K at
  rodata 0x75a2c-0x75a48, the stance the effective one. VERIFIED. A lean
  counts as the walk in the rate (`pm_flags` 0x80 or `leanf` non-zero,
  0x24bf7) and in the step ids (0x3105c), but not in
  `PM_ShouldMakeFootsteps` (0x24a5a), so a lean run is audible with walk
  ids. VERIFIED. The ladder branch also skips `pm_type` 1 and takes its
  quiet window from `pm_ladderJumpTime` (300, `.rodata` 0x75630). VERIFIED.
  An airborne body, ladder or not, whose effective stance differs from its
  flags' (`pm_flags & 3` against 1 prone, 2 crouch, asm 0x24e51) falls
  through to the ground code. VERIFIED.
- `PM_Weapon` 0x30643: 1.1's 255 store for a prone body with move keys or a
  swing (`cod11-combat.md` 2.1). VERIFIED.

## 6. Weapons and view

- `PM_UpdateAimDownSightLerp` (0x2d1a9): `rechamberWhileAds 0` with
  `weaponstate` 4 lowers the sight (cmp at 0x2d2f2). Stock 1.5 sets 0 on
  the springfield, kar98k_sniper and mosin_nagant_sniper and 1 on every
  other gun; a missing key parses as 0. VERIFIED.
- Reload start with a spent case (0x2dd15, asm 0x2deee): the delay is the
  bolt time when under the reload's own delay, else the reload's own (1.1:
  else 1), and never 0. VERIFIED.
- `disableWeapon` sets `pm_flags` 0x100000, which the weapon change treats
  like the ladder bit: the change check puts a held weapon away to 0
  (0x2f937) and the pickup raises 0 (0x2e4bc). VERIFIED. vcod's pmove
  holsters on `PlayerState::weapon_disabled` the way it does on the ladder,
  and the wire rewrite carries the bit as `flags::PMF_WEAPON_DISABLED`
  (cod15-protocol-diff.md 13).
- The weapon file table grew five fields (`rechamberWhileAds` 0x2d0,
  `adsViewErrorMin`/`Max` 0x2d4/0x2d8, `horizViewJitter`/`vertViewJitter`
  0x41c/0x420); every field from `cookOffHold` on moved 0xc. VERIFIED
  (table at 0x817a0, 253 entries). vcod parses by key.
- `BG_WeaponFireRecoil` (0x3246c) kicks the server's gun spring off
  `G_PlayerEvent` (0x3b2e4) on fire events, into `gclient` past the
  playerstate: a server-side aim offset with the server's RNG, nothing on
  the wire, nothing to predict. The cgame's own view kick is 1.1's.
  VERIFIED, by a capture; the section below has the reads.
- Bob: the cvars are `cg_bobAmplitudeStanding` 0.007, `cg_bobAmplitudeDucked`
  0.0075, `cg_bobAmplitudeProne` 0.03 and `cg_bobMax` 8; `bg_bob*` are the
  C variables behind them. VERIFIED. 1.1's cgame already registers the
  four (table 0x30074d70..0x30074da0, flag 0x200); 1.5's game module adds
  them (`.data` 0x83990..0x839d8, flag 0x800) and 1.5's cgame registers them
  with 0xa00 (0x30077220..0x30077250), so a 1.5 server's values reach its
  clients through the 140/204 mirror. VERIFIED. They scale the gun bob
  (cgame 1.5 0x30013520, capped at 10) and the eye's walk bob
  (`CG_OffsetFirstPersonView` 1.1 0x30032ae0, 1.5 0x30034180, through
  `BG_GetVerticalBobFactor` and `BG_GetHorizontalBobFactor`, 1.5 game
  0x30a54 and 0x30b37, capped at `cg_bobMax`), each amplitude the speed
  times the stance's cvar. VERIFIED. The sight's view bob (capped at 45)
  reads the same three, as it shares the gun bob's stance multiplier in
  1.1's game module. INFERRED. 1.5 picks the stance by `viewHeightTarget`, the stepped eye
  target. VERIFIED. 1.5's `CalcMuzzlePoint` adds the eye bob to the
  server's shot origin. VERIFIED. The eye is never set below the origin
  plus 8 (cgame 1.1 0x30032de5). VERIFIED.
- `PM_Weapon`'s other states, the ADS flag, the walking flag and the spread
  are 1.1's (side-by-side decompile, INFERRED).

### The server's gun kick

1.1d's game module never kicks the gun-kick spring of `cod11-combat.md`
15.2, so a 1.1 server's aim block adds nothing for it. 1.5b's does, and
so does United Offensive's (both VERIFIED by the captures below).

- `BG_PlayerStateToEntityState` (1.5b 0x1c449, which `ClientThink_real`,
  0x39116, calls after `Pmove` and `ClientEndFrame`, 0x3ab2a, calls again)
  hands every event from `entityEventSequence` to `eventSequence` to
  `G_PlayerEvent(entnum, event)` before it copies it to the entity.
  VERIFIED. `ClientThink_real` runs `ClientEvents`, which fires the shot,
  after that copy. VERIFIED, the call order. So a shot's kick reaches the
  aim of the next cmd, never its own. INFERRED.
- `G_PlayerEvent` (1.5b 0x3b2e4) calls `BG_WeaponFireRecoil(ps,
  &client+0x22c0, local)` for events 159, 160, 161 and 168
  (`EV_FIRE_WEAPON`, `B`, `LASTSHOT`, `MG42`; 1.5's ids are 1.1's) and
  nothing else. UO's (0x435cc) does the same for its 163..166 and 173,
  which adds `EV_FIRE_WEAPONC`, with the spring at `client+0x4714`.
  VERIFIED.
- `BG_WeaponFireRecoil` (1.5b 0x3246c, UO 0x396bc) draws four `rand()`s,
  each scaled by 1/2^31 (`.rodata` 0x77758 is 2147483648.0, UO 0x9d990):
  the view kick's pitch and yaw, off the `ads` pair when `fWeaponPosFrac`
  is exactly 1.0 (0x77754) and the `hip` pair otherwise, written into the
  caller's local and dropped; then the gun kick's pitch and yaw, off the
  `ads` pair when the fraction is above 0 (0x77760), added to the spring's
  two speeds. `min + (max - min) * r` each. The 1.5 file offsets are 1.1's
  plus 0xc (gun kick 0x35c..0x368 and 0x39c..0x3a8, view kick 0x37c..0x388
  and 0x3bc..0x3c8); UO's sit at 0x3d4..0x448. VERIFIED.
- The spring itself (1.5b 0x31940 stepping 0x3171a per axis) is 1.1's: 5 ms
  substeps (0x77700) of `msec * 0.001`, the four keys blended hip to sight
  by the fraction, settled under 0.25 degrees (0x776e8) and 1 a second
  (0x776f0), and run only for an `aimDownSight` weapon. Its angles reach the
  shot only down a sight with the fraction non-zero, through the aim
  block's composition. VERIFIED, the reads and constants; INFERRED, the
  order.

Measured, VERIFIED: `gunkick_ab.rs`'s three captures
(`mp_carentan-dm-slope-8ms-gunkick-{1.1,1.5,uo}.txt`) hold a thompson whose
file has no spread and its gun-kick ranges pinned (pitch -60, yaw 40), held
from the hip, then down the sight, at a wall 1091 units out. From the hip
every round of all three strikes one spot. Down the sight 1.1d's thirteen
rounds stay within two units of it; 1.5b's climb from z 18 to 81 and UO's
from 15 to 81, each a few units across, the spring at its 4-degree
`gunMaxPitch`. Ours replays the cmds on retail's level times and lands every
round within a unit of retail's on all three.

Measured, VERIFIED (live, retail UO 1.51b, 2026-10-17): the
`vehicle_armor_ab` sweep recipe on mp_cassino with the shooter's
`--save-slope` on, its thompson file's spread zeroed and the kick left
stock (`adsGunKickPitch` -55..-50, yaw -60..90). The net probe sends its
cmds on a 60 Hz grid, 0, 16 and 32 or 33 ms past each frame's time, so
the 85 ms `fireTime` fires on every sixth cmd, 100 ms apart, and every
round left within 0.3 degrees of the view the gsc set. With the kick
pinned as above in the same recipe the rounds sat 3.0 to 3.35 degrees
above the view. The spring's balance turns on the round spacing: at
100 ms a stock round's kick settles before the next, at 96 ms (even
16 ms cmds, the sixth at 96) the remainder adds up to `gunMaxPitch`
over about 12 s (INFERRED, the spring stepped by hand both ways; vcod's
spring lands the same at each spacing). A gate whose held trigger fires under
the kick sends the probe's 60 Hz cmds (`ProbeHold::sixty`).

**As implemented.** `ClientSim::fire_recoil` (`crates/server/src/spectate.rs`)
draws the four off the server's RNG for each fire event a cmd's pmove
raised, under `MoveRules::v15()` only, and `ClientSim::update_aim` steps
the spring (`pmove::aim::GunKick`) before each cmd's aim block and hands it
in. A spawn clears it with the rest of the aim state.

## 7. Stored floats

1.5b's and UO's modules store every float variable where 1.1d's keep the
x87 stack, so a fall's last bits differ between the builds. Three stores
reach a player's origin and velocity. Addresses: 1.5b `game.mp.i386.so`
and `cod_lnxded`, UO `game.mp.uo.i386.so` and `coduo_lnxded`.

- `PM_SlideMove`'s gravity (1.5b 0x28e20-0x28e5f, UO 0x2e714-0x2e753):
  `velocity[2] - gravity * frametime` is stored (0x28e34, UO 0x2e728)
  before `(velocity[2] + end) * 0.5` reads it back. 1.1d averages the
  unrounded end (`cod11-mantle.md`, "Frame flow"). VERIFIED.
- The tail of the default arm (1.5b 0x288ad-0x289e3, UO 0x2e072 on): the
  displacement's three components are stored (0x288b3, 0x288cc, 0x288e5),
  the rate `|d|^2 / frametime^2` is stored (0x28912) and so is the squared
  speed (0x28967); the speed times 0.25 (rodata 0x75ae0) is compared with
  the stored rate, and each new component is `(1.0 / frametime) * d`
  computed on the stack (0x28993-0x289e3). 1.1d keeps x on the stack and
  never stores the rate or the speed (`cod11-mantle.md`, "The tail of the
  default arm"). VERIFIED.
- `CM_BoxTrace`'s end point (1.5b 0x805aba1, UO 0x805a9d5): the shifted
  end is stored (0x805acdd, UO 0x805ab11) before the delta is taken from
  it, where 1.1d's 0x8056310 keeps it on the stack; the end point is still
  `start + fraction * delta` rounded once (0x805b2xx). VERIFIED. On a clear
  fall the end point is therefore not 1.1d's in the last bit.

VERIFIED, `spawnview_ab` against the 1.5b and UO logs
(`cod11-gametypes-re-bel.md` 8.6): with the three in, a spawn think from
2 under the spot takes the move (3.0888) and one from 4 over it ends at
-0.00680161, both as retail, and the other heights stay right. Before,
vcod's tail ran in `f32` throughout and its trace ended where 1.1d's does.
VERIFIED, `moves_ab`: the 1.5 moves capture's one `velocity` miss, a frame
past a jump's apex, goes with them once its replay world runs
`Arith::Stored`.

vcod: `pmove::clamp_velocity_to_move` and `slide_move`'s gravity take the
stores under `MoveRules::v15()`; `collision::cm_endpos` takes the stored
end under `Arith::Stored`, which the server sets on protocols 6 and 22 and
the client's live world, `play::replay` and `play::loopback` set off the
connection's protocol (`Arith::for_protocol`), so the predictor computes
what a Linux 1.5b or UO server does. INFERRED: a Windows server's or
cgame's arithmetic is MSVC's and was not read.

## Unchanged

Every `pm_*` tunable (1.5 rodata 0x75624-0x75664 against 1.1's
0x70824-0x70864), jump height 39, the 500 ms jump gate, the 300 ms ladder
re-grab, the spread kick of 64 capped at 255, `PM_SlideMove`,
`PM_ClipVelocity`, the accelerate, `PM_LadderMove`, `PM_AirMove`, the dead
friction, `PM_SetWaterLevel`, the stun and fall damage. VERIFIED.

## What vcod does with this

`pmove::MoveRules` picks the patch; `PlayerState::rules` carries it, set by
`predict::from_wire` off the connection's protocol, so the client predicts
1.5 on a 1.5 server; a protocol 6 `vcod-server` runs it for every client,
bots included (cod15-protocol-diff.md 13). Under `V1_5`
pmove runs sections 2 and 3, all of 4, the friction, ground trace, step,
ladder, land anim and footstep changes of 5, and the `rechamberWhileAds`,
bolt delay and `disableWeapon` of 6; `pm_flags` 0x2000 keeps its place
through the wire rewrite and 0x100000 lands on `flags::PMF_WEAPON_DISABLED`. The client reads the bob cvars out
of the mirror on a 1.5 server (`pmove::BobCvars`, the defaults elsewhere)
and draws the eye bob on both. The ADS-prone zeroing runs on every patch
(`coduo-movement.md` 13). Not modelled: `PM_trace`, the near-ground ADS
test and the talk-button zeroing.
