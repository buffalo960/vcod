# United Offensive 1.51 movement against 1.5

What United Offensive changed in the movement a client predicts, read out
of its game module and measured on the retail 1.51b server. The wire is
`coduo-diff.md`'s; 1.5's movement, which UO keeps, is
`cod15-movement.md`'s. The short answer: UO is 1.5 plus a sprint with a
fatigue bar, an LMG that sights only by deploying a bipod, and a grenade
that cooks.

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
  zeroed, as they are with the sight flag on an effective prone stance
  (1.1 and 1.5 zero the prone case too, section 13).
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
of the run scale while sprinting (VERIFIED decompile). The footstep id
picker (0x25b9b) adds 47 to the surface for a prone body, else 70 for a
sprinting one, else 24 for a walk, else 1 (VERIFIED decompile, those
immediates); the jump's own event (0x24328) is the same `surface + 70`, 83
on a ladder (VERIFIED decompile). The UO cgame plays 70..92 as
`step_sprint_<surface>` plus `gear_rattle_sprint` (`CG_EntityEvent`
`uo_cgame_mp_x86.dll` 0x30022810, its footstep ranges at
0x3002289c-0x30022933, VERIFIED disassembly), so a UO jump sounds like a
sprint step. The sprint capture's sprint steps read 76 (dirt, 70 + 6)
(VERIFIED, section 9).

Acceleration and friction are 1.5's (rodata 0x9b4a4..0x9b4e4 equal to 1.5's
0x75624..0x75664, VERIFIED bytes).

## 4. Fatigue

`PM_UpdateFatigue` (0x2aa22), constants VERIFIED bytes: 0.001 at 0x9b8fc,
0.5 at 0x9b900, `pm_sprintFatigue` 1/3 at 0x9b4e8, `pm_fatigueRegen` 1/7
at 0x9b4ec, `pm_fatigueRegenDelay` 1000 at 0x9b4f0. The logic, VERIFIED
decompile:

- a sprinting cmd (not noclip, `bg_nofatigue` 0) stamps `lastSprintTime`
  with its `serverTime` and takes `0.001 * msec / 3` off the bar; under
  0.5 it raises event 140 (`EV_FATIGUE_LAST_SOUND`) and sets `pm_flags`
  0x20000 (winded), on every such cmd with no throttle; under 0 the bar is
  0;
- a cmd that neither sprints nor holds the button adds `0.001 * msec / 7`
  once `serverTime >= lastSprintTime + 1000`; at 1 the bar is 1 and 0x20000
  clears; then it calls `PM_PlayFatigueSound` (0x2a968);
- a held button that does not sprint refills nothing and calls
  `PM_PlayFatigueSound`; a cmd inside the refill's delay without the
  button calls nothing.

`PM_PlayFatigueSound` (0x2a968, VERIFIED decompile; 0x6a4 = 1700 its
immediate): winded, once `iFatigueSoundTime` (ps+0x3c, the netfield of
that name) is more than 1700 ms behind the cmd's `serverTime`, it raises
event 140 and stamps the field with `serverTime`; not winded, it zeroes a
positive stamp that old. Event 141 (`EV_FATIGUE_SOUND`) is raised nowhere
in the module (VERIFIED by absence of the immediate). The UO cgame plays
140 as `fatigue_breath` on the event's entity, for every player, and 141
as `sprint_breath_last`, an alias no stock csv defines (VERIFIED,
`CG_EntityEvent` cases 0x30022b3c and 0x30022b23 and the csvs). The sprint
capture's event ring and `iFatigueSoundTime` match vcod's prediction on
every snapshot (VERIFIED, section 9).

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

The sight through a rechamber is 1.5's (`cod15-movement.md` 6).
VERIFIED: the weapon file field table entry at `.data` 0xabd90 names
`rechamberWhileAds` at weaponDef +0x32c (type 5), and
`PM_UpdateAimDownSightLerp` tests that field against 0 at 0x33682 and
`weaponstate` against 4 at 0x33695, both branching to 0x336a0. INFERRED,
from the branch targets: 0x336a0 is the path that lowers the sight, so a
rechamber of a gun whose file says 0 drops the scope whatever the button
says, and the held button raises it again once the state leaves 4. UO's
`kar98k_sniper_mp`, `mosin_nagant_sniper_mp` and `springfield_mp` all
say 0 (VERIFIED, `pakuo` weapon files). 1.1 has no such test: its
rechamber picks `WEAP_ADS_RECHAMBER` on a raised sight and keeps it
(`cod11-combat.md` 1.9). So a scoped shot comes every `fireTime` plus
`rechamberTime` plus `adsTransInTime` on UO (0.33 + 0.95 + 0.45 s on the
springfield), for a player and a bot alike.

