# United Offensive 1.51 vehicles from the seat

How a United Offensive player drives, rides and fires a vehicle: what the
client sends from a seat, what the server makes of it, what the client
predicts there and where its camera sits. `coduo-diff.md` 9 covers the
vehicle as an entity (definition files, spawn, physics outline, damage,
how the cgame poses and draws it); sections 1 to 9 here are the
occupant's side, 10 to 14 the server's: spawn, seats, the drive step, the
guns and the damage as `vcod-server` reproduces them.
The short answer: the server owns the vehicle and the body in it, the
client predicts only the seated player's view angles and sight, and the
camera comes off the posed vehicle's tags.

## Sources and conventions

- `uo/game.mp.uo.i386.so` 1.51b and `uo/uo_cgame_mp_x86.dll` 1.51, md5s in
  `coduo-diff.md`. Game addresses are `nm` addresses (Ghidra's import sits
  0x10000 higher); cgame addresses are virtual at 0x30000000;
  `CoDUOMP.exe` addresses at 0x00400000.
- Struct offsets name `ps` fields through the netfield table: `pm_type`
  0x4, `pm_flags` 0xc, `delta_angles` 0x4c, `eFlags` 0x84, `weapon` 0xd8,
  `fWeaponPosFrac` 0xe0, `viewangles` 0xe8, `viewlocked_entNum` 0x5bc,
  `vehPos` 0x614, `vehType` 0x618. The cgame's `cg.predictedPlayerState`
  sits at 0x304831c4, `centity_t` at 0x3048c6e0 with stride 0x288, its
  +0x88 the vehicle entity's `surfType` (the file's type: 1 four-wheel, 2
  tank, 5 artillery).
- `eFlags` are UO's raw bits here: 0x100000 in a seat, 0x200000 the
  look-along-the-hull flag, 0x400000 popped out of the hatch. vcod's
  decoder shifts them one place up (`EF_IN_VEHICLE_V22` 0x200000,
  `EF_HULL_VIEW_V22` 0x400000, `EF_POPPED_OUT_V22` 0x800000).
- Method: the Ghidra exports of both modules, `objdump` of `CoDUOMP.exe`
  and the cgame, constants read from the file bytes, and the two captures
  of section 8. Every claim carries its own label.

## 1. Getting in and out

- **Picking the vehicle.** `G_GetActivateEnt` (0x56e85) gathers entities
  with contents 0x200000 in a box ±(192, 192, 96) around the eye; a
  vehicle (`eType` 12) must be the first thing a 128-unit trace along the
  view (mask 0x2810011) hits, within 128 units and a view dot of at least
  0.76, and the lowest `(1 - (dot - 0.76) / 0.24) * 256 + distance` wins
  (VERIFIED decompile).
- **Cursor hint.** `G_CheckForCursorHints` (0x575bb) sets
  `serverCursorHint` 2 (`hintActivate`) and `serverCursorHintString` to the
  vehicle file's `useHintString`, and skips everything for a seated player
  (VERIFIED decompile). The strings are `GMI_VEHICLE_*_HINT` ("Press
  [{+activate}] to use the Panzer", ...) in `gmi_vehicle.str` (VERIFIED,
  the paks); the capture shows `serverCursorHint` 2 with string index 6 on
  foot beside the Panzer IV and 5 beside the Willys (VERIFIED, capture).
  The cgame draws no vehicle-specific icon: hint 2 is `hintActivate` in
  the table at 0x3044b6f8 (VERIFIED).
- **The use press.** `Cmd_Activate_f` (0x4dffb) runs on the use button's
  press edge. For a seated player it calls the vehicle's use (the exit);
  otherwise it calls `G_IsVehicleUsable` (0x8b673) on the hint entity
  and then the use (INFERRED, control flow). `G_IsVehicleUsable` wants a
  free seat, the vehicle at most 100 units/s, health, contents 0x200000,
  no grenade in hand (`grenadeTimeLeft` 0) and every occupant on the
  player's team (VERIFIED decompile).
- **Entering** (0x8998d, VERIFIED decompile): the player must be on the
  ground or falling slower than 80, and 250 ms past its last seat change;
  the seat is the first free one from 1 (seat 2 needs `hasGunner`, seat n
  from 3 needs `numPassengers > n - 3`). The mount sets `eFlags` 0x300000,
  `viewlocked_entNum`, `vehPos`, `vehType`, clears `pm_flags` 0x20 and
  sends `v cl_run 1` and `v cl_stance 0`. It calls `SetClientViewAngle`
  (0x44301) with the angles of the seat's bone (`tag_player` for seat 1,
  `tag_secondary_gun` for 2, `tag_body` else) at roll 0, which rewrites
  `delta_angles`. `ps.weapon` is not written: the infantry weapon stays.
  The capture's entries leave the view where it was and move
  `delta_angles` (VERIFIED, capture).
- **pm_type.** `G_RunClient` (0x41dfc) puts a linked client in `pm_type` 1
  (VERIFIED decompile), and every seated snapshot of the captures reads 1
  (VERIFIED, capture).
- **Exiting** is the use button again: `VEH_UnlinkPlayer` (0x89fb2)
  refuses a tank's voluntary exit for 1500 ms after the entry
  (`GMI_GAME_VEHICLE_EXIT_DELAY`), places the body at
  `VEH_FindValidDismountSpot` (0x88dee) or refuses
  (`GMI_GAME_VEHICLE_UNLINK_BLOCKED`), clears `eFlags` 0x700000, sets
  `viewlocked_entNum` 1023 and `vehPos` 0 (leaving `vehType`), clears
  `fWeaponPosFrac` and sends `v cl_run 1` and `v cl_stance 0` again; it
  does not call `SetClientViewAngle` (VERIFIED decompile). In the capture
  the exits drop the body beside the vehicle and the next snapshot reads
  `pm_type` 0 (VERIFIED, capture).
- **Changing seats.** The cgame's `weapnext` (0x300474c0) and `weapprev`
  (0x30047550) send `nextvehslot` and `prevvehslot` instead of cycling
  weapons while the predicted `eFlags` hold 0x100000 (VERIFIED,
  disassembly). The server runs `Cmd_NextVehSlot_f` (0x4ed9b) and
  `Cmd_PrevVehSlot_f` (0x4ede1) for a seated, living player, and
  `G_VEH_CycleSlot` (0x8da1e) moves it to the next free seat, refusing
  inside 250 ms of the last change with `GMI_GAME_VEHICLE_DELAY_IGNORE` and
  stamping the change 1500 ms ahead, so changes are 1750 ms apart
  (VERIFIED decompile). The new seat's entry runs `SetClientViewAngle`
  again: the capture's two cycles each move `delta_angles` (VERIFIED,
  capture). A Panzer IV (`numPassengers 0`) cycles 1 → 2 → 1, a Willys
  1 → 2 → 3 (VERIFIED, capture).
- **The hatch is disabled.** `G_VehiclePopOut` (0x8bbb0) opens with
  `Com_Error(1, "G_VehiclePopOut: Popout disabled\n")` (string at
  0xa74c0, the call at 0x8bbe6, VERIFIED disassembly) ahead of the code
  that would toggle 0x400000 and move the rider to `tag_popout`. Nothing
  calls it and no data word holds its address; the only word equal to
  0x8bbb0 in the file is its own `.dynsym` entry (VERIFIED, `objdump` and
  a byte search). So retail UO 1.51 never sets 0x400000, and the cgame's
  tests of it (`CL_KeyMove`'s `upmove`, the seat crosshair 0x30019cf0, the
  seat HUD owner draws from 0x30031cb0) never see it set (INFERRED).

## 2. The cmd a seat sends, and what the server reads

The client sends the same usercmd it sends on foot; nothing in
`CL_CreateCmd` (CoDUOMP.exe 0x40c900) or `CL_MouseMove` (0x40c330) tests a
seat (VERIFIED disassembly). One thing changes:

- **upmove.** `CL_KeyMove` (0x40bf40) adds the kbutton at 0x8cdf18 to
  `upmove` while `pm_type` is 2, 3 or 4, or while `eFlags` hold 0x100000
  without 0x400000 (VERIFIED disassembly, 0x40c056..0x40c099). Both
  `+moveup` (0x40b740) and `+gostand` (0x40bdf0) press that kbutton
  (VERIFIED disassembly), so the stock SPACE (`+gostand`) puts 127 in a
  seated cmd's `upmove`.

