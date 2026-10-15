# Bot objectives (S&D, retrieval, Behind Enemy Lines, United Offensive)

How vcod's debug bots (`--bots`) play stock Search & Destroy, Retrieval
(section 4) and Behind Enemy Lines (section 5), what changes on a
United Offensive server (section 6), and UO's own `ctf`, `dom`, `hq` and
`bas` (section 7). The brain is
`crates/server/src/bots.rs` (`ObjTarget`, `think_objective`); the server fills
`BotView::sd` and `BotView::linked` in `crates/server/src/server.rs`
(`bot_sd`, `site_stand`) from `ScriptRuntime::sd_objectives`
(`crates/server/src/game/script.rs`). Navigation is in `bot-navigation.md`.

## 1. What stock `sd.gsc` asks of a player

Line numbers are `maps/MP/gametypes/sd.gsc` in `pak5.pk3`.

- VERIFIED (`sd.gsc` `bombzones`, 1797-1814): the sites are the two
  `trigger_multiple` entities with targetname `bombzone_A` and `bombzone_B`;
  `level.planttime` is 5 and `level.defusetime` 10 (seconds). The defuse
  trigger is the entity with targetname `bombtrigger`, sent 10000 units down
  by `_utility::triggerOff` (`maps/MP/_utility.gsc` 134-141).
- VERIFIED (map scripts, `maps/MP/mp_*.gsc`): each map sets
  `game["attackers"]` and `game["defenders"]`. `mp_pavlov` has axis attack,
  `mp_carentan` allies. VERIFIED (grep of `sd.gsc`): the gametype only
  mentions the two assignments in its header comment (51-52) and never
  writes either key, so the sides hold for the whole map.
- INFERRED (`bombzone_think`, 1816-1935, branch conditions): a plant starts
  when a player whose `pers["team"]` is the attackers' touches a zone, is on
  the ground and holds use, while the other zone has no `.planting`. The
  script then `linkTo`s the planter to the zone and counts 0.05 s per frame
  while use stays down; a release or a death before 5 s aborts and unlinks.