## 7. The LMG bipod

`PM_UpdateAimDownSightFlag` (0x32c0b, VERIFIED decompile; 15.0 at 0x9d4cc
and 1.0 at 0x9d4d0, VERIFIED bytes). The sight's usual conditions are
1.5's plus one: `weaponstate` 13 keeps the flag only while the cmd's
weapon byte is the held weapon. For an LMG without the flag:

- `pointcontents` 15 units ahead along the view's yaw and 1 up, mask
  0x404000: a hit deploys there, standing, or crouched where the contents
  carry 0x400000, with `proneDirection` the view's yaw;
- else `serverCursorHint` 7 deploys the same way, `serverCursorHintVal`
  non-zero choosing crouched (section 11);
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

A deployed LMG's rounds start from the eye pushed up to 19 forward along
`proneDirection` by an 8-unit box trace, `CalcMuzzlePoint` 0x8b8ea
(VERIFIED; addresses and the rest of UO's muzzle in `cod11-combat.md` 2.1,
"1.5 and United Offensive").

## 8. Buttons and binds

`CL_CmdButtons` (`CoDUOMP.exe` 0x40c700, VERIFIED decompile) puts
`+sprint` in `buttons` 0x8; every other bit is 1.1's. UO's
`default_mp.cfg` (`localized_english_pakuo01.pk3`, line 9) binds ALT to
`+sprint` (VERIFIED, the file). There is no hold-breath or binoculars
button; binoculars are a weapon (VERIFIED by absence of the strings).

The controls screen: UO's `ui/options_move.menu`
(`localized_english_pakuo00.pk3`) is 1.1's file plus a last bind row,
`@GMI_MENU_SPRINT` on cvar `+sprint` at rect `5 325` after `+strafe`
(VERIFIED, both files). The UI module's `g_bindings` table
(`uo_ui_mp_x86.dll` 0x400406b0, 55 records of 24 bytes, against 1.1's 50
at `ui_mp_x86.dll` 0x40036130) adds `mp_quickmap` and `mp_purchase` after
`mp_QuickMessage`, `weaponslot satchel` and `weaponslot binocular` after
`weaponslot smokegrenade`, and `+sprint` between `toggle cl_run` and
`screenshot` (VERIFIED, the table).

The config: UO's `Com_Init` queues `exec default_mp.cfg`, `language.cfg`,
`uoconfig_mp.cfg` and `autoexec_mp.cfg` (`CoDUOMP.exe` 0x43bd3c-0x43bd5f,
VERIFIED strings and pushes). A written `uoconfig_mp.cfg` opens with
`unbindall` (VERIFIED, the file). So a config without a `+sprint` line
would leave ALT unbound (INFERRED from that order). UO's file is its own,
so a first UO run never reads 1.1's `config_mp.cfg` (VERIFIED, the two
names).

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
for protocol 22: sections 1 to 7, 11 and 12 as written, the fatigue
breaths and the sprint footstep ids included, with these left out:

- `bg_nofatigue` and the mirrored `bg_lmg_yawcap` (the defaults are
  used);
- vehicles: the seat arm, the view clamps and any input.

The client sends `buttons` 0x8 to a UO server only, from `+sprint`, which
`vcod`'s default binds put on ALT. `BIND_COMMANDS`
(`crates/client/src/frontend/options.rs`) carries UO's five extra
`g_bindings` rows, so UO's Sprint row rebinds it. vcod keeps one
`vcod_mp.cfg` per mod directory, as UO keeps its own file; one under
`--mod-dir uo` written before vcod had the sprint gets ALT back when ALT
and `+sprint` are both unbound (`Shell::bind_if_free`), which is what a
UO-written config would hold. The client side is section 14.

## 11. The mounts and the cursor hint

`G_CheckForCursorHints` (0x575bb, VERIFIED decompile; 15.0 and 1.0 at
0xa1098 and 0xa109c, VERIFIED bytes) clears `serverCursorHint`, its value
and its string each frame, and for a live client holding a `weaponClass`
3 weapon that is neither sighted nor in a vehicle seat it probes
`trap_PointContents` 15 ahead along the view's yaw and 1 up under mask
0x404000. A hit, or `G_CheckPointInsideTriggerMount` (0x7780f) finding a
`trigger_mount` within 0.1 of the point, sets the hint to 7; only the
second sets the value, to the trigger's spawnflag 1 (INFERRED, the
branch order). So a crouched mount the server's own contents see sends
value 0.

