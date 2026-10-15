# Call of Duty: United Offensive 1.51 (protocol 22) against CoD 1.1 and 1.5

What vcod would have to change to join, play on and serve United Offensive
1.51, and what it may leave alone. The short answer: the transport is 1.1's
(netchan, XOR scramble, Huffman, connect compression, svc/clc framing,
snapshot shape, configstring transport), the assets load through vcod's
parsers unchanged, and everything above that moved. The netfield tables
grew and gained a per-entity table switch and a 16-bit angle encoding, the
playerState carries sprint and vehicle fields and twice the HUD elements,
the configstring blocks were rebased, and the game itself added vehicles,
sprint, deployable MGs, artillery, three gametypes and about 45 script
builtins.

Section 17 is the scope estimate and the recommended staging.

## Sources

| Binary | md5 | Role |
|---|---|---|
| `coduo_lnxded` 1.51b | `ab76f7d54388fc1ef0823b328f6fb4ec` | retail Linux server, stripped; netfield tables, codec, query and connect strings |
| `uo/game.mp.uo.i386.so` 1.51b | `a4ffc2bfc3ae7af0b2374a23db3fa8ee` | game module with dynamic symbols (`gamedate: Feb 10 2005`) |
| `CoDUOMP.exe` 1.51 | `928dd08dc169bd85fdd12d2db28def70` | client engine; the client's copies of the tables and the codec |
| `uo/uo_cgame_mp_x86.dll` 1.51 | `d83491ca20c375783a2da610e69f6431` | client game |
| `uo/uo_game_mp_x86.dll` 1.51 | `116baa157416da174b47312464a6daff` | Windows server game |
| `uo/uo_ui_mp_x86.dll` 1.51 | `f2bdbd326be38c61050de4844b86040e` | UI module |

The paks are `uo/pakuo00..07.pk3`, `uo/localized_english_pakuo00..02.pk3` and
`uo/uomappack00.pk3` of a UO install, layered over a 1.5 `main/` (section
1). The 1.1 and 1.5 counterparts are the ones `AGENTS.md` and
`cod15-protocol-diff.md` list.

Addresses in `coduo_lnxded` and `game.mp.uo.i386.so` are their `nm` / ELF
virtual addresses (Ghidra's import of the `.so` sits 0x10000 higher; Ghidra's
own labels for the `.so` are quoted as such). `CoDUOMP.exe` addresses are
virtual at 0x00400000, cgame ones at 0x30000000.

Method: `tools/re/netfields.py` (with a table address and count, as below)
and `tools/re/netfield_diff.py` on both servers and on `CoDUOMP.exe`;
Ghidra exports of `coduo_lnxded`, `CoDUOMP.exe`, `uo_cgame_mp_x86.dll` and
`game.mp.uo.i386.so`; and live traffic between the retail UO server and
`crates/common/examples/handshake_probe.rs` on loopback, captured
2026-10-13. The assets went through vcod's own loaders (bsp, xmodel, xanim,
shader, efx, weapon, menu, sound alias, gsc) from a scratch harness.

## 1. Running the retail UO server

- `COD_VERSION=uo tools/run_server.sh [map] [+set ...]` runs it. It wants
  the binary in `private/reference/coduo-lnxded-1.51b/`, a 32-bit
  `libstdc++.so.5` in that directory's `lib/` (the same Debian
  `libstdc++5` i386 copy the 1.5b server uses), a homepath holding
  `uo/game.mp.uo.i386.so` (`private/server-uo` by default) and a `COD_DIR`
  with both the 1.5 `main/` and the UO `uo/`. As with 1.5, point `COD_DIR`
  at a directory of symlinks to the stock paks only when the install
  carries third-party `zzz_*` pk3s (VERIFIED, runs on ports 30811-30813,
  maps `mp_cassino` dm and `mp_arnhem` ctf).
- The engine searches `<homepath>/uo`, the `uo/` paks, `<homepath>/main`,
  the `main/` paks, then the localized paks, newest pak first (`FS_Startup`
  log line, VERIFIED). A UO map's art is split across both directories.
- Stock pak names are hexadecimal like 1.5's: `%s/pak%x`, `%s/pakuo%x`,
  `%s/pakuo0%x`, `%s/mp_pak%x` (strings in `coduo_lnxded`, VERIFIED).
- A non-pure UO server on `mp_cassino` references `uo/pakuo07 uo/pakuo04
  uo/pakuo03 uo/pakuo01 uo/pakuo00 main/pak9 main/pak8 main/pak5 main/pak4
  main/pak0` (`sv_referencedPakNames`, configstring 1, VERIFIED). `pak8` and
  `pak9` exist only in 1.5, so a UO client needs a 1.5 base install, not a
  1.1 one.
- The master is `coduomaster.activision.com` and CD keys go to
  `coduoauthorize.activision.com` (strings, VERIFIED). Neither was queried.

## 2. Protocol number and queries

- `getinfo` and `getstatus` both report `protocol\22`; `getstatus` reports
  `shortversion\1.51` and `gamename\CoD:United Offensive` (live capture,
  VERIFIED).
- `infoResponse` keys, in order: `challenge`, `protocol`, `hostname`,
  `mapname`, `clients` (only when nonzero), `sv_maxclients`, `gametype`,
  `pure` (only when 1), `minPing` and `maxPing` (only when nonzero), `fs_game`
  (only when set), `sv_allowAnonymous` (only when nonzero), `pswrd` (only
  with `g_password`), `ff`, `kc`, `timeoutsAllowed`, `jps`, `tnk` (each only
  when its cvar is nonzero), `hw`, `pb`, `mod` (the builder at
  `coduo_lnxded` 0x809392e, VERIFIED; the order is INFERRED from the call
  order). The cvars behind the short keys are `scr_friendlyfire` (`ff`),
  `scr_killcam` (`kc`), `scr_allow_jeeps` (`jps`) and `scr_allow_tanks`
  (`tnk`), and `hw` is the constant 1 (VERIFIED, the `Cvar_VariableString`
  arguments beside each key). `pb` is `sv_punkbuster` and `mod` comes out of
  a comparison of `sv_referencedPakNames` with `fs_basegame` (INFERRED); it
  read 1 on the stock non-pure server.
- A connect whose `protocol` key is 1, 6 or 21 gets
  `error\nEXE_SERVER_IS_DIFFERENT_VER\x151.51\n` (live, VERIFIED). vcod's
  client starts at protocol 1 and switches only to a version
  `Protocol::by_short_version` knows, so today it drops with that reason
  (INFERRED from `net::other_version`).
- `challengeResponse <n>` is one integer, as on 1.1 and 1.5 (live,
  VERIFIED).

## 3. Handshake, netchan and codec

A connect with `protocol\22` built by vcod's 1.1 code (`build_connect`,
Huffman from byte 12) is accepted, and vcod's 1.1 `Netchan`, `Huffman` and
`MsgReader` read the gamestate and the snapshots after it; the client
header (`[u32 seq][u16 qport][u8 serverId][i32 messageAck][i32 reliableAck]`
and the scrambled Huffman block) brings the client from `CS_PRIMED` to
`CS_ACTIVE` (live, `developer 1` server log, VERIFIED). These are unchanged:

- connect compression, netchan fragments (1300-byte fragments of the 14
  KB gamestate), the server-to-client decode and the client-to-server XOR
  scramble (`SV_Netchan_Decode`, `coduo_lnxded` 0x809578a, is 1.1d's
  0x808de60 with the struct offsets moved, VERIFIED decompile);
- `MSG_WriteBits` LSB first one bit at a time (0x8080188), raw bytes,
  shorts and longs at `readcount` (0x80804dc, 0x8080569, 0x80805b9), the
  13-bit float selector biased by 0x1000 (VERIFIED decompile);
- svc ops 1..8 and clc ops 0..3, the gamestate body (`svc_configstring`
  index short and big string, `svc_baseline`, `svc_EOF`, then `clientNum`
  and `checksumFeed`), the snapshot header (`serverTime` long, `deltaNum`
  byte, `snapFlags` byte, no areamask), the entity stream terminated by
  1023 in 10 bits and the clientState stream gated by one bit each
  (`SV_WriteSnapshotToClient`, 0x8096673, VERIFIED decompile; the live
  parse ends each message at `svc_EOF` with only the last byte's padding
  left, VERIFIED);
- the usercmd key (`checksumFeed ^ messageAck ^
  Com_HashKey(serverCommand[reliableAck], 32)`, 0x808d8af) and
  `MAX_CONFIGSTRINGS` 2048 (`CoDUOMP.exe` 0x4014c6, `configstring >
  MAX_CONFIGSTRINGS`), VERIFIED decompile.

What changed is inside the struct delta (`MSG_WriteDeltaStruct`,
`coduo_lnxded` 0x8081b38, and its field writer 0x8081850; the readers are
`CoDUOMP.exe` 0x44b280 and its callers):

- **Field type -100, an angle.** A field with `bits` -100 carries the zero
  flag and then a raw short of `round(angle * 65536 / 360)` (0x8080815, the
  constant 182.044 at 0x80e292c, VERIFIED decompile). 1.1 sends the same
  fields as floats (bits 0 in its tables, VERIFIED). It applies to `apos.trBase[0..2]` in the main entity table, ten
  fields of the alternate table and `viewangles[0..2]` in the playerState.
- **A table-select bit.** After the 10-bit entity number the writer puts
  one bit, set when `to->eType == 12` (the callback 0x8081d54), and both
  the writer (0x8081d9a) and the client reader (`CoDUOMP.exe` 0x44b45e) pick
  the alternate table on it. The wire for an entity is therefore `[number
  10][alt 1][removed 1][changed 1][lc byte][fields]` where 1.1 has no `alt`
  bit (VERIFIED decompile and live: both mp_cassino tank baselines decode
  with `alt 1`, eType 12).
- Integer, float and zero-flag rules are 1.1's otherwise (VERIFIED
  decompile, and the live decode).

## 4. Netfield tables

`coduo_lnxded` holds the tables back to back, so `netfields.py`'s scan runs
them together; the starts and counts come from the code that pushes them
(VERIFIED disassembly):

| Table | `coduo_lnxded` | `CoDUOMP.exe` | Count | Pushed at |
|---|---|---|---|---|
| entityState | 0x080e11a0 | 0x0058ec68 | 60 | 0x8081e1a, `CoDUOMP.exe` 0x44af66 |
| entityState, eType 12 | 0x080e1480 | 0x0058ef38 | 60 | 0x8081dce, `CoDUOMP.exe` 0x44af7f |
| archived entity | 0x080e17c0 | 0x0058f208 | 68 | 0x8081eb9 |
| clientState | 0x080e1c40 | 0x0058f538 | 22 | 0x8081f39 (6-bit number, leading bit) |
| playerState | 0x080e2260 | 0x0058f640 | 114 | 0x8082876 (`cmp $0x72`), `CoDUOMP.exe` 0x44c899 |
| objective | 0x080f0360 | | 6 | 0x8082826 |
| HUD element | 0x080f03c0 | | 30 | 0x8082550 |

The server's and the client's copies are equal entry for entry (VERIFIED,
`netfield_diff.py` on each pair). Against 1.5b (`cod15-protocol-diff.md`
section 4), with `netfield_diff.py` (VERIFIED):

- **entityState** (60 against 59): `_union.scale` at offset 216 became
  `scale` at 220, a new `xmodel` (9 bits, offset 144) sits at index 29,
  `apos.trBase[0..2]` are -100 angles, `weapon` is 7 bits (6 in 1.1). The
  58 shared fields keep their relative order.
- **entityState, eType 12**: the same 60 fields in a different order (a
  vehicle's `pos.trBase[2]`, angles, `time`, `time2` and `origin2` lead),
  with `time` -16, `time2` 6, `origin2[0]` and `origin2[2]` 8 bits,
  `origin2[1]`, `angles2[0..2]`, `leanf` as -100 angles, `iHeadIcon` 6 and
  `animMovetype` 8 bits. A tank baseline carries `time2` 8 or 24,
  `origin2[0]` and `origin2[2]` 127 (live, VERIFIED); what the vehicle code
  packs into them is section 8.
- **clientState**: unchanged.
- **playerState** (114 against 103): new `iCompassTankInfo` (32, index 16),
  `sprintSpeedScale` (float, 29), `fatigueScale` (35), `lastSprintTime`
  (36), `iFatigueSoundTime` (32, 90), `vehPos` (4), `vehType` (4),
  `vehMotion` (2), `weapons[2..3]` and `weaponrechamber[2..3]` (107..113);
  `gunfx` is gone; `pm_flags` is 24 bits (1.1 19, 1.5 21); `weapon` is 7
  bits; `viewangles[0..2]` are -100 angles. The 102 shared fields keep
  their relative order. The weapon bit arrays doubled to four words, so a
  player can hold up to 128 weapon indices.
- **objective**: unchanged.
- **HUD element** (30 against 28): `SCOORD` and `TCOORD` (32 bits each)
  appended.

## 5. playerState array blocks

`MSG_WriteDeltaPlayerstate` (`coduo_lnxded` 0x8082826, VERIFIED decompile)
keeps 1.1's five blocks and their gates: the six scalars at struct offsets
0x11c..0x130 with widths short, short, short, 6 bits, short, byte; `ammo`
behind a group gate and four gated 16-short sub-blocks; `ammoclip` with no
group gate; 16 objectives, each a 3-bit state and a gated six-field delta.
The HUD block changed:

- each of the two arrays (struct offsets 0x267c, written first, and 0x7f8)
  holds up to 63 elements with a 6-bit count, 1.1 31 and 5 bits (0x8082550,
  VERIFIED);
- an element is 0x7c bytes and its last-changed index stays 5 bits over 30
  fields (VERIFIED).

The playerState struct is 0x4504 bytes, 1.5's 0x20d0 (the `memset` of the
null base, VERIFIED). A spectator's first snapshot on mp_cassino carries
`walkSpeedScale` 0.4, `runSpeedScale` 1, `sprintSpeedScale` 1.6,
`proneSpeedScale` 0.15, `crouchSpeedScale` 0.65, `strafeSpeedScale` 0.8,
`backSpeedScale` 0.7, `leanSpeedScale` 0.4, `fatigueScale` 1, `friction` 1
and `pm_flags` 0x100000 (live, VERIFIED).

## 6. usercmd

`MSG_ReadDeltaUsercmdKey` (`coduo_lnxded` 0x80814ce, VERIFIED decompile) is
1.1's two branches field for field, with one width changed: the full
branch's `weapon` is 7 bits (1.1 6). The serverTime preamble, change bit,
branch bit, the compact branch, the angle shorts, the 4-bit forward/right
code, 6 upper button bits, the `wbuttons` byte and the 2-bit up code are
1.1's (VERIFIED). The struct is still 24 bytes (VERIFIED, the `memset`).

The base cmd built from the playerstate (0x8080e16, VERIFIED decompile)
copies `weapon` and the view angles less `delta_angles` as 1.1's does
(`docs/protocol-1.1.md`, "The base cmd is built from the playerstate"). The
stance, lean and sight bits sit behind `pm_flags` 0x80000 where 1.1 tests
0x40000, both the own-view bit of their version (section 11): `eFlags` 0x40
sets `wbuttons` 0x40 and `up` -127, 0x20 sets 0x80 and -127, `leanf` sets
0x20 or 0x10, `fWeaponPosFrac` sets button 0x10 (VERIFIED decompile). New
behind the same gate, `pm_flags` 0x10000 (sprinting) sets button 0x8, the
sprint button (VERIFIED decompile), so a held sprint costs no full-branch
cmd. After each cmd the server asks game export 16 about the weapon byte
and puts `ps.weapon` back when it refuses, as 1.1 does with export 14
(0x808d8af, VERIFIED).

## 7. Configstrings

The blocks were rebased (`G_FindConfigstringIndex` and the indexers in
`game.mp.uo.i386.so`, VERIFIED decompile; first used slots from the live
mp_cassino and mp_arnhem gamestates, VERIFIED):

| Block | 1.1 | UO | UO size | Indexer (Ghidra label) |
|---|---|---|---|---|
| serverinfo, systeminfo, ... scores, northyaw | 0..11 | same slots | | |
| `setWind` | | 12 | 1 | (0x6ca27) |
| fog, level start, vote, winner | 12, 13, 15..18, 20 | 13, 14, 16..19, 21 | | `G_setfog` (0x4a59a), section 20 |
| status icons | 21..28 | 22..37 | 16 | `trap_SetConfigstring(i + 0x16)` |
| head icons | 29..43 | 38..52 | 15 | `i + 0x26` |
| `target_location` names | | 53.. | | `i + 0x35` |
| tags | 108 + i | 117 + i | 32 | `G_TagIndex` (0x75) |
| cvar mirror names / values | 140..203 / 204..267 | 149.. / 277.. | 128 each (INFERRED from the gap) | (first slots live) |
| models | 268 + i | 405 + i | 256 | `G_ModelIndex` (0x195) |
| sound aliases | 524 + i | 661 + i | 256 | `G_SoundAliasIndex` (0x295) |
| effects | 780 + i | 917 + i | 80 | `G_EffectIndex` (0x395) |
| fx on tag | 844 + i | 997 + i | 256 | `playFxOnTag` (0x3e5) |
| shell shock | 1100 + i | 1253 + i | 16 | `G_ShellShockIndex` (0x4e5) |
| script menus | 1180.. | 1333.. | 32 | `i + 0x535` |
| hint strings | 1212.. | 1365.. | 32 | `trigger_use` (`i + 0x555`) |
| localized strings | 1244 + i | 1397 + i | 256 | `G_LocalizedStringIndex` (0x575) |
| shaders | 1500 + i | 1653 + i | 256 | `G_ShaderIndex` (0x675) |
| match timeout | (1.5: 1628, 1629) | 1909, 1910 | 2 | `Cmd_MatchTimeout_f` (0x775, 0x776) |

- The mirrored cvar block holds 78 names on mp_cassino dm, among them the
  sprint and fatigue cvars (`bg_nofatigue`), `bg_lmg_yawcap`, the bob cvars,
  `cg_hudcompassMaxRange`, the team banners and colours and 36 `ui_allow_*`
  weapon and vehicle switches (live, VERIFIED).
- Configstring 7 lists 63 weapons on mp_cassino, 1.1 32 (live, VERIFIED).
  The entity, playerState and usercmd `weapon` fields grew to 7 bits, which
  fits the 128 weapon slots of section 12.
- An entity's `xmodel` field indexes models: a Sherman baseline's `xmodel`
  36 is configstring 441 `xmodel/mp_vehicle_sherman`, a Panzer IV's 38 is
  443 `xmodel/mp_vehicle_panzeriv` (live, VERIFIED).
- `serverinfo_<gametype>` is the first script menu (1333) as on 1.5, and the
  gamestate on mp_cassino is 14757 bytes decompressed with 504 configstrings
  and 52 baselines; mp_arnhem ctf 586 and 62 (live, VERIFIED).

## 8. Entity types, events, server commands

### Entity types

