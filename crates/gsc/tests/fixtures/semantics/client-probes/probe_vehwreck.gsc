//	United Offensive: a vehicle's death on the wire (docs/research/
//	coduo-vehicles.md 15). Stock tdm with the map's vehicles, so
//	`_tankdrive_gmi` and `_tankeffects_gmi` run their death threads. The
//	first player to spawn is put `probe_side` units (default 450, past the
//	death's 300-unit radiusDamage) off the first vehicle's right flank (of
//	the `probe_vehicletype` cvar's type when set), facing it; four seconds
//	later the vehicle takes the stock self-destruct's
//	`dodamage(10000, origin + (0, 0, 1), self, self, "MOD_EXPLOSIVE")`.
//	Logs, each as `PROBE <step> <time> ...`:
//
//	place     where the player stands
//	kill      the vehicle's number, origin and health before the hit
//	death     the vehicle's `death` notify
//	explode   its `allow_explode` notify (the second blast)
//
//	Record with a spawned `vcod --net-probe --probe-team axis
//	--save-snapshots --capture-tag uo-wreck`. UO only.

main()
{
	thread watch();
	maps\mp\gametypes\tdm::main();
}

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

watch()
{
	wait 2;
	veh = probe_vehicle();
	if (!isdefined(veh))
	{
		logPrint("PROBE place unsupported " + getcvar("mapname") + "\n");
		return;
	}
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			if (isdefined(players[i].sessionstate) && players[i].sessionstate == "playing")
			{
				players[i] place(veh);
				veh thread log_death();
				wait 4;
				logPrint("PROBE kill " + gettime() + " " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + " " + veh.health + "\n");
				veh dodamage(10000, veh.origin + (0, 0, 1), veh, veh, "MOD_EXPLOSIVE");
				return;
			}
		}
		wait 0.05;
	}
}

place(veh)
{
	side = getcvarint("probe_side");
	if (side == 0)
		side = 450;
	right = anglestoright(veh.angles);
	spot = veh.origin + (right[0] * side, right[1] * side, 8);
	yaw = vectortoangles(veh.origin - spot)[1];
	self setorigin(spot);
	self setplayerangles((10, yaw, 0));
	logPrint("PROBE place " + gettime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
}

log_death()
{
	self waittill("death");
	logPrint("PROBE death " + gettime() + " " + self getEntityNumber() + "\n");
	self waittill("allow_explode");
	logPrint("PROBE explode " + gettime() + " " + self getEntityNumber() + "\n");
}
