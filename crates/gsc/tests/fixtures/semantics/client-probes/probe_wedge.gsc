//	Where a body comes to rest in a terrain crease on mp_hurtgen (bot
//	navigation doc, section 3, "mp_hurtgen seed 43"). Under probe_teleport 1
//	it respawns every allied player at each trial spot, at rest, 3 s apart,
//	and logs the origin every frame for 2 s. Run by tools/run_probe.sh with a
//	--probe-fall --probe-team allies client (standing still, or walking at
//	a yaw with --probe-fall-walk).

main()
{
	thread watch();
	maps\mp\gametypes\dm::main();
}

watch()
{
	if (getcvar("probe_teleport") != "1")
		return;
	if (getcvar("mapname") != "mp_hurtgen")
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
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
		return;
	if (isdefined(self.probe_running))
		return;
	if (self.pers["team"] != "allies")
		return;
	self.probe_running = 1;
	self thread trials();
}

trials()
{
	// The rest vcod reaches, then drops onto it from above, then the slope
	// the bot slid down.
	spots[0] = (436.8, 2824.3, -97.2);
	spots[1] = (436.8, 2824.3, -80);
	spots[2] = (430, 2810.4, -87.5);
	spots[3] = (416.7, 2799, -60);
	spots[4] = (379.3, 2806.7, -33.5);
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
