# Votes: `callvote`, `vote`, `CheckVote`

How the 1.1 and 1.5 game modules run a vote, what the client draws, and
what two retail runs on this machine showed. vcod's server side is
`crates/server/src/game/vote.rs`, the client's `crates/client/src/hud/vote.rs`;
the gate is `crates/server/tests/votes_ab.rs`.

Modules: `game.mp.i386.so` 1.1d (symbols; addresses are `nm`'s) and 1.5b
(`nm`'s; Ghidra's `game15.so.c` loads it 0x10000 higher, so its `DAT_000889ce`
is file 0x789ce), `cgame_mp_x86.dll` 1.1 and 1.5 (image base 0x30000000),
`ui_mp_x86.dll` 1.1 (0x40000000), `cod_lnxded` 1.5b. Code was read with
`tools/re/annotate_func.py` and the Ghidra exports; strings were read out
of the files at the addresses given.

## 1. The level block

`level` holds the vote at `+0x318` (`voteString`, 0x400 bytes, the console
command a pass runs), `+0x718` (`voteDisplayString`, 0x400, configstring
16's text), `+0xb18` (`voteTime`), `+0xb1c` (`voteExecuteTime`), `+0xb20`
(yes), `+0xb24` (no) and `+0xb28` (`numVotingClients`), in both modules
(the operands of `Cmd_CallVote_f` 1.1 0x47888 and `CheckVote` 0x501f0,
VERIFIED). `level` is zeroed by `G_InitGame`, so a map load or a
`map_restart` drops a running vote without a word (INFERRED from the
memset; the 1.1d run in section 6 shows no `e` line at a restart, VERIFIED).

The voted flag is `ps.eFlags` 0x20000, byte `+0x82` bit 2 of the client
(`or BYTE PTR [eax+0x82],0x2` at 1.1 0x4800b and 0x482ab, VERIFIED).
`ClientSpawn` keeps `eFlags & 0x20008` across a spawn (1.1 0x427d1,
VERIFIED), and only `Cmd_CallVote_f` clears it, on every slot below
`level+0x1e0` (0x47fe6, VERIFIED). Live: the caller's and a voter's
playerstate carry 0x20000 from the snapshot after the command, through a
`kill` and the respawn, until the next call clears it (`ps.eFlags vote
bit` lines of the 1.1d run, 2026-10-14, VERIFIED). vcod mirrors
`GameHost::client_voted` into the sim at end frame.

`numVotingClients` is counted by `CalculateRanks` (0x50c60): every client
with `connected == 2` whose `sessionTeam` (`client+0x217c`, 1.5 `+0x2184`)
is not 3, the spectator team (VERIFIED, 0x50c71..0x50cc9). `CalculateRanks`
is called from `ClientConnect`, `ClientBegin`, `ClientDisconnect` and the
two script client-field setters at 0x418ef and 0x41b07 (relocations,
VERIFIED); vcod counts the same clients when it checks (INFERRED to be the
same number at every check), except its own bots (`--bots`), which never
vote and so would hold every vote short of a pass. Retail has no bots to
skip; the divergence is vcod's choice.

## 2. `Cmd_CallVote_f`

`ClientCommand` compares `ps.pm_type` with 5 at 1.1 0x48909 (VERIFIED).
Read off its control flow, that test returns for a client in intermission
after `score` and before `mr`, so neither vote command runs there
(INFERRED).
Both are dispatched by name, `callvote` and `vote` (0x48e03, 0x48e26,
VERIFIED).

The tests (1.1 0x47888, 1.5 0x43c00). Each string below is VERIFIED at
its address or in a capture. The order is read off the branches (INFERRED):

1. `g_allowVote` 0: `e "GAME_VOTINGNOTENABLED"`.
2. A vote running: `e "GAME_VOTEALREADYINPROGRESS"`.
3. `client+0x215c` at 3 or more: `e "GAME_MAXVOTESCALLED"`. No
   instruction in either module writes `+0x215c` (an `objdump` of both
   finds only this compare, 1.1 0x478cf, 1.5 0x43cab, VERIFIED), so this
   never goes.
4. A spectator: `e "GAME_NOSPECTATORCALLVOTE"`.
5. `;` in `argv[1]` or `argv[2]`: `e "GAME_INVALIDVOTESTRING"`.
6. An unknown kind: `e "GAME_INVALIDVOTESTRING"` then the list,
   `e "GAME_VOTECOMMANDSARE\x15 map_restart, map_rotate, map <mapname>,
   g_gametype <typename>, typemap <typename> <mapname>, kick <player>,
   clientkick <clientnum>"` (1.1 0x73c40); 1.5's adds `tempBanUser
   <player>, tempBanClient <clientNum>, drawfriend <value>, killcam
   <value>, friendlyfire <value>` (1.5 file 0x78a40). VERIFIED, both
   captures.
7. 1.5 only: the kind's own `g_allowVote<Kind>` at 0:
   `e "GAME_VOTINGNOTENABLED"` (decompile, VERIFIED gates by name; the
   1.5b run's `clientkick` under `g_allowVoteClientKick 0`, VERIFIED).
8. A passed vote still waiting runs now: `voteExecuteTime` cleared and
   `voteString` sent to the console (1.1 0x47a45, VERIFIED).
9. The kind's own checks and strings (below); a refusal sends one `e`.
10. `e "GAME_CALLEDAVOTE\x15<netname>"` to everyone, `client+0x21b4` as is
    (VERIFIED, both captures), then `voteTime`, yes 1, no 0, every slot's
    voted bit off and the caller's on, and configstrings 15, 16, 17, 18 in
    that order (1.1 0x48023..0x48076, VERIFIED).

Configstring 15 is `level.time + 30000` on 1.1 (0x47fac adds 0x7530,
VERIFIED; the 1.1d run's 49650 at serverTime 19700, the level time of the
frame before). 1.5 sets `voteTime` from `trap_Milliseconds() + 30000` and
writes `"30000"` into configstring 15 (decompile, VERIFIED; the 1.5b run's
`d 15 30000` on every call, VERIFIED). A configstring set to the value it
already holds is not resent: the second kick vote's 16 and the third
vote's 17 did not come again (1.1d run, VERIFIED).

The kinds. The strings are VERIFIED in the modules. Which check guards
which string is read off the branches (INFERRED):

| Kind | Checks | `voteString` | Configstring 16 |
|---|---|---|---|
| `map_restart`, `map_rotate` | none | the kind as typed | `GAME_VOTE_MAPRESTART`, `GAME_VOTE_NEXTMAP` |
| `map <m>` | 1.5: `trap_MapExists` (vcod: both) | `map <m>` | `GAME_VOTE_MAP\x15<m>` |
| `g_gametype <g>` | `Scr_IsValidGameType` | `g_gametype <g>; map_restart` | `GAME_VOTE_GAMETYPE\x14<name>` |
| `typemap <g> <m>` | valid `<g>`; 1.5 (vcod: both): `<m>` exists; `<g>` equal to `g_gametype` and `<m>` to `mapname` are dropped, both dropped is `GAME_TYPEMAP_NOCHANGE` | `g_gametype <g>; map_restart`, `map <m>` or `g_gametype <g>; map <m>` | `GAME_VOTE_GAMETYPE\x14<name>`, `GAME_VOTE_MAP\x15<m>` or `GAME_VOTE_GAMETYPE\x14<name>\x15 - \x14GAME_VOTE_MAP\x15<m>` |
| `kick <name>` (1.5 also `tempBanUser`) | a `connected == 2` slot of 0..63 whose `Q_CleanStr`'d name matches without case; the last match wins | `clientkick "<n>"` (`tempBanClient "<n>"`) | `GAME_VOTE_KICK\x15(<n>)<netname>` |
| `clientkick <n>` (1.5 also `tempBanClient`) | `strtol`/`atoi` of `<n>`, 0 only from `"0"`, in 0..63 and `connected == 2` | as above | as above |
| 1.5 `drawfriend <v>` | `<v>` equal to `scr_drawfriend`: `PATCH_1_5_DRAWFRIEND_NOCHANGE` | `setdrawfriend <v>` | `PATCH_1_5_VOTE_DRAWFRIEND\x14MENU_ON` for a `1` first, else `MENU_OFF` |
| 1.5 `killcam <v>` | equal to `scr_killcam`: `PATCH_1_5_KILLCAM_NOCHANGE` | `setkillcam <v>` | `PATCH_1_5_VOTE_KILLCAM\x14MENU_ON/OFF` |
| 1.5 `friendlyfire <v>` | equal to `scr_friendlyfire`: `..._FRIENDLYFIRE_NOCHANGE`; not 0..3: `PATCH_1_5_VOTE_ARG_NOTVALID\x15<v>` | `setfriendlyfire <v>` | `PATCH_1_5_VOTE_FRIENDLYFIRE\x14MENU_OFF/ON/REFLECT/SHARED` |

A missing kick target is `GAME_CLIENTNOTONSERVER`, an invalid game type
`GAME_INVALIDGAMETYPE`, a missing map `e "\x15the server doesn't have that
map"` (1.5 file 0x78b80). `drawfriend`'s and `killcam`'s value test asks
for a value equal to both `"0"` and `"1"` before it refuses, so it never
refuses: the 1.5b run's `drawfriend 7` opened a vote (decompile and run,
VERIFIED). `Scr_IsValidGameType` (0x5c900) and the name it hands back
(`Scr_GetGameTypeNameForScript` 0x5c8b4) come from `Scr_ParseGameTypeList`
(0x5c21c): each `maps/mp/gametypes/*.gsc` not starting with `_`, named by
the first token of its `.txt` (`"TDM_TEAM_DEATHMATCH"` in pak5, VERIFIED).

## 3. `Cmd_Vote_f`

1.1 0x48088, 1.5 0x450aa. While the client's teamkill complaint window
(`client+0x216c` past `level.time`) runs, `vote` answers the complaint
instead (`m -1/-2/-3`, a kick at `g_complaintlimit`); only the
`allowComplaint` builtin (1.1 0x43578) opens one and no stock gametype
calls it (grep of the stock paks, VERIFIED), so vcod leaves that arm out.

Otherwise the strings below (VERIFIED) go in this order. The order is read
off the branches (INFERRED):
no vote, `e "GAME_NOVOTEINPROGRESS"`; voted bit set,
`e "GAME_VOTEALREADYCAST"`; a spectator, `e "GAME_NOSPECTATORVOTE"`; else
`e "GAME_VOTECAST"` to the voter, the bit set, and yes or no counted and
its configstring (17 or 18) set. Yes is `argv[1][0] == 'y' ||
argv[1][1] == 'Y' || argv[1][1] == '1'` (0x48120..0x4812f, VERIFIED): `vote
yes`, `vote y` count yes, and `vote Yes` and `vote 1` count no. The 1.1d
run's `vote Yes` and `vote 1` each failed a two-voter vote (VERIFIED).

## 4. `CheckVote`

Called once a frame from `G_RunFrame` after the `ClientEndFrame` loop and
`CheckTeamStatus` (1.1 0x50ae4, VERIFIED call site); 1.5's
`CheckMatchTimeout` follows it (`cod15-protocol-diff.md` 12).

1. A waiting command whose `voteExecuteTime` is below the clock runs:
   `trap_SendConsoleCommand(EXEC_APPEND, "%s\n")` (0x50207..0x50229,
   VERIFIED).
2. No vote: done.
3. Clock at or past `voteTime`: `e "GAME_VOTEFAILED"`.
4. 1.1: the operands at 0x5024d..0x50289 are `n / 2`, the yes and no
   counts and 0xbb8 (VERIFIED). Read off the branches: yes `> n / 2`
   passes, `e "GAME_VOTEPASSED"` and `voteExecuteTime = level.time +
   3000`; else no `>= n / 2` fails; else nothing (INFERRED). 1.5: with
   `m = n / 2 + 1`, yes `>= m` passes, no `> n - m` fails (decompile,
   INFERRED). The two differ only for an odd `n`: three voters fail on one
   no on 1.1 and need two on 1.5.
5. After a pass or a fail: `voteTime` 0 and configstring 15 `""`
   (0x5029f..0x502b3, VERIFIED). 16, 17 and 18 keep their values.

The clock is `level.time` on 1.1 and `trap_Milliseconds` on 1.5 (decompile,
VERIFIED), so a 1.5 match timeout does not hold a vote. The command runs
on the next `Cbuf_Execute`, which splits a line on `;` outside quotes; vcod
splits the same way in `Server::drain_console`.

The 1.5 engine's own commands a 1.5 pass runs (`cod_lnxded` 1.5b
registrations at 0x80887f5..0x808896d, VERIFIED): `setdrawfriend`,
`setfriendlyfire` and `setkillcam` (0x808863e, 0x80886a9, 0x8088714) print
`Usage: %s <integer>\n` without an argument and otherwise `Cvar_Set`
`scr_<x>` and `ui_<x>` to it (the strings at 0x80ddd23..0x80ddd82,
VERIFIED; the 1.5b run's `scr_friendlyfire` read `2` after the pass,
VERIFIED). `tempBanUser` and `tempBanClient` are registered beside them
(0x80887f5, 0x8088809, VERIFIED names) and `sv_kickBanTime` defaults to
300 (cvar capture, VERIFIED). Their handlers are not read here: vcod
kicks and refuses the address for `sv_kickBanTime` seconds, which is
INFERRED from the names alone.

