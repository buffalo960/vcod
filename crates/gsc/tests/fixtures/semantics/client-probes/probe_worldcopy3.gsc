//	The turned world copy of probe_worldcopy2 to the end of its unpitch, with
//	the player's offset from the copy's origin O printed to a ten-thousandth,
//	for docs/research/cod11-movers.md 21 ("The turned centre"). mp_carentan
//	only. Same stand, pack, copy and verbs as probe_worldcopy2, no grids and
//	no func_statics. Every server frame of a phase logs
//
//		PROBE f <phase> <time> <player origin> <pack origin> <copy origin> <copy angles>
//		PROBE g <phase> <time> <player x - O.x> <player y - O.y> <player z - O.z>
//
//	the second's floats printed by the engine's %g, six digits.
//	Run by tools/run_probe.sh client-probes/probe_worldcopy3 mp_carentan
//	with a --probe-team allies client.

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	s = (-512, 2688, -16);
	p = undefined;
	for (i = 0; i < 600 && !isdefined(p); i++)
	{
		wait 0.05;
		players = getentarray("player", "classname");
		for (j = 0; j < players.size; j++)
			if (players[j] playing())
				p = players[j];
	}
	if (!isdefined(p))
	{
		logPrint("PROBE no_player\n");
		return;
	}
	wait 2;
	p setorigin(s);
	p setplayerangles((0, 90, 0));
	h = spawn("item_health", s + (40, 0, 0));
	wait 1;
	logPrint("PROBE player " + p getEntityNumber() + " pack " + h getEntityNumber() + "\n");
	sample("rest", p, h, undefined, 0.5);

	e = spawn("script_brushmodel", (-576, 2688, -75.875));
	logPrint("PROBE copy " + e getEntityNumber() + "\n");
	sample("spawn", p, h, e, 0.5);

	e movez(16, 1);
	sample("up", p, h, e, 2);

	e rotateyaw(45, 1.5);
	sample("yaw45", p, h, e, 2);

	e rotateyaw(45, 1.5);
	sample("yaw90", p, h, e, 2);

	e rotateyaw(-90, 2);
	sample("yaw0", p, h, e, 2.5);

	e rotatepitch(5, 1);
	sample("pitch", p, h, e, 1.5);

	e rotatepitch(-5, 1);
	sample("unpitch", p, h, e, 1.5);

	logPrint("PROBE done " + getTime() + "\n");
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
playing()
{
	if (!isdefined(self.sessionstate))
		return false;
	return self.sessionstate == "playing";
}

sample(phase, p, h, e, secs)
{
	frames = secs * 20;
	for (i = 0; i < frames; i++)
	{
		o = "(none) (none)";
		if (isdefined(e))
			o = "" + e.origin + " " + e.angles;
		logPrint("PROBE f " + phase + " " + getTime() + " " + p.origin + " " + h.origin + " " + o + "\n");
		logPrint("PROBE g " + phase + " " + getTime() + " " + (p.origin[0] + 576) + " " + (p.origin[1] - 2688) + " " + (p.origin[2] + 75.875) + "\n");
		wait 0.05;
	}
}