Read off the `CG_AddCEntity` switch (`uo_cgame_mp_x86.dll` 0x30022170), the
`eType` stores in `game.mp.uo.i386.so` and its `ET_*` debug names:

| eType | UO | 1.1 |
|---|---|---|
| 0..8 | general, player, corpse, item, missile, mover, portal, invisible, script mover | the same (VERIFIED, names and switch) |
| 9 | `ET_SOUND_BLEND`: `SP_sound_blend` (0x5a5e7) sets it, and the vehicle init (0x7c935) spawns four through `G_SpawnSoundBlend` (0x5a65d) for its engine; cgame case 9 (0x30021860) crossfades two aliases indexed from configstring 661 (VERIFIED decompile) | 1.1's cgame has a case 9 (0x3001ce40), and no 1.1 or 1.5 game module sets 9 (VERIFIED, no store found) |
| 10, 11 | FX, turret, as 1.1 (VERIFIED, `G_SpawnTurret` 0x5cc27 stores 11) | |
| 12 | `ET_VEHICLE`, from the vehicle init behind `G_SpawnVehicle` (0x8abfd) (VERIFIED); sent through the alternate netfield table (section 3) | first event |
| 13 | a destroyed vehicle: `G_FreeVehicle` (0x8e72f) stores 13 and classname `script_vehicle_corpse`; cgame draws 12 and 13 with one function, 0x30021660 (VERIFIED decompile) | |
| 14 | `script_vehicle_collmap` (0x8df52 stores 14 and clears contents, VERIFIED); cgame has no case 14 (VERIFIED); it is presumably never sent (INFERRED) | |
| 15 | `script_vehicle_owner_icon` (0xa19fd, VERIFIED); cgame case 15 is 0x30021540 (VERIFIED) | |
| 16 + n | event n (`ET_EVENTS` 16, `G_TempEntity` 0x79cba stores `event + 0x10`, VERIFIED) | 12 + n |

On mp_cassino dm the gamestate holds 30 items (3), 19 eType 8 entities and
two tanks (12); on mp_arnhem ctf 31 items, 20 eType 8 and two turrets (11)
(live baselines, VERIFIED).

