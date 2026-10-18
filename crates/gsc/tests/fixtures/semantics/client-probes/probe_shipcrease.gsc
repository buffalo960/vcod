//	Whether a body at rest in mp_ship's shelf corners at x 5375.1 runs free
//	(bot navigation doc, section 3, "The corners at x 5375.1"). Under
//	probe_teleport 1 it respawns every allied player at each corner, at
//	rest, 3 s apart, and logs the origin every frame for 2 s. Run by
//	tools/run_probe.sh with a --probe-fall --probe-team allies client
//	walking at a yaw with --probe-fall-walk.

main()
{
	thread watch();
	maps\mp\gametypes\dm::main();
}

watch()
{
	if (getcvar("probe_teleport") != "1")
		return;
	if (getcvar("mapname") != "mp_ship")
	{
		logPrint("PROBE teleport unsupported " + getcvar("mapname") + "\n");
		return;
	}
	wait 1;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_trials();
		wait 0.05;
	}
}

try_trials()
{
	// The north corner, then the south one.
	spots[0] = (5375.1, 321.1, 352.125);
	spots[1] = (5375.1, -193.1, 352.125);
	wait 3;
	for (i = 0; i < spots.size; i++)
	{
		self.maxhealth = 1000;
		self.health = 1000;
		// `spawn`, not `setOrigin`: a respawn starts the body at rest,
		// where a teleport keeps the walk's velocity.
		self spawn(spots[i], (0, 45, 0));
		logPrint("PROBE drop " + getTime() + " " + i + " " + spots[i] + "\n");
		for (f = 0; f < 40; f++)
		{
			wait 0.05;
			logPrint("PROBE f " + getTime() + " " + i + " " + self.origin + "\n");
		}
		wait 0.5;
	}
	logPrint("PROBE done " + getTime() + "\n");
}
