# vcod

A from-scratch reimplementation of bits of Call of Duty (2003), patch 1.1,
in Rust. A map viewer, a client that spectates and plays on real 1.1
servers, and a dedicated server a retail 1.1 client can join. It reads the
game's own pk3s and speaks the original 1.1 wire protocol, which nobody ever
wrote down, so I dug it out of the binaries.

![mp_pavlov in fly mode](docs/screenshots/fly-mp_pavlov.jpg)

![Walk mode with the kar98k viewmodel](docs/screenshots/walk-mp_pavlov.jpg)

![Spectating a public TDM server](docs/screenshots/spectate.jpg)

## Read this first: it's a toy

vcod is a hobby project. I work on it while poking at a 2003 engine is fun,
and I stop when it isn't.

- **It is not a playable game.** You can run around, shoot people and plant
  a bomb, and some evenings it almost feels like Call of Duty. It is still a
  research rig with a renderer bolted on. Expect missing pieces and the
  occasional soldier living inside a wall.
- **Retail parity is not a goal.** No roadmap, no "1.0". If it ever becomes
  fully playable, that's an accident and I'll take the credit anyway.
- **It doesn't replace your copy of the game.** It needs that copy to run.
- **Don't host real players on it.** No anti-cheat, an rcon that knows a
  couple of dozen commands, and strong opinions about tick order. Your
  regulars deserve a server that was tested on someone other than bots.

If you want to play Call of Duty, play Call of Duty. If you want to watch a
2003 game boot in a window someone rebuilt from the bytes up, stick around.

## What it does

### Map viewer

- Draws any stock or custom map with textures, lightmaps and props. Skies,
  water, fences, foliage and terrain blends run through the maps' own
  Q3-style shader scripts, sun disc and fog included. Lamp glows face the
  camera (`autosprite`), flags flap, and a `.sun` file's flare, glare and
  blind follow the sun's visibility. The map's `corona` entities glow
  around their lamps when seen from below, as retail draws them.
- Culls with retail's cells, portals and occluders. F4 freezes or disables
  culling so you can fly out and see what the camera was drawing.
- Props, players and every other model switch to their coarser LODs with
  distance, zoom and Model Detail, by retail's rule.
- Plays the map's ambient loop.

### Walk mode (`--walk`), offline

- Spawns a soldier on a spawn point with retail movement: gravity, crouch,
  prone, stepping, leaning, wall sliding.
- Seven weapons with their own viewmodels, animations, sounds and reserve
  ammo. Hitscan shots with per-surface impacts and tracers.
- Footsteps on retail's cadence per surface; the landing sound scales with
  fall speed.

### Client

- Opens on the stock main menu and server browser, drawn from the game's own
  `ui_mp/*.menu` files. The browser's three sources are retail's: Local
  (a `getinfo` broadcast to ports 28960-28963), Internet (what
  `codmaster.activision.com` knows) and Favorites (kept in `servercache.dat`
  beside `CoDMP.exe`, the file retail uses). It pings, filters (by game
  type too), shows when each source was last refreshed, sorts by
  column, joins on double-click, and runs the stock password, server info,
  filter and favourite popups. Losing the server drops you back on the menu
  with the localized reason in the stock error popup.
  In a game, Esc opens the script menu and its Main Menu tab the stock
  in-game main menu (Back to Game, Disconnect). The stock Mods menu lists
  the mod directories beside `main/` and switches to one, in a game too
  (not while a server you started with Dedicated No runs, as retail refuses),
  and the menus load from the mod's own `ui_mp/menus.txt` and
  `ui_mp/ingame.txt`, each file tried under `cl_language`'s directory first
  (latched, as retail: a change waits for `vid_restart`).
- Start New Server runs the stock create server menus (game type, the maps
  `mp/*.arena` lists for it, server and game type settings), then starts
  `vcod-server` from the same directory as `vcod` with those settings and
  the Mods menu's mod. The server logs to `vcod-server.log` beside
  `vcod_mp.cfg`. Dedicated No joins it once it answers and stops it when
  the client disconnects; LAN and Internet leave the client in the menus.
  The client kills any server it started when it exits.
