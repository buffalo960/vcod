//	United Offensive: what a wrecked vehicle's `spawnDuplicate` takes on
//	its first think (docs/research/coduo-vehicles.md 15.2). Stock tdm with
//	the map's vehicles; no client needed. Two seconds in the first vehicle
//	(of the `probe_vehicletype` cvar's type when set) takes the stock
//	self-destruct's `dodamage(10000, ...)`; `tank_respawn` spawns its
//	duplicate `delay` (`g_vehicleBurnTime`) later, where the wreck still
//	stands. Logs, each as `PROBE <step> <time> ...`:
//
//	kill      the vehicle's number and health before the hit
//	dupe      the duplicate's number, origin and health when first seen
//	damage    the duplicate's `damage` notify: amount, attacker, dir,
//	          point, means
//	death     the duplicate's `death` notify
//	health    the duplicate's health every frame for a second
//
//	COD_VERSION=uo PORT=<p> PROBE_SECS=30 \
//	    tools/run_probe.sh client-probes/probe_vehdupe mp_cassino

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
		logPrint("PROBE kill unsupported " + getcvar("mapname") + "\n");
		return;
	}
	old = getentarray("script_vehicle", "classname");
	logPrint("PROBE kill " + gettime() + " " + veh getEntityNumber() + " " + veh.health + "\n");
	veh dodamage(10000, veh.origin + (0, 0, 1), veh, veh, "MOD_EXPLOSIVE");
	for (;;)
	{
		vehs = getentarray("script_vehicle", "classname");
		for (i = 0; i < vehs.size; i++)
		{
			seen = false;
			for (j = 0; j < old.size; j++)
				if (isdefined(old[j]) && old[j] == vehs[i])
					seen = true;
			if (seen)
				continue;
			dupe = vehs[i];
			logPrint("PROBE dupe " + gettime() + " " + dupe getEntityNumber() + " " + dupe.origin + " " + dupe.health + "\n");
			dupe thread log_damage();
			dupe thread log_death();
			for (k = 0; k < 20; k++)
			{
				wait 0.05;
				logPrint("PROBE health " + gettime() + " " + dupe.health + "\n");
			}
			return;
		}
		wait 0.05;
	}
}

log_damage()
{
	for (;;)
	{
		self waittill("damage", amount, attacker, dir, point, mod);
		logPrint("PROBE damage " + gettime() + " " + amount + " " + attacker getEntityNumber() + " " + dir + " " + point + " " + mod + "\n");
	}
}

log_death()
{
	self waittill("death", attacker);
	logPrint("PROBE death " + gettime() + " " + attacker getEntityNumber() + "\n");
}
