//	The shellshock playerstate fields, mp_carentan only. Once a player is
//	playing and the level clock reads 15000, puts it on probe_legs' mark
//	(the map's bottom brush floor under the garden, not a street) facing +x,
//	then on a schedule from there:
//
//		+1000  shellShock("default", 2.5)
//		+4000  shellShock("pain", 1.25)     over a running shock
//		+4500  stopShellShock()
//		+5500  shellShock("default", 0)
//		+6500  shellShock("default", 3)
//
//	each logged as `PROBE <what> <time>`, and the player's origin every
//	server frame as `PROBE o <time> <origin>`. The --save-shock client
//	runs forward and back and holds wbuttons 0x4 while the shock runs.
//	Run by tools/run_probe.sh with a --save-shock client.

main()
{
	precacheShellShock("default");
	precacheShellShock("pain");
	thread watch_players();
	maps\mp\gametypes\dm::main();
}

watch_players()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_run();
		wait 0.05;
	}
}

try_run()
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
		return;
	if (isdefined(self.probe_running))
		return;
	self.probe_running = 1;
	self thread phases();
	self thread origins();
}

phases()
{
	while (getTime() < 15000)
		wait 0.05;
	self setorigin((932, -376, -151.875));
	self setplayerangles((0, 0, 0));
	t0 = getTime();
	logPrint("PROBE placed " + t0 + " " + self.origin + "\n");
	at(t0 + 1000);
	self shellShock("default", 2.5);
	logPrint("PROBE shock default 2.5 " + getTime() + "\n");
	at(t0 + 4000);
	self shellShock("pain", 1.25);
	logPrint("PROBE shock pain 1.25 " + getTime() + "\n");
	at(t0 + 4500);
	self stopShellShock();
	logPrint("PROBE stop " + getTime() + "\n");
	at(t0 + 5500);
	self shellShock("default", 0);
	logPrint("PROBE shock default 0 " + getTime() + "\n");
	at(t0 + 6500);
	self shellShock("default", 3);
	logPrint("PROBE shock default 3 " + getTime() + "\n");
}

at(t)
{
	while (getTime() < t)
		wait 0.05;
}

origins()
{
	self endon("disconnect");
	while (getTime() < 15000)
		wait 0.05;
	for (i = 0; i < 260; i++)
	{
		logPrint("PROBE o " + getTime() + " " + self.origin + "\n");
		wait 0.05;
	}
}
