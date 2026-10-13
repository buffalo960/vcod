# CoD 1.5 (protocol 6) against 1.1 (protocol 1), client side

What a 1.1 client has to change to join a patch 1.5 server, and what it may
leave alone. The short answer: 1.5 is 1.1's wire with one wider netfield, a
renumbered top of `pm_flags`, a different protocol number, two more userinfo
keys and four new server commands. Huffman, netchan, the message codec, the
entity and clientState tables, the event table and the configstring layout
did not move.

## Sources

| Binary | md5 | Role |
|---|---|---|
| `cod_lnxded` 1.5b | `d3ac406b33acf2c9278a813d5f011b46` | retail Linux server; netfield tables, query and connect strings |
| `main/game.mp.i386.so` 1.5b | `343f99cd67b79ac74aeaa5261f63c011` | game module with symbols; `pm_flags`, configstrings, server commands |
| `CoDMP.exe` 1.5 | `4bdf293d8e6fb32208d1b0942a1ba6bc` | client engine; message codec, userinfo cvars |
| `main/cgame_mp_x86.dll` 1.5 | `075e2af18aeaf2aeaf2a75ce22db683a` | client game; server commands, events |
| `ui_mp_x86.dll` 1.5 (`pakb.pk3`) | `90274612d492da05d56b4a02cf684423` | UI module; server list columns, the PunkBuster join popup |
| `main/game_mp_x86.dll` 1.5 | `d396f23231a113f292c171443c403074` | game module the Windows server below loads |
| `pb/pbsv.dll` (1.5 install) | `faf14ec5f413e822e090e3fb704ac25d` | PunkBuster server, v1.076 |

The 1.1 counterparts are the ones `AGENTS.md` and
`cod11-events-and-fx.md` list. Addresses in `game.mp.i386.so` are `nm`
addresses (Ghidra's import of either module sits 0x10000 higher); 1.5
`CoDMP.exe` and cgame addresses are virtual at their image bases.

Method: `tools/re/netfields.py` and `tools/re/dump_field_table.py` on both
servers; a Ghidra export of 1.5 `CoDMP.exe`, `cgame_mp_x86.dll` and
`game.mp.i386.so` diffed function by function against the 1.1 exports after
renaming every address and local away; and live traffic between `vcod
--net-probe` and the 1.5b server run by `COD_VERSION=1.5
tools/run_server.sh` on loopback, captures dated 2026-10-13. The 1.5b
binary needs a 32-bit `libstdc++.so.5`; the script's header says where it
looks. PunkBuster has no Linux module here, so its measurements ran 1.5
`CoDMP.exe` as a dedicated server under wine (display drivers disabled)
inside `unshare -rn`, a namespace with only loopback, so PunkBuster reached
no outside server; `+set logfile 2` puts its console in `main/console_mp.log`.

## 1. Protocol number and queries

- `getinfo` and `getstatus` both report `protocol\6`; `getstatus` reports
  `shortversion\1.5` (live capture, VERIFIED).
- `infoResponse` keys, in order: `challenge`, `protocol`, `hostname`,
  `mapname`, `clients` (only when nonzero), `sv_maxclients`, `gametype`,
  `pure` (only when 1), `pswrd` (only when 1), `kc`, `hw`, `pb`, `mod` (live
  capture with 0 and 1 clients, `g_password` set and cleared, `sv_pure` 0
  and 1, VERIFIED). 1.1 sends `clients`, `pure` and `pswrd` always and has
  no `kc`, `hw`, `pb` or `mod`, and 1.5 drops `sv_allowAnonymous`
  (VERIFIED against the 1.1 capture in `cod11-server-handshake.md`). vcod's
  `ServerInfo` reads a missing key as 0, which is what each one means. `kc`
  tracks `scr_killcam` (INFERRED from the values seen beside it). `pb` is 1
  with PunkBuster running and 0 without (live, the Windows server with and
  without `+set sv_punkbuster 1`, VERIFIED). `hw` is 1 from the Linux 1.5b
  server and 2 from the Windows one (live, VERIFIED), the index of the
  1.5 UI's hardware icons (section 11). `mod` read 1 from both servers on
  stock paks (live, VERIFIED); what sets it is not decoded.
- `challengeResponse <n>`: one integer, as on 1.1 (live capture, VERIFIED);
  both format strings survive at 0x80ddeb8 and 0x80ddea0 (VERIFIED).
- A connect whose `protocol` key is wrong gets
  `error\nEXE_SERVER_IS_DIFFERENT_VER\x151.5\n` (live capture against a
  protocol-1 connect, VERIFIED; token at 0x80de2c6).
- The master query is `getservers <protocol> full empty` in both, the
  protocol coming from the UI's `globalservers %d %d full empty` (the
  string is in both `ui_mp_x86.dll`s, VERIFIED). The master answering
  `getservers 6` has not been measured: this machine queries no public
  server.

