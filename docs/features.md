# What vcod does, and what it doesn't

The long version of the README's summary. Where a research doc measured
something, the bullet links it.

## What works

### Map viewer

- Draws any stock or custom map with textures, lightmaps and props. Skies,
  water, fences, foliage and terrain blends run through the maps' own
  Q3-style shader scripts, sun disc and fog included. Lamp glows face the
  camera (`autosprite`), flags flap, and a `.sun` file's flare, glare and
  blind follow the sun's visibility; the sun shows only on frames the sky
  draws, and `r_drawSun 0` hides it. The map's `corona` entities glow
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
  you don't, matched by checksum as retail does; a 1.5 or UO server's
  `sv_wwwBaseURL` redirect is fetched over plain `http://`, as retail's
  libwww fetches it ([cod15-protocol-diff.md](research/cod15-protocol-diff.md)
  14; `cl_wwwDownload 0` turns it down). A downloaded pak whose checksum is
  not the one the server lists is deleted before anything loads it, and a
  redirected one is asked for again, a later redirect of it refused so a
  1.5 server sends it over UDP. Pure servers take its pak
  checksums. Modded servers work too: a server's `fs_game` layers its mod
  directory over `main/` and swaps in the mod's menus, its
  `cl_allowDownload` decides whether to download, and a mod's script menus
  and `hud.menu` text show up.
- `--vanilla` (client and server) puts only the install patch's stock paks
  on the search path and ignores third-party maps, `zzz_*` mods, downloads
  and `fs_game`; the client downloads nothing under it
  ([usage.md](usage.md)).
- Watches a United Offensive 1.51 server (protocol 22) with `--mod-dir uo`:
  it switches protocol when the server refuses the 1.1 connect, loads the
  UO map and models from `uo/` over the 1.5 `main/`, and draws the players,
  items, script models, the stock UO script menus, the kill feed and the
  events UO shares with 1.1. Tanks and jeeps draw posed as UO's cgame poses
  them (hull tilt, turret and barrel, the gunner's gun, on your own view
  and inside its spans when you man it, the jeep's steering
  wheel, turned four times as far under your own hands, and front wheels),
  their treads and tyres scrolling with the vehicle's travel and their
  wheels kicking up the dust of the ground under them, never over another
  vehicle (`cg_vehicletrails`), and a seated player rides its seat in UO's seat
  blend, the jeep driver's steer included, with the head turned to the
  view as UO's cgame turns it; on 1.5 and UO every body's spine and lean
  ease toward the view as those patches' do
  ([coduo-diff.md](research/coduo-diff.md) section 18,
  [coduo-vehicles.md](research/coduo-vehicles.md) 9). UO's own
  events play their effects and sounds (bullet impacts by ammo type,
  artillery, smoke and tank explodes, deploy, fatigue, overheating, the
  gunner's fire, the MG fire loop), vehicle engines and MG loops crossfade
  as UO's sound blend entities do, tank shells whizz in flight, a
  vehicle MG's rounds draw the impacts and tracers UO's cgame traces
  itself and stop on vehicles, tracers follow UO's length gate and
  `sv_night` width, the flamethrower streams flame that bounces off
  bodies, leaves burning residue and reports its burns on you to the
  server (`fdc`), efx
  camera shakes shake the view, and the HUD adds the sprint and fatigue
  icon, the vehicle status icons and crosshair and vehicles on the compass
  (section 19).
- Plays on a United Offensive server on foot: `+sprint` (ALT, rebindable
  from the controls screen) sprints with UO's gun pose, breaths and
  footsteps, an LMG's sight deploys its bipod on sandbags, at a
  `trigger_mount` or prone, and the sight cooks a pulled grenade.
  Prediction runs UO's movement, 1.5's plus the sprint, the fatigue bar,
  the bipod's locks and yaw cap, the cook and automatic fire, replayed
  against retail captures
  ([coduo-movement.md](research/coduo-movement.md)).
- Drives, rides and fires United Offensive vehicles: the use key gets in
  and out, the weapon wheel changes seats, the stock keys drive, turn a
  tank's hull to the view (SPACE) and fire the main and alternate guns,
  and the camera sits where UO's does (a tank driver's chase camera and
  gunsight, the gunner on its gun with its sway and firing shake, a
  jeep's riders turning with it), with your own body drawn in the seat
  where retail draws it. Prediction holds a seat's view clamps and sight,
  replayed against retail captures
  ([coduo-vehicles.md](research/coduo-vehicles.md)).
- Draws the server's script menus (team, weapon, view map, call vote,
  quick messages) from their stock `.menu` files with their layout,
  pictures and sounds, driven by mouse and keys as CoD's UI drives them,
  or answers the team and weapon menus straight from `--team` and
  `--weapon`.