The server reads each seat's stored cmd from its vehicle think, which
`G_VehicleClientThink` (0x8b0ce) runs for every driven vehicle from
`G_RunFrame`, 50 ms apart (VERIFIED decompile):

- **Driver, moves** (0x87cd3): `forwardmove`, `rightmove` and `upmove`
  out of the client's last cmd, skipped while popped out or inside 150 ms
  of a collision (VERIFIED decompile). Only the signs drive: forward
  accelerates toward 0.8 `maxSpeed`, back toward half that, `rightmove`
  steers, and a tank with `upmove > 0` turns its hull toward the view's
  yaw at up to `rotRate` (VERIFIED decompile). A tank reversing has its
  steer negated, and one with no turret span (SU-152, Elefant) treats a
  held sight as `upmove` 127 (VERIFIED decompile). The think clears
  `eFlags` 0x200000 on a tank or under `upmove > 0`, sets it otherwise
  (VERIFIED decompile).
- **Driver, turret** (0x85b32): the view's pitch and yaw taken into the
  hull's frame, stepped toward at `turretRotRate * 0.05` a think and held
  inside the file's spans; out on `angles2[0]` and `angles2[1]` (VERIFIED
  decompile). Stock rates: Panzer IV and T-34 30, Sherman 32, Flak 88 20
  (VERIFIED, the files). The view is never clamped to the turret: the
  spans bind the gun.
- **Driver, fire** (0x86c57): `buttons` 0x1 held fires the main gun
  (`turret_fire`), 0x20 the alternate gun (`turret_alt_fire`), each on its
  own timer; the shot aims along the view from `tag_chasecam`, or
  `tag_aimdownbarrel` while sighted (VERIFIED decompile). A vehicle with
  no turret sounds its horn on 0x1 (INFERRED).
- **Gunner** (0x86285, 0x871a6): the view's pitch and yaw in the hull's
  frame at `turretGunnerRotRate` (stock 360) inside the gunner spans,
  pitch on `angles2[2]`, yaw on `origin2[1]` relative to the turret; 0x1
  fires (`turret_gunner_fire`) (VERIFIED decompile).
- **Passengers** give the vehicle nothing; the four-wheel's seat 3 fires
  its own weapon through `PM_Weapon` (section 3).

The capture shows it (VERIFIED, `the_tank_capture_drives_and_turns_on_the_seat_cmds`):
`forwardmove` 127 carried the Panzer IV over 200 units with its yaw
untouched, and `upmove` 127 turned the hull more than 100°, to within 3°
of a view sweeping from -90° to -30°.

## 3. Pmove in a seat

The client predicts a seated player, and the prediction moves nothing.

- `CG_PredictPlayerState` (cgame 0x30035800) runs `Pmove` for every cmd in
  a seat as on foot; it bails only on demo playback, `pm_flags` 0x40000,
  `cg_nopredict` and `g_synchronousClients` (VERIFIED decompile).
- Before its cmd loop, in a seat, it fills `pm` +0x3c..0x44 with the
  vehicle centity's lerped angles and `pm` +0x48/+0x4c with 60 and 70 for
  the four-wheel's seat 3, 0 elsewhere (VERIFIED decompile). The server's
  `ClientThink_real` (0x40c91) fills the same fields from the hull's
  angles (VERIFIED decompile).
- `PmoveSingle` (0x2d530) runs a no-move arm for `eFlags & 0x106000`
  (VERIFIED decompile). The arm clears `pm_flags` 0x10, sets
  `groundEntityNum` 1023, zeroes the ground plane and the walking flag,
  and runs the sight flag, the walking flag, the sprint flag, the fatigue
  and two unnamed steps (VERIFIED decompile). It ends on `PM_Weapon` where
  `BG_AllowPlayerWeaponAtVehiclePos` (0x23148: `vehType` 1, `vehPos` 3)
  allows, and on `PM_UpdateAimDownSightLerp` elsewhere (VERIFIED decompile).
  `G_RunClient` sets `pm_type` 1 for a link (section 1), which takes the
  same arm (INFERRED, the jump table at 0x9b9a8).
- **The body.** `ClientEndFrame` (0x42f59) calls
  `G_PlayerVehiclePositionAndBlend` (0x9048f), which puts `ps.origin` at
  the seat tag plus `BG_GetVehiclePosOffset` (VERIFIED decompile). Nothing
  writes the velocity (INFERRED, absence).
- **The sight.** `PM_UpdateAimDownSightFlag` (0x32c0b) sets `pm_flags`
  0x20 off the button alone in a seat (VERIFIED decompile).
  `PM_UpdateAimDownSightLerp` (0x3349e) wants the sight whenever the flag
  is set in a seat, snaps `fWeaponPosFrac` to 0 or 1 in seat 1 and ramps it
  at the held weapon's rates elsewhere (VERIFIED decompile). The capture's
  driver reads 0 then 1 with no step between (VERIFIED, capture).
- **The view.** `PM_UpdateViewAngles` (0x2b15c) holds the pitch at 16000
  shorts on foot and 12000 in a seat by rewriting `delta_angles[0]` to
  `limit - cmd.angles[0]` (VERIFIED decompile); the capture's view stops
  at 65.92° both ways (VERIFIED, capture). Then, in a seat, each axis
  whose `pm` +0x48 arc is at least 1.0 is held within the arc of the
  vehicle's angle: the excess past the arc is pushed onto `delta_angles`
  as `excess * 182.0444` and the view set on the arc's edge (VERIFIED
  decompile; 1.0 double at 0x9b930, 182.0444 at 0x9b938, 0.0 at 0x9b93c,
  VERIFIED bytes). The push truncates, as vcod's other pushes do
  (INFERRED from the replay matching).
- **The cone.** `PM_AdjustAimSpreadScale` (0x35dcb) counts a body off the
  ground as airborne only when `pm_type` is not 1 (VERIFIED decompile), so
  a seat opens no cone for it.

## 4. The vehicle's own motion

The client simulates no vehicle: neither `CoDUOMP.exe` nor the cgame
carries vehicle physics (`coduo-diff.md` 9, VERIFIED absence). The seated
body's origin is the snapshot's, and the drawn vehicle is the interpolated
snapshot entity.

## 5. The view

`CG_CalcViewValues` (0x30041a30) runs 0x30040580, then, unless rendering
third person, `CG_VehicleView` (0x30040810) for a seat with a valid
`viewlocked_entNum`, `CG_OffsetFirstPersonView` (0x3003fb60) otherwise
(VERIFIED decompile). So a seat takes none of the kick, bob, landing dip
or lean (INFERRED). `cg_thirdPerson` plays no part: the cgame only
registers it and resets it to 0 (VERIFIED decompile).

`CG_VehicleView` poses the vehicle first (0x30021d30), so every tag comes
off the drawn, interpolated vehicle (VERIFIED decompile). Its branches
(VERIFIED decompile; constants VERIFIED bytes at the cited addresses):

- **Artillery** (class 5) with no sight: the eye is `tag_chasecam`.
- **Seat 2, the gunner**, on any vehicle: the gun frame G is
  `tag_secondary_gun`; the eye is `tag_secondary_view`, else G's origin
  -20 forward and 6.5 up (0x3007c2b4, 0x3007c2b0). The refdef takes G's
  pitch and roll and keeps the view's yaw (VERIFIED, disassembly
  0x30040c3d). When the view's pitch is more than 0.4° (double at
  0x3007c2a8) off G's, trap 0xf7 sets the client's view to G's pitch and
  yaw less `delta_angles` (VERIFIED decompile), which the engine writes
  into `cl.viewangles` (CoDUOMP.exe, VERIFIED decompile), so the gunner's
  aim sits on the gun the server turns at 360°/s. A sway and a firing
  shake ride the eye (below).
- **First person**, for the four-wheel's seat 3, a four-wheel with no
  sight, or anything else sighted: on a four-wheel the seat tag
  (`BG_GetVehiclePosTag`, 0x30008190) -12 forward and +32 up (0x3007bdc4,
  0x3007bdd0); elsewhere, or with no such tag, `tag_aimdownbarrel`, else
  `tag_barrel` +30 forward, -3 left, +10 up (0x3007becc, 0x3007be5c,
  0x3007bda4). Angles are the view's.