## 5. The client

`CG_ConfigStringModified` (1.1 0x3002c6b0, 1.5 0x3002db40) keeps
configstring 15 as `atol` (1.5: plus trap 6, `Sys_Milliseconds`, when not
0), 17 and 18 as `atol`, each setting a modified flag, and 16 through
trap 0x39 (`"vote string"`, the localizer) into 255 bytes (decompile,
VERIFIED). The gamestate reader (1.1 0x30022f10, 1.5 0x300241f0) reads the
four without the flag (VERIFIED). UO's cgame keeps the same four at 16..19,
time relative (`uo_cgame_mp_x86.dll` 0x30038e70, decompile, VERIFIED).

The draw (1.1 0x30018120, 1.5 0x300191b0), with no teamkill complaint
prompt up: when a vote runs, a set modified flag plays `player_talk`
(0x301d5d78, VERIFIED) and clears; then at x 8, y 200 and 210 in font 0 at
0.2083 (0x3e555555), colour (1, 1, 0, 1), style 3 (VERIFIED operands):

- `"%s(%i):%s"`: `CGAME_VOTE`, `max(0, (voteTime - now) / 1000)` with
  `now` 1.1's `cg.time` (0x30207148) and 1.5's trap 6, the vote string;
- without the voted bit, `"%s(%s):%i, %s(%s):%i"`: `CGAME_YES`, the key
  bound to `vote yes`, yes, `CGAME_NO`, the key bound to `vote no`, no;
  with it, `"%s:%i, %s:%i"` without the keys. An unbound command shows as
  itself (0x30046940 returns 0 and the literal is copied, VERIFIED).