- Draws retail's scoreboard on Tab and through intermission: the panel
  with the game type, map, host name and address, the objective, the
  team banners leading team first with their totals and average ping,
  rows in the server's order with status icons and your own row lit,
  and the scroll bar with page keys and wheel once it overflows
  ([cod11-hud-protocol.md](research/cod11-hud-protocol.md) 3.1).
- The stock quick message menu (`V`): commands, statements and responses
  go to the gametype script, which plays the voice line and sends the team
  chat line.
- **Spectates.** Every player is an assembled, animated soldier. Kill feed,
  chat, scoreboard, sounds, tracers, impacts, muzzle flashes, ejected
  brass and script effects (the S&D bomb, the bombzone exploders, mines,
  `playLoopedFx`, `playFxOnTag`) come off the same events retail reads.
  Tracers start at the shooter's posed muzzle tag (a mounted gun's own,
  a gunner's stance point), on 1.1, 1.5 and UO alike.
- **Plays.** Move, jump, crouch, prone, lean, fire, aim down the sight,
  reload, melee, use, switch weapons, on retail's default binds. A usercmd
  is built every 8 ms, like a 125 fps retail client, and they go out at
  retail's `cl_maxpackets` 30 with `cl_packetdup` 1 off the LAN.
- Keeps its own weapon selection as retail's `cg_weaponSelect` does: it
  answers the server's switch and pickup commands, swaps off a dry weapon,
  takes `weapalt` and `weapon <name>`, and keeps asking for the gun a
  ladder took out of your hands. A select shows retail's weapon-slot strip,
  UO's with its satchel and binocular slots.
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
  `tag_player`, a hit still kicks it, and the crosshair turns into the
  gun's reticle. On 1.5 and UO the view is held inside the gun's arcs and
  the gun turns with it; on 1.5 the view jitters as the gun fires, on UO
  the eye sways and shakes and a hit does not kick it.
- The HUD the stock `hud.menu` lays out: crosshair that opens with spread,
  health, ammo, stance, compass with objectives and teammates, use hints, hit
  direction with retail's red flash, chat, announcements and the "You killed" centre print. Also the gametype script's own HUD
  elements (S&D clock, bomb icons, progress bar) in retail's fonts, and head
  icons over players. A spectator following someone sees their HUD, weapon
  and scope.
- Shows a running vote as retail's cgame does (what it's for, the seconds
  left, the yes and no counts with the keys to press) and plays its sound;
  F1 and F2 vote yes and no, and the call vote menu's restart, next map,
  map, type-and-map and kick picks call one.
- Follows the server through a map change: loading screen, downloads, new
  map. Open menus close, as in CoD.
