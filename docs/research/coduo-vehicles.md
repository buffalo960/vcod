# United Offensive 1.51 vehicles from the seat

How a United Offensive player drives, rides and fires a vehicle: what the
client sends from a seat, what the server makes of it, what the client
predicts there and where its camera sits. `coduo-diff.md` 9 covers the
vehicle as an entity (definition files, spawn, physics outline, damage,
how the cgame poses and draws it); this document is the occupant's side.
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
- `G_VehiclePopOut` (0x8bbb0) toggles 0x400000 and moves the rider to
  `tag_popout` (VERIFIED decompile); no caller was found, and no stock
  key reaches it (INFERRED).

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
  aim sits on the gun the server turns at 360°/s. A bob off a
  `thompson_mp` playerstate (0x30015ca0, scale 2) and a firing shake are
  added to the eye (VERIFIED, the calls; their shape INFERRED).
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

**Field of view** (0x3003ffc0, VERIFIED decompile): in a seat but the
four-wheel's seat 3, the driver with a sighted weapon closes from
`cg_fov` toward 70 (0x3007bf84) by `fWeaponPosFrac`; every other seat
keeps `cg_fov`.

## 6. View weapon and HUD

- `CG_AddViewWeapon` (0x30046570) draws no gun while `eFlags & 0x106000`
  is set, except in the four-wheel's seat 3 (VERIFIED decompile).
- The seat crosshair (0x30019cf0) and the vehicle status owner draws are
  `coduo-diff.md` 19's, and vcod draws them already.

## 7. Prediction against the capture

`moves_ab`'s `the_predictor_replays_uo_seats` rebuilds every snapshot of
section 8's two captures through `predict::from_wire` with its vehicle's
angles and predicts it to the next under UO's rules. Every field but
these matches retail (VERIFIED, replay):

- `delta_angles` at each `nextvehslot`: the new seat's
  `SetClientViewAngle`, the server's.
- `origin` and `velocity` on the exit's snapshot: the dismount.
- `aimSpreadScale` on the first pair, which has no cmd at its
  `commandTime`; on the tank also the snapshot the main gun fired on (44
  on retail, written outside pmove, INFERRED) and two inside the pitch
  clamp, a unit apart (unexplained).
- `weaponTime` and `weaponDelay` on the jeep passenger's shot: retail's
  begin one 8 ms cmd later (unexplained).
- On the tank, `eventSequence` on the main gun's snapshot (an event the
  server gives the driver outside pmove, INFERRED) and `events` on the
  landing after the exit, which falls from the dismount's velocity.

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
  angles filled by the caller (the predictor from the newest snapshot's
  vehicle entity, not the lerped one). `pmove`'s seat arm and
  `cmd::clamp_view` follow section 3; the 16000 pitch clamp now applies on
  foot under every patch's rules.
- **View.** `play::seat::camera` and `clip` follow section 5, off
  `EntityScene::tag_pose` on the drawn vehicle and the world's box trace.
  The gunner's snap and the four-wheel's turn go through
  `PlayInput::set_view`, the trap 0xf7 write. A followed rider gets the
  seat camera on the snapshot's view, without the writes.
- **Input.** The stock keys already make section 2's bytes
  (`a_seat_drives_with_the_stock_keys`); `weapnext`/`weapprev` send
  `nextvehslot`/`prevvehslot` in a seat; `v cl_run` sets the cvar and `v
  cl_stance` the stance.
- **View weapon and fov** as section 5 and 6.
- **Not done.** The gunner's bob and firing shake; the roll 0x30040580
  writes; the own body in a seat (retail draws the local rider of a jeep's
  seat 1 and of passenger seats, vcod never draws its own body);
  `G_VehiclePopOut`'s hatch view; the jeep's steering anim blend; the
  push-off 0x30035800 gives a body that walks into a vehicle; vehicle
  sounds and the vehicle MG's client-traced impacts (another branch).
