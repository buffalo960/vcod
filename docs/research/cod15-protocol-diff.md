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
  stock paks (live, VERIFIED); section 13 reads what sets it off the 1.5b
  server.
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
| 0x2000 | none | jump penalty: set with 0x8 by the jump (0x1f882) and by prone entry (0x24037), timed by `pm_time` up to 1800 ms, scaling friction (0x1eab3) and the next jump's speed (`cod15-movement.md` 2) | decompile, INFERRED; set on every takeoff in the moves capture, VERIFIED |
| 0x10000 | 0x10000 | following a player | `SpectatorThink` and `StopFollowing` in both, INFERRED |
| 0x20000 | 0x40000 | own view, a live client in its own body | `ClientEndFrame` (1.1 0x40e98, 1.5 0x3ab2a) sets it on the playing branch where 1.1 sets 0x40000, and `GetFollowPlayerState`, `StuckInClient` and `BG_PlayerStateToEntityState` (1.1 0x2cbe8 tests 0x50000, 1.5 0x1c449 0x30000) test it, INFERRED; the live capture reads 0x20000 on every frame of a spawned player, VERIFIED |
| 0x40000 | none | a spectator may change whom it follows | `SpectatorClientEndFrame` (1.5 0x39edd) sets it on a free follow and clears it on a forced one, INFERRED; a spectator in the live capture reads 0x40000, VERIFIED |
| 0x80000 | none | a following spectator may leave to free flight (`G_ClientCanSpectateTeam(4)`) | same function, INFERRED |
| 0x100000 | none | `disableWeapon` | the method table at file offset 0x82da4 pairs `disableweapon` with 0x408e0, which sets it, and `enableweapon` with 0x40983, which clears it, VERIFIED |
| (FOLLOW without 0x40000) | 0x20000 | forced follow | 1.1's `SpectatorClientEndFrame` (0x40760) sets 0x20000 on the branch where 1.5 clears 0x40000 and 0x80000, INFERRED |

1.1 never sets 0x2000, so the rewrite keeps it in place for the predictor's
1.5 rules. 0x100000 lands on vcod's `flags::PMF_WEAPON_DISABLED`, above
1.1's 19 bits; the follow permissions drop on the way in. The server keeps
them as `PMF_MAY_CYCLE` and `PMF_MAY_FREE` and maps all three back on the
way out (`flags::pm_flags_v1_to_v6`, section 13).

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
`bg_prone_softyawedge` and of the prone dive, the bob cvars
moved into the game module (`cg_bobAmplitude*`, `cg_bobMax`, C variables
`bg_bob*`, mirrored to clients) and `BG_WeaponFireRecoil` (exported symbol
sets of both modules, VERIFIED). `cod15-movement.md` has the function by
function diff and the captures; vcod's predictor runs 1.5's rules on a
protocol 6 connection (`pmove::MoveRules`).

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
stock paks no local pak matches by checksum. `vcod-server` serves a stock
pak it lists to a client that asks, on either protocol, as retail does.

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

### The server's half

The game module (`game.mp.i386.so` 1.5b; `nm` addresses):

- `level` holds the timeout from 0x1744: the allied and axis banks (ms
  left), the timeouts each side called, the running timeout's length, its
  start (`trap_Milliseconds`), the recovery's end and the calling side
  (decompile of 0x45a91, VERIFIED). `G_InitGame` (0x4f10a) zeroes `level`
  and fills both banks from `g_timeoutBank`, so a map load or a
  `map_restart` resets all of it (VERIFIED).
- `Cmd_MatchTimeout_f` (0x45a91) tests, in order: `g_timeoutsAllowed` and
  `g_timeoutLength` above 0 (else `PATCH_1_5_TIMEOUT_NOTENABLED`), the
  caller playing on a side (`MUSTBEPLAYING`), no timeout or recovery
  running (`ALREADYINPROGRESS`), the side allies or axis
  (`INVALIDGAMETYPE`), its count under `g_timeoutsAllowed`
  (`MAXCALLED\x14<side>`), its bank above 0 (`MAXTIMEUSED\x14<side>`).
  Then the count goes up, the length is the smaller of the bank and
  `g_timeoutLength`, `timescale` is set to `0`, 1628 and 1629 are set and
  `e "PATCH_1_5_TIMEOUT_CALLED_PLAYERNAME\x15<name>"` goes to everyone
  (the strings and the branch order, VERIFIED; the refusals go to the
  caller alone).