`trigger_mount` (`SP_trigger_mount` 0x776d5) is a trigger whose contents
are 0x4000, or 0x400000 with spawnflag 1; script makes one with
`spawnTriggerMount(mins, maxs, crouch)` (0x67974, `SP_trigger_mount_no_brush`
0x77754), a box with the same contents (VERIFIED decompile). No stock UO
map places one and no stock script spawns one (VERIFIED, every UO BSP's
entity lump and every stock `.gsc`); every stock UO map has both mount
brushes (VERIFIED, the material lumps).

The captures (VERIFIED, 2026-10-14):
`crates/server/tests/fixtures/playerstate/mp_foy-dm-slope-8ms-uofoot-{stand,crouch,mount}.txt`,
`--save-slope --probe-uofoot <yaw>` (`uofoot_script` in
`crates/client/src/probe.rs`) on `COD_VERSION=uo tools/run_probe.sh
client-probes/probe_uofoot mp_foy +set probe_teleport 1 +set probe_spot
<spot>`, `--probe-weapon mg30cal_mp`, cmds every 8 ms:

- `stand`, facing the `hmgstanding` brush at (-85 2700 -5): the sight
  deployed standing (`pm_flags` 0xa0 over the base) in 1799 ms, and the
  same sight pressed crouched stood the body up deployed (0xa2 becomes
  0xa0) and the breakdown left it crouched under the held key;
- `crouch`, facing the `hmgcrouching` brush at (-1080 -3313 0): the deploy
  went crouched, hint 7 with no value on the wire beforehand;
- `mount`, in front of a `spawnTriggerMount` crouched box over open
  ground: hint 7 with value 0, and the server deployed crouched.

The last is the trigger's own contents seen by the server's pmove
(INFERRED: the server's point contents include linked entities, the
client's do not), while the predictor reading hint 7 value 0 deploys
standing on those two snapshots, as the retail cgame's identical code
would (INFERRED); the next snapshot corrects it. `moves_ab`'s
`the_predictor_replays_uo_mounts_and_cook` replays all three under UO's
rules: every compared field right but those two snapshots
(`UOFOOT_GAPS`). The stand and mount captures were retaken 2026-10-17
(VERIFIED, same recipe) with the cmds sent ahead of the first logged
snapshot in the file: the ones stamped past its `commandTime` run in the
first pair, and without them that pair's `aimSpreadScale` read one turn
short.

### The server's half

`G_CheckForCursorHints` (game.mp.uo 0x67aa0, VERIFIED decompile) zeroes
the hint and its value, and for a live client with no in-use flag sets the
string to -1 (0xff on the wire). A client holding a `weaponClass` 3 (LMG)
weapon with `pm_flags` 0x20 set, or one with `eFlags` 0x100000 (a vehicle
seat), skips the rest: a deployed bipod reads hint 0 and no activate
entity is looked for. Otherwise the LMG arm probes
`r.currentOrigin + 15 * forward(0, viewangles[1], 0) + (0, 0, 1)` with
`trap_PointContents(point, -1, 0x404000)`; a hit, or
`G_CheckPointInsideTriggerMount`, sets hint 7, the value that function
wrote (0 where only the contents hit) and, when the weapon's
`useHintString` (weapon def +0x494) is not empty, its index (+0x498) as
the string. The activate-entity loop runs after it and the first usable
entity's hint replaces the 7; its value stays (VERIFIED decompile, the
stores at 0x67c37). No stock LMG file sets `useHintString` (VERIFIED, the
`pakuo` weapon files), so the string stays -1.

`G_CheckPointInsideTriggerMount` (0x8780f, VERIFIED decompile) asks
`trap_EntitiesInBox` for the point grown 0.1 under mask 0x405c0008 and
takes the first entity whose classname is `trigger_mount`; the value is
its spawnflag 1. The entity query walks linked boxes, which `SV_LinkEntity`
grows a unit, so the value is only ever reached from a point within that
margin outside the trigger (INFERRED).

Whether `trap_PointContents` masks the world's contents with 0x404000 as
it does the entities' is not read out of the server binary; vcod masks
both (INFERRED: otherwise every wall 15 ahead would hint 7, and the
captures show no hint past the mounts).

`spawnTriggerMount(mins, maxs, crouch)` (0x77974, VERIFIED decompile)
reads two vectors and `Scr_GetBool`, sorts each axis, `G_Spawn`s,
sets the classname `trigger_mount`, `r.mins`/`r.maxs` to the sorted corners
at origin 0 and calls `SP_trigger_mount_no_brush`: touch function
`Touch_trigger_mount` (`G_Trigger`, the `"trigger"` notify), contents
0x401c0008 with 0x4000 or, crouched, 0x400000 and spawnflag 1,
`r.svFlags` 1 (no client sees it), `eFlags` |= 2, linked; the entity is
returned. `SP_trigger_mount` (0x876d5), the map's, ORs the same bit into
`InitTrigger`'s contents.

