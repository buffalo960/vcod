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
  The client plays only servers of its install's game: with `--mod-dir uo`
  it refuses a 1.1 or 1.5 server, without it a UO one, and says which
  flag to use.
- `--vanilla` loads only the stock retail paks of the install's patch
  (`pak0`..`pak8` on 1.1, `pak0`..`pakf` on 1.5, UO's `pakuo*` and its
  `uomappack00` too, and their `localized_*` paks) and ignores every other
  pk3 in `main/` and `uo/`: third-party maps, `zzz_*` mods, downloads. It
  downloads nothing: a server whose map is not stock fails the load with a
  popup naming the server's paks, and on one that only adds files the
  client plays with the stock ones. It ignores `fs_game` (the Mods menu
  lists none), and Start New Server starts a `--vanilla` server. The
  read-only `vc_vanilla` cvar reads 1, and the start-up log line says
  `(vanilla)`.
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
  `--set scr_friendlyfire=1`, `--set sv_mapRotation="..."`. Each is set
  once, before the scripts load; one written after the map
  (`vcod-server mp_carentan --set ...`) runs again once the level is up,
  as retail's `+map x +set ...` does, so it outranks a script's load-time
  `setCvar`. A later level never puts it back.
- `--set fs_game=<dir>` runs a mod: its directory's paks go over `main/`,
  and clients are told to load it, as retail's `+set fs_game`.
- `--vanilla` loads only the stock retail paks, as the client's does, so
  `sv_paks` and `sv_referencedPakNames` name stock paks alone and a
  third-party map does not load. It refuses `--set fs_game`.
- `--set rconPassword=<pw>` turns rcon on.
- `--set g_password=<pw>` makes the server private, checked as retail does
  at connect, `map_restart` and map change.
- `--set sv_privateClients=N --set sv_privatePassword=<pw>` reserves the
  first N slots for clients that send that password, as retail does.
- `--set scr_motd="..."` is the message of the day players read: on the
  team menu (1.1) or the server info screen before it (1.5, UO), in vcod
  and in the retail clients. `g_motd` reaches the client as configstring
  14 (UO 15), which no retail client draws, so it shows nowhere.
- `dedicated` defaults to 1, not retail's 2, so dev runs stay off the master
  list. `--set dedicated=2` heartbeats `codmaster.activision.com`
  (`coduomaster.activision.com` on UO) every three
  minutes and sends a flatline on Ctrl-C or `quit`.