- Retail's drop-down console on `` ` ``. It takes the commands under
  [Console](usage.md#console); binds and archived cvars persist in `main/vcod_mp.cfg`.
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
  ([cod11-light-grid-and-leaf-lights.md](research/cod11-light-grid-and-leaf-lights.md),
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
  ([cod11-gamma.md](research/cod11-gamma.md), section 8).
- `vc_hdr 1` (archived, takes effect on `vid_restart`, default 0) draws the
  world into a float target, so stacked effects, flares and the sun go past
  white and a tonemap rolls them off instead of clipping; ordinary surfaces
  keep retail's look, and bright overbright ones lose some of their clip.
  With bloom on, only those over-white pixels glow. `vc_grade`,
  `vc_vignette` and `vc_grain` (archived, live, default 0, value is the
  strength) add a period-film colour grade, a vignette and film grain.
  None of them touch the HUD. All are divergences from retail's look
  ([cod11-gamma.md](research/cod11-gamma.md), section 9).


### Dedicated server (`vcod-server`)

- Answers server browsers, accepts retail 1.1 clients, sends the gamestate
  and delta snapshots. Run from a patch 1.5 install it speaks 1.5 instead
  (protocol 6): 1.5's queries, cvars, menus and gametype scripts, with
  the configstring table 1.5b builds on mp_carentan
  ([cod15-protocol-diff.md](research/cod15-protocol-diff.md) 13). On 1.5
  and UO the entity cull follows those engines: a sounded entity rides
  past the PVS for 300 ms, a looping one while it loops, a sounded
  `script_origin` never, and cull fog drops whatever lies past its far
  distance, matched against live 1.5b and UO runs (same doc, 15).
- Hosts United Offensive 1.51 (protocol 22) from a UO install with
  `--mod-dir uo`: UO's queries, cvars, configstring layout, 63 weapons,
  means of death and event numbering, and UO's stock `dm`, `tdm`, `sd`,
  `ctf`, `dom`, `hq` and `bas` scripts. Players sprint, tire, deploy the bipod and cook grenades by
  UO's rules. An LMG shows the mount hint and deploys on the sandbag
  brushes and on a script's `spawnTriggerMount` as on retail
  ([coduo-movement.md](research/coduo-movement.md) 11). The join, a spawned player's playerstate and a kar98k's
  shots and reload match the retail 1.51b server
  ([coduo-diff.md](research/coduo-diff.md) 20). Artillery strikes,
  smoke grenades, satchel charges, the flamethrower's reported damage and
  mounted MG overheat run UO's rules too; the strike, the smoke, the bipod
  MG, the satchel and the overheat match retail's captures, and a satchel
  can be shot or blasted off (same doc, 21). The four UO
  modes' configstrings match retail's, and so do a flag pickup and capture
  in `ctf`, a flag taken and retaken in `dom`, a radio captured and
  neutralized in `hq` and a base breach in `bas`, HUD, compass and
  scoreboard included. So do the endings: `ctf`'s flag drop and 20 s
  return, `dom`'s domination bonus, `hq`'s wave scoring, `bas`'s bomb
  plant, defuse and blast, and in all four the round end, the score or
  round limit, intermission and the restart `exitLevel` falls back to
  (same doc, 22).
- Hosts United Offensive's tanks and jeeps: they spawn with retail's
  entity numbers and configstrings, the use key boards the one you look
  at, `nextvehslot`/`prevvehslot` change seats, and they drive, aim, fire
  shells and MGs, overheat, sound, take damage and run players over by
  the 1.51 game module's rules, a tank's driver shut against every round
  but his own tank's. Their hulls, posed per bone, stop rounds, grenades,
  shells and blasts; a jeep takes bullets and a struck vehicle a shell's
  whole damage as on retail, a vehicle driven into another shoves it, and
  one wedged seven seconds destroys itself, a wreck's respawn included.
  A seated driver's and gunner's body meets a level ray where retail's
  does, to a quarter unit, with the jeep's steer blend on the driver.
  A Panzer IV's and a Willys's drives replay on ours as retail's to the
  field, and so do the seat changes; a vehicle and its sound blends leave
  a client's snapshots where retail's do
  ([coduo-vehicles.md](research/coduo-vehicles.md) 10-14).
- Runs Activision's own gametype and map scripts on `vcod-gsc`, a VM for
  CoD's `.gsc` language that lives in this repo. Menus, spawns, scoring,
  rounds and limits all come from the stock scripts, not from Rust. All five
  stock gametypes are checked against retail: `dm`, `tdm` and `sd` end to
  end, `re` and `bel` through their key events
  ([cod11-gametypes-re-bel.md](research/cod11-gametypes-re-bel.md)).
- Shared pmove with capsule players that block and push each other. Falls
  stun and hurt.
- Bullets trace the world, players, static props and script models,
  through the stock damage callback with per-bone hit locations. Rifle
  rounds go through players, every round goes through glass. Curved
  patches, terrain and brushes clip a round in the arithmetic of each
  patch's server build, and a patch only from the BSP leaves that list
  it, in retail's leaf order, so a round slips through the seams retail's
  does. Shots,
  swings and throws start from each patch's muzzle (1.1's truncated
  point, 1.5's and UO's bobbed one). Melee works. Grenades are real
  missiles that bounce (off script models, and off a player's posed bones
  at an eighth of their speed), rest (on a player too, falling once he
  leaves on 1.5 and UO) and explode with retail's falloff, and a blast
  walks its victims in retail's area-tree order, so the guy in front still
  eats it for the guy behind. The panzerfaust (and UO's bazooka and
  panzerschreck) fires a rocket that flies, strikes and explodes as
  retail's does.
- Corpses in the eight-slot body queue, dropped weapons, pickups by touch or
  use key, items that fly retail's arc and respawn on its timer.
- Search & Destroy end to end: plant, defuse, progress bar, compass
  objectives.
- Mounted MG42s: mount, aim inside the arc (1.1's 15-degree step and
  view snap, 1.5's and UO's plain clamp), fire, dismount; 1.1d, 1.5b and
  UO captures replayed against ours.
- Map triggers, script movers that carry and shove players and items, and
  `linkTo` on script models, brush models, items, turrets, player tags and
  tags on `attach`ed models.
- Intermission, `map_restart` and `sv_mapRotation` the way retail runs them.
- Spectator follow and the killcam, replayed from a ring of archived frames.
- rcon with retail's commands and replies, bans, the master heartbeat,
  zombie slots and retail-measured pings. `getinfo` and `getstatus` answer
  byte for byte as the three retail servers do, odd challenges included.
- Votes (`callvote`, `vote`) with retail's kinds, refusals, configstrings,
  pass rule, 30 s timeout and 3 s delay before the command runs: 1.1's
  seven kinds under `g_allowVote`, 1.5's twelve under their own
  `g_allowVote*` gates, measured against both retail servers. Bots are
  not counted as voters, and a map vote needs the map on the server on
  1.1 too. A script's `allowComplaint` opens retail's teamkill
  complaint, which `vote` answers and the client prompts for.
- Publishes retail's pak lists and checksums in the systeminfo, serves the
  paks it lists to clients that lack them at retail's pace (on 1.5 and UO,
  `sv_wwwDownload` sends them to `sv_wwwBaseURL` instead, connected or
  disconnected), and with `--set sv_pure=1` checks each client's pak checksums the way a pure retail server
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
  On a 1.5 or UO server the graph is proven under that patch's movement.
  UO bots sprint long straight legs while they have the breath, pick from
  UO's weapon menus and deploy a light machine gun's bipod to fire it, cook
  their frags, throw one only along a flight that clears walls and bodies
  and run from live ones, and play
  the objectives of UO's `sd`, `ctf` (take, carry, return), `dom` (stand
  in the flag zones), `hq` (take and neutralize the radio) and `bas`
  (plant on a breached base and defuse; they can't breach one, since they
  ignore vehicles). The nav graph is rebuilt when the script deletes walls,
  as `bas` does to a breached base.

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
  ([cod15-protocol-diff.md](research/cod15-protocol-diff.md)).
  Prediction runs 1.5's movement there (jump penalty, no prone dive, the
  stepped stance, the prone body's swing search, the footstep changes,
  `disableWeapon`), and the bobs take the server's bob cvars
  ([cod15-movement.md](research/cod15-movement.md)).
  PunkBuster servers refuse it (vcod runs no PunkBuster), and `cg_autodemo`
  and `cg_autoscreenshot` do nothing (no demos or screenshots).