- `Cmd_MatchTimein_f` (0x45f42): `NOTENABLED`, `NONEINPROGRESS`,
  `TIMEIN_MUSTBEPLAYING`, `TIMEIN_WRONGTEAM` for a side other than the
  caller's; else the elapsed time comes off the bank, the recovery starts
  (`g_timeoutRecovery`), 1628 and 1629 change and `TIMEIN_CALLED_PLAYERNAME`
  goes to everyone. Both arms of its side test charge the allied bank
  (0x1744), whichever side called (decompile, VERIFIED); vcod keeps that.
- `CheckMatchTimeout` (0x4fea8) runs at the end of `G_RunFrame`, after
  `CheckTeamStatus` and `CheckVote`, every frame (decompile, VERIFIED).
- `G_IsInMatchTimeout` (0x51043) is `level.frametime == 0` (VERIFIED). Its
  two callers are the turret think's fire block (0x52305), skipped whole,
  cooldown included, and its loop sound (0x523fd), which stops with the
  stop alias on every frame its timer still runs (decompile, INFERRED from
  the branches).

The engine (`cod_lnxded` 1.5b; virtual addresses):

- 1.5b keeps two clocks where 1.1d has `svs.time` alone: 0x83ccd88 and
  0x83ccd84. `SV_Frame` (0x80942a1) steps the first by the frame
  (`1000 / sv_fps`) and the second by `Com_ModifyMsec` (0x80716b2) of it,
  per frame, then runs the game frame (disassembly, VERIFIED). The live
  capture below says which is which: the snapshot's `serverTime` is the
  scaled one, the snapshot pacing the other (VERIFIED).
- 1.5's `Com_ModifyMsec` sets a negative `timescale` to `0`, scales by it
  always (1.1d skips the scale at 0, so a 1.1 `timescale 0` runs at full
  speed), truncates, and bumps a result under 1 to 1 only when `timescale`
  is not 0 (disassembly of 0x80716b2 against 1.1d's 0x806e32c, VERIFIED).
- `map` and `map_restart` set a `timescale` at or under 0 to 1
  (0x80876aa, 0x8087777, VERIFIED). A restart in a timeout leaves 1628 and
  1629 standing, since the table survives a restart and `G_InitGame` writes
  neither (INFERRED).

Live, 1.5b against `vcod-server` on the same paks, tdm,
`g_timeoutsAllowed 2`, `g_timeoutLength 15000`, `g_timeoutRecovery 5000`,
one probe on axis sending `matchtimeout` twice, `say hi`, `matchtimein`
twice (2026-10-14, VERIFIED on both, the same sequence):

- the call brings `d 1628 15000`, `d 1629
  PATCH_1_5_TIMEOUT_CALLED\x14GAME_AXIS\x15`, the `e` line, and only then
  `d 227 0`, the mirror's `timescale` row, which the engine flushes at the
  top of the next `SV_Frame`;
- the second call gets `e "PATCH_1_5_TIMEOUT_ALREADYINPROGRESS"`, the
  second timein `e "PATCH_1_5_TIMEOUT_NONEINPROGRESS"`;
- `serverTime` stands through the timeout and the recovery (13250 on ours,
  30200 on 1.5b, 19 s), snapshots still 20 a second; the recovery's end
  brings `d 1628 ""` then `d 227 1`, and `serverTime` moves on from where it
  stood.

vcod: `Server::sv_time_ms` is the game clock and `Server::svs_time_ms()`
the network's (pacing, pings, timeouts, the flood window, downloads,
heartbeats); on a protocol 6 server `Server::game_step_ms` is
`Com_ModifyMsec`. The game half is `game::timeout`. The commands run
inside the packet, their configstrings sent at once ahead of the `e`
lines, as `trap_SetConfigstring` does; the frame check's go out before the
frame's own configstring diff for the same reason.

## 13. Server side