The alternate table's fields are reused for the vehicle. The field offsets
are VERIFIED. Their meanings, read off the driven captures of section 18
(`snapshots-uo-tank.bin`, `snapshots-uo-jeep.bin`, the driver's snapshots
beside the gunner's own `VEH` lines) and the cgame callback of section 9:

- `angles2[0]` and `angles2[1]`: turret pitch (down positive, clamped to
  `turretVertSpanDown`: the Panzer IV's ran -1.2..15) and yaw off the hull
  (VERIFIED, capture; the driver's view yaw less the hull's, reached at
  `turretRotRate`).
- `origin2[1]` and `angles2[2]`: the gunner's yaw off the turret (a tank)
  or the hull (a jeep) and its pitch (VERIFIED, capture: the tank's ran
  -180..180 and -6.8..22.1, the jeep's -104..144 and -3.5..30).
- `origin2[0]` and `origin2[2]`: body pitch and roll, 8 bits of `127 +
  127 * deg / 30`, 127 level, 105..146 in the runs (VERIFIED, capture; the
  scale is the cgame's unpack, 0x30020574). This is a sway on top of the
  entity's own `apos`, which carries the hull's slope (VERIFIED, capture:
  the jeep's `apos` pitch reached -14.8 on a climb).
- `leanf`: the front wheels' steer, negative for a right turn (VERIFIED,
  capture: the jeep's ran -55..53 under `rightmove` 127 and -127; a tank's
  stays 0).
- `time`: a clock in ms that grows with forward travel and shrinks in
  reverse, 16 bits unsigned (VERIFIED, capture: the Panzer's rose 0..25367
  and fell back to 18639 reversing; the jeep's reached 47827). The struct
  reader takes a negative width as its plain width with no sign extension
  (`coduo_lnxded` FUN_08081f72, VERIFIED decompile), so the cgame's `time <
  0` branch (0x30021346) never fires.
- `time2`: constant per vehicle, 24 on the Panzer IV and the Willys jeep,
  8 on the Sherman, through every drive (VERIFIED, capture). It is not the
  suspension travel; the cgame reads it as `time2 / 1000` units of wheel
  reach (section 9).
- `animMovetype`: the seat mask, bit n for seat n (2 the driver, 6 driver
  and gunner); `otherEntityNum` the driver or 1023 (VERIFIED, capture).
  `weapon` the turret weapon (the Panzer's 42, the Sherman's 51; INFERRED
  from the vehicle files).
- A seated player's `surfType` packs the seat in bits 0-2, `vehType` in
  3-5 and `vehMotion` in 6-7, the same three numbers its own playerState
  carries (VERIFIED, capture: the tank gunner's 0x52, 0x92, 0xd2 idle,
  forward and reverse beside its `vehPos` 2, `vehType` 2, `vehMotion` 1-3).
  Its `otherEntityNum` is the vehicle (VERIFIED, capture).
- A tank's driver is never sent to another client; its gunner is, and both
  of a jeep's crew are (VERIFIED, capture: in 750 snapshots the tank gunner
  never saw the driver, and the driver saw the gunner in every frame after
  the climb in).

### Events

`evtab.py` runs unchanged on `uo_cgame_mp_x86.dll` (table 0x30082428, 211
entries) and `uo_game_mp_x86.dll` (0x20083428, identical); 1.1 and 1.5 have
202 (VERIFIED). Ids 0..139 are 1.1's; everything after moved (VERIFIED,
the two name tables):

| UO | 1.1 | Change |
|---|---|---|
| 140, 141 | | new `EV_FATIGUE_LAST_SOUND`, `EV_FATIGUE_SOUND` |
| 142..159 | 140..157 | `EV_STANCE_FORCE_STAND` .. `EV_WEAPON_ALT`, +2 |
| 160, 161 | | new `EV_DEPLOY_WEAPON`, `EV_BREAKDOWN_WEAPON` |
| 162..164 | 158..160 | `EV_PULLBACK_WEAPON`, `EV_FIRE_WEAPON`, `EV_FIRE_WEAPONB`, +4 |
| 165 | | new `EV_FIRE_WEAPONC` (vehicle gunner) |
| 166..177 | 161..172 | `EV_FIRE_WEAPON_LASTSHOT` .. `EV_SOUND_ALIAS`, +5 |
| 178 | 173, 174 | `EV_BULLET_HIT` replaces `_SMALL` and `_LARGE` |
| 179..181 | 175..177 | `EV_BULLET_HIT_CLIENT_SMALL/LARGE`, `EV_GRENADE_BOUNCE`, +4 |
| 182..184 | | new `EV_GRENADE_SPOON`, `EV_PROJECTILE_LAUNCH`, `EV_PROJECTILE_INCOMING` |
| 185, 186 | 178..180, 183, 184 | `EV_PROJECTILE_EXPLODE(_NOMARKS)` replace the grenade, rocket and custom explodes |
| 187..197 | 181..193 | `EV_MOLOTOV_EXPLODE` .. `EV_PLAY_FX_ON_TAG` |
| 198, 199 | | new `EV_PLAY_FX_ON_PLAYER`, `EV_STOP_ATTACHED_FX` |
| 200, 201 | 194, 195 | `EV_FLAMEBARREL_BOUNCE`, `EV_EARTHQUAKE` |
| 202 | | new `EV_REMOVE_WEAPON_ATTACHMENTS` |
| 203..207 | 196..200 | `EV_DROPWEAPON` .. `EV_PLAYER_TELEPORT_OUT` |
| 208 | | new `EV_OVERHEATING` |
| 209 | 201 | `EV_OBITUARY` |
| 210 | | new `EV_GRENADE_SUICIDE` |

Every projectile explode rides 185/186 with the kind in the weapon's
`projExplosionType` (INFERRED from the names; `G_ExplodeSmokeGrenade`
0x5eb72 raises 185 with a direction byte, VERIFIED decompile).

### Server and client commands

- `CG_ServerCommand` (`uo_cgame_mp_x86.dll` 0x3003ac90) handles 1.5's set
  (`a`..`v`, `B`..`E`) and adds `F <ms>` (`rewindFx`, cgame trap 0xf0) and
  `S <n>` (`CG_LocalSound`, sound alias configstring 661 + n, sent by
  `playLocalAnnouncerSound` 0x494d8) (VERIFIED decompile).
  `Cmd_Killcam_f` (0x4b70a) sends `C <ms>`, which UO's cgame reads as 1.5's
  `cg_autodemo` start (VERIFIED both ends; a leftover, INFERRED).
- New client commands `nextvehslot`, `prevvehslot`, `killcam`, `fdc`,
  `trace_profile`, `voice`; `vsay` and `vsay_team` are gone (VERIFIED,
  `ClientCommand`'s string compares).
- The menu handshake is 1.5's: `t 0` opens `serverinfo_<gametype>`, its
  `close` brings `t 1` (the team menu), a team brings the weapon menu, and
  `mr <serverId> <index> <response>` answers each (live, mp_cassino dm:
  `mr 16 0 close`, `t 1`, `mr 16 1 autoassign`, `t 3`, `mr 16 3 kar98k_mp`,
  then a spawn as axis with `ps.weapon` 20, configstring 7's 20th name
  `kar98k_mp`, VERIFIED).

## 9. Vehicles

Everything below is the game module's; the client neither predicts nor
simulates a vehicle. `CoDUOMP.exe` and the UO cgame carry no vehicle file
parser, physics or collmap code or strings (VERIFIED, absence). A vehicle is
therefore an interpolated snapshot entity on the client (INFERRED).

- **Definition files.** `G_ParseScrVehicleInfo` (0x8a51f) loads up to 64
  `vehicles/*` files, `VEHICLEFILE` header, under 8 KB, an info string over
  58 keys (table 0xae9c0); speeds are mph times 17.6; `type` is one of
  `4 wheel`, `tank`, `plane`, `boat`, `artillery` (VERIFIED decompile). The
  23 stock files live in `pakuo03.pk3`, nine of them MP (`elefant_mp`,
  `flak88_mp`, `gaz67b_mp`, `horch_mp`, `panzeriv_mp`, `shermantank_mp`,
  `su152_mp`, `t34_mp`, `willyjeep_mp`), re-shipped in `pakuo07.pk3`
  (VERIFIED). The server lists 39 files on a UO-over-1.5 install, those
  and `main/pak0`'s, in the search path's order (`coduo-vehicles.md` 10.1).
  The format is 1.1's `VEHICLEFILE\k\v` with new keys
  (`hasGunner`, `numPassengers`, `turretAltWeapon`, `turretGunnerWeapon`
  and its spans, `dmgScale{Front,Side,Behind,Bullet,Under}`, box
  `mins/maxs`, `stepsize`, `collision{Front,Back}Dist`, `impactSnd`, ...)
  (VERIFIED, the files).
- **Spawn.** `SP_script_vehicle` (0x8dec6) takes one of 64 pool slots of
  0x3ec bytes through `G_SpawnVehicle`; contents 0x2080, plus 0x200000 while
  usable; the vehicle takes its `script_vehicle_collmap`'s brush model and
  contents 0x800000 | 0x200000 (VERIFIED decompile, `coduo-vehicles.md`
  10.3). 246 `script_vehicle` entities sit in 10 stock MP
  maps (`panzeriv_mp` 71, `shermantank_mp` 43, `horch_mp` 33, `t34_mp` 27,
  `Flak88_MP` 22, `willyjeep_mp` 22, `gaz67b_mp` 12, `elefant_mp` 8,
  `su152_mp` 8) (VERIFIED, BSP entity lumps). `info_vehicle_node` paths are
  special-cased in `G_CallSpawn` (0x719a6) and no stock `maps/mp` script uses
  them (VERIFIED, grep).
- **Physics.** `G_VehicleClientThink` (0x8b0ce) runs a driven vehicle from
  `G_RunFrame`, after the client loop and before `ClientEndFrame`, on a fixed
  50 ms step (VERIFIED decompile). It reads the driver's last stored
  usercmd, so one input per server frame (INFERRED). Forward accelerates to
  0.8 `maxSpeed` and reverse to half that (VERIFIED); `rightmove` steers, a
  tank with `upmove > 0` turns its hull to the view yaw, then friction, a
  box slide (`VEH_SlideMove`, mask | 0x2800000), suspension off the wheel
  tags, `VEH_CheckPushClients` and collision damage (`MOD_CRUSH_JEEP`,
  `MOD_CRUSH_TANK`, `MOD_COLLISION`, notify `vehicle_collision`)
  (INFERRED, callees and constants).
- **Occupancy.** The use button (0x40) reaches `Cmd_Activate_f` on the
  cursor-hint entity; `G_IsVehicleUsable` wants the usable flag, speed at
  most 100, health, no grenade in hand and the same team; the player takes
  the first free seat 1..6 (seat 2 only with `hasGunner`, passengers up to
  `numPassengers`) and links at the seat tag (`tag_player`,
  `tag_secondary_player`, `tag_passenger*`) (VERIFIED decompile). Mounting
  sets eFlags 0x100000 and 0x200000, `viewlocked_entNum`, `vehPos` and
  `vehType`, sends `v cl_run 1` and `v cl_stance 0`, and notifies
  `activated` on the vehicle and `vehicle_activated` on the player
  (VERIFIED). `VEH_UnlinkPlayer` (0x89fb2) dismounts on a grid search
  about a point beside the vehicle, or at `tag_detach` for artillery
  (VERIFIED decompile, `coduo-vehicles.md` 11.2). `nextvehslot` / `prevvehslot`
  cycle seats (`G_VEH_CycleSlot` 0x8da1e, VERIFIED). Only the jeep's seat 3
  may fire a personal weapon (`BG_AllowPlayerWeaponAtVehiclePos`, enforced in
  `FireWeapon` 0x7bd05, VERIFIED). In a vehicle `PM_UpdateViewAngles`
  clamps pitch to 12000 short units either way (VERIFIED). `vehMotion` is 1
  idle, 2 forward, 3 reverse, and `BG_PlayerStateToEntityState` mirrors it
  into the player entity's `surfType` bits 6-7 (INFERRED).
- **Firing.** The think notifies `turret_fire`, `turret_alt_fire` (button
  0x20) and `turret_gunner_fire`, and the stock `_tankdrive_gmi`,
  `_jeepdrive_gmi` and `_flak_gmi` scripts answer with `fireTurret` and
  `fireGunner` (VERIFIED). `CMD_VEH_FireTurret` (0x8ee02) fires from
  `tag_flash` through `Bullet_Fire` or `Weapon_Artillery_Fire` (0x7b594,
  `fire_rocket` with `trType` 5) and raises event 163 (174/175 for quad
  barrels), the gunner 165 (VERIFIED). At overheat it raises 208, notifies
  `overheating` and sets `pm_flags` 0x800000 on the gunner, cleared in
  `VEH_UnlinkPlayer` (VERIFIED).
- **Damage.** `G_IsVehicleImmune` gates by damage type,
  `Scr_Vehicle_DamageScale` (0x8ccad) scales by hit direction, a tank
  driver is invulnerable, and occupants take 0.1 of radius damage (0.2 in
  artillery) (VERIFIED decompile).
- **Compass.** `G_GetNonPVSTankInfo` (0x8bf6d) packs yaw, dy, dx and a tank
  index into `iCompassTankInfo`; the cgame (0x30016c00) draws same-team
  eType 12 entities and that non-PVS tank (VERIFIED). The driver checks in
  the packer contradict each other as decompiled, so it may never report
  anything (INFERRED; wants a capture).
- **Client rendering.** `CG_AddCEntity` runs one function for `eType` 12
  and 13 (0x30021660), and its DObj pose callback is 0x30020540, reached
  through 0x30022040 for the same two types (INFERRED, disassembly). The
  first skips the model under `eFlags` 0x80, takes it from the `xmodel`
  field, sets the refEntity's `shaderTime` to `(cg.time - cent+0x1fc) /
  1000` and draws a team head icon sprite over the driver (0x300213c0)
  (INFERRED, disassembly). The callback:
  - Writes each control through 0x3001fbb0: `yaw ⊗ pitch ⊗ roll` and a
    translation into the bone's 32-byte slot of the DObj's control array
    (trap 0xa1), gated by trap 0xa2 (INFERRED, disassembly).
  - Poses `tag_body` `(pitch, 0, roll)` off `origin2[0]` and `origin2[2]`
    unpacked as `(v - 127) * 30 / 127`, `tag_turret` `(0, angles2[1], 0)`
    and `tag_barrel` `(angles2[0], 0, 0)`; on `surfType` 1 (a four-wheel)
    `tag_secondary_base` `(0, origin2[1], 0)`, on anything else
    `tag_secondary_player`, then `tag_secondary_gun` `(angles2[2], 0, 0)`,
    each lerped over the snapshot pair by `LerpAngle` (0x3004bd00)
    (INFERRED, disassembly). The tag names (0x300771a4..0x300771f8) and
    the 127.0 and 30.0 (0x3007be60, 0x3007becc) are VERIFIED.
  - For the local player in that vehicle's seat 2 (playerState `eFlags`
    0x100000, `vehPos` 2, `viewlocked_entNum` the vehicle) takes the
    gunner's yaw and pitch from its view against the vehicle's axis, the
    pitch held between entity offsets 236 and 240, instead of the wire
    (INFERRED, disassembly).
  - On a four-wheel poses `tag_steeringwheel` with `(210, 0, steer *
    0.75)` (`* 4` for the local driver when playerState offset 0xec is 0)
    and a translation `(2, 0, 0)` out of an 8-slot ring at 0x300e8b70
    (INFERRED, disassembly; 210.0, 0.75, 4.0 and 2.0 VERIFIED). The tag
    binds at yaw 180 and pitch about -32 (the Willys model, VERIFIED).
    That is pitch 210 up to a roll of 180 about the column, so a control
    replaces the bone's local rotation in its parent's frame rather than
    turning on top of it (INFERRED). Every other vehicle tag binds
    unrotated (the Panzer IV, Sherman and Willys models, VERIFIED), where
    the two readings agree.
  - Moves each of the six `tag_wheel_*` (names at 0x300851ac, VERIFIED)
    by its contact against its bind, tracing from 40 above the wheel to
    `time2 / 1000` below (mask 0x211) off the bone matrices (trap 0xb0),
    so at most `time2 / 1000` units either way, under 0.03 on the stock
    vehicles; a front wheel turns `(0, steer, 0)` when the steer is not 0
    (INFERRED, disassembly). A second trace `time2 / 1000 + 16` down feeds
    the surface effects: with the gate at 0x304582ac set, a wheel that
    moved over 32 units since its last effect (cent+0x23c, reset past 200)
    plays the effect the table at 0x3044cac0 gives its surface, +6 for a
    four-wheel (INFERRED, disassembly).
  - Stores the lerped `time` in cent+0x1fc (`time * 0.001` lerped, times
    1000.0 at 0x3007be88), which makes the model's material clock `time /
    1000` seconds (INFERRED, disassembly). The tread skins' `tread_metal`
    stage is `tcmod scroll -1 0` and the tyres' `wheel_metal` and
    `wheel_metal_masked` are `tcmod rotate -90` (`pak4.pk3`
    `shadertypes/model/`, VERIFIED). Treads and tyres therefore run with
    the vehicle's travel and stop when it stops (INFERRED).
  - A seated player (0x30032fe0) draws at the seat tag
    (`BG_GetVehiclePosTag`, cgame 0x30008190: `tag_player`,
    `tag_secondary_player`, `tag_passenger`, `tag_passenger2..4`,
    VERIFIED strings) of the posed vehicle, shifted in the tag's axes by
    0x300081e0's `(0, -24, 0)` on `surfType` 5, `(10, 5, -39)` for a
    tank's seat 2 and `(0, 0, -20)` for its seat 1 (VERIFIED constants),
    and turned with it; a tank's seat 1 is not drawn, nor the local
    player's own seats 2 and 3 of a four-wheel or 2 of a tank in first
    person (0x3003392b), and the driver's anim tree weights blend by the
    steer (INFERRED, disassembly).
  - The seat view sits at the seat tag of `viewlocked_entNum` (0x30040810)
    (INFERRED, decompile).

## 10. Gametypes

UO's `maps/mp/gametypes/` holds `bas`, `bel`, `ctf`, `dm`, `dom`, `hq`, `re`,
`sd`, `tdm` and the helpers `_gameobjects`, `_killcam_gmi`, `_loadout_gmi`,
`_rank_gmi`, `_scoring_gmi`, `_secondary_gmi`, `_shellshock_gmi`,
`_spawnlogic`, `_spawnlogic_gmi`, `_teams` (`pakuo01.pk3`, VERIFIED); `hq`
is in 1.5's paks already and `_callbacksetup.gsc` comes from `main/`
(VERIFIED). `pakuo06` and `pakuo07` override `sd.gsc`, `pakuo06` `re.gsc`
(VERIFIED). The callback signatures are 1.1's (VERIFIED).

- **Base assault (`bas`).** Each team's bases (`gmi_base`, `script_team`)
  take damage up to `scr_bas_basehealth`, switch to a damage model at half
  health and take bombs on a `trigger_lookat`, and respawns come in waves
  near the team's tanks (INFERRED from the script). The base health
  defaults to 24500, commented "700 tank shell * 35" (VERIFIED).
- **Capture the flag (`ctf`).** A flag per team (`script_model`
  `ctf_flag_<team>`, a carried `_mobile` copy) with `trigger_multiple`
  pickups and `mp_gmi_ctf_flag` spots; a carrier entering a vehicle is
  caught with `waittill("vehicle_activated", pos, vehicle)` (INFERRED from
  the script).
- **Domination (`dom`).** Numbered `flag#` capture triggers, faster with
  more cappers, held flags opening `mp_uo_spawn_*_secondary` spawns, wave
  respawns (`scr_dom_respawn_wave_time`) (INFERRED from the script).
- **Shared.** Battle rank (`_rank_gmi`: `pers["rank"]`, status and head
  icons `hud@us_rank1..5`, rank 4 gets `binoculars_artillery_mp` in the
  `binocular` slot), a scripted killcam (`_killcam_gmi`, `rewindFx`),
  shell shock, and the `satchel`, `smokegrenade` and `binocular` slots
  (INFERRED from the scripts).
- **Engine support.** None of the three new modes has engine code. They use
  1.1's `objective_*` builtins with 16 objectives (bound 0xf, VERIFIED).
  The `capturing` entity field is plain storage no engine code reads
  (INFERRED). Rank is
  script plus localized strings (`GMI_RANK_*` is in no binary, VERIFIED).
  New in the engine: the vehicle compass, `killcam` and `F`, spectator
  inactivity (`g_inactivityspectator`, `noInactivityKick`), `g_obituary`,
  `g_night` (VERIFIED, symbols).
- **Map entities.** Beside the vehicles: `script_vehicle_collmap` (46),
  `trigger_damage` (18), `mpweapon_bazooka`, `mpweapon_panzerschreck`,
  `mpweapon_flamethrower`, `mpweapon_satchelcharge`,
  `mpweapon_stielhandgranate`, `mp_uo_spawn_{allies,axis}[_secondary]`,
  `mp_gmi_bas_*`, `mp_gmi_ctf_flag`, `mp_{ctf,dom}_intermission`, and the
  keys `vehicletype`, `health`, `script_damagemodel`, `leftarc`,
  `rightarc`, `harc`, `respawn`, `script_team`, `script_timer`, `fxid`
  (VERIFIED, BSP entity lumps).

## 11. Movement

**pm_flags** (24 bits). 0x1..0x8000 keep 1.5's meanings (INFERRED); above
that UO renumbered again:

| UO | Meaning | 1.5 | Evidence |
|---|---|---|---|
| 0x10000 | sprinting | | `PM_UpdatePlayerSprintingFlag` (0x2c74a), INFERRED |
| 0x20000 | winded (fatigue under 0.5) | | `PM_UpdateFatigue` (0x2aa22), INFERRED |
| 0x40000 | following a player | 0x10000 | INFERRED |
| 0x80000 | own view, a live client in its own body | 0x20000 | the usercmd base gates on it (section 6, VERIFIED decompile); a spawned player reads 0x80000 (live, VERIFIED) |
| 0x100000 | a spectator may cycle | 0x40000 | a free spectator reads 0x100000 (live, VERIFIED) |
| 0x200000 | a spectator may leave a follow | 0x80000 | INFERRED |
| 0x400000 | `disableWeapon` | 0x100000 | the method table pairs it with 0x490e6 / 0x49189, VERIFIED |
| 0x800000 | gunner's MG overheated | | section 9, VERIFIED write and clear |

**eFlags** (24 bits). 1.5's bits from 0x100 up sit one place lower
(ADS 0x100, firing 0x200, mount mask 0x6000, vote 0x10000, talk
0x20000, ping 0x40000, friend ping 0x80000) and 0x100000..0x400000 are the
vehicle bits (INFERRED from the set and test sites). 1.5's 0x100, the
hidden model, is UO's 0x80: the per-entity runner mirrors `hide()`'s
`flags & 0x1000` into `eFlags` 0x80 (`game.mp.uo.i386.so`, Ghidra
`FUN_00069a35`), where 1.1's (0x602bc) writes 0x100 (VERIFIED decompile).
Which 1.1 bit below 0x100 UO dropped to make room is not resolved.

**Sprint and fatigue.** The usercmd's `buttons` 0x08 is sprint (`+sprint`,
`CL_CmdButtons` 0x40bfd0 in `CoDUOMP.exe`, VERIFIED). The tunables are
constants, not cvars: `pm_sprintFatigue` 1/3, `pm_fatigueRegen` 1/7,
`pm_fatigueRegenDelay` 1000 (0x9b4e8 and neighbours, VERIFIED); the only
cvars are `bg_nofatigue` and `bg_lmg_yawcap` 55, both mirrored (VERIFIED).
A sprint starts standing, unsighted, off a ladder, on the ground, moving,
outside a vehicle or turret and with fatigue above 0.25; `fatigueScale`
drains from 1 to 0 in 3 s, regenerates over 7 s after a 1 s delay, raises
event 140 under 0.5, and the speed uses `sprintSpeedScale` (1.6) in place of
`runSpeedScale`; `PM_InteruptWeaponWithSprintMove` (0x33a44) and the fire
and melee gates keep the weapon down (INFERRED, decompile). Script gets
`getFatigue` / `setFatigue` (VERIFIED). Every UO weapon file carries a
sprint pose (`sprintMove*`, `sprintRot*`, `sprintMoveMinSpeed 110`)
(VERIFIED).

**Deployable MGs.** There is no deploy flag: sighting a `weaponClass lmg`
weapon (`mg30cal_mp`, `mg34_mp`, `dp28`...) deploys it (INFERRED).
`PM_UpdateAimDownSightFlag` (0x32c0b) probes `pointcontents` with mask
0x404000 15 units ahead: a hit deploys standing, or crouched when the
contents carry 0x400000; else a `serverCursorHint` of 7 (`trigger_mount`)
gives the stance; else the player goes prone if that is valid (INFERRED,
decompile). `G_CheckForCursorHints` (0x575bb) runs the same probe on the
server to set hint 7 (VERIFIED). The contents are the new
`textures/common/hmgstanding` (0x4000) and `hmgcrouching` (0x400000)
brushes, in no 1.1 MP map (VERIFIED, BSP material lumps), and
`trigger_mount` uses the same bits (VERIFIED). Deploy goes to
`weaponstate` 12 with event 160, breakdown to 13 with 161; while deployed
the moves are zeroed, stance and lean refused and the yaw capped by
`bg_lmg_yawcap` around `proneDirection` (INFERRED). On mp_cassino an
mg34 sighted in the open deployed prone with event 160 and broke down
with 161 (live, VERIFIED, section 19 "Captures").

**Grenade cooking exists.** `PM_Weapon` (0x3726c) counts `grenadeTimeLeft`
down by the frame msec for a grenade with `cookOffHold`, raises
`EV_GRENADE_SPOON` (182) on the button edge, throws by itself under 51 ms
(event 163, plus 210 when `explosionInnerDamage` is non-zero) and
`fire_grenade` (0x5f7ad) fuses with what is left (VERIFIED decompile).
Every stock UO grenade, smoke, flash and satchel has `cookOffHold 1`
(VERIFIED). This contradicts the 1.1 rule in `AGENTS.md` ("no grenade cook
in 1.1 MP"), so any shared code needs a version gate. The cook only starts
on the sight button's edge; `coduo-movement.md` 12 has it measured.

The jump penalty, friction, ladder and slide are 1.5's apart from struct
offsets (INFERRED), so UO movement is 1.5's (`cod15-movement.md`) plus
sprint, fatigue, the bipod, the cook and the vehicle view clamp.
`coduo-movement.md` takes the sprint, the fatigue and the bipod function
by function and measures them against a capture.

## 12. Weapons and items

- 63 `weapons/mp` files against 32 in 1.1 (VERIFIED). New kit:
  `binoculars_mp`, `binoculars_artillery_mp`, `satchelcharge_mp`,
  `smokegrenade_mp`, `flashgrenade_mp`, `flamethrower_mp`, `bazooka_mp`,
  `panzerschreck_mp`, `mg30cal_mp`, `mg34_mp`, `dp28_mp`, `gewehr43_mp`,
  `svt40_mp`, `tt33_mp`, `webley_mp`, `sten_silenced_mp`, and the vehicle
  and turret weapons (VERIFIED). vcod's `weapon::load` reads all of them,
  ignoring the new keys (VERIFIED).
- Enums (the weapon file parser at 0x30263 and its tables 0xab468,
  0xab4e0, 0xab4a0, 0xab540, VERIFIED): `weaponType` adds `spotter` and
  `gas`; `weaponClass` inserts `lmg` after `smg`, which shifts `pistol` and
  everything after it, and adds `spotter`, `non-player` and
  `flamethrower`; `weaponSlot` adds `satchel` and `binocular`;
  `projExplosionType` is `grenade`, `smoke`, `rocket`, `molotov`,
  `artillery`, `mortar`, `tank`, `b17`, `none` (1.5: grenade, rocket,
  molotov, none); new `ammoType`.
- About 40 new keys (VERIFIED, the files and the parser): bipod
  (`lmgDeploy*`, `adsDeploy*`, `deploySound`, `breakdownSound`), falloff
  (`damageInnerRadius`, `damageOuterRadius`, `minDamagePercent`, used by
  `Damage_Falloff` 0x7a7f8), heat (`fireHeat`, `cooldownRate`), spread
  (`minSpread`, `maxSpread`, `adsSpreadDucked`, `adsSpreadProne`),
  barrage (`projectileCount`, `projectileRadius`, `projectileDelay`,
  `projectileSpacingMin/Max`, `projectileSoundBlend1/2`), sprint pose, and
  `attachModel`, `pickupModel`, `doNotDrop`, `cloth`, `takeDamage`,
  `turret_fov`, `vehiclescale`. 1.5's `horiz/vertViewJitter` are absent.
- `FireWeapon` (0x7bd05) dispatches bullet, grenade, rocket, spotter
  (`Weapon_ArtilleryStrike_Fire`) and gas (nothing server side) (VERIFIED).
- **Artillery.** `Weapon_ArtilleryStrike_Fire` (0x7b758) traces the view
  (mask 0x2802031), refunds the ammo on sky or no hit, else starts
  `fire_artillery_barrage`; `G_BarrageThink` (0x5e7b5) every 50 ms raises
  temp event 183, then at a random point within `projectileRadius` 184 and
  `fire_artillery` (0x5ff4a), a missile dropping straight down armed 1750
  ms later through `G_DropArtillery`, `projectileCount` (15) times
  (VERIFIED decompile). The stock spotter weapon is a rank-4 reward handed
  out by `_rank_gmi` (INFERRED).
- **Smoke and flash.** `projExplosionType smoke` routes a grenade to
  `G_ExplodeSmokeGrenade`: event 185, no damage, freed 15 s later
  (VERIFIED). `flashgrenade_mp` is the same type, so any flash effect is
  cgame-only (INFERRED).
- **Flamethrower damage is reported by the client.** The cgame's flame
  particles send `fdc <entnum>` at most every 2000 ms per index (0x300291c0),
  `Cmd_FlameDamageClient_f` (0x4ee2c) stores the inflictor and time on the
  sender, and `G_CheckFlameDamage` (0x3f4d4, from `ClientEndFrame`) applies
  the weapon's damage (four times for someone else's flame) with
  `MOD_FLAME` within 3000 ms of the last report (VERIFIED decompile). The
  server trusts the client (INFERRED).
- **Satchel** is a plain grenade (slot `satchel`, 6 s fuse, radius 450,
  inner damage 500, `projExplosionType rocket`) with no engine code of its
  own (VERIFIED).
- **Means of death** (`modNames` 0xacf20, 26 entries): `KICKED`,
  `GRABBER`, `AIRSTRIKE`, `SLIME`, `LAVA` gone, `ARTILLERY(_SPLASH)` 13/14,
  `CRUSH_TANK` 17, `CRUSH_JEEP` 18, `COLLISION` 24, `FLAME` 25 new, the rest
  renumbered (VERIFIED). The gsc callbacks see these names. The client's
  obituary (`uo_cgame_mp_x86.dll` 0x3002231e) keeps 1.1's parm: bit 7
  set, the low seven bits are the means of death, else a weapon index; its
  icon switch reads 7 melee, 8 head shot, 13 `gfx/icons/hud@artillery`, 15
  drown, 16 crush, 17 and 18 the tank and jeep crush, 20 falling, 21
  suicide, 22 `killIconDied` and 26 `gfx/icons/hud@bino_owned`, a 27th
  id whose `MOD_MELEE_BINOCULARS` string follows `MOD_FLAME` in `.rodata`
  (VERIFIED decompile and strings).
- **Items.** `bg_itemlist` (0xa9880) has 134 entries against 1.5's 70:
  weapon slots 1..128, then two `stielhandgranate` ammo items and three
  health items (VERIFIED), so `MAX_WEAPONS` is 128 and the playerState
  weapon bit arrays doubled (section 4).

## 13. Script API

- Builtin tables (`game.mp.uo.i386.so`, bounds from each lookup loop,
  VERIFIED): functions 120 (1.5 110), entity methods 56 (44), player 60
  (54), script entity 12, hudelem 14, and a new vehicle method table of 31
  (0xaec80). `Scr_GetMethod` (0x6e75f) tries player, script entity,
  vehicle, hudelem, then the generic table (VERIFIED). Nothing was removed.
- Added (VERIFIED, the tables): `precacheVehicle`, `getNumVehicles`,
  `precacheTurret`, `spawnTriggerMount`; the 31 vehicle methods (paths,
  speed, turret target and fire, `fireGunner`, heat, compass, usable,
  engine sound, dismount spot, owner); entity `doDamage`, `doDamageMod`,
  `setTakeDamage`, `setBounds`, `linkIntoWorld`, `unlinkFromWorld`,
  `spawnDuplicate`, `verifyPosition`, `getEntityNumber`,
  `clearVehiclePosition`, `getTurretHeat`, `getTurretOverheating`; player
  `getFatigue`, `setFatigue`, `isADS`, `isInVehicle`, `giveClipAmmo`,
  `playLocalAnnouncerSound`; and `playFxOnPlayer`, `rewindFx`, `setWind`,
  `exec`, `isValidPlayer`, `getFullClipAmmo`. The stock UO MP scripts call
  about 45 builtins no 1.1 table has (VERIFIED, `dump_builtins.py` on 1.1
  against the scripts' call sites).
- New engine notifies: `activated`, `deactivated`, `turret_fire`,
  `turret_alt_fire`, `turret_gunner_fire`, `overheating`,
  `vehicle_collision` on vehicles; `vehicle_activated`,
  `vehicle_deactivated`, `artillery` on players (INFERRED from the scripts'
  `waittill`s and the notifying code).
- Fields (VERIFIED): entity `vehicletype`, `capturing`, `vehicle_owner`;
  client `noInactivityKick`; a vehicle-node object (`targetname`, `target`,
  `origin`, `angles`, `speed`, `lookahead`). The field type enum was
  renumbered (`Scr_GetGenericField`).
- Spawn classnames (0xae800, 32 against 27): `script_vehicle`,
  `script_vehicle_collmap`, `script_vehicle_owner_icon`, `trigger_mount`,
  `trigger_dammage`, plus `info_vehicle_node(_rotate)` (VERIFIED).
- Language: vcod's gsc compiler parses and compiles every UO MP script
  (`every_uo_script_compiles`, VERIFIED). Two SP scripts with an empty
  first argument (`setanimknobrestart(,...)`) stay out. Stock
  `pakuo01.pk3:maps/mp/gametypes/dom.gsc:3377` writes `", "^7`, a binary
  `^` outside a string: both retail compilers take `|` and `^` at C's
  precedence, and `^` on a string is a runtime error (VERIFIED,
  `probe_bitops` on 1.1d and UO; cod11-gsc-language.md 4).

## 14. Asset formats

- **BSP.** All 37 `.bsp` entries in the UO paks are `IBSP` version 59 with
  1.1's 33 lumps and record sizes, and vcod's `bsp::parse` loads all 36
  distinct maps (19 MP, 17 SP) (VERIFIED). New in the data: content bits
  0x4000 and 0x400000 (section 11), the slick surface bit 0x2 (vcod has
  `SURF_SLICK`), and surface types 12 (ice) and 18 (sand) in MP maps
  (VERIFIED, material lumps).
- **Models and animations.** Every xmodel, xmodelparts, xmodelsurfs and
  xanim in the UO paks is version 14 and loads through vcod's loaders
  (846 xmodels, 1304 xanims; VERIFIED).
- **Shaders.** No new stage keywords. New: shader-level `nolightscale`
  (the flamethrower), `surfaceparm` `snow`, `sand`, `gravel`, `hmgstanding`,
  `hmgcrouching`, `slick`, `depthFunc less` twice and an `rgbGen` `noise`
  wave (VERIFIED, keyword diff and vcod's `ShaderLib` warnings, 207 unique
  against 1.1's 65).
- **Effects.** 768 `fx/` and 970 `lofx/` files, the latter a new
  low-detail tree; vcod's efx parser reads all 1738 (VERIFIED). New: a
  `CameraShake` block (82 uses, key `intensity`), emitter keys
  `rotationAccel`, `rotationClamp`, `velocityClamp`, flags
  `clampRotation`, `clampVelocity[XYZ]`, `continualLighting`, spawn flags
  `oppositeAccel`, `oppositeRotation` (VERIFIED).
- **Other.** Cinematics are Bink (`video/*.bik`), not RoQ; WAVs are 1.1's
  PCM16 (VERIFIED). Two `.shock` files hold shell shock presets (VERIFIED).
- **Size.** About 10,600 files and 1.85 GB unpacked over 12 paks; 19 stock
  MP maps (`arnhem`, `berlin`, `cassino`, `foy`, `italy`, `kharkov`,
  `kursk`, `ponyri`, `rhinevalley`, `sicily`, `uo_stanjel`, `peaks`,
  `streets`, `uo_carentan`, `uo_dawnville`, `uo_depot`, `uo_harbor`,
  `uo_hurtgen`, `uo_powcamp`) (VERIFIED).

## 15. UI and menus

- New script menus `quickrequests`, `quickvehicles`,
  `serverinfo_{bas,ctf,dom}`, settings menus for the new gametypes and
  `options_shoot.menu` (VERIFIED, `ui_mp/`).
- New keywords against 1.1: `disableCvar` (also 1.5), `execOnCvarIntValue`
  (vcod handles it), `onAnyKey` (VERIFIED). vcod's menu parser skips what it
  does not know, so the menus load without their behaviour.
- `menudef.h` renumbers the owner draws: `CG_PLAYER_COMPASS_TANKS` takes
  89 and pushes `CG_PLAYER_BAR_HEALTH`, `_TITLE` and `CG_DRAW_SHADER` one
  up, and the tank, jeep and vehicle ammo owner draws take 93..106
  (VERIFIED). Anything keyed on 1.1's owner draw numbers misdraws UO's
  `hud.menu`.
- The stock binds add `+sprint` (ALT), `+melee` (SHIFT), `weaponslot
  satchel|binocular|smokegrenade` and `mp_quickmap` (`default_mp.cfg`,
  VERIFIED).

## 16. Sound

- 15 of the 26 alias CSVs add `lod_min,lod_max` before `channel`, and a
  `type` value `loaded` appears (17 rows) (VERIFIED). vcod's alias parser
  keys columns by header, so both read without error (VERIFIED).
- Which alias files load is chosen per map by `soundloadspecs/mp/<map>.csv`
  (12 maps plus `default.csv`) (VERIFIED, the files; that the engine reads
  them is INFERRED from the header comment). vcod loads every
  `soundaliases/*.csv`, which under UO merges SP duplicates into MP alias
  groups (`panzerIV_idle_low` has 7 rows across files, VERIFIED).
- Shells blend `projectileSoundBlend1/2` (INFERRED from the keys). The
  cgame's missile case (0x3001edb0) picks two aliases off weaponInfo
  +0x130 and +0x134 and hands a blend off the missile's speed and closing
  rate to the same syscall 199 at 0x3001efa1 (VERIFIED, call site; the
  blend math read in outline only, not ported).
- **Lod pick.** UO's alias csvs may carry `lod_min` and `lod_max`
  (columns 16 and 17 of `CoDUOMP.exe`'s column table at 0x5c519c, stored
  at row +0x40/+0x44, both -1 when blank: the defaults 0x436770 writes)
  (VERIFIED). The cgame picks every sound through syscall 0xc4 (0x438300)
  with an origin; the engine turns it into the listener distance, -1 for
  an origin of exactly 0 (0x438300..0x43838b), and weighs only the rows
  0x4366b0 admits: any row with a negative `lod_min`, else one with
  `lod_min <= d < lod_max`, and at -1 only a `lod_min` of 0 (VERIFIED
  decompile). `weap_mg42_loop` and `weap_mg34_loop` in
  `gmi_mp_vehicles_ge.csv` split on it: the near file inside 1800 units,
  the distant one past it (VERIFIED, the csv). How the pick weighs and
  excludes repeats among the admitted rows (0x438300's counter at row
  +0x10) is read only in outline; vcod applies its 1.1 weighting to them
  (INFERRED).

### Sound blend entities

What the cgame's case 9 and syscall 199 do with an `ET_SOUND_BLEND`
entity (section 8). Addresses are `uo_cgame_mp_x86.dll`'s unless named.

- **The game side.** `G_SpawnSoundBlend` (`game.mp.uo.i386.so` 0x5a65d)
  spawns one with `otherEntityNum` 1023 and the vehicle init (0x7c935)
  ties four to the vehicle (`otherEntityNum`, `puVar2[0x1d]`): two engine
  pairs, two MG loops (VERIFIED decompile). `G_SetSoundBlend` (0x5a6a9)
  writes the two alias indexes to `eventParm` and `es.scale` and the blend
  to `angles2[1]` and links the entity, or unlinks it when both indexes
  are 0 (VERIFIED disassembly). `G_SetSoundBlendAndPitch` (0x5a708) would
  pack a pitch as `-floor(100 * pitch) - blend` (`blend` held under 0.99,
  constants 100.0, 0.99 at 0xa13e8, 0xa13ec) but writes it to offset 0xd8,
  `leanf`, which the case never reads, and no code calls it (VERIFIED
  disassembly, no call site). The vehicle think (Ghidra's 0x97480, VA 0x87480) sets the engine
  pairs from the vehicle file's alias indexes with its idle and engine
  blends, and an MG's pair is its weapon's loop alias twice at blend 0
  while its countdown (+0x2e4, +0x2f0, 50 ms a frame) lasts, then plays
  the stop alias (VERIFIED decompile).
- **Position.** `CG_CalcEntityLerpPositions` (0x30021d30) gives an
  `eType` 9 entity with `otherEntityNum` other than 1023 that entity's
  `lerpOrigin` and `lerpAngles` (`cg_entities` stride 0x288 at
  0x3048c6e0) (VERIFIED disassembly). The blend's own origin is 0 on the
  wire (live, VERIFIED, section 19 "Captures").
- **Case 9** (0x30021860). Nothing when either offset is 0, or under
  `pm_type` 5 (intermission, `cg.snap` +0x10) for a tied entity. Each
  offset plus 661 (0x295) names an alias picked through syscall 0xc4 at
  the entity's `lerpOrigin`; either missing plays nothing. The blend lerps
  `angles2[1]` between `currentState` (+0x6c) and `nextState` (+0x160) by
  `cg.frameInterpolation` (0x304831a8): plainly from 0 up, and below 0 the
  whole and fractional parts of the absolute values lerp apart, the
  fraction turned to 1 at 0.99 (0x3007bed0), and come back as `-(whole +
  fraction)`. Syscall 199 (0x3003e500) gets the two aliases, the blend,
  the entity number, the `lerpOrigin` and 0 (VERIFIED disassembly).
- **Syscall 199** (`CoDUOMP.exe` 0x452ba0, dispatched at 0x402dc8). It
  checks the pair (0x452a20: same sound file, loop flag, type, channel,
  master and slave, no random volume or pitch on a looping pair; a
  mismatch only prints a warning). A blend from 0 up plays at pitch scale
  1; below 0 the fraction of its absolute value is the blend and its whole
  part times 0.01 (0x5b9bf8) the pitch scale (VERIFIED disassembly). Then
  0x452870: on a 3D channel, nothing past the blended `dist_max`; a voice
  already playing on the entity with the same two alias names and the
  loop flag is refreshed (0x4523c0): its volume to the blend of the rows'
  `vol_min` times 0.8, its pitch to the blend of their `pitch_min` when
  both rows fix the pitch (`pitch_min == pitch_max`) and not both at 1,
  its rate to that pitch times the pitch scale, its 3D position to the
  origin, its frame stamp to this frame. Otherwise it ends the entity's
  sounds on a non-`auto` channel (0x451940) and starts the first alias's
  file at a volume drawn between the blended `vol_min` and `vol_max` times
  0.8 and a pitch drawn between the blended pitch bounds, falloff over the
  blended `dist_min..dist_max` (0x4527c0, 0x451cf0) (VERIFIED decompile).
  The end-of-frame reaper ends a looping voice no call refreshed, so a
  blend ends with its entity's last frame in the snapshot (INFERRED, the
  frame stamp and section 2 of `cod11-sound-system.md`).
- **The stock rows.** The `panzerIV_*` and `jeep_*` pairs are one file
  each at two volumes (`panzerIV_idle` 0 to 0.75, `panzerIV_engine` 0 to
  1.5, `jeep_idle` 0 to 0.8, `jeep_engine` 0.2 to 2.5) and, for the jeep's
  engine, two pitches (0.75 to 1.25); an MG loop blends an alias with
  itself (VERIFIED, `gmi_mp_vehicles_ge.csv`).

## 17. Scope estimate and staging

What vcod would need per subsystem, with a size guess (S under a day of one
agent, M one agent round, L several rounds) and the main risk.

| Subsystem | Client join and spectate | Play (client) | Server | Main risk |
|---|---|---|---|---|
| Wire (section 3-6) | protocol 22 in `net::protocol`: six tables, the alt-table bit, the -100 angle codec, 63 HUD elements of 30 fields, 128-weapon arrays; M | usercmd weapon 7 bits, sprint button; S | the writer side of the same; S | a width off by one bit desyncs silently; `handshake_probe` and a committed UO fixture make it testable |
| Ids and layouts (7, 8, 11) | per-protocol configstring layout (today `CS_*_V1` constants), EV, eType, eFlags, pm_flags remaps to vcod's 1.1 model as 1.5's `pm_flags` already is; M | the same | the same, outbound; M | a missed eFlags remap gives wrong poses with no error |
| Filesystem | `uo/` layered over a 1.5 `main/` (today `--mod-dir` mounts one directory), soundloadspecs; S-M | | | |
| Rendering and fx | vehicles from snapshots (bones from the packed fields, wheel traces, treads), vehicle corpses, owner icons, `CameraShake`, new efx keys, `lofx/`; M | seat view at the seat tag, vehicle HUD owner draws, compass tanks; M | | no captures of a moving vehicle yet |
| UI | UO menus and owner draw renumbering, `onAnyKey`, quick vehicles; M | | | |
| Movement | | sprint, fatigue, bipod, cook, vehicle pitch clamp on top of the 1.5 movement mode (`cod15-movement.md`); M-L | the same pmove server side, `trigger_mount`; M | shared pmove must stay byte-identical for 1.1 (the pmove A/B gates) |
| Weapons and combat | | spotter and artillery, smoke, flame (client-reported damage), satchel, new slots, falloff, heat; M | the same plus the barrage think, `G_CheckFlameDamage`, MOD order, 128-slot registry, per-version weapon enums; M-L | `weaponClass` shifted by `lmg`: every class test needs the version |
| Vehicles | | inputs and seat cycling only (no prediction); S | definition files, pool, physics, occupancy, turrets and heat, damage, compass, 31 methods; L (the decompile suggests 4-5k lines) | many physics constants and partly read steering; A/B gates against the local retail UO server are possible |
| Script | | | about 45 builtins and 31 vehicle methods, fields, notifies, binary `^`, the stock `_*_gmi` helpers and `bas`/`ctf`/`dom`; M-L | the stock gametypes need nearly all of it before one round runs |
| Assets | none (all formats load) | | | |

Recommended staging:

1. **Spectate on a UO server.** Protocol 22 decode and the remaps, the
   layered `uo/` + `main/` filesystem, UO's menus and HUD, vehicles and the
   new effects drawn from snapshots, the new events. This alone exercises
   every wire change, and its fidelity is measurable against the local
   retail server with `handshake_probe` captures turned into fixtures.
   Roughly one round of three or four agents.
2. **Play on a UO server.** UO pmove in prediction, built on the 1.5
   movement mode prediction already runs (`cod15-movement.md`), sprint and
   the bipod, the new weapons' client sides,
   entering and driving vehicles (server-simulated, so the client only
   sends input and draws), flame damage reports. One to two rounds.
3. **Serve UO.** The writer side of the wire, the game-side remaps, the
   script API, then vehicles, then the three new gametypes as A/B gates
   against `COD_VERSION=uo tools/run_server.sh`. The largest stage, several
   rounds; vehicles alone are bigger than the turret and mover work
   combined.

Each stage stands on its own: stage 1 is useful without 2, and 1 and 2
are the same work a 1.5 server needed, scaled up. The risks that cut across
all three are version gating in shared code (pmove, combat, the weapon
enums, the 1.1 "no cook" rule) without moving 1.1's A/B gates, and testing:
this machine runs the retail UO server but joins no public one, so
third-party UO content and live player behaviour stay unmeasured.

## 18. What vcod does

Stage 1 of section 17, the client side: vcod watches a UO server.

- **Wire.** `net::protocol::PROTOCOL_V22` is a third per-connection
  descriptor beside 1.1's and 1.5's: the tables of section 4
  (`net::fields_v22`), the vehicle table behind the select bit, the -100
  angle codec, 63 HUD elements of 30 fields, the 7-bit usercmd weapon, the
  configstring layout of section 7 (`Protocol::cs`), `ET_EVENTS` 16. A
  decoded entity keeps the main table's field order whichever table carried
  it, and an angle holds f32 degrees. The handshake picks it off the
  `EXE_SERVER_IS_DIFFERENT_VER\x151.51` refusal and sends 1.5's userinfo.
- **Evidence.** `crates/common/tests/fixtures/net/gamestate-uo.bin` and
  `snapshots-uo.bin` are a spectator's 867 messages on the retail server
  (mp_cassino dm, 2026-10-13), beside an axis probe running the combat
  script, the spectator following it for stretches. Every frame re-encodes
  byte for byte through protocol 22's tables with the flag maps raw
  (`protocol_22_capture_reencodes_and_maps`, VERIFIED): free spectator
  `pm_flags` 0x100000, a follow 0x340000 (follow, may cycle, may leave)
  with the followed player's stance bits, the tanks in the vehicle table,
  bullet impacts as temp entities of `eType` 194 (16 + 178).
- **Remaps onto 1.1's model**, at decode: events by the table of section 8
  (`event_ids::event_v22_to_v1`; `EV_BULLET_HIT` reads as the small hit,
  `EV_PROJECTILE_EXPLODE` and its no-marks twin as the grenade and the
  rocket's no-marks explode, the weapon file naming the effect either way),
  `pm_flags` by section 11's table, `eFlags` by the one-place shift above
  0x7f (section 11), means of death by `modNames` (section 12). An event
  with no 1.1 counterpart comes out as `EV_FOREIGN | id` and is logged once.
- **Data.** `--mod-dir uo` searches `uo/` over the sibling `main/`
  (`pk3::base_dirs`), as section 1's `FS_Startup` order; the stock pak test
  knows `uo/pakuo*`.
- **Drawn.** Players, corpses, items, script models and movers as on 1.1.
  `eType` 9, 14 and 15 draw nothing. The fog and the spectator header's
  timer read their slots through `Protocol::cs` (13 and 14 here; the
  retail gamestate's 13 holds the fog's `"0"` and 14 the start time,
  VERIFIED, `the_uo_timer_reads_the_rebased_start_slot`). The ammo and
  health items draw at 129..133 and a weapon pickup up to 128 selects
  (`weapon_table::static_item`, `Protocol::max_weapons`, section 12). The stock UO script menus open
  through the same front end as 1.1's, the menu block at 1333; `S <n>`
  plays a local sound like `s`.
- **Vehicles.** A vehicle (`eType` 12) and its wreck (13) draw as their
  `xmodel`, posed as section 9's callback poses them
  (`vcod_common::vehiclepose`): the body sway, turret and barrel, the
  gunner's base and gun, a jeep's steering wheel and front wheels, each a
  control replacing its tag's local rotation. The instance carries the
  callback's material clock (`DynamicModelInstance::shader_time`), so a
  stage of a lone `tcmod scroll` or `tcmod rotate` runs on `time / 1000`
  while the vehicle moves and holds still when it stops. A seated player
  draws at its seat tag of the vehicle as posed that frame, shifted and
  turned with it, a tank's driver not at all. Tags on a posed vehicle
  serve effect bolts (`EntityScene::tag_pose`).
- **Vehicle evidence.** `crates/common/tests/fixtures/net/` holds two
  driven captures, 2026-10-14, retail UO 1.51b, `tdm` through
  `client-probes/probe_vehicle`: `*-uo-tank.bin` on mp_cassino (`+set
  probe_vehicletype panzeriv_mp`), `*-uo-jeep.bin` on mp_peaks
  (`willyjeep_mp`, `+set probe_side 75 +set probe_pitch 15`). Recipe: the
  server with the gsc installed as `probe_vehicle` and those cvars, then
  `vcod --net-probe <addr> --probe-team axis --probe-vehicle driver
  --save-snapshots --capture-tag <tag> --probe-secs 40` and, 3 s later, a
  second `--probe-vehicle gunner`. The gunner waits for the driver's seat
  before its use tap and the driver for both seats before it drives, so
  they land in seats 1 and 2 whatever order they join in. Both captures
  re-encode frame for frame (`uo_vehicle_captures_reencode`, VERIFIED);
  `posed_tags_follow_the_captured_fields` poses every frame pair on the
  models' tag binds.
- **Events, effects, HUD.** Section 19.
- **Not done.** The wheel suspension (under 0.03 units) and the wheel and
  tread effects of section 9, the local gunner's view-driven gun, the
  driver's steering blend and the head icon over a driver, a shell's
  sound blend, `lofx/`, soundloadspecs, the rest of section 19's "Not done"
  and stage 3.
- **Playing (stage 2, on foot).** The client predicts under
  `MoveRules::Uo` and sends `+sprint` as `buttons` 0x8; what that covers
  and leaves out is `coduo-movement.md` 10.
- **Playing in a vehicle.** The client gets in and out with the use key,
  changes seats with `weapnext`/`weapprev`, drives, turns the turret and
  fires through the stock keys, predicts the seat's view clamps and sight,
  and puts the camera where `CG_VehicleView` does, the local gunner's view
  on its gun included: `coduo-vehicles.md`.
- **The net probe.** Every scripted mode reads through protocol 22 and
  prints 1.1's event ids and means of death (AGENTS.md, "The probe
  client"). Against the retail 1.51b server on mp_cassino and mp_carentan
  dm the combat script saw its shots (`weaponstate` 3, `EV_FIRE_WEAPON` 163
  printed as 1.1's 159), a suicide obituary's parm 149 (MOD 21) printed as
  150, and the weapon-select probe read `EV_PUTAWAY_WEAPON` and
  `EV_RAISE_WEAPON` as 1.1's 156 and 155 (live, VERIFIED). On mp_carentan
  dm the grenade script threw one frag, and its second frag switch raised
  `EV_NOAMMO` (UO 151) with the carbine left in hand (live, VERIFIED). The
  stock loadout there carries one frag (INFERRED).

## 19. Events, effects and the HUD

The UO cgame (`uo_cgame_mp_x86.dll`, base 0x30000000) read for what it
does with the events of section 8 and what its HUD draws beyond 1.1's.
Addresses are the DLL's unless named. Live rows come from
`client-probes/probe_uofx.gsc` on the retail server (recipe at the end).

### Event dispatch

`CG_EntityEvent` is 0x30022810; events 139..210 jump through the table at
0x30023564 indexed by `event - 0x8b`, whose slots point at 0x30023539
(`CG_Error("Unknown event: '%s'")`) or 0x3002355a (nothing) for the events
below marked so (VERIFIED, table bytes). 0x3002ca50 plays a sound on an
entity; 0x3002ca80 plays one at a position as `ENTITYNUM_WORLD`, the
handle in EAX and the position in ECX (VERIFIED, call sites). The sound
handles are registered in 0x3002b560 (VERIFIED, strings).

| UO id | Event | What the cgame does | Label |
|---|---|---|---|
| 140 | `EV_FATIGUE_LAST_SOUND` | `fatigue_breath` (handle 0x3044c1f4) on the entity, case 0x30022b3c | VERIFIED |
| 141 | `EV_FATIGUE_SOUND` | `sprint_breath_last` (0x3044c1f8), an alias no stock csv has; the game raises only 140 (`PM_PlayFatigueSound` 0x3a968, `PM_UpdateFatigue` 0x3aa22 in `game.mp.uo.i386.so`) | VERIFIED |
| 160, 161 | `EV_DEPLOY_WEAPON`, `EV_BREAKDOWN_WEAPON` | the weapon's `deploySound` / `breakdownSound` (weaponInfo +0x110 / +0x114, base 0x30413580 stride 0x1c4) on the entity | VERIFIED |
| 163..166, 173..175 | the fire events | `CG_FireWeapon` 0x30047d20: the weapon is the parm's low seven bits when set, else `es.weapon`; bit 7 has the client trace the bullet itself (0x30045550). On a vehicle (`eType` 12) the flash plays on `tag_altfire` for 164, `tag_secondary_flash` for 165 (the gunner), else `tag_flash`; a `gas` weapon (`weaponType` 4) returns before flash and sound | VERIFIED |
| 176 | `EV_BULLET_TRACER` | case 0x30023172: a tracer from `trBase` to `origin2` through 0x30048260, the weapon in `eventParm`, no chance roll (the server rolled `g_tracerChance` or `g_tracerChanceLMG`, `Bullet_Fire_Extended`); 1.1's cgame has no case for it and no 1.1 game raises it | VERIFIED |
| 178 | `EV_BULLET_HIT` | pre-event 0x30048e60: the normal effect at `ByteToDir(parm)` and the reflected one at `ByteToDir(es.scale)`, both from the row of the shooter's weapon's `ammoType` (weaponDef +0x90; smg 0, pistol 1, rifle 2, lmg 3, hmg 4, umg 5, enum 0x30084368); `bullet_small_<surface>` for smg and pistol rounds, `bullet_large_<surface>` for the rest (0x30048ef3); no tracer from a grenade or projectile weapon | VERIFIED |
| 181 | `EV_GRENADE_BOUNCE` | `satchel_bounce` (0x3044c200) for a weapon with `cloth` set (weaponDef +0x33c, the satchel), else `grenade_bounce_<surface>`; then the `grenade_bounce` effect row | VERIFIED |
| 182 | `EV_GRENADE_SPOON` | `us_grenade_lever` (0x3044c1fc) at the position | VERIFIED |
| 183, 184 | `EV_PROJECTILE_LAUNCH`, `_INCOMING` | parm = explosion type; only artillery (4) and mortar (5) register a sound: `shell_flash`, `barrage_incoming` | VERIFIED |
| 185, 186 | `EV_PROJECTILE_EXPLODE(_NOMARKS)` | case 0x30023355: explosion type in `es.scale` (offset 0xdc), 8 `none` skips the row; smoke (1) bolts its row's effect to the grenade's `g_body`; else the row's 24th column (`vehicle`) when `eFlags` 0x100000 (a vehicle hit, `G_MissileImpact` 0x6db67) and it has one, else the surface's; the sound table 0x3044bc30 (`type * 23 + surface`) registers grenade, rocket, artillery, mortar and tank rows only; then the weapon's `projExplosionEffect` and `projExplosionSound` | VERIFIED |
| 187, 188, 190, 198, 199 | molotov explodes, `EV_BULLET`, `EV_PLAY_FX_ON_PLAYER`, `EV_STOP_ATTACHED_FX` | `CG_Error`; no `game.mp.uo.i386.so` code raises 198 or 199, and `playFxOnPlayer` sets the client's `cg_atmos`, which nothing reads | VERIFIED |
| 201 | `EV_EARTHQUAKE` | pre-event 0x30023866: scale `angles2[0]`, radius `angles2[1]`, duration in ms as a float in `angles2[2]` (1.1: `es.time`) | VERIFIED, live: `earthquake(0.4, 1.5, origin, 600)` arrives as `[0.4, 600, 1500]` |
| 202, 210 | `EV_REMOVE_WEAPON_ATTACHMENTS`, `EV_GRENADE_SUICIDE` | nothing | VERIFIED |
| 208 | `EV_OVERHEATING` | `mg_overheat_vehicle` when the carrier's `eType & 0xc` (a turret, a vehicle) else `mg_overheat`, at the origin; raised by `turret_think` 0x6c1f2 and the vehicle MGs | VERIFIED |

- **Impact tables.** `CG_RegisterImpactEffects` 0x3001dfb0 reads every
  `fx/*.csv` in name order, a later file's row replacing an earlier one's
  (`gmi_impacts.csv`, then UO's `iw_impacts.csv`), over 22 impact types:
  six bullet classes by normal and reflect, the eight explosion types,
  `grenade_bounce` and `molotov_explode_reflect`. A surface named
  `vehicle` takes index 23 (0x3001dd30) (VERIFIED).
- **MG fire loop.** `CG_FireWeapon` sets `cent+0x204` to `fireTime` + 51
  ms when the weapon has a `loopFireSound`; 0x300490b0, called from
  `CG_AddPlayerWeapon` for player bodies off a turret and out of a
  vehicle, counts it down by the frame time, plays the loop at
  `tag_flash` while it lasts and `stopFireSound` once at the end
  (VERIFIED). A turret's loop rides `es.loopSound` from `turret_think`
  0x6bc29; a vehicle's MGs ride its sound blend entities (VERIFIED,
  `game.mp.uo.i386.so`).
- **Flamethrower.** No event: `CG_Player` (call 0x30034618) streams
  chunks while the body's `eFlags` 0x200 (firing) is up with a `gas`
  weapon, the view weapon does the same at 0x30046387 (VERIFIED). The
  chunk system 0x30024050..0x30029210 is RTCW's `cg_flamethrower.c`
  reworked: 8192 chunks, nozzle speed 1800 u/s, minimum 30, friction 3600
  u/s per s, life 833 ms, start size 16, size cap 290, growth
  `sizeMax / life / (2 (v / 1800)^3 + 0.8)` per ms, chunk spacing
  `k * 8.89` ms (0.75 own view, 0.35 others, the latter scaled by eye
  distance over 2048 clamped to 1..2.93), a hit reflecting the chunk at a
  quarter speed, sprite `flamethrowerFire{2..44}` by the share of life
  spent, smoke `flameSmoke1..6` (VERIFIED, the constants). Which UO
  function stands for which RTCW one is read off their structure
  (INFERRED). Sounds: `flamethrower_start` on a new
  stream, `flamethrower_fire` held while firing, `flamethrower_cooldown`
  on release (0x30029210, VERIFIED). On the wire a flamethrower shot is a
  plain `EV_FIRE_WEAPON` (163) with parm 0, and the shooter's `eFlags`
  carry 0x200 while it fires (live, VERIFIED). The victim's client reports
  the burn with `fdc <entnum>` (0x300291c0) and `wbuttons` 0x1 ("The flame
  report" below).
- **CameraShake.** CoDUOMP.exe parses the efx primitive as type 0xc
  (0x4a6900); at runtime (0x4a908a) it calls the cgame's vmMain 16
  (0x3002b0e0) with `intensity * 0.05` (parsed into `bounce`'s slot,
  0x4ac0c0, so the later key wins), a random `life` in ms, the effect's
  origin and a random `radius`, which starts the earthquake core 0x3001b420
  (VERIFIED). UO's core also jitters `refdef.vieworg` by up to 0.8 units
  (VERIFIED).
- **`F <ms>`.** `CG_ServerCommand` 0x3003ac90 hands it to trap 0xf0,
  CoDUOMP.exe 0x4af950, which frees every effect started in the last
  `ms`; the server's `rewindFx(player, seconds)` sends it to that player
  only (VERIFIED, live: the shooter got it, the spectator did not).

### Client-traced rounds

A vehicle MG's round (`CMD_VEH_FireTurret` 0x8ee02,
`CMD_VEH_FireAltTurret` 0x8f57e in `game.mp.uo.i386.so`) and a mounted
MG's (0x5aaa9) fire through `Bullet_Fire` and set parm bit 7 when it
returned 0 (`or 0x80` at 0x8f4a7, 0x8fc06; `mov 0x80` at 0x5abb7), which `Bullet_Fire_Extended` does when it raised
nothing: no `EV_BULLET_HIT` (a hit on a non-client is sent only when the
shooter is its own inflictor or the trace named a part) and no
`EV_BULLET_TRACER` (rolled only on a client hit or a miss under the same
condition) (VERIFIED disassembly and decompile). A vehicle's inflictor is
the vehicle and its attacker the crewman, so a round into the world comes
with the bit and one into a player or a part-named model without it
(INFERRED; live, the jeep capture's one round without the bit, at 26600,
rode with the server's own `EV_BULLET_HIT`, surface 11).

The cgame side, `uo_cgame_mp_x86.dll`:

- **Where.** `CG_FireWeapon` passes the bit to the flash (0x30047ef6),
  for a vehicle on its fire event's tag and for a turret (`eType` 11) on
  `tag_flash` (the events' tag index 0, 0x3002308d);
  the flash (0x30045c10) traces only after its effect plays (the weapon
  has a flash effect and the tag resolves), from the same tag
  (0x30045c83) (VERIFIED disassembly).
- **Once a frame.** 0x30045550 returns at once when it already traced the
  same entity at this `cg.time` (0x300dca18, 0x300dca20) (VERIFIED
  disassembly). None of the captures carries two rounds of one vehicle in
  one snapshot (live, VERIFIED).
- **The axis.** The tag's world matrix (0x3001fdf0); for `tag_altfire`
  (the coaxial, string 0x3007ac10) instead: a trace from
  `cg.refdef.vieworg` 10240 units (0x3007c3ec) along `cg.snap`'s
  `ps.viewangles` (snapshot +0xf4), then the tag's angles turned toward
  that end point by `AngleSubtract` (0x3004bd70), at most 5 degrees of
  pitch and 2 of yaw (0x3007c3e8, 0x3007bde0, 0x3007c150, 0x3007bce4),
  roll kept (VERIFIED disassembly). The server turns the gun the same way
  toward its driver's aim before its own trace (0x8ee02) (VERIFIED
  decompile).
- **The spread.** Up to eight draws: `hipSpreadStandMin + rand() / 32768
  * hipSpreadMax` degrees (weaponDef +0x298 and +0x2a4, which a turret
  file names `minSpread` and `maxSpread`: the cgame's field table gives
  both names the same offsets), then 0x3000fa20: `end = start + forward *
  10000 + r * tan(spread) * 10000` along the two other axes, `r` and the
  angle each a uniform draw (0x30049270) (VERIFIED disassembly). The stock
  vehicle MGs are `minSpread` 0.1, `maxSpread` 2 (VERIFIED, `pakuo`
  weapon files).
- **The trace.** `CG_Trace` (0x30035310: syscall 0x26, then the solid
  entities, 0x300350d0) from the tag under mask 0x2802031 skipping the
  shooter, then one under 0x10000 (skipping the shooter only for
  `tag_altfire`), which replaces the first when it is shorter and its
  entity is a vehicle (`eType` 12) (VERIFIED disassembly). A hit entity
  below 64 (a player) draws again; after eight such, nothing (VERIFIED
  disassembly).
- **A miss.** A fraction of 1 or a surface with flag 0x4 (sky): a tracer
  from the tag to the end on a roll under `cg_tracerchance` 0.4, or
  `cg_tracerchancelmg` 0.7 for an `ammoType` 3..5 weapon (vmCvars
  0x30456200, 0x30455900) (VERIFIED, the table and 0x30045a77).
- **A hit.** `CG_BulletHitEvent` (0x30048e60), the pre-event's own
  function, with the end point, the normal, the reflection `dir - 2 (dir
  . n) n`, the weapon, the surface `(surfaceFlags >> 20) & 0x1f` and the
  seat: 2 for `tag_secondary_flash`, else 1 (VERIFIED disassembly). It
  draws the ammo type's normal and reflected rows; its tracer comes off the vehicle's `tag_secondary_flash` for
  seat 2 and `tag_altfire` for seat 1, off a turret's own `tag_flash`,
  through the same roll (0x30048d60)
  (VERIFIED decompile). It also plays a sound at the point through
  0x3002ca80, the alias in EAX (VERIFIED, call; which alias is INFERRED
  to be the pre-event's surface sound).
- **The tracer cvars.** A heavy round's tracer (`ammoType` 3..5) is a
  local entity flagged 0x20 (0x300482f8), which draws with
  `cg_tracerlengthlmg` 220 and `cg_tracerwidthlmg` 1.6 instead of
  `cg_tracerlength` 160 and `cg_tracerwidth` 0.8 (0x3002ab06) (VERIFIED,
  the table and disassembly). `cg_tracernightscale` 5 widens it under a
  flag at 0x30421e4c (VERIFIED, unread further).

### The flame report

The server never traces a flame: the victim's cgame tests its own flame
chunks against its own box and reports the burn (section 21,
"Flamethrower"). Addresses are `uo_cgame_mp_x86.dll` unless named.

- **Where it runs.** `CG_AddFlameChunks` (0x30027d10) runs every frame
  from the draw (call at 0x3004262e, after the view weapon). Its
  chunk clock is `2 * cg.time` in half-ms ticks (`0x300ab718`, from the
  `fild cg.time; fadd st,st` at 0x30026904) (VERIFIED). Once a frame's
  move pass is done it clears the in-flame flag `0x300d9758` if the local
  client's check time (`centFlameInfo` row 0x300ab7bc, stride 0xb8,
  indexed by `cg.snap->ps.clientNum`) is behind the clock, draws every
  chain, then sets that check time 100 ticks (50 ms) on if it was behind,
  and hands the flag to the exe through trap 0x58 (0x30029199)
  (VERIFIED). `cg.snap` is 0x30459160, the playerstate at +0xc
  (`CG_TransitionSnapshot` stores 0x30459164 into it; `clientNum` +0xe0,
  `origin` +0x20, `mins` +0x568, `maxs` +0x574 match the UO netfield
  offsets 0xd4, 0x14, 0x55c, 0x568) (VERIFIED). So the check runs once
  per 50 ms and the flag holds in between (INFERRED, the order).
- **Which chunks.** The sprite add (0x300268e0) returns for chunk type 3
  (`+0x2c`, the smoke `0x30027ad0` spawns) and 1, and for a negative
  alpha; it tests a chunk of type 0, or type 2 with `+0x14` -1, when
  `rand() % 5 == 0` and its life fraction (`+0xe8`) is under 0.9
  (`0x3007c038`), passing the owner (`+0x38`), the size (`+0xe4`) and the
  origin (`+0xd8`, in EDI) (VERIFIED, disassembly 0x300268fa..0x3002698c).
- **The test** (0x300265c0, VERIFIED disassembly). It returns once the
  flag is up or the chunk sits at the world origin. Reach is `size * 0.4`
  (`0x3007162c`), halved (`0x3007bce8`) for the victim's own flame, and
  never under 14 (`0x3007c0a0`). It needs the local check due, and it
  skips a chunk within reach of the last chunk tested on the same tick
  (`0x300a8710`, `0x300d9760`). The nearest point is the snapshot
  origin plus `clamp(chunk - origin, mins, maxs)` per axis. The victim's
  own chunk counts only past 0.4 of its life (`0x3007c098`, a double). The
  distance to the nearest point must be under the reach. Then a point
  trace (`0x30035310`: trap 0x26, then the entity clip) runs from the
  origin's x and y at the chunk's height, clamped a unit inside the box's
  top and bottom (`0x3007bce0`), to the chunk, mask 0x2802031. It must
  end with fraction 1 or on the local client's entity. On a pass it calls
  0x300291c0 with the client in EAX and the owner in ECX and raises the
  flag.
- **`fdc`.** 0x300291c0 sends `fdc %i` with the owner through trap 0x18
  (the client command) when the client's last report (row 0x300ab7f8) is
  at least 2000 ms of `cg.time` old, and stamps it and the owner (row
  0x300ab7fc) (VERIFIED decompile).
- **`wbuttons` 0x1.** Trap 0x58 stores its argument at `0x4963774`
  (CoDUOMP.exe `CL_CgameSystemCalls` 0x401f30); `CL_FinishMove`'s button pass
  (0x40c700) ORs 0x1 into the cmd's `wbuttons` (byte +5) while it is set
  (`or byte [eax+5],1` at 0x40c89b), beside trap 0x57's shellshock 0x4
  (VERIFIED disassembly). The server burns only while this bit rides the
  victim's last cmd (section 21).
- **The flood window.** UO's `SV_ClientCommand` (`coduo_lnxded`
  0x808d629) exempts only `team `, `score ` and `mr ` (the third string,
  0x80e4c88, read out of the binary) and opens the 800 ms window for any
  other command, as 1.1 does (docs/protocol-1.1.md), so `fdc` both opens
  it and is dropped inside it (VERIFIED). The cgame does not space it
  (INFERRED, nothing in 0x300291c0 reads the window).
- **Live, retail.** With `client-probes/probe_flame` on the 1.51b
  server and the recipe below, the victim probe raised the flag on
  each of the shooter's 150 ms pulses, sent `fdc 1` at 18394 ms and again
  2086 ms later, and took 8 (`damage 2` times four) with `MOD_FLAME` on
  every server frame its cmds carried the bit: three frames a pulse, never
  while the bit was down, though the last `fdc` was inside 3 s (VERIFIED,
  2026-10-14, `PROBE damage` lines). Thirteen burns killed it.
- **vcod.** `fx::flame::FlameSystem::report` runs the check and the test
  each frame after the chunk step, with `cg.snap` as the older of the
  drawn pair. `PlayInput::in_flame` carries the flag into `wbuttons`
  (`msg::WBUTTON_IN_FLAME_V22`), and the client sends the `fdc`. Three
  differences: the trace is the world's alone, so a body between the
  chunk and the victim does not block (INFERRED small); vcod's chunks are
  `fx::flame`'s simplified ones, with no residue chunks of type 2; and
  vcod holds an `fdc` back while the flood window is open
  (`NetClient::flood_window_open`), so the report retries at a later
  check instead of being dropped. A following spectator's cgame tests the
  followed player's box and reports for itself (INFERRED, the check reads
  `cg.snap`'s `clientNum` and nothing else); vcod does the same.

Recipe (the victim's cmds face world yaw 0, as both probes send world
angles, so the gsc lays the pair out along +x):

    COD_VERSION=uo PORT=31051 PROBE_SECS=110 tools/run_probe.sh client-probes/probe_flame mp_cassino
    vcod --game-dir <UO install> --mod-dir uo --net-probe 127.0.0.1:31051 \
        --probe-team axis --probe-weapsel --capture-tag flameshooter --probe-secs 60
    vcod --game-dir <UO install> --mod-dir uo --net-probe 127.0.0.1:31051 \
        --probe-team allies --probe-flame --probe-secs 55   # 3 s after the shooter

started once the server answers (its boot takes some 15 s). The shooter's
`--probe-weapsel` fixture is discarded. The same pair on vcod-server
(`--gametype-script` the probe) burns the victim the same way, 8 with
`MOD_FLAME` a frame while the bit rides. It also hits the victim with
`MOD_PISTOL_BULLET` for 3 at each pulse, which retail never does
(section 21).

### HUD

`CG_OwnerDraw` 0x300320e0 returns at once under `cg_drawStatus 0` and
switches on the owner draw id through 0x300326e8 / 0x30032648 (VERIFIED).

- **Stance (20, 0x3002efc0).** Not drawn from a vehicle seat (`eFlags`
  0x100000 without 0x400000, popped out of the hatch). The icon is
  `hudStanceSprint` while `pm_flags` 0x10000, else prone, crouch or stand;
  over it, while `fatigueScale` > 0, `hudFatigue{Sprint,Prone,Crouch,Stand}`
  drawn on the rect's bottom `fatigueScale` share with the art's bottom
  share (st `(0, 1 - f)..(1, 1)`); then `hudStanceFlash` in white for a
  second after a prone or crouch change (UO registers
  `cg_hudStanceFlash_r/g/b` and never reads them) (VERIFIED).
- **Compass vehicles (89, 0x30016c00).** 64 slots by the vehicle pool
  index in `es.attackerEntityNum`, fed by every `eType` 12 entity whose
  driver (`otherEntityNum`) is a teammate other than the viewer, at its
  origin, its yaw (a flak gun, type 5, its `angles2[1]`) and its type in
  `es.surfType` (1 jeep, 2 tank, 5 artillery); `iCompassTankInfo` packs one
  more as `iCompassFriendInfo` is packed, with no type. A slot lingers 800
  ms; each draws 16 units square, green, turned by the view's yaw less
  its own: `hud@objective_jeep`, `hud@objective_flak_88`, else
  `hud@objective_tank` (VERIFIED). `iCompassTankInfo` is set only for
  vehicles a script `addVehicleToCompass`es, which no stock MP script does
  (INFERRED).
- **Teammates (88).** 1.1's, the ping flash 4 s (VERIFIED).
- **Vehicle status (93, 94, 95).** Drawn at the stance slot from a seat,
  keyed on the vehicle type through the item's `textfont`: the body
  outline turned by `AngleNormalize180(viewYaw - vehicleYaw)`, the barrel
  about the body's centre by the `tag_turret` yaw (pivot a quarter up the
  art, mirrored), and each seat icon whose `textstyle` bit is set in
  `es.animMovetype` (VERIFIED). 101, 102 and 104 draw nothing (VERIFIED).
- **Seat crosshair (0x30019cf0).** Two 60% black bars: (300,240,40,2) and
  (319,242,2,16) for a non-jeep's position 1, (310,240,20,2) and
  (319,242,2,8) for position 2; none for a jeep driver or the passenger
  (VERIFIED).
- **Weapon HUD (81..83, 5, 6, 70, 103, 105, 106).** Hidden from a seat but
  the jeep's passenger (`vehType` 1, `vehPos` 3), whose ammo items sit 20
  units up (VERIFIED for the rule, the rects from `hud.menu`).
- **Health block.** UO's `hud.menu` puts the back (`hud@health.tga`) at
  (501,436,128,32), the bar at (502,460,126,6) and the cross at
  (488,457,12,12) (VERIFIED, `pakuo03` `ui_mp/hud.menu`).
- **Cursor hints (0x3044b6f8).** 1.1's names with `hintLMGMount` at 7 and
  the rest one higher; weapon icons from 11, ammo icons from 139
  (VERIFIED).
- **Overheat and capture bars.** Script hudelems (`_turret_gmi.gsc`,
  `_tankdrive_gmi.gsc`, the gametypes), no cgame code (VERIFIED).
- **Binoculars.** Both binocular weapons draw the generic sight overlay
  (`adsOverlayShader`); no artillery marker or range readout exists
  (VERIFIED).

### Captures

Retail UO 1.51b, mp_cassino, 2026-10-14, each a `--probe-follow`
spectator's `--save-snapshots` (fixtures in
`crates/common/tests/fixtures/net/`, recipe below). Times are the
snapshots' server times; ids and `eFlags` are the wire's, as
`event_dump` prints them.

- **Artillery** (`*-uo-arty.bin`, dm). With `binoculars_artillery_mp` in
  the binocular slot and one round in its clip, as `_rank_gmi`'s rank 4
  gets it, sighting raises `EV_DEPLOY_WEAPON` (160) on weapon 6 and
  letting the sight go `EV_BREAKDOWN_WEAPON` (161): the binoculars ride
  the bipod's events. The trigger under the sight is
  `EV_FIRE_WEAPON_LASTSHOT` (166) (live, VERIFIED). The strike is 15
  `EV_PROJECTILE_LAUNCH` (183) temp entities (`eType` 199), parm 4, all at
  one point 280 units from the shooter, 13900..21350 ms; 13
  `EV_PROJECTILE_INCOMING` (184, `eType` 200), parm 4, each at a shell's
  coming impact 1 to 2 s before it; and 15 `EV_PROJECTILE_EXPLODE` (185)
  on the shells' own entities (`eType` 0), parm the normal, weapon 6, type
  4 in `es.scale`, the hit's `surfType`, `eFlags` 0x8000, 21550..38350 ms
  (live, VERIFIED). The shooter's obituary (209) rides the first
  explode's frame (live, VERIFIED).
- **Smoke grenade** (`*-uo-smoke.bin`, dm, `smokegrenade_mp` in its own
  slot). `EV_PULLBACK_WEAPON` (162) and `EV_FIRE_WEAPON` (163) on weapon
  52, two `EV_GRENADE_BOUNCE` (181) on the grenade with parm 17 and
  `surfType` 0, then 185 on the grenade 3.95 s after the throw with type 1,
  `eFlags` 0. The grenade stays `eType` 4, unlike a shell's or a frag's
  explode, and stays in the snapshots 15 s more (live, VERIFIED). A
  `giveWeapon` of the smoke grenade aborts the server, as an unprecached
  flamethrower does; the slot builtins do not (live, VERIFIED, two runs).
- **Bipod** (`*-uo-mg.bin`, dm, `mg34_mp`). Sighting in the open raises
  160 and takes the body prone (`eFlags` 0x40 from the next frame on);
  36 `EV_FIRE_WEAPON` (163) with `eFlags` 0x200 over the 3 s held under
  the sight, 50 to 100 ms apart, each with its `EV_BULLET_HIT` (178) on
  weapon 26; 161 when the sight goes (live, VERIFIED). A prone stance key
  held before the sight is refused every cmd with
  `EV_STANCE_FORCE_STAND` (142) on mp_cassino and mp_carentan both (live,
  VERIFIED, 741 events in one run); the sight alone deploys.
- **Vehicle fire** (`*-uo-tank.bin`, `*-uo-jeep.bin`, `*-uo-overheat.bin`,
  tdm). On the vehicle (`eType` 12): the tank's main gun 163 with parm 0
  (weapon 42 `panzeriv_turret_mp`, `es.weapon`), its coaxial MG 164 with
  parm 0x9b (bit 7 and weapon 27 `mg34_tank_mp`), its gunner 165 with
  parm 0xa0 (bit 7 and 32 `mg42_turret_mp`), the jeep's gunner 165 with
  0x82 (bit 7 and 2 `50cal_tank_mp`). Every MG round carries bit 7, and
  none brings a bullet-hit temp entity, so the client traces them all.
  The shell explodes 50 ms after its 163 on its own entity, weapon 42,
  type 6, `eFlags` 0x8000 (live, VERIFIED).
- **Overheat** (`*-uo-overheat.bin`, the tank capture's crew with the
  gunner holding its trigger 6 s). 49 rounds of 165, 100 ms apart from
  17350 ms, then `EV_OVERHEATING` (208) on the tank, parm 0, at 22150 and
  no round until the trigger's next press at 31350 (live, VERIFIED).
- **Turret** (`*-uo-turret.bin`, dm on mp_carentan, the mounted MG42
  at (1712 1830 8), weapon 30 `mg42_bipod_stand_mp`, with
  `client-probes/probe_turret`'s `probe_pose 1`, so every hit is logged
  and lands no damage). 21 `EV_FIRE_WEAPON_MG42` (173) on the gun
  (`eType` 11) with parm 0x80 from 31750 ms, 50 ms apart, into the world;
  then 127 with parm 0, one per `PROBE hit` on the target, 83 of them
  with an `EV_BULLET_TRACER` (176, temp `eType` 192) in the same frame,
  parm 30 (the weapon), `trBase` the muzzle and `origin2` the hit;
  `EV_OVERHEATING` (208) on the gun at 36750, 5 s into the fire, and
  rounds again from 39350 to 41700 (live, VERIFIED). The trigger was
  still held through the gap (INFERRED, the probe's 10 s of fire). So a
  mounted MG raises the bit as a vehicle's does (`turret_fire`,
  `game.mp.uo.i386.so` 0x5abb7: 0x80 when `Bullet_Fire` returned 0,
  VERIFIED disassembly) and cools off with the trigger down, which the
  tank's gunner did not (the overheat capture above).
- **Sound blend entities.** The tank carries `eType` 9 entities with
  `otherEntityNum` the tank, origin 0, `eventParm` and `es.scale` sound
  offsets from configstring 661 and the crossfade in `angles2[1]`: one
  from the spawn (17/18, `panzerIV_idle_low`/`_high`), a second from the
  crew's boarding on, absent for some frames (19/20,
  `panzerIV_engine_low`/`_high`), one per MG while it fires
  (71/71 `weap_mg42_loop` for the gunner, 69/69 `weap_mg34_loop` for the
  coaxial), each gone 200 ms after its last round or the overheat (live,
  VERIFIED). What the cgame and the engine do with them is section 16,
  "Sound blend entities".
- **Re-encode.** All five `probe_uofx` pairs and the three vehicle pairs
  re-encode byte for byte. The flame pair's frame 523 (the follow
  landing) sends `delta_angles[1]` 0x3555 over a base of 0x3555 and the
  smoke pair's message 150 the grenade's `eFlags` behind a set zero flag
  as 24 zero bits: the server's ints differed above the field's width
  (VERIFIED, the gates; `docs/protocol-1.1.md`, "PlayerState delta").

### What vcod does

- Events: the UO-only ids keep their numbers as `EV_UO_*` (`EV_FOREIGN |
  id`), and `EV_BULLET_HIT` and the projectile explodes join them; each
  plays as the table above (`fx::registry`, `audio::cues`). `GameEvent`
  carries the carrier's `eType` and `eFlags`. The impact table reads
  every `fx/*.csv`.
- The MG loop (`AudioSystem::step_fire_loops`), the flamethrower
  (`fx::flame`, retail's constants; no residue chunks, no chain merging,
  one smoke puff per chunk), `CameraShake` and `F` (`fx::sim`), UO's
  earthquake fields (`play::quake`).
- The HUD items above (`hud::uo`); `pm_flags` keeps sprinting, winded
  and overheated at 0x1000000, 0x2000000, 0x4000000.
- The captures above gate the dispatch (`fx::registry`'s and
  `audio::cues`' `uo_capture_gates`, `AudioSystem`'s
  `the_uo_mg_capture_holds_one_fire_loop_through_the_burst`): each
  vehicle gun's flash on its tag, the artillery, tank and smoke rows, the
  cloud on the grenade's `g_body`, the barrage, pop, bipod and overheat
  aliases, one mg34 loop through the burst, no unknown event. A loaded
  weapon file with no `worldFlashEffect` (a grenade, binoculars) flashes
  nothing, as `CG_MuzzleFlash` spawns only a set handle
  (`cod11-events-and-fx.md` 5b).
- Sound blend entities (`audio::blend`, `AudioSystem::set_sound_blends`)
  as section 16 has them, every frame off the drawn snapshot pair, the
  voice on the blend entity placed on its vehicle's drawn origin; the
  pick weighs rows by `lod` on a UO connection (`AliasTable::pick_lod`).
  `the_uo_vehicle_captures_blend_the_engines_and_the_mg_loops` replays
  the three vehicle captures: one voice per blend entity with aliases,
  on its vehicle, the engines' volume and the jeep engine's pitch the
  stock rows' blend at `angles2[1]`, gone with the entity.
- Client-traced rounds (`fx::client_bullet`, `fx::registry::client_round`)
  as above: a hit becomes the `EV_UO_BULLET_HIT` the server would have
  sent (normal and reflection as byte dirs), resolved and played as one
  with its tracer off the seat's tag; a miss draws its tracer. The trace
  is vcod's world (no static models, as `CM_BoxTrace`) and the snapshot's
  player capsules, not the second trace under 0x10000 (no client
  collision for a vehicle's collmap). Heavy rounds' tracers take the
  `*lmg` cvars' defaults. A server's `EV_BULLET_TRACER` draws unrolled.
  Registry gates check which captured rounds are traced and from which
  tags (`the_uo_vehicle_captures_trace_the_mg_rounds_the_server_left`,
  `the_uo_turret_capture_traces_its_misses_and_draws_the_served_tracers`),
  and the cues gate the turret's overheat alias.
- Not done: the stance owner
  draw's prone-blocked reset on `EF_DEAD`, the objective pointers' vehicle
  reference, UO's view jitter in the shake, `sprint_breath_last` (no
  alias), the flamethrower's residue, an explode on a vehicle
  (the `vehicle` column, not captured), the tracer's `cg_tracernightscale`
  and its gate that draws only a shot longer than the tracer (0x300482ff).

### Capture recipe

`probe_uofx.gsc` precaches the flamethrower and the artillery
binoculars, hands the first player the weapon `probe_weapon` names (none:
its own) and a second later switches it there, then raises the
earthquake and the `rewindFx`:

    COD_VERSION=uo PORT=30881 PROBE_SECS=100 tools/run_probe.sh client-probes/probe_uofx mp_cassino \
        +set probe_weapon flamethrower_mp
    vcod --game-dir <UO install> --mod-dir uo --net-probe 127.0.0.1:30881 \
        --probe-team spectator --probe-follow --save-snapshots --capture-tag uo-flame --probe-secs 50
    vcod --game-dir <UO install> --mod-dir uo --net-probe 127.0.0.1:30881 \
        --probe-team axis --probe-weapsel --capture-tag shooter --probe-secs 44

`--probe-weapsel` takes the server's `a` select and pulls the trigger
from 15 s; the shooter's own fixture is discarded. The `*-uo-flame.bin`
pair is that run (2026-10-14); the `*-uo-quake.bin` pair the same with no
`probe_weapon` and `--save-combat` for the shooter, so its own kar98k: the
earthquake and five kar98k hits. `probe_uofx` hands a binoculars weapon
over through the binocular slot with one round and the smoke grenade
through its own slot; the artillery, smoke and bipod pairs are

    COD_VERSION=uo PORT=30941 PROBE_SECS=75 tools/run_probe.sh client-probes/probe_uofx mp_cassino \
        +set probe_weapon binoculars_artillery_mp   # smokegrenade_mp, mg34_mp
    vcod --game-dir <UO install> --mod-dir uo --net-probe 127.0.0.1:30941 \
        --probe-team spectator --probe-follow --save-snapshots --capture-tag uo-arty --probe-secs 55
    vcod --game-dir <UO install> --mod-dir uo --net-probe 127.0.0.1:30941 \
        --probe-team axis --probe-uo-weapon arty --probe-secs 48   # smoke, mg

the shooter started 3 s after the spectator. `--probe-uo-weapon` takes
the `a` select, then `arty` sights 12 degrees down for 1.5 s and fires,
`smoke` holds the trigger a second and lets go 10 degrees up, `mg`
sights 3 s, fires 3 s, holds the sight 2 s and lets go. The
`*-uo-overheat.bin` pair is section 18's tank recipe with `--probe-vehicle
overheat` for the gunner. The `*-uo-turret.bin` pair is

    COD_VERSION=uo PORT=30961 PROBE_SECS=100 tools/run_probe.sh client-probes/probe_turret mp_carentan \
        +set probe_teleport 1 +set probe_pose 1
    vcod --game-dir <UO install> --mod-dir uo --net-probe 127.0.0.1:30961 \
        --probe-team spectator --probe-follow --save-snapshots --capture-tag uo-turret --probe-secs 65
    vcod ... --probe-team axis --probe-secs 60                   # 2 s later, the target
    vcod ... --save-turret --probe-turret-target-ms 9000 --capture-tag uo-turret --probe-secs 55

the gunner 3 s after the target; its own turret fixture is discarded.
`cargo run -p vcod-common --example event_dump -- 22 net/gamestate-uo-flame.bin net/snapshots-uo-flame.bin`
prints a pair's events. Without the `precacheItem`, which `dm` does not
do, the retail server dumps core once the flamethrower comes out
(VERIFIED, three runs); with it the run completes (VERIFIED). The
artillery binoculars from the hip raise and fire nothing (live, VERIFIED:
`EV_RAISE_WEAPON`, no fire event).

## 20. Server: vcod-server hosts UO (stage 1)

`vcod-server --game-dir <UO install> --mod-dir uo <map>` speaks protocol 22:
`server::install_protocol` picks it when `uo/pakuo*` paks are mounted, and
`--set protocol=22` forces it. One protocol per process, as with 1.5
(`cod15-protocol-diff.md` 13). Measured against the retail 1.51b server on
loopback (`COD_VERSION=uo tools/run_server.sh`, mp_cassino dm,
2026-10-14).

### What the server writes as UO's

- **Queries and connect.** `getinfo` is 1.5's builder with `jps` and `tnk`
  after `timeoutsAllowed`; `getstatus` is 1.5's. Both replies, and the
  refusal of a protocol 1 or 6 connect
  (`EXE_SERVER_IS_DIFFERENT_VER\x151.51`), are byte for byte retail's on
  the same paks (VERIFIED, live, `protocol_22_answers_getinfo_and_refuses_as_uo_does`).
- **Cvars.** The table is 1.5's with UO's rows on top
  (`cvars::registry::REGISTRY_V22`, `tools/capture_cvars.py` against the
  retail server, VERIFIED): the identity rows, `fs_basegame uo`, the
  vehicle and tracer cvars, `sv_night`, `sv_obituary`, `bg_nofatigue`,
  `bg_lmg_yawcap`, `cg_hudcompassMaxRange`, `scr_allow_jeeps` and
  `scr_allow_tanks` (`S`, `L`, default 1), `sv_master1
  coduomaster.activision.com`. The mirror adds `bg_lmg_yawcap`,
  `bg_nofatigue` and `cg_hudcompassMaxRange` to 1.5's set (VERIFIED, the
  gamestate's 155, 156, 165); systeminfo adds `sv_allowDownload`,
  `sv_night` and `sv_obituary` to 1.5's rows (VERIFIED, configstring 1).
- **Configstrings.** The server's table is the protocol's own layout
  (`Protocol::cs`, section 7): every allocator, the mirror, the vote, the
  match timeout and the hint strings index through it. The fixed slots
  12..21 moved up one: `setWind` has 12 (game.mp.uo 0x6ca27), `G_setfog`
  writes 13 (0x4a59a, and `G_InitGame` clears it with `"0"`, 0x586cf),
  `SP_worldspawn` the start time 14, `G_InitGame` the winner 21 (VERIFIED
  decompile). Configstring 7 is UO's 63 weapons (`defs::UO`, the
  gamestate's list, VERIFIED), configstring 8 34 hex digits for
  `bg_numItems` 134, and the five hint strings 1365..1369 are static as
  1.1's two are (VERIFIED, gamestate). From 1370 on come the spawned
  vehicles' `useHintString`s in BSP order: mp_cassino's Sherman and
  Panzer IV, mp_foy's jeep, Horch, Panzer IV, Sherman and Flak 88
  (VERIFIED, `gamestate-uo.bin` and `mp_foy-bas-uo.txt`;
  `coduo-vehicles.md` 10.2).
- **Items.** `bg_itemlist` is 128 weapon rows, then the two grenade ammo
  rows and the three health rows at 129..133 (section 12, VERIFIED);
  `RegisterItem` takes a UO weapon file's `pickupModel` between its
  `worldModel` and `projectileModel` (VERIFIED, the gamestate's 547/548),
  and its `attachModel` between `worldModel` and `pickupModel`: the
  flamethrower's `xmodel/c_ge_eqp_tank` sits at 528, between 527's
  `w_ge_prj_flamethrower_game` and 529's pickup (VERIFIED, mp_foy `bas`
  capture `mp_foy-bas-uo.txt`).
- **Means of death.** `modNames` is UO's (section 12), and the obituary
  sends `0x80 | mod` for 7, 8, 13, 15, 16, 17, 18, 20, 21 and 26, the
  weapon index otherwise (`obituary` 0x6a819, VERIFIED decompile).
- **Wire numbering.** The server builds every entity and playerstate in
  1.1's numbering and maps it once a snapshot (`game::wire::entity_to_protocol`,
  `ps_to_protocol`): `eFlags` one place down from 0x100
  (`flags::eflags_v1_to_v22`), the event ids by section 8's table read
  backwards (`event_ids::event_v1_to_v22`, both bullet hits to 178, the
  explodes to 185 and 186), and an event entity's `eType` from 16. The
  baselines go through the same map. `pm_flags` maps in the writer, as on
  1.5.
- **Playerstate.** `sprintSpeedScale` 1.6, `fatigueScale`,
  `lastSprintTime` and the sprinting and winded `pm_flags` come off the
  sim, which runs `MoveRules::Uo` for every client and bot; `gunfx`, gone
  in UO, is left out. A spawned axis kar98k player matches retail's
  capture on all 114 fields except the spawn point and the anim restart
  toggle; `viewmodelIndex` matches since the vehicles spawn
  (`playerstate_uo_ab`, VERIFIED, `fixtures/playerstate/mp_cassino-dm-uo.txt`).
- **Scripts.** UO's stock `dm`, `tdm` and `sd` compile and run. A string
  indexes by character (`probe_string_index`, 1.1d and UO, VERIFIED;
  `_gameobjects::main` needs it). The builtins they reach that vcod lacked
  are in `game::builtins::uo`: `getFullClipAmmo` (0x6782e, the weapon's
  `clipSize`), `isValidPlayer` (0x67176), `distanceSquared` (0x6b83d),
  `iPrintLnBold` (`g`, 0x66e86 and 0x489fd), `getWeaponSlotAmmo` (0x4838a),
  `getWeaponSlotClipAmmo` (0x486f3), `attackButtonPressed`, `isInVehicle`
  (always 0), `viewKick` (0x6db55: damage feedback of `force` from the
  given point), `setContents` (0x696b9), and `setTakeDamage` and
  `setBounds`, which check their arguments and change nothing (VERIFIED
  decompile for each address). `weaponSlot` gains `satchel` and
  `binocular` (strings 0x9bc21, 0x9bc29, VERIFIED).
- **Anims.** UO's player anim indices follow two rules 1.1's tree never
  exercised (`player-model-anim-system.md`, "United Offensive's array").
  The server's torso anims under the kar98k match retail's
  (`combat_uo_ab`, VERIFIED).

### Vehicles

`vcod-server` spawns, seats, drives, fires and damages UO's vehicles as
retail does: `coduo-vehicles.md` 10 to 14, which carry the addresses, the
captures and the gates.

### Measured

- `configstrings_uo_ab`: the whole table against `gamestate-uo.bin` but the
  start time (UO's level clock starts at the server's uptime: 3's `t` and
  14 read 1810050 there), configstring 8 (the vehicle weapons' bits), the
  referenced pak pair and `sv_serverid`, which retail's first level reads
  as 17 and vcod as 16 (VERIFIED, live; the extra low-nibble bump is the
  restart path's, coduo_lnxded 0x8088325, which vcod does not take at
  startup, INFERRED).
- `playerstate_uo_ab`: above.
- `combat_uo_ab`: retail's `--save-combat` capture (kar98k: single shot,
  sustained fire, reload, crouched and prone shots) replayed on a protocol
  22 server: the same five shots and the same `weaponstate`, `weapAnim` and
  `torsoAnim` values per step (VERIFIED).
- The server command stream of a join (team menu, weapon menu, the
  objective text and `cg_hudcompassMaxRange`) is retail's verbatim
  (VERIFIED, live, both probes' logs).
- `protocol_22_sprints_and_tires_on_the_wire`: a held sprint raises the
  sprint flag and drains `fatigueScale` on the wire.
- The vcod client (`--net-probe`, protocol 22) joins ours, answers
  `serverinfo_dm`, the team and weapon menus and spawns.

### Not done (stage 1)

The compass's tanks and what `coduo-vehicles.md` 14 leaves out; `bas`,
`ctf`, `dom` and `hq` (section 22 has them now); artillery, the flamethrower's `fdc` damage,
smoke and the satchel's own paths (they ride the generic missile code;
section 21 has them now);
the cooked-out frag's `EV_GRENADE_SUICIDE` death and the `trigger_mount`
cursor hint 7 (`G_CheckForCursorHints`) and `spawnTriggerMount`
(`coduo-movement.md` 11, 12); `EV_BULLET_HIT`'s reflected direction in
`es.scale` and a projectile explode's type in `es.scale` (both go out 0);
`bg_nofatigue`; battle rank's `GMI_RANK_*` path past what `dm` runs; a
retail UO client against vcod-server is a hand check.

## 21. Server: UO's weapons on vcod-server

Addresses are `game.mp.uo.i386.so` file vaddrs (Ghidra's export sits
0x10000 higher) unless named otherwise. The gate is
`crates/server/tests/uo_weapons_ab.rs`: it runs `client-probes/probe_uofx`
(and `probe_turret`) as the gametype on a protocol 22 server, places the
shooter where the retail capture's stood, drives `--probe-uo-weapon`'s step
scripts at the probe client's 60 Hz cmd cadence and compares the events and
carriers against the section 19 captures.

### Missiles

- **Bounce.** `G_BounceMissile` (0x5d6e5) reflects the velocity at the
  impact time and scales all of it by 0.4 (.rodata 0xa17a4), 0.1 (0xa17a0)
  in water or off a surface with flag 0x2000000; 1.1's keeps 0.75 along the
  surface and 0.3 off it (cod11-combat.md 12.4). The rest test (normal z
  over 0.7, speed under 20), the 0.1 nudge and the 100 units/s event gate
  are 1.1's (VERIFIED decompile and constants). `G_MissileImpact`
  (0x5db67) bounces off a player that takes damage with no event when the
  missile's `damage` is 0, and gives a `flamebarrel` classname
  `EV_FLAMEBARREL_BOUNCE` (200) in place of 181 (VERIFIED decompile).
  With the 0.4 the smoke capture's two bounces land on retail's spots and
  frames, and the pop on retail's point (VERIFIED, the gate).
- **Clipmask.** `G_RunMissile` (0x5ee7d) traces every move with the
  entity's clipmask (`+0x1a0`), which `fire_grenade` (0x5f7ad),
  `fire_rocket` (0x5fc74) and `fire_artillery` (0x5ff4a) all set to
  0x2802091 (VERIFIED decompile); 1.1's grenade falls back to 0x11
  (cod11-combat.md 12.1). The 0x2080 kerb and floor brushes stop a UO
  missile: one of the strike capture's shells reports concrete (5) off a
  0x2080 caulk brush's top side at (3229 -2877 408) (VERIFIED, the capture
  against our collision). vcod's UO missiles trace with 0x2802091.
- **Explode.** `G_ExplodeMissile` (0x5e2b5) writes `projExplosionType`
  (weapon def +0x390, the 0xab540 enum) into `es.scale` (+0xdc), keeps
  1.1's `eType` 0, `eFlags` 0x80 (1.1's 0x100) and downward-trace normal,
  centres the blast on the end of a trace 10 units up (mask 0x11) and
  ignores the missile itself; it spawns no concussion entity:
  `Concussive_fx` (0x5e221) has no caller (VERIFIED decompile). vcod sends
  the type and skips the concussion under UO. In water (`trap_PointContents`
  mask 0x20 at the origin) it traces with mask 0x20 from 500 above
  (.rodata 0xa17d0) to the downward trace's end, 16 under (0xa17cc), and
  on a fraction under 1 (0xa17d4) sets the origin to the trace's end
  before the blast's lift trace, surface 20 (VERIFIED decompile and
  constants). 1.1's has no such move (VERIFIED decompile, game.mp
  0x53d6c; cod11-combat.md 13.2). vcod ports
  it (`a_uo_explode_in_water_comes_up_to_the_surface`; no capture).
- **Smoke.** `fire_grenade` (0x5f7ad) arms `G_ExplodeSmokeGrenade`
  (0x5eb72) for type 1: 185 with the normal of a trace 16 down, type 1 in
  `es.scale`, the surface type, then `G_FreeEntity` 15000 ms on, with
  `eType`, `eFlags` and the trajectory left alone (VERIFIED decompile). The
  grenade keeps flying as a missile until the free. Ours rides
  `15000 - 50` ms of snapshots past the pop; retail's capture ends inside
  that (VERIFIED, the gate).
- **Satchel.** `satchelcharge_mp` is a type-2 (`rocket`) grenade: 185 type
  2, radius 450, 500 to 20, `MOD_GRENADE_SPLASH` (VERIFIED, the file and
  `fire_grenade`'s +0x25c 4). Its `takeDamage 1` and 6 s fuse (anything
  past 500 ms) give it a (-6,-6,0)..(6,6,6) box with contents 0x2000,
  `takedamage`, 1 health, `die` `G_GrenadeDie`, `eFlags` 0x10, the fuse
  saved at `+0x32c` and the think `G_GrenadeLeaveOwnerThink` 500 ms on,
  and links it (VERIFIED decompile).
  `G_GrenadeLeaveOwnerThink` (0x5f6b7) point-tests the origin against
  bodies (mask 0x2000000, no pass entity); while one holds it and the
  fuse is more than 100 ms off it runs again 100 ms on, otherwise it makes
  the grenade its own owner (`r.ownerNum`), relinks it and hands back the
  saved fuse with the explode (or smoke) think (VERIFIED decompile).
  Until then its thrower's traces pass it and it passes him (INFERRED,
  Q3's `SV_ClipMoveToEntities` owner skip). `G_GrenadeDie` (0x5f683) sets
  the think 100 ms on and clears `takedamage`; a grenade shot while still
  in its leave-owner phase therefore keeps its fuse (INFERRED, the think
  it leaves in place). vcod links the box in the area tree (the blasts'
  candidates; other missiles stay out of the tree), stops rounds, turret
  rounds and strikes on it (the impact surface 0), damages it from blasts,
  and runs the leave-owner think
  (`a_satchel_passes_its_thrower_until_he_steps_off`,
  `a_shot_satchel_blows_100_ms_on`,
  `a_round_stops_on_a_satchel_its_shooter_does_not_own`). The bounce
  sound is the client's (`cloth`, section 19).
- **Satchel capture** (`*-uo-satchel.bin`, mp_cassino dm, 2026-10-14,
  `probe_weapon satchelcharge_mp`, which `probe_uofx` hands over through the
  `satchel` slot with one charge, shooter on `--probe-uo-weapon smoke`):
  `EV_PULLBACK_WEAPON` (162), `EV_FIRE_WEAPON_LASTSHOT` (166) and
  `EV_NOAMMO` (151) on weapon 48, three bounces (181) at +350, +700 and
  +1000 ms with parms 5, 17, 17 and `eFlags` 0x10, then 185 at +5950 on the
  satchel as `eType` 0, type 2, surface 17, `eFlags` 0x90, which kills the
  thrower (obituary parm 48, the weapon) (live, VERIFIED). After the third
  bounce the thrower's ring carries `EV_REMOVE_WEAPON_ATTACHMENTS` (202)
  with parm 48 and a raise of weapon 0 (live, VERIFIED): UO's putaway
  (0x448ee), in the branch for a weapon gone from under the player,
  raises 202 with the weapon going down where 1.1's raises nothing
  (VERIFIED decompile). vcod's satchel bounces on the same spots, frames
  and surfaces, explodes on the same point and frame with the same type,
  flags and surface, and the thrower's ring carries the 202 with parm 48
  on the same frame (VERIFIED, the gate). Recipe: section 19's artillery lines with
  `+set probe_weapon satchelcharge_mp`, `--capture-tag uo-satchel` and
  `--probe-uo-weapon smoke`, ports 31021 (run with 75 s, spectator 55,
  shooter 48, 3 s apart).

### Artillery

- `FireWeapon` (0x7bd05) sends type 3 (`spotter`) to
  `Weapon_ArtilleryStrike_Fire` (0x7b758): `BG_Bullet_Endpos` 10000 out
  with the weapon's spread, a locational trace (mask 0x2802031); a miss or
  `SURF_SKY` gives the round back (`Add_Ammo(1)`), anything else calls
  `fire_artillery_barrage` (0x601fb) on the end point (VERIFIED decompile).
  vcod's trace stops on live bodies and shootable grenades as a round's
  does (`a_strike_on_a_body_stops_at_the_body`).
- `PM_Weapon`'s spent-`clipOnly` take (0x35ce8) skips `weaponClass` 8, so
  the binoculars stay with no `EV_NOAMMO` (VERIFIED decompile; the capture's
  breakdown after the strike is on weapon 6).
- The barrage is an `eType` 0, `SVF_NOCLIENT` entity (classname
  `rocket`) at the strike point with first think 50 ms on, the first launch
  due at +250 and the first incoming at +`projectileDelay` (6000), counted
  from the `level.time` the firing cmd ran under; it notifies its owner
  `artillery` with the point, which `_rank_gmi` waits on (VERIFIED
  decompile, `_rank_gmi.gsc` line 627).
- `G_BarrageThink` (0x5e7b5), every 50 ms: while launches are left and the
  next is due (strictly past), `EV_PROJECTILE_LAUNCH` (183) at the strike
  point, parm the explosion type, next = `(int)(0.5 * irand(spacingMin,
  spacingMax) + next)`; while incomings are left and due,
  `EV_PROJECTILE_INCOMING` (184) at the strike plus `projectileRadius` times
  one `crandom` per axis along a random yaw's forward (z unchanged), next +=
  `irand(300, 1800)`, and `fire_artillery` 1000 above it; once both counts
  reach `projectileCount` (15) it frees itself on the next think (VERIFIED
  decompile; `irand` 0x3f3a3 is `min + (max - min) * u`). Measured: first
  launch at the fire's frame + 250, first incoming + 6000 (VERIFIED, both
  sides of the gate).
- `fire_artillery` (0x5ff4a): `eType` 4, `eFlags` 0x8000, classname
  `rocket`, `SVF_NOCLIENT`, `groundEntityNum` 1022 (which keeps
  `G_RunMissile`'s fall test, 0x5ee7d, off it), `trType` 0 another 1000 up,
  `trDelta` (0, 0, -`projectileSpeed`), damage 2000, splash 300 to 10 over
  500, means of death 13 and 14 (`MOD_ARTILLERY`, `_SPLASH`), mask
  0x2802091; `G_DropArtillery` (0x5eabe), its think 1750 ms on, sets
  `trType` 5 with `trTime` one frame back, `svFlags` 0x88 and a 15 s
  `G_ExplodeMissile` fallback (VERIFIED decompile). The shell falls about
  0.2 s: each explode sits inside 1750 to 2050 ms after its incoming on the same
  spot on both sides (VERIFIED, the gate).
- On contact `G_MissileImpact`'s explode arm: the hit entity, if it takes
  damage, takes `Damage_Falloff` of 2000 (the stock file's 100
  `minDamagePercent` leaves it whole) as `MOD_ARTILLERY`; 185, or 186 off
  a player or a body part, with the face's normal; `eType` 0, `eFlags`
  unchanged (0x8000), type 4, the face's surface type (20 in water),
  0x100000 on a vehicle hit; the origin `SnapVectorTowards` the base; the
  splash from the end of a trace 4 off the face, skipping the hit entity,
  credited to the owner with the shell as inflictor (VERIFIED decompile).
  UO's `dm.gsc` (line 556) folds the splash into `MOD_ARTILLERY` for the
  obituary, so both sides send parm 141 (VERIFIED, the capture and ours).
- Surfaces: a trace reports the side it stopped on, not the brush's
  material (bsp-ibsp59-format.md, lump 3). With that and the clipmask
  above, our collision reads retail's surface at all 15 of the strike
  capture's explodes and under the satchel's three bounces and its
  explode (VERIFIED, the gate). The earlier guess that terrain wins a tie
  with a brush face was wrong: there is no terrain at those spots.

### Bullets and turrets

- `Bullet_Fire_Extended` (0x7a920) raises the world impact (178) only when
  the pass entity is the attacker, with the weapon (+0xcc) and the round's
  reflection as a byte dir in `es.scale` (+0xdc, `dir - 2 (dir.n) n`); a
  hit on a player draws `EV_BULLET_TRACER` (176) from the start with
  probability `g_tracerChanceLMG` for an `lmg`/`hmg`/`umg` `ammoType`
  (the 0xab50c enum, def +0x90), else `g_tracerChance`; the return is
  whether anything was raised (VERIFIED decompile). vcod's player rounds
  carry the weapon and reflection (the bipod gate's 36 impacts) and roll
  the tracer once a shot, for its first leg's player hit or miss; retail
  rolls once a leg (INFERRED small: only a round through a body has two).
- A turret round passes the gun (`turret_fire` 0x5aaa9 hands
  `Bullet_Fire` the gun as inflictor), so it never raises a world impact,
  and its `EV_FIRE_WEAPON_MG42` takes parm 0x80 when the round raised
  nothing (VERIFIED decompile; the capture's 21 rounds into the world
  carry 0x80 and no impact). On a player hit vcod rolls the tracer
  (the turret's `hmg` takes 0.7: the capture's 83 of 127).
- **Heat.** A round adds `fireHeat` (rec +0x40; 0.02 on
  `mg42_bipod_stand_mp`); `turret_think` (0x5c1f2), every frame on every
  gun, sets the latch (rec +0x44), raises `EV_OVERHEATING` (208), notifies
  `overheating` and writes `es.animMovetype` 1 when the heat is 1 or more,
  clears the latch once it is 0.5 or less, then takes `cooldownRate` (0.01)
  off; the fire tick (0x5bb49) and the round (0x5aaa9) both refuse a
  latched gun (VERIFIED decompile). `animMovetype` is never cleared
  (VERIFIED, the capture: 1 from the overheat to the end). The gate
  matches the capture's 100 rounds before the overheat, the overheat 5000
  ms after the first round, the first round after it and the mark
  (VERIFIED). The overheat tank capture's vehicle gunner is section 9's,
  not this path.

### Flamethrower

- `weaponType gas` does nothing in `FireWeapon` (0x7bd05: types 0 to 3
  have an arm, 4 falls through the `!= 4` error test; VERIFIED). vcod
  used to fire it as a bullet (3 damage, `MOD_PISTOL_BULLET` a pulse);
  its fire event now starts nothing (`a_flamethrower_pulse_deals_no_bullet_damage`).
  `fdc <n>`
  (`Cmd_FlameDamageClient_f` 0x4ee2c, one argument, 0 to
  `level.num_entities`) stores `n` and the time; `G_CheckFlameDamage`
  (0x3f4d4) runs in `ClientEndFrame` (0x42f59) after the cursor hints: with
  `wbuttons` bit 1 in the client's last cmd and the report inside 3000 ms,
  the reporter takes the flamethrower's `damage` (2), four times over when
  `n` is someone else, from `n` with `MOD_FLAME` (25), point `n`'s origin
  (VERIFIED decompile). The victim's cgame sends `fdc` at most every 2 s
  and raises bit 1 through trap 0x58 while its chunks reach it (section 19,
  "The flame report", VERIFIED disassembly), and vcod's client does the
  same. On the retail server a vcod victim burnt 8 a frame while the bit
  rode and not otherwise (VERIFIED, live, section 19's recipe); on
  vcod-server the same pair burns the same way. `FireWeapon` (0x7bd05)
  starts nothing for weapon type 4, `gas`, so a flamethrower pulse deals
  no damage of its own (VERIFIED decompile); vcod-server fired it as a
  bullet weapon, 3 with `MOD_PISTOL_BULLET` a pulse, until a gate pinned
  the pulse to no damage. vcod checks `n` against `MAX_GENTITIES`, not
  the live count. The burn lands after vcod's end-frame pass, so the
  victim's `P_DamageFeedback` runs again right after it, putting
  `damageEvent` and the pain on the burn's own snapshot as retail's
  `ClientEndFrame` order does (VERIFIED decompile, 0x42f59; the gate pins
  ours).
- `eFlags` 0x200 (1.1's 0x400) rides every fire while the trigger is held
  (the flame capture), which vcod's sim already sets for any held trigger.

### Not done

A live capture of a satchel shot off (the decompile and unit tests only);
a satchel shot in the 300 ms its explode rides the wire (retail's
`takedamage` survives the explode, so a second `G_GrenadeDie` there would
explode it again; INFERRED, not ported); melee against a satchel; the
other missiles' links in the area tree, which retail makes every frame;
a live run of a retail UO client burning on vcod-server; the
flamethrower's stray bullets on vcod-server. The bipod deploy's entity
bit is not a gap: `BG_PlayerStateToEntityState` (game.mp.uo 0x2cd8b) ORs
0x100 (1.1's 0x200) into the entity's `eFlags` whenever `pm_flags` 0x20,
the sight flag the deploy rides on, is set, as 1.1's does with 0x200
(VERIFIED decompile, both modules), and vcod's entity carries it
(`a_uo_deploy_frame_carries_the_sight_bit_on_the_entity`); a ps event's
carrier reads `ps.eFlags`, which never has it.

## 22. Server: UO's gametypes on vcod-server

`bas`, `ctf`, `dom` and `hq` run on vcod-server under protocol 22 with
UO's stock scripts. Addresses are `game.mp.uo.i386.so` file vaddrs
(Ghidra's export sits 0x10000 higher) unless named otherwise. Two gates,
both with `COD_DIR` at a UO install:

- `configstrings_uo_ab`'s `the_configstring_tables_match_uo_s_gametypes`
  diffs the table after the load and 20 frames against
  `tests/fixtures/configstrings/<map>-<gametype>-uo.txt`
  (`--save-configstrings --capture-tag uo` beside `COD_VERSION=uo
  tools/run_server.sh <map> +set g_gametype <gt>`; mp_cassino for `ctf`,
  `dom`, `hq`, mp_foy for `bas`, the first stock maps whose arena lists
  them). The model, alias and effect blocks are retail's minus the vehicles'
  entries in retail's order; the shader and hint string blocks match once
  the vehicles' (`GMI_VEHICLE_*`, `fire_ready_shell`) are set aside. All
  four match (VERIFIED, the gate).
- `gametypes_ab`'s `uo_*` tests run `client-probes/probe_<gt>` as the
  gametype with two `--save-scripted` probes, allies then axis, and
  compare every view (commands, sounds, HUD, objectives, scoreboard, the
  roster, objective models and the probe's log) against
  `tests/fixtures/gametypes/<map>-<gt>-<role>-uo.txt` (2026-10-14, ports
  31013-31015). Each probe moves every spawn onto a fixed spot per team
  (each team's first spawn point), so what one probe sees of the other is
  not the spawn script's random pick. A UO run drops objective entity
  numbers and model entity numbers, which the vehicles vcod does not spawn
  shift by about 40.

### What each probe drives

- **ctf** (mp_cassino, 75 s). The first level ends in ctf's own match
  start: once both teams hold a weapon, `CheckMatchStart` calls
  `endRound("reset")`, a 15 s stopwatch (`setClock`) and
  `map_restart(true)` (INFERRED from the script; the restart itself
  VERIFIED, the capture's `n`). On the second level the allied player is
  set on `level.axis_flag.trigger`, 8 units up, and picks the flag up 50 ms
  later with the carrier status icon `hud@ctf_german.dds`; set into the
  axis flag's `goal` trigger, he captures 50 ms later: allies 1, his score
  13, the rank 2 icon a frame after (VERIFIED, the retail log). The gate
  matches every view (VERIFIED).
- **dom** (mp_cassino, 75 s). The same restart. The allied player stood on
  `flag5_neutral` (trigger `*3`, `script_timer` 3) owns the flag 3.05 s
  later, allies 1, his score 3; the axis player on the same spot retakes it
  3.1 s later (VERIFIED, the retail log). The compass marker on the flag
  reads `objective_yellow` with the flag model's entity while capping, then
  the owner's colour (VERIFIED, the captures). The gate matches every view
  (VERIFIED).
- **hq** (mp_cassino, 110 s). No rounds and no restart (INFERRED from the
  script). Once both teams are in, one of the map's seven `hqradio` script
  models (the BSP's own; mp_cassino's level script spawns none, VERIFIED,
  the entity lump and the script) shows at a random pick 45 s into the
  level; the allied player on it captures it 10 s later and the axis
  player neutralizes it 8 s after he steps on, `HQ_SCORED_AXIS` 36
  (VERIFIED, the retail log and captures). The radio is a random pick on
  both servers, so the gate drops objective origins and compares the radio
  models as a set; everything else matches (VERIFIED).
- **bas** (mp_foy, 80 s). The same restart as ctf. The probe's
  `doDamage(30000, origin + (0,0,64), allied, allied,
  "MOD_GRENADE_SPLASH")` on the axis `hq` base (`gmi_base`, 24500 health)
  breaches it 50 ms later: health -5500, status `destroyed`, the
  attacker's score 8 (VERIFIED, the retail log). No tank is needed: the
  base takes any player's `MOD_GRENADE*`, `MOD_PROJECTILE*` or
  `MOD_ARTILLERY*` (INFERRED from bas.gsc 3954). Two views are gaps
  (`gametypes_ab::GAPS`, "Gaps" below).

### Engine facts the gametypes reach

- **Model entities send `xmodel`.** UO's `G_DObjUpdate` (0x77df9) writes
  the model index into `s.xmodel` (entity +0x90) for `eType` 0, 8, 11, 12,
  13 and 14; 1.1's (0x66054) writes `s.index` (+0x8c) for 0, 8 and 11
  (VERIFIED decompile). Retail UO's script models carry `index` 0 and the
  model in `xmodel` (VERIFIED, every `ctf`/`hq` capture). vcod's server
  moves a non-brush script mover's or turret's model there under protocol
  22 (`wire::entity_to_protocol`), and the client and the scripted capture
  read it there; a brush model keeps its inline number in `index`.
- **`setShader` takes texture coordinates.** UO's (0x52779) takes 1, 3 or
  5 arguments, the last two floats stored at hudelem +0x70/+0x74 (1.0 when
  absent, .rodata 0x9ff3c), which go out as `SCOORD`/`TCOORD`; owner and
  team moved to +0x7c/+0x80 (`HudElem_Alloc` 0x51a1c) (VERIFIED
  decompile). dom's capping icon passes five.
- **`setClock`/`setClockUp`** (UO 0x52c95/0x52cca, 1.1 0x4c644/0x4c664,
  shared body UO 0x529c5): three or five arguments, both times rounded up
  to ms, `time` above zero except for `setClockUp`, `duration` above zero,
  the shader indexed as `setShader` does, a negative size refused; type 8
  or 9, `time` = `level.time` + time at +0x5c, `duration` at +0x60
  (VERIFIED decompile). `_util_mp_gmi::InitClock` and bas's bomb
  stopwatch draw one.
- **`playLocalAnnouncerSound(alias)`** (0x494d8) sends `S <alias index>`
  to that player (the format at .rodata 0x9e432); **`rewindFx(ent, s)`**
  (0x6c620) sends `F <s * 1000>` to the entity's client (.rodata 0xa3b60);
  **`getStance()`** (0x68590) answers `prone` under `pm_flags` 1, else
  `crouch` under 2, else `stand`; **`detach(model[, tag])`** (0x67cb0,
  `G_EntDetach` 0x7818a) drops the first attachment on the tag whose model
  matches without case and shifts the rest down with their
  ignore-collision bits, or fails `failed to detach model '%s' from tag
  '%s'`; **`print3d`** (0x66eb4) is a `trap_AddDebugString` a dedicated
  server never draws (VERIFIED decompile, each).
- **`setTakeDamage`, `doDamage`, `doDamageMod`.** `setTakeDamage` (0x691d2)
  writes `takedamage` (+0x17d). `doDamage(damage, source[, attacker[,
  inflictor[, mod]]])` (0x68e67) calls `G_Damage` with the direction from
  `source` to the entity, the attacker NULL (the world) when absent, the
  inflictor the attacker, the means of death `G_IndexForMeansOfDeath`'s (0,
  `MOD_UNKNOWN`, when absent); `doDamageMod(damage, source, mod)` (0x69042)
  passes NULL for both (VERIFIED decompile). `G_Damage` (0x4fedb) on a
  non-client with `takedamage`: `eType` 5 notifies `trigger` and calls the
  `use`; `eType` 8 only notifies `damage` with six arguments (damage,
  attacker, normalized direction, point, means of death name, inflictor),
  leaving `health` alone; anything else takes the damage off `health`
  (raised to 1), notifies the same six, then `death` at 0 or below
  (VERIFIED decompile). `G_RadiusDamage` (0x5098b) passes the direction
  from the blast to `r.currentOrigin` lifted 24 and the blast's origin as
  the point (VERIFIED decompile). vcod reproduces all of this
  (`GameHost::damage_nonclient`); the client arm of `doDamage`
  (`G_DamageClient`, which the vehicle scripts reach) is not done. A
  `radiusDamage((100,200,10), 200, 100, 50)` on a script model at
  (100,200,0) with `setTakeDamage(true)` notifies `9 MOD_EXPLOSIVE (0, 0,
  1) (100, 200, 10)` on retail and on vcod (VERIFIED,
  `client-probes/probe_blastbase` on mp_cassino, `blast_entity_uo_ab`);
  the 9 against the distance's 97 is `CanDamage`'s 0.1 for a point on the
  floor (INFERRED).
- **`map_restart`'s letter.** 1.5's and UO's `SV_MapRestart_f`
  (coduo_lnxded 0x8088325; CoDMP15.exe's twin) send `n` when the game
  keeps `pers[]` (the `VM_Call(gvm, 0x12)` answer that also feeds
  `SV_RestartGameProgs`) and `B` when not: the strings at 0x80e35b1 and
  0x80e35b3 (VERIFIED, bytes; CoDMP15.exe 0x59df44 and 0x59df40). Retail
  UO sent `n` on bas's `map_restart(true)` (VERIFIED, the capture). UO's
  cgame runs `CG_MapRestart(1)` for `n` and `(0)` for `B`, the 0 also
  resetting the script menus (uo_cgame 0x3003ac90, 0x30039500; VERIFIED
  decompile). vcod sends `n` for a persisting restart on every protocol.
- **UO's placed weapons register.** `mpweapon_bazooka`,
  `mpweapon_panzerschreck`, `mpweapon_flamethrower` and eleven more UO
  radiant names (read off `weapons/mp/*` in pakuo*) register their items
  at spawn, which puts their projectile models in retail's order (mp_foy's
  `weapon_panzerfaust_rocket` at 487) (VERIFIED, the bas gate).
- **Fog.** `setCullFog`/`setExpFog` write slot 13 on UO (`CsLayout::fog`;
  mp_foy's `500 5500 1 0.5254 0.6117 0.7215 0`), 12 on 1.1 (VERIFIED, the
  gate).
- **An empty `model` key** (mp_foy's effect origins) takes no model slot:
  `G_ModelIndex` (0x77b37) returns 0 for `""` (VERIFIED decompile and the
  gate).
- **A notify reaches only the threads registered when it runs** (VERIFIED,
  `probe_endon_after_notify` on 1.1d and UO): hq's `level notify("Timer
  Changed"); level thread hq_reinforcement_timer()` keeps the new timer's
  endon (cod11-gsc-language.md 9).

### Gaps

- **bas `hud`.** Retail's `map_restart` recompiles the scripts, about 850 ms
  of wall time, and catches the frames up before its first snapshot
  (level start 23300, first snapshot 24150), so a new base health bar's
  first half second at full width (`setShader(..., 32, 4)` before the 0.5 s
  update writes 31) never reaches a client (VERIFIED, the capture's times;
  cod11-map-cycle.md 4.7). The gate ticks with no hitch.
- **bas `players`.** The probes stand 8500 units apart on mp_foy's open
  ground under a 500..5500 cull fog. Retail never sends either the other;
  vcod's PVS does. No fog or distance string is in either server binary;
  where retail culls is not found (open).
- Not measured: the bas bomb plant and defuse (a 15 s use hold on a
  `trigger_lookat`), ctf's drop and return, dom's domination bonus, hq's
  wave scoring past the first, any round or map end. `doDamage` on a
  client.

## 23. HTTP downloads

UO 1.51 runs 1.5's www download redirect (`cod15-protocol-diff.md` 14,
which holds the addresses for both). What is UO's own:

- A client without `cl_wwwDownload` on a server with `sv_wwwDownload 1`
  gets nothing: `SV_WriteDownloadToClient` (0x808c559) returns through a
  stub that returns 1 (0x808c54f) before it opens the pak (decompile,
  INFERRED from the branches). Live on loopback the server logged
  `clientDownload: 0 : begining "main/nsf_mp_j5outpost.pk3"` every frame
  and the client timed out at 0 bytes (VERIFIED). 1.5b opens the pak
  first and falls back to UDP by accident.
- A `wwwdl fail` sets the fallback flag, but the fallback branch returns
  through the same stub and the next frame redirects again; UO never
  falls back to UDP (decompile, INFERRED).
- `sv_wwwDlDisconnected 1` makes the redirect's flags 1; the client's
  `wwwdl bbl8r` draws `GMI_EXE_DOWNLOADDISCONNECTED` (live, VERIFIED,
  `crates/common/tests/fixtures/net/www-redirect-uo-discon.bin`).
- `CoDUOMP.exe` registers `cl_wwwDownload` default `1`, flags 3 in
  `CL_Init` (0x414500, decompile, VERIFIED), parses the redirect in
  `CL_ParseDownload` (0x418f90) and finishes it in `CL_WWWDownload`
  (0x413420).

vcod's client sends `cl_wwwDownload 1` on protocol 22 and fetches the
redirect over HTTP, connected or disconnected (`cod15-protocol-diff.md`
14, "What vcod does").

## Tools

- `tools/re/netfields.py <bin> <va> <count>` prints one table; UO's need
  it because the scan cannot split them.
- `tools/re/netfield_diff.py <binA> <va> <n> <binB> <va> <n>` diffs two
  tables by field name.
- `crates/common/examples/handshake_probe.rs` connects at any protocol
  number and prints the replies and the gamestate's configstrings; given a
  directory of `netfields.py` dumps of UO's six tables (named `ent.txt`,
  `alt.txt`, `ps.txt`, `cli.txt`, `obj.txt`, `hud.txt`), it decodes the
  baselines, enters the world with an unchanged cmd, answers the script
  menus (server info, auto-assign, the team's bolt rifle) and decodes every
  snapshot for `PROBE_SECS` (default 25) with the codec of sections 3 to 5.
  Live on mp_cassino dm and mp_arnhem ctf it spawned as axis with
  `kar98k_mp` and as British allies with `enfield_mp`, `pm_flags` 0x80000,
  every message parsed to `svc_EOF` (VERIFIED).
- `COD_VERSION=uo tools/run_server.sh` runs the retail server.