## 2. Handshake and userinfo

- 1.5 registers two userinfo cvars that 1.1 lacks, `cl_guid` (default
  `unknown`, flags 0x42) and `cl_punkbuster` (default 0, flags 0x43), between
  `cl_anonymous` and `password` (`CL_Init`, 1.5 `CoDMP.exe` 0x413380,
  VERIFIED). The newest-first info string walk therefore puts
  `\cl_punkbuster\0\cl_guid\unknown` between `password` and `cl_anonymous`
  (INFERRED from the registration order, as for 1.1's order). Nothing in
  1.5 `CoDMP.exe` sets `cl_guid` (VERIFIED: its only reference is the
  registration); PunkBuster sets it, which is INFERRED.
- The 1.5b server logs `Going from CS_FREE to CS_CONNECTED for vcod (num 0
  guid 0)` for that userinfo and accepts it (live, VERIFIED).
- The connectionless commands a 1.5 client knows are 1.1's minus the MOTD:
  `getmotd`, `cl_motd` and `codmotd.activision.com` are gone from 1.5
  `CoDMP.exe`; `challengeResponse`, `connectResponse`, `infoResponse`,
  `statusResponse`, `getserversResponse`, `keyAuthorize`, `needcdkey`,
  `loadingnewmap`, `print`, `disconnect` and `updateResponse` are in both
  (string sets, VERIFIED). CD key authorization uses the same
  `getKeyAuthorize %i %s` and `getIpAuthorize %i %i.%i.%i.%i %s %i` strings
  (VERIFIED). vcod does neither, as on 1.1.
- PunkBuster: `sv_punkbuster`, `cl_punkbuster` and the `pbsv.so` loader
  strings are new in 1.5b (VERIFIED). See "PunkBuster" below for what a
  PunkBuster server does.
- HTTP downloads: 1.5b has `sv_wwwDownload`, `sv_wwwBaseURL` and a `wwwdl`
  client command, and logs `Client '%s' is not configured for www download`
  for a client without `cl_wwwDownload` before it falls back to the UDP
  transfer (strings, VERIFIED; the fallback is INFERRED from the message).
  vcod sends no `cl_wwwDownload` and keeps the UDP path.

### PunkBuster

- `sv_punkbuster` is read-only once registered: the Linux 1.5b server
  prints `sv_punkbuster is read only.` for `+set sv_punkbuster 1` and runs
  without PunkBuster (live, VERIFIED; no `pbsv.so` here). The Windows
  server loads `pb/pbsv.dll` and prints `PunkBuster Server (v1.076 | A1319
  C1.074) Enabled` for the same `+set` (live, VERIFIED). `pb_sv_enable` on
  a running server answers `PunkBuster Server: This edition of PunkBuster
  Server cannot be enabled while the game server is running.` (live, rcon,
  VERIFIED).
- With PunkBuster on, a connect whose userinfo has `cl_punkbuster 0`, or no
  `cl_punkbuster` and no `cl_guid` at all, gets one
  `error\nPunkBuster must be Enabled before joining this server. PunkBuster
  can be Enabled/Disabled in the Multiplayer Options menu. Visit
  www.evenbalance.com for PunkBuster support.\n` and nothing else (live,
  VERIFIED). The text comes from `pbsv.dll`: `SV_DirectConnect` (1.5
  `CoDMP.exe` 0x457380) calls the PunkBuster hook at `0x14604b4` when it is
  set and sends what it returns when that starts `error\n` (decompile,
  INFERRED). Without PunkBuster the same server takes `cl_punkbuster` 0, 1
  or absent and logs `guid 0` for each (live, VERIFIED).
- 1.5's UI checks first: `JoinServer` (ui_mp_x86.dll 1.5 0x4000b124) opens
  `joinpb_popmenu` (`paka.pk3` `ui_mp/pb_popmenus.menu`) when the picked
  server's PunkBuster byte is 1 and `cl_punkbuster` is 0, else runs
  `connect %s` (disassembly, INFERRED from the branches). The byte comes
  from UI trap 0x62; the engine function that ends at 1.5 `CoDMP.exe`
  0x419ad6 returns offset 0x20 of a server entry, where the `pb` key is
  stored at 0x414145 (disassembly, VERIFIED). The popup's Yes runs
  `setPbClStatus 1` and `uiScript joinserver` (the menu file, VERIFIED).

vcod sends `cl_punkbuster 0` and `cl_guid unknown`, the 1.5 defaults with
PunkBuster off, on protocol 6, and never claims PunkBuster. Its
`joinserver` skips the popup, whose Yes it cannot honour, and connects; the
refusal above then drops the connect with the server's text, which the
error popup shows. The browser marks PunkBuster servers on 1.5's menus
(section 11).

## 3. Message codec, Huffman, netchan

Every function of 1.1 `CoDMP.exe` between 0x443000 and 0x449000 (52
functions: the bit and byte readers and writers, the usercmd, entityState
and clientState deltas, the Huffman coder) has a 1.5 twin whose decompile
is identical once addresses and locals are renamed, except the playerState
reader and writer (1.1 0x447860 / 0x446830, 1.5 0x44a630 / 0x449600), which
differ only in the size of the zeroed or copied playerState, 0x834 dwords
against 0x833 (VERIFIED, normalised decompile diff). The snapshot writer
in the server half (1.1 0x45c2b0, 1.5 0x4613e0) has the same shape and
passes a null base for a full frame in both (VERIFIED, call sites at 1.5
0x46154b).

The live capture agrees: the 1.5 pair in `crates/common/tests/fixtures/net/`
re-encodes byte for byte through vcod's 1.1 writer given 1.5's tables
(`protocol_6_capture_reencodes_and_maps_pm_flags`, VERIFIED).

## 4. Netfield tables

`cod_lnxded` 1.5b holds the tables at 0x080db860 (entityState, 59),
0x080dbfe0 (clientState, 22) and 0x080dc560 (playerState, 103), and the
objective and HUD-element tables from 0x080e9a80 (VERIFIED,
`dump_field_table.py`). Against 1.1d's 0x080d1760, 0x080d2058, 0x080d229c and
0x080de384:

- entityState, clientState, objective and HUD-element entries: equal name,
  offset and width (VERIFIED).
- playerState [12] `pm_flags`: 21 bits, 1.1 19 (VERIFIED).
- playerState [76] at offset 104: renamed `fJumpOriginZ` from `fJumpPeak`,
  same width (VERIFIED).
- playerState [86] `deltaTime`: offset 8392, 1.1 8396 (VERIFIED). The offset
  is a struct layout detail and does not reach the wire.

`CoDMP.exe` 1.5's own copies (0x580080 onward) differ from 1.1's in the same
two playerState entries and nowhere else (VERIFIED, `netfields.py`). A 1.1
decoder reading a 1.5 snapshot desyncs on the first frame that changes
`pm_flags` (the "overran the message" warnings of the first live run,
VERIFIED) and reads the rest correctly.

## 5. `pm_flags`

The low sixteen bits keep their 1.1 meaning except 0x2000, and the bits
above them were renumbered. vcod rewrites a protocol 6 value into 1.1's
layout as it decodes it (`flags::pm_flags_v6_to_v1`), so nothing past the
decoder knows the difference.

| 1.5 | 1.1 | Meaning | Evidence |
|---|---|---|---|
| 0x0001..0x1000, 0x4000, 0x8000 | same | prone, crouch latch, jump held, ladder, ADS, back run, ADS walk, knockback, damage, 0x400, respawned, melee latch, lean, prone blocked | the same bit set and tested in the same functions of both modules (`PM_UpdateViewAngles` 0x1, 0x10, 0x8000; `PM_UpdateLean` 0x4000; `StopFollowing` 0x10020; the melee latch 0x1000 beside `PM_AddEvent(164)` at 1.1 0x38f68 and 1.5 0x301a4), INFERRED |
| 0x0004 | 0x0004 | 1.1's `PMF_PRONE_DIVE`; 1.5 never sets it | no write of 0x4 to `pm_flags` in the 1.5 export, INFERRED |
| 0x2000 | none | jump penalty: set with 0x8 and `fJumpOriginZ` by the jump (0x1f882), timed by `pm_time` up to 1800 ms, scaling friction (0x1eab3) and the next jump's speed | decompile, INFERRED |
| 0x10000 | 0x10000 | following a player | `SpectatorThink` and `StopFollowing` in both, INFERRED |
| 0x20000 | 0x40000 | own view, a live client in its own body | `ClientEndFrame` (1.1 0x40e98, 1.5 0x3ab2a) sets it on the playing branch where 1.1 sets 0x40000, and `GetFollowPlayerState`, `StuckInClient` and `BG_PlayerStateToEntityState` (1.1 0x2cbe8 tests 0x50000, 1.5 0x1c449 0x30000) test it, INFERRED; the live capture reads 0x20000 on every frame of a spawned player, VERIFIED |
| 0x40000 | none | a spectator may change whom it follows | `SpectatorClientEndFrame` (1.5 0x39edd) sets it on a free follow and clears it on a forced one, INFERRED; a spectator in the live capture reads 0x40000, VERIFIED |
| 0x80000 | none | a following spectator may leave to free flight (`G_ClientCanSpectateTeam(4)`) | same function, INFERRED |
| 0x100000 | none | `disableWeapon` | the method table at file offset 0x82da4 pairs `disableweapon` with 0x408e0, which sets it, and `enableweapon` with 0x40983, which clears it, VERIFIED |
| (FOLLOW without 0x40000) | 0x20000 | forced follow | 1.1's `SpectatorClientEndFrame` (0x40760) sets 0x20000 on the branch where 1.5 clears 0x40000 and 0x80000, INFERRED |

Nothing in 1.1 carries the 1.5-only bits, so the rewrite drops them; vcod's
predictor runs 1.1's pmove and does not model the jump penalty (section 9).

## 6. eFlags, entity types, events

- eFlags: the same bits set and cleared at the same sites (pingPlayer
  0x80000, the friend ping 0x100000 in `ClientEndFrame`, talk 0x40000 off
  the usercmd's button 2 in `PmoveSingle`, the vote bit 0x20000 kept across a
  follow in `SpectatorClientEndFrame`, the firing bit 0x400), INFERRED. The
  netfield is 24 bits in both (VERIFIED).
- Entity types: players arrive as 1, the carentan MG42s as 11, as on 1.1
  (live capture, VERIFIED).
- Events: the `EV_*` name tables of 1.1 and 1.5 `cgame_mp_x86.dll` (202
  entries) and `game_mp_x86.dll` are identical (`evtab.py`, VERIFIED), so no
  event id needs remapping.

## 7. Configstrings

- The indexers walk the same bases: `G_FindConfigstringIndex` callers pass
  16, 32, 64, 108, 128, 256, 524, 780, 844, 1100, 1244 and 1500, and
  `G_ModelIndex` reads from 0x10c (268) (1.5 game module, VERIFIED).
- A gamestate from each server on mp_carentan dm puts every block where 1.1
  does: the cvar mirror at 140..203 and 204..267, models from 269, menus from
  1180, localized strings from 1245 (live captures, VERIFIED).
- New: 1628 and 1629, the match timeout (section 12): written by
  `Cmd_MatchTimeout_f`, `Cmd_MatchTimein_f` and `CheckMatchTimeout`
  (0x4fea8) (`trap_SetConfigstring(0x65c, ..)`, `(0x65d, ..)`, VERIFIED).
  1.1's cgame reads nothing at 1628 or 1629 (its `CG_ConfigStringModified`
  ends its last block at 1627, VERIFIED), so vcod reads them on either
  protocol.