`default_mp.cfg` in `localized_english_pak0.pk3` binds F1 to `vote yes`
and F2 to `vote no` (VERIFIED); vcod's first-run binds carry them.

The stock `ui_mp/scriptmenus/callvote.menu` sends `cmd callvote
map_restart` and `cmd callvote map_rotate` from its buttons, and calls
`uiScript voteMap`, `voteTypeMap` and `voteKick` (pak0, VERIFIED). In
`ui_mp_x86.dll` 1.1 those format `callvote map %s\n`, `callvote typemap %s
%s\n` and `callvote kick "%s"\n` off the selected map, game type and
player row (0x4000af4c..0x4000afee, VERIFIED strings at 0x4002e8fc..
0x4002e938). The player rows come from a UI trap that names each client
slot (0x40008f34..0x40008f83, INFERRED); vcod has no player list feeder,
so its kick pick does nothing.

## 6. The retail runs

Both runs on 2026-10-14: `tools/run_server.sh mp_carentan +set g_gametype
tdm` on 1.1d and on 1.5b (with five `g_allowVote*` gates opened), three
`--net-probe` clients named Able (axis), Baker (allies) and Charlie
(spectator) sending `--probe-say` commands, and rcon lines on the 1.1d
one. The probe prints every vote configstring change with the snapshot
time it arrived at and the playerstate's vote bit. The captures are
`crates/server/tests/fixtures/votes/mp_carentan-tdm-votes.txt` and
`...-votes-1.5.txt`; `votes_ab.rs` replays each on vcod and matches every
line (VERIFIED, both).

What they show beyond the sections above (VERIFIED):

- A vote command answered in the packet arrives ahead of the snapshot its
  frame sends: `e` first, then the configstrings, as the code orders them.
  A `vote` that settles the vote brings its own count, then the frame's
  `GAME_VOTEPASSED`/`GAME_VOTEFAILED` and configstring 15 `""` in the same
  message.
- A passed `map_restart` reached the clients as `n` after
  `GAME_VOTEPASSED` and before the next command; the restart's ambient
  configstring 3 carried `t\35800`, 3050 ms past the pass's snapshot.
- An rcon `map_restart` during a vote drops it silently, yet configstring
  15 keeps its old time (no `d 15` came), so a retail client keeps drawing
  the vote until that time passes; `vote` then answers
  `GAME_NOVOTEINPROGRESS`. vcod does the same: its restart keeps the table
  and starts a new `level`.
- `g_allowvote 0` from rcon refuses the next call at once.
- 1.5: a vote nobody answers fails 30 s after the call (35150 to 65150).
