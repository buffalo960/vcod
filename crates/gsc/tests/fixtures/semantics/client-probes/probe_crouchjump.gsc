//	United Offensive crouched jump capture's server half (COD_VERSION=uo
//	tools/run_probe.sh, mp_cassino or mp_streets, a --save-slope
//	--probe-crouchjump client). Under probe_teleport 1 it puts every
//	playing client, once per spawn, on a floor of a room whose only way
//	out is a window too low for a standing body, facing the window:
//	probe_spot cassino is mp_cassino's axis room, streets the mp_streets
//	axis room at z 101, low the mp_streets room at z -24.
//	docs/research/bot-navigation.md, "Walled-in spawns".

main()
{
	thread watch_teleports();
	maps\mp\gametypes\dm::main();
}

watch_teleports()
{
	if (getcvar("probe_teleport") != "1")
		return;
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
	self.probe_placed = 1;
	spot = getcvar("probe_spot");
	if (spot == "streets")
	{
		origin = (323, -6658, 101.125);
		yaw = 180;
	}
	else if (spot == "low")
	{
		origin = (532, -3378, -23.875);
		yaw = -90;
	}
	else
	{
		origin = (5347, -1496, 128.125);
		yaw = -128;
	}
	self setorigin(origin);
	self setplayerangles((0, yaw, 0));
	logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + " " + origin + " " + yaw + "\n");
}