- The stock 1.5 gametypes register `serverinfo_<gametype>` as the first
  script menu and open it before the team menu, which they open once it
  answers `close` (`maps/MP/gametypes/dm.gsc` in `pak9.pk3`, and
  `ui_mp/scriptmenus/serverinfo_dm.menu` in `pak8.pk3`, VERIFIED). The live
  capture has `serverinfo_dm` at 1180 and the team menu at 1181, one later
  than 1.1 (VERIFIED). A `t <index>` names the menu by that index, not by
  `g_scriptMainMenu`, which still names the team menu (live, VERIFIED).
- The weapon list in configstring 7 on carentan dm is the same 32 names
  (live captures, VERIFIED).

## 8. Server commands

1.5's `CG_ServerCommand` (cgame 0x3002f660, 1.1 0x3002e0d0) handles 1.1's
lowercase set unchanged and adds four (decompile, VERIFIED):

- `B`: `CG_MapRestart(0)` (0x3002df90). 1.5b's `map_restart` sends it in
  place of 1.1's `n` (live, VERIFIED: `d 13`, `d 12`, `d 3`, `B`, then `d 1`
  with the new `sv_serverid`). `n` now calls `CG_MapRestart(1)`; the
  argument only decides whether the script menus are reset (INFERRED). vcod
  treats `B` as `n`.