vcod-server (2026-10-16): `spawnTriggerMount` and a UO map's
`trigger_mount` register `TriggerKind::Mount` triggers
(`game::trigger`, `builtins::uo::spawn_trigger_mount`). The end frame
runs the LMG arm (`ScriptRuntime::cursor_hints`), writes
`serverCursorHintVal`, and puts hint 7 on the server's playerstate
(`ClientSim::set_cursor_hint`), which the shared pmove reads as the
client's does. The server's pmove point contents add every linked
trigger box holding a mount bit (`MoveWorld::with_boxes`,
`trigger::mount_boxes`): retail's `SV_PointContents` meets entities, which
is why the server deploys crouched on the script trigger where the
predictor, with hint value 0, deploys standing. A brush `trigger_mount`
is tested by its box, not its brushes; no stock map places one.

`crates/server/tests/mount_uo_ab.rs` replays the three captures' cmds
through vcod-server on `probe_uofoot` (VERIFIED, 2026-10-16): every
snapshot's `pm_flags`, hint and value run through the same states as
retail's, each within three snapshots of retail's length. One state one
snapshot long is dropped on both sides: the first cmd off the bipod still
reads ducked, since `PmoveSingle` checks the stance before the sight flag,
and retail's snapshot there held five release cmds where ours may hold one.

## 12. The grenade cook

In `PM_Weapon` (0x3726c), ahead of its timers, for `weaponType` 1
(VERIFIED decompile, the immediates 10, 0x33, 0x32, 0xa3, 0xd2, 0x640,
0xb6):

- with `cookOffHold` set, an edge of `buttons` 0x10 (the sight, pressed
  or released) against the previous cmd while `grenadeTimeLeft` is at or
  above `fuseTime` takes 10 ms off it and raises `EV_GRENADE_SPOON` (182);
- with `grenadeTimeLeft` positive: with `cookOffHold` and the fuse below
  `fuseTime` it loses the frame's msec; under 51 the grenade goes by
  itself: `grenadeTimeLeft` 50, event 163 (`EV_FIRE_WEAPON`), 210
  (`EV_GRENADE_SUICIDE`) as well when `explosionInnerDamage` (def+0x37c)
  is non-zero, a round off the clip, `weaponDelay` 0, `weaponTime` 1600,
  and the clip-only weapon taken with event 151 when it was the last
  (0x35ce8), and `PM_Weapon` returns;
- otherwise the held trigger pins `weaponDelay` at the frame's msec plus
  one, as 1.1's pin reads.

So the pullback alone leaves the fuse whole, as on 1.1, and only the
sight's edge starts it (INFERRED from the conditions). The server's
`ClientEvents` (0x40860) answers 210 with `player_die`, means of death
0x15, unless the thrower has the god flag (VERIFIED decompile).
`fire_grenade` (0x5f7ad) fuses the missile with what is left of
`grenadeTimeLeft`, 2500 ms when that is 0, and zeroes it (VERIFIED
decompile). `PM_UpdatePlayerSprintingFlag` refuses the sprint while a
`cookOffHold` grenade's fuse is running (section 2, VERIFIED decompile).
Every stock UO grenade has `cookOffHold 1`; the frag's `fuseTime` is 4 and
`explosionInnerDamage` 120 (VERIFIED, `pakuo07.pk3`).

Measured in all three captures of section 11 (VERIFIED): a plain throw
reads `grenadeTimeLeft` 4000 from the pullback to the throw; the sight
pressed 400 ms into a hold raises 182 and the fuse runs down 50 ms a
snapshot to the release; the sight held into the pullback and released
600 ms later spoons on the release; a cook held past the fuse ends with
the thrower dead (a corpse, no further spawned snapshot), the last
spawned snapshot reading 63 to 93 ms left. vcod's replay matches the fuse,
`weaponDelay` and every event by id on every snapshot.

## 13. Two questions from the first pass