- **The chase camera** otherwise (a tank's driver at the hip, a
  four-wheel's driver and passengers sighted): pivot T is `tag_turret`
  for seat 1 and `tag_passenger`..`tag_passenger4` for seats 3..6
  (0x300407c0). The offset is `tag_chasecam` less T (times 2.5 on a
  four-wheel, 0x3007be68), else T's forward -200 and up 32 (0x3007c2a4);
  it is taken into T's frame and put back on the axis of the view's
  angles at roll 0, so the camera orbits T as the view turns. A tank's
  vertical part is scaled by 0.75 (0x3007be38); a sighted four-wheel's
  camera drops 135 along the view's up (0x3007c2a0). The distance from T
  is held between half and twice the offset's length.
- **The gunner's sway.** The arm copies `cg.predictedPlayerState`, puts
  the weapon `thompson_MP` (string 0x3007a718, looked up by name through
  0x300110f0) in the copy and runs 0x30015ca0 on it with scale 2.0 and
  `cg.frametime` (pushes at 0x30040964..0x3004096d, VERIFIED disassembly).
  0x30015ca0 is the cgame's turn sway, the function vcod ports as
  `BG_CalculateWeaponPosition_Sway` (`cod11-combat.md` 15): each view axis'
  turn since the last call, clamped to `swayMaxAngle`, times the
  `sway*Scale`s and the scale, eased at `swayLerpSpeed` by `frametime *
  0.001` (VERIFIED decompile; 0.001 at 0x3007bd94, VERIFIED bytes). Its
  state sits at 0x300dc9dc..0x300dca00 (VERIFIED disassembly). Nothing
  else writes it, so no seat change resets it (INFERRED). The eye moves by the offset in the
  gun's frame: `fwd * o[0] + left * o[1] - up * o[2]`, where only `o[1]`
  and `o[2]` are ever written (VERIFIED decompile).
- **The gunner's firing shake** (0x30040ae3..0x30040d26, VERIFIED
  disassembly) runs while `cg.snap`'s playerstate `eFlags` hold
  `EF_FIRING` (UO's 0x200) and the vehicle centity's es +0xe4
  (`animMovetype`) reads 0. Its phase is `cg.time % 100` folded at 50,
  times 0.02 (0x3007c2b8): the eye backs along the gun by `phase * 0.5`
  (-0.5 at 0x3007bf50), and after the snap the refdef pitch lifts by
  `phase * 0.5` (0x3007bce8) and the yaw gains `0.25 * sin(2.334 * 2π t)
  * cos(3.198 * 2π t)` of `t = cg.time / 1000` (0x3004d200 with 2.334 and
  3.198 pushed as floats, 2π at 0x3007bd00, 1000 at 0x3007bd08, 0.25 at
  0x3007be58; the `fsin` and `fcos` CRT stubs at 0x3006ba30 and
  0x3006b980; VERIFIED bytes and disassembly). The vehicle's +0xe4 is
  what the gunner's think (game 0x871a6) writes 1 at an overheat and 0
  once the gun cools (VERIFIED decompile), over the seat bits 2 and 6 the
  captures show while crewed (VERIFIED, `snapshots-uo-overheat.bin`:
  6, then 1 at the overheat, then 0 with both seats still taken). So a
  crewed vehicle's gunner shakes only once its gun has overheated and
  cooled (INFERRED from the two).
- **Clip.** Every branch ends on a ±8 box trace (mask 0x211, the vehicle
  skipped) from `tag_body`, else the vehicle's origin, 20 up (0x3007be04)
  to the eye, retraced under 0x11 when it starts in a 0x200 brush; a hit
  stops the eye there, `(1 - fraction) * 32` higher, and a second trace
  under 0x11 brings it back (VERIFIED, disassembly 0x30040d30..0x3004152e).

The stock MP Sherman, T-34, Panzer IV, Flak 88 and Willys models carry
`tag_chasecam` and `tag_aimdownbarrel`, all but the Flak 88
`tag_secondary_view` (VERIFIED, the xmodelparts strings).

**Sight toggle.** MOUSE2 is `toggle cl_run` in UO's `default_mp.cfg`
(VERIFIED, the pak). With the mount's `cl_run` 1, the right button
therefore takes a tank driver from the chase camera to the gunsight and
a jeep driver from first person to the chase camera (INFERRED from the
branches).

**The four-wheel's riders** (0x30040580): on a four-wheel, for every seat
but 2 and the passenger with its own weapon, with a weapon in hand, the
cgame keeps last frame's vehicle axis and turns the view by the vehicle's
turn since: the new axis is the view's times last frame's vehicle axis
transposed times this frame's. It writes the pitch while `fWeaponPosFrac`
is 0 and the yaw while `eFlags` 0x200000 is set, and sets the
client's view through trap 0xf7 (VERIFIED disassembly). That the product
turns the view by the vehicle's turn is a reading of the matrices
(INFERRED). A jeep driver's view therefore swings with the
jeep unless `upmove` is held, which steers toward the view instead
(section 2).

**Which angles.** The four-wheel turn builds the vehicle's axis from the
centity's `lerpAngles` (+0x214, `AnglesToAxis` at 0x3004060f, VERIFIED
disassembly) after a `tag_player` lookup that only gates it (string
0x30072d68 loaded at 0x300405f1, VERIFIED). The prediction's
`pm` +0x3c..0x44 come from the same field (section 3). The frame runs
`CG_PredictPlayerState` (inside 0x30036070) before `CG_CalcViewValues`
(0x30041a30) and the packet entities (0x3001f810) (VERIFIED decompile of
the frame at 0x30036780..0x30036790), and `lerpAngles` is written by
`CG_CalcEntityLerpPositions` (0x30021d30, VERIFIED decompile), which
`CG_VehicleView`, 0x30040580 and the packet entities call. The
prediction therefore reads the angles lerped the frame before (INFERRED
from the order).

**Field of view** (0x3003ffc0, VERIFIED decompile): in a seat but the
four-wheel's seat 3, the driver with a sighted weapon closes from
`cg_fov` toward 70 (0x3007bf84) by `fWeaponPosFrac`; every other seat
keeps `cg_fov`.

## 6. View weapon and HUD

- `CG_AddViewWeapon` (0x30046570) draws no gun while `eFlags & 0x106000`
  is set, except in the four-wheel's seat 3 (VERIFIED decompile).
- The seat crosshair (0x30019cf0) and the vehicle status owner draws are
  `coduo-diff.md` 19's, and vcod draws them already.