- `C`, `D`, `E`: start a demo, stop it, take a screenshot, each gated on the
  client's `cg_autodemo` or `cg_autoscreenshot` being 1 (decompile,
  INFERRED from the branches). `C` runs `record %s-%s` and `E`
  `screenshotJPEG %s-%s`, `D` `stoprecord` (the strings, VERIFIED); the
  arguments are the player's `name` stripped to letters and digits
  (0x3002f470) and a string from cgame trap 0xf4, not decoded (decompile,
  INFERRED).
- The 1.5b game module sends them, reliably to one client, from the player
  methods `autoDemoStart`, `autoDemoStop` and `autoScreenshot` (0x411bb,
  0x41295, 0x4136f; their usage errors name them, VERIFIED). vcod has no
  demo recording and no screenshot command, so it ignores all three.

## 9. Pmove

1.5 changed movement the client predicts: the jump penalty of section 5,
the removal of `bg_duck2prone_time`, `bg_prone2duck_time` and
`bg_prone_softyawedge` and of the prone dive, new view bob cvars
(`bg_bobAmplitude*`, `bg_bobMax`) and `BG_WeaponFireRecoil` (exported symbol
sets of both modules, VERIFIED; their effect is INFERRED from names and the
decompile of `PM_Friction` and the jump). vcod predicts with 1.1's pmove, so
on a 1.5 server a second jump inside 1.8 s and the landing slowdown are
corrected by the server rather than predicted.

