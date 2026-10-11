//	SP_func_bobbing (game.mp 0x583d0) and SP_func_pendulum (0x56fe0), for
//	docs/research/cod11-movers.md 18. Two modes, picked by the map:
//	- stock mp_carentan (no func_bobbing in the lump): script spawns three
//	  bobbings (spawnflags 0, 1, 2) and two pendulums (spawnflags 0, 1) at
//	  1050 and logs their origins and angles every frame until the server
//	  drops the map one period on.
//	- mp_carentan with the patched entity lump (sine_ab.rs's `sine_bsp`):
//	  the map-placed func_bobbing and func_pendulum, logged every frame for
//	  60 s; their keys put the drop minutes away. A --net-probe client
//	  attached to this run prints their wire trajectories.
//	- stock mp_carentan with +set probe_wire 1: one bobbing and one
//	  pendulum at 1050, for a --net-probe client started before the server
//	  to see before the drop. A map change does not restart the level
//	  clock, so the client cannot be attached any other way.
//	Run: tools/run_probe.sh client-probes/probe_bobbing mp_carentan
//	  PROBE ent <tag> <time> <num> <classname> <spawnflags> '<model>' <origin> <angles>
//	  PROBE f <time> (<tag> <origin> <angles>)...

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
	thread run();
}

run()
{
	ents = [];
	tags = [];
	placed = getentarray("func_bobbing", "classname");
	if (placed.size)
	{
		ents[0] = placed[0];
		tags[0] = "mb";
		pend = getentarray("func_pendulum", "classname");
		ents[1] = pend[0];
		tags[1] = "mp";
		frames = 1200;
		wait 1;
	}
	else if (getcvar("probe_wire") == "1")
	{
		wait 1;
		ents[0] = spawn("func_bobbing", (10, 20, 100));
		ents[1] = spawn("func_pendulum", (10, 20, 100));
		tags[0] = "b0";
		tags[1] = "p0";
		frames = 200;
	}
	else
	{
		wait 1;
		ents[0] = spawn("func_bobbing", (10, 20, 100));
		ents[1] = spawn("func_bobbing", (10, 20, 100), 1);
		ents[2] = spawn("func_bobbing", (10, 20, 100), 2);
		ents[3] = spawn("func_pendulum", (10, 20, 100));
		ents[4] = spawn("func_pendulum", (10, 20, 100), 1);
		tags[0] = "b0";
		tags[1] = "b1";
		tags[2] = "b2";
		tags[3] = "p0";
		tags[4] = "p1";
		frames = 200;
	}
	for (i = 0; i < ents.size; i++)
	{
		e = ents[i];
		logPrint("PROBE ent " + tags[i] + " " + gettime() + " " + e getEntityNumber() + " " + e.classname + " " + e.spawnflags + " '" + e.model + "' " + e.origin + " " + e.angles + "\n");
	}
	for (f = 0; f < frames; f++)
	{
		wait 0.05;
		line = "PROBE f " + gettime();
		for (i = 0; i < ents.size; i++)
			line = line + " " + tags[i] + " " + ents[i].origin + " " + ents[i].angles;
		logPrint(line + "\n");
	}
	logPrint("PROBE done\n");
}

Callback_PlayerConnect()
{
	logPrint("PROBE connected " + gettime() + "\n");
}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