- The stock Options and Multiplayer Options screens: rebind keys, mouse
  sensitivity and invert, player name, rate, master volume, video mode and
  full screen, brightness, texture detail, filter and quality, dynamic
  lights, sync, blood and sound quality, applied at the restarts retail
  applies them at. Choices land in the console's binds and cvars and persist in
  `vcod_mp.cfg`; settings vcod has no use for are kept but do nothing.
- Joins a 1.1 server (`--connect` or the browser): handshake, Huffman,
  netchan, delta snapshots, and pak downloads for whatever the server has and
  you don't, matched by checksum as retail does. Pure servers take its pak
  checksums. Modded servers work too: a server's `fs_game` layers its mod
  directory over `main/` and swaps in the mod's menus, its
  `cl_allowDownload` decides whether to download, and a mod's script menus
  and `hud.menu` text show up.
- Watches a United Offensive 1.51 server (protocol 22) with `--mod-dir uo`:
  it switches protocol when the server refuses the 1.1 connect, loads the
  UO map and models from `uo/` over the 1.5 `main/`, and draws the players,
  items, script models, the stock UO script menus, the kill feed and the
  events UO shares with 1.1. Tanks and jeeps draw posed as UO's cgame poses
  them (hull tilt, turret and barrel, the gunner's gun, the jeep's steering
  and front wheels), their treads and tyres scrolling with the vehicle's
  travel, and a seated player rides its seat
  ([coduo-diff.md](docs/research/coduo-diff.md) section 18). UO's own
  events play their effects and sounds (bullet impacts by ammo type,
  artillery, smoke and tank explodes, deploy, fatigue, overheating, the
  gunner's fire, the MG fire loop), the flamethrower streams flame, efx
  camera shakes shake the view, and the HUD adds the sprint and fatigue
  icon, the vehicle status icons and crosshair and vehicles on the compass
  (section 19).
- Draws the server's script menus (team, weapon, view map, call vote,
  quick messages) from their stock `.menu` files with their layout,
  pictures and sounds, driven by mouse and keys as CoD's UI drives them,
  or answers the team and weapon menus straight from `--team` and
  `--weapon`.
- The stock quick message menu (`V`): commands, statements and responses
  go to the gametype script, which plays the voice line and sends the team
  chat line.
- **Spectates.** Every player is an assembled, animated soldier. Kill feed,
  chat, scoreboard, sounds, tracers, impacts, muzzle flashes, ejected
  brass and script effects (the S&D bomb, the bombzone exploders, mines,
  `playLoopedFx`, `playFxOnTag`) come off the same events retail reads.
- **Plays.** Move, jump, crouch, prone, lean, fire, aim down the sight,
  reload, melee, use, switch weapons, on retail's default binds. A usercmd
  is built every 8 ms, like a 125 fps retail client, and they go out at
  retail's `cl_maxpackets` 30 with `cl_packetdup` 1 off the LAN.
- Keeps its own weapon selection as retail's `cg_weaponSelect` does: it
  answers the server's switch and pickup commands, swaps off a dry weapon,
  takes `weapalt` and `weapon <name>`, and keeps asking for the gun a
  ladder took out of your hands. A select shows retail's weapon-slot strip.
- Predicts your movement by replaying unacknowledged cmds through the same
  step the server runs. A correction eases out over 100 ms.
- A script's `shellShock` plays retail's effect from its `.shock` file: the
  view trails and shakes, the mouse damps, the walk slows to 0.4, the mix
  ducks under the shellshock loop. `cg_shellshock <secs> [file]` tries one
  locally.
- Draws and stamps cmds on retail's client clock, which slews a millisecond
  or two per snapshot instead of stepping on jitter; `cl_timeNudge` works.
- First-person weapon with your team's hands, sight zoom at the weapon's FOV,
  sniper scope overlay that follows the sway and the hit kick, and your own
  fire, reload and footstep sounds played off the prediction. The gun bobs,
  breathes, lags a turn and shifts with speed and stance off the weapon
  file's keys, as retail's cgame draws it, and the eye bobs with the walk.
- Fire recoil off the weapon file's view and gun kick keys: the view kick
  rides your cmd angles, as retail's does, rolls the view with it, springs
  back to centre and clears on a respawn.
- A hit kicks the view along its direction, a landing dips it, and a script
  `earthquake` or a mounted MG's fire shakes it, with retail's curves.
- Mounted MG42s with their fire anim and flash; on the gun the view rides
  `tag_player` and the crosshair turns into the gun's reticle.