## 10. Paks and pure servers

- Stock pak names are hexadecimal in 1.5: `FS_idPak` (1.5 `CoDMP.exe`
  0x43e310) walks `pak0`..`pakf`, `mp_pak*` and `sp_pak*` with `%x`, where
  1.1's (0x43b6b0, `%s/pak%d`) walks `pak0`..`pak8` (decompile, VERIFIED). The 1.5 install ships `pak0`..`pak6`, `pak8`, `pak9`, `paka`
  and `pakb`.
- 1.1 and 1.5 `pak0.pk3` differ, so a client must load a 1.5 install to join
  a pure 1.5 server: from one, vcod's `cp` passes (live, `sv_pure 1`,
  VERIFIED); from a 1.1 install the server drops it with
  `EXE_SERVERDISCONNECTREASON\x14EXE_UNPURECLIENTDETECTED` (live, VERIFIED).
- A non-pure 1.5 server takes a client on a 1.1 install (live probe,
  VERIFIED), but references `pak8` and `pak9`, which that install lacks,
  and the 1.5 menus (`serverinfo_dm`) live there. On tdm it references
  `main/pakb main/pak9 main/pak8 main/pak5 main/pak4 main/pak1 main/pak0`
  (live, VERIFIED).
- 1.5's `FS_ComparePaks` (`CoDMP.exe` 0x43e510) skips a name `FS_idPak`
  calls stock and one whose lower-cased name holds `_svr_` (0x43e490); 1.1's
  (0x43b830) skips only `FS_idPak`'s (decompile, INFERRED from the
  branches). `FS_idPak` compares the name with `%s/mp_bin`, `%s/pak%x`,
  `%s/mp_pak%x`, `%s/sp_pak%x` (1.1 `%d`) and a `localized_` name's rest
  with `_pak%x` (the format strings, VERIFIED; the loops' bounds, 0x10 and
  9, are INFERRED). So a 1.5 client never asks for a stock pak, whatever
  its install holds (INFERRED).