What a server has to do differently to host 1.5 clients. `cod_lnxded` 1.5b
addresses are virtual; `game.mp.i386.so` 1.5b ones are `nm` addresses, as
above. Live comparisons: the 1.5b server on loopback (`COD_VERSION=1.5
tools/run_server.sh`, a basepath of the eleven stock 1.5 paks and the
localized ones) against `vcod-server` on the same paks, 2026-10-13.

### One protocol per process

Each retail binary checks the connect's `protocol` against its own and
nothing else (`SVC_DirectConnect`, 0x8089ebc, INFERRED), so a 1.5b server
takes 1.5 clients only and a 1.1d one 1.1 clients only. vcod keeps that
shape: one protocol per process, picked by `--set protocol=<1|6>` or, by
default, by the install (6 when `main/pak0.pk3` has 1.5's checksum,
1048127331 as the 1.5b server's `sv_referencedPaks` lists it, VERIFIED).
Mixing both on one server would need two script sets and two pak lists,
which no retail server has.

### `getinfo` (`SVC_Info`, 0x8092a74)

The key strings, each referenced in turn from 0x8092a74, are VERIFIED; the
value and the condition of each are read off its branch:

| Key | Value | When | Evidence |
|---|---|---|---|
| `challenge`, `protocol` (6), `hostname`, `mapname` | as 1.1 | always | INFERRED |
| `clients` | occupied slots, private ones included | non-zero | INFERRED |
| `sv_maxclients` | `sv_maxclients - (sv_privateClients - private slots in use)` | above 0 | INFERRED |
| `gametype` | `g_gametype` | always | INFERRED |
| `pure` | `sv_pure->integer` | non-zero | INFERRED |
| `minPing`, `maxPing` | as 1.1 | non-zero | INFERRED |
| `game` | `fs_game` | non-empty | INFERRED |
| `sv_allowAnonymous` | the integer | non-zero | INFERRED |
| `pswrd` | `1` | `g_password` non-empty | INFERRED |
| `ff`, `kc`, `timeoutsAllowed` | the string of `scr_friendlyfire`, `scr_killcam`, `g_timeoutsallowed` | its `atoi` non-zero | INFERRED |
| `hw` | `1`, a constant in the Linux binary (0x8092e6f) | always | VERIFIED |
| `pb` | `sv_punkbuster->integer` | always | INFERRED |
| `mod` | see below | always | INFERRED |

1.1 counts `clients` from `sv_privateClients` up and sends `sv_maxclients`
less every private slot (`cod11-server-handshake.md`, "Private slots"); 1.5
counts all of them and gives back the private slots in use (the two loops
from 0x8092a8e, INFERRED).

`mod` starts 0 and goes to 1 when `sv_pure` is 0, when `fs_game` is set,
or when a name in `sv_referencedPakNames` fails `FS_IdPak(name, "main")`
(0x8092ecf..0x8092f6c, INFERRED from control flow). 1.5's `FS_idPak`
(0x80754ea) accepts `mp_bin` and `pak`, `mp_pak`, `sp_pak` or
`localized_<language>_pak` with one hex digit (its format strings
`%s/mp_bin`, `%s/pak%x`, `%s/mp_pak%x`, `%s/sp_pak%x`, `%s/localized_`,
`_pak%x`, VERIFIED). vcod's reply to `getinfo xyz` on the 1.5 paks is byte
for byte the 1.5b server's (live, VERIFIED).

### `getstatus` (0x8092538)

The serverinfo string, then `challenge`, `con_disabled` (the
`sv_disableClientConsole` integer, only when non-zero), `pswrd` (`1` or
`0`, always) and `mod`, then 1.1's `%i %i "%s"` player lines (format at
0x80dfe4b) (INFERRED from the branches; the strings are VERIFIED). vcod's
reply matches the 1.5b server's byte for
byte on the same paks (live, VERIFIED).

### Connect

