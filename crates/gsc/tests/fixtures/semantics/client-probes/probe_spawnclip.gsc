//	Whether a script-spawned brush entity clips, for
//	docs/research/cod11-movers.md 19. `spawn` gives the entity no model, so
//	`trap_SetBrushModel` links inline model 0, the world. No client.
//	A bulletTrace straight down onto the first dm spawn point's floor, before
//	and after a script_brushmodel is spawned at (0, 0, 64), after it moves up
//	32, after notSolid, solid and delete, then under a script-spawned
//	func_bobbing every 0.25 s until the level drops.
//	Run: tools/run_probe.sh client-probes/probe_spawnclip mp_carentan
//	  PROBE tr <tag> <time> <position> <fraction> <entnum or -1>
//	  PROBE ent <tag> <num> <classname> '<model>' <origin>

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

tr(tag, p)
{
	t = bulletTrace(p + (0, 0, 100), p + (0, 0, -100), false, undefined);
	n = -1;
	if (isdefined(t["entity"]))
		n = t["entity"] getEntityNumber();
	logPrint("PROBE tr " + tag + " " + gettime() + " " + t["position"] + " " + t["fraction"] + " " + n + "\n");
}

ent(tag, e)
{
	logPrint("PROBE ent " + tag + " " + e getEntityNumber() + " " + e.classname + " '" + e.model + "' " + e.origin + "\n");
}

run()
{
	wait 1;
	spots = getentarray("mp_deathmatch_spawn", "classname");
	p = spots[0].origin;
	logPrint("PROBE spot " + p + "\n");
	tr("bare", p);

	e = spawn("script_brushmodel", (0, 0, 64));
	ent("bm", e);
	tr("bm_now", p);
	wait 0.05;
	tr("bm", p);
	e movez(32, 0.5);
	wait 1;
	ent("bm_moved", e);
	tr("bm_moved", p);
	e notsolid();
	tr("bm_notsolid", p);
	e solid();
	tr("bm_solid", p);
	e delete();
	tr("bm_deleted", p);
	wait 0.05;
	tr("bm_freed", p);

	b = spawn("func_bobbing", (0, 0, 0));
	ent("bob", b);
	for (i = 0; i < 20; i++)
	{
		tr("bob " + b.origin, p);
		wait 0.25;
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