- Prediction carries you on a moving brush model and turns your view with
  its yaw, as retail's does, but never turns your position about it.
  Neither does retail's.
- A mod's UI DLL is ignored; only its menu files count
  ([cod11-front-end.md](research/cod11-front-end.md) sections 16-17).
  Switching mods in a game keeps the loaded map's geometry.

**Server**

- The server and the client pose a moving body as retail does
  (ground-speed rate, phase, blend times, torso over legs).
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
  `unbanUser`, `setdrawfriend`, `setfriendlyfire` and `setkillcam`. A
  temporary ban refuses an off-LAN client for `sv_kickBanTime` seconds by
  address, where retail's keys it on the authorize server's guid; a LAN
  client is only kicked, as on retail. `gameCompleteStatus` reaches only
  the masters it resolved itself, as retail's does; `scriptUsage` and
  `stringUsage` print retail's report with vcod's VM's numbers. Bans
  live in vcod (`--ban-file` keeps them across runs) where retail 1.1
  hands them to Activision's authorize server and 1.5 keys its `ban.txt`
  on that server's guid. On 1.1, like
  retail's, a ban refuses an address off the LAN and leaves the banned
  player connected until they leave on their own; on 1.5 it kicks an
  off-LAN player and does nothing to a LAN one, which has no guid. After
  `killserver` the process sits there deaf, since there is no stdin
  console.