`SVC_DirectConnect` keeps 1.1's refusal strings (`EXE_BAD_CHALLENGE`,
`EXE_ERR_HIGH_PING_ONLY`, `EXE_ERR_LOW_PING_ONLY`, `EXE_SERVERISFULL`,
`error\n%s` for the game's verdict; the string set referenced from
0x8089ebc on, VERIFIED). It tests them in 1.1's order and only changes the
version it names (INFERRED). It reads
`cl_punkbuster` (an `atoi`) and `cl_guid` out of the userinfo (0x808a46a,
VERIFIED). Both go with the address to 0x80c191c, whose non-null answer is
either relayed when it starts `error\n` or drops the connect silently
(0x808a52b..0x808a59e, INFERRED); that it is PunkBuster's
authorization is INFERRED from the arguments and `sv_punkbuster`. The guid
the log line `Going from CS_FREE to CS_CONNECTED for %s (num %i guid %i)`
prints comes from the challenge entry (0x808a1d0), which only the CD key
authorize answer fills, INFERRED; the 1.5b server logs `guid 0` for a LAN
client (live, VERIFIED). vcod accepts the two keys and ignores them.

### Cvars

`tools/capture_cvars.py` against the 1.5b server, diffed with 1.1d's
capture (VERIFIED): `protocol 6`, `shortversion 1.5`, `gamename "Call of
Duty"` (1.1 `main`), `fs_game` gains `S`, `g_allowVote` gains `AL`, and new
rows: the `g_allowVote*` family, `g_autodemo`, `g_autoscreenshot`,
`g_deadChat`, `g_maxDroppedWeapons`, the four `g_timeout*`
(`g_timeoutsallowed` serverinfo), `sv_console_lockout`,
`sv_disableClientConsole`, `sv_kickBanTime`, `sv_packet_info`,
`sv_punkbuster` (`SRA`), the three `sv_www*` and the `ui_*` mirrors of the
vote, friendly fire, killcam and timeout settings. The game module also
registers `scr_friendlyfire` and `scr_killcam` (`SRA`) and `scr_drawfriend`
(`RA`) (`cvarlist scr_`, VERIFIED), which is why 1.5's serverinfo carries
`g_timeoutsallowed`, `scr_friendlyfire`, `scr_killcam` and `sv_punkbuster`
where 1.1's does not (the gamestate fixture's configstring 0, VERIFIED).
1.5 registers no `sv_master*` row; vcod keeps them for its heartbeat.

The 140/204 mirror on 1.5 drops `bg_duck2prone_time`, `bg_prone2duck_time`
and `bg_prone_softyawedge`, adds the four `cg_bob*` rows and `timescale`,
and is sorted by folded name: `ui_allow_thompson` sits at 180 and
`ui_allowVote` at 181, which byte order would swap (`gamestate-1.5.bin`,
VERIFIED).

Configstring 1 drops `timescale` and adds `sv_console_lockout`,
`sv_disableClientConsole` and the `ui_*` rows, in folded-name order
(gamestate fixture, VERIFIED). `G_SetUICvars` (0x4ec08) copies
`g_allowvote`, the twelve `g_allowVote*`, `scr_drawfriend`,
`scr_friendlyfire`, `scr_killcam` and the four `g_timeout*` into them
(decompile, VERIFIED); `G_InitGame` calls it at every level load
(INFERRED). With `sv_pure 1`
both servers send the same `sv_pakNames` and `sv_paks`, `pakb` down to
`pak0` (live, VERIFIED); `sv_referencedPakNames` differs, as on 1.1, since
retail lists the paks a file was read from. 1.5's systeminfo runs past
`MAX_INFO_STRING` on a full referenced list, so vcod drops stock paks off
that list first: they are never downloaded.

### Script builtins the stock 1.5 gametypes call

The 1.5 scripts (`pak9`, `paka`, `pakb`) call seven player methods and one
entity method 1.1 lacks; the player method table is at 0x82ca0, 54 entries
(VERIFIED):

- `getGuid` (0x4110b): `trap_GetGuid`, the engine's guid (0 without an
  authorize server, see Connect) (decompile, VERIFIED).
- `disableWeapon` (0x408e0) and `enableWeapon` (0x40983) set and clear
  `pm_flags` 0x100000 (decompile, VERIFIED). Pmove tests it beside the
  ladder's 0x10 at the weapon change (0x2f7be) and the raise (0x2e43d) and
  holsters the same way (INFERRED from control flow). `sd.gsc` brackets the
  plant and the defuse with them.
