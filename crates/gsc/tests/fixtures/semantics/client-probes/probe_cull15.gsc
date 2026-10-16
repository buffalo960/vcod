//	What 1.5's snapshot builder sends past the PVS (cod15-protocol-diff.md
//	15): `playSound` and `stopLoopSound` stamp `r.broadcastTime` at level
//	time plus 300, `playLoopSound` at -1, none of them touches `svFlags`,
//	and a `setCullFog` far distance culls whatever lies beyond it. 1.5
//	twin of probe_uocull, on script models instead of a vehicle.
//	mp_carentan only. Slot 0, the viewer, stands on the dm spawn at
//	(-584, 1872, 8). Out of its PVS: `far` (992, -744), `loop`
//	(1128, 424) and the script_origin `org` (1872, 928). In it: `near`
//	(-1008, 2088), 490 units off, `mid` (776, 3488), 2100 off, and the
//	script_origin `here` (-1136, 1504). Logs:
//
//		PROBE begin <num>
//		PROBE ent <name> <num>
//		PROBE step <name> <time>
//
//	Recipe: `COD_VERSION=1.5 SECS=55 tools/run_probe.sh
//	client-probes/probe_cull15 mp_carentan`, then `vcod --game-dir <1.5
//	install> --net-probe <addr> --probe-edge watch --probe-secs 45`. The
//	viewer's `EDGE ents` lines are the measurement
//	(crates/server/tests/cull15_ab.rs).

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
	precacheModel("xmodel/weapon_mp40");
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
	self spawn((-584, 1872, 8), (0, 0, 0));
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

model(name, at)
{
	e = spawn("script_model", at);
	e setModel("xmodel/weapon_mp40");
	logPrint("PROBE ent " + name + " " + e getEntityNumber() + "\n");
	return e;
}

origin(name, at)
{
	e = spawn("script_origin", at);
	logPrint("PROBE ent " + name + " " + e getEntityNumber() + "\n");
	return e;
}

step(name)
{
	logPrint("PROBE step " + name + " " + getTime() + "\n");
}

script()
{
	far = model("far", (992, -744, 32));
	loop = model("loop", (1128, 424, 40));
	near = model("near", (-1008, 2088, 192));
	mid = model("mid", (776, 3488, 48));
	org = origin("org", (1872, 928, 48));
	here = origin("here", (-1136, 1504, 48));
	wait 4;
	step("quiet");
	wait 3;
	step("sound");
	far playSound("MP_bomb_plant");
	wait 3;
	step("sound_twice");
	far playSound("MP_bomb_plant");
	wait .2;
	far playSound("MP_bomb_plant");
	wait 3;
	step("origin_sound");
	org playSound("MP_bomb_plant");
	here playSound("MP_bomb_plant");
	wait 3;
	step("loop");
	loop playLoopSound("bomb_tick");
	wait 3;
	step("stop_loop");
	loop stopLoopSound();
	wait 3;
	step("cull_fog");
	setCullFog(0, 1000, .5, .5, .5, 0);
	wait 3;
	step("move_in");
	mid.origin = (-1136, 1504, 48);
	wait 3;
	step("move_out");
	near.origin = (1552, 3120, 40);
	wait 3;
	step("fog_sound");
	far playSound("MP_bomb_plant");
	wait 3;
	step("fog_loop");
	loop playLoopSound("bomb_tick");
	wait 3;
	step("exp_fog");
	setExpFog(.0002, .5, .5, .5, 0);
	wait 3;
	step("map_fog");
	loop stopLoopSound();
	setCullFog(0, 16500, 0.7, 0.85, 1.0, 0);
	wait 3;
	step("done");
}
