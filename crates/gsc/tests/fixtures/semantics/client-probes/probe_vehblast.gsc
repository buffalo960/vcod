//	United Offensive: a rocket's or a frag's explode on an empty vehicle
//	(docs/research/coduo-vehicles.md 13, "A blast on a hull"). Stock tdm
//	with the map's vehicles. Client 0 (`--probe-uo-weapon direct`) is
//	handed the `probe_weapon` cvar's weapon on its first spawn (into the
//	grenade slot when `probe_slot` is `grenade`, else the second primary,
//	with its ammo) and switched to it a second later. Then, for each shot
//	k from 0 to `probe_shots` - 1, it is put `probe_dist_<k>` units
//	(default `probe_dist`, 250) off the right flank of the first vehicle
//	(of the `probe_vehicletype` cvar's type when set), turned
//	`probe_dir_<k>` degrees about it toward the front, its view turned
//	every frame onto the aim point: the vehicle's origin moved
//	`probe_side_<k>` units to its right, `probe_along_<k>` along it and
//	`probe_z_<k>` (default 40) up. 1.5 s after each placement the script
//	presses the trigger for `probe_hold` ms (default 100) through
//	`iPrintLn("PROBE_IN <buttons> <wbuttons> <up>")`, refills the clip, and
//	places the next shot `probe_gap` ms (default 9000) after the press.
//	Logged:
//
//	PROBE vehicle <number> <vehicletype> <origin> <angles> health <health>
//	PROBE place <time> <k> <spot> <yaw>
//	PROBE fire <time> <k> <origin> <angles> <vehicle origin> <vehicle angles>
//	PROBE vdamage <time> <amount> <mod> <point> <dir> <attacker number> <inflictor number> <health>
//	PROBE hit <time> <client> dmg <iDamage> mod <mod> loc <hitloc>
//	PROBE done <time>
//
//	the `vdamage` lines being the vehicle's `damage` notifies. A player's
//	damage callback only logs, so the shooter never dies of his own blast.
//	UO only.

main()
{
	w = getcvar("probe_weapon");
	if (w != "")
		precacheItem(w);
	thread watch();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hit " + gettime() + " " + self getEntityNumber() + " dmg " + iDamage + " mod " + sMeansOfDeath + " loc " + sHitLoc + "\n");
}

//	The lowest-numbered `script_vehicle`, of the `probe_vehicletype`
//	cvar's type when that is set.
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

//	The shot's cvar `<name>_<k>`, else `<name>`, else `def`.
shot_num(name, k, def)
{
	if (getcvar(name + "_" + k) != "")
		return getcvarint(name + "_" + k);
	if (getcvar(name) != "")
		return getcvarint(name);
	return def;
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
	logPrint("PROBE vehicle " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + " " + veh.angles + " health " + veh.health + "\n");
	veh thread log_vehicle_damage();
	shooter = undefined;
	while (!isdefined(shooter))
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			if (p getEntityNumber() == 0 && isdefined(p.sessionstate) && p.sessionstate == "playing")
				shooter = p;
		}
		wait 0.05;
	}
	shooter arm();
	wait 1;
	shooter switchToWeapon(getcvar("probe_weapon"));
	wait 1;
	shots = shot_num("probe_shots", -1, 1);
	for (k = 0; k < shots && isdefined(veh); k++)
		shooter shot(veh, k);
	wait 6;
	logPrint("PROBE done " + gettime() + "\n");
}

arm()
{
	w = getcvar("probe_weapon");
	if (getcvar("probe_slot") == "grenade")
	{
		self setWeaponSlotWeapon("grenade", w);
		self setWeaponSlotClipAmmo("grenade", 3);
	}
	else
	{
		self setWeaponSlotWeapon("primaryb", w);
		self setWeaponSlotClipAmmo("primaryb", 1);
		self setWeaponSlotAmmo("primaryb", 5);
	}
}

refill()
{
	if (getcvar("probe_slot") == "grenade")
		self setWeaponSlotClipAmmo("grenade", 3);
	else
		self setWeaponSlotAmmo("primaryb", 5);
}

aim_point(veh, k)
{
	fwd = anglestoforward(veh.angles);
	right = anglestoright(veh.angles);
	side = shot_num("probe_side", k, 0);
	along = shot_num("probe_along", k, 0);
	z = shot_num("probe_z", k, 40);
	return veh.origin + (fwd[0] * along + right[0] * side, fwd[1] * along + right[1] * side, z);
}

shot(veh, k)
{
	dist = shot_num("probe_dist", k, 250);
	right = anglestoright(veh.angles + (0, shot_num("probe_dir", k, 0), 0));
	spot = veh.origin + (right[0] * dist, right[1] * dist, 8);
	yaw = vectortoangles(veh.origin - spot)[1];
	self setorigin(spot);
	self setplayerangles((10, yaw, 0));
	logPrint("PROBE place " + gettime() + " " + k + " " + spot + " " + yaw + "\n");
	hold = shot_num("probe_hold", k, 100);
	gap = shot_num("probe_gap", k, 9000);
	for (t = 0; t < 1500 + gap; t += 50)
	{
		// A killed vehicle's wreck is freed in time.
		if (!isdefined(veh))
			return;
		at = aim_point(veh, k);
		self setplayerangles(vectortoangles(at - (self.origin + (0, 0, 60))));
		if (t == 1500)
		{
			self iprintln("PROBE_IN 1 0 0");
			logPrint("PROBE fire " + gettime() + " " + k + " " + self.origin + " " + self.angles + " " + veh.origin + " " + veh.angles + "\n");
		}
		if (t == 1500 + hold)
		{
			self iprintln("PROBE_IN 0 0 0");
			self refill();
		}
		wait 0.05;
	}
}
