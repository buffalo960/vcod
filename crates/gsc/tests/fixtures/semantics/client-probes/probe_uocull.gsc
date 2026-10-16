//	What 1.5's and United Offensive's snapshot builders send past the PVS:
//	`playSound` (game.mp.uo 0x68abb) stamps `r.broadcastTime` at level time
//	plus 300 instead of 1.1's SVF_BROADCAST; a vehicle's sound blends are
//	tested as their vehicle; and a `setCullFog` far distance culls whatever
//	lies beyond it. UO only, mp_berlin. Slot 0, the viewer, stands at the
//	tdm spawn (1232, 2316), out of PVS of the Panzer IV parked at
//	(-5936, 2550) and in PVS of the T-34 at (523, 3020), 1000 units off.
//	The script sounds the Panzer, then a script_origin beside it, sets a
//	500-unit cull fog, then exponential fog, then the map's own. Logs:
//
//		PROBE begin <num>
//		PROBE vehicle <num> <type> <origin>
//		PROBE step <name> <time>
//
//	Recipe: `COD_VERSION=uo SECS=40 tools/run_probe.sh
//	client-probes/probe_uocull mp_berlin`, then `vcod --game-dir <UO
//	install> --mod-dir uo --net-probe <addr> --probe-edge watch
//	--probe-secs 30`. The viewer's `EDGE ents` lines are the measurement
//	(crates/server/tests/uocull_ab.rs).

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();
}

Callback_StartGameType()
{
}

Callback_PlayerConnect()
{
	self waittill("begin");
	n = self getEntityNumber();
	logPrint("PROBE begin " + n + "\n");
	self.sessionteam = "allies";
	self.sessionstate = "playing";
	self.maxhealth = 100;
	self.health = 100;
	self spawn((1232, 2316, 518), (0, 180, 0));
	if (n == 0)
		self thread script();
}

Callback_PlayerDisconnect()
{
}

Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
}

//	The script_vehicle nearest `at`.
vehicle(at)
{
	vehs = getentarray("script_vehicle", "classname");
	best = undefined;
	for (i = 0; i < vehs.size; i++)
		if (!isdefined(best) || distance(vehs[i].origin, at) < distance(best.origin, at))
			best = vehs[i];
	return best;
}

step(name)
{
	logPrint("PROBE step " + name + " " + getTime() + "\n");
}

script()
{
	veh = vehicle((-5936, 2550, 700));
	logPrint("PROBE vehicle " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + "\n");
	wait 4;
	step("quiet");
	wait 3;
	step("vehicle_sound");
	veh playSound("tank_reload");
	wait 3;
	step("origin_sound");
	org = spawn("script_origin", veh.origin + (0, 0, 64));
	org playSound("tank_reload");
	wait 3;
	step("cull_fog");
	setCullFog(0, 500, .5, .5, .5, 0);
	wait 3;
	step("exp_fog");
	setExpFog(.0002, .5, .5, .5, 0);
	wait 3;
	step("map_fog");
	setCullFog(500, 15000, .0273, .0820, .1601, 0);
	wait 3;
	step("done");
}