- `SV_WriteDownloadToClient` passes the download name, `.pk3` included, to
  `FS_idPak` (1.5b `cod_lnxded` 0x808b92a, the name at client+0x10a64;
  disassembly, VERIFIED). The `EXE_CANTAUTODLGAMEPAK` refusal therefore
  never matches a stock pak (INFERRED). Before vcod knew 1.5's stock set, a 1.1
  install on the 1.5b server asked for `main/pakb.pk3`, and the server
  logged `clientDownload: 0 : beginning "main/pakb.pk3"` and streamed it
  (live, VERIFIED). 1.1d's download path calls its `FS_idPak` the same way
  (0x80709c0 on client+0x10a64; decompile, INFERRED).

vcod's client skips what the connection's protocol calls stock
(`Protocol::id_pak_numbers`, `skips_svr_paks`;
`net::download::is_stock_pak`) and warns once per load which referenced
stock paks no local pak matches by checksum. `vcod-server` refuses a
stock pak with `EXE_CANTAUTODLGAMEPAK` under 1.1's set, which retail does
not.

## 11. Server browser

1.5's `joinserver.menu` (`paka.pk3`) lists nine columns where 1.1's lists
six (the menu files, VERIFIED). 1.5's UI feeder (`UI_FeederItemText`,
ui_mp_x86.dll 1.5 0x4000ce00) switches on the column through the
nine-entry jump table at 0x4000d22c (disassembly, VERIFIED); per column,
off the branches (INFERRED):

| col | 1.5 | 1.1 |
|---|---|---|
| 0 | `X` when `pswrd` | the same |
| 1 | the `hw` icon, `ui_mp/assets/server_hardware_*` in the order unknown, linux_dedicated, win_dedicated, mac_dedicated, win_listen, mac_listen for 0..5; none outside | name |
| 2 | `hostname` when the ping is above 0, else `addr` | map |
| 3 | `mapname` | players |
| 4 | `%s (%s)` over `clients`, `sv_maxclients` | game type |
| 5 | `gametype`, `?` when empty | ping |
| 6 | `ui_mp/assets/punkbusterlogo` when `pb` | |
| 7 | `X` when `mod` | |
| 8 | the ping, `...` when 0 | |