**Weapon times truncate.** The weapon parser's field type 7 (the 22
millisecond keys from `fireDelay` to `adsTransBlendTime`) loads the float,
stores it to f32, multiplies by 1000.0f on the x87 stack and stores it
with `fistp` under a control word or'd with 0xc00 (VERIFIED instructions:
`ParseConfigStringToStruct` 1.1d 0x6adf8 case 0x6af30, `fistpl` at
0x6af62; 1.5b 0x717de case 0x7194e, `fistpl` 0x71984; UO 0x943ea case
0x9455a, `fistpl` 0x94590; the 1000.0f at 0x7a31c, 0x7f7dc and 0xa80fc).
So 1.8 is 1799 and 0.35 is 349 on every Linux server; 83 of 1.1's 522
stock time values, 88 of 1.5's and 125 of UO's 812 land a millisecond
under rounding (VERIFIED, a scan of the stock files). The Windows modules
call `__ftol2` on the unstored double instead (VERIFIED instructions,
`cgame_mp_x86.dll` 0x3003f3f8, `uo_cgame_mp_x86.dll` 0x3004f288). Under
the default 53-bit precision that equals rounding for every stock value
but two (INFERRED). vcod's
`weapon::stored_ms` truncates for pmove and the turrets, and every 1.1,
1.5 and UO gate stays green under it.

**1.1 and 1.5 hold a sighted prone body still too.** 1.1's `PmoveSingle`
(0x33dfc) zeroes the cmd's forward and right moves when `pm_flags & 0x21`
is 0x21 (0x33f48-0x33f61), and 1.5's (0x27eb7) when `pm_flags` 0x20 is
set and `PM_GetEffectiveStance` returns prone (0x281ae-0x281c8), each
early, before the move (VERIFIED instructions). vcod applies both
(`hold_still_sighted`).

## 14. The client: sounds and the sprint pose

Sounds, all VERIFIED from the cgame and the csvs:

- 140 plays `fatigue_breath` (section 4); vcod's cue table plays it on the
  event's entity.
- 182 plays the fixed `us_grenade_lever` at the event's origin from the
  world entity (case 0x30023298), not the weapon's `pullbackSound`.
- 70..92 play the sprint steps (section 3).

The sprint pose: the cgame's weapon field table (0x300843a8) loads
`sprintMoveF/R/U` (def+0x11c..0x124), `sprintRotP/Y/R` (0x128..0x130) and
`sprintMoveMinSpeed` (0x19c); `sprintRotMinSpeed`, in every file, has no
row (VERIFIED, the table). The gun's movement term (0x30044ce0, UO's
0x300362c0) takes `sprintMove*` and `sprintMoveMinSpeed` in place of the
stance's while `pm_flags` 0x10000 is set, and the tilt (0x30014ea0, the
game's twin 0x37cfb) `sprintRot*` from the unparsed minimum (VERIFIED
code); the stance's rates and the sight's fade are unchanged (VERIFIED
code). Every stock UO player weapon spells `sprintMove` (-14, -10, -14),
`svt40_mp` -35 forward, `sprintRot` 0 and `sprintMoveMinSpeed` 110
(VERIFIED, the files). There is no sprint viewmodel anim: no `*Anim` key
or weaponstate names one (VERIFIED). vcod's `GunMove::step` and
`movement_rot` (`crates/common/src/pmove/aim.rs`) take the sprint's keys
on the predicted sprint flag.

## 15. Automatic fire and the view kick

The capture: `mp_carentan-dm-slope-8ms-autofire-uo.txt`, `--probe-autofire`
(`autofire_script` in `crates/client/src/probe.rs`) with `mp44_mp` on
retail 1.51b, from `probe_prone`'s street placement. `moves_ab.rs`
(`the_predictor_replays_uo_automatic_fire`) replays it under UO's rules.

- The cadence is 1.1's. The trigger held 1.5 s from the hip fires 13
  rounds, one `EV_FIRE_WEAPON` and one round off the clip each, 100 or
  150 ms apart on the 50 ms snapshots, so 120 ms a shot, the file's
  `fireTime` 0.12. The same 1.5 s down the sight fires 12 (VERIFIED, the
  capture). Four taps 400 ms apart fire four rounds (VERIFIED, the
  capture). UO's `mp44_mp` (`pakuo07.pk3`) spells the same `fireTime`,
  `fireDelay`, `rechamberTime`, kick and spread keys as 1.1's `pak0.pk3`
  copy, apart from `adsSpread`, `hipSpreadMoveAdd` and `hipSpreadTurnAdd`
  (VERIFIED, the two files diffed).
- vcod-server fires the same script at the same rate: 12 hip rounds, 13
  sighted (one round either way off retail's, by where the hold starts
  against the 120 ms), four taps, and the same `weaponTime` pattern after
  each shot (VERIFIED, an `--capture-tag` run of the probe against ours, not
  committed). The predictor misses none of `weaponTime`, `weaponstate`,
  `weaponDelay` or `eventSequence` across the 240 snapshot pairs (VERIFIED,
  the gate); the one miss is the first pair's `aimSpreadScale`, as in the
  sprint gate.
