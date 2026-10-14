# United Offensive 1.51 movement against 1.5

What United Offensive changed in the movement a client predicts, read out
of its game module and measured on the retail 1.51b server. The wire is
`coduo-diff.md`'s; 1.5's movement, which UO keeps, is
`cod15-movement.md`'s. The short answer: UO is 1.5 plus a sprint with a
fatigue bar, and an LMG that sights only by deploying a bipod.

## Sources and conventions

The binary is `uo/game.mp.uo.i386.so` 1.51b (md5 in `coduo-diff.md`,
dynamic symbols). Addresses are its `nm` addresses; Ghidra's import sits
0x10000 higher, and its labels for the module's statics (`FUN_xxxx`) carry
that higher address. Rodata constants were read from the file bytes at
their `nm` address (the `.rodata` file offset equals the vaddr). Struct
offsets name `ps` fields through the playerState netfield table
(`coduo-diff.md` 4): `fatigueScale` 0x5b0, `lastSprintTime` 0x5b4,
`sprintSpeedScale` 0x58c, `proneDirection` 0x5a4, `weaponstate` 0xdc,
`fWeaponPosFrac` 0xe0. Method: the Ghidra export of the module, paired
with 1.5b's function by function through `PmoveSingle`'s call order, and
the capture of section 9. Every claim carries its own label.

## 1. Call order

`PmoveSingle` (0x2d530) against 1.5's (0x27eb7), VERIFIED from both
decompiles:

- New in the prologue: the held weapon's def is looked up first; when the
  previous cmd sprinted (`pm_flags` 0x10000)
  `PM_InteruptWeaponWithSprintMove` (0x33a44) runs; with the sight flag
  (0x20) on an LMG (`weaponClass` 3) the cmd's forward and right moves are
  zeroed, as they are with the sight flag on an effective prone stance.
- The normal arm runs `PM_UpdatePlayerSprintingFlag` (0x2c74a) after
  `PM_UpdatePlayerWalkingFlag` and before `PM_UpdatePronePitch`, and
  `PM_UpdateFatigue` (0x2aa22) after `PM_DropTimers`, before the move.
- The mounted arm (`eFlags & 0x106000`, a turret or a vehicle seat) and
  the linked arm run the sprint flag and the fatigue too; their weapon
  step runs only for the one seat `BG_AllowPlayerWeaponAtVehiclePos`
  allows.

So the sprint flag a cmd moves with is decided on that cmd, after its
stance and ground trace (INFERRED from the order).

## 2. The sprint flag

`PM_UpdatePlayerSprintingFlag` (0x2c74a) clears `pm_flags` 0x10000 and
sets it again only when every one of these holds (VERIFIED decompile, the
two fatigue thresholds VERIFIED bytes at 0x9b968 and 0x9b96c):

- `pm_type < 4` and the cmd's `buttons` 0x8;
- `fatigueScale > 0`, and `> 0.25` unless the previous cmd sprinted;
- `pm_flags & 3` clear (neither prone nor ducked) and 0x20 clear (no
  sight);
- on the ground, unless the previous cmd sprinted;
- a forward or a right move, either sign;
- not mounted, `weaponstate` not 13, not on a ladder (`pm_flags` 0x10);
- with a grenade in hand (`weaponType` 1): not cooking, the attack button
  up and `weaponstate` not 3.

The capture's backpedal and strafe sprints set the flag (VERIFIED), and a
sprint held against a wall keeps it with no speed (VERIFIED, ct 29092).

## 3. Speed

The walk's cmd scale (0x23af5) picks `walkSpeedScale` for the ADS walk, a
lean or an LMG in `weaponstate` 13, else `sprintSpeedScale` while 0x10000
is set, else `runSpeedScale`; while sprinting it leaves the weapon's
`moveSpeedScale` out (VERIFIED decompile). The air and ladder scale
(0x23905) makes the same three-way pick (VERIFIED decompile).
`sprintSpeedScale` reads 1.6 on the wire (VERIFIED, `coduo-diff.md` 5).
In the capture the mg30cal (`moveSpeedScale` 1.1) runs at 209 and
sprints at 304, 190 times 1.6 with no weapon factor (VERIFIED).

`PM_Footsteps` (0x297e4) divides the bob rate by the sprint scale in place
of the run scale while sprinting (VERIFIED decompile). The sprint
footstep events are their own ids (`surface + 70`, 0x25b9b, VERIFIED
decompile); vcod raises its run ids instead.

Acceleration and friction are 1.5's (rodata 0x9b4a4..0x9b4e4 equal to 1.5's
0x75624..0x75664, VERIFIED bytes).

## 4. Fatigue