- A heartbeat reaches the master, which probes back. I haven't seen vcod
  listed yet.
- A server speaks one patch, 1.1, 1.5 or UO, as each retail binary does. On 1.5
  everyone, bots too, moves by 1.5's rules (as far as the client's
  prediction models them), and so do the bots' nav graph's walks.
  A 1.5 server runs the match timeout, 1.5's chat (`g_deadChat`,
  `say_squad`) and refuses PunkBuster as the Linux 1.5b server does.
  On 1.5 and UO a prone body's pose, the bend a drop leaves it, and a frag
  that lands on his back match the retail servers' captures
  ([cod15-movement.md](research/cod15-movement.md) 4,
  [cod11-combat.md](research/cod11-combat.md) 12.2).

**Rendering and sound**

- Props are lit per vertex from the map's lights and its light-visibility
  grid at load, as retail does. Dynamic lights add on top with retail's
  falloff instead of competing for a prop's eight light slots. Players,
  other entity models and the viewmodel pick their eight lights from the
  grid every frame, fx lights and the panzerfaust's black rocket light
  among them
  ([cod11-light-grid-and-leaf-lights.md](research/cod11-light-grid-and-leaf-lights.md)).
- The frame holds retail's framebuffer bytes and the gamma ramp doubles
  it once at the end, as retail's hardware ramp did; windowed it draws
  as retail's windowed mode does, with no doubling and lightmaps shifted
  at load ([cod11-gamma.md](research/cod11-gamma.md)). Entity models
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
  with every live effect and sound, empties the objective text and stands
  the player up running, and drops the centre print, the vote lines and
  the hit icons, as `CG_MapRestart` does; the next snapshot then runs the
  respawn reset. `cg_shadows 1` (a cheat, off by
  default as in retail) drops the `markShadow` blob under players 250 to
  1024 units away. Brush models and props take none, as in retail; the
  dlight pass skips them
  ([cod11-events-and-fx.md](research/cod11-events-and-fx.md), section 9).
- `deformVertexes` draws `wave`, `flap`, `bulge`, `move`, `normal`,
  `autosprite` and `autosprite2`, up to three per shader in script order
  as retail runs them; `text` and `syncNormal` (no stock user) parse and
  do nothing. Entity models and the viewmodel take `skins/` script blocks
  and run the same deforms, all but the sprites (no stock MP entity has
  one). A `type@` skin whose template or image is missing draws retail's
  default shader. NV/ATI
  hardware-path stages are dropped, as retail did on cards without them,
  and stages that need three texture units give way to the scripts'
  two-unit ones. `portal` draws no portal view.
  Dynamic lights (muzzle flashes, blasts) light the world through retail's
  dlight pass: the projected dlight image over each opaque surface a light
  reaches, through its `perLight` stages, the copy of its texture or
  `dlightshader`. The ship's deckflag has no file behind it, so a stand-in
  draws ([cod11-shader-scripts.md](research/cod11-shader-scripts.md)).
- Visibility draws a bit more than retail on purpose. Retail assumes nobody
  looks over a cell's walls, and the mp_ship decks disagree.
- Audio follows the engine on paper (falloff, panning, voice pools, stealing,
  ducking) but hasn't been checked by ear against the real game. Wall
  occlusion is a vcod addition.

**Things that look like gaps and aren't**

- No mantling, no swimming. Retail 1.1 MP has neither
  ([cod11-mantle.md](research/cod11-mantle.md)).
- No grenade cooking on 1.1 or 1.5. The fuse runs in full from the
  release, however long you clutched it. United Offensive has one, and vcod
  does it there.
- No doppler. The 1.1 engine never gives a sound a velocity.
- Quick chat (`vsay`) does nothing on a stock install, same as retail: its
  `.voice` tables only ship in mods.
- Asphalt footsteps are silent in retail because the engine asks for
  `asphalt` and the sound table spells it `asphault`. vcod uses the table's
  spelling, so you can hear asphalt. Twenty-three years late, Infinity Ward,
  you're welcome.