- The recoil is 1.1's. `CG_WeaponFireRecoil` (`uo_cgame_mp_x86.dll`
  0x30016120, called from `CG_FireWeapon` 0x30047d20 under `pm_flags`
  0xc0000 for the view's own client) sets the view kick's speed from the
  sight's ranges (def+0x3f4..0x400) only at `fWeaponPosFrac` 1.0, else the
  hip's (def+0x43c..0x448), roll -0.5 times the yaw, and adds the gun's
  from the sight's (def+0x3d4..0x3e0) at any `frac` above 0.0, else the
  hip's (def+0x41c..0x428) (VERIFIED, the decompile; constants 1.0, 0.0,
  -0.5 and 1/32768 read at 0x3007bce0, 0x3007bcec, 0x3007bf50,
  0x3007bec0). `CG_KickAngles` (0x3003f9f0) steps 5 ms slices, centres at
  the sight's speed (def+0x404) above `frac` 0.5, else the hip's
  (def+0x44c), at 2400 with no weapon, moves back at 0.06 of the speed
  and clamps at 10 degrees (VERIFIED, the decompile; constants at
  0x3007bdac, 0x3007bda8, 0x3007bd30, 0x3007bce8). The sight's offsets
  sit 0x84 past 1.1's (0x370, 0x350, 0x380) and the hip's 0x8c past
  (0x3b0, 0x390, 0x3c0), each block in 1.1's order (VERIFIED, both
  decompiles). The code reads the same keys (INFERRED, the def layout is
  not dumped).

What was wrong in vcod: the client fired each UO shot twice. The
prediction replays from every new snapshot's playerstate, and
`predict::from_wire` kept that ring in UO's event numbering while the
prediction pushes and the dedup (`play::events::PredictedEvents`) records
1.1's. A slot the snapshot confirmed read as changed, played as the 1.1
event of UO's number (UO's `EV_FIRE_WEAPON` 163 is 1.1's
`EV_EJECT_BRASS`), and the record it left made the snapshot drain's own
copy of the fire play too: a second fire sound, flash and view kick per
round (VERIFIED, `a_uo_snapshot_confirming_a_shot_plays_nothing_more`
fails on the old ring). It hit every UO weapon and every predicted event
at 140 and up, not the mp44 alone (INFERRED, the mapping is by id). 1.1
and 1.5 number the ring alike, so neither was affected (VERIFIED,
`Protocol::event_to_v1` is the identity on both). `from_wire` now maps
the ring through `Protocol::event_to_v1`.

`crates/client/src/play/replay.rs` replays the capture through the
client's frame, one frame per cmd: the prediction, the event collection
and its dedup (`play::events::frame_events`), the kick, the effects and
the sound cues. It plays 29 fire events, 29 flashes, 29 fire sounds and
29 kicks for the 29 rounds the snapshots brought, and every event id as
often as the snapshots brought it (VERIFIED,
`a_uo_autofire_capture_plays_each_shot_once`). With the ring left in UO's
numbering the same replay plays 58 fire events and 58 `EV_EJECT_BRASS`
(VERIFIED, the gate run on a revert of the mapping).

A second cause, vcod-server's alone: its playerstate went out without
`iFatigueSoundTime`, which retail's carries (VERIFIED, the sprint
capture's `wire=` holds ten stamps, 24074 to 44484, each a 1700 ms step
or more). The client's predictor rebuilds the field from every snapshot
(`predict::from_wire`), so a winded body read a stamp of 0, called the
last breath stale and raised event 140 on the first cmd it replayed, on
every snapshot; the server raised it once every 1700 ms (VERIFIED, the
loopback trace: the snapshots read `iFatigueSoundTime` 0 and a breath at
one sequence while the replay put a breath on each new one). Held fire
after a sprint then found its round at another sequence on the next
rebuild, the slot read as changed, and the round played again: its sound,
its flash and its kick inside 50 ms of the first (VERIFIED, the trace:
ring `[159, 4236, 159, 4236]` after one snapshot, `[159, 159, 4236, 4236]`
after the next). The breath also played on every snapshot (INFERRED, the
same events reach `audio::cues`). `ClientSim::to_wire` now writes the
field.

