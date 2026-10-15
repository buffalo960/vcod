//	A script model in the way of a locational trace and of rounds
//	(docs/research/cod11-combat.md 2.7). Stock tdm.
//
//	The trace part, any patch, no client needed: 2 s in a `script_model`
//	with `xmodel/barrel_black1` (a 44-unit collision cylinder) is spawned
//	64 units ahead of the first team deathmatch spawn point, and a
//	`bulletTrace` from 24 above the spawn point through the barrel's middle
//	is logged with the barrel shown, hidden, shown and `notSolid()`, then
//	deleted:
//
//	PROBE model <entity number> <origin>
//	PROBE mtrace <time> <phase> <entity number or -1> <fraction> <surfacetype> <position>
//
//	The rounds part, United Offensive only (it needs a vehicle to take the
//	rounds that get past): client 0, the shooter (`--probe-uo-weapon aim`),
//	is put 250 units off the right flank of the first `willyjeep_mp` with
//	its view turned every frame onto the point 20 above the jeep's origin,
//	and a second barrel stands three quarters of the way there on the
//	ground under that line. The barrel is shown for 3 s, hidden for 3 s and
//	deleted; each phase's start is logged, and every `damage` notify the
//	jeep takes:
//
//	PROBE place <time> <entity number> <origin> <yaw>
//	PROBE rphase <time> <phase>
//	PROBE vdamage <time> <amount> <mod> <point>
//
//	A player's damage callback only logs (`PROBE hit`).

main()
{
	precacheModel("xmodel/barrel_black1");
	thread trace_part();
	thread watch_places();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hit " + gettime() + " " + self getEntityNumber() + " dmg " + iDamage + " mod " + sMeansOfDeath + " loc " + sHitLoc + "\n");
}

entnum(e)
{
	if (!isdefined(e))
		return -1;
	return e getEntityNumber();
}

mtrace(phase, eye, at)
{
	d = vectornormalize(at - eye);
	trace = bullettrace(eye, at + (d[0] * 200, d[1] * 200, d[2] * 200), true, undefined);
	logPrint("PROBE mtrace " + gettime() + " " + phase + " " + entnum(trace["entity"]) + " " + trace["fraction"] + " " + trace["surfacetype"] + " " + trace["position"] + "\n");
}

trace_part()
{
	wait 2;
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (!spawns.size)
		spawns = getentarray("mp_tdm_spawn", "classname");
	sp = spawns[0];
	fwd = anglestoforward((0, sp.angles[1], 0));
	ground = bullettrace(sp.origin + (0, 0, 16), sp.origin + (0, 0, -64), false, undefined);
	base = ground["position"];
	at = base + (fwd[0] * 64, fwd[1] * 64, 20);
	model = spawn("script_model", base + (fwd[0] * 64, fwd[1] * 64, 0));
	model setmodel("xmodel/barrel_black1");
	logPrint("PROBE model " + model getEntityNumber() + " " + model.origin + "\n");
	eye = base + (0, 0, 24);
	wait 0.1;
	mtrace("shown", eye, at);
	model hide();
	wait 0.1;
	mtrace("hidden", eye, at);
	model show();
	model notsolid();
	wait 0.1;
	mtrace("notsolid", eye, at);
	model delete();
	wait 0.1;
	mtrace("deleted", eye, at);
}

//	The first `willyjeep_mp` by entity number.
probe_vehicle()
{
	vehs = getentarray("script_vehicle", "classname");
	best = undefined;
	for (i = 0; i < vehs.size; i++)
	{
		if (vehs[i].vehicletype != "willyjeep_mp")
			continue;
		if (!isdefined(best) || vehs[i] getEntityNumber() < best getEntityNumber())
			best = vehs[i];
	}
	return best;
}

log_vehicle_damage()
{
	for (;;)
	{
		self waittill("damage", amount, attacker, dir, point, mod);
		logPrint("PROBE vdamage " + gettime() + " " + amount + " " + mod + " " + point + "\n");
	}
}

watch_places()
{
	wait 2;
	veh = probe_vehicle();
	if (!isdefined(veh))
		return;
	veh thread log_vehicle_damage();
	model = undefined;
	placed = undefined;
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
			at = veh.origin + (0, 0, 20);
			if (!isdefined(placed))
			{
				placed = gettime();
				d = at - eye;
				x = eye + (d[0] * 0.75, d[1] * 0.75, d[2] * 0.75);
				ground = bullettrace(x + (0, 0, 64), x + (0, 0, -128), false, undefined);
				model = spawn("script_model", ground["position"]);
				model setmodel("xmodel/barrel_black1");
				logPrint("PROBE rphase " + gettime() + " shown " + model.origin + "\n");
			}
			else if (gettime() == placed + 3000)
			{
				model hide();
				logPrint("PROBE rphase " + gettime() + " hidden\n");
			}
			else if (gettime() == placed + 6000)
			{
				model delete();
				logPrint("PROBE rphase " + gettime() + " deleted\n");
			}
			shooter setWeaponSlotClipAmmo("primary", 20);
			shooter giveMaxAmmo(shooter getcurrentweapon());
			shooter setplayerangles(vectortoangles(at - eye));
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
	right = anglestoright(veh.angles);
	spot = veh.origin + (right[0] * 250, right[1] * 250, 8);
	yaw = vectortoangles(veh.origin - spot)[1];
	self setorigin(spot);
	self setplayerangles((10, yaw, 0));
	logPrint("PROBE place " + gettime() + " " + self getEntityNumber() + " " + spot + " " + yaw + "\n");
}
