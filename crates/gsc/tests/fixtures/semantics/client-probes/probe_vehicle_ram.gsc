//	United Offensive: a vehicle driven into another
//	(docs/research/coduo-vehicles.md 12). Stock tdm with the map's vehicles.
//	Client 0 is put `probe_side` units (default 110) off the right flank of
//	the `probe_index`th vehicle (default 0, in entity order) of the
//	`probe_vehicletype` cvar's type, to board it with `--probe-vehicle ram`.
//	The struck vehicle is the other one nearest the point `probe_ahead`
//	units ahead of it. Every frame from the
//	placement on:
//
//	PROBE a <time> <origin> <angles> <speed>
//	PROBE b <time> <origin> <angles> <speed>
//
//	and the struck vehicle's `damage` notifies and each one's
//	`vehicle_collision` notifies (`collision` the driven one's, `bcollision`
//	the struck one's):
//
//	PROBE vdamage <time> <amount> <mod>
//	PROBE collision <time> <point> <normal>
//
//	UO only.

main()
{
	thread watch_places();
	maps\mp\gametypes\tdm::main();
}

probe_vehicle()
{
	want = getcvar("probe_vehicletype");
	index = getcvarint("probe_index");
	vehs = getentarray("script_vehicle", "classname");
	sorted = [];
	for (i = 0; i < vehs.size; i++)
	{
		if (want != "" && vehs[i].vehicletype != want)
			continue;
		sorted[sorted.size] = vehs[i];
	}
	for (i = 0; i < sorted.size; i++)
		for (j = i + 1; j < sorted.size; j++)
			if (sorted[j] getEntityNumber() < sorted[i] getEntityNumber())
			{
				t = sorted[i];
				sorted[i] = sorted[j];
				sorted[j] = t;
			}
	if (index >= sorted.size)
		return undefined;
	return sorted[index];
}

//	The other vehicle nearest the point `probe_ahead` units (default 0)
//	ahead of `veh`.
nearest_other(veh)
{
	fwd = anglestoforward(veh.angles);
	ahead = getcvarint("probe_ahead");
	at = veh.origin + (fwd[0] * ahead, fwd[1] * ahead, 0);
	vehs = getentarray("script_vehicle", "classname");
	best = undefined;
	for (i = 0; i < vehs.size; i++)
	{
		if (vehs[i] == veh)
			continue;
		if (!isdefined(best) || distance(vehs[i].origin, at) < distance(best.origin, at))
			best = vehs[i];
	}
	return best;
}

log_damage()
{
	for (;;)
	{
		self waittill("damage", amount, attacker, dir, point, mod);
		logPrint("PROBE vdamage " + gettime() + " " + amount + " " + mod + "\n");
	}
}

log_collisions(tag)
{
	for (;;)
	{
		self waittill("vehicle_collision", point, normal);
		logPrint("PROBE " + tag + " " + gettime() + " " + point + " " + normal + "\n");
	}
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
	other = nearest_other(veh);
	logPrint("PROBE vehicle " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + " " + veh.angles + "\n");
	logPrint("PROBE struck " + other getEntityNumber() + " " + other.vehicletype + " " + other.origin + " " + other.angles + "\n");
	other thread log_damage();
	veh thread log_collisions("collision");
	other thread log_collisions("bcollision");
	placed = false;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			players[i] try_place(veh);
			if (isdefined(players[i].probe_placed))
				placed = true;
		}
		if (placed)
		{
			logPrint("PROBE a " + gettime() + " " + veh.origin + " " + veh.angles + " " + veh.speed + "\n");
			logPrint("PROBE b " + gettime() + " " + other.origin + " " + other.angles + " " + other.speed + "\n");
		}
		wait 0.05;
	}
}

//	Early returns: sessionstate is undefined before the first spawn.
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
	side = getcvarint("probe_side");
	if (side == 0)
		side = 110;
	right = anglestoright(veh.angles);
	spot = veh.origin + (right[0] * side, right[1] * side, 8);
	yaw = vectortoangles(veh.origin - spot)[1];
	self setorigin(spot);
	self setplayerangles((10, yaw, 0));
	logPrint("PROBE place " + gettime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
}