- **The own body.** `CG_AddPacketEntities` adds `cg.predictedPlayerEntity`
  when the playerstate's `pm_flags` hold UO's 0xc0000 (0x3001f810,
  VERIFIED decompile), built as `BG_PlayerStateToEntityState` (the game
  module's, by name) builds one: the seat in `surfType` bits 0-2, `vehType`
  in 3-5, `vehMotion` in 6-7, `viewlocked_entNum` in `otherEntityNum`
  while `eFlags & 0x106000` (VERIFIED decompile). `CG_Player`
  (0x300343e0) sends a seated body (`eFlags` 0x100000) through 0x30032fe0
  and draws nothing when that returns 0, and draws a seated body with
  `renderfx` 0 where a first-person own body would get 2 (VERIFIED
  decompile). 0x30032fe0 (from 0x3003392b, VERIFIED decompile) returns 0
  for a tank's seat 1, and for the body whose client is `cg.snap`'s
  outside third person: a four-wheel's seats 2 and 3, any other
  four-wheel seat while the predicted `fWeaponPosFrac` is 0, and a tank's
  seat 2. So the jeep driver and its passengers 4..6 see their own body
  with the sight up, which is when the view leaves the seat for the chase
  camera, and an artillery crew always (INFERRED from the branches).

## 7. Prediction against the capture

`moves_ab`'s `the_predictor_replays_uo_seats` rebuilds every snapshot of
section 8's two captures through `predict::from_wire` with its vehicle's
angles and predicts it to the next under UO's rules. Every field but
these matches retail (VERIFIED, replay):

- `delta_angles` at each `nextvehslot`: the new seat's
  `SetClientViewAngle`, the server's.
- `origin` and `velocity` on the exit's snapshot: the dismount.
- `aimSpreadScale` on the first pair, which has no cmd at its
  `commandTime`, and on the tank two more: 33.8 against ours 32.8 inside
  the pitch clamp and 88.6 against 78.4 a second before the exit
  (unexplained).
- On the tank, `aimSpreadScale` and `eventSequence` on the snapshot the
  main gun fired on: the driver took damage feedback. That snapshot
  carries `damageEvent` 1, `damageCount` 44, `damageYaw` 105 and
  `damagePitch` 10, `aimSpreadScale` 44.0 and one event, v1 187 (UO's
  191, `EV_PAIN`) with parm 100 (VERIFIED, capture). `P_DamageFeedback`
  (game 0x3f65a) adds the damage count to `aimSpreadScale` and raises
  `EV_PAIN` (0xbf) with `100 * health / maxHealth` (VERIFIED decompile);
  the parm 100 is a driver whose health did not move, as a tank driver
  takes no damage (`coduo-diff.md` 9). That the hit is the tank's own
  shell is INFERRED (the snapshot is the shot's). Neither is pmove's.
- On the tank, `events` on the landing after the exit: retail's
  `EV_LANDING` read surface 17 (rock), ours 0 (VERIFIED, replay). Two
  brushes meet under the spot, (4369.4, -2165.1, -7.875) on mp_cassino:
  a `textures/common/caulk` brush whose top side is
  `textures/italy/floors/rock@cobblestone-01` (0x1100000, material 17)
  and a `textures/common/clip_nosight_rock` one (0x11040a0, material 17)
  (VERIFIED, lumps 0, 3 and 4). Ours stopped on the caulk brush and
  reported the brush's material; a trace now reports the struck side's
  (`bsp-ibsp59-format.md`, lump 3), which reads 17 on either brush, and
  the replay's landing matches (VERIFIED, `moves_ab`).
- `weaponTime` and `weaponDelay` on the jeep passenger's rechamber:
  retail's begin one 8 ms cmd later. Its `weaponTime` counts down with
  every cmd's msec through the shot (289, 247, 189, 131, 89, 39 at the
  captured `commandTime`s, VERIFIED, capture), so it reaches -2 on the cmd
  ending 46032, where ours rechambers; retail rechambers on the cmd ending
  46041 (VERIFIED, capture). The seat arm of `PmoveSingle` (game
  0x2d530) runs `PM_Weapon` (0x3726c) with no timer step before it, and
  the decrement and rechamber are `PM_Weapon`'s own (0x360fa, VERIFIED
  decompile); none of the arm's other steps (0x27e16, 0x2a829, 0x297e4,
  the sight, walking, sprint and fatigue updates) writes `weaponTime` or
  `weaponDelay` (VERIFIED decompile). Unexplained.

With the seat stripped from the playerstate the same replay misses
`delta_angles` on 44 and 230 snapshots, `fWeaponPosFrac` and the
driver's weapon state (VERIFIED, replay): the gate sees the seat.

## 8. Captures

`crates/server/tests/fixtures/playerstate/mp_cassino-tdm-slope-8ms-tank-seats-uo.txt`
and `mp_peaks-tdm-slope-8ms-jeep-seats-uo.txt`, retail UO 1.51b,
2026-10-14: one probe through every seat of a Panzer IV (mp_cassino) and
a Willys jeep (mp_peaks, `+set probe_side 75 +set probe_pitch 15`).
Recipe: `client-probes/probe_vehicle` installed as the gametype `probe_vehicle`
with `+set probe_vehicletype panzeriv_mp` (or `willyjeep_mp`), then
`vcod --game-dir <UO install> --mod-dir uo --net-probe <addr> --probe-team
axis --probe-vehicle seats --save-slope --probe-cmd-ms 8 --capture-tag
tank-seats-uo --probe-secs 55`. The script (`vehicle_probe.rs` `SEATS`)
enters, pitches past the clamp both ways, sights, drives, turns the hull
with `upmove`, fires, sends `nextvehslot` twice, sweeps the gunner's gun
and the passenger's arcs, fires from the passenger's seat and gets out.
Each `!snap` line carries `vang`, the seat's vehicle's angles as that
snapshot sent them.

## 9. What vcod does

- **Prediction.** `pmove::VehicleSeat` (`PlayerState::seat`) is read off
  a UO playerstate's `eFlags`, `vehPos` and `vehType`, the vehicle's
  angles filled by the caller: the vehicle's angles as drawn the frame
  before (`entities::BuiltScene::vehicle_angles`), the newest snapshot's
  until it has been drawn. The four-wheel turn takes this frame's. `pmove`'s seat arm and
  `cmd::clamp_view` follow section 3; the 16000 pitch clamp now applies on
  foot under every patch's rules.
- **View.** `play::seat::camera` and `clip` follow section 5, off
  `EntityScene::tag_pose` on the drawn vehicle and the world's box trace;
  the gunner's sway (`play::seat::GunnerSway`, `aim::step_turn_sway` off
  configstring 7's `thompson_mp`) and firing shake
  (`play::seat::gunner_shakes`, `shake_phase`) included.
- **The own body.** `play::seat::own_body_drawn` is 0x30032fe0's gate,
  read on the frame before's predicted sight, and `play::seat::own_body`
  builds the entity `BG_PlayerStateToEntityState` would, which the seat
  path of `entities::build_instances` draws.
  The gunner's snap and the four-wheel's turn go through
  `PlayInput::set_view`, the trap 0xf7 write. A followed rider gets the
  seat camera on the snapshot's view, without the writes.
- **Input.** The stock keys already make section 2's bytes
  (`a_seat_drives_with_the_stock_keys`); `weapnext`/`weapprev` send
  `nextvehslot`/`prevvehslot` in a seat; `v cl_run` sets the cvar and `v
  cl_stance` the stance.
- **View weapon and fov** as section 5 and 6.
- **Not done.** The roll 0x30040580 writes; the jeep's steering anim
  blend; the push-off 0x30035800 gives a body that walks into a vehicle.
  The hatch needs nothing: retail disabled it (section 1). Vehicle
  sounds and the vehicle MG's client-traced impacts are `coduo-diff.md`
  19's.

## 10. The server: definition files and spawn

