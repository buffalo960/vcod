//	A script-spawned brush entity, a copy of the world's clip where it stands
//	(docs/research/cod11-movers.md 19), on the wire and as a pusher; movers
//	doc 20. mp_carentan only. Waits for one playing client, stands it at S
//	(an open column on the north street) and drops a health pack beside it,
//	then spawns a script_brushmodel at (0, 0, -64), so the copy's floor sits
//	64 under the real one, and moves it: up 96 through the player and the
//	pack, across 32, down 96, then deletes it. A func_static spawned far
//	below is the wire's second sample. Every server frame of a phase logs:
//
//		PROBE f <phase> <time> <player origin> <pack origin> <copy origin>
//
//	Run by tools/run_probe.sh client-probes/probe_worldcopy mp_carentan with
//	a --probe-ride --probe-team allies client, the wire half: the copy's
//	RIDE_NEW line and trajectories, and the player's RIDE line per snapshot.

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

	logPrint("PROBE at spawn " + getTime() + "\n");
	e = spawn("script_brushmodel", (0, 0, -64));
	logPrint("PROBE copy " + e getEntityNumber() + "\n");
	sample("spawn", p, h, e, 1);

	logPrint("PROBE at up " + getTime() + "\n");
	e movez(96, 2);
	sample("up", p, h, e, 3);

	logPrint("PROBE at x " + getTime() + "\n");
	e movex(32, 1);
	sample("x", p, h, e, 2);

	logPrint("PROBE at down " + getTime() + "\n");
	e movez(-96, 2);
	sample("down", p, h, e, 3);

	logPrint("PROBE at delete " + getTime() + "\n");
	e delete();
	sample("delete", p, h, undefined, 1);

	logPrint("PROBE at static " + getTime() + "\n");
	f = spawn("func_static", (0, 0, -20000));
	logPrint("PROBE static " + f getEntityNumber() + "\n");
	sample("static", p, h, f, 1);
	f delete();
	wait 1;
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
		o = "(none)";
		if (isdefined(e))
			o = "" + e.origin;
		logPrint("PROBE f " + phase + " " + getTime() + " " + p.origin + " " + h.origin + " " + o + "\n");
		wait 0.05;
	}
}