- The HUD the stock `hud.menu` lays out: crosshair that opens with spread,
  health, ammo, stance, compass with objectives and teammates, use hints, hit
  direction, chat and announcements. Also the gametype script's own HUD
  elements (S&D clock, bomb icons, progress bar) in retail's fonts, and head
  icons over players. A spectator following someone sees their HUD, weapon
  and scope.
- Shows a running vote as retail's cgame does (what it's for, the seconds
  left, the yes and no counts with the keys to press) and plays its sound;
  F1 and F2 vote yes and no, and the call vote menu's restart, next map,
  map and type-and-map picks call one. Its kick pick does nothing yet.
- Follows the server through a map change: loading screen, downloads, new
  map. Open menus close, as in CoD.
- Retail's drop-down console on `` ` ``. It takes the commands under
  [Console](#console); binds and archived cvars persist in `main/vcod_mp.cfg`.
- `vc_bicubic 1` (archived, live, default 0) filters lightmaps with a cubic
  B-spline instead of retail's bilinear, so low-res shadow edges stop
  stepping. A deliberate divergence from retail's look.
- `vc_bloom` (archived, default 0) adds a glow around bright pixels, which
  retail never had: the value is the strength (1 is subtle) and
  `vc_bloomThreshold` (default 0.8) the on-screen brightness where it
  starts. The HUD doesn't glow. Off by default to keep the retail look.
- Beyond retail, off by default: `vc_ssao` (archived, live) adds
  screen-space ambient occlusion to the lightmapped and ambient-lit
  surfaces, its value the strength (1 is subtle, 2 heavier). Retail has
  none; the blends, effects, sky, weapon and HUD are left alone.
- `vc_shadows 1` (off by default, live): players, corpses, entity models,
  moved brush models and waving flags cast soft sun shadows on the world
  and on each other, glass and water included; foliage, fences and
  grilles cast and block through their alpha, blended glass and water do
  not. A deliberate divergence: retail draws none. The baked lightmap
  shadows stay as they are, so a point the map already shadows is not
  darkened again, and a cast shadow is as dark as the map's own baked
  ones measure, per 1024-unit square where the lightmaps say enough
  ([cod11-light-grid-and-leaf-lights.md](docs/research/cod11-light-grid-and-leaf-lights.md),
  section 12).
- `vc_sunShafts` (archived, live, default 0): light shafts radiate from
  the sun through gaps in whatever stands in front of the sky, the value
  their strength (1 is moderate). Only maps whose `.sun` file gives the sun
  a flare cast them; they fade as the sun leaves the screen, feed
  `vc_bloom`, and never cover the HUD. Retail has none.
- `vc_softParticles` (archived, live, default 0): smoke, dust, fire and
  explosion sprites fade out where they meet walls and ground instead of
  cutting a hard line into them. The value scales the fade distance (1 is
  half a sprite's size, at most 32 units). Retail has none; impact marks,
  world shader stages and the HUD are unchanged
  ([cod11-gamma.md](docs/research/cod11-gamma.md), section 8).
- `vc_hdr 1` (archived, takes effect on `vid_restart`, default 0) draws the
  world into a float target, so stacked effects, flares and the sun go past
  white and a tonemap rolls them off instead of clipping; ordinary surfaces
  keep retail's look, and bright overbright ones lose some of their clip.
  With bloom on, only those over-white pixels glow. `vc_grade`,
  `vc_vignette` and `vc_grain` (archived, live, default 0, value is the
  strength) add a period-film colour grade, a vignette and film grain.
  None of them touch the HUD. All are divergences from retail's look
  ([cod11-gamma.md](docs/research/cod11-gamma.md), section 9).

I have played it against my own server and the retail Linux 1.1d server, and
spectated public servers with it. I haven't joined a public server as a
player, and neither should you. Those people signed up for Call of Duty, not
for my test suite.

### Dedicated server (`vcod-server`)

- Answers server browsers, accepts retail 1.1 clients, sends the gamestate
  and delta snapshots. Run from a patch 1.5 install it speaks 1.5 instead
  (protocol 6): 1.5's queries, cvars, menus and gametype scripts, with
  the configstring table 1.5b builds on mp_carentan
  ([cod15-protocol-diff.md](docs/research/cod15-protocol-diff.md) 13).
- Runs Activision's own gametype and map scripts on `vcod-gsc`, a VM for
  CoD's `.gsc` language that lives in this repo. Menus, spawns, scoring,
  rounds and limits all come from the stock scripts, not from Rust. All five
  stock gametypes are checked against retail: `dm`, `tdm` and `sd` end to
  end, `re` and `bel` through their key events
  ([cod11-gametypes-re-bel.md](docs/research/cod11-gametypes-re-bel.md)).
- Shared pmove with capsule players that block and push each other. Falls
  stun and hurt.
- Bullets trace the world, players and static props, through the stock
  damage callback with per-bone hit locations. Rifle rounds go through
  players, every round goes through glass. Melee works. Grenades are real
  missiles that bounce, rest and explode with retail's falloff, and a blast
  walks its victims in retail's area-tree order, so the guy in front still
  eats it for the guy behind.
- Corpses in the eight-slot body queue, dropped weapons, pickups by touch or
  use key, items that fly retail's arc and respawn on its timer.
- Search & Destroy end to end: plant, defuse, progress bar, compass
  objectives.
- Mounted MG42s: mount, aim inside the arc, fire, dismount.
- Map triggers, script movers that carry and shove players and items, and
  `linkTo` on script models, brush models, items, turrets and player tags.
- Intermission, `map_restart` and `sv_mapRotation` the way retail runs them.
- Spectator follow and the killcam, replayed from a ring of archived frames.
- rcon with retail's commands and replies, bans, the master heartbeat,
  zombie slots and retail-measured pings.
- Votes (`callvote`, `vote`) with retail's kinds, refusals, configstrings,
  pass rule, 30 s timeout and 3 s delay before the command runs: 1.1's
  seven kinds under `g_allowVote`, 1.5's twelve under their own
  `g_allowVote*` gates, measured against both retail servers.
- Publishes retail's pak lists and checksums in the systeminfo, serves the
  paks it lists to clients that lack them at retail's pace, and with `--set
  sv_pure=1` checks each client's pak checksums the way a pure retail server
  does. `sv_minPing`/`sv_maxPing` refuse off-LAN clients by challenge ping,
  and an off-LAN client's messages wait out its rate and `snaps`. Big
  messages, the gamestate included, go one fragment per frame, and a client
  not yet in the world gets retail's bare keepalive (every frame on a LAN,
  a second apart off it).
- `--bots` adds bots that join through the stock menus and roam a nav graph
  built from pmove runs, ladders and jumps. They play the objectives: S&D
  plants and defuses, Retrieval carries and escorts, Behind Enemy Lines
  hunts. With `--bots-shoot` they fight back, with a reaction delay, a turn
  cap and aim that settles on target. They are still bad at it. So am I.

## How I know it's right (when it is)

The retail 1.1d Linux server is the oracle. When vcod and retail disagree,
retail wins and the disagreement goes into a research doc.

- A headless probe client (`vcod --net-probe`) joins a server and records
  what it sees, with a few dozen scripted modes: shoot someone, throw a
  grenade, plant the bomb, crawl prone up a hill, get stuck inside another
  player.
- The retail recordings are committed as fixtures. A/B tests replay the same
  inputs on vcod's server and diff the result snapshot by snapshot.
- Small `.gsc` probes run on retail and on `vcod-gsc`, and the suite compares
  their output.

This catches a lot. Anything about how it looks or sounds still needs a human
squinting at a screen.

## What it doesn't do

Retail does a lot more than this list. These are the gaps you'll hit first.

**Front end.** Main menu, browser and its popups, options, quit and error
popups work. The options screens' texture detail, filter and quality,
model detail, dynamic lights, sync, blood, wall marks, ejecting brass and
sound quality take effect as retail applies them; the sound provider and
NVIDIA fog are stored but ignored, and the language picker stays hidden.
Show Compass and Team Overlay do nothing, as in retail 1.1. CD key prints
"not in vcod yet".
There is no listen server: Start New Server always runs a separate
`vcod-server`, Dedicated No joins it as an ordinary client (it needs the
password too), and LAN or Internet does not turn the window into a server
console.

**Client**

- Patch 1.5 servers (protocol 6) are joined by switching protocol when
  one refuses the 1.1 connect. A pure 1.5 server wants a 1.5 install as
  `COD_DIR`
  ([cod15-protocol-diff.md](docs/research/cod15-protocol-diff.md)).
  Prediction runs 1.5's movement there (jump penalty, no prone dive, the
  stepped stance, the prone body's swing search, the footstep changes,
  `disableWeapon`), and the bobs take the server's bob cvars
  ([cod15-movement.md](docs/research/cod15-movement.md)).
  PunkBuster servers refuse it (vcod runs no PunkBuster), and `cg_autodemo`
  and `cg_autoscreenshot` do nothing (no demos or screenshots).
- On a United Offensive server vcod watches; playing there is untested and
  predicts without sprint, fatigue or the bipod. Vehicles kick up no wheel
  dust or tread effects, a seated player keeps the server's anim without
  UO's steering blend, and a gunner you follow aims its gun off the wire,
  not the followed view. A vehicle's MG draws no client-traced impacts or
  tracers, and the flamethrower leaves no burning residue
  ([coduo-diff.md](docs/research/coduo-diff.md) sections 17-19).
- Prediction carries you on a moving brush model but not its rotation.
  Neither does retail's.
- A mod's UI DLL is ignored; only its menu files count
  ([cod11-front-end.md](docs/research/cod11-front-end.md) sections 16-17).
  Switching mods in a game keeps the loaded map's geometry.
- `playFxOnTag` on the body the view rides (your own, or the player you
  follow) draws nothing; on any other entity it plays on the tag.

**Server**

- The server and the client pose a moving body as retail does
  (ground-speed rate, phase, blend times, torso over legs). A tag on an `attach`ed model
  can't take a link.
- A brush model that turned and turned back keeps a sliver of yaw on retail
  and drifts its riders about 0.02 units a frame. vcod's comes back to zero.
  Yes, I'm calling that a bug in retail.
- `sv_pure` defaults to 0 where retail's default is 1, and a client may
  download only the paks the server lists. Retail serves any file under its
  directories, `..` included, so I'm keeping that one on purpose.
- rcon runs `map`, `devmap`, `map_restart`, `map_rotate`, `status`,
  `clientkick`, `kick`, `banUser`, `banClient`, `dumpuser`, `serverinfo`,
  `systeminfo`, `say`, `set`, `seta`, `cvarlist`, cvar queries, `heartbeat`,
  `killserver` and `quit`, and on 1.5 `tempBanUser`, `tempBanClient`,
  `setdrawfriend`, `setfriendlyfire` and `setkillcam`. A temporary ban
  refuses the address for `sv_kickBanTime` seconds; retail's keys it some
  other way I haven't measured. Not `gameCompleteStatus`, `scriptUsage` or
  `stringUsage`. Bans live in vcod (`--ban-file` keeps them across runs)
  where retail hands them to Activision's authorize server. Like retail's, a
  ban refuses an address off the LAN and leaves the banned player connected
  until they leave on their own. After `killserver` the process sits there
  deaf, since there is no stdin console.
- A heartbeat reaches the master, which probes back. I haven't seen vcod
  listed yet.
- A server speaks one patch, 1.1 or 1.5, as each retail binary does. On 1.5
  everyone, bots too, moves by 1.5's rules (as far as the client's
  prediction models them), but the bots' nav graph is proven under 1.1's.
  A 1.5 server runs the match timeout, 1.5's chat (`g_deadChat`,
  `say_squad`) and refuses PunkBuster as the Linux 1.5b server does.
- The teamkill complaint half of `vote` (`allowComplaint`) is not there; no
  stock script opens it.

**Rendering and sound**

- Props are lit per vertex from the map's lights and its light-visibility
  grid at load, as retail does. Dynamic lights add on top with retail's
  falloff instead of competing for a prop's eight light slots. Players,
  other entity models and the viewmodel pick their eight lights from the
  grid every frame, fx lights and the panzerfaust's black rocket light
  among them
  ([cod11-light-grid-and-leaf-lights.md](docs/research/cod11-light-grid-and-leaf-lights.md)).
- The frame holds retail's framebuffer bytes and the gamma ramp doubles
  it once at the end, as retail's hardware ramp did; windowed it draws
  as retail's windowed mode does, with no doubling and lightmaps shifted
  at load ([cod11-gamma.md](docs/research/cod11-gamma.md)). Entity models
  and the viewmodel draw every stage of their material (pickup glow,
  objective pulse, the grenades' environment map) and sort among the
  world's surfaces by material sort, as retail's draw list does. The world
  draws its shaders in retail's sorted order (sort, then first-stage
  images), each shader's stages together, with no depth sorting; effects
  join the same list by sort.
- Impact marks (bullet holes, scorches) clip onto the world's triangles on
  the CPU, take the surface's lightmap, fade over the second half of their
  life and draw at their shader's sort and polygon offset, 1024 pieces at
  most, as the retail cgame and renderer do. A `map_restart` clears them
  with every live effect and sound. `cg_shadows 1` (a cheat, off by
  default as in retail) drops the `markShadow` blob under players 250 to
  1024 units away. Brush models and props take none, as in retail; the
  dlight pass skips them
  ([cod11-events-and-fx.md](docs/research/cod11-events-and-fx.md), section 9).
- `deformVertexes` draws `wave`, `flap`, `autosprite` and `autosprite2`;
  `bulge`, `move`, `normal` and `text` (no stock user) parse and do
  nothing. Entity models and the viewmodel take `skins/` script blocks
  but run no vertex deform (no stock MP entity has one). NV/ATI
  hardware-path stages are dropped, as retail did on cards without them.
  Dynamic lights (muzzle flashes, blasts) light the world through retail's
  dlight pass: the projected dlight image over each opaque surface a light
  reaches, through its `perLight` stages, the copy of its texture or
  `dlightshader`. The ship's deckflag has no file behind it, so a stand-in
  draws ([cod11-shader-scripts.md](docs/research/cod11-shader-scripts.md)).
- Visibility draws a bit more than retail on purpose. Retail assumes nobody
  looks over a cell's walls, and the mp_ship decks disagree.
- Audio follows the engine on paper (falloff, panning, voice pools, stealing,
  ducking) but hasn't been checked by ear against the real game. Wall
  occlusion is a vcod addition.

**Things that look like gaps and aren't**

- No mantling, no swimming. Retail 1.1 MP has neither
  ([cod11-mantle.md](docs/research/cod11-mantle.md)).
- No grenade cooking. The fuse runs in full from the release, however long
  you clutched it.
- No doppler. The 1.1 engine never gives a sound a velocity.
- Quick chat (`vsay`) does nothing on a stock install, same as retail: its
  `.voice` tables only ship in mods.
- Asphalt footsteps are silent in retail because the engine asks for
  `asphalt` and the sound table spells it `asphault`. vcod uses the table's
  spelling, so you can hear asphalt. Twenty-three years late, Infinity Ward,
  you're welcome.

## Requirements

- A purchased copy of Call of Duty (2003) with patch 1.1. This repo has no
  game data. Patch 1.5 ships the same `pak1-4.pk3` and a different
  `pak0.pk3`; a 1.5 install works as the asset source, but its `pak0` is
  not on a pure 1.1 server's list, and a pure 1.5 server refuses a 1.1
  install's. The client speaks 1.1 and 1.5 and watches United Offensive
  1.51 (that install's `uo/` and 1.5 `main/`); the server speaks whichever
  patch its install is.
- Rust 1.90 or newer.
- For the client, a GPU with BC (DXT) texture compression. wgpu picks a
  backend; `WGPU_BACKEND=vulkan|gl|dx12|metal` narrows it. The server needs
  no GPU.
- Linux is the only platform I run. Windows and macOS builds exist and are
  untested, so godspeed.
- An audio device if you want sound. Without one the client warns and runs
  silent.

## Building

Prebuilt Linux amd64, Windows amd64 and macOS arm64 binaries are on the
[nightly release](https://github.com/janost/vcod/releases/tag/nightly),
rebuilt from `master` on every push that touches code. It's a rolling tag:
the assets are replaced in place and the previous build is gone.

```
cargo build --release
```

gives `target/release/vcod` and `target/release/vcod-server`. Both expect to
sit next to `CoDMP.exe` and read the paks from `main/`. Copy them there, set
`COD_DIR=/path/to/CallOfDuty`, or pass `--game-dir`. `--game-dir` beats
`COD_DIR`, which beats the executable's directory.

```
COD_DIR=/path/to/CallOfDuty cargo test
```

Without `COD_DIR` the tests that need game data return early and pass, so a
green run without the game proves nothing about the parsers. CI runs that
way, because it can't ship the game either.

## Usage

### The closest thing to a game

```
vcod-server mp_carentan --bots 4 --bots-shoot
vcod --connect 127.0.0.1:28960
```

Pick a team, pick a weapon, go get shot by a bot.

### Client

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

### Server

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
  1.5 clients; `--set protocol=1` or `--set protocol=6` overrides the
  install.
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

## Documentation

- [docs/protocol-1.1.md](docs/protocol-1.1.md): the 1.1 wire protocol, both
  directions, with every divergence from RTCW/Q3 at the end.
- [docs/research/](docs/research/): about thirty documents on file formats,
  movement, combat, items, turrets, movers, HUD, sound, script semantics,
  the console, the front end, bots and more. Every claim names the module and
  address it rests on and says whether it was verified against a binary or
  capture, or inferred. I'd call this the most useful part of the repo.
- [tools/re/](tools/re/): the scripts that pull tables out of the binaries,
  and the Linux server disassembly notes.
- [AGENTS.md](AGENTS.md): the contributor guide. Layout, measuring against
  retail, and the traps I already paid for.

The code is four crates: `vcod-common` (formats, collision, movement,
protocol; no GPU or audio), `vcod` (client), `vcod-server` and `vcod-gsc`
(the script VM, which depends on nothing else here).

## Some numbers, for fun

As of October 2026:

- about 173,000 lines of Rust,
- about 2,200 tests,
- about 270,000 words of protocol and research notes, longer than most novels
  and with a worse plot, except the twist where grenade cooking turned out
  not to exist,
- one asphalt footstep restored.

## Why

Mostly to see whether it could be done. CoD 1 descends from id Tech 3, and
the Quake III and RTCW sources are public, but the 1.1 wire protocol was
never documented. Every field width, enum order and place Infinity Ward
wandered off from RTCW had to come out of the binaries and be confirmed
against the retail server. Also, watching a 2003 game come back to life in a
window you built yourself is a lot of fun.

### This is an AI-driven project

An AI coding agent wrote most of the code and docs here, under my direction.
I decide what to build, review what comes back, run it against the real game
and the retail server, and do the pixel and by-ear checks the agent can't.
The research docs label each fact verified or inferred so you can tell them
apart. Treat the code as a working prototype, not a reference
implementation.

## Lineage and inspiration

- [Quake III Arena](https://github.com/id-Software/Quake-III-Arena) and
  [Return to Castle Wolfenstein](https://github.com/id-Software/RTCW-MP) by
  id Software, GPL. CoD1 descends from RTCW-MP, and its netcode, movement and
  animation code follow those sources closely. Where vcod ports a routine,
  the comment names the file it came from.
- [ioquake3](https://github.com/ioquake/ioq3), for a cleaner reading of the
  same netcode.
- [CoDExtended](https://github.com/xtnded/codextended), a GPL server
  extension for CoD1 1.1 whose struct layouts were the starting point for the
  netfield tables.
- [cod-asset-importer](https://github.com/mauserzjeh/cod-asset-importer), a
  GPL Blender add-on. The xmodel triangle-strip decoder is ported from it.
- [wgpu](https://github.com/gfx-rs/wgpu),
  [winit](https://github.com/rust-windowing/winit) and
  [kira](https://github.com/tesselode/kira) for graphics, windowing and
  audio.

## Legal

vcod is not affiliated with or endorsed by Activision or Infinity Ward. Call
of Duty is a trademark of Activision Publishing, Inc. This repository
contains no game assets, game code or binaries; it reads the files of a copy
you own. The reverse engineering was done to interoperate with the game's own
files and servers. The research notes document file formats and the network
protocol for that purpose: layouts and addresses recovered from the
binaries, no copied code. The screenshots show art owned by Activision.

Quake III Arena and Return to Castle Wolfenstein are trademarks of id
Software. Their GPL sources, and the other ported code, are credited in
[NOTICE](NOTICE).

## License

GPL-3.0-or-later; see [LICENSE](LICENSE). The network code was written with
the RTCW (GPLv3) and Quake 3 (GPLv2-or-later) sources as reference, so the
project is GPL to stay compatible with them.
