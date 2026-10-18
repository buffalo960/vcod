//	g_speed and g_gravity capture's server half: probe_prone's street
//	placement on mp_carentan with the speed and gravity changed.
//	probe_gravity N sets g_gravity once the level has loaded (SP_worldspawn
//	writes the map's gravity or 800 over any +set). probe_maxspeed N sets
//	each allied player's maxspeed at his placement (1.5 and UO only: 1.1
//	has no such field and reads g_speed, which +set g_speed sets). Run by
//	tools/run_probe.sh with a --save-slope --probe-moves client.

main()
{
	thread set_gravity();
	thread watch_teleports();
	maps\mp\gametypes\dm::main();
}

set_gravity()
{
	wait 0.05;
	if (getcvar("probe_gravity") != "")
		setcvar("g_gravity", getcvar("probe_gravity"));
	logPrint("PROBE gravity " + getcvar("g_gravity") + " speed " + getcvar("g_speed") + "\n");
}

watch_teleports()
{
	if (getcvar("mapname") != "mp_carentan")
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
	if (getcvar("probe_maxspeed") != "")
		self.maxspeed = getcvarint("probe_maxspeed");
	spot = (900, 1930, -38);
	yaw = 90;
	self setorigin(spot);
	self setplayerangles((0, yaw, 0));
	logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
}