An icon cell returns empty text and a shader handle. 1.1's
`Item_ListBox_Paint` (0x400159b0) draws a handle as a square of the
column's width at `x + 4 + pos`, `y - 1 + elementHeight / 2` (decompile,
INFERRED); 1.5's list box doing the same is INFERRED. `ServerSort <n>`
sorts on column `n` in both: 1.5's `LAN_CompareServers` (`CoDMP.exe`
0x419c00) switches on keys 0..8 to `pswrd`, `hw`, hostname, mapname,
clients, gametype, `pb`, `mod` and ping (disassembly, INFERRED from the
jump table's targets). 1.5's master query asks `getservers 6`, so its list
holds 1.5 servers only (INFERRED from section 1). No column marks a
server's version (the menu files, VERIFIED).

vcod reads the layout off the list box the loaded menu has (nine columns
mean 1.5's), fills the cells as above, and sorts the extra columns by
value. vcod lists 1.1 and 1.5 servers together and marks neither, as
retail has no column for it.

## 12. Match timeout

1.5 adds `matchtimeout` and `matchtimein` client commands (`ClientCommand`
0x46341, the `Q_stricmp` strings, VERIFIED). They work when
`g_timeoutsAllowed` and `g_timeoutLength` are above 0, and a team player
calls one for its team, with `g_timeoutBank` and `g_timeoutRecovery`
beside them (`Cmd_MatchTimeout_f` 0x45a91, decompile, INFERRED).

Measured on the 1.5b server, tdm, `g_timeoutsAllowed 2`,
`g_timeoutLength 15000`, `g_timeoutRecovery 5000`, a probe on axis:

- `matchtimeout` brings `cs 1628 "15000"`, `cs 1629
  "PATCH_1_5_TIMEOUT_CALLED\x14GAME_AXIS"` and `e
  "PATCH_1_5_TIMEOUT_CALLED_PLAYERNAME\x15vcod"`, and the cvar mirror's
  `timescale` value (227) changes while configstring 1 does not (live,
  VERIFIED). The game sets `timescale` to `0` (the `trap_Cvar_Set`
  arguments, VERIFIED).
- Snapshots keep coming at 20 a second, all at the serverTime of the call,
  43600 for 13 s (live, VERIFIED).
- `matchtimein` brings `cs 1628 "5000"`, `cs 1629
  "PATCH_1_5_TIMEOUT_ENDING\x15"` and `e
  "PATCH_1_5_TIMEIN_CALLED_PLAYERNAME\x15vcod"`; 5 s later `cs 1628 ""`
  arrives and serverTime moves again (live, VERIFIED).
- `CheckMatchTimeout` sends `e "PATCH_1_5_TIMEOUT_EXPIRED"` and the
  recovery's 1628 when the length runs out, and sets `timescale 1` with an
  empty 1628 when the recovery does (decompile, INFERRED).

The cgame:

- `CG_ConfigStringModified` (0x3002db40) keeps 1628 as `atol` plus cgame
  trap 6 and clears a flag; 1629 goes through trap 0x39 (`"timeout
  string"`) into a 256-byte buffer. `CG_Init`'s reader (0x30024290) sets
  that flag for a nonzero 1628 (decompile, INFERRED). Trap 6 is
  `Sys_Milliseconds` in 1.1's `CoDMP.exe` (jump table 0x4041f0 to
  0x4659d0, `timeGetTime`; VERIFIED); 1.5's being the same is INFERRED.
- `CG_Draw2D` (0x300199e0) calls the banner (0x300195c0) right after the
  vote, past the intermission (`pm_type` 5) return (decompile, INFERRED).
- The banner formats `"%s: %i \n%s"` over `CGAME_PAUSED`, the whole
  seconds left and the message, or `"%s\n%s"` with the flag set, and draws
  it at ((640 - width) / 2, (480 - height) / 2) in font 0 at 0.5, white,
  style 6 (the strings and constants, VERIFIED; which format runs when is
  INFERRED from the branch). `CGAME_PAUSED` is "Paused" in 1.5's
  `cgame.str` (the asset, VERIFIED).

The client's clock (`docs/protocol-1.1.md`, "The client's clock") runs
`CL_AdjustTimeDelta` on each snapshot parsed, frozen or not, as Q3's
`CL_ParseSnapshot` sets `cl.newSnapshots` for every valid one (INFERRED
from the lineage). Over a frozen serverTime the adjusts pull the delta
back as fast as local time pushes it, and the drawn time stops about
150 ms past the snapshot's (INFERRED, simulating the rule at 8 ms
frames).

vcod draws the banner (`hud::timeout`), with the line break as a second
line under the first and its shadow vcod's usual 1 px one, and counts a
new snapshot by its message number, so the drawn time holds through a
timeout. A 1.1 install lacks `CGAME_PAUSED` and the `PATCH_1_5_*` strings
(`localized_english_pak5.pk3`) and shows the keys.

## What vcod does with this

- `net::protocol::PROTOCOL_V6` beside `PROTOCOL_V1`: version, short version,
  the tables (1.1's with the wider `pm_flags`, `net::fields_v6`) and the
  `pm_flags` rewrite. A `NetClient` holds one; `NetClient::protocol` hands
  it to everything that decodes.
- The client connects as protocol 1 and, refused with
  `EXE_SERVER_IS_DIFFERENT_VER` naming a version it speaks, starts the
  handshake over in that one. The browser sends `getservers` for both.
- `B` restarts like `n`; the join probe and `--team` close
  `serverinfo_<gametype>` and resolve a `t` index through the menu
  configstrings.
- Stock paks follow the protocol (section 10), the browser fills 1.5's
  columns (section 11), and the match timeout draws its banner and leaves
  the clock standing (section 12). PunkBuster servers refuse vcod with
  their own text (section 2).

Not done: the jump penalty and the other 1.5 pmove changes in prediction,
HTTP downloads, `C`/`D`/`E` (vcod has no demos or screenshots), and
anything server side.