- `allowSpectateTeam(team, allowed)` (0x40f93) keeps a mask at
  `gclient+0x2174`: `none` 1, `axis` 2, `allies` 4, `freelook` 0x10 (the
  `scr_const` strings at +0xf8, +8, +4 and +0x100 are `none`, `axis`,
  `allies` and `freelook`, VERIFIED); allowed clears the bit, refused sets
  it (INFERRED). `G_ClientCanSpectateTeam(client, team)` (0x3a36d) is `!(mask >> team
  & 1)`, team 4 being free flight (VERIFIED). `_teams::SetSpectatePermissions`
  calls it on every team change.
- What the mask does (INFERRED from control flow): `SpectatorThink`
  (0x389eb) lets the sight edge end a follow only with free flight allowed
  and gives the spectator `ps.speed` 0 without it; `Cmd_FollowCycle_f`
  (0x42aef) skips a client whose team is refused; and
  `SpectatorClientEndFrame` (0x39edd) drops a follow of a refused team, and
  a spectator following nobody without free flight is cycled onto someone.
  The follow frame carries 0x40000 for a follow the script did not force
  and 0x80000 beside it with free flight allowed; a spectator left free
  carries 0x40000 when it may watch team 0, 1 or 2 (section 5).
- `autoDemoStart`, `autoDemoStop` (0x411bb, 0x41295) under `g_autodemo` and
  `autoScreenshot` (0x4136f) under `g_autoscreenshot` send the reliable
  `C`, `D` or `E` (strings at 0x781d4, 0x781f2, 0x78223, VERIFIED) and
  return 1; with the cvar at 0 they do nothing and return nothing
  (INFERRED).
- `getAttachIgnoreCollision(index)` (0x5ceeb), and `attach`'s third
  argument, which `G_EntAttach` (0x6b9b9) keeps as one bit per attachment
  at `gentity+0x17a` (decompile, VERIFIED). `_utility::saveModel` and
  `loadModel` carry it across the killcam. `G_DObjUpdate` (0x6b71c) and
  `BG_UpdatePlayerDObj` (0x1afc9) hand the bit to the engine beside each
  attached model (VERIFIED); that the engine's trace then skips that
  model's boxes is INFERRED from the name. No stock 1.5 script passes
  true. vcod's hit rig zeroes such a model's boxes
  (`hitrig::Assembly::ignore_collision`).
- `takeWeapon` (1.1 0x4493c, 1.5 0x3d749) is in both modules: the
  weapon's reserve and clip zeroed, then `BG_TakePlayerWeapon` (decompile,
  VERIFIED). No 1.1 stock script calls it; 1.5's `_teams::givePistol` does
  on every spawn.

With those, a protocol 6 server on the 1.5 paks builds 1.5b's whole
configstring table for mp_carentan dm, slot for slot, bar the referenced
pak pair (`crates/server/tests/configstrings_15_ab.rs` against
`gamestate-1.5.bin`, VERIFIED; the test runs when `COD_DIR` is a 1.5
install).

### Chat

1.5's `G_Say` (0x42dc9) differs from 1.1's (0x46c14) in four places
(decompile, VERIFIED; the strings at 0x786bd..0x787d0):

- The status ahead of the name: `\x15(\x14GAME_SPECTATOR\x15)` on the
  spectator team, else `\x15` and the side's colour, `^9` axis, `^8`
  allies, nothing on neither, then `(\x14GAME_DEAD\x15)` when the
  session state is not playing. 1.1 tests the health instead and colours
  nothing.
- `say_squad` (`ClientCommand` 0x46341, `Cmd_Say_f` 0x434b7 with mode 3):
  a speaker in squad `squad_alpha` or `squad_bravo` (the `sessionsquad`
  client field, set by 0x3b515) gets `%s(\x14<side>\x15
  \x14PATCH_1_5_SQUADALPHA\x15)%s%s: ` in colour 5 as an `h` line to
  `InSameSquad` (0x69987) clients; any other speaker's squad say becomes a
  team say, without the team say's own fall back to a plain say.
- `G_SayTo` (0x42c8b) lets a speaker out of play reach a playing client
  when `g_deadChat` is not 0.
- The log lines (`say;%d;%d;%s;%s`) carry the guid.

