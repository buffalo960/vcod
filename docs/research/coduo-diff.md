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
| serverinfo, systeminfo, ... scores, fog, northyaw, level start | 0..21 | same slots | | |
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
are VERIFIED. Their meanings are INFERRED from their writers in the game
module:
`angles2[0..1]` turret pitch and yaw, `origin2[1]` and `angles2[2]` gunner
yaw and pitch, `origin2[0]` and `origin2[2]` body pitch and roll as 8 bits
of `127 + 127 * deg / 30`, `leanf` the steer angle, `time` the tread scroll,
`time2` suspension travel times 1000 in 6 bits, `animMovetype` the occupied
seat mask, `otherEntityNum` the driver, `weapon` the turret weapon. The live
tanks read `origin2[0]` and `origin2[2]` 127, level (VERIFIED). `time2` read
8 and 24 there; a stock `suspensionTravel` of 7 would give 7000, which 6
bits cannot hold, so that meaning wants a capture of a moving vehicle
(INFERRED).

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
  (VERIFIED). The format is 1.1's `VEHICLEFILE\k\v` with new keys
  (`hasGunner`, `numPassengers`, `turretAltWeapon`, `turretGunnerWeapon`
  and its spans, `dmgScale{Front,Side,Behind,Bullet,Under}`, box
  `mins/maxs`, `stepsize`, `collision{Front,Back}Dist`, `impactSnd`, ...)
  (VERIFIED, the files).
- **Spawn.** `SP_script_vehicle` (0x8dec6) takes one of 64 pool slots of
  0x3ec bytes through `G_SpawnVehicle`; contents 0x2080, plus 0x200000 while
  usable; a `script_vehicle_collmap` brush gets 0x800000 | 0x200000
  (VERIFIED decompile). 246 `script_vehicle` entities sit in 10 stock MP
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
  (VERIFIED). `VEH_UnlinkPlayer` (0x89fb2) dismounts at `tag_detach` or one
  of six box offsets (0xa6660) (VERIFIED). `nextvehslot` / `prevvehslot`
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
- **Client rendering.** cgame 0x30020540 lerps the packed angles, poses
  `tag_body`, `tag_turret`, `tag_barrel`, `tag_secondary_base`,
  `tag_secondary_gun` and `tag_steeringwheel`, traces each `tag_wheel_*`
  down for the suspension and plays tread and wheel effects per surface;
  the seat view sits at the seat tag of `viewlocked_entNum` (0x30040810)
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

**eFlags** (24 bits). The bits from 0x200 up moved down one place against
1.5 (ADS 0x100, firing 0x200, mount mask 0x6000, vote 0x10000, talk
0x20000, ping 0x40000, friend ping 0x80000) and 0x100000..0x400000 are the
vehicle bits (INFERRED from the set and test sites; where 1.5's 0x100 went
is not resolved).

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
`bg_lmg_yawcap` around `proneDirection` (INFERRED).

**Grenade cooking exists.** `PM_Weapon` (0x3726c) counts `grenadeTimeLeft`
down by the frame msec for a grenade with `cookOffHold`, raises
`EV_GRENADE_SPOON` (182) on the button edge, throws by itself under 51 ms
(event 163, plus 210 when `explosionInnerDamage` is non-zero) and
`fire_grenade` (0x5f7ad) fuses with what is left (VERIFIED decompile).
Every stock UO grenade, smoke, flash and satchel has `cookOffHold 1`
(VERIFIED). This contradicts the 1.1 rule in `AGENTS.md` ("no grenade cook
in 1.1 MP"), so any shared code needs a version gate.

The jump penalty, friction, ladder and slide are 1.5's apart from struct
offsets (INFERRED), so UO movement is 1.5's plus sprint, fatigue, the
bipod, the cook and the vehicle view clamp.

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
  renumbered (VERIFIED). The gsc callbacks see these names.
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
- Language: vcod's gsc compiler parses and compiles 326 of the 329 UO
  scripts (VERIFIED, scratch harness). The failures are two SP scripts with
  an empty first argument (`setanimknobrestart(,...)`) and stock MP
  `pakuo01.pk3:maps/mp/gametypes/dom.gsc:3377`, whose `", "^7` is a binary
  `^` outside a string. Retail ships and loads `dom`, so UO's compiler takes
  `^` as an operator (INFERRED); vcod's grammar notes `^` as absent from 1.1
  code.

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
- Vehicle engines are `ET_SOUND_BLEND` entities crossfading two aliases
  (section 8); shells blend `projectileSoundBlend1/2` (INFERRED from the
  keys).

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
| Movement | | sprint, fatigue, bipod, cook, vehicle pitch clamp on top of 1.5's pmove (the pmove15 work); M-L | the same pmove server side, `trigger_mount`; M | shared pmove must stay byte-identical for 1.1 (the pmove A/B gates) |
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
2. **Play on a UO server.** UO pmove in prediction (after pmove15's 1.5
   mode lands), sprint and the bipod, the new weapons' client sides,
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
