# vcod agent notes

Rust map viewer, client and dedicated server for Call of Duty 1 (2003), patch
1.1. README.md says what it does and doesn't do. This file holds what the code
and configs don't tell you: where the evidence lives, how to measure against
retail, and the traps already paid for.

Retail is the oracle. When ours and retail disagree, retail is right, and the
answer goes into a research doc with the bytes.

## Layout

| Crate | Package | Owns |
|---|---|---|
| `crates/common` | `vcod-common` | formats (bsp, xmodel, xanim, shader, pk3, menus), collision, vis, pmove, weapons, animscript, `net/` (the 1.1 protocol both ways, 1.5 and UO 1.51 as per-connection `net::protocol::Protocol` descriptors, plus the master query) |
| `crates/client` | `vcod` | window, renderer, HUD, fx, audio, `console/` (drop-down console, cvars, binds), `frontend/` (stock main menu, server browser, options screens, Start New Server, the server's script menus), `play/` (join, input, cmd clock, prediction), `probe.rs` |
| `crates/server` | `vcod-server` | `server.rs` (tick), `spectate.rs` (usercmds in, playerstate out), `game/` (script host, combat, entities, links), `bots.rs` + `nav.rs`, `rcon.rs`, `console.rs`, `cvars/`, `bans.rs`, `master.rs` (heartbeat), `area.rs` |
| `crates/gsc` | `vcod-gsc` | the `.gsc` compiler and VM (`vm/interp.rs`, `vm/sched.rs`) that runs Activision's stock gametype and map scripts |

- Dependency rules: `common` imports no wgpu, winit or kira; `gsc` depends on
  neither `common` nor the client. Check with `cargo tree -p vcod-common -e
  normal --depth 1` and `cargo tree -p vcod-gsc -e normal` (only `anyhow` and
  `log`; `vcod-common` is a gsc dev-dependency for the corpus tests).
- Grep these, don't read them whole: `client/src/probe.rs` (11k lines),
  `server/src/server.rs` (10k), `common/src/pmove.rs`, `client/src/renderer.rs`
  (5k each), `server/src/game/script.rs`, `client/src/main.rs`,
  `common/src/collision.rs`, `server/src/spectate.rs` (3k+ each).
  `docs/research/cod11-combat.md` is 300 KB; jump by section.
- Gitignored and local to one machine: `private/` (GPL lineage sources, the
  retail 1.1d Linux server, Ghidra output, old plans), `docs/design/`,
  `docs/superpowers/`, `tmp/` (scratch; probe captures land there).

## Research docs

`docs/protocol-1.1.md` is the wire reference; its last section lists every
divergence from RTCW/Q3, so check it before assuming Q3 semantics.
`docs/research/*.md` hold verified engine facts with binary addresses. Read
the subsystem's doc before touching it, and extend it with what you learn
instead of leaving the fact in a commit message.

| Subsystem | Doc |
|---|---|
| Handshake, configstrings, rcon, heartbeat, zombies, pings, rate, bans, `g_password`, private slots, ping limits, message pacing, downloads | `cod11-server-handshake.md` (Housekeeping section) |
| Patch 1.5 (protocol 6) against 1.1: tables, `pm_flags`, handshake, server commands | `cod15-protocol-diff.md` |
| Patch 1.5 movement against 1.1: jump penalty, stance, lean, step, weapons | `cod15-movement.md` |
| United Offensive 1.51 (protocol 22): wire, configstrings, events, vehicles, sprint, gametypes, assets, scope | `coduo-diff.md` |
| Map change, `map_restart`, `sv_serverid`, rotation | `cod11-map-cycle.md` |
| Shots, damage, death, grenades, blasts, hit locations, shot timing | `cod11-combat.md` |
| Movement, stances, constants | `cod11-mantle.md`, `bsp-ibsp59-format.md` (Movement constants) |
| Player clip, kerbs, stuck | `cod11-player-clip.md` |
| Movers, push, `linkTo` | `cod11-movers.md` |
| Items, pickups, drops | `cod11-items.md` |
| Mounted MGs | `cod11-turrets.md` |
| HUD elems, the script menu handshake, server cvar mirror (CS 140/204) | `cod11-hud-protocol.md` |
| Events and effects, impact marks, module md5s | `cod11-events-and-fx.md`, `efx-grammar.md` |
| Sound | `cod11-sound-system.md` |
| gsc language and object model | `cod11-gsc-language.md`, `cod11-gsc-object-model.md` |
| `re` / `bel` gametypes | `cod11-gametypes-re-bel.md` |
| Spectator follow, killcam | `cod11-spectator-follow.md` |
| Client console, binds, `vcod_mp.cfg` | `cod11-console.md` |
| Main menu, server browser, options screens, Start New Server, script menus in the UI | `cod11-front-end.md` |
| Chat, quick chat | `cod11-chat.md`, `cod11-quick-chat.md` |
| Bots | `bot-navigation.md` (its Build times set the census floors), `bot-objectives.md` |
| Player models, anims, formats | `player-model-anim-system.md`, `xmodel-v14-format.md`, `xanim-v14-format.md` |
| Shader scripts, light grid, prop and entity lighting | `cod11-shader-scripts.md`, `cod11-light-grid-and-leaf-lights.md` |
| Gamma, overbright, `r_intensity` | `cod11-gamma.md` |

### Evidence labels

Every claim in a research doc names the module, the virtual address and the
string or table it rests on, and carries its own label. VERIFIED: read out of
a binary, an asset or a live capture. INFERRED: read off control flow, which
includes instruction order and branch conditions ("then", "followed by",
"when that field is non-null"). One label per claim. A document may open with
a document-level VERIFIED default only when its provenance is uniform and
every exception is labelled; a section-level blanket, including one in a
heading, is never allowed. Docs carry derived facts (offsets, tables, enum
orders), never pasted decompiler output. `crates/common/tests/evidence_labels.rs`
catches two mechanical shapes; its header lists what a reader still has to
check.

## Workflow

- Feature work happens on a branch in a worktree. Master is merge-only: `git
  fetch`, merge `origin/master` into the branch, then `git merge --no-ff` with
  `merge <branch>: <summary>`. Other sessions push to master too.
  Conventional prefixes (`feat:`, `fix:`, `docs:`, `test:`, `perf:`,
  `refactor:`, `style:`, `chore:`).
- Before merging a feature or a fix, reread every line of AGENTS.md and
  README.md that touches what the branch changed (README's feature and
  limitation lists included), and commit a fix on the branch for each one the
  work made stale or wrong. The merge waits until both files match the code.
- Before a commit, run what CI runs: `cargo fmt --all --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Set
  `COD_DIR` for the suite: tests that need paks go through
  `vcod_common::testing::game_fs()` and pass vacuously without it.
- `ci.yml` runs on ubuntu with no game data, so a new test needing paks
  returns early through `game_fs()` or uses `Pk3Fs::empty()`. `nightly.yml`
  rebuilds the rolling `nightly` release on master pushes that touch code
  (docs, tools and `*.md` are path-ignored); `workflow_dispatch` forces one.
- Ignored tests to run when you touch their area: `nav_census` (all stock nav
  graphs, ~3 min: `cargo test -p vcod-server --test nav_census -- --ignored`)
  and the ignored tests in `server/tests/bots.rs`: three mp_ship six-bot
  runs (~1-2 min together), mp_ship's boat, mp_ship's head-on ladder pair,
  mp_ship's hold ladder top and mp_hurtgen's crease (a graph build each,
  ~1.5 min). `cargo run
  --release -p vcod-server --example ship_stalls` is the stall detector
  behind `bot-navigation.md`'s tables.
- The all-maps census tests scope to stock `pak[0-9].pk3`. Map downloads drop
  third-party `zzz_*.pk3` into `main/`, and a census failure on one of those
  is not a regression.
- `Server::new` seeds its RNG with a constant; only `main.rs` passes the clock
  through `Server::with_seed`. Tests that depend on spawn picks rely on that.
- Comments: one line where the code doesn't explain itself (an invariant, a
  unit or sign convention, a workaround and what it works around). Facts that
  live in a doc stay there; the comment is the pointer.

## Running

- Both binaries expect to sit next to `CoDMP.exe` and read `main/` (`uo/`
  over `main/` with `--mod-dir uo`: any base other than `main` sits over
  its sibling `main/`, `pk3::base_dirs`). `COD_DIR` overrides the install,
  `--game-dir` overrides both. `pak1-4` are identical between 1.1 and 1.5
  and `pak0` is not (its checksum differs), so a 1.5 install works as the
  asset source but not on a pure 1.1 server; the RE notes are about the 1.1 binaries. The client
  also joins 1.5 servers (protocol 6, `net::protocol::PROTOCOL_V6`,
  `cod15-protocol-diff.md`) and watches United Offensive 1.51 ones
  (protocol 22, `PROTOCOL_V22`, `coduo-diff.md` 18; run it with the UO
  install as `--game-dir` and `--mod-dir uo`): it connects as 1.1 and
  switches when refused.
  The server speaks one protocol per process, the install's
  (`server::install_protocol`, 1.5's `pak0` checksum) unless `--set
  protocol=` says otherwise; `configstrings_15_ab` runs only with `COD_DIR`
  at a 1.5 install.
- Client modes: `vcod <map>` flies, `vcod <map> --walk` walks offline, bare
  `vcod` opens the stock main menu and browser, `vcod --connect ip:port`
  joins through the stock team menu (`--team spectator` spectates).
  vcod has no listen server: Start New Server spawns the `vcod-server`
  beside `vcod` (build both), and the client kills it when it exits.
- `vcod-server <map>` runs ours. `dedicated` defaults to 1 (no heartbeat,
  unlike retail's 2); `--set dedicated=2` opts in. There is no stdin console:
  admin commands go through rcon, refused until `--set rconPassword=...`.
- `RUST_LOG` drives env_logger (default `info`); `WGPU_BACKEND` narrows the
  backend.
- F3 (`--debug-overlay`) shows frame, draw, vis, shader, fog, audio, camera
  and (online) prediction, net and snapshot counters. The `audio` line reads
  `audio vN plays N miss N cull N drop N steal N N.NNms`: live voices, cues
  started, aliases not found, cues past `dist_max`, cues refused by a full
  pool, voices stolen. The `vis` line reads `vis: <mode> cells n/m soups a/b
  tris c/d props p/q occ o h X.XXms` (occluders built, portals they hid).
  The `draw` line ends `lod a/b/c gone N tris T`: dynamic model
  instances drawn at each LOD, those past their last LOD (not drawn), and
  the triangles drawn.
  The `clock` line reads `clock: behind N  reset N fast N +N -N  extrap N`:
  newest snapshot time minus the drawn time, then `CL_AdjustTimeDelta`'s
  resets, fast and slow adjusts and the frames that reached the newest
  snapshot (docs/protocol-1.1.md, "The client's clock").
  The `ssao` line reads `ssao <vc_ssao|off>  gpu scene X.XXms ao Y.YYms`:
  GPU timestamps around the world passes and the SSAO passes alone,
  smoothed; absent where the adapter has no timestamp queries.
- With `vc_shadows 1` F3 adds `shadow: ents N static Ak/Bk tris cut Ck/Dk
  xN cpu X.XXms gpu static X.XX dyn X.XX mask X.XXms`: casting entity
  surfaces, static caster triangles drawn at the last static redraw (a
  strip of the static layer, or the square around the eye after a load
  or a teleport) of the total, the alpha-tested ones of each, static
  redraws so far, and the CPU and GPU (timestamp) cost of each pass.
  `RUST_LOG=vcod::shadows=debug` logs each timed static redraw's GPU ms
  (`start-over` marks the square) and every 120 readbacks the smoothed
  dynamic, mask and CPU ms.
- `VCOD_NETSIM="ping=100,jitter=20,loss=2"` gives the client a bad link
  without root; `vcod --net-probe ADDR --probe-clock` measures the clock on
  one.
- F4 cycles culling `on -> locked -> off`; `locked` freezes the visible set so
  you can fly out and inspect it.
- Pre-existing noise: a few `vkAcquireNextImageKHR` fence validation errors
  per run on Vulkan, and 360-415 unique ShaderLib warnings per map load
  (tokens vcod skips by design).
- To reproduce a screenshot spot in fly mode, add a temporary
  `VCOD_POS="x y z yaw"` override to the `FlyCamera::new` spawn in `main.rs`
  and remove it before committing.
- Pixel and by-ear checks (aim sign, lean direction, shading, sound) belong to
  the human. Report them as pending.

## Measuring against retail

### Servers

- `tools/run_server.sh [map] [+set ...]` runs the **retail** 1.1d Linux
  dedicated server (setup in the script header). Anything after the map goes
  to the engine verbatim. `COD_VERSION=1.5` runs the 1.5b server instead;
  point `COD_DIR` at a directory whose `main/` holds only the stock 1.5 paks
  (pak0-pakb and the localized ones), since third-party pk3s in a 1.5
  install override the stock gametypes and break the script compile.
  `COD_VERSION=uo` runs the United Offensive 1.51b server (default map
  `mp_cassino`); its `COD_DIR` needs the 1.5 `main/` and the UO `uo/`.
  The net probe joins it with the UO install as `--game-dir` and
  `--mod-dir uo`; `cargo run -p vcod-common --example handshake_probe` is
  the standalone decoder `coduo-diff.md` was written with.
- `tools/run_probe.sh <probe> [map] [+set ...]` boots retail with a
  `crates/gsc/tests/fixtures/semantics/probe_*.gsc` (or
  `client-probes/probe_*`) installed as the gametype and prints its `PROBE`
  lines. `tools/capture_probes.sh > .../retail-captures.txt` reruns the
  top-level probes for `crates/gsc/tests/semantics_ab.rs`. Read that
  directory's `README.md` before writing a probe: six engine behaviours
  dictate a probe's shape.
- `tools/capture_cvars.py <port> <rconPassword>` prints a running retail
  server's cvar registry as the rows of `crates/server/src/cvars/registry.rs`.
- `cargo run -p vcod-server -- <map>` runs **ours**. `--gametype-script
  <file>` runs a gametype from disk (how a client probe runs against ours),
  `--set NAME=VALUE` is retail's `+set`, `--bots N [--bots-shoot]` adds
  clients, `--trace` logs per-snapshot timing.
- Public servers: `codmaster.activision.com:20510` still answers
  `getservers 1 full empty`. A 60-100 s spectator capture during a round
  shows every combat event.

### The probe client

`vcod --net-probe <ip:port>` is a headless client that joins, prints a
per-second snapshot summary, and dumps captures to `tmp/`. Each scripted mode
(`--save-*`, `--probe-*`) is documented on its flag in
`crates/client/src/main.rs` (`vcod --help`): what it does, which fixture it
writes, which gsc probe it pairs with. A multi-shell recipe (gsc probe, target
client, shooter client, `+set` cvars, start order, `--probe-secs`) is in the
header of the fixture it produced.

- Both servers treat loopback and private addresses as LAN and skip rate
  pacing for them. `tools/offlan_capture.sh retail|ours <dir>` runs a
  server and the probe in two rootless network namespaces with public test
  addresses and logs every datagram; `--probe-rate` / `--probe-snaps` set
  the probe's userinfo.
- **Committed fixtures are retail evidence.** Most modes write into
  `crates/server/tests/fixtures/` under a fixed name, so a run against ours
  overwrites the evidence. After one, move the files to `tmp/` and `git
  checkout` the directory. `--capture-tag` names a second run; tagged
  fixtures refuse to overwrite without `--overwrite-fixture`.
- A new mode builds on the probe's helpers, which encode retail's input
  rules: the loop echoes `ps.weapon` in every cmd (a different byte reads as
  a holster request), a switch goes through `WeaponSwitch`, fire through
  `Pulse` (stock rifles and pistols are semi-automatic, so taps at least
  `fireTime` apart), and joining through `JoinProbe`, which re-answers the
  team menu after every gamestate and `n`. Captures from before 2026-09-02
  carry fake putaways at reload, stance changes and jumps.
- Space client commands past 800 ms yourself; nothing on the client enforces
  it. Retail silently drops non-exempt commands (including bare `score`)
  inside the flood window; `team `, `score ` and `mr ` (with the space) are
  exempt.
- An event lives four slots in the ring, so a transient needs a `!trace` per
  snapshot, not a settled sample. Two probes on one server see each other's
  entities, so a two-client question doesn't need two retail clients. The
  entity list is position-dependent (`--probe-pvs`).
- `--save-fixture` / `--save-snapshots` rewrite the parser's byte-exact
  captures in `crates/common/tests/fixtures/net/`. The `*-delta.bin` pair came
  from a lone spectator on `run_server.sh mp_carentan` with
  `SNAP_CAPTURE_TARGET` raised to 400 and the files renamed by hand; the gate
  in `net/snapshot.rs` pins the frame counts, so a refresh hits the same count
  or updates the assertions. `--capture-tag <tag>` writes the pair as
  `*-<tag>.bin` and captures for all of `--probe-secs`; the `*-1.5.bin` pair
  is two spawned probes on `COD_VERSION=1.5 tools/run_server.sh
  mp_carentan`, the `*-uo.bin` pair a following spectator beside an axis
  probe on `COD_VERSION=uo tools/run_server.sh` (recipe on
  `protocol_22_capture_reencodes_and_maps`). The probe's scripted modes
  still read 1.1's tables, so on UO only the join, the roster, the follow
  and the capture are trustworthy.

### The A/B gates

`crates/server/tests/*_ab.rs` replay a retail fixture's inputs on ours and
diff the result. Where a gate tolerates rows, it names them in a `*GAPS`
constant with the research section that explains each. Debug env vars
(`*_REPORT=1`, `*_TRACE=<ct>`, `*_DUMP=<path>`, `SLOPE_FIXTURE=<path>`) are
read near the top of each gate. Gates that need paks skip without `COD_DIR`.
A gate on a client probe whose watcher sleeps before it polls runs the
server alone for `common::PROBE_JOIN_MS` before its client connects, as
retail's client joined late: a body's idle clip runs from the spawn, and a
spawn before the first poll put `linkto_ab`'s bones a quarter second off
retail's (`cod11-movers.md` 15).

## The server tick

`Server::tick_frames` and `replay_moves` (`crates/server/src/server.rs`) follow
retail's `SV_Frame` / `ClientThink_real` / `ClientEndFrame` order, and the
comments at each step cite the section that measured it. The shape: console
lines (a `map` reloads first), bots, pings, clock; every packet's
cmds in arrival order, each one a `ClientThink_real` with its shots traced and
their callbacks run inside the cmd (`cod11-combat.md` 16); menu responses,
the script threads; spawns, weapon, link and sim ops; the entity pass,
items, links, movers, missiles, thinks, temp entity frees and linked clients
one entity at a time by number, each blast walked on its grenade's turn and
each mover's push on its own (14.7); every
slot's end frame, then per slot the aim trace, `commit_pose` and
that slot's turret, whose rounds deliver inside its turn; outgoing commands,
`SV_CheckTimeouts` on the advanced clock (a silent client goes on its sixth
stale frame, a zombie is freed in the zombie pass), snapshots, zombies,
heartbeat. A tick is one `SV_Frame`: `Server::tick_frames` runs every frame
the wall clock owes, clock through end frames, back to back with only the
archive between them, then one round of timeouts and snapshots. `main.rs`
runs the console before it counts what is owed, so a slow `map_restart` is
caught up with a single snapshot (`cod11-map-cycle.md` 4.7).

Moving work across this order makes a snapshot or a later round read a
frame-old field.

`Cx::spawn` runs a script thread before the calling builtin's next
instruction; `Cx::spawn_then` adds `Host::spawn_returned` once that thread
first suspends. Only a `Cx` handed to a builtin (or to `spawn_returned`)
honours either: a spawn through `get_field`, `set_field` or `Vm::with_cx` is
dropped, so test such a builtin through `ScriptRuntime`.

## Traps already paid for

### Wire and netcode

- Protocol 6 (1.5) renumbered the top of `pm_flags`; the decoder rewrites
  it into 1.1's layout (`Protocol::pm_flags_to_v1`), so a `PlayerState`
  never holds the raw 1.5 value. The writer maps it back
  (`Protocol::pm_flags_from_v1`); 1.5-only state (`flags::PMF_MAY_CYCLE`,
  `PMF_MAY_FREE`, `PMF_WEAPON_DISABLED`) sits above 1.1's 19 bits, which a
  protocol 1 wire masks off. 1.5's jump penalty keeps its 0x2000, which 1.1
  never sets, and `predict::from_wire` turns it on under `MoveRules::V1_5`
  only. A 1.5 body's `eFlags` stance follows the eye's stepped target, so
  the predictor reads 1.5's stance off the flags. 1.5 sends `B` where 1.1 sends `n` for a
  `map_restart`, and its gametypes open `serverinfo_<gametype>` before the
  team menu, so a `t` index is not `g_scriptMainMenu`'s.
- Protocol 22 (UO) sends an `eType` 12 entity (a vehicle) through a second
  netfield table behind a select bit after its number; a decoded
  `EntityState` keeps the main table's order whichever table carried it
  (`Protocol::vehicle_field_slots`). Its `bits -100` fields are 16-bit
  angles held as f32 degrees; one equal to its base after quantizing is
  nudged a mantissa bit so a capture re-encodes. UO's `eType` 9..15 are its
  own (`flags::ET_VEHICLE_V22` and kin) and `ET_EVENTS` is 16: ask
  `Protocol::event_of`, never `eType >= ET_EVENTS`. Its events, `eFlags`,
  `pm_flags` and means of death are mapped onto 1.1's at decode, the
  configstring blocks are `Protocol::cs`, never the `CS_*` constants.
- The stock pak set is the protocol's (`Protocol::id_pak_numbers`: 1.1
  `pak0`..`pak8`, 1.5 `pak0`..`pakf`, UO's `pakuo` ones in `uo/` too), not the install's, and a retail
  server streams a stock pak anyone asks for. A 1.5 match timeout freezes
  snapshot serverTime, so the client clock counts snapshots by message
  number, not by time.
- `entityState.weapon` is 1-based into configstring 7. The ammo and clip
  index tables also start at 1.
- `health`, `ammo[]`, `ammoclip[]` sit in the playerstate array blocks, and a
  retail client predicts its ammo counter from the clip it is sent.
- `eventSequence` counts after the write: the new events are `(prev..cur)`.
- The cmd's weapon byte is the client's own selection (`cg_weaponSelect`,
  `play::input::PlayInput::selected`), never an echo of `ps.weapon`;
  `switchToWeapon` and a weapon pickup only send the client `a <weapon>`
  (combat doc 9.8).
- The first usercmd of a message decodes against a base built from the
  playerstate (sight bit = `fWeaponPosFrac != 0`, stance from `eFlags`), not
  the last cmd sent. vcod's writer sends the full branch every cmd for that
  reason.
- `eFlags` 0x8 flips on every spawn; leave it pinned and a client smears a
  respawn from the corpse. A respawn clears the event ring.
- The damage feedback fields never clear; `damageEvent`'s increment is the
  edge.
- A configstring range's first slot comes from its indexer: status icon, head
  icon and script menu scan from 0, localized strings and shaders from 1.
- A player's wire `solid` is packed at link time (after each cmd's move, at
  spawn), never at end frame.
- The netchan scramble keeps `%`. The server reads client commands with the
  server-side string reader; the client reader's `%` mapping garbles every
  later message after a chat line containing one.
- A move message's ack is stamped with the newest frame already sent when the
  packet arrived, not the tick time, or loopback ping reads 21-50 ms.
- On a 1.5 server `sv_time_ms` is the game clock (`level.time`, the
  snapshot's `serverTime`) and `svs_time_ms()` the network's; a match
  timeout (`timescale 0`) holds the first only, so whatever paces, pings
  or times a client out reads `svs_time_ms()` (`cod15-protocol-diff.md`
  12).
- A cmd's clock is clamped to [-1000, +200] ms around the last frame's
  `level.time`, and the entering cmd seeds `commandTime` inside that window.
- Server commands queue until the next snapshot and squash like
  `SV_AddServerCommand` (rule in `docs/protocol-1.1.md`).
- The frame loop sends a fragmented snapshot, download or gamestate
  message one fragment per tick, as retail does, and an off-LAN client
  waits out its rate and its `snaps` interval between messages. A test that
  reads one tick's packets sees only the first fragment of a big message.
  A client not in the world gets a 9-byte bare message on each turn (every
  tick on a LAN, 1 s apart off it), so such a test drains those too.
- A pure server drops a client that enters the world without a valid `cp`
  (pak checksums keyed with the gamestate's `checksumFeed`). Extra
  arguments to `tools/run_server.sh` land after `+map`, so `+set sv_pure 1`
  there leaves `sv_paks` empty yet still drops; pass it before the map to
  test a pure server.
- Cvars: every `L` cvar latches until the next load, so a latched
  `g_gametype` must not reach serverinfo early. `devmap` sets `sv_cheats 1`
  after the load and `map` sets it back. The server ignores an OOB
  `disconnect`; a client honours one only after 3 s without a packet.

### Map cycle

- A map change is pull-shaped: the server sends only the OOB
  `loadingnewmap`, and resends the gamestate when a client's next message
  carries the old serverId. Nothing goes on the reliable stream.
- `map <current map>` is a restart, not a spawn.
- `map_restart` keeps the configstring table and the item registry; only a
  map load clears them. vcod's restart also keeps the compiled scripts
  (`Carry::scripts`); a test that swaps a script mid-level goes through
  `Server::overlay_script`, which drops them.
- `game[]` and `pers[]` survive only `map_restart(1)` / `exitLevel(1)`, and
  entity handles inside `game[]` never survive. `dm` passes 0, `sd` passes 1.
- Time and score limits are script. The engine has no `CheckExitRules`;
  `g_intermissionDelay` and `nextmap` are read by nothing.

### Movement and collision

- Movement constants come from retail rodata. Retail 1.1 MP has no mantling,
  no swimming and no water jump.
- One cmd step for every caller: `crates/common/src/pmove/cmd.rs` (`chop`,
  `player_step`). The server, the client predictor and bots wrap it.
- The player is a capsule. A box sits `15 tan` higher on every grade.
- A spectator collides: a (-8,-8,-8)..(8,8,16) capsule under mask 0x800011
  (no playerclip, no bodies), stepping in the air because its
  `groundEntityNum` reads 0 (protocol doc, "Spectator").
- The ground trace runs origin ±0.25; a test that places a player exactly on
  a face lands allsolid and stuck. Place it 0.125 up.
- No render soup collides: model 0 clips through its brushes and lump 24
  (terrain partitions, patch facet grids). A patch's box is its subdivided
  grid grown by a unit, not its control points.
- Static props and 0x2080 kerb brushes clip bullets, blasts, missiles and
  items, never a moving player. `shot_trace`, `missile_trace` and
  `item_trace` see props, `box_trace` does not.
- A submodel's brushes clip only while its entity is linked. Script `delete`,
  `notSolid` and `_gameobjects` unlink them; a world no script runs on
  applies the rule through `CollisionWorld::unlink_script_brushes`.
- A brush entity with no `*N` (any script `spawn` of one) links inline model
  0 and clips as a copy of the whole world where it stands, pushes like any
  brush model, goes out with `index` 0 and is clipped by client prediction
  (`CollisionWorld::set_world_copy`, movers doc 19-20). Brush bounds are the
  lump's grown a unit (`bsp::Model::clip_bounds`), before the link's own
  unit. A turned copy or submodel is traced in its own frame: the box's
  centre turned, not the origin, so a pitch shifts a player's capsule, and
  the box axial there, not in the world. A capsule meets the axial sides
  as its upright world box, `|turned sphere offset| + r` a side
  (movers doc 21, 22).
- A fall onto a slope slides as far as its cmd split lets it: a cmd that
  starts within the ground trace's reach turns the fall into ground speed.
  An A/B over one replays the client's three cmds a frame, never one
  (movers doc 21, "The slide down the slope").
- A trace whose start is its end is a position test: terrain reads
  `startsolid` only through `Terrain::position_test`. A push's candidate
  and spot tests are retail's box against brushes, not the capsule
  (`pusher_box_trace`, `box_position_test`). A sine mover never stalls: it
  crushes a player it cannot place (movers doc 21).
- `setOrigin` and a teleport link at the unsnapped origin until the next cmd
  relinks.
- Wish speed multiplies the weapon's `moveSpeedScale` and the sight, back,
  strafe and lean scales: a plain carbine run is 224, sighted 89.7.

### Combat and items

- There is no grenade cook in 1.1 MP; "cooking" in the code means held.
  `grenadeTimeLeft` is `fuseTime` at the pullback and 0 at the throw, and the
  fuse always runs in full from release. The fuse rides the fire event's parm
  inside vcod only; the wire ring carries 0, as retail sends.
- The explode rides the missile's own entity (`eType` flips to 0), so an
  `eType == 4` filter drops the explode frame.
- A stock frag bounces off a live player: `fraggrenade_mp` has `damage 0`.
- A blast and the touch pass walk `crate::area::AreaTree`, retail's entity
  area tree, so victim and touch order is link history, not entity order. A
  client links through `GameHost::link_client`, everything else through
  `GameHost::link_entity`; an `origin` write on a non-client relinks it, a
  player `setOrigin`, spawn or death is an unlink then a link (combat doc
  14.7).
- Blast victims are measured per turn, after earlier callbacks ran. A client
  killed this frame keeps `takedamage` until its own end frame.
- A killed gunner keeps the turret until its own end-frame turn;
  `player_die` releases nothing.
- `setPlayerIgnoreRadiusDamage` is a level flag read only by the
  `radiusDamage` builtin.
- The body queue is eight entities at 64..71 with no timer; a corpse lives
  until its slot is reused.
- A temp entity takes its number from `G_Spawn`'s first-in-first-out free
  list like a script `spawn`, rides every snapshot, and is freed on the
  first entity pass more than 300 ms past its event (combat doc 14.7,
  "Entity numbers"). Engine notifies raised in the entity pass (`movedone`)
  wake their waiters on the next frame's threads.
- A `clipOnly` weapon (the frag) has no reserve entry on the wire.
- An item's `count` of 0 means unset; a drop writes -1 for empty.
- A `trigger_lookat` is never touched; it fires off `ClientEndFrame`'s aim
  trace. A `trigger_use` is never touched either (contents 0x200000); it
  fires off the use key's aim pick. A trigger-woken thread runs on the next
  frame's clock.

### Script

- gsc folds case for identifiers, fields, paths and event names but not for
  string values or array keys. A misrouted event name hangs its `waittill`
  silently.
- Unary `!` takes a string (`!"0"` is 1); an `if` on a string is fatal.
- A float prints `%g`-style (`1e-05`, `1.23457e+06`), not Rust's format.
- A BSP key reaches script only if it is in the entity field table or
  `radiant/keys.txt`; anything else is dropped silently.
- A brush model's `.model` reads `""` (its `*N` key goes to `s.index`).
- `setTimerUp` has no `> 0` check, unlike the other three timers; `bel`
  starts with `setTimerUp(0)`.
- Threads due in one frame resume newest-queued first; `wait 0` resumes at
  once; a notify's waiters run after the notifier's step, in start order.
- The script clock is not `level.time`: a thread due at a frame's
  `level.time` runs the frame after, so a `wait 1` from
  `Callback_StartGameType` at 0 wakes at 1050. Between frames the clock is
  the last frame's `level.time`; `Vm::set_time` starts it at a load. It is
  24-bit (`CLOCK_MASK`, wraps about 4.7 h into server uptime), and `wait`
  refuses a negative wait and one of 16777 s or more.
- A callback a cmd raises (damage, fall, `kill`) runs before the frame and
  reads the last frame's `getTime()`; nothing on that path may advance
  `host.level_time_ms`.
- A thread's own `notify` doesn't fire its own `endon`.
- `loadFx` of a new effect after the bootstrap's first `wait` is a script
  error (`GameHost::initializing`), and so is any `precacheModel`,
  `precacheShader`, `precacheString`, `precacheItem` or
  `precacheShellShock`, even of a name already precached; a test that
  precaches late fails there. Menus and icons may precache any time.
  Past that wait `setModel`, `attach`, `setViewModel`, `setShader`,
  `setText`, `label` and the objective icons only find a name already
  indexed (`Allocators::index_precached`), and `statusicon`, `headicon`
  and `shellshock` never index one, so a test that sets one fills its
  configstring first.
- A thread due on the last frame's clock runs before the frame's anim
  update, and a player bone it asks for keeps that pose, with every bone
  above it, for the rest of the frame (`GameHost::dobj_posed`, movers doc
  15, "A bone asked for before the anim update").

### Assets, rendering, sound

- A coarse xmodel LOD ships no `xmodelparts`: its vertex bone ids index
  LOD 0's bones. A model past its last LOD distance is not drawn at all
  (xmodel doc, "LOD selection").
- xanim translation keys are offsets from the bind pose. Root `tag_origin`
  sits at the feet.
- A non-player entity rotates through `AnglesToAxis`: positive pitch is nose
  down.
- A control bone (`back_*`, `neck`, `head`, `pelvis`) turns about the model's
  axes, not its own; about its own Y the body leans sideways. The torso pitch,
  torso yaw and legs yaw swing after the view on both server and client, and
  `tag_origin`'s local tag turns the body by the legs' yaw off the view, so a
  test that poses a turned body must step it to settle (combat doc 16.3, 16.4).
- A player's clip plays at the rate its ground speed gives, starts at a
  phase off the clip it replaces or the level time, and blends 120-250 ms
  (30 into a fire anim) (`vcod_common::animclock`, movers doc 15). Its clock
  runs on level time and the slot number, so a pose A/B replays cmds on
  retail's level times. The client runs the same clock on render time and
  measures the rate on the newest snapshot's time and `trBase`. Both pose
  through `playerpose::pose_player`: the torso and legs clips blend per bone
  as two tree nodes, weighted 1 and 0.01 under a torso anim.
- The 24/30/32 pt fonts span two or three atlas pages per glyph, named in the
  `.dat`. HUD colours are display values, scaled into the framebuffer by
  the material's gen (`colour_scale`) in the HUD pass.
- The scene target holds retail's framebuffer bytes: textures upload
  without sRGB decode and blends run in byte space. The overbright x2 lives
  only in the gamma pass's ramp, so a pass that draws a display value
  scales it by `identityLight` (0.5 full screen, 1 windowed) first.
  `vc_hdr 1` swaps the world's target for a float one that keeps the same
  units and is tonemapped back to bytes before the HUD (`crate::post`).
- Effect shaders live in `fxshaders/` in `pak5.pk3`. Some map paths have a
  leading slash.
- The asphalt sound alias suffix is `asphault`, and vcod uses that spelling on
  purpose.
- Portal walk: a portal's plane faces out of its owning cell. vcod adds three
  over-marking rules in `common/src/vis.rs` for the mp_ship decks.

### Bots

- Nav build: `Prim::Tri` is terrain only, and `under_ground`'s ray counts
  terrain only, or a patch spar overhead makes a ladder foot read as
  underground. Minefields and `trigger_hurt` brushes are excluded.
- A* is resumable under a shared per-tick expansion budget, so a plan can
  take several ticks, and the body may have left the start's floor when it
  lands (`Follower::proved_start`).
- A body on the rungs off the ground sends three to six cmds a tick, bots
  and the graph's walks alike (`nav::tick_times`): the rungs let go 8 under
  a ladder's top, and whether the step-up clears the lip depends on the
  frame length. A walk that steps a ladder some other way proves edges no
  bot can follow.
- On the rungs a strafe held under a full-rate climb never turns the
  sideways pace the grab carried in, and only a released key bleeds it;
  bots let the key go while the body slides away from the rungs' middle.
- A bot backs down a drop the way `Gait::Creep` walks (`Bot::steer`), not
  `Gait::Backward`; re-prove a backed drop with the creep.
- A following bot runs at the graph's 190 units/s: on the ground its keys
  are cut by the weapon's `moveSpeedScale` (`nav::paced_key`), never on a
  jump's tick, in the air or on the rungs. A probe that runs the bot's
  own move holds the same key.
- `PM_CheckJump` refuses a jump inside 500 ms of the last, and the graph's
  walks set off on every edge with one to hand: a bot on a jump edge waits
  on its foot until the jump is ready (`Follower::jump_wait`).

## Reverse engineering

| File | Role |
|---|---|
| `main/cgame_mp_x86.dll` (1.1) | MP client game: event dispatch, effects. The authority for the client. |
| `main/game_mp_x86.dll` (1.1) | MP server game |
| `game.mp.i386.so` (1.1d Linux) | same as game_mp, with full symbols |
| `CoDMP.exe` (1.1) | client netfield tables, sound system, efx renderer |
| `cgamex86.dll` (1.1) | single-player; its `EV_*` ids diverge from 173 up. Wrong module for MP. |

Image bases: `0x30000000` cgame, `0x20000000` game DLLs, `0x00400000`
CoDMP.exe. md5s are in `docs/research/cod11-events-and-fx.md`.

- Ghidra exports live under `private/ghidra/`. Grep the existing `.c` export
  before re-running; a full decompile takes ~15 minutes.
  `tools/re/ExportDecomp.java` makes one: `analyzeHeadless <dir> <proj>
  -import <dll>`, then `-process <dll> -noanalysis -scriptPath tools/re
  -postScript ExportDecomp.java`. `DecompAt.java <hex addr>...` under
  `-readOnly` decompiles just those functions into `decomp_at.c` in about
  a minute, making a function where analysis left a label (a refexport
  slot's target).
- `tools/re/`: `evtab.py` (`EV_*` table), `netfields.py`,
  `dump_field_table.py`, `xref.py`, `dump_script_fields.py`,
  `dump_builtins.py`, `dump_cvars.py`, `dump_itemlist.py`, and
  `annotate_func.py`, which resolves the PIC relocations that hide every call
  and cvar in a plain `objdump` of `game.mp.i386.so`. The `dump_*` game
  module dumpers read 1.1d, 1.5b and UO alike through `gameso.py`, which
  finds each table by symbol or off its lookup function and resolves the
  relocations: a pointer stored in `.data` reads as 0 in the file.
  `net-notes.md` is the server disassembly log.
- Ghidra prints a `fistp` under a `| 0xc00` control word as `ROUND`, and
  that store truncates toward zero; read the `fldcw` before porting one
  (the bob cycle and every `delta_angles` push are such stores). A float
  times a float constant feeding one stays unrounded on the x87 stack, so
  an `f32` product in Rust can land a step off; widen it to `f64`.
- Find dispatch code by string: `CG_EntityEvent:%s`, `CG_EntityPreEvent:%s`
  (bullet impacts live in the pre-event), `fx/impacts/`. Sound-system entry
  points are catalogued in `docs/research/cod11-sound-system.md`.
