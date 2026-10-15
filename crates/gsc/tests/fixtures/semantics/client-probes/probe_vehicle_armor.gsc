//	United Offensive: what a gun on foot does to a vehicle's riders
//	(docs/research/coduo-vehicles.md 13). Stock tdm with the map's vehicles.
//	Client 0 is put `probe_side` units (default 110) off the right flank of
//	the first vehicle (of the `probe_vehicletype` cvar's type when set),
//	facing the hull, to board it with `--probe-vehicle sit`; client 1, the
//	shooter (`--probe-uo-weapon hold`), `probe_shoot_dist` units (default
//	250) out on the same flank, its clip kept full and its view turned onto
//	client 0 every frame, at heights cycling through `aim_heights()` above
//	client 0's origin. No damage lands: the damage callback only logs
//
//	PROBE hit <time> phase <n> seat <vehpos> veh <isInVehicle> dmg <iDamage> mod <mod> loc <hitloc> flags <iDFlags> aim <z>
//
//	so client 0 lives through every seat. `phase` counts client 0's
//	`activated` notifies (0 on foot before the use tap); a hit out of every
//	seat logs seat 0. The vehicle's `damage` notify is
//	logged as `PROBE vdamage`. Run the target first, the shooter once the
//	target's `PROBE place` is logged. UO only.

main()
{
	thread watch_places();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

aim_heights()
{
	a = [];
	a[0] = 0;
	a[1] = 16;
	a[2] = 32;
	a[3] = 48;
	a[4] = 64;
	return a;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	seat = 0;
	if (self isInVehicle())
		seat = self.probe_seat;
	phase = 0;
	if (isdefined(self.probe_phase))
		phase = self.probe_phase;
	aim = -1;
	if (isdefined(level.probe_aim))
		aim = level.probe_aim;
	logPrint("PROBE hit " + gettime() + " phase " + phase + " seat " + seat + " veh " + self isInVehicle() + " dmg " + iDamage + " mod " + sMeansOfDeath + " loc " + sHitLoc + " flags " + iDFlags + " aim " + aim + "\n");
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

log_seats()
{
	for (;;)
	{
		self waittill("activated", vehpos, activator);
		activator.probe_seat = vehpos;
		if (!isdefined(activator.probe_phase))
			activator.probe_phase = 0;
		activator.probe_phase++;
		logPrint("PROBE seat " + gettime() + " " + vehpos + " phase " + activator.probe_phase + "\n");
	}
}

log_unseats()
{
	for (;;)
	{
		self waittill("deactivated", player);
		logPrint("PROBE unseat " + gettime() + "\n");
	}
}

log_vehicle_damage()
{
	for (;;)
	{
		self waittill("damage", amount, attacker, dir, point, mod);
		logPrint("PROBE vdamage " + gettime() + " " + amount + " " + mod + "\n");
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
	logPrint("PROBE vehicle " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + " " + veh.angles + "\n");
	veh thread log_seats();
	veh thread log_unseats();
	veh thread log_vehicle_damage();
	frame = 0;
	aim = 0;
	for (;;)
	{
		players = getentarray("player", "classname");
		target = undefined;
		shooter = undefined;
		for (i = 0; i < players.size; i++)
		{
			players[i] try_place(veh);
			if (!isdefined(players[i].probe_placed))
				continue;
			if (players[i] getEntityNumber() == 0)
				target = players[i];
			else if (players[i] getEntityNumber() == 1)
				shooter = players[i];
		}
		if (isdefined(target) && isdefined(shooter))
		{
			//	Every half second a full clip and a new height: the middle of
			//	the heights whose line from the eye meets client 0 first, as
			//	a bullet's trace would, else the next of `aim_heights()`.
			eye = shooter.origin + (0, 0, 60);
			if (frame % 10 == 0)
			{
				shooter setWeaponSlotClipAmmo("primary", 20);
				shooter giveMaxAmmo(shooter getcurrentweapon());
				open = [];
				for (z = 0; z <= 120; z += 8)
				{
					trace = bullettrace(eye, target.origin + (0, 0, z), true, shooter);
					if (isdefined(trace["entity"]) && trace["entity"] == target)
						open[open.size] = z;
				}
				if (open.size > 0)
				{
					mid = 0;
					while ((mid + 1) * 2 <= open.size)
						mid++;
					level.probe_aim = open[mid];
				}
				else
				{
					heights = aim_heights();
					level.probe_aim = heights[aim % heights.size];
					aim++;
				}
				logPrint("PROBE aim " + gettime() + " " + level.probe_aim + " open " + open.size + " weapon " + shooter getcurrentweapon() + " ammo " + shooter getWeaponSlotAmmo("primary") + "\n");
			}
			shooter setplayerangles(vectortoangles(target.origin + (0, 0, level.probe_aim) - eye));
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
	side = getcvarint("probe_side");
	if (side == 0)
		side = 110;
	if (self getEntityNumber() != 0)
	{
		side = getcvarint("probe_shoot_dist");
		if (side == 0)
			side = 250;
	}
	right = anglestoright(veh.angles);
	spot = veh.origin + (right[0] * side, right[1] * side, 8);
	yaw = vectortoangles(veh.origin - spot)[1];
	self setorigin(spot);
	self setplayerangles((getcvarint("probe_pitch") + 10, yaw, 0));
	logPrint("PROBE place " + gettime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
}
