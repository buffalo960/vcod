//	United Offensive: what rounds do to an empty vehicle
//	(docs/research/coduo-vehicles.md 13). Stock tdm with the map's vehicles.
//	Client 0, the shooter (`--probe-uo-weapon hold`), is put
//	`probe_shoot_dist` units (default 250) off the right flank of the first
//	vehicle (of the `probe_vehicletype` cvar's type when set), turned
//	`probe_dir` degrees about it toward the front, its clip kept
//	full and its view turned every frame onto the point `probe_aim_z`
//	(default 40) above the vehicle's origin, moved `probe_aim_along` units
//	(default 0) along the hull. Every half second it logs where a
//	`bulletTrace` from the eye to that point stops:
//
//	PROBE trace <time> <entity number or -1> <fraction> <surfacetype> <position>
//
//	and every `damage` notify the vehicle takes:
//
//	PROBE vdamage <time> <amount> <mod> <point> <dir> <attacker number> <inflictor number> <health>
//
//	A player's damage callback only logs (`PROBE hit`). UO only.

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

//	The vehicle the probe uses: the lowest-numbered `script_vehicle`, of
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

aim_point(veh)
{
	z = getcvarint("probe_aim_z");
	if (getcvar("probe_aim_z") == "")
		z = 40;
	fwd = anglestoforward(veh.angles);
	along = getcvarint("probe_aim_along");
	return veh.origin + (fwd[0] * along, fwd[1] * along, z);
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
	logPrint("PROBE vehicle " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + " " + veh.angles + " health " + veh.health + "\n");
	veh thread log_vehicle_damage();
	frame = 0;
	for (;;)
	{
		players = getentarray("player", "classname");
		shooter = undefined;
		for (i = 0; i < players.size; i++)
		{
			players[i] try_place(veh);
			if (isdefined(players[i].probe_placed) && players[i] getEntityNumber() == 0)
				shooter = players[i];
		}
		if (isdefined(shooter))
		{
			eye = shooter.origin + (0, 0, 60);
			at = aim_point(veh);
			if (frame % 10 == 0)
			{
				shooter setWeaponSlotClipAmmo("primary", 20);
				shooter giveMaxAmmo(shooter getcurrentweapon());
				d = vectornormalize(at - eye);
				trace = bullettrace(eye, at + (d[0] * 200, d[1] * 200, d[2] * 200), true, shooter);
				logPrint("PROBE trace " + gettime() + " " + entnum(trace["entity"]) + " " + trace["fraction"] + " " + trace["surfacetype"] + " " + trace["position"] + "\n");
			}
			shooter setplayerangles(vectortoangles(at - eye));
			frame++;
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
	side = getcvarint("probe_shoot_dist");
	if (side == 0)
		side = 250;
	//	`probe_dir` turns the spot that many degrees about the vehicle
	//	from its right flank toward its front.
	right = anglestoright(veh.angles + (0, getcvarint("probe_dir"), 0));
	spot = veh.origin + (right[0] * side, right[1] * side, 8);
	yaw = vectortoangles(veh.origin - spot)[1];
	self setorigin(spot);
	self setplayerangles((10, yaw, 0));
	logPrint("PROBE place " + gettime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
}
