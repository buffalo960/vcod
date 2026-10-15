//	United Offensive: a tank's shell on another vehicle
//	(docs/research/coduo-vehicles.md 13). Stock tdm with the map's vehicles.
//	Client 0 is put `probe_side` units (default 110) off the right flank of
//	the first vehicle (of the `probe_vehicletype` cvar's type when set) to
//	board it with `--probe-vehicle sit`. While it drives, its view is turned
//	every frame onto the point 40 above the nearest other vehicle (or the
//	`probe_target` cvar's entity number), and the gun is fired with
//	`fireTurret` every `probe_fire_ms` (default 4000) from 3 s into the seat.
//	Logged:
//
//	PROBE target <number> <vehicletype> <origin> <angles> health <health>
//	PROBE fire <time> <vehicle origin> <vehicle angles>
//	PROBE vdamage <time> <amount> <mod> <point> <dir> <attacker number> <inflictor number> <health>
//	PROBE hit <time> <client> dmg <iDamage> mod <mod> loc <hitloc>
//
//	the `vdamage` lines being the target's `damage` notifies. A player's
//	damage callback only logs. UO only.

main()
{
	thread watch_places();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hit " + gettime() + " " + self getEntityNumber() + " dmg " + iDamage + " mod " + sMeansOfDeath + " loc " + sHitLoc + "\n");
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

target_vehicle(veh)
{
	want = getcvarint("probe_target");
	vehs = getentarray("script_vehicle", "classname");
	best = undefined;
	for (i = 0; i < vehs.size; i++)
	{
		if (vehs[i] == veh)
			continue;
		if (want != 0)
		{
			if (vehs[i] getEntityNumber() == want)
				best = vehs[i];
			continue;
		}
		if (!isdefined(best) || distance(vehs[i].origin, veh.origin) < distance(best.origin, veh.origin))
			best = vehs[i];
	}
	return best;
}

entnum(e)
{
	if (!isdefined(e))
		return -1;
	return e getEntityNumber();
}

log_vehicle_damage()
{
	for (;;)
	{
		self waittill("damage", amount, attacker, dir, point, mod, inflictor);
		logPrint("PROBE vdamage " + gettime() + " " + amount + " " + mod + " " + point + " " + dir + " " + entnum(attacker) + " " + entnum(inflictor) + " " + self.health + "\n");
	}
}

watch_seat()
{
	for (;;)
	{
		self waittill("activated", vehpos, activator);
		self.probe_driver = undefined;
		if (vehpos == 1)
			self.probe_driver = activator;
		self.probe_seated_at = gettime();
		logPrint("PROBE seat " + gettime() + " " + vehpos + "\n");
	}
}

watch_unseat()
{
	for (;;)
	{
		self waittill("deactivated", player);
		self.probe_driver = undefined;
		logPrint("PROBE unseat " + gettime() + "\n");
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
	target = target_vehicle(veh);
	logPrint("PROBE vehicle " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + " " + veh.angles + "\n");
	logPrint("PROBE target " + target getEntityNumber() + " " + target.vehicletype + " " + target.origin + " " + target.angles + " health " + target.health + "\n");
	veh thread watch_seat();
	veh thread watch_unseat();
	target thread log_vehicle_damage();
	every = getcvarint("probe_fire_ms");
	if (every == 0)
		every = 4000;
	last = 0;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_place(veh);
		if (isdefined(veh.probe_driver) && isdefined(target))
		{
			driver = veh.probe_driver;
			eye = driver.origin + (0, 0, 60);
			driver setplayerangles(vectortoangles(target.origin + (0, 0, 40) - eye));
			since = gettime() - veh.probe_seated_at;
			if (since >= 3000 && gettime() - last >= every)
			{
				last = gettime();
				logPrint("PROBE fire " + gettime() + " " + veh.origin + " " + veh.angles + "\n");
				veh fireTurret();
			}
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
