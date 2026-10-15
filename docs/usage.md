# Using vcod

Command lines, controls, the console and the menus. The README's
[How to play](../README.md#how-to-play) is the short version.

## The closest thing to a game

```
vcod-server mp_carentan --bots 4 --bots-shoot
vcod --connect 127.0.0.1:28960
```

Pick a team, pick a weapon, go get shot by a bot.

## Client

```
vcod                                   # main menu and server browser
vcod mp_pavlov                         # fly
vcod mp_pavlov --walk                  # walk, offline
vcod --list                            # every .bsp in the search path
vcod --connect <ip:port> --team axis --weapon kar98k_mp
```

- The map name is case-insensitive.
- `--team <allies|axis|autoassign|spectator>` and `--weapon <file>` answer
  the menus without showing them, for `--connect` and every console
  `connect`. If the menu refuses the weapon, you pick by hand.
- `--mod-dir uo` searches United Offensive's `uo/` over the `main/` beside
  it, as UO itself does, so `vcod --game-dir <UO install> --mod-dir uo
  --connect <ip:port> --team spectator` watches a UO server and
  `vcod mp_cassino --game-dir <UO install> --mod-dir uo` flies its maps.
- `--debug-overlay` (or F3) shows frame, draw, vis, net and audio counters.
- `--rtx-on` turns on every beyond-retail extra for one run (`vc_bicubic`,
  `vc_bloom`, `vc_shadows`, `vc_ssao`, `vc_sunShafts`, `vc_softParticles`,
  `vc_hdr`, `vc_grade`, `vc_vignette`, `vc_grain`) without saving them to
  the config. Despite the name, no rays were harmed.
- `--no-audio` runs silent; `--volume <0..1>` sets the master volume
  (`mss_volume`, default 0.8, also on the Sound screen).
- `--net-probe <ip:port>` is the headless probe client; `vcod --help`
  documents its modes.

## Server

```
vcod-server mp_carentan --port 28960 --hostname "my server" --gametype tdm
```

- Binds `0.0.0.0` and answers `getstatus` from anyone, like retail. Keep it
  on a LAN or behind a firewall you control.
- `--gametype` picks the script under `maps/mp/gametypes/` (default `dm`);
  `--max-clients` sets `sv_maxclients` (default 8).
- `--set NAME=VALUE` is retail's `+set`, repeatable:
  `--set scr_friendlyfire=1`, `--set sv_mapRotation="..."`.
- `--set fs_game=<dir>` runs a mod: its directory's paks go over `main/`,
  and clients are told to load it, as retail's `+set fs_game`.
- `--set rconPassword=<pw>` turns rcon on.
- `--set g_password=<pw>` makes the server private, checked as retail does
  at connect, `map_restart` and map change.
- `--set sv_privateClients=N --set sv_privatePassword=<pw>` reserves the
  first N slots for clients that send that password, as retail does.
- `dedicated` defaults to 1, not retail's 2, so dev runs stay off the master
  list. `--set dedicated=2` heartbeats `codmaster.activision.com` every three
  minutes and sends a flatline on Ctrl-C or `quit`.
- `--bots <n>` adds `n` bots in real client slots. The nav graph builds on
  its own thread on the first tick (a second or two on big maps); bots
  wander until it's ready. `--bots-shoot` lets them fight.
- On a patch 1.5 install the server speaks 1.5 (protocol 6) and takes only
  1.5 clients; with `--mod-dir uo` on a UO install it speaks UO (protocol
  22). `--set protocol=1`, `6` or `22` overrides the install.
- `--gametype-script <file>` runs a gametype from disk instead of the paks.
- `--test-entities <n>` adds invisible entities that move on the wire, to
  exercise the entity encoding.
- `--trace` logs every snapshot per client, and once a second the slowest
  tick and how far the loop fell behind its 20 Hz schedule.

## Controls

Click to capture the mouse, Esc to release it.

### Fly mode

| Input | Action |
|---|---|
| W / A / S / D | Move |
| Space / Ctrl | Up / down |
| Shift | Speed boost |
| Scroll | Fly speed |

### Playing

The stock `config_mp.cfg` binds, with the sight on the right mouse button as
`+speed`. `bind MOUSE2 "toggle cl_run"` makes the sight a toggle.

| Input | Action |
|---|---|
| W / A / S / D | Move |
| Space | Stand up; jump when standing |
| C / Ctrl | Crouch / prone |
| Q / E | Lean (held) |
| LMB | Fire (semi-autos fire once per click) |
| RMB | Aim down the sight (held) |
| R | Reload |
| Shift | Melee |
| F | Use: pick up, mount an MG, plant, defuse, respawn |
| 1 / 2 / 3 / 4 | Primary, second primary, pistol, grenade |
| Scroll | Next / previous weapon |
| Tab | Scoreboard (held) |
| T / Y | Chat to everyone / your team |
| V | Quick message menu |
| M | Script menu: team, or weapon once you have a team |
| Esc | Script menu, as retail; its Main Menu tab opens the main menu (Back to Game, Disconnect, Quit) |

A script menu works as in CoD: the cursor shows, the item under it takes
the focus, a click or its digit picks it, Tab and the arrows step the
focus, Esc closes every script menu, and the stock menu sounds play. A
menu keeps the mouse captured and moves its cursor by the mouse's motion,
as CoD does; in a window the console lets the pointer go. The
quick message menus leave the mouse and every key but their digits on the
game. The main menu takes the mouse; Esc or Back to Game
returns to the game. As a spectator, Space rises and C sinks; as on retail,
E (lean right) sinks at 512 and Q (lean left) rises at 256.

### Walk mode

| Input | Action |
|---|---|
| W / A / S / D | Move |
| Space | Jump (no autohop) |
| Ctrl | Crouch (held) |
| Z | Toggle prone |
| Q / E | Lean |
| Shift | Slow walk |
| LMB / RMB | Fire / aim down sights |
| R | Reload |
| 1-7 | colt, thompson, mp40, mp44, enfield, kar98k, scoped kar98k |

### Console

`` ` `` or `~` toggles it; while it's down the game gets no keys. Up / Down
walk history, Tab completes, Page Up / Page Down scroll. In game, a line
without a leading `/` or `\` is chat, as in retail. `;` separates commands.

| Command | Does |
|---|---|
| `connect <ip:port>`, `disconnect`, `reconnect`, `quit` | What they say |
| `say`, `say_team` | Chat |
| `cmd <text>` | Send a raw client command |
| `bind`, `unbind`, `unbindall`, `bindlist` | Binds |
| `set`, `seta`, `toggle`, `<cvar> [value]` | Cvars; `seta` also saves |
| `cvarlist`, `cmdlist`, `echo`, `clear` | The usual |

Anything else goes to the server while connected (`callvote`, `kill`,
`follownext`). Bindable commands and key names are retail's (`+attack`,
`+speed`, `weaponslot pistol`, `MOUSE1`, `MWHEELUP`). The client cvars are
`name`, `cl_run`, `sensitivity`, `m_yaw`, `m_pitch`, `cg_fov`
(cheat-protected, so 80 unless the server runs `sv_cheats 1`), `rate`
(25000; retail's first-run 5000 starves snapshots), `snaps`,
`scr_conspeed`, `mss_volume`, `r_mode` / `r_fullscreen` (applied at start
and by `vid_restart`, which also reloads the menus closed),
`password` and the browser's `ui_netSource`,
`ui_joinGametype` and `ui_browserShow*`. `exec <file>` runs a config from the paks or `main/`,
and `ui_load` reloads the menus off the list `ui_menuFiles` names.
Binds only act while connected.

### Main menu and browser

Mouse to pick, double-click or Enter to join, the arrows, Home, End and
Page Up / Down with the cursor over the list to move the selection, the wheel or the scroll bar to
scroll, Esc to go back. The wheel scrolling a list is vcod's; CoD's wheel
scrolls none. Tab and the arrows no list takes move the menu's focus for
as long as the key is down, as CoD's do. Held keys do not repeat in a
menu, except typing and Backspace in a text field. Enter and the middle
button act on the item under the cursor, as CoD's do. Click Source to cycle Local, Internet and Favorites, and Game Type
to cycle All and the installed game types; right-click steps either back.
On a bind, click or Enter, then press the new key (Esc cancels, Backspace
clears); a third key replaces both of the old ones. Click a text field to
type into it; Enter or Esc finishes, Tab and the up and down arrows move
to the next field. In Start New Server, click Game
Type to cycle it (right-click goes back); the map list scrolls with its bar,
and with the cursor over it the arrows, Home, End and Page Up / Down move
the selection. The Mods list works the same way.

### Everywhere

| Input | Action |
|---|---|
| `` ` `` / ~ | Console |
| F3 | Debug overlay |
| F4 | Culling: on, locked, off |
