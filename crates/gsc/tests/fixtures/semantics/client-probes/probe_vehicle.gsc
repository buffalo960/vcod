//	United Offensive vehicle capture (docs/research/coduo-diff.md, 9 and 18):
//	stock tdm with the map's vehicles, plus a placement. Client 0 is put 110
//	units off the first vehicle's right flank and client 1 off its left, each
//	facing the hull (`probe_side` units out and `probe_pitch` degrees under
//	10 down when set), once per spawn, so a `--probe-vehicle driver` and a
//	`--probe-vehicle gunner` client can press use into seats 1 and 2. Logs
//	the tanks once and every seat change. A third client is put 350 units
//	out along `probe_watch_yaw` to watch. UO only: `script_vehicle`,
//	`isInVehicle` and the `activated` notify do not exist in 1.1.

main()
{
	thread log_vehicles();
	thread watch_places();
	maps\mp\gametypes\tdm::main();
}

log_vehicles()
{
	wait 1;
	vehs = getentarray("script_vehicle", "classname");
	for (i = 0; i < vehs.size; i++)
	{
		logPrint("PROBE vehicle " + vehs[i] getEntityNumber() + " " + vehs[i].vehicletype + " " + vehs[i].origin + " " + vehs[i].angles + "\n");
		vehs[i] thread log_seats();
		vehs[i] log_clearance();
	}
}

//	How far the ground is clear around a tank, every 45 degrees of world
//	yaw at hull height, so the client's script can drive into the open.
log_clearance()
{
	start = self.origin + (0, 0, 40);
	line = "PROBE clear " + self getEntityNumber();
	for (yaw = 0; yaw < 360; yaw += 45)
	{
		f = anglestoforward((0, yaw, 0));
		end = start + (f[0] * 1500, f[1] * 1500, 0);
		trace = bullettrace(start, end, false, self);
		line = line + " " + yaw + ":" + trace["fraction"] * 1500;
	}
	logPrint(line + "\n");
}

log_seats()
{
	for (;;)
	{
		self waittill("activated", vehpos, activator);
		logPrint("PROBE activated " + gettime() + " " + self getEntityNumber() + " seat " + vehpos + " by " + activator getEntityNumber() + "\n");
	}
}

//	The vehicle the capture uses: the lowest-numbered `script_vehicle`, of
//	the `probe_vehicletype` cvar's type when that is set.
probe_vehicle()
{
	want = getcvar("probe_vehicletype");
	vehs = getentarray("script_vehicle", "classname");
	best = undefined;
	for (i = 0; i < vehs.size; i++)
	{
		if (want != "" && vehs[i].vehicletype != want)
			continue;
		if (!isdefined(best) || vehs[i] getEntityNumber() < best getEntityNumber())
			best = vehs[i];
	}
	return best;
}

watch_places()
{
	wait 2;
	veh = probe_vehicle();
	if (!isdefined(veh))
	{
		logPrint("PROBE place unsupported " + getcvar("mapname") + "\n");
		return;
	}
	level.probe_home = veh.origin;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_place(veh);
		wait 0.05;
	}
}

//	Early returns, as probe_turret's try_place: sessionstate is undefined
//	before the first spawn.
try_place(veh)
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
	if (self getEntityNumber() >= 2)
	{
		//	A watcher: 350 units out from where the vehicle started, along
		//	the `probe_watch_yaw` cvar's world yaw, facing back.
		f = anglestoforward((0, getcvarint("probe_watch_yaw"), 0));
		spot = level.probe_home + (f[0] * 350, f[1] * 350, 8);
		yaw = vectortoangles(level.probe_home - spot)[1];
		self setorigin(spot);
		self setplayerangles((0, yaw, 0));
		logPrint("PROBE place " + gettime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
		return;
	}
	side = getcvarint("probe_side");
	if (side == 0)
		side = 110;
	if (self getEntityNumber() != 0)
		side = 0 - side;
	right = anglestoright(veh.angles);
	spot = veh.origin + (right[0] * side, right[1] * side, 8);
	yaw = vectortoangles(veh.origin - spot)[1];
	self setorigin(spot);
	self setplayerangles((getcvarint("probe_pitch") + 10, yaw, 0));
	logPrint("PROBE place " + gettime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
}
