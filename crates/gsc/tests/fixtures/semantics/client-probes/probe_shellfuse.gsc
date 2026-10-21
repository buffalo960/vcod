//	United Offensive: a tank shell that strikes nothing goes off on its 15 s
//	fuse (`G_ExplodeMissile`), and whom the splash names as its attacker
//	(docs/research/cod11-combat.md 13.7). Stock tdm with the map's vehicles.
//	Two seconds in, `fireTurret` on the first vehicle (of the
//	`probe_vehicletype` cvar's type when set) with no one in it. Client 0 is
//	then put `probe_side` units (default 110) off its right flank to board
//	it with `--probe-vehicle sit`. While it drives, its view is turned every
//	frame onto the point 40 above the nearest other vehicle (the target),
//	and the gun is fired with `fireTurret` 3 s and 5 s into the seat. Each
//	shell, found as the newest `rocket` the call spawned, is pinned with an
//	`origin` write every frame (`Scr_SetOrigin`, `G_SetOrigin`): the first
//	180 above the target's origin, the second 180 above the firing
//	vehicle's, so its splash reaches the hull under it. Logged:
//
//	PROBE vehicle <number> <vehicletype> <origin> <angles>
//	PROBE target <number> <vehicletype> <origin> <angles> health <health>
//	PROBE idlefire <time> rockets <count before> <count after>
//	PROBE seat <time> <vehpos>
//	PROBE fire <time> <k> <vehicle origin> <vehicle angles>
//	PROBE shell <time> <k> <number> <origin at the fire> <pin>
//	PROBE gone <time> <k>
//	PROBE vdamage <who> <time> <amount> <mod> <point> <dir> <attacker number> <inflictor number> <health>
//	PROBE hit <time> <client> dmg <iDamage> mod <mod> loc <hitloc> att <attacker number> inf <inflictor number>
//
//	the `vdamage` lines being the two vehicles' `damage` notifies (`who` is
//	`target` or `self`). A player's damage callback only logs. `gone`
//	never printed on retail: the freed shell ends its thread. UO only.

main()
{
	thread watch_places();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hit " + gettime() + " " + self getEntityNumber() + " dmg " + iDamage + " mod " + sMeansOfDeath + " loc " + sHitLoc + " att " + entnum(eAttacker) + " inf " + entnum(eInflictor) + "\n");
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
	vehs = getentarray("script_vehicle", "classname");
	best = undefined;
	for (i = 0; i < vehs.size; i++)
	{
		if (vehs[i] == veh)
			continue;
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

log_vehicle_damage(who)
{
	for (;;)
	{
		self waittill("damage", amount, attacker, dir, point, mod, inflictor);
		logPrint("PROBE vdamage " + who + " " + gettime() + " " + amount + " " + mod + " " + point + " " + dir + " " + entnum(attacker) + " " + entnum(inflictor) + " " + self.health + "\n");
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
		if (!isdefined(self.probe_seated_at))
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

//	The `rocket` entities, by number.
rockets()
{
	return getentarray("rocket", "classname");
}

//	The rocket in `after` that `before` lacks, else undefined.
new_rocket(before, after)
{
	for (i = 0; i < after.size; i++)
	{
		seen = false;
		for (j = 0; j < before.size; j++)
			if (after[i] == before[j])
				seen = true;
		if (!seen)
			return after[i];
	}
	return undefined;
}

//	Holds the shell at `pin` with an `origin` write each frame until it is
//	freed.
pin(k, at)
{
	while (isdefined(self))
	{
		self.origin = at;
		wait 0.05;
	}
	logPrint("PROBE gone " + gettime() + " " + k + "\n");
}

fire(veh, k, at)
{
	logPrint("PROBE fire " + gettime() + " " + k + " " + veh.origin + " " + veh.angles + "\n");
	before = rockets();
	veh fireTurret();
	shell = new_rocket(before, rockets());
	if (!isdefined(shell))
	{
		logPrint("PROBE shell " + gettime() + " " + k + " none\n");
		return;
	}
	logPrint("PROBE shell " + gettime() + " " + k + " " + shell getEntityNumber() + " " + shell.origin + " " + at + "\n");
	shell thread pin(k, at);
}

//	`fireTurret` with no one in the vehicle, in a thread of its own: a
//	script error there ends only this thread.
idle_fire(veh)
{
	before = rockets();
	veh fireTurret();
	logPrint("PROBE idlefire " + gettime() + " rockets " + before.size + " " + rockets().size + "\n");
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
	target thread log_vehicle_damage("target");
	veh thread log_vehicle_damage("self");
	thread idle_fire(veh);
	fired = 0;
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
			if (fired == 0 && since >= 3000)
			{
				fire(veh, 0, target.origin + (0, 0, 180));
				fired = 1;
			}
			else if (fired == 1 && since >= 5000)
			{
				fire(veh, 1, veh.origin + (0, 0, 180));
				fired = 2;
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