- `--bots <n>` adds `n` bots in real client slots. The nav graph builds on
  its own thread on the first tick (a second or two on big maps); bots
  wander until it's ready. `--bots-shoot` lets them fight; without it
  `--bots` sets `vc_bot_shoot 0`. See [Bots](#bots) for managing them
  while the server runs.
- On a patch 1.5 install the server speaks 1.5 (protocol 6) and takes only
  1.5 clients; with `--mod-dir uo` on a UO install it speaks UO (protocol
  22). `--set protocol=1`, `6` or `22` overrides the install.
- `--gametype-script <file>` runs a gametype from disk instead of the paks.
- `--test-entities <n>` adds invisible entities that move on the wire, to
  exercise the entity encoding.
- `--trace` logs every snapshot per client (its `wall` gap since the last
  tick beside the `frames` that tick ran), and once 20 frames have run the
  loop's ticks and frames, the slowest tick and how far the loop fell
  behind its 20 Hz schedule.

### Bots

The full guide, with the minimum player count, team balance and slots,
is [bots.md](bots.md).

Bots are a vcod extension; retail has none. Each joins like a client,
through the stock team and weapon menus, named `[BOT] <name>` from a pool
of 120 (`crates/server/src/botmgmt.rs`), never two alike. Manage them over
rcon (`--set rconPassword=<pw>`):

| Command | Does |
|---|---|
| `addbot [allies\|axis\|auto] [count]` | Adds `count` bots (1). `auto`, the default, deals them by balance; a named team pins them there. |
| `removebot [name\|all\|count]` | Removes one bot by name (`[BOT] ` optional), all of them, or `count` (1), lowest score first. |
| `listbots` | Slot, team, score and name of each bot, then the counts and cvars. |

The `set`/`seta` cvars below apply within half a second, over rcon or
as `--set`:

| Cvar | Default | Does |
|---|---|---|
| `vc_bot_minplayers` | 0 | Keeps at least this many players, topping up with bots. A human counts once he picks a team; one joining takes a bot's place (off the larger team) and one leaving brings it back. 0 is off. |
| `vc_bot_autobalance` | 1 | In team gametypes (all but `dm` and `bel`) bots join the smaller team and move so the teams differ by at most one. A bot moves only while dead or spectating; humans and pinned bots never move. |
| `vc_bot_shoot` | 1 | 0 leaves the bots walking the map without fighting. |
| `vc_bot_names` | 1 | 0 names new bots `bot<slot>`. |
| `vc_bot_vehicles` | 1 | United Offensive: 0 stops bots getting into vehicles. |

Bots never take a private slot (`sv_privateClients`). `addbot` can fill
the public slots; the floor leaves one free for a joining human.
`removebot` lowers the count `addbot` and `--bots` asked for, but
`vc_bot_minplayers` still tops the server up to its floor.

```
rcon addbot 4
rcon addbot axis 2
rcon set vc_bot_minplayers 8
rcon removebot NadeSpammer
rcon listbots
```

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
| Tab | Scoreboard (held); Page Up / Page Down or the wheel scroll a long one |
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
| `record [name]`, `stoprecord` | Record a demo to `demos/` in the game directory |
| `demo <name>` | Play one back; `.dm_1`, `.dm_2` or `.dm_3`, found without the extension. `timedemo 1` times it, `cl_freezeDemo 1` pauses it, `nextdemo` holds a command to run when it ends |

Anything else goes to the server while connected (`callvote`, `kill`,
`follownext`). Bindable commands and key names are retail's (`+attack`,
`+speed`, `weaponslot pistol`, `MOUSE1`, `MWHEELUP`). The client cvars are
`name`, `cl_run`, `sensitivity`, `m_yaw`, `m_pitch`, `m_filter`,
`cl_freelook` (0 moves with the mouse's vertical motion unless `+mlook` is
held; `m_forward`, and `m_side` under `+strafe`), `cg_fov`
(cheat-protected, so 80 unless the server runs `sv_cheats 1`), `rate`
(25000; retail's first-run 5000 starves snapshots), `snaps`,
`scr_conspeed`, `mss_volume`, `r_mode` / `r_fullscreen` (applied at start
and by `vid_restart`, which also reloads the menus closed; see
[Window and fullscreen](#window-and-fullscreen)), `com_maxfps`
(retail's frame cap, `timedemo` included: `1000 / com_maxfps` whole ms a frame; vcod defaults to 0, no cap past vsync, where retail defaults to 85, about 90 fps),
`password` and the browser's `ui_netSource`,
`ui_joinGametype` and `ui_browserShow*`. `exec <file>` runs a config from the paks or `main/`,
and `ui_load` reloads the menus off the list `ui_menuFiles` names.
Binds only act while connected.

### Window and fullscreen

- `r_fullscreen 0` is a window, `1` a borderless window over the monitor
  the window is on, at the desktop's resolution, `2` exclusive fullscreen:
  the monitor switches to its mode closest to the `r_mode` size (the
  desktop's size when `r_mode` names none). Wayland has no exclusive
  mode, so `2` is borderless there. Alt+Enter flips between windowed and
  `1` and runs `vid_restart`.
- `r_mode` picks retail's 13 sizes (0 is 320x240, 3 640x480, 6 1024x768,
  11 856x480, 12 1920x1200). `r_mode -1` takes `r_customwidth` x
  `r_customheight`, or keeps the window's own size while either is 0,
  vcod's default: `r_customwidth 3440; r_customheight 1440; r_mode -1;
  vid_restart` asks for a 21:9 window or, with `r_fullscreen 2`, that mode.
- `vc_widescreen 1` (default) draws a window wider than 4:3 Hor+: `cg_fov`
  and the zoom fovs are a 4:3 view's width, so 16:9 at `cg_fov 80` is
  about 96 degrees wide and 21:9 about 111, as tall as 4:3, and the HUD's
  side pieces sit at the window's edges. `vc_widescreen 0` keeps retail's
  horizontal fov (a wider window sees less above and below) and the whole
  HUD in the centred 4:3 box. Menus draw in that box either way.

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
