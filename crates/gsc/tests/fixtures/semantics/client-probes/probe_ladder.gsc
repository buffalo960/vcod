//	Ladder weapon capture's server half. Under probe_teleport 1 it puts every
//	allied player at the foot of mp_ship's hold ladder (4332 -94 56), facing
//	the rungs at yaw 90, once per spawn, and logs the weapon it holds every
//	second. Run by tools/run_probe.sh with a --probe-ladder 90 client.

main()
{
	thread watch_teleports();
	maps\mp\gametypes\dm::main();
}

watch_teleports()
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
			players[i] try_place();
		wait 0.05;
	}
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
try_place()
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
	{
		self.probe_placed = undefined;
		return;
	}
	if (isdefined(self.probe_placed))
		return;
	if (self.pers["team"] != "allies")
		return;
	self.probe_placed = 1;
	spot = (4332, -94, 57);
	self setorigin(spot);
	self setplayerangles((0, 90, 0));
	logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + " " + spot + "\n");
}