`PM_UpdateFatigue` (0x2aa22), constants VERIFIED bytes: 0.001 at 0x9b8fc,
0.5 at 0x9b900, `pm_sprintFatigue` 1/3 at 0x9b4e8, `pm_fatigueRegen` 1/7
at 0x9b4ec, `pm_fatigueRegenDelay` 1000 at 0x9b4f0. The logic, VERIFIED
decompile:

- a sprinting cmd (not noclip, `bg_nofatigue` 0) stamps `lastSprintTime`
  with its `serverTime` and takes `0.001 * msec / 3` off the bar; under
  0.5 it raises event 140 and sets `pm_flags` 0x20000 (winded); under 0
  the bar is 0;
- a cmd that neither sprints nor holds the button adds `0.001 * msec / 7`
  once `serverTime >= lastSprintTime + 1000`; at 1 the bar is 1 and 0x20000
  clears; then `PM_PlayFatigueSound` (0x2a968) repeats event 140 every
  1700 ms while winded;
- a held button that does not sprint refills nothing.

Measured (VERIFIED, the capture): a sprint from full ran 2967 ms (ct 21099
to 24066), wound at 0.492 1483 ms in, and the bar came back from 0.09 to 1
at ct 37240, 1000 ms after the last sprint plus 6.4 s; a sprint pressed at
0.345 started. Nothing else in the module reads the bar (VERIFIED by
absence): it slows nothing and drains on no jump or stance change.

## 5. The jump

`PM_CheckJump` (0x24483) is 1.5's, the takeoff curve, the landing arms and
the 500 ms gate equal (VERIFIED bytes and decompile), with one refusal
added: no jump with the sight flag on an LMG (VERIFIED decompile). The
capture's sprinting jump rose 40.8, 1.5's unpenalised height at 125 fps
(VERIFIED, peak at ct 38890).

## 6. The weapon while sprinting

There are no sprint weaponstates (the state names at 0x9d6a0, VERIFIED).
`PM_InteruptWeaponWithSprintMove` (0x33a44) drops a deploy or a breakdown
to `weaponstate` 0 with both timers 0 and the raise anim, and leaves every
other state (VERIFIED decompile). Inside `PM_Weapon` (0x3726c, VERIFIED
decompile):

- the shot (0x36c0e) never goes while sprinting, a spotter only down an
  `adsFire` sight, an LMG only with the sight flag and `fWeaponPosFrac` at
  0.99 or more (0x9d58c);
- a reload (0x358b2) never starts while sprinting, nor on an LMG short of
  the same 0.99 (0x9d4e8);
- the swing (0x36fb7) does not latch while sprinting, and not at all on a
  deployed LMG or one in `weaponstate` 13;
- `aimSpreadScale` is held at 255 while sprinting with a move key, as for
  a prone body moving.

## 7. The LMG bipod

`PM_UpdateAimDownSightFlag` (0x32c0b, VERIFIED decompile; 15.0 at 0x9d4cc
and 1.0 at 0x9d4d0, VERIFIED bytes). The sight's usual conditions are
1.5's plus one: `weaponstate` 13 keeps the flag only while the cmd's
weapon byte is the held weapon. For an LMG without the flag:

- `pointcontents` 15 units ahead along the view's yaw and 1 up, mask
  0x404000: a hit deploys there, standing, or crouched where the contents
  carry 0x400000, with `proneDirection` the view's yaw;
- else `serverCursorHint` 7 (a `trigger_mount`) deploys the same way, its
  `serverCursorHintVal` choosing crouched;
- else a body not prone that is airborne or fails `BG_CheckProneValid`
  (0x2162c) with its 16th argument set is refused: `pm_flags` 0x8000 and
  `EV_STANCE_FORCE_STAND` or `_CROUCH` unless `wbuttons` 0x2;
- else the flag goes on with `pm_flags` 1: the body is put prone.

An LMG with the flag keeps it as long as the conditions hold (VERIFIED
decompile). The 16th argument adds a +/-6 box from 8 up to 38 ahead,
which must be clear (VERIFIED decompile; 8, -38 and 180 at 0x9a9b4,
0x9a9b8 and 0x9a9ac, VERIFIED bytes). The mount contents are the
`hmgstanding` and `hmgcrouching` brushes (`coduo-diff.md` 11). In the
capture a sighted press standing against a wall was refused on every cmd
(`pm_flags` 0x8000 and `EV_STANCE_FORCE_STAND` from ct 42499, VERIFIED).

