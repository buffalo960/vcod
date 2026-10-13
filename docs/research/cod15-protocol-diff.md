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
looks.

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
  tracks `scr_killcam` and `pb` `sv_punkbuster` (INFERRED from the values
  seen beside those cvars); `hw` and `mod` are not decoded.
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
  strings are new in 1.5b (VERIFIED). The 1.5b server here runs with
  `sv_punkbuster 0` and no PunkBuster files; what a PunkBuster server does
  to a client that sends `cl_punkbuster 0` is not measured.
- HTTP downloads: 1.5b has `sv_wwwDownload`, `sv_wwwBaseURL` and a `wwwdl`
  client command, and logs `Client '%s' is not configured for www download`
  for a client without `cl_wwwDownload` before it falls back to the UDP
  transfer (strings, VERIFIED; the fallback is INFERRED from the message).
  vcod sends no `cl_wwwDownload` and keeps the UDP path.

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
| 0x100000 | none | `disableWeapon`; vcod keeps it in place (`flags::PMF_WEAPON_DISABLED`) for the 1.5 predictor | the method table at file offset 0x82da4 pairs `disableweapon` with 0x408e0, which sets it, and `enableweapon` with 0x40983, which clears it, VERIFIED |
| (FOLLOW without 0x40000) | 0x20000 | forced follow | 1.1's `SpectatorClientEndFrame` (0x40760) sets 0x20000 on the branch where 1.5 clears 0x40000 and 0x80000, INFERRED |

1.1 never sets 0x2000, so the rewrite keeps it in place for the predictor's
1.5 rules; the other 1.5-only bits have nothing in 1.1 to land on and drop.

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
- New: 1628 and 1629, written by `CheckMatchTimeout` (0x4fea8) for the 1.5
  match timeout (`trap_SetConfigstring(0x65c, ..)`, `(0x65d, ..)`, VERIFIED).
  vcod does not read them.
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
  client's `cg_autodemo` or `cg_autoscreenshot` (decompile, VERIFIED). vcod
  ignores them.
- The 1.5b game module sends `C`, `D` and `E` (the string arguments of its
  `trap_SendServerCommand` calls, VERIFIED).

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
  and the 1.5 menus (`serverinfo_dm`) live there.

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

- Prediction on a protocol 6 connection runs 1.5's movement
  (`cod15-movement.md`, "What vcod does with this").

Not done: the 1.5 pmove changes that list leaves out, the match timeout
configstrings, HTTP downloads, PunkBuster, and anything server side.