`crates/client/src/play/loopback.rs` runs vcod-server in the test process
and plays it through the client's frame: `play::clock`, the cmd ring and
its packets, the prediction, `frame_events` and the kick, beside a server
frame every 50 ms, with the mp44 held from the hip, down the sight, on the
move, after a sprint, jumping, crouched, prone and leaning. Each round the
server took off the ammo plays one fire event and one kick, at a steady
125 fps, at 4-25 ms frames with 1% hitches of 60-250 ms, and over a 60 ms
link with 40 ms of jitter (VERIFIED, `each_round_of_held_fire_plays_and_kicks_once`
on UO and 1.1). Without the field the 125 fps run plays 97 fires and 97
kicks for 76 rounds (VERIFIED, the gate on the old `to_wire`).

With 3% loss each way the jittery run played one fire more than the
server fired, at a UO install only. It was not lost cmds: it held with
`cl_packetdup` 5 too. A winded sprint writes up to five fatigue events
into the ring between snapshots, so a late snapshot's events sat 17
sequences behind the prediction. The drain recorded one of them in the
16-slot played table over a newer shot's slot, and that shot played again
when the next snapshot brought it (VERIFIED, a trace of the gate's seed
14). Retail's `CG_CheckChangedPredictableEvents` (cgame 0x30028c70)
compares only the sequences inside `(cg.eventSequence - 16,
cg.eventSequence)` (`add eax,-0x10` at 0x30028c8e) and leaves the rest
alone. VERIFIED (disassembly). `PredictedEvents::filter_snapshot` now
drops a sequence 16 or more behind without recording it. The gate runs a
lossy case (seed 14), which played 77 fires for 76 rounds before the fix,
and 13 seeds of it were exact after it (2026-10-18).

`predict_ab`'s `predictor_matches_the_uo_server_step` checks the round
trip field by field: each UO on-foot slope capture's cmds (sprint and
bipod, automatic fire, the three mp_foy mount and cook runs) go to a
protocol 22 `ClientSim` and to a predictor rebuilt from that sim's
playerstate after `ps_to_protocol` and the wire codec, at every captured
snapshot and every fifth cmd. Every field a rebuild restores agrees on
every cmd, the event ring read back through the wire's 8 bits included
(VERIFIED, 22 392 cmds, 0 mismatches). Dropping `iFatigueSoundTime` from
`to_wire` puts 3098 fatigue clock misreads and 2632 `eventSequence`
misses on the sprint run (VERIFIED, the gate on that revert).
`predictor_matches_the_1_5_server_step` runs the 1.5 moves and turns
captures the same way on protocol 6 (VERIFIED, 12 821 cmds, 0
mismatches). The six seated captures (tank and jeep seats and drive, the
jeep gun and rider runs) ride the same gate since 2026-10-18: at each
snapshot the harness puts retail's seat on the sim, its vehicle's angles
(`vang`) and its origin, since the boarding, the exit and the tag
placement are the server's (`follow_seat`). Every read agrees on all six
(VERIFIED, 33 648 cmds, 3 200 seated snapshots, 0 mismatches). Dropping
`vehPos` from `to_wire` puts 367 and 381 `delta_angles` misreads on the
jeep seats and rider runs, the passenger's arcs, and six
`weapon_pos_frac` ones on the tank seats run (VERIFIED, the gate on that
revert).

`mp_carentan-dm-slope-8ms-uostates-uo.txt` (2026-10-18,
`client-probes/probe_uostates` beside a `--save-slope --slope-every-state
--probe-moves` client) adds a death, a respawn, a spell as a spectator, a
second respawn, and a `linkTo` to a script_origin that moves 96 units.
At each snapshot the harness puts retail's `pm_type` on the sim
(`follow_state`): the death; the respawn or spectator spawn, reseeded from
retail's playerstate; the link, held at retail's origin. The client
predicts neither a dead body nor a spectator (`predict::predictable`), so
those cmds step the sim alone. Every read agrees on the rest, the linked
stretch included (VERIFIED, 6855 cmds, 220 dead, spectating or linked
snapshots, 770 cmds unpredicted, 0 mismatches). A `to_wire` that sends
the linked `pm_type` 1 as 0 puts 1285 misreads on the gate (VERIFIED,
the gate on that mutation).