The deploy driver (0x35b66) runs in `PM_Weapon` for `weaponClass` 3 and 8
outside a match timeout, after the timers (VERIFIED decompile). It leaves
`weaponstate` 1, 2, 10 and 11 alone, and 5 to 9 while sighted (INFERRED,
its jump table). Otherwise a sight flag on a fraction short of 1 starts
the deploy (0x347aa: `weaponstate` 12, `weaponTime` the def's
`adsTransInTime`, `weapAnim` 19, event 160) and the flag's loss on a
fraction above 0 starts the breakdown (0x3484c: 13, `adsTransOutTime`,
20, event 161) (VERIFIED decompile). Each ends to `weaponstate` 0 and
`weapAnim` 0 once both timers are out (0x350c2, 0x35120, VERIFIED
decompile). The stock mg30cal reads `adsTransInTime 1.8`,
`adsTransOutTime 1.9` and `moveSpeedScale 1.1` (VERIFIED, the weapon
file), and the capture's first deploy snapshot reads `weaponTime` 1774,
25 ms in, the breakdown's 1875 24 ms in (VERIFIED, ct 48091 and 58582):
both start a millisecond short, 1799 and 1899. That the def stores the
float's thousandfold truncated is INFERRED from those two.

While deployed (VERIFIED decompile): the moves are zeroed (section 1),
`PM_CheckDuck` (0x27e16) takes no stance key, `PM_UpdateLean` (0x2ac45)
eases any lean back, the jump is refused, the sprint cannot start, the
swing is refused, and the putaway (0x348ee) refuses a switch off an LMG or
spotter with the flag or a fraction above 0.0001. The sight stays up
through an LMG's reload (`PM_UpdateAimDownSightLerp` 0x3349e). The
breakdown walks (section 3).

`PM_UpdateViewAngles` (0x2b15c, VERIFIED decompile; 5 at 0x9b948, 55
deg/s at 0x9b94c, 0.98 at 0x9b950, +/-45 at 0x9b954/0x9b958, VERIFIED
bytes) runs its prone block for a prone body and for a deployed LMG. The
yaw cap is `bg_lmg_yawcap` (55, a mirrored cvar) whenever the held weapon
is an LMG, prone and undeployed included, and `bg_prone_yawcap` otherwise;
the body's swing toward the view runs only for a prone body not deployed,
and the pull of the view toward the body only for a prone body without an
LMG. The cap and the pitch clamp then run as in 1.5. In the capture the
deployed view swung 70 degrees off `proneDirection` was held at 55 by
`delta_angles[1]` pushes (VERIFIED, ct 53450 on). The pitch clamp of a
standing or crouched deploy measures from `proneTorsoPitch`, which only
prone writes (INFERRED).

How a client reads a deploy off the wire: `pm_flags` 0x20 with an LMG in
hand, the stance in `pm_flags` 3, `weaponstate` 12 or 13 through the
transitions and `proneDirection` as the cap's centre (INFERRED from the
writes above).

## 8. Buttons and binds

`CL_CmdButtons` (`CoDUOMP.exe` 0x40c700, VERIFIED decompile) puts
`+sprint` in `buttons` 0x8; every other bit is 1.1's. UO's
`default_mp.cfg` (`localized_english_pakuo01.pk3`) binds ALT to
`+sprint` (VERIFIED, the file). There is no hold-breath or binoculars
button; binoculars are a weapon (VERIFIED by absence of the strings).

## 9. The capture

`crates/server/tests/fixtures/playerstate/mp_carentan-dm-slope-8ms-sprint-uo.txt`,
2026-10-14: `sprint_script` (`--probe-sprint` in
`crates/client/src/probe.rs`) from `client-probes/probe_prone`'s street
placement on `COD_VERSION=uo tools/run_probe.sh`, `--probe-weapon
mg30cal_mp`, cmds every 8 ms, every snapshot's whole playerstate. The
script runs, sprints past exhaustion with the view swinging round, rests,
sprints too early, sprints with a jump and each other gait (backpedal,
strafe, crouched, sighted, firing), lies down, deploys, swings the view
past the cap both ways, moves and fires deployed, breaks down and stands.
mp_carentan is pak4's map, the same file in 1.1 and 1.5 (VERIFIED, both
installs list it at 10971520 bytes, 2003-10-03).

`crates/server/tests/moves_ab.rs`'s
`the_predictor_replays_uo_sprint_and_bipod` replays it through the client's
predictor under UO's rules: 900 snapshots, every compared field right but
the first pair's `aimSpreadScale` (no cmd at the first snapshot), and
under 1.5's, which misses the sprint on 156 snapshots. It needs `COD_DIR`
at a UO install.

## 10. What vcod does

`MoveRules::Uo` (`crates/common/src/pmove.rs`), which the predictor takes
for protocol 22: sections 1 to 7 as written, with these left out:

- the `trigger_mount` cursor hint deploy (the server's hint is not read);
- `bg_nofatigue` and the mirrored `bg_lmg_yawcap` (the defaults are
  used);
- event 140 and `iFatigueSoundTime`, and the sprint footstep ids;
- the grenade cook (`coduo-diff.md` 11), so the sprint's cooking test
  never fires;
- vehicles: the seat arm, the view clamps and any input.

The client sends `buttons` 0x8 to a UO server only, from `+sprint`, which
`vcod`'s default binds put on ALT.