- INFERRED (same function, the success arm): a finished plant deletes both
  zones, spawns `level.bombmodel` at `getPlant()` (the planter's feet), moves
  `bombtrigger` to the model's origin and sets `level.bombplanted = true`.
  `bomb_countdown` (1956) runs the 60 s fuse; the explosion deletes
  `bombtrigger` (1956-1990).
- INFERRED (`bomb_think`, 1993-2085, branch conditions): a defuse starts when
  a defender on the ground has `isLookingAt(bombtrigger)`, stands under 64
  units from its origin and holds use. It links the defuser the same way and
  needs use held 10 s; success deletes `bombtrigger`. The progress loop does
  not test `isLookingAt` again, so only the start needs the aim.
- VERIFIED (measured on ours, `mp_rocket`): the moved defuse trigger's bounds
  are centred 8 units above its origin, which is the point the bot aims at.

## 2. What the server tells the brain

- `linked`: `sim.link_to` is set, which is the plant's and defuse's
  `linkTo`. A linked body cannot move, so the brain does not count it stuck.
- `sd`, only while the level's `g_gametype` is `sd`, read fresh every frame
  because a `map_restart` respawns the zones under new handles:
  - `role`: the bot's `sessionteam` against `game["attackers"]` /
    `game["defenders"]`;
  - `sites`: each zone still standing, its absolute bounds and a standing
    point: the nav graph node inside the bounds (16 units in from the sides)
    nearest the middle, from the largest strongly connected component, so a
    bot anywhere on the main graph can path to it. Cached per bounds for the
    life of the graph;
  - `bomb`: once `level.bombplanted` and while `bombtrigger` exists, its
    origin and the middle of its bounds. `None` again after a defuse or the
    explosion.

## 3. The brain

Goal order: a plant or defuse in progress (use held, or linked), a visible
enemy, objective travel, then the rest. Use is pressed only at the
objective, since its rising edge also picks up weapons and mounts turrets.

- Sites are dealt out by rank: a bot's place among its team's bots by slot
  (`SdView::rank`), so a team's bots go A, B, A, ... on both sides.
- Attacker before the plant: its site, moved on by one for every failed
  try in a life. Inside the zone (8 units in) it stops, looks 60 degrees up
  and holds use. Held 20 ticks with no link, or 6 s in all, counts as
  failed: it lets go for 10 ticks and goes to the other site.
- The upward look is for the press's rising edge, which also takes the item
  or turret `G_GetActivateEnt` picks (`cod11-items.md` 2.1). INFERRED (from
  `activate_score`'s 128-unit reach and 0.76 cone, about 40 degrees): an
  item lying on the floor within reach sits more than 30 degrees below
  level, so a view 60 up leaves it outside the cone, and a weapon dropped in
  the zone is no longer swapped for the planter's.
- Attacker after the plant: stands 96 to 250 units from the bomb. A body on
  the bomb would block a defender's `isLookingAt`, and the planter starts
  on it.
- Defender before the plant: goes to its site's standing point and holds
  within 250 units of it, looking about.
- Defender after the plant: only the lead, its team's playing bot nearest
  the bomb (lower slot on a tie, `SdView::lead`), goes for the defuse. The
  rest stand 96 to 250 units off the bomb like the attackers: defenders
  crowding the bomb stood in each other's line from eye to trigger. A dead
  lead hands the defuse to the next nearest on the next frame.
- The lead walks to the bomb, stops within 48 units, turns onto the aim
  point at the normal turn rate, and presses use once the view has sat
  within 1 degree for two ticks (the aim trace is read a frame late,
  `cod11-gsc-object-model.md` 23.1). It holds 10.5 s, or lets go after 20
  ticks with no link and tries again.
- A death sits out the round. VERIFIED (`maps/MP/gametypes/sd.gsc`
  `Callback_PlayerKilled` 756, `spawnSpectator` at 856; `re.gsc` 838, 973,
  `pak5.pk3`): after the 2 s `wait`, the dead player gets the killcam or a
  spectator spawn, never `spawnPlayer`. So on an `sd` or `re` level a bot held in a crease
  (`bot-navigation.md` section 3, "mp_hurtgen's crease") does not send the
  `kill` it sends elsewhere (`BotView::one_life`): held there it is alive,
  can still shoot what comes into view and keeps its team in the round
  (`bots::tests::a_body_held_in_the_air_asks_for_a_kill`). `bel` respawns
  (`bel.gsc` `Callback_PlayerKilled`, `respawn` at 681-757), so it keeps
  the kill.

VERIFIED (measured, `crates/server/tests/bot_objectives.rs`, 2 bots, shoot
off, seed 7, `mp_carentan`): the attacker plants at tick 733 of the run,
counting the match-start restart; the defender defuses 225 ticks later.
Sweeps over seeds 1, 2 and 7 with shoot off, 4 bots on `mp_carentan` and
`mp_rocket` and 6 on `mp_harbor` and `mp_dawnville`, all planted and
defused, the defuse 221 to 234 ticks after the plant. With 6 shooting bots
on `mp_carentan`, seed 1 planted and defused; seeds 2 and 7 planted and
the round ended with every defender dead. `mp_chateau` and `mp_ship` carry
no `bombzone`, `bombtrigger` or S&D spawn entity (VERIFIED, ents lump
strings), so stock S&D does not run there.

## 4. Retrieval (`re`)

### 4.1 What stock `re.gsc` asks of a player

Line numbers are `maps/MP/gametypes/re.gsc` in `pak5.pk3`.

- VERIFIED (`retrieval`, 1878-1886, and `retrieval_spawn_objective`,
  1922-1979): the objectives are the `script_model` entities with
  targetname `retrieval_objective`, gathered into
  `level.retrieval_objective`. Each one's `trigger` field is the
  `trigger_use` it targets, moved onto one of its `mp_retrieval_objective`
  spots, and its `goal` field the `trigger_multiple` it targets.
- VERIFIED (`retrieval_think`, 1981-2042): the pickup is the `trigger_use`'s
  `trigger` notify, which the use key raises through the aim pick
  (`cod11-gametypes-re-bel.md` 4.2). A player of `game["re_attackers"]`
  takes it, and `other.hasobj[self.objnum]` is set to the objective.
- VERIFIED (`hold_objective`, 2044-2073): a taken objective's trigger goes
  10000 units down (`triggerOff`, 2573-2576) and `re_pickup` is logged.
- INFERRED (`objective_carrier_atgoal_wait`, 2075, branch conditions): the
  delivery is the goal's `trigger` notify with the carrier as `other`; any
  other player's touch is let through and ignored.
- INFERRED (`holduse`, 2434-2534, branch conditions): a carrier holding use
  0.3 s is linked in place, and 2 s drops the objective.
- VERIFIED (`objective_think`, 1888-1911, and the default at 184-185): each
  objective and each goal gets a compass record with no team. With
  `scr_re_showcarrier` "0", the default, a pickup sets the objective's
  record to `game["re_attackers"]` and a drop sets it back to none, so the
  defenders' compass loses it while it is carried. With any other value
  the record stays on both compasses.
- VERIFIED (`hold_objective`, 2062; `retrieval_think`, 1983-1984): the
  record follows the carrier (`objective_onEntity`), and the next
  `retrieval_think` after a drop pins it with `objective_position`, which
  detaches it (`cod11-gametypes-re-bel.md` 7.4). While it is carried the
  record's origin still reads where the objective last lay.
- VERIFIED (`Callback_PlayerKilled`, 853-872, and
  `drop_objective_on_disconnect_or_death`, 2617-2760): a carrier's death
  drops the objective without clearing his `hasobj[objnum]`. Only the
  use-key drop (`drop_objective`, 2302-2308) and the capture (2159-2161)
  clear it. The dead carrier's `hasobj` therefore still names the
  objective while it lies there and after the next attacker takes it.
- VERIFIED (`objective_timeout`, 2422-2432): an objective left lying 60 s
  goes back to its start. VERIFIED (`retrieval_think`, 2021-2037): a
  defender's use only prints a message; defenders cannot return it.
- VERIFIED (measured on ours, `mp_dawnville`): the two goal triggers there
  are 500 by 368 units. A defender standing in one fired it every frame it
  was armed, before the carrier's touch (slot order) could, and the carrier
  stood in the goal for 85 s without delivering. INFERRED (Q3 lineage
  `multi_trigger`): a `trigger_multiple` waiting out its `wait` ignores
  every touch, so the same holds on retail.

### 4.2 The brain

`BotView::re`, filled on an `re` level from `ScriptRuntime::re_objectives`:
the bot's role, each objective's pickup trigger middle while it lies there,
its goal's standing point (`site_stand`, as for a bombzone) with the
distance at which a body is clear of the goal trigger, who carries it, the
carrier's position when the bot's team's compass shows the record, and the
record's own origin (where it last lay).

The carrier is the client the compass record follows (`entNum`), not the
one whose `hasobj` names it. Read off `hasobj`, a carrier who died kept
the objective in the bots' view: while it lay, the defenders guarded the
goal instead of it, and once a teammate with a higher slot took it, that
teammate was not told it carried and stood outside the goal.

- Attacker carrying an objective: walks to its goal and never presses use.
- Attacker otherwise: walks to the nearest objective lying there, stops
  within 48 units, turns onto the trigger's middle, and once the view has
  sat within 1 degree for two ticks taps use for two ticks (under
  `holduse`'s 0.3 s). VERIFIED (measured, `mp_dawnville`): from the side
  the graph led to, the eye's trace to the trigger's middle met the world,
  so each tap that took nothing moves the next try 40 units round the
  objective, eight sides in turn.
- Attacker with a teammate carrying: escorts him, holding within 250
  units and closing again when he moves on. Its compass always shows him.
- Attacker with nothing to pick up and no carrier on its compass, and the
  first defender by rank with one carried: a ring round its goal from just
  clear of the trigger to 150 units past.
- Other defenders with one carried: at the carrier while the compass shows
  him (`scr_re_showcarrier` set). Under the default they walk to where the
  objective last lay, once per carry: the carrier set out from there for
  the goal, so the walk runs his likely way backwards. Then the goal ring.
- Defender with nothing carried: guards an objective by rank, within 250
  units of its trigger. A dropped objective counts as lying there.

VERIFIED (measured, `crates/server/tests/bot_objectives.rs`, 2 bots, shoot
off, seed 7, `mp_carentan`): the attacker picks up at tick 483 and delivers
377 ticks later. The same run on each stock map for 150 s: 10 of 12 pick
up and deliver. On `mp_depot` the attacker never reaches the objective,
which lies on an upper floor at z 148; on `mp_hurtgen` every round ends in
an allied win within 18 s and nothing is picked up.

VERIFIED (measured, 2026-10-07, same run): `mp_hurtgen` now picks up at
tick 1108 and delivers at tick 2275. Its rounds ended because the lone axis
bot wandered into the minefields round its spawn (`MOD_EXPLOSIVE`, killer
`world`), and it wandered because the objective's bunker was not on the
graph (bot-navigation.md, "Spacing", "The flood" and section 3).
`mp_depot` still picked nothing up then. Its documents lie on a crate top
at z 148, and the nearest the graph got was a step 32 units below at
(-1852, -608, 116), across a gap the flood never jumped. VERIFIED (measured,
2026-10-07, `tests/bot_objectives.rs`, same run): with leaps
(bot-navigation.md, "Jumps") the attacker comes to rest on the step, leaps
onto the crate, picks up at tick 814 and delivers 224 ticks later.
VERIFIED (measured 2026-10-12, same test): with the leap's lip read off
the tick's move (bot-navigation.md, "Jumps"), at tick 483 and 272 ticks
later; the two leaps before it that walked off the step are gone.

With 6 shooting bots, `mp_carentan` delivered and `mp_harbor` picked up
twice in 150 s and delivered neither.

VERIFIED (measured, ours, 2026-10-08, release, 6 shooting bots, seeds 3
to 6, 200 s per run, the carrier read off the compass record and the
escort and chase above, against the same runs on the code before them):
`mp_carentan` picked up 17 times and delivered 9 (before: 24 and 1);
`mp_dawnville` 19 and 0 (23 and 0), `mp_harbor` 4 and 0 (4 and 0),
`mp_brecourt` 10 and 0 (10 and 0). Kills were 55 to 66 per map in both.
On the three maps that never deliver the carrier dies on the way; the
rounds end on eliminations.

## 5. Behind Enemy Lines (`bel`)

### 5.1 What stock `bel.gsc` asks of a player

Line numbers are `maps/MP/gametypes/bel.gsc` in `pak5.pk3`. The team swap
itself was measured against retail in `cod11-gametypes-re-bel.md` 8.

- VERIFIED (150, 283-310): the team menu is `team_germanonly`, and the
  menu response handler has an `axis` and a `spectator` case and no
  `allies` one. A player only ever joins axis; the script picks who plays
  allied. A bot that answered the menu with `allies` stayed a spectator.
- VERIFIED (`Set_Number_Allowed_Allies`, 1329-1352): `level.alliesallowed`
  is 1 for up to 3 axis players, then one more per 3, up to 11 past 30.
  `CheckAllies_andMoveAxis_to_Allies` (1265-1327) moves random players
  across to keep the allied count there.
- INFERRED (`Callback_PlayerKilled`, 634-760, branch conditions): an axis
  player who kills an allied one becomes allied on his respawn 2 s later
  (`move_to_allies`) while there is room, and the victim respawns axis.
  An allied player who kills an axis one scores a point. An allied death to
  himself or the world moves him to axis and an axis player across.
- INFERRED (`make_obj_marker`, 1563-1606, and `give_allied_points`,
  1608-1628): an allied player scores one point on spawning and one every
  `scr_bel_alivepointtime` seconds (10) while alive, and heals 3 health a
  second under 100. With `scr_bel_showoncompass` 1 (the default, 96-97),
  each one carries objective slot `entnum + 1`, team axis: the axis compass
  shows it. INFERRED (`updateScriptCvars`, 1167-1265, `wait 1` and the
  `update obj` notify; 1588-1603): every `scr_bel_positiontime` seconds (6)
  the marker moves to the mean of where it was and where the player
  stands, so it trails him.

### 5.2 The brain

`BotView::bel`, filled on a `bel` level: whether the bot is allied (the
hunted side), and the live objective records its team's compass shows
(`GameHost::objectives` filtered as `objectives_for` does), which on the
axis side are the allied players' markers.

- Every bot answers `team_germanonly` with `axis`.
- A visible enemy comes first on both sides, as everywhere.
- Hunted (allies): with no enemy in sight, `Goal::Away` from the last
  enemy it saw or the gunfire it heard (`Bot::recall`, else the tick's
  noise), else from its own marker (record `entnum + 1`), else `Roam`. It
  never walks toward a noise. Its compass does not show the marker; the
  bot reads it because a player knows the hunters walk to where he was,
  and moving off it keeps the marker trailing. The points come from
  staying alive.
- Hunters (axis): a remembered or heard spot first, else the nearest
  marker it has not yet stood at, else `Roam`. The marker sits still for
  6 s, so a hunter that reached it looks round from there (the roam)
  instead of standing on it, and heads for it again once it moves.

VERIFIED (measured, ours, 2026-10-08, `mp_brecourt` `bel`, 6 shooting
bots, release, two runs of 120 and 90 s): all six joined; the axis
killed an allied bot 8 times in each run, so 8 swaps each; allied bots
killed axis ones 6 and 7 times and earned 17 and 11 `bel_alive_tick`s.
Whether the hunt and the flight look sensible is a hand check
(`pending-manual-test.md` section 49).

VERIFIED (measured, ours, 2026-10-08, release, 6 shooting bots, seeds 1
to 4, 150 s, `mp_brecourt` and `mp_carentan`): with the flight from the
bot's own marker the allied side earned 160 `bel_alive_tick`s over the
eight runs, against 163 without it. The flight changes how the hunted
move, not measurably how long they live.

## 6. United Offensive

Line numbers are `maps/mp/gametypes/sd.gsc` in UO's `pakuo00.pk3`.

- VERIFIED (`sd.gsc` 2125-2133, `bombzone_think` 2141-2205, `bomb_think`
  2350-2362): the sites, the bomb trigger, the plant and defuse times and
  the defuse's `isLookingAt` and 64-unit reach are 1.1's. The plant adds
  `!(other isinvehicle())` and `_util_mp_gmi::canPlantGMI()`
  (`maps/mp/_util_mp_gmi.gsc` 373-415: a `bulletTrace` 18 down from 10
  over the feet, 11 ahead along the view, then at the feet, then at four
  corners 16 out, hits something not a `script_vehicle`), at the start
  and every frame of the hold. A bot standing on the zone's floor passes both,
  so the brain is section 3's unchanged.
- VERIFIED (`checkMatchStart` 1503-1531, `endRound` 1558-1560): UO's
  version notifies `kill_endround` and then threads `endRound`, whose
  first line is `level endon("kill_endround")`; 1.1's threads it only.
  vcod's VM queued a notify to the end of the step and killed the new
  `endRound`, so the match-start round reset never ran, the round's
  `level endon("round_ended")` had already ended both bombzone threads,
  and no plant could start for the rest of the map. The VM now acts on a
  notify where it executes, as retail does (`cod11-gsc-language.md` 9,
  `probe_endon_late`). The same pattern opens UO's `re`, `bas`, `ctf` and
  `dom`.
- VERIFIED (measured, ours, 2026-10-14, `mp_cassino` `sd`, 2 bots, seed
  7, UO rules, `tests/bot_objectives.rs`
  `uo_bots_plant_and_defuse_on_cassino`): the attacker plants at tick 779
  and the defender defuses 224 ticks later; the next round plants at 1778
  and defuses 224 later again.

Weapons, from UO's menus (`ui_mp/scriptmenus/weapon_*.menu`,
`pakuo00.pk3`) and `weapons/mp/*` (`pakuo07.pk3`), all VERIFIED (the
files):

- UO's menus add `mg30cal_mp` to the American and British, `svt40_mp` and
  `dp28_mp` to the Russian, `gewehr43_mp` and `mg34_mp` to the German, and
  `bel`'s two-nation menus (`weapon_americangerman` and kin, `bel.gsc`
  222) offer both sides'. A UO bot answers from `bots::WEAPON_MENUS_UO`,
  a two-nation menu from both nations' lists. The three `weaponClass lmg`
  guns (`mg30cal_mp`, `mg34_mp`, `dp28_mp`; `scr_allow_mg30cal`,
  `scr_allow_dp28`, `scr_allow_mg34` in `_teams.gsc` 1592-1753) fire only
  off the deployed bipod with the sight at 0.99 or more
  (`coduo-movement.md` 6, 7). `bren_mp` and `bar_mp` are `weaponClass
  rifle`.
- An LMG bot (`Bot::bipod`): with an enemy in sight it holds the sight
  button, which deploys the bipod where it stands (on a mount, or prone),
  keeps its move keys at 0 and fires once `PlayerState::deployed` and the
  sight fraction is 0.99. A deploy that has not taken 10 ticks after the
  press (a wall ahead, no room to go prone) lets the button go for 2 s,
  and so does a target more than 50 degrees round `proneDirection` (the
  cap is `bg_lmg_yawcap`'s 55): the breakdown, then a fresh deploy facing
  it. The bipod stays down 3 s after the target is lost. An LMG is put
  away only with the bipod up (the putaway, `coduo-movement.md` 7), and
  its 1.8 s deploy is as long as a fight at pistol range, so an LMG bot
  with an enemy inside 500 units, or resting its button, draws the pistol;
  no frag is thrown with the bipod down. Whether a deployed bot looks
  right is a hand check (`pending-manual-test.md` 217). VERIFIED
  (measured, ours, 2026-10-15, `ship_stalls` `UO=1 SHOOT=1`, `mp_cassino`
  `tdm`, seeds 13, 15, 17, 33): the deployed bots stand for the stall
  count's 12 s, 952, 1 049, 951 and 1 570 stalled bot-ticks against 0,
  0, 187 and 20 with the three LMGs restricted, and their kills were 2 of
  the 25 on seed 33. `ship_stalls` and `uo_bots_fight_and_sprint_in_tdm`
  no longer count a deployed bot (`BotBody::deployed`).
- UO's grenades are `fraggrenade_mp` and its three national twins
  (`weaponSlot grenade`), `smokegrenade_mp` and `flashgrenade_mp`
  (`weaponSlot smokegrenade`, no damage) and `satchelcharge_mp`
  (`weaponSlot satchel`). A bot throws only the `grenade` slot's.
- Every UO grenade has `cookOffHold 1` and the frag's `fuseTime` is 4
  (`coduo-movement.md` 12): the fuse starts at the sight button's edge
  once the pullback runs. A UO bot's throw holds the trigger 24 to 33
  ticks and the sight button from the fifth, so it cooks 1.0 to 1.5 s and
  lets go with 2.5 s or more left; a 1.1 bot's hold is 6 to 13 ticks with
  no sight button (`bots::tests::a_uo_frag_is_spooned_after_the_pullback`).
- A throw aims (`Bot::lob_angles`, `bots::tests::a_throw_lobs_over_the_enemy`):
  through the switch, the hold and the switch back the view turns toward
  the enemy in sight and up over it, 5 to 25 degrees with the range. The
  grenade stages used to send zero angles, so on 1.1 too the frag went
  wherever `delta_angles` pointed. No throw goes at an enemy nearer than
  250: the frag's blast is 350 units, 120 damage at its centre falling to
  5 at its edge (`fraggrenade_mp`, both patches), so at 250 it still
  takes 38 from the thrower. VERIFIED (measured, ours, 2026-10-14,
  `mp_cassino` `tdm`, 6 shooting bots, 120 s, ten seeds, release): before
  the aim and the minimum, 3 of 8 frag kills over five seeds were the
  thrower's own; after, 1 of 11 over ten (seed 8).
- What still killed the thrower (VERIFIED, measured, ours, 2026-10-15,
  same runs, `crates/server/examples/obj_watch.rs` with each throw and
  missile contact logged): over seeds 1 to 19 odd, 3 of 15 frag kills.
  One frag struck a teammate's body 25 units in front of the hand and
  came straight back past the thrower (a stock frag bounces off a live
  player, and a UO bounce damps only in water); in two the enemy had
  closed to 77 and 105 units during the hold. Over seeds 21 to 39 odd
  more: lobs from an upper floor into its ceiling or a window frame that
  dropped the frag at the thrower's feet, a short throw that rolled back,
  and throwers that ran after their enemy into their own blast. Hence:
  - `BotView::lob` (`Server::lob_probe`): of the lobs 5 to 40 degrees
    over the line from the eye, every 5, nearest first to the one the
    range asks for, the first whose flight (the throw's launch, gravity,
    50 ms steps for 1.5 s) meets no wall and no body before 0.6 of the
    range and 250 units across. No throw starts without one, the hold
    turns onto it, and the release waits for one.
  - An enemy inside 250 units at the release: the frag goes the other
    way, 30 up. The release waits until the view is within 30 degrees of
    the throw's yaw, 16 ticks at most (a UO frag cooks on meanwhile,
    with 2.5 s or more left).
  - `BotView::frag_near`: a live frag or satchel, anyone's, within 400
    across and 160 up or down. The goal is away from it and the keys run
    that way, the view still on the fight.
  VERIFIED (measured, ours, 2026-10-15, release `ship_stalls` `UO=1
  SHOOT=1`, `mp_cassino` `tdm`, 6 bots, 120 s, seeds 1 to 39 odd): with
  the turn-away and the release wait alone, 5 of 35 frag kills were the
  thrower's own; with the flight from live frags too, 4 of 17; with the
  lob probe, 0 of 12, among 499 kills. Fewer frags kill at all: the
  probe passes on most throws from indoors, and everyone runs from one.

VERIFIED (measured, ours, 2026-10-14, `mp_cassino` `tdm`, 6 shooting
bots, seed 1, 90 s, `tests/bots.rs` `uo_bots_fight_and_sprint_in_tdm`):
24 kills, none with an LMG or by a bot's own frag, 470 sprinting
bot-ticks, no bot 12 s in one spot, no script abort (with the UO weapons
of `coduo-diff.md` 21 merged). Bots never drive or ride a vehicle.

## 7. United Offensive's `ctf`, `dom`, `hq` and `bas`

Line numbers are `maps/mp/gametypes/<gt>.gsc` in `pakuo00.pk3`; the
server reads each gametype's state off the script every frame
(`ScriptRuntime::uo_objectives`) and deals each bot a `bots::UoView`
(`Server::bot_uo_view`), its zones' standing points by `site_stand` as
for a bombzone. Roles go by rank (the bot's place among its team's bots
by slot): odd ranks keep their own side's objective, even ranks go for
the other side's, so a lone bot attacks. A visible enemy still comes
first everywhere.

### 7.1 What the scripts ask of a player

- VERIFIED (`ctf.gsc` `ctf`, `ctf_spawn_flag`, 2514-2699): each team's
  flag is the entity `ctf_flag_allies` / `ctf_flag_axis` in
  `level.allies_flag` / `level.axis_flag`, its `trigger` the
  `trigger_multiple` it targets, its `goal` the `trigger_multiple` its
  `mp_gmi_ctf_flag` spot targets, and `mobile_trigger` the one its
  `_mobile` model targets, sent 10000 down (`triggerOff`, 3424) while the
  flag is home. VERIFIED (measured, ours, mp_cassino): the axis flag's
  goal lies at the allied flag's home and the other way round, so a
  carrier captures at his own base.
- INFERRED (`ctf_think`, 2701-2814, branch conditions): a touch of the
  flag's trigger, or of the mobile trigger once it has `moved`, by a live
  player of the other team takes it (`hold_flag`: `carried_by`, `moved`,
  both triggers off); a touch of the mobile trigger by its own team
  returns it home (`reset_flag`). A player in a vehicle is passed over.
- INFERRED (`flag_carrier_atgoal_wait`, 2946-3087): the carrier's touch
  of the carried flag's `goal` captures only while his own team's flag
  is not `moved`; every other touch is passed over and the wait goes on.
- VERIFIED (`drop_flag`, 3125-3257; `flag_timeout`, 3259-3291): a
  carrier's death leaves the flag at his feet with the mobile trigger on
  it, and 20 s lying there sends it home. With `scr_ctf_showoncompass`
  0, the default (304), no compass marker follows a carried flag.
- VERIFIED (`dom.gsc` `Flag_InitTriggers`, `Flag_ZoneThink`, 1461-1750):
  the flags are the triggers `flag1` .. `flag14` by targetname, each with
  a `team` (`neutral` at the start). INFERRED (branch conditions): each
  frame of a zone with live players of one team only in it, not in a
  vehicle and not its owners, adds 0.05 s, and half again for each
  capper past the first, until `script_timer` (10 by default) turns it;
  any player of the other team in the zone stops the count.
  `Flag_AllCapturedThink` (1757-1800) ends the round when one team holds
  every flag.
- VERIFIED (`hq.gsc` `hq_setup`, 1720-1755; `hq_radio_think`, 1871-2168):
  the radios are `level.radio`, the `hqradio` script models, one at a
  time on show (`hidden` false), each with a `team` (`none` while
  neutral) and a `radius` (`script_radius`, else `level.radioradius`
  120); `level.zradioradius` is 50 (186-196). INFERRED (branch
  conditions): a playing player not in a vehicle within `radius` across
  and 50 up or down counts; a neutral radio with one team only near it
  and that team holding no other is captured over `RadioCaptureTime`,
  and a held one with the other team near it and no holder is
  neutralized over `RadioNeutralTime`. The next radio's marker is
  objective 0 at its origin before it shows (`hq_obj_think`, 1792).
- VERIFIED (`bas.gsc` `BaseAssault_GMI`, 4089-4186): the bases are
  `level.bases`, the `gmi_base` script models, each with `script_team`,
  `health` (`scr_bas_basehealth`, 24500) and `alive`; a team wins when
  the other has none alive. INFERRED (`BaseAssault_base_think`,
  3844-4060): only a player's `MOD_PROJECTILE*`, `MOD_GRENADE*` or
  `MOD_ARTILLERY*` from the other team takes health off (3954); at 0 the
  base is breached, and every attached thing with a `script_idnumber`
  (its walls, `script_brushmodel`s) is deleted. INFERRED
  (`BaseAssault_ObjectiveZone_think`, 3666-3800): a breached base's
  `objective` (a `trigger_lookat`, `BaseAssault_get_objective` 3081)
  fires for a player of the other team looking at it within 150 units
  of its `objectivemodel`; on the ground with use held he is linked and
  plants over `scr_bas_planttime` (15 s), the hold breaking past 155
  units. `BaseAssault_bomb_think` (3137-3273) is the defuse, the same for
  the base's own team over `scr_bas_defusetime` (10 s).
  `BaseAssault_bomb_countdown` (3586-3664) blows after `scr_bas_bombtime`
  with a 1000-damage `radiusDamage` on every player within 500 units,
  and kills the base.
- VERIFIED (mp_foy's entity lump, and the same on mp_italy, mp_kursk,
  mp_ponyri and mp_rhinevalley by the charges' heights): each base's
  charge sits in a room under the bunker, 176 units under the base's
  origin, down a stair whose way in is the base's `script_idnumber`
  walls (`*66` for mp_foy's axis hq), and whose stairwell holds the
  base's `rubble` brushes (`script_noteworthy rubble`, `*21`), which
  `BaseAssault_base_think` lifts 1000 units at the start (3908-3924) and
  `BaseAssault_KillBase` drops back (3475-3544). So nothing reaches a
  charge until its base is breached.
- VERIFIED (retail UO 1.51b, `client-probes/probe_spawnint` on mp_foy,
  2026-10-15, gate `spawnint_uo_ab`): those walls' `"script_idnumber"
  "foo"` reads 0, defined, as `strtol` leaves it. Ours left the field
  undefined and never deleted a breached base's walls.

### 7.2 The brain

- **ctf.** The carrier makes for the carried flag's goal, and waits
  inside it while his own flag is away. His own flag lying out is
  returned by a touch: a defender goes for it, and so does an attacker
  within 1000 units or one whose team's carrier is out. A defender stands
  by his flag at home, or by the goal its carrier needs while it is
  carried. An attacker takes the other flag, at home or lying out, else
  escorts the teammate carrying it.
- **dom.** A bot sets out for a flag its team does not hold, the rank-th
  nearest, keeps it until its team holds it, and stands inside the zone
  (8 units in) looking about. With every flag held it guards one by
  rank.
- **hq.** Neutral or the other team's radio: stand within its radius
  less 16 and 34 up or down. Its own team's: guard it.
  None up: wait by the next one's marker.
- **bas.** An attacker takes the other side's alive bases by rank. A
  breached one with no charge on it is planted: walk within 112 of the
  objective model, rest the view on the trigger's middle, hold use
  15.5 s (S&D's defuse, section 3, with its reach and hold as
  parameters). A charged one is guarded from outside the blast, 520 to
  670 units off. A defender guards its own bases by rank and defuses a
  charge on one (10.5 s), and so does an attacker within 2000 units of
  it. Bots ride no tank and carry no rocket in hand, so they breach
  nothing: a plant waits on a human's breach, or on
  `scr_bas_basehealth 0`, which breaches every base at the round's start
  (no damage to take).
- The way into a breached base opens only when its walls go, after the
  graph was built. The server builds the graph without the `rubble`
  brushes (`World::rubble`; out of the way except over a dead base) and
  rebuilds it, on its own thread, over the brushes the script has linked
  and unlinked since, once that count (`GameHost::brush_links`, bumped
  by `delete`, `notSolid` and `solid` on a brush model) has held still
  2 s (`Server::relink_nav`); the cache keys on which submodels clip, so
  a restart that puts the same walls back finds the old graph. Two
  engine fixes were needed on the way: a brush model's clip now follows
  a bare `origin` write (the rubble's lift; only movers and `linkTo`
  posed it before), and a respawned brush entity links its model again
  (a restart's walls).

VERIFIED (measured, ours, 2026-10-15, release, UO rules, 2 bots, seed 7,
`tests/bot_objectives.rs`, after gtend's gametype endings merged): `ctf`
on mp_cassino with shoot on, a flag taken at tick 1351 and captured 1342
ticks later, after a kill dropped the other flag and it went home (with
shoot off both bots take a flag and wait in their goals for good); `dom`
on mp_cassino with shoot off, the first flag at tick 843 and 4 more
turned in the 60 s after; `hq` on mp_cassino with shoot on, a radio held
at tick 1102 (with shoot off the two bots contest it for good); `bas` on
mp_foy with shoot off and `scr_bas_basehealth 0`, a charge planted at
tick 2020 on the axis hq, down the stair once the graph had the walls
out, and gone off 1199 ticks later with the lone defender at the other
side's bases. No script abort in any.