`mp_carentan-dm-slope-8ms-uoturret-uo.txt` (2026-10-18,
`client-probes/probe_turret` under `probe_teleport 1` beside a
`--save-slope --slope-every-state --probe-uoturret 229` client) mounts the
misc_mg42 at (1712 1830 8) standing, sweeps the view past both arcs, fires
a burst, holds the move, jump and stance keys on the gun and lets go. What
retail's playerstate shows on the gun, each VERIFIED off the capture:
`eFlags` carries `EF_MOUNTED_STAND` (0xc000) from the snapshot after the
use tap, `viewlocked_entNum` the gun (298) and `groundEntityNum` 1023;
the origin follows the barrel's yaw, the turret pass's gunner placement
(1746.4 1861.9 at the gun's rest, 1733.3 1873.2 swung 20 degrees); the
back key sets `pm_flags` 0x40 (`PMF_BACKWARDS_RUN`), which outlives the
mount; the release brings `EV_STANCE_FORCE_STAND` and puts the body at
the mount origin raised one unit, from where it falls. The harness has no
gun, so at each snapshot it mounts and releases the sim where retail's
`eFlags` do and holds it at retail's origin while on the gun
(`follow_turret`), as `follow_seat` does a seat. Every read agrees
(VERIFIED, 2890 cmds, 450 mounted snapshots, 0 mismatches). A `from_wire`
that ignores the mounted bits puts 8076 misreads on that run, 2416 of
them `mounted` (VERIFIED, the gate on that mutation).

## 16. The lean's footstep cadence

`PM_Footsteps` (0x297e4, 1.5's 0x24ae8) on the ground picks the walk's
cadence (the walk's speed scale in the full-speed divisor and the walk's
K) from `pm_flags` 0x80 only; 1.5's also takes it on a non-zero `leanf`
(`ps+0x40`, 0x24bf7). UO's ladder branch and `PM_FootstepEvent`
(0x2937c, its walk step ids) still test the lean (`ps+0x44`). VERIFIED,
both Ghidra exports. So a leaning body on UO bobs at the run's rate for
its speed: a lean strafe at 71 ticks `bobCycle` 6 a 50 ms snapshot on
retail. VERIFIED, `mp_carentan-dm-slope-8ms-moveshot-walk-uo.txt`
(`cod11-combat.md` 2.1, "Measured moving"). vcod under 1.5's rule ticked
14 on the same cmds. VERIFIED (the gate before the fix). vcod's `pmove::footsteps`
takes the lean out of the cadence under `MoveRules::Uo` only
(`a_uo_lean_bobs_at_the_run_s_rate`).

## 17. The prone check

VERIFIED, `BG_CheckProneValid` (0x2162c) against 1.5b's (0x1cf58), read
side by side in the Ghidra export: UO adds two arguments. The sixteenth
turns on a sweep of a ±6 box from the origin 8 up, 38 units along the
facing (rodata 0x9a9b4 8.0, 0x9a9b8 -38.0 along the back vector), refusing
on any hit; only the bipod's own call passes it (`BG_CheckProne`, 0x22ff3,
passes 0). The seventeenth, a function of the hit entity, refuses a body
whose ground 10 units down (0x9a9a4) is entity type 12, a vehicle; it runs
only on the lie-down call (the ninth argument 0). VERIFIED: with the
fourteenth argument 0 the trace mask is 0x810031, 1.5b's 0x810011 plus
water; a feet or chest sample that lands in water skips the steep test,
and a chest sample more than 24 (0x9a9e0, a double) off the origin plus 6
refuses. `PM_VerifyPronePosition` (0x2e3b0) passes the fourteenth 0, so the
prone move's check sees water.

VERIFIED, rodata: the bend test after the chest sample (the legs' pitch,
feet less hips, `AngleSubtract`ed from the torso's, hips less chest) keeps
70.0 (0x9a9ec) as its top but reads -45.0 (0x9a9e8) as its bottom, where
1.1d reads -50.0 (0x701a8) and 1.5b -50.0 (0x74fb8). A UO body refuses
legs bent up more than 45 degrees past the hips; 1.1 and 1.5 allow 50.

VERIFIED, retail capture (`mp_carentan-dm-slope-8ms-patchwalk-pillar-uo.txt`,
2026-10-17): crawling forward and left along the brick wall at x 971.9 on
mp_carentan from (956.9, 588), UO refuses every cmd past y 599.77 from ct
34625 (the snapshot keeps 599.76776, velocity 0), where 1.1d and 1.5b
crawl on to 600.36 and 600.34 on the same script. VERIFIED, ours under the
same samples: between y 599.77 and 599.9 the feet sample, 51 behind the origin, lands on
top of brush 640 (z 46.125, 32 over the floor) instead of the floor, and
the legs' bend reads -49.7 at y 599.9 to -49.9 at 600.34, inside 1.5's
-50 and outside UO's -45. With UO's bound ours refuses the same cmds and
the pillar run matches all 372 snapshots (`moves_ab.rs`,
`the_predictor_replays_walks_against_patches`, no gap rows;
`pmove::tests::a_uo_body_refuses_legs_bent_up_past_45_degrees`).
