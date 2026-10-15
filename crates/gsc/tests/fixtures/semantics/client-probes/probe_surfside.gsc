//	A brush's top side against its own material, for
//	docs/research/bsp-ibsp59-format.md (Lump 3). mp_carentan only. Logs a
//	bulletTrace straight down onto five brush tops whose top side's
//	material differs from the brush's:
//
//		PROBE surf <i> <surfacetype> <position> <fraction>
//
//	(every one reads "default" on a dedicated server), then puts every
//	playing client, once a spawn, at (320 1252) on the floor brush at
//	x 232..552, y 1096..1408 (rock@cobbl_patrn1a, its top side wood),
//	facing +x:
//
//		PROBE placed <time> <entnum>
//
//	Run by tools/run_probe.sh client-probes/probe_surfside mp_carentan
//	(SECS=200) with one --save-slope --probe-moves --probe-cmd-ms 8
//	--capture-tag surfside client, whose footsteps on the floor are the
//	measurement; its fixture is discarded.

main()
{
	thread run();
	thread place();
	maps\mp\gametypes\dm::main();
}

run()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	wait 1;
	pts = [];
	pts[0] = (392, 1252, 8);
	pts[1] = (60, 1914, 160);
	pts[2] = (1900, 1592, -16);
	pts[3] = (1130, 600, 8);
	pts[4] = (756, 600, 160);
	for (i = 0; i < pts.size; i++)
	{
		p = pts[i];
		t = bullettrace(p + (0, 0, 32), p - (0, 0, 8), false, undefined);
		logPrint("PROBE surf " + i + " " + t["surfacetype"] + " " + t["position"] + " " + t["fraction"] + "\n");
	}
}

place()
{
	if (getcvar("mapname") != "mp_carentan")
		return;
	for (;;)
	{
		wait 0.05;
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			if (p.sessionstate != "playing" || isdefined(p.probe_placed))
				continue;
			p.probe_placed = true;
			p setorigin((320, 1252, 8.125));
			p setplayerangles((0, 0, 0));
			logPrint("PROBE placed " + getTime() + " " + p getEntityNumber() + "\n");
			p thread unplace();
		}
	}
}

unplace()
{
	self waittill("spawned_player");
	self.probe_placed = undefined;
}