Sections 10 to 14 are the server's half: what `vcod-server` reproduces of
`game.mp.uo.i386.so` (addresses `nm`, as above; `coduo_lnxded` addresses
are the engine's, non-PIC). Gentity offsets are UO's, 4 above 1.1's
(UO's `entityState_t` grew `xmodel`): `r.svFlags` +0xf8, `r.contents`
+0x120, `r.ownerNum` +0x154, the vehicle record pointer +0x164,
`health` +0x240 (VERIFIED, the netfield table at `coduo_lnxded`
0x080e11a0 and `dump_script_fields.py`).

### 10.1 The files

- `G_ParseScrVehicleInfo` (0x8a51f) runs at the top of `G_InitGame`
  (0x586cf), after `BG_SetupWeaponInfo` and before every other
  configstring registration of the level (VERIFIED decompile).
- It lists `vehicles/` through `trap_FS_GetFileList`, at most 64 files
  (`Com_Error` past it), each `VEHICLEFILE` plus an info string under
  8 KB parsed over the 58-key table at 0xae9c0 (VERIFIED decompile and
  table dump).
- The list is the search path's order: newest pak first, each pak's own
  entry order, a name already listed (case-folded) skipped, no sort
  (INFERRED: a simulation of that order reproduces the gamestate's alias
  block 662..727 exactly). On a UO-over-1.5 install that is 39 files:
  `pakuo07`'s nine `_mp` files, then `pakuo03`'s fourteen others, then
  `main/pak0`'s sixteen (VERIFIED, `Pk3Fs::list_dir_in_search_order`
  against the paks; `coduo-diff.md` 9's 23 counts the UO paks alone).
- `maxSpeed`, `accel`, `collisionSpeed` and `engineSndSpeed` are scaled by
  17.6 (f32 at 0xa72b4) and the box's half extents by 0.5 (0xa72b8)
  after the parse (VERIFIED bytes).
- Per file in list order, the seven alias keys (`lowIdleSnd` ..
  `impactSnd`, `info+0x174 + 0x40 i`) then `vehicle_hit_person` go
  through `G_SoundAliasIndex`, so the vehicles hold aliases 1..66 (CS
  662..727) on every map (VERIFIED decompile; VERIFIED capture,
  `configstrings_uo_ab`).

### 10.2 Spawning one

In BSP order, as any entity (`G_CallSpawn` 0x719a6, VERIFIED decompile):

1. `G_Spawn` takes the number; the field parse indexes the `model` key
   (CS 405 + i) (VERIFIED decompile).
2. `SP_script_vehicle` (0x8dec6): `delay` defaults to
   `g_vehicleBurnTime` ("10"), the spawn vars are backed up for
   `spawnDuplicate` (VERIFIED decompile).
3. `G_SpawnVehicle` (0x8abfd): the first free slot of 64 (0x3ec bytes
   each), whose index goes in `s.attackerEntityNum`; the file found by
   `strcasecmp` on `vehicletype`, else `Com_Error` naming the targetname
   (VERIFIED decompile).
4. `fn_7c935`: `eType` 12, `r.svFlags` 4, `r.contents` 0x2080 (|
   0x200000 under spawnflag 1), both trTypes 1, `s.weapon` the turret
   weapon, `surfType` the file's type, `origin2[0]` and `[2]` the ints
   127, think `Scr_Vehicle_Init` 50 ms on, `takedamage` 1, clipmask
   0x201; the DObj's bounds; `RegisterItem` on the turret, alt and
   gunner weapons, which registers their models only (VERIFIED
   decompile).
5. `fn_7cebf`: `otherEntityNum` 1023, the slot's record, the
   `useHintString` through `G_GetHintStringIndex` (CS 1365 + i, the first
   equal or empty slot); then up to four sound-blend entities: B1 when
   both idle aliases are set, B2 when both engine aliases are, B3 and B4
   always (their test is a null check that never fires, 0x7d1e4); each
   `G_SpawnSoundBlend` (0x5a65d): `eType` 9, contents 0, `svFlags |= 8`,
   `otherEntityNum` the vehicle, no classname (VERIFIED disassembly).
   Then the alt and gunner weapons' `loopFireSound` and `stopFireSound`
   aliases (VERIFIED decompile).

So mp_cassino's eight tanks take 156, 161, .., 191, each followed by its
four blends, in pool slots 0..7; dm keeps 176 (slot 4) and 181 (slot 5),
the trigger_hurt lands on 435 and the collmaps on 436, 437 (VERIFIED,
the gamestate's baselines against a simulation of the order). No effect
and no other model is registered on the way: the wrecks and effects come
from `_tankeffects_gmi` (VERIFIED decompile).

### 10.3 Collmaps

- `SP_script_vehicle_collmap` (0x8df52) sets `eType` 14 and contents 0
  and never links (VERIFIED decompile).
- After the BSP pass, `G_SetupScrVehicles` (0x8ab36) pairs each vehicle in
  slot order: `VEH_SetupCollmap` (0x7d3e2) takes the first in-use collmap
  by number whose `targetname` is the vehicle's model name, case folded,
  puts its `*N` in the **vehicle's** `s.index`, runs `SetBrushModel` on
  the vehicle and sets the vehicle's contents to 0x800000 | 0x200000
  (VERIFIED decompile; `coduo-diff.md` 9 had it on the collmap). One
  collmap serves every vehicle of its model (INFERRED, the scan). A
  vehicle placed after load pairs at once (VERIFIED decompile).
- The capture's tanks carry `index` 27 and 28 and `solid` 0xffffff
  (VERIFIED, baselines).
- mp_cassino's two collmaps' brushes carry contents 0x28031640: player
  clip, the vehicle clip 0x200, no SOLID and no shot bit (VERIFIED, the
  BSP).
- So a player bumps a tank and a bullet passes its collmap; retail tests a
  brush model's own contents against the mask inside the clip (INFERRED).
  A vehicle's clipmask 0x201 meets the collmap brushes through 0x200, a
  player's 0x2810011 through 0x10000 (VERIFIED, the masks and the brush
  word).
- Each vehicle clips its own copy of the brushes where it stands, under
  its own `r.contents`: mp_kharkov's Horch glances off a parked Panzer IV
  that is not the last of its model to have moved (VERIFIED, the ram
  capture, section 12). vcod keeps a per-entity model instance in its
  collision world for each vehicle (`CollisionWorld::set_model_instance`),
  names the vehicle on its hits and passes it on the vehicle's own traces;
  until then one posed brush model served every vehicle of a model, so
  only one tank of each kind blocked anything.

### 10.4 The first think and the park

`Scr_Vehicle_Init` (0x8c5ab), 50 ms after the spawn, settles a tank or a
four-wheel on its wheel traces, so the BSP's z -8 parks at -7.875
(VERIFIED, baselines) and `time2` becomes `trunc(1000 *
suspensionTravel)`, 6 bits on the wire (24 for the Panzer IV, 8 for the
Sherman) (VERIFIED decompile; capture). The vehicle table's angles are 16
bits, so the BSP's yaw 270 reads -90 (VERIFIED, baselines).

### 10.5 Freeing

`G_FreeEntity` runs `G_FreeVehicle` (0x8ad7d) first: B1..B4 join the free
list, then the vehicle (VERIFIED decompile). `freeVehicle` turns a vehicle
into `eType` 13 `script_vehicle_corpse`; no stock MP script calls it
(VERIFIED grep).

## 11. Seats on the server

### 11.1 Boarding

- The use press (`ClientThink_real` 0x40c91, inside the cmd) reaches
  `Cmd_Activate_f` (0x4dffb). A rider in a usable vehicle gets the
  vehicle's `use`, the exit; anyone else takes the cursor hint's entity
  and, for a vehicle `G_IsVehicleUsable` (0x8b673) passes, its `use`, the
  entry (VERIFIED decompile).
- `G_IsVehicleUsable`: not seated, a free seat, the vehicle at most 100
  units/s and alive, contents 0x200000, no grenade in hand, every rider
  on the player's team (VERIFIED decompile).
- `VEH_LinkPlayer` (0x8998d), in order: the owner lock (`vehicle_owner`
  a client, seat 1 free, not the player: `e
  "GMI_GAME_VEHICLE_NOT_OWNER"`); silent unless on the ground or falling
  slower than 80, and 250 ms past the last seat stamp; the stamp
  (`client+0x468c`: now on a tank's first entry, now + 1500 else); the
  seat; `SetClientOrigin` at the seat tag (+1 z, the teleport bit);
  `SetClientViewAngle` at the view tag's forward (`tag_player`, seat 2
  `tag_secondary_gun`, else `tag_body`), roll 0; the link; the record's
  occupant and `animMovetype |= 1 << seat`; on seat 1 the vehicle driven,
  its `eFlags` 0x100000, `r.ownerNum` and `otherEntityNum` the driver;
  `ps.eFlags |= 0x300000`, `viewlocked_entNum`, the sight flag off;
  reliable `v cl_run 1` and `v cl_stance 0`; `vehPos`, `vehType`;
  `activated (vehpos, player)` on the vehicle and `vehicle_activated
  (vehpos, vehicle)` on the player; `fTorsoPitch` and `fWaistPitch` the
  gunner's vertical spans (VERIFIED decompile).

### 11.2 Getting out

- `VEH_UnlinkPlayer` (0x89fb2): a voluntary exit (the use key) is
  refused on a live tank inside 1500 ms of the stamp while the seat has
  changed at most once (`e "GMI_GAME_VEHICLE_EXIT_DELAY"`), or with no
  dismount spot (`e "GMI_GAME_VEHICLE_UNLINK_BLOCKED"`). Then the seat
  and its mask bit are cleared, on seat 1 the vehicle's driver fields,
  and a tank coasts 3 s on its last forward input; the player's seat
  bits, `viewlocked_entNum` 1023, `vehPos` 0 (`vehType` and `vehMotion`
  kept), the sight; `v cl_run 1`, `v cl_stance 0`; a voluntary exit
  lands on the spot (+1 z) and inside the coast window pops up at 200;
  `deactivated (player)`, `vehicle_deactivated (vehicle)` (VERIFIED
  decompile; the exits of both seat captures, VERIFIED).
- `VEH_FindValidDismountSpot` (0x88dee): a point off the side away from
  the vehicle's motion, or toward the player at rest, sized `60 + maxsX +
  maxs.y` sideways or `maxs.x + max(collisionFront, collisionBack) + 100`
  along, 4 up, along `AngleVectors` of the record's whole angles
  (`rec+0xd0`: on a slope the spot tilts with the hull); a 20-unit grid
  of offsets -80..60 about it (0xa6e08 80, 0xa6e1c 20), each point at two
  heights 32 apart (the inner loop's bound 2, 0xa6e18), four passes
  flipping forward then side. The grid's first point is the base itself:
  each offset is applied after its point's two heights (VERIFIED
  decompile). A spot needs a capsule position test under 0x2000000 (no
  body) to pass, a capsule from it 256 down under 0x810011 to hit without
  starting solid, and a box (the player's, `maxs.z - 30`) from the
  vehicle's origin 30 up to the spot 30 up, passing the player's
  `r.ownerNum` (the vehicle) under 0x810011, to arrive or stop on the
  vehicle (VERIFIED decompile). vcod took the yaw alone and three heights,
  which on mp_peaks' slope put the jeep's exit 10 above retail's; it now
  lands on retail's spot (VERIFIED, `vehicle_ab`). The six vectors at 0xa6660 are the collision
  probes' (section 12), not the dismount's (VERIFIED, every reference;
  `coduo-diff.md` 9 had them here).
- Forced exits (no refusal, no search, the body left at the seat): a seat
  change, the vehicle think's pass over dead riders, `ClientSpawn`,
  `ClientDisconnect`, a team change (VERIFIED decompile).
- `Scr_Vehicle_Die` (0x8dccb) probes each rider with a 5-point
  `MOD_EXPLOSIVE` hit (health padded by 10) and dismounts one it did not
  hurt, the rest in place (VERIFIED decompile).

### 11.3 Changing seats

`Cmd_NextVehSlot_f` (0x4ed9b) and `Cmd_PrevVehSlot_f` (0x4ede1), for a
living rider, run `G_VEH_CycleSlot` (0x8da1e): none when no seat is
free; a tank's first change after a use entry is never refused; inside
250 ms of the stamp `e "GMI_GAME_VEHICLE_DELAY_IGNORE"` at most every
500 ms; else a forced exit and an entry at the next free seat (wrapping
6 to 1), stamped 1500 ms ahead, so changes are 1750 ms apart and the
script sees `deactivated` then `activated` (VERIFIED decompile).

### 11.4 Every frame

- `G_RunClient` (0x41dfc) puts a linked player in `pm_type` 1, 7 dead
  (VERIFIED decompile).
- `ClientEndFrame` (0x42f59): `vehMotion` 1 under 20 units/s of the
  hull's forward speed, else 2 forward, 3 back; the entity's `surfType`
  is `vehPos | vehType << 3 | vehMotion << 6` and its `otherEntityNum`
  the vehicle (VERIFIED decompile); then `G_PlayerVehiclePositionAndBlend`
  (0x9048f) puts `ps.origin` at the seat tag plus `BG_GetVehiclePosOffset`
  in the tag's axes (VERIFIED decompile).
- The tag comes off the vehicle's DObj as the frame's first tag query
  posed it, with the controls from before the frame's step: a rider placed
  with this frame's controls misses retail's origin by up to 3 units in a
  turn, with the last frame's by none (VERIFIED, replay, section 14).
- The engine never sends a tank's driver to anyone (`coduo_lnxded`
  0x8096b97, test at 0x8096d7a: a player in a vehicle whose `surfType`
  is 2 and whose own `surfType & 7` is 1) (VERIFIED decompile).
- A rider's legs come off the animscript: `BG_AnimUpdatePlayerStateConditions`
  (0x1c973) sets `mounted` to the seat (`vehDriver`, `vehGunner`,
  `vehPassenger1`..), `vehicle` to the kind (`tank`, `jeep`, `flak88`) and
  `vehicle_motion` (`idle`, `forward`, `reversing`) (VERIFIED decompile),
  and `playeranim.script`'s clauses pick them; the selection runs for a
  rider though it is off the ground (INFERRED; the replay's legs match).

## 12. The drive step

- `Scr_Vehicle_Think` (0x8c7cc) is every vehicle's think, a fixed 50 ms
  step. A driven one (`ent+0x17e` 2) steps once a server frame from
  `G_VehicleClientThink` (0x8b0ce), after every client's cmds and before
  every `ClientEndFrame`; any other on its own think (VERIFIED
  decompile).
- The input is the driver's `pers.cmd`, the last cmd that ran: a cmd
  repeating the last one's `serverTime` never runs and never drives
  (VERIFIED, the tank drive capture at 29750).
- The motion (0x87cd3 and its callees, the slide 0x8036e, the suspension
  0x81331, the collision probes 0x827ab), the turret and gunner aim
  (0x85b32, 0x86285), the sway pack (0x86735), the steer and the track
  clock (0x86a29, 0x86aa1) are `crates/server/src/game/vehicle/physics.rs`,
  each formula with its constants there. Two facts the replay forced: the
  yaw passes `AngleNormalize180`'s 16-bit truncation every step (VERIFIED
  disassembly 0x3dc9e), and terrain meets the hull's half box as a
  capsule of its half width (VERIFIED, replay: the min-radius capsule
  lands the Panzer 0.63 low on the first slope).
- The probes and the slide meet vehicle clip brushes (contents 0x200),
  which only the vehicle clipmask 0x201 holds: where retail's tank
  bounced at mp_cassino's slope the brush is one (VERIFIED, replay; vcod's
  collision world now keeps such brushes, unmet by every other mask).
- **Another vehicle.** The probes' mask holds 0x800000, so another
  vehicle's collmap stops them; when what they struck is a
  `script_vehicle` (scr_const 0x82) 0x827ab pushes it (VERIFIED
  decompile, constants at 0xa6b78..0xa6c2c):
  - unless its sway kick still runs (`rec+0x2b0` not 0) it is kicked
    (0x7d492) toward its origin from ours, scale `clamp(2 impact, 0.2,
    0.5)`, decel 175 (0x442f0000);
  - with `k = 100 * its maxSpeed / ours` and `share = clamp(|our velocity|
    / our maxSpeed, 0.5, 1)`, a struck vehicle slower than `k * share`
    gains `k * share` along the line from us to it, capped at its
    `maxSpeed`;
  - moving toward it, and no faster-topped than us, we lose
    `min(|its velocity - ours|, our maxSpeed / 2)` along that line when
    our velocity points at it, and the soft bounce is skipped; moving
    away from it our velocity is zeroed.
  vcod's step hands the kick and the velocity to the parent, which writes
  them on the other record (`physics::VehiclePush`). Measured, retail UO
  1.51b, `client-probes/probe_vehicle_ram` (`--probe-vehicle ram`, fixture
  `mp_kharkov-jeep-ram-uo.txt`): mp_kharkov's second Horch driven 6 s at
  the Panzer IV parked 680 ahead glances off it (yaw 35 to 74) and shoves
  it 2.95 units over three thinks; ours follows retail's path within a
  cmd and shoves it 1.7 (VERIFIED, `vehicle_ram_ab`).
- Sounds, 0x87480 with `G_SetSoundBlend` (0x5a6a9): B1 the idle pair and
  B2 the engine pair crossfaded by the step's blend while a driver runs
  the engine (B2 also quiet with B1 present and the blend 0, or driverless
  under 0.1 units/s, 0.1 double at 0xa6d30); `loopSound` the turret spin
  alias while it turns, its stop alias once as it stops; B3 and B4 each
  MG's loop held 200 ms past a round, its cooldown alias as the hold runs
  out. A blend goes out with `eventParm` and `scale` the aliases and
  `angles2[1]` the crossfade, unlinked when both aliases are 0 (VERIFIED
  decompile; VERIFIED capture: the crew Panzer's 187 on 17/18 and 188 on
  19/20).

## 13. Guns, heat and damage

- The driver's reader (0x86c57), with a turret weapon, `capturing` 0:
  `s.weapon` the turret weapon; main timer `< 1` with `buttons & 1`
  notifies `turret_fire`, else 50 off; the alt timer 50 off every think,
  `< 1` with `buttons & 0x20` notifies `turret_alt_fire`; the aim point
  10240 along the view from `tag_chasecam` (`tag_aimdownbarrel` sighted),
  stopped by a locational trace under 0x2091 (no vehicle, no body) passing
  the driver, stored at `rec+0x28c`; a `trap_Trace` between the flash and
  barrel tags that hits sets `rec+0x270`, the straight shot (VERIFIED
  decompile). The reader runs in the vehicle's think, after the frame's
  threads, so a gsc `setPlayerAngles` on the driver turns the aim, and a
  script's `fireTurret` bends toward the point the last think stored
  (VERIFIED, the shell capture below: the round bent 5 down and 2 across
  toward the Elefant where the cmd's own view pointed behind the tank).
  vcod's think stores the point (`Vehicle::aim_point`); the flash-barrel
  test is not ported. The gunner's
  (0x871a6) notifies `turret_gunner_fire` the same way (VERIFIED
  decompile). The scripts fire on the next frame's threads.
- `fireTurret` (0x7ee02): from `tag_flash`, the barrel's forward bent
  toward the aim point by at most 5° pitch, 2° yaw; a bullet weapon's
  spread `minSpread + rand * maxSpread`; a projectile weapon goes through
  `Weapon_Artillery_Fire` (0x7b594) to `fire_rocket` (0x5fc74), `trType`
  5 (z falls 400 t²), the speed truncated per axis, the driver credited
  through the vehicle, `MOD_PROJECTILE` direct and `_SPLASH` around;
  event 163 on the vehicle, the hull jolted 0.75 back along `tag_barrel`,
  the timer `fireTime` (VERIFIED decompile). `fireAltTurret` (0x7f57e)
  fires from `tag_altfire` (164) and `fireGunner` (0x7fc7a) along
  `tag_secondary_flash` (165); both refuse while overheated, add
  `fireHeat` and hold their loop 200 ms (VERIFIED decompile).
- A vehicle round's event parm is its weapon with bit 7 set unless it
  struck a player: the round raises no impact of its own because its
  inflictor (the vehicle) is not its attacker (VERIFIED decompile, 0x7a920;
  VERIFIED capture: 164 parm 0x9b, 165 parm 0xa0).
- Heat: overheat at 1.0, recovery at 0.5 or under (0xa6d14, 0xa7590),
  checked before the cooling by `cooldownRate` each think; 208 on the
  vehicle (the alt gun's not on a four-wheel); the gunner's overheat
  writes `animMovetype` 1 and its recovery 0, over the seat mask, sets
  `pm_flags` 0x800000 on the gunner and notifies `overheating` every
  think (VERIFIED decompile; VERIFIED capture: 49 rounds, 208, the mask
  6 → 1 → 0, recovery 2550 ms on).
- `G_Damage` (0x4fedb) on a vehicle: `G_IsVehicleImmune` (0x8b893)
  (bullets need `bulletDamage`, grenades `grenadeDamage`, projectiles and
  artillery `projectileDamage`; water, explosive and collision always
  land; every other means never does); a blast under 100 dropped;
  `Scr_Vehicle_DamageScale` (0x8ccad) by the hit's side, 0.7 of a blend
  for splash, `dmgScaleBullet` on bullets; then `damage (amount,
  attacker, dir, point, mod, inflictor)` and at 0 `death` and
  `Scr_Vehicle_Die`; else `Scr_Vehicle_Pain` (0x8d1ca) jolts the hull
  when the attacker holds a grenade or a launcher, the means is
  `MOD_EXPLOSIVE` or the damage tops 200 (VERIFIED decompile). The scale
  times the damage is truncated (`fistp` under a 0xc00 control word,
  0x4ffbf..0x4ffd1) and a result under 1 is lifted to 1 (VERIFIED
  disassembly; the jeep's rear 18.75 reads 18 in the hits capture below).
  The generic tail takes the damage off `health` for every classname but
  `misc_mg42` and `misc_turret` (scr_const 0x4e, 0x50), so a vehicle's
  health falls; the stock scripts set 2900 and refund friendly fire
  (VERIFIED decompile and script).
- A rider takes a tenth of a blast, a fifth in artillery
  (`G_VehicleOccupantRadiusDamageScale` 0x8ba86), and a tank's driver
  nothing whose inflictor is not his own tank
  (`G_IsVehicleOccupantInvulnerable` 0x8b96b) (VERIFIED decompile).
  The rider branch, in order, inside `G_DamageClient` (0x4fd4b), ahead
  of `Scr_PlayerDamage` and so of the damage callback:
  - It runs for a victim whose `ps.eFlags` has 0x100000 (VERIFIED
    decompile).
  - A hit with `DFLAG_RADIUS` is multiplied by the scale and truncated: an
    `fldcw` with 0xc00 ORed in, 0x4fdf3..0x4fe05 (VERIFIED disassembly).
  - The scale is 0.2 (f32 at 0xa74b0) for a vehicle file of type 5 and
    0.1 (0xa74b4) for every other (VERIFIED bytes and decompile).
  - Both helpers give a rider no protection unless `ps.eFlags` has
    0x100000 without 0x400000 and `r.ownerNum` (+0x154) names an entity
    with a vehicle record. `VEH_LinkPlayer` (0x8998d) writes the vehicle's
    number into the rider's `r.ownerNum` (VERIFIED decompile).
  - The invulnerability needs file type 2 (`cmpw $0x2` at 0x8ba54) and
    `vehPos` 1 (`cmpl $1,0x614` at 0x8ba64). So in a tank only the driver
    has it, and only while not popped out, which 1.51 never sets (section
    1) (VERIFIED disassembly).
  - The early return needs an inflictor whose number differs from the
    rider's `r.ownerNum`. A NULL inflictor passes, so a `radiusDamage`
    blast and fall damage reach a tank's driver (VERIFIED decompile).
  - A bullet names its shooter as the inflictor (`Bullet_Fire_Extended`
    0x7a920, unless the shooter rides in a seat other than 3, when it is
    the shooter's vehicle) (VERIFIED decompile).
- Measured, retail UO 1.51b, 2026-10-15, `client-probes/probe_vehicle_armor`
  (fixtures `crates/server/tests/fixtures/vehicles/*-armor-uo.txt`, recipe
  in their headers). The rider sits 12 s in each seat while a sighted
  `thompson_mp` 140 units off the flank holds its trigger on him. The gsc
  aims at the height where a `bulletTrace` (mask 0x2802031 and
  `bulletPriorityMap`, as `Bullet_Fire_Extended`'s, builtin 0x6b145) from
  the shooter's eye meets the rider first. Its damage callback only logs.
  - The Panzer IV's driver: 0 callbacks in two 12 s stints, though the
    trace met his body at 6 to 8 of 16 heights. The gunner: 1 callback in
    one run, 10 in another, with the trace meeting him at 2 or 3 heights.
    On foot before and after: about 10 a second (VERIFIED capture).
  - The Willys: the driver 1, the gunner 4, the passenger 11 (VERIFIED
    capture).
  - Traces toward the seated rider that miss him stop on the tank, entity
    186, at fraction 0.69 to 0.83 (VERIFIED capture).
  - The retail probe client (`--probe-uo-weapon hold`) sends a cmd every
    16 ms holding the view it saw at its placement, rebased on each
    snapshot's `delta_angles`; the gsc's `setPlayerAngles` writes the view
    against the last cmd, and the aim block reads the view ahead of
    `Pmove`, so only a frame's first cmd fires along the gsc's aim and the
    rest along the placement's (VERIFIED, the shooter's snapshots: `delta`
    pitch drifting 557 a frame under a view held at the gsc's). The
    capture's riders took their few rounds from those first cmds; the
    replays send the same stream (`common::ProbeHold`).
- **A hull stops rounds.** `SV_ClipMoveToEntity` (`coduo_lnxded`
  0x809a72a) sends a locational trace on an entity with a DObj and
  `svFlags & 6` to the model arm: the model's collision contents against
  the mask (0x80c7d32), its bounds (0x80c6b36), vmMain 15 posing the
  entity, then 0x80c7be8, which walks every part's collision surfaces
  (0x80c4892), each in its bone's posed frame, a one-sided segment test
  per triangle (VERIFIED decompile). A vehicle has `svFlags` 4 and contents
  0xa00000, which every bullet (0x2802031), missile (0x2802091) and blast
  probe (0x802091) mask meets, so the Panzer's turret surfaces stop rounds
  where the turret points. The result carries the entity's own contents
  (no glass, so the round ends), the struck surface's flags (13, metal, on
  the stock hulls) and hit location 0 (VERIFIED decompile). The trace's
  pass rule skips the pass entity, what it owns and what its owner owns
  (0x809a72a): a vehicle's own round (`fireTurret` passes the vehicle)
  skips its hull and riders, owned by it; a rider's own round skips the
  other riders but not the hull.
- On a hull `Bullet_Fire_Extended` draws the shooter's impact (178 with
  the surface, the weapon and the reflection) and runs `G_Damage` on the
  vehicle with the leg's falloff damage, the shot's forward and the hit
  point (`coduo-diff.md` 21) (VERIFIED decompile). Measured, retail UO
  1.51b, `client-probes/probe_vehicle_hits` (fixtures
  `*-jeep-hits-{side,front,rear}-uo.txt`, `mp_cassino-tank-hits-side-uo.txt`):
  a `thompson_mp` 250 off an empty Willys's flank lands 22 a round (50 at
  `dmgScaleSide` 90 and `dmgScaleBullet` 50, truncated), 25 from the
  front, 18 from behind (18.75 truncated), `MOD_PISTOL_BULLET`, the
  shooter as both attacker and inflictor; a Panzer IV stops the
  `bulletTrace` and the rounds on its side and takes none (`bulletDamage`
  0) (VERIFIED, live). vcod poses the hull per bone
  (`vehiclepose::posed_collision`) for every shot, swing, missile step and
  blast probe, and `vehicle_hits_ab` matches all four within a fifth of
  retail's notify count with retail's amounts.
- **A shell on a hull.** `G_MissileImpact` (0x5db67) runs `G_Damage` on a
  struck entity that takes damage: `Damage_Falloff` of the shell's flight
  distance, truncated (0x5de9d), the flight velocity as the direction, the
  contact as the point, dflags 0, the shell's means of death, the driver
  as the attacker and the vehicle as the inflictor for a vehicle's shell;
  the splash then skips that entity; the explode is 186, not 185, when the
  trace named a player or a bone (`trace+42`, a hull's surface), with
  0x100000 on a vehicle (VERIFIED decompile). Measured, retail UO 1.51b,
  `client-probes/probe_vehicle_shell` on mp_kharkov (fixtures
  `mp_kharkov-tank-shell-uo.txt`, `*-uo-shellveh.bin`): a Panzer IV's
  three shells on the Elefant 276 units off each give it one notify, 279
  `MOD_PROJECTILE` (700 through its side scale), attacker the driver,
  inflictor the tank, and an explode 186 with `eFlags` 0x100000 on metal
  (13) (VERIFIED, live). `vehicle_shell_ab` matches every field.
- `VEH_PlayerCollision` (0x8213f): a body a probe strikes is knocked away
  and, at most every 500 ms, takes `150 min(speed / 400, 1)` under
  `MOD_CRUSH_JEEP` or `MOD_CRUSH_TANK`, the driver the attacker (VERIFIED
  decompile).

## 14. What vcod-server does, and what it measured

- **Code.** `crate::game::vehicle` (`info`, `physics`, `seat`, `think`,
  `fire`) and `game::builtins::vehicle` (the 31 methods but the five
  path ones, `precacheVehicle`, `getNumVehicles`, `doDamage`,
  `doDamageMod`, `spawnDuplicate`, `linkIntoWorld`, `unlinkFromWorld`,
  `verifyPosition`, `clearVehiclePosition`). A tank shell is a
  `crate::game::missile` shell with the vehicle as its inflictor.
- **`configstrings_uo_ab`.** The whole configstring table against
  `gamestate-uo.bin` (vehicles included: models, aliases, effects, hint
  strings, configstring 8), and the dm tanks' baselines 176 and 181 field
  by field (VERIFIED).
- **`vehicle_ab`.** Two new retail captures, `--probe-vehicle drive
  --save-slope` (`mp_cassino-tdm-slope-8ms-tank-drive-uo.txt`, a Panzer
  IV, `mp_peaks-tdm-slope-8ms-jeep-drive-uo.txt`, a Willys; each
  snapshot line carries the vehicle's whole entity state), and the seat
  capture of section 8. The replay boards the same vehicle on ours
  through the probe gametype and the use key, sends the capture's cmds on
  our clock (each rebased on our `delta_angles`, as the probe builds them,
  and `nextvehslot` where the capture's seat changes) and diffs every
  snapshot. The vehicle's state matches retail on every field and every
  snapshot of both drives: pose, sway bytes, turret, steer, track clock,
  seat mask, events (the main gun, the coaxial MG, the turret's stop
  alias), `loopSound`. The rider matches in seat, view, pm flags, legs
  and origin (VERIFIED, replay). Gaps: the few snapshots after the exit,
  where ours rises or falls off retail's spot about a cmd ahead, and the
  probe's view past the seat pitch clamp, where the per-cmd push runs a
  snapshot apart from the rebase (the clamp itself is `moves_ab`'s).
- **`vehicle_drive_ab`** replays the step alone on the collision world
  against the two drive captures. Run free from the first snapshot it
  matches retail on every compared field through both drives, the clip
  brush bounces included (VERIFIED, replay); rebased on retail's pose at
  every snapshot it misses where the state the wire does not carry
  (vertical velocity, sway velocity, a hit's lockout) matters, each such
  stretch named in its gaps.
- **`vehicle_fire_ab`.** The Panzer IV's guns on ours against
  `*-uo-tank.bin` (VERIFIED, replay and capture):
  - Retail's two main-gun shells explode as 185 on `eType` 0 with type 6
    in `es.scale`, weapon 42 (`panzeriv_turret_mp`), `eFlags` 0x8000 and
    no vehicle bit. The second, fired at 31100, is first sent already
    exploded (entity 164, `eventSequence` 1): it struck within its first
    step. So is ours, fired from the probe's seat into the wall 500 units
    ahead. The client plays that explode only because retail's
    `CG_ResetEntity` replays an entering missile's or general entity's
    ring from 0 (`cod11-events-and-fx.md` 1, "An entity that enters the
    snapshot"); vcod's tracker skipped it, which was the hand test's tank
    shell with sound off the fire event and no explosion.
  - Along retail's first shell's line (its in-flight `trBase` to its
    explode) ours stops on the face retail's struck, (3363.5, -2709.9, 89)
    on mp_cassino, with retail's normal (byte 32) and surface (16,
    plaster). That face is a plaster side of a caulk brush, and vcod read
    the brush's caulk (0) until its traces took the struck side's own
    material (`bsp-ibsp59-format.md`, lump 3): retail drew
    `tank_plaster.efx` where vcod's client would have drawn
    `tank_generic.efx`.
  - The coaxial (164) and gunner (165) rounds that strike no player carry
    their weapon with bit 7, `0x9b` and `0xa0` on both sides, and neither
    side sends a bullet-hit temp entity for them: the client traces them
    (`fx::client_bullet`, coduo-diff.md 19).
  - A shell on a vehicle: `G_MissileImpact` (game.mp.uo 0x6db67) ors
    0x100000 into the explode's `eFlags` when the struck entity's `eType`
    is 12 (VERIFIED decompile), which turns the client to the impact
    row's `vehicle` column. vcod's shell names the entity its trace
    stopped on (a vehicle's collmap names its vehicle) and the entity pass
    marks the explode when that is a live vehicle. No capture has a shell
    on a vehicle yet; the unit test drives one into a named collmap.
  - The landing surface off the tank (section 7) was the same per-side
    material.
- **Riders.** Every damage path into a player's callback goes through
  `Vehicles::occupant_damage`, the rider branch above: shots, swings,
  blasts, the `radiusDamage` walk, crushes, shells, `doDamage`. A bot
  never picks a tank's driver as its enemy, since no client is sent his
  body. `vehicle_armor_ab` replays the probe on ours. The driver's two
  stints take no round, and every other seat and the two stints on foot
  take some, as on retail (VERIFIED, replay).
- **Hulls, shells, pushes.** Section 13's hull (`vehicle_hits_ab`,
  `vehicle_armor_ab` with the probe client's cmd stream), shell
  (`vehicle_shell_ab`) and section 12's push (`vehicle_ram_ab`).
- **Not done.** The armor probe's riders still take more rounds on ours
  than retail's (the Willys's driver 36 against 1); the seated body's pose
  as the bone trace meets it is the suspect. The driver's flash-barrel
  test; the path methods; `closespeed` and `harc` read 0; the probe hit in
  `Scr_Vehicle_Die` and the stuck self-kill; the compass's tanks
  (`G_GetNonPVSTankInfo`); `vehicle_hit_person` and the collision impact
  alias; the model arm's whole-model bounds reject (0x80c6b36), which
  vcod does not take.