Live, 1.5b against `vcod-server`, tdm, an axis probe saying `say_squad`,
`say_team`, `kill`, two plain says and a `say_squad` while dead, an allied
probe listening, `g_deadChat 1` set over rcon between the two dead says
(2026-10-14, VERIFIED, identical on both):

    i "\x15\x15^9(\x14GAME_AXIS\x15)vcod^7: ^5sq"
    h "\x15\x15^9(\x14GAME_DEAD\x15)vcod^7: ^7dead1"
    i "\x15\x15^9(\x14GAME_DEAD\x15)(\x14GAME_AXIS\x15)vcod^7: ^5dsq"

and only `dead2`, said after `g_deadChat 1`, reached the living allied
probe. No stock 1.5 script sets `sessionsquad` (`pak9`..`pakb`,
VERIFIED); vcod has no such field, so every squad say is a team say.

### PunkBuster on the Linux server

The 1.5b Linux server has no `pbsv.so` here and runs without PunkBuster
(section 2). Live, against it (2026-10-14, VERIFIED): `+set sv_punkbuster
1` on the command line prints `sv_punkbuster is read only.` and `getinfo`
answers `pb 0` (`+set protocol 5` is refused the same way, `+set
shortversion 9` is not); `set`, `seta` and a bare `sv_punkbuster 1` over
rcon answer `sv_punkbuster is read only.`; `pb_sv_enable`, `pb_sv_ver`,
`pb_sv_kick 0` and any other unknown command answer an empty `print`; an
out-of-band `PB_` packet gets nothing. `vcod-server` on 1.5 answers each
the same way: `Server::set_cvar` refuses `sv_punkbuster`, the console
refuses the read-only cvar, and an unknown command prints nothing over
rcon.

### Spawn events

Retail 1.5b's spawn sends no event: a probe joining dm or tdm, and one
respawned by `scr_forcerespawn` after a `kill`, reads `eventSequence` 0
and an empty ring from its first playing snapshot; the death frame reads
`EV_DEATH` and `EV_RAISE_WEAPON` (2026-10-14, VERIFIED). `vcod-server`
sends the same. The `--probe-moves` run over `client-probes/probe_prone`
drains the same events on both (41, then 87 and 110 per jump, 18). The
`EV_PUTAWAY_WEAPON` an earlier comparison saw at a 1.5b spawn did not
come back. One difference showed instead: the dead playerstate's
`weaponstate` reads 0 on 1.5b and 1 on `vcod-server` in 1.5 mode (both 0
on 1.1).

### Not modelled

- The vote kinds the new `g_allowVote*` rows gate: vcod-server has no
  `callvote` or `vote` on either protocol.
- The `sessionsquad` client field (squad say above).
- The parts of 1.5's movement `cod15-movement.md` leaves out. The rest
  runs on a protocol 6 server for every client, bots included, as the 1.5
  game module's one pmove does (`MoveRules::V1_5`, set at `enter_world`);
  the wire carries the jump penalty's 0x2000 and `pm_time`, the stepped
  `viewHeightTarget` and the `eFlags` stance that follows it. The bots'
  nav graph is still proven under 1.1's rules. Live, the `--probe-moves`
  script against `vcod-server` on the 1.5 paks jumps 39.7, 39.5, 16.4,
  18.1, 40.8, 16.2, 17.8 and 39.3 units where the 1.5b capture
  (`mp_carentan-dm-slope-8ms-moves-1.5.txt`) jumps 40.3, 39.3, 16.6, 18.1,
  40.6, 16.7, 18.1 and 39.7 and 1.1d's about 40 every time, with the
  penalty bit on 400 snapshots against 1.5b's 399 (2026-10-13, VERIFIED).
- HTTP downloads. PunkBuster is refused as the Linux 1.5b server
  refuses it (above).

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

- Prediction on a protocol 6 connection runs 1.5's movement
  (`cod15-movement.md`, "What vcod does with this").
- `vcod-server` hosts protocol 6 when its install is 1.5 (section 13).

- `vcod-server` on 1.5 runs the match timeout (section 12), 1.5's chat
  and `g_deadChat` (section 13).

Not done: the 1.5 pmove changes `cod15-movement.md` leaves out, HTTP
downloads, PunkBuster, `C`/`D`/`E` on the client (vcod has no demos or
screenshots) and the server's votes (section 13).
